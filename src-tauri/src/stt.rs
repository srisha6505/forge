//! Speech-to-text via whisper.cpp CLI subprocess.
//!
//! We avoid linking whisper-rs (which bundles its own ggml and clashes with
//! llama-cpp-sys-2's ggml). Instead we shell out to a pre-built `whisper-cli`
//! binary that uses GGML models directly.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::process::Command;

/// Lazy handle (currently just holds the binary path; can be extended later).
pub struct Whisper {
    pub binary: PathBuf,
    pub model: PathBuf,
}

impl Whisper {
    pub fn new(model_path: &Path) -> Result<Self, String> {
        // Single source of truth for binary lookup: env var → managed
        // bin dir (~/.local/share/forge/bin/, where the in-app installer
        // drops it) → legacy ~/.forge/bin/ → PATH. Keeping this in sync
        // with binaries.rs so users who installed via Settings → STT
        // don't see "binary not found" because of a path mismatch.
        let binary = crate::binaries::resolve_whisper_cli().ok_or_else(|| {
            "whisper-cli not found. Install it via Settings → STT, set FORGE_WHISPER_BIN, or put it on PATH.".to_string()
        })?;
        if !model_path.exists() {
            return Err(format!("whisper model not found: {}", model_path.display()));
        }
        eprintln!("[forge-stt] whisper-cli={} model={}", binary.display(), model_path.display());
        Ok(Self { binary, model: model_path.to_path_buf() })
    }

    /// Transcribe 16kHz mono f32 PCM by writing to a temp WAV + calling whisper-cli.
    pub fn transcribe(&self, pcm: &[f32], language: Option<&str>) -> Result<String, String> {
        if pcm.is_empty() { return Ok(String::new()); }

        // Write WAV to temp file.
        let tmp = std::env::temp_dir().join(format!("forge-stt-{}.wav", std::process::id()));
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        {
            let mut w = hound::WavWriter::create(&tmp, spec)
                .map_err(|e| format!("wav create: {e}"))?;
            for &s in pcm {
                let v = (s.max(-1.0).min(1.0) * 32767.0) as i16;
                w.write_sample(v).map_err(|e| format!("wav write: {e}"))?;
            }
            w.finalize().map_err(|e| format!("wav finalize: {e}"))?;
        }

        let mut cmd = Command::new(&self.binary);
        cmd.arg("-m").arg(&self.model)
            .arg("-f").arg(&tmp)
            .arg("--no-timestamps")
            .arg("--no-prints");
        if let Some(lang) = language {
            if lang != "auto" && !lang.is_empty() {
                cmd.arg("-l").arg(lang);
            }
        } else {
            cmd.arg("-l").arg("auto");
        }

        let out = cmd.output().map_err(|e| format!("whisper-cli spawn: {e}"))?;
        let _ = std::fs::remove_file(&tmp);
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(format!("whisper-cli exit {}: {}", out.status, err));
        }
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(text)
    }
}

/// Decode a WAV byte slice into 16kHz mono f32 PCM.
pub fn decode_wav(bytes: &[u8]) -> Result<Vec<f32>, String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut reader = hound::WavReader::new(cursor).map_err(|e| format!("wav: {e}"))?;
    let spec = reader.spec();
    let src_rate = spec.sample_rate;
    let channels = spec.channels as usize;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().filter_map(|s| s.ok()).map(|s| s as f32 / max).collect()
        }
        hound::SampleFormat::Float => reader.samples::<f32>().filter_map(|s| s.ok()).collect(),
    };
    let mono: Vec<f32> = if channels == 1 {
        samples
    } else {
        samples.chunks(channels).map(|f| f.iter().sum::<f32>() / channels as f32).collect()
    };
    Ok(to_mono_16khz(&mono, src_rate, 1))
}

pub struct WhisperHandle(pub Mutex<Option<Whisper>>);
impl Default for WhisperHandle {
    fn default() -> Self { Self(Mutex::new(None)) }
}

// ── Mic capture via cpal on a dedicated worker thread ────────────────

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::sync::mpsc;

enum MicCmd { Stop }

pub struct MicSession {
    tx: mpsc::Sender<MicCmd>,
    result_rx: mpsc::Receiver<Result<(Vec<f32>, u32, u16), String>>,
    running: Arc<AtomicBool>,
}

pub struct MicHandle(pub Mutex<Option<MicSession>>);
impl Default for MicHandle {
    fn default() -> Self { Self(Mutex::new(None)) }
}

impl MicSession {
    pub fn start() -> Result<Self, String> {
        let (cmd_tx, cmd_rx) = mpsc::channel::<MicCmd>();
        let (res_tx, res_rx) = mpsc::channel::<Result<(Vec<f32>, u32, u16), String>>();
        let running = Arc::new(AtomicBool::new(true));
        let running_thread = Arc::clone(&running);

        std::thread::Builder::new()
            .name("forge-mic".into())
            .spawn(move || {
                let host = cpal::default_host();
                // Try the default device first. If its config-probe fails
                // (common on Linux when pipewire's default device is in a
                // weird state), fall back to walking every input device
                // until one returns a valid config. Surface a list of
                // attempted devices in the error so the user can pick
                // a different default in their OS sound settings.
                let mut tried: Vec<String> = Vec::new();
                let mut picked: Option<(cpal::Device, cpal::SupportedStreamConfig)> = None;

                if let Some(d) = host.default_input_device() {
                    let name = d.name().unwrap_or_else(|_| "default".into());
                    match d.default_input_config() {
                        Ok(c) => picked = Some((d, c)),
                        Err(e) => tried.push(format!("{name}: {e}")),
                    }
                }
                if picked.is_none() {
                    let devices = host.input_devices().ok();
                    if let Some(it) = devices {
                        for d in it {
                            let name = d.name().unwrap_or_else(|_| "?".into());
                            match d.default_input_config() {
                                Ok(c) => { picked = Some((d, c)); break; }
                                Err(e) => tried.push(format!("{name}: {e}")),
                            }
                        }
                    }
                }
                let (device, cfg) = match picked {
                    Some(p) => p,
                    None => {
                        let msg = if tried.is_empty() {
                            "No input device available".to_string()
                        } else {
                            format!(
                                "No usable input device. Tried: {}",
                                tried.join("; ")
                            )
                        };
                        let _ = res_tx.send(Err(msg));
                        return;
                    }
                };
                let input_rate = cfg.sample_rate().0;
                let channels = cfg.channels();
                eprintln!("[forge-stt] mic: device={:?} rate={} channels={}",
                    device.name().unwrap_or_default(), input_rate, channels);

                let samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
                let samples_cb = Arc::clone(&samples);
                let running_cb = Arc::clone(&running_thread);
                let err_fn = |e| eprintln!("[forge-stt] mic stream error: {e}");

                let stream_result = match cfg.sample_format() {
                    cpal::SampleFormat::F32 => device.build_input_stream(
                        &cfg.config(),
                        move |data: &[f32], _: &_| {
                            if !running_cb.load(Ordering::Relaxed) { return; }
                            if let Ok(mut buf) = samples_cb.lock() { buf.extend_from_slice(data); }
                        }, err_fn, None),
                    cpal::SampleFormat::I16 => device.build_input_stream(
                        &cfg.config(),
                        move |data: &[i16], _: &_| {
                            if !running_cb.load(Ordering::Relaxed) { return; }
                            if let Ok(mut buf) = samples_cb.lock() {
                                buf.extend(data.iter().map(|&s| s as f32 / 32768.0));
                            }
                        }, err_fn, None),
                    cpal::SampleFormat::U16 => device.build_input_stream(
                        &cfg.config(),
                        move |data: &[u16], _: &_| {
                            if !running_cb.load(Ordering::Relaxed) { return; }
                            if let Ok(mut buf) = samples_cb.lock() {
                                buf.extend(data.iter().map(|&s| (s as f32 - 32768.0) / 32768.0));
                            }
                        }, err_fn, None),
                    fmt => { let _ = res_tx.send(Err(format!("unsupported: {:?}", fmt))); return; }
                };
                let stream = match stream_result {
                    Ok(s) => s,
                    Err(e) => { let _ = res_tx.send(Err(format!("build stream: {e}"))); return; }
                };
                if let Err(e) = stream.play() {
                    let _ = res_tx.send(Err(format!("stream play: {e}")));
                    return;
                }
                let _ = cmd_rx.recv();
                running_thread.store(false, Ordering::Relaxed);
                drop(stream);
                let buf = samples.lock().map(|g| g.clone()).unwrap_or_default();
                let _ = res_tx.send(Ok((buf, input_rate, channels)));
            })
            .map_err(|e| format!("spawn mic thread: {e}"))?;

        Ok(Self { tx: cmd_tx, result_rx: res_rx, running })
    }

    pub fn stop_and_take(self) -> Result<(Vec<f32>, u32, u16), String> {
        self.running.store(false, Ordering::Relaxed);
        let _ = self.tx.send(MicCmd::Stop);
        self.result_rx.recv().map_err(|e| format!("mic result: {e}"))?
    }
}

pub fn to_mono_16khz(samples: &[f32], src_rate: u32, channels: u16) -> Vec<f32> {
    let mono: Vec<f32> = if channels <= 1 {
        samples.to_vec()
    } else {
        samples.chunks(channels as usize)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    };
    const TARGET: u32 = 16000;
    if src_rate == TARGET { return mono; }
    let ratio = TARGET as f32 / src_rate as f32;
    let new_len = (mono.len() as f32 * ratio) as usize;
    let mut out = Vec::with_capacity(new_len);
    for i in 0..new_len {
        let p = i as f32 / ratio;
        let idx = p as usize;
        let frac = p - idx as f32;
        let a = mono.get(idx).copied().unwrap_or(0.0);
        let b = mono.get(idx + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }
    out
}
