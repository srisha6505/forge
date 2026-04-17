//! Conversation/voice-chat orchestrator.
//!
//! Continuous mic capture + WebRTC VAD end-of-speech detection + whisper STT
//! + LLM + piper TTS. Hands-free loop with events emitted to the UI so the
//! React side can render state transitions and play back audio.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::sync::mpsc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use webrtc_vad::{Vad, VadMode, SampleRate};

pub enum VoiceEvent {
    State(String),
    Transcript(String),
    AssistantText(String),
    TtsChunk(Vec<u8>), // base64-encoded wav on wire
    BargeIn,            // wake word heard during TTS — UI should kill playback
    Error(String),
    Stopped,
}

pub struct VoiceSession {
    stop_flag: Arc<AtomicBool>,
    interrupt_flag: Arc<AtomicBool>,
    muted_flag: Arc<AtomicBool>,
    wake_pending: Arc<AtomicBool>,
    pub running: bool,
}

pub struct VoiceHandle(pub Mutex<Option<VoiceSession>>);
impl Default for VoiceHandle {
    fn default() -> Self { Self(Mutex::new(None)) }
}

#[derive(Clone)]
pub struct VoiceConfig {
    pub whisper_bin: PathBuf,
    pub whisper_model: PathBuf,
    pub piper_bin: PathBuf,
    pub piper_voice: PathBuf,
    pub language: String,
    /// When Some, utterances must contain this phrase to trigger processing.
    /// The wake phrase is stripped from the prompt.
    pub wake_word: Option<String>,
}

impl VoiceSession {
    /// Start the conversation loop on a dedicated thread. Sends events via `tx`.
    /// The loop ends when `stop_flag` is set. `on_prompt` is called with each
    /// final transcript; it should return the assistant's final text reply
    /// (after running the agent loop). That text is then fed to piper for TTS.
    pub fn start<F>(cfg: VoiceConfig, tx: mpsc::Sender<VoiceEvent>, on_prompt: F) -> Self
    where
        F: Fn(String, &mpsc::Sender<VoiceEvent>) -> Result<String, String> + Send + 'static,
    {
        let stop = Arc::new(AtomicBool::new(false));
        let interrupt = Arc::new(AtomicBool::new(false));
        let muted = Arc::new(AtomicBool::new(false));
        let wake_pending = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let interrupt_thread = Arc::clone(&interrupt);
        let muted_thread = Arc::clone(&muted);
        let wake_thread = Arc::clone(&wake_pending);

        std::thread::Builder::new()
            .name("forge-voice".into())
            .spawn(move || {
                if let Err(e) = run_loop(cfg, &tx, stop_thread, interrupt_thread, muted_thread, wake_thread, on_prompt) {
                    let _ = tx.send(VoiceEvent::Error(e));
                }
                let _ = tx.send(VoiceEvent::Stopped);
            })
            .expect("spawn voice thread");

        Self { stop_flag: stop, interrupt_flag: interrupt, muted_flag: muted, wake_pending, running: true }
    }

    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        self.interrupt_flag.store(true, Ordering::Relaxed);
    }

    pub fn interrupt(&self) {
        self.interrupt_flag.store(true, Ordering::Relaxed);
    }

    pub fn set_muted(&self, m: bool) {
        self.muted_flag.store(m, Ordering::Relaxed);
    }
}

fn run_loop<F>(
    cfg: VoiceConfig,
    tx: &mpsc::Sender<VoiceEvent>,
    stop: Arc<AtomicBool>,
    interrupt: Arc<AtomicBool>,
    muted: Arc<AtomicBool>,
    wake_pending: Arc<AtomicBool>,
    on_prompt: F,
) -> Result<(), String>
where
    F: Fn(String, &mpsc::Sender<VoiceEvent>) -> Result<String, String>,
{
    let wake_trigger = cfg.wake_word.clone().unwrap_or_else(|| "riva".into()).to_lowercase();

    loop {
        if stop.load(Ordering::Relaxed) { return Ok(()); }
        interrupt.store(false, Ordering::Relaxed);

        // If muted: don't listen. Idle until unmuted. Do not stop TTS or other work.
        if muted.load(Ordering::Relaxed) {
            let _ = tx.send(VoiceEvent::State("muted".into()));
            while muted.load(Ordering::Relaxed) && !stop.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            if stop.load(Ordering::Relaxed) { return Ok(()); }
        }

        // Wake pending from barge-in — skip to capture follow-up.
        let mut text = if wake_pending.swap(false, Ordering::Relaxed) {
            eprintln!("[voice] wake pending, capturing follow-up");
            let _ = tx.send(VoiceEvent::State("wake_active".into()));
            let follow = capture_with_timeout(Arc::clone(&stop), Arc::clone(&muted), 5_000)?;
            if follow.is_empty() || audio_rms(&follow) < 0.02 {
                eprintln!("[voice] wake: no follow-up speech");
                continue;
            }
            let _ = tx.send(VoiceEvent::State("transcribing".into()));
            let (t, _) = run_whisper(&cfg, &follow)?;
            t
        } else {
            let _ = tx.send(VoiceEvent::State("listening".into()));
            let pcm = capture_until_silence(Arc::clone(&stop), Arc::clone(&muted))?;
            if stop.load(Ordering::Relaxed) { return Ok(()); }
            if pcm.is_empty() { continue; }
            let rms = audio_rms(&pcm);
            if rms < 0.02 {
                eprintln!("[voice] skip: rms={:.4} too low", rms);
                continue;
            }
            let _ = tx.send(VoiceEvent::State("transcribing".into()));
            let (t, lang) = run_whisper(&cfg, &pcm)?;
            eprintln!("[voice] whisper: lang={} text={:?}", lang, t);

            // Wake-word gate (idle mode): require wake word; strip it.
            if cfg.wake_word.is_some() {
                let lower_txt = t.to_lowercase();
                if let Some(pos) = lower_txt.find(&wake_trigger) {
                    let after = &t[pos + wake_trigger.len()..];
                    let stripped = after.trim_start_matches(|c: char| {
                        c.is_whitespace() || c == ',' || c == '.' || c == '!' || c == '?' || c == ':'
                    });
                    if stripped.trim().is_empty() {
                        // Just "Riva" — wait for follow-up.
                        eprintln!("[voice] wake-only utterance, listening for follow-up");
                        let _ = tx.send(VoiceEvent::State("wake_active".into()));
                        let follow = capture_with_timeout(Arc::clone(&stop), Arc::clone(&muted), 5_000)?;
                        if follow.is_empty() || audio_rms(&follow) < 0.02 {
                            eprintln!("[voice] no follow-up after wake");
                            continue;
                        }
                        let _ = tx.send(VoiceEvent::State("transcribing".into()));
                        let (t2, _) = run_whisper(&cfg, &follow)?;
                        t2
                    } else {
                        eprintln!("[voice] wake+prompt: {stripped:?}");
                        stripped.to_string()
                    }
                } else {
                    eprintln!("[voice] no wake word, skip");
                    continue;
                }
            } else {
                t
            }
        };

        // Final guards.
        if text.trim().len() < 3 { continue; }
        // Remove common hallucinations.
        {
            let low = text.trim().to_lowercase();
            let hallucinations = ["thanks for watching", "thank you for watching",
                "thank you.", "you", "bye", ".", "♪", "(music)", "[music]"];
            if hallucinations.iter().any(|h| low == *h || low == format!("{h}.")) {
                eprintln!("[voice] skip: hallucination");
                continue;
            }
        }
        text = text.trim().to_string();
        let _ = tx.send(VoiceEvent::Transcript(text.clone()));

        let _ = tx.send(VoiceEvent::State("thinking".into()));
        let reply = on_prompt(text, tx)?;
        if reply.trim().is_empty() { continue; }
        let _ = tx.send(VoiceEvent::AssistantText(reply.clone()));

        // Clean text: strip markdown + control chars so TTS doesn't say "asterisk".
        let clean = sanitize_for_tts(&reply);
        if clean.trim().is_empty() { continue; }

        let _ = tx.send(VoiceEvent::State("speaking".into()));

        // Spawn barge-in listener: during TTS, keep mic open and check for
        // wake phrase every short capture. If heard, set interrupt flag so
        // the sentence loop aborts, plus mark wake_pending so the next
        // iteration captures a follow-up instead of waiting for another wake.
        let barge_stop = Arc::new(AtomicBool::new(false));
        let barge_handle = spawn_barge_in(
            cfg.clone(),
            Arc::clone(&interrupt),
            Arc::clone(&barge_stop),
            Arc::clone(&muted),
            Arc::clone(&wake_pending),
            tx.clone(),
        );

        for chunk in split_sentences(&clean) {
            if interrupt.load(Ordering::Relaxed) || stop.load(Ordering::Relaxed) {
                break;
            }
            match run_piper(&cfg, &chunk) {
                Ok(wav) => {
                    use base64::Engine;
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&wav);
                    let _ = tx.send(VoiceEvent::TtsChunk(b64.into_bytes()));
                }
                Err(e) => {
                    let _ = tx.send(VoiceEvent::Error(format!("tts: {e}")));
                    break;
                }
            }
        }
        // Stop barge-in listener.
        barge_stop.store(true, Ordering::Relaxed);
        if let Some(h) = barge_handle { let _ = h.join(); }
    }
}

/// Background thread that listens during TTS for the wake word. If the
/// configured wake phrase (or "riva" default) appears in a captured
/// utterance, it sets `interrupt_flag` so the outer speaking loop aborts.
fn spawn_barge_in(
    cfg: VoiceConfig,
    interrupt_flag: Arc<AtomicBool>,
    stop_flag: Arc<AtomicBool>,
    muted: Arc<AtomicBool>,
    wake_pending: Arc<AtomicBool>,
    tx: mpsc::Sender<VoiceEvent>,
) -> Option<std::thread::JoinHandle<()>> {
    let trigger = cfg.wake_word.clone().unwrap_or_else(|| "riva".into()).to_lowercase();
    let cfg_clone = VoiceConfig {
        whisper_bin: cfg.whisper_bin.clone(),
        whisper_model: cfg.whisper_model.clone(),
        piper_bin: cfg.piper_bin.clone(),
        piper_voice: cfg.piper_voice.clone(),
        language: cfg.language.clone(),
        wake_word: None, // this thread does its own matching
    };

    std::thread::Builder::new()
        .name("forge-barge".into())
        .spawn(move || {
            eprintln!("[voice] barge-in thread started");
            while !stop_flag.load(Ordering::Relaxed) {
                if muted.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    continue;
                }
                let local_mute = Arc::new(AtomicBool::new(false));
                let pcm = match capture_short(Arc::clone(&stop_flag), local_mute) {
                    Ok(p) => p,
                    Err(e) => { eprintln!("[voice] barge capture err: {e}"); std::thread::sleep(std::time::Duration::from_millis(200)); continue; }
                };
                if pcm.is_empty() { continue; }
                if audio_rms(&pcm) < 0.02 { continue; }
                eprintln!("[voice] barge heard {} samples, rms={:.3}", pcm.len(), audio_rms(&pcm));
                let (text, _) = match run_whisper(&cfg_clone, &pcm) {
                    Ok(r) => r,
                    Err(e) => { eprintln!("[voice] barge whisper err: {e}"); continue; }
                };
                eprintln!("[voice] barge text: {:?}", text);
                if text.to_lowercase().contains(&trigger) {
                    eprintln!("[voice] barge-in wake word heard: {text:?}");
                    wake_pending.store(true, Ordering::Relaxed);
                    interrupt_flag.store(true, Ordering::Relaxed);
                    let _ = tx.send(VoiceEvent::BargeIn);
                    break;
                }
            }
        })
        .ok()
}

/// Capture with a no-speech timeout. Waits up to `timeout_ms` for speech
/// to start; once started, captures until end-of-speech.
fn capture_with_timeout(
    stop: Arc<AtomicBool>,
    muted: Arc<AtomicBool>,
    timeout_ms: usize,
) -> Result<Vec<f32>, String> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or("no input device")?;
    let cfg = device.default_input_config().map_err(|e| format!("cfg: {e}"))?;
    let input_rate = cfg.sample_rate().0;
    let channels = cfg.channels();
    let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let buf_cb = Arc::clone(&buffer);
    let running_flag = Arc::new(AtomicBool::new(true));
    let running_cb = Arc::clone(&running_flag);
    let err_fn = |_e| {};
    let rate = input_rate;
    let ch = channels;

    let stream = match cfg.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &cfg.config(),
            move |data: &[f32], _: &_| {
                if !running_cb.load(Ordering::Relaxed) { return; }
                let r = crate::stt::to_mono_16khz(data, rate, ch);
                if let Ok(mut b) = buf_cb.lock() { b.extend_from_slice(&r); }
            }, err_fn, None),
        cpal::SampleFormat::I16 => {
            let buf_cb = Arc::clone(&buffer);
            let running_cb2 = Arc::clone(&running_flag);
            device.build_input_stream(
                &cfg.config(),
                move |data: &[i16], _: &_| {
                    if !running_cb2.load(Ordering::Relaxed) { return; }
                    let f: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                    let r = crate::stt::to_mono_16khz(&f, rate, ch);
                    if let Ok(mut b) = buf_cb.lock() { b.extend_from_slice(&r); }
                }, err_fn, None)
        },
        _ => return Err("unsupported format".into()),
    }.map_err(|e| format!("build stream: {e}"))?;
    stream.play().map_err(|e| format!("play: {e}"))?;

    let mut vad = Vad::new_with_rate_and_mode(SampleRate::Rate16kHz, VadMode::VeryAggressive);
    const FRAME_SAMPLES: usize = 480;
    const FRAME_MS: usize = 30;
    const TRAILING_SILENCE_MS: usize = 700;
    const FRAME_RMS_FLOOR: f32 = 0.015;

    let start = std::time::Instant::now();
    let mut consumed = 0usize;
    let mut speech_started = false;
    let mut silence_ms = 0usize;

    loop {
        if stop.load(Ordering::Relaxed) || muted.load(Ordering::Relaxed) { break; }
        let elapsed = start.elapsed().as_millis() as usize;
        if !speech_started && elapsed >= timeout_ms { break; }
        if speech_started && elapsed >= 30_000 { break; }

        std::thread::sleep(std::time::Duration::from_millis(30));
        let frames: Vec<(f32, Vec<i16>)> = {
            let b = buffer.lock().map_err(|e| format!("lock: {e}"))?;
            let avail = b.len().saturating_sub(consumed);
            let n = avail / FRAME_SAMPLES;
            let mut v = Vec::with_capacity(n);
            for i in 0..n {
                let st = consumed + i * FRAME_SAMPLES;
                let f = &b[st..st + FRAME_SAMPLES];
                let rms = { let s: f32 = f.iter().map(|x| x*x).sum(); (s / f.len() as f32).sqrt() };
                let i16f: Vec<i16> = f.iter().map(|&s| (s.clamp(-1.0,1.0)*32767.0) as i16).collect();
                v.push((rms, i16f));
            }
            consumed += n * FRAME_SAMPLES;
            v
        };
        for (rms, f) in frames {
            let is = vad.is_voice_segment(&f).unwrap_or(false) && rms >= FRAME_RMS_FLOOR;
            if is { speech_started = true; silence_ms = 0; }
            else if speech_started { silence_ms += FRAME_MS; }
        }
        if speech_started && silence_ms >= TRAILING_SILENCE_MS { break; }
    }

    running_flag.store(false, Ordering::Relaxed);
    drop(stream);
    if !speech_started { return Ok(Vec::new()); }
    let buf = buffer.lock().map_err(|e| format!("final: {e}"))?.clone();
    Ok(buf)
}

/// Short capture for barge-in: stops after first end-of-speech or 3s max,
/// whichever comes first.
fn capture_short(stop: Arc<AtomicBool>, muted: Arc<AtomicBool>) -> Result<Vec<f32>, String> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or("no input device")?;
    let cfg = device.default_input_config().map_err(|e| format!("cfg: {e}"))?;
    let input_rate = cfg.sample_rate().0;
    let channels = cfg.channels();

    let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let buf_cb = Arc::clone(&buffer);
    let running_flag = Arc::new(AtomicBool::new(true));
    let running_cb = Arc::clone(&running_flag);
    let err_fn = |_e| {};
    let rate = input_rate;
    let ch = channels;

    let stream = match cfg.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &cfg.config(),
            move |data: &[f32], _: &_| {
                if !running_cb.load(Ordering::Relaxed) { return; }
                let r = crate::stt::to_mono_16khz(data, rate, ch);
                if let Ok(mut b) = buf_cb.lock() { b.extend_from_slice(&r); }
            }, err_fn, None),
        cpal::SampleFormat::I16 => {
            let buf_cb = Arc::clone(&buffer);
            let running_cb2 = Arc::clone(&running_flag);
            device.build_input_stream(
                &cfg.config(),
                move |data: &[i16], _: &_| {
                    if !running_cb2.load(Ordering::Relaxed) { return; }
                    let f: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                    let r = crate::stt::to_mono_16khz(&f, rate, ch);
                    if let Ok(mut b) = buf_cb.lock() { b.extend_from_slice(&r); }
                }, err_fn, None)
        },
        _ => return Err("unsupported format".into()),
    }.map_err(|e| format!("build stream: {e}"))?;
    stream.play().map_err(|e| format!("play: {e}"))?;

    let mut vad = Vad::new_with_rate_and_mode(SampleRate::Rate16kHz, VadMode::VeryAggressive);
    const FRAME_SAMPLES: usize = 480;
    const FRAME_MS: usize = 30;
    const MAX_MS: usize = 1_500;           // barge-in: short, fast detection
    const TRAILING_SILENCE_MS: usize = 300; // quick end-of-speech
    const FRAME_RMS_FLOOR: f32 = 0.02;      // higher floor to ignore tts bleed

    let start = std::time::Instant::now();
    let mut consumed = 0usize;
    let mut speech_started = false;
    let mut silence_ms = 0usize;

    loop {
        if stop.load(Ordering::Relaxed) || muted.load(Ordering::Relaxed) { break; }
        if start.elapsed().as_millis() as usize >= MAX_MS { break; }
        std::thread::sleep(std::time::Duration::from_millis(30));

        let frames: Vec<(f32, Vec<i16>)> = {
            let b = buffer.lock().map_err(|e| format!("lock: {e}"))?;
            let avail = b.len().saturating_sub(consumed);
            let n = avail / FRAME_SAMPLES;
            let mut v = Vec::with_capacity(n);
            for i in 0..n {
                let st = consumed + i * FRAME_SAMPLES;
                let f = &b[st..st + FRAME_SAMPLES];
                let rms = { let s: f32 = f.iter().map(|x| x*x).sum(); (s / f.len() as f32).sqrt() };
                let i16f: Vec<i16> = f.iter().map(|&s| (s.clamp(-1.0,1.0)*32767.0) as i16).collect();
                v.push((rms, i16f));
            }
            consumed += n * FRAME_SAMPLES;
            v
        };
        for (rms, f) in frames {
            let is = vad.is_voice_segment(&f).unwrap_or(false) && rms >= FRAME_RMS_FLOOR;
            if is { speech_started = true; silence_ms = 0; }
            else if speech_started { silence_ms += FRAME_MS; }
        }
        if speech_started && silence_ms >= TRAILING_SILENCE_MS { break; }
    }

    running_flag.store(false, Ordering::Relaxed);
    drop(stream);
    if !speech_started { return Ok(Vec::new()); }
    let buf = buffer.lock().map_err(|e| format!("final: {e}"))?.clone();
    Ok(buf)
}

/// Remove markdown / control characters that would be read aloud awkwardly.
fn sanitize_for_tts(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_code = false;
    for line in s.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") { in_code = !in_code; continue; }
        if in_code { continue; }
        // Strip heading markers, bullets, emphasis, link syntax.
        let mut t = trimmed.trim_start_matches(|c: char| c == '#' || c == '>' || c == '-' || c == '*' || c.is_whitespace()).to_string();
        // Remove inline markdown markers.
        for pat in ["**", "__", "`", "~~"] {
            t = t.replace(pat, "");
        }
        // Inline code already stripped. Replace single asterisk + underscore.
        t = t.chars().filter(|c| *c != '*' && *c != '_').collect();
        // Replace markdown links [text](url) → text.
        while let Some(lb) = t.find('[') {
            if let Some(rb) = t[lb..].find(']') {
                let rb_abs = lb + rb;
                if let Some(lp) = t[rb_abs..].find('(') {
                    if t[rb_abs..].as_bytes()[lp] == b'(' && lp == 1 {
                        if let Some(rp) = t[rb_abs + lp..].find(')') {
                            let rp_abs = rb_abs + lp + rp;
                            let text = t[lb + 1..rb_abs].to_string();
                            t.replace_range(lb..=rp_abs, &text);
                            continue;
                        }
                    }
                }
            }
            break;
        }
        if !t.is_empty() {
            out.push_str(&t);
            out.push(' ');
        }
    }
    // Collapse whitespace.
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Split text into sentence-ish chunks for progressive TTS.
fn split_sentences(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    for ch in s.chars() {
        buf.push(ch);
        if matches!(ch, '.' | '!' | '?') && buf.len() > 20 {
            out.push(buf.trim().to_string());
            buf.clear();
        }
    }
    if !buf.trim().is_empty() { out.push(buf.trim().to_string()); }
    if out.is_empty() { out.push(s.to_string()); }
    out
}

/// Capture 16 kHz mono audio from the default input until end-of-speech is
/// detected (VAD). Returns the speech-only PCM (trimmed leading/trailing
/// silence).
fn capture_until_silence(stop: Arc<AtomicBool>, muted: Arc<AtomicBool>) -> Result<Vec<f32>, String> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or("no input device")?;
    let cfg = device.default_input_config().map_err(|e| format!("cfg: {e}"))?;
    let input_rate = cfg.sample_rate().0;
    let channels = cfg.channels();
    eprintln!("[voice] cap: rate={} ch={} fmt={:?}", input_rate, channels, cfg.sample_format());

    // Ring buffer of resampled-to-16k mono f32 samples.
    // The cpal callback pushes resampled samples directly.
    let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let buf_cb = Arc::clone(&buffer);
    let running_flag = Arc::new(AtomicBool::new(true));
    let running_cb = Arc::clone(&running_flag);
    let err_fn = |e| eprintln!("[voice] stream err: {e}");

    let rate = input_rate;
    let ch = channels;
    let build_f32 = move |data: &[f32], _: &_| {
        if !running_cb.load(Ordering::Relaxed) { return; }
        let resampled = crate::stt::to_mono_16khz(data, rate, ch);
        if let Ok(mut b) = buf_cb.lock() { b.extend_from_slice(&resampled); }
    };
    let stream = match cfg.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &cfg.config(), build_f32, err_fn, None),
        cpal::SampleFormat::I16 => {
            let buf_cb = Arc::clone(&buffer);
            let running_cb2 = Arc::clone(&running_flag);
            device.build_input_stream(
                &cfg.config(),
                move |data: &[i16], _: &_| {
                    if !running_cb2.load(Ordering::Relaxed) { return; }
                    let f: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                    let resampled = crate::stt::to_mono_16khz(&f, rate, ch);
                    if let Ok(mut b) = buf_cb.lock() { b.extend_from_slice(&resampled); }
                }, err_fn, None)
        },
        fmt => return Err(format!("unsupported fmt: {:?}", fmt)),
    }.map_err(|e| format!("build stream: {e}"))?;

    stream.play().map_err(|e| format!("play: {e}"))?;

    // VAD: 30ms frames at 16kHz = 480 samples.
    let mut vad = Vad::new_with_rate_and_mode(SampleRate::Rate16kHz, VadMode::VeryAggressive);
    const FRAME_MS: usize = 30;
    const FRAME_SAMPLES: usize = 480;
    const TRAILING_SILENCE_MS: usize = 600;
    const MAX_RECORD_MS: usize = 30_000;
    const MIN_SPEECH_MS: usize = 400;           // require real speech length
    const PRE_SPEECH_TIMEOUT_MS: usize = 10_000;
    // Noise gate: per-frame RMS floor. Frames below this level cannot count
    // as speech even if VAD flags them. 0.015 ≈ -36 dBFS, above typical
    // room/electrical noise, below normal speech.
    const FRAME_RMS_FLOOR: f32 = 0.015;
    // Require consecutive voiced frames to START tracking, so one noise
    // burst doesn't flip to "speech_started".
    const START_CONFIRM_FRAMES: usize = 4; // 120 ms

    let start = std::time::Instant::now();
    let mut consumed: usize = 0;
    let mut speech_ms = 0usize;
    let mut silence_ms = 0usize;
    let mut speech_started = false;
    let mut pending_speech_frames: usize = 0;
    let mut last_dbg = std::time::Instant::now();

    loop {
        if stop.load(Ordering::Relaxed) { break; }
        if muted.load(Ordering::Relaxed) {
            eprintln!("[voice] muted mid-capture, aborting");
            running_flag.store(false, Ordering::Relaxed);
            drop(stream);
            return Ok(Vec::new());
        }
        let elapsed_ms = start.elapsed().as_millis() as usize;
        if elapsed_ms > MAX_RECORD_MS { eprintln!("[voice] max record hit"); break; }
        if !speech_started && elapsed_ms > PRE_SPEECH_TIMEOUT_MS {
            eprintln!("[voice] no speech detected in {PRE_SPEECH_TIMEOUT_MS}ms, giving up");
            break;
        }

        std::thread::sleep(std::time::Duration::from_millis(30));

        // Drain: pull completed 30ms frames as (f32, i16) pairs so we can
        // check RMS floor too.
        let frames: Vec<(f32, Vec<i16>)> = {
            let b = buffer.lock().map_err(|e| format!("lock: {e}"))?;
            let avail = b.len().saturating_sub(consumed);
            let n_frames = avail / FRAME_SAMPLES;
            let mut v = Vec::with_capacity(n_frames);
            for i in 0..n_frames {
                let start = consumed + i * FRAME_SAMPLES;
                let frame_f = &b[start..start + FRAME_SAMPLES];
                let rms = {
                    let s: f32 = frame_f.iter().map(|&x| x * x).sum();
                    (s / frame_f.len() as f32).sqrt()
                };
                let frame_i: Vec<i16> = frame_f.iter()
                    .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
                    .collect();
                v.push((rms, frame_i));
            }
            consumed += n_frames * FRAME_SAMPLES;
            v
        };

        for (rms, frame) in frames {
            // Frame must pass both VAD flag AND RMS floor to count as speech.
            let vad_voice = vad.is_voice_segment(&frame).unwrap_or(false);
            let is_speech = vad_voice && rms >= FRAME_RMS_FLOOR;

            if is_speech {
                if !speech_started {
                    pending_speech_frames += 1;
                    if pending_speech_frames >= START_CONFIRM_FRAMES {
                        speech_started = true;
                        speech_ms += pending_speech_frames * FRAME_MS;
                        pending_speech_frames = 0;
                        eprintln!("[voice] speech START rms={:.3}", rms);
                    }
                } else {
                    speech_ms += FRAME_MS;
                    silence_ms = 0;
                }
            } else {
                pending_speech_frames = 0;
                if speech_started {
                    silence_ms += FRAME_MS;
                }
            }
        }

        if last_dbg.elapsed().as_millis() > 500 {
            eprintln!("[voice] speech_started={} speech_ms={} silence_ms={} consumed={}",
                speech_started, speech_ms, silence_ms, consumed);
            last_dbg = std::time::Instant::now();
        }

        if speech_started && silence_ms >= TRAILING_SILENCE_MS && speech_ms >= MIN_SPEECH_MS {
            eprintln!("[voice] end-of-speech detected");
            break;
        }
    }

    running_flag.store(false, Ordering::Relaxed);
    drop(stream);

    if !speech_started { return Ok(Vec::new()); }
    let final_buf = {
        let g = buffer.lock().map_err(|e| format!("final lock: {e}"))?;
        g.clone()
    };
    Ok(final_buf)
}

fn audio_rms(pcm: &[f32]) -> f32 {
    if pcm.is_empty() { return 0.0; }
    let sum_sq: f32 = pcm.iter().map(|&s| s * s).sum();
    (sum_sq / pcm.len() as f32).sqrt()
}

fn run_whisper(cfg: &VoiceConfig, pcm: &[f32]) -> Result<(String, String), String> {
    let tmp = std::env::temp_dir().join(format!("forge-voice-{}.wav", std::process::id()));
    let spec = hound::WavSpec {
        channels: 1, sample_rate: 16000,
        bits_per_sample: 16, sample_format: hound::SampleFormat::Int,
    };
    {
        let mut w = hound::WavWriter::create(&tmp, spec).map_err(|e| format!("wav: {e}"))?;
        for &s in pcm {
            let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
            w.write_sample(v).map_err(|e| format!("wav write: {e}"))?;
        }
        w.finalize().map_err(|e| format!("wav fin: {e}"))?;
    }
    let mut cmd = Command::new(&cfg.whisper_bin);
    cmd.arg("-m").arg(&cfg.whisper_model)
        .arg("-f").arg(&tmp)
        .arg("--no-timestamps");
    if cfg.language != "auto" && !cfg.language.is_empty() {
        cmd.arg("-l").arg(&cfg.language);
    } else {
        cmd.arg("-l").arg("auto");
    }
    let out = cmd.output().map_err(|e| format!("whisper spawn: {e}"))?;
    let _ = std::fs::remove_file(&tmp);
    if !out.status.success() {
        return Err(format!("whisper: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Parse line like: "whisper_full_with_state: auto-detected language: en (p = 0.999...)"
    let mut lang = String::new();
    for line in stderr.lines() {
        if let Some(pos) = line.find("auto-detected language:") {
            let rest = &line[pos + "auto-detected language:".len()..].trim();
            lang = rest.split_whitespace().next().unwrap_or("").to_string();
            break;
        }
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok((text, lang))
}

fn run_piper(cfg: &VoiceConfig, text: &str) -> Result<Vec<u8>, String> {
    use std::io::Write;
    eprintln!("[voice] piper synth: {:?}", text);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("empty text".into());
    }
    let out_wav = std::env::temp_dir().join(format!("forge-piper-{}-{}.wav",
        std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
    let mut child = Command::new(&cfg.piper_bin)
        .arg("--model").arg(&cfg.piper_voice)
        .arg("--output_file").arg(&out_wav)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn().map_err(|e| format!("piper spawn: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(trimmed.as_bytes()).map_err(|e| format!("piper stdin: {e}"))?;
    }
    let out = child.wait_with_output().map_err(|e| format!("piper wait: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("piper exit {}: {}", out.status, err));
    }
    let bytes = std::fs::read(&out_wav).map_err(|e| format!("piper read wav: {e}"))?;
    let _ = std::fs::remove_file(&out_wav);
    if bytes.is_empty() {
        return Err("piper produced empty WAV".into());
    }
    Ok(bytes)
}
