//! Microsoft Edge TTS (public service used by Edge's Read Aloud). Free,
//! no API key, ~400 multilingual voices. Uses a WebSocket to
//! speech.platform.bing.com with a well-known client token.
//!
//! We request 24kHz 16-bit mono PCM output and wrap it in a WAV header
//! so the frontend audio element plays it without an MP3 decoder.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rand::RngCore;
use tungstenite::{Message, client::IntoClientRequest, connect};

const TRUSTED_CLIENT_TOKEN: &str = "6A5AA1D4EAFF4E9FB37E23D68491D6F4";
const WS_URL: &str = "wss://speech.platform.bing.com/consumer/speech/synthesize/readaloud/edge/v1";
// Edge's public service only streams MP3 back (raw PCM requests are
// silently rejected and yield no binary frames).
const OUTPUT_FORMAT: &str = "audio-24khz-48kbitrate-mono-mp3";
const CHROMIUM_FULL_VERSION: &str = "143.0.3650.75";
/// Windows filetime epoch: seconds from 1601-01-01 to 1970-01-01.
const WIN_EPOCH_SECS: u64 = 11_644_473_600;

/// Synthesize `text` as `voice` (e.g. "en-US-JennyNeural"). Returns a
/// complete WAV file (PCM 24kHz mono). `rate`/`pitch` take
/// percentage-style strings like "+0%", "-10%".
pub fn synth(text: &str, voice: &str, rate: &str, pitch: &str) -> Result<Vec<u8>, String> {
    synth_cancellable(text, voice, rate, pitch, Arc::new(AtomicBool::new(false)))
}

pub fn synth_cancellable(
    text: &str,
    voice: &str,
    rate: &str,
    pitch: &str,
    cancel: Arc<AtomicBool>,
) -> Result<Vec<u8>, String> {
    let text = text.trim();
    if text.is_empty() { return Err("empty text".into()); }
    let voice = if voice.is_empty() { "en-US-JennyNeural" } else { voice };

    let gec = sec_ms_gec();
    let gec_ver = format!("1-{}", CHROMIUM_FULL_VERSION);
    let url = format!(
        "{WS_URL}?TrustedClientToken={TRUSTED_CLIENT_TOKEN}&Sec-MS-GEC={gec}&Sec-MS-GEC-Version={gec_ver}&ConnectionId={}",
        request_id()
    );

    // Add headers Edge expects. tungstenite::connect pulls origin / host
    // from URL automatically; we only need to inject UA + Origin.
    let mut req = url.as_str().into_client_request()
        .map_err(|e| format!("req build: {e}"))?;
    {
        let h = req.headers_mut();
        h.insert(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36 Edg/143.0.0.0"
                .parse().unwrap(),
        );
        h.insert(
            "Origin",
            "chrome-extension://jdiccldimpdaibmpdkjnbmckianbfold".parse().unwrap(),
        );
        h.insert("Pragma", "no-cache".parse().unwrap());
        h.insert("Cache-Control", "no-cache".parse().unwrap());
        h.insert("Accept-Encoding", "gzip, deflate, br, zstd".parse().unwrap());
        h.insert("Accept-Language", "en-US,en;q=0.9".parse().unwrap());
    }

    let (mut ws, _resp) = connect(req).map_err(|e| format!("ws connect: {e}"))?;

    // Per-request id (hex). Used in every frame's X-RequestId.
    let req_id = request_id();
    let ts = timestamp();

    // Frame 1: synthesis config (picks output format). Edge rejects
    // metadata bools as JSON booleans. they must be quoted strings.
    let cfg_body = format!(
        "{{\"context\":{{\"synthesis\":{{\"audio\":{{\
         \"metadataoptions\":{{\"sentenceBoundaryEnabled\":\"false\",\"wordBoundaryEnabled\":\"false\"}},\
         \"outputFormat\":\"{OUTPUT_FORMAT}\"}}}}}}}}"
    );
    let cfg_msg = format!(
        "X-Timestamp:{ts}\r\nContent-Type:application/json; charset=utf-8\r\nPath:speech.config\r\n\r\n{cfg_body}"
    );
    ws.send(Message::Text(cfg_msg)).map_err(|e| format!("send cfg: {e}"))?;

    // Frame 2: SSML.
    let ssml = build_ssml(voice, rate, pitch, text);
    let ssml_msg = format!(
        "X-RequestId:{req_id}\r\nContent-Type:application/ssml+xml\r\nX-Timestamp:{ts}Z\r\nPath:ssml\r\n\r\n{ssml}"
    );
    ws.send(Message::Text(ssml_msg)).map_err(|e| format!("send ssml: {e}"))?;

    // Collect MP3 bytes from binary frames until turn.end.
    let mut mp3: Vec<u8> = Vec::with_capacity(64 * 1024);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = ws.close(None);
            return Err("cancelled".into());
        }
        if std::time::Instant::now() > deadline {
            let _ = ws.close(None);
            return Err("edge tts timeout".into());
        }
        let msg = match ws.read() {
            Ok(m) => m,
            Err(e) => return Err(format!("ws read: {e}")),
        };
        match msg {
            Message::Text(t) => {
                if t.contains("Path:turn.end") {
                    let _ = ws.close(None);
                    break;
                }
            }
            Message::Binary(b) => {
                // Header len is a big-endian u16 at offset 0; payload
                // follows at offset 2+hlen.
                if b.len() < 2 { continue; }
                let hlen = ((b[0] as usize) << 8) | (b[1] as usize);
                if b.len() < 2 + hlen { continue; }
                mp3.extend_from_slice(&b[2 + hlen..]);
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    if mp3.is_empty() {
        return Err("edge tts returned no audio (check voice name or network)".into());
    }
    Ok(mp3)
}

fn build_ssml(voice: &str, rate: &str, pitch: &str, text: &str) -> String {
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        "<speak version='1.0' xmlns='http://www.w3.org/2001/10/synthesis' xml:lang='en-US'>\
         <voice name='{voice}'>\
         <prosody rate='{rate}' pitch='{pitch}'>{escaped}</prosody>\
         </voice></speak>"
    )
}

fn request_id() -> String {
    let mut buf = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Microsoft's anti-abuse header. Algorithm: take unix seconds, shift to
/// Windows file-time epoch, round down to the current 5-minute window,
/// convert to 100-ns ticks, then SHA-256 over `"{ticks}{TOKEN}"` and
/// uppercase hex. Matches the reference `edge-tts` Python lib.
fn sec_ms_gec() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let unix_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let win_secs = unix_secs + WIN_EPOCH_SECS;
    let rounded = win_secs - (win_secs % 300);
    let ticks: u64 = rounded * 10_000_000;
    let payload = format!("{ticks}{TRUSTED_CLIENT_TOKEN}");
    let digest = openssl::sha::sha256(payload.as_bytes());
    let mut s = String::with_capacity(64);
    for b in digest {
        s.push_str(&format!("{:02X}", b));
    }
    s
}

fn timestamp() -> String {
    // Edge accepts any ISO-ish string. UTC components derived from epoch
    // seconds; avoids pulling in chrono.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let h = (rem / 3600) as u32;
    let mi = ((rem % 3600) / 60) as u32;
    let s = (rem % 60) as u32;
    let d = days + 719_468;
    let era = if d >= 0 { d } else { d - 146_096 } / 146_097;
    let doe = (d - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5) as u32 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (y + if month <= 2 { 1 } else { 0 }) as u32;
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{mi:02}:{s:02}.000")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ssml_escapes() {
        let s = build_ssml("v", "+0%", "+0%", "a & b < c");
        assert!(s.contains("a &amp; b &lt; c"));
    }
}
