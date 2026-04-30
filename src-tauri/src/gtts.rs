//! Unofficial Google Translate TTS. Free, no API key, but rate-limited
//! and capped at ~200 chars per request. We chunk the text to stay
//! under the limit and concatenate the MP3 responses.
//!
//! Returns raw MP3 bytes. The frontend <audio> element handles MP3
//! natively; we don't decode to WAV here.

const BASE: &str = "https://translate.google.com/translate_tts";
const MAX_CHARS: usize = 180;

/// Synthesize `text` in `lang` (BCP-47-ish: "en", "es", "fr", "hi",
/// "ja", etc.). Returns concatenated MP3 bytes.
pub fn synth(text: &str, lang: &str) -> Result<Vec<u8>, String> {
    let text = text.trim();
    if text.is_empty() { return Err("empty text".into()); }
    let lang = if lang.is_empty() { "en" } else { lang };

    let chunks = chunk_text(text, MAX_CHARS);
    let total = chunks.len();
    let mut out = Vec::with_capacity(16 * 1024);
    for (i, c) in chunks.iter().enumerate() {
        let tk = generate_token(c);
        let url = format!(
            "{BASE}?ie=UTF-8&client=tw-ob&tl={lang}&total={total}&idx={i}&textlen={}&tk={tk}&q={}",
            c.chars().count(),
            urlencode(c)
        );
        let resp = ureq::get(&url)
            .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .set("Referer", "https://translate.google.com/")
            .call()
            .map_err(|e| format!("gtts request: {e}"))?;
        let mut body = Vec::new();
        use std::io::Read;
        resp.into_reader().take(2 * 1024 * 1024).read_to_end(&mut body)
            .map_err(|e| format!("gtts read: {e}"))?;
        if body.is_empty() { return Err("gtts: empty chunk".into()); }
        out.extend_from_slice(&body);
    }
    Ok(out)
}

/// Split on sentence/word boundaries staying under `max` chars.
fn chunk_text(text: &str, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    for word in text.split_whitespace() {
        if buf.chars().count() + word.chars().count() + 1 > max {
            if !buf.is_empty() {
                out.push(std::mem::take(&mut buf));
            }
            // A single word > max is rare; push as-is.
            if word.chars().count() > max {
                out.push(word.to_string());
                continue;
            }
        }
        if !buf.is_empty() { buf.push(' '); }
        buf.push_str(word);
    }
    if !buf.is_empty() { out.push(buf); }
    if out.is_empty() { out.push(text.to_string()); }
    out
}

/// Reverse-engineered Google Translate `tk` token. Computed over the
/// text with a time-windowed seed. The algorithm below matches the one
/// used by `gtts-token` / `gTTS` and has been stable for years.
fn generate_token(text: &str) -> String {
    let tkk_a: i64 = 406644;
    let tkk_b: i64 = 3293161072;
    let a = tkk_a;
    let b = tkk_b;

    let mut d: i64 = a;
    for ch in text.chars() {
        d = work_token(d + ch as i64, "+-a^+6");
    }
    d = work_token(d, "+-3^+b+-f");
    d ^= b;
    if d < 0 {
        d = (d & 0x7fffffff) + 0x80000000;
    }
    d %= 1_000_000;
    format!("{}.{}", d, d ^ a)
}

fn work_token(mut a: i64, seed: &str) -> i64 {
    let bytes = seed.as_bytes();
    let mut i = 0;
    while i < bytes.len() - 2 {
        let ch = bytes[i + 2];
        let d = if ch >= b'a' { (ch - 87) as i64 } else { (ch - b'0') as i64 };
        let op = bytes[i + 1];
        let shift = if op == b'+' { a as u32 >> d as u32 } else { (a << d) as u32 };
        a = if bytes[i] == b'+' {
            ((a as u32).wrapping_add(shift) & 0xffffffff) as i64
        } else {
            (a ^ shift as i64) & 0xffffffff
        };
        i += 3;
    }
    a
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chunking_respects_max() {
        let text = "one two three four five six seven eight nine ten";
        for c in chunk_text(text, 10) {
            assert!(c.chars().count() <= 10, "chunk too long: {c:?}");
        }
    }
    #[test]
    fn urlenc() {
        assert_eq!(urlencode("hello world"), "hello%20world");
    }
}
