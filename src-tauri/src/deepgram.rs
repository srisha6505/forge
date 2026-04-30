//! Deepgram STT + TTS client. Blocking HTTP via ureq, matches the rest of
//! the voice stack's sync model.
//!
//! Keep whisper.cpp + piper as the fallback so users can still run fully
//! offline. Which provider is used per-turn is chosen in voice.rs based on
//! `VoiceConfig.provider`.

use std::io::Read;
use serde_json::Value;

const STT_URL: &str = "https://api.deepgram.com/v1/listen";
const TTS_URL: &str = "https://api.deepgram.com/v1/speak";

/// Default STT model. `nova-3` is current best, falls back to `nova-2` if
/// unavailable on the account tier.
pub const DEFAULT_STT_MODEL: &str = "nova-3";

/// Default TTS voice. `aura-2-thalia-en` is Aura 2's flagship female voice.
/// Users can override in settings.
pub const DEFAULT_TTS_VOICE: &str = "aura-2-thalia-en";

/// Transcribe 16 kHz mono f32 PCM via Deepgram's prerecorded endpoint.
///
/// We send the audio as a raw WAV blob rather than streaming. simplest, fits
/// the existing capture-then-transcribe loop. Streaming can be added later.
pub fn stt(pcm: &[f32], api_key: &str, language: Option<&str>, model: Option<&str>) -> Result<String, String> {
    if pcm.is_empty() { return Ok(String::new()); }
    if api_key.is_empty() { return Err("deepgram api key is empty".into()); }

    let wav = encode_wav(pcm, 16_000)?;

    let mut url = format!("{STT_URL}?model={}&smart_format=true&punctuate=true",
        model.unwrap_or(DEFAULT_STT_MODEL));
    if let Some(lang) = language {
        if !lang.is_empty() && lang != "auto" {
            url.push_str(&format!("&language={lang}"));
        } else {
            // Deepgram's multi-language detection.
            url.push_str("&detect_language=true");
        }
    }

    let resp = ureq::post(&url)
        .set("Authorization", &format!("Token {api_key}"))
        .set("Content-Type", "audio/wav")
        .send_bytes(&wav)
        .map_err(|e| format!("deepgram stt: {e}"))?;

    let body: Value = resp.into_json().map_err(|e| format!("deepgram stt parse: {e}"))?;
    let transcript = body
        .get("results")
        .and_then(|r| r.get("channels"))
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("alternatives"))
        .and_then(|a| a.get(0))
        .and_then(|a| a.get("transcript"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    Ok(transcript)
}

/// Synthesize speech to a WAV byte buffer via Deepgram's speak endpoint.
/// Returns raw WAV bytes (mono 16-bit 24 kHz for Aura 2 voices).
pub fn tts(text: &str, api_key: &str, voice: Option<&str>) -> Result<Vec<u8>, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() { return Err("empty tts text".into()); }
    if api_key.is_empty() { return Err("deepgram api key is empty".into()); }

    let voice = voice.unwrap_or(DEFAULT_TTS_VOICE);
    // `container=wav` makes the response a self-contained WAV so the web side
    // can just Blob-decode it without knowing the sample rate.
    let url = format!("{TTS_URL}?model={voice}&encoding=linear16&container=wav");

    let body = serde_json::json!({ "text": trimmed });
    let resp = ureq::post(&url)
        .set("Authorization", &format!("Token {api_key}"))
        .set("Content-Type", "application/json")
        .send_string(&body.to_string())
        .map_err(|e| format!("deepgram tts: {e}"))?;

    let mut buf = Vec::with_capacity(32 * 1024);
    resp.into_reader()
        .read_to_end(&mut buf)
        .map_err(|e| format!("deepgram tts read: {e}"))?;
    if buf.is_empty() {
        return Err("deepgram tts returned empty body".into());
    }
    Ok(buf)
}

fn encode_wav(pcm: &[f32], sample_rate: u32) -> Result<Vec<u8>, String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = std::io::Cursor::new(Vec::<u8>::with_capacity(pcm.len() * 2 + 44));
    {
        let mut w = hound::WavWriter::new(&mut cursor, spec)
            .map_err(|e| format!("wav writer: {e}"))?;
        for &s in pcm {
            let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
            w.write_sample(v).map_err(|e| format!("wav sample: {e}"))?;
        }
        w.finalize().map_err(|e| format!("wav finalize: {e}"))?;
    }
    Ok(cursor.into_inner())
}

// ── Streaming STT with built-in VAD ─────────────────────────────────

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

/// Open a Deepgram streaming session, feed it PCM chunks, return the final
/// transcript once Deepgram signals end-of-utterance.
///
/// Endpointing (Deepgram's server-side VAD) fires `speech_final: true` after
/// the configured silence window following speech. Until then we keep the
/// WebSocket open and feed mic data. `stop_flag` is checked between chunks
/// so the caller can abort (mute, interrupt, etc).
///
/// Single-threaded: underlying TCP socket is set non-blocking after
/// handshake, then we alternate poll-read / poll-write in one loop.
///
/// Blocking call; returns when an utterance is complete or `stop_flag` is set.
pub fn stream_transcribe(
    api_key: &str,
    model: Option<&str>,
    language: Option<&str>,
    mic_rx: mpsc::Receiver<Vec<f32>>,
    stop_flag: Arc<AtomicBool>,
) -> Result<String, String> {
    if api_key.is_empty() { return Err("deepgram api key is empty".into()); }

    let model = model.unwrap_or(DEFAULT_STT_MODEL);
    // `interim_results=true` is REQUIRED for speech_final to fire reliably.
    // Without it Deepgram only emits a single Results at end but the
    // endpointing signal often gets swallowed.
    // `utterance_end_ms=1000` → emit UtteranceEnd after 1s of true silence
    // (server-side VAD) even if endpointing didn't fire.
    let mut url_str = format!(
        "wss://api.deepgram.com/v1/listen?\
         model={}&encoding=linear16&sample_rate=16000&channels=1\
         &punctuate=true&smart_format=true\
         &endpointing=300&vad_events=true&interim_results=true&utterance_end_ms=1000",
        model
    );
    if let Some(lang) = language {
        if !lang.is_empty() && lang != "auto" {
            url_str.push_str(&format!("&language={lang}"));
        }
    }
    eprintln!("[dg-stream] opening {}", url_str);
    let url = url::Url::parse(&url_str).map_err(|e| format!("url: {e}"))?;

    let req = tungstenite::http::Request::builder()
        .method("GET")
        .uri(url.as_str())
        .header("Host", url.host_str().unwrap_or(""))
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", tungstenite::handshake::client::generate_key())
        .header("Authorization", format!("Token {api_key}"))
        .body(())
        .map_err(|e| format!("build req: {e}"))?;

    let (mut ws, _resp) = tungstenite::connect(req).map_err(|e| format!("ws connect: {e}"))?;

    // Give read() a short timeout so the single-threaded loop can fall
    // through to writes. Keep writes blocking. non-blocking sends break
    // under native-tls (WouldBlock on partial writes corrupts the stream).
    set_read_timeout(&mut ws, std::time::Duration::from_millis(50)).ok();

    let mut transcript = String::new();
    let mut got_final = false;
    let mut bytes_sent: usize = 0;
    let mut frames_sent: usize = 0;
    let mut events_received: usize = 0;
    let started = std::time::Instant::now();
    // Hard cap so the loop can't hang forever if every finalization signal
    // is silently dropped. 20s covers a long sentence + generous silence.
    let hard_cap = std::time::Duration::from_secs(20);
    // Idle cutoff: if we have a non-empty transcript and nothing new has
    // arrived for this long, treat it as the utterance.
    let idle_cap = std::time::Duration::from_millis(1500);
    let mut last_event_at = started;

    loop {
        if stop_flag.load(Ordering::Relaxed) {
            eprintln!("[dg-stream] stop flag set, breaking");
            break;
        }
        if started.elapsed() > hard_cap {
            eprintln!("[dg-stream] hard cap {}s hit", hard_cap.as_secs());
            break;
        }
        if !transcript.is_empty() && last_event_at.elapsed() > idle_cap {
            eprintln!("[dg-stream] idle {}ms with transcript, finalising",
                last_event_at.elapsed().as_millis());
            break;
        }

        // Poll one message from the WS.
        match ws.read() {
            Ok(tungstenite::Message::Text(txt)) => {
                events_received += 1;
                let v: serde_json::Value = serde_json::from_str(&txt).unwrap_or(serde_json::Value::Null);
                let kind = v["type"].as_str().unwrap_or("");
                last_event_at = std::time::Instant::now();
                match kind {
                    "Results" => {
                        let alt = &v["channel"]["alternatives"][0];
                        let text = alt["transcript"].as_str().unwrap_or("").trim().to_string();
                        let is_final = v["is_final"].as_bool().unwrap_or(false);
                        let speech_final = v["speech_final"].as_bool().unwrap_or(false);
                        eprintln!("[dg-stream] Results is_final={} speech_final={} text={:?}",
                            is_final, speech_final, text);
                        if is_final && !text.is_empty() {
                            if !transcript.is_empty() { transcript.push(' '); }
                            transcript.push_str(&text);
                        }
                        if speech_final {
                            got_final = true;
                        }
                    }
                    "UtteranceEnd" => {
                        eprintln!("[dg-stream] UtteranceEnd");
                        got_final = true;
                    }
                    "SpeechStarted" => {
                        eprintln!("[dg-stream] SpeechStarted");
                    }
                    "Metadata" => {
                        eprintln!("[dg-stream] Metadata: request_id={}",
                            v["request_id"].as_str().unwrap_or(""));
                    }
                    "Error" => {
                        eprintln!("[dg-stream] server error: {}", v);
                        break;
                    }
                    other => {
                        eprintln!("[dg-stream] other msg type={} body={}", other, v);
                    }
                }
            }
            Ok(tungstenite::Message::Close(f)) => {
                eprintln!("[dg-stream] server closed: {:?}", f);
                break;
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) =>
            {
                // No pending message; fall through to writes.
            }
            Err(e) => {
                eprintln!("[dg-stream] read err: {e}");
                break;
            }
        }

        if got_final { break; }

        // Drain PCM queue. send up to 5 chunks per iteration so we keep up
        // with a fast mic callback without starving reads.
        for _ in 0..5 {
            match mic_rx.try_recv() {
                Ok(chunk) => {
                    if chunk.is_empty() { continue; }
                    let mut bytes = Vec::with_capacity(chunk.len() * 2);
                    for &s in &chunk {
                        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                        bytes.extend_from_slice(&v.to_le_bytes());
                    }
                    let n = bytes.len();
                    if let Err(e) = ws.send(tungstenite::Message::Binary(bytes)) {
                        eprintln!("[dg-stream] send err after {} bytes: {e}", bytes_sent);
                        return Err(format!("ws send: {e}"));
                    }
                    bytes_sent += n;
                    frames_sent += 1;
                    if frames_sent % 50 == 0 {
                        eprintln!("[dg-stream] streamed {} frames / {} bytes / {} events",
                            frames_sent, bytes_sent, events_received);
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    eprintln!("[dg-stream] mic channel disconnected");
                    got_final = true;
                    break;
                }
            }
        }
        if !got_final {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    eprintln!("[dg-stream] loop exit: frames={} bytes={} events={} transcript={:?}",
        frames_sent, bytes_sent, events_received, transcript);

    // Signal end-of-stream so Deepgram flushes any remaining transcript.
    let _ = ws.send(tungstenite::Message::Text(r#"{"type":"CloseStream"}"#.into()));
    // Drain a few more messages in case a final Results is still pending.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
    while std::time::Instant::now() < deadline {
        match ws.read() {
            Ok(tungstenite::Message::Text(txt)) => {
                let v: serde_json::Value = serde_json::from_str(&txt).unwrap_or(serde_json::Value::Null);
                if v["type"].as_str() == Some("Results") {
                    let alt = &v["channel"]["alternatives"][0];
                    let text = alt["transcript"].as_str().unwrap_or("").trim();
                    let is_final = v["is_final"].as_bool().unwrap_or(false);
                    if is_final && !text.is_empty() {
                        if !transcript.is_empty() { transcript.push(' '); }
                        transcript.push_str(text);
                    }
                }
            }
            Ok(tungstenite::Message::Close(_)) | Err(_) => break,
            _ => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }
    let _ = ws.close(None);

    Ok(transcript)
}

/// Set a read timeout on the underlying TCP socket so `read()` returns
/// with TimedOut/WouldBlock instead of hanging forever.
fn set_read_timeout(
    ws: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    dur: std::time::Duration,
) -> std::io::Result<()> {
    use tungstenite::stream::MaybeTlsStream;
    match ws.get_mut() {
        MaybeTlsStream::Plain(s) => s.set_read_timeout(Some(dur)),
        MaybeTlsStream::NativeTls(s) => s.get_mut().set_read_timeout(Some(dur)),
        _ => Ok(()),
    }
}
