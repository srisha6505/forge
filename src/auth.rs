//! OAuth authentication for Anthropic API using Claude subscription.
//! Implements PKCE browser flow with localhost callback.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// OAuth constants (from Claude Code's registered app).
const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
// Console auth URL grants org:create_api_key scope (Claude.ai URL does not).
const AUTH_URL: &str = "https://platform.claude.com/oauth/authorize";
const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const SCOPES: &str = "org:create_api_key user:inference user:profile";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at_ms: Option<u64>,
    #[serde(default)]
    pub email: Option<String>,
    /// API key created from the OAuth token (used for actual API calls).
    #[serde(default)]
    pub api_key: Option<String>,
}

impl OAuthTokens {
    fn token_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("forge")
            .join("oauth_tokens.json")
    }

    pub fn load() -> Option<Self> {
        let path = Self::token_path();
        let data = fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    pub fn save(&self) {
        let path = Self::token_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            fs::write(&path, json).ok();
            // Restrict permissions on Unix.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).ok();
            }
        }
    }

    pub fn delete() {
        fs::remove_file(Self::token_path()).ok();
    }

    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires_at_ms {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            now >= expires
        } else {
            false
        }
    }
}

/// Generate a PKCE code verifier (43-128 chars, Base64url).
fn generate_code_verifier() -> String {
    let mut bytes = vec![0u8; 32];
    openssl::rand::rand_bytes(&mut bytes).expect("CSPRNG");
    base64url_encode(&bytes)
}

fn base64url_encode(data: &[u8]) -> String {
    use openssl::base64::encode_block;
    encode_block(data)
        .replace('+', "-")
        .replace('/', "_")
        .replace('=', "")
        .replace('\n', "")
}

/// Generate PKCE code challenge from verifier (S256).
fn code_challenge(verifier: &str) -> String {
    let hash = openssl::hash::hash(
        openssl::hash::MessageDigest::sha256(),
        verifier.as_bytes(),
    ).expect("SHA-256");
    base64url_encode(&hash)
}

/// Run the OAuth login flow. Opens browser, waits for callback, returns tokens.
/// This blocks until the user completes auth or timeout (120s).
pub fn login() -> Result<OAuthTokens, String> {
    let verifier = generate_code_verifier();
    let challenge = code_challenge(&verifier);
    let state = generate_code_verifier(); // reuse for state param

    // Bind to random port for callback.
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("Failed to bind localhost: {e}"))?;
    let port = listener.local_addr()
        .map_err(|e| format!("Failed to get port: {e}"))?
        .port();
    let redirect_uri = format!("http://localhost:{port}/callback");

    // Build authorization URL.
    let auth_url = format!(
        "{AUTH_URL}?code=true&client_id={}&response_type=code&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&state={}",
        urlencod(CLIENT_ID),
        urlencod(&redirect_uri),
        urlencod(SCOPES),
        urlencod(&challenge),
        urlencod(&state),
    );

    eprintln!("[forge-auth] Opening browser for authentication...");
    eprintln!("[forge-auth] If browser doesn't open, visit: {auth_url}");

    // Try to open browser.
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(&auth_url).spawn().ok();
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(&auth_url).spawn().ok();
    }

    // Set timeout.
    listener.set_nonblocking(false).ok();
    let timeout = std::time::Duration::from_secs(120);
    listener.set_nonblocking(true).ok();

    // Wait for callback.
    let start = std::time::Instant::now();
    let auth_code = loop {
        if start.elapsed() > timeout {
            return Err("Authentication timed out (120s)".into());
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);

                // Extract code from GET /callback?code=xxx&state=yyy
                if let Some(code) = extract_param(&request, "code") {
                    // Send success response to browser.
                    let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
                        <html><body><h2>Authentication successful!</h2>\
                        <p>You can close this tab and return to Forge.</p></body></html>";
                    stream.write_all(response.as_bytes()).ok();
                    break code;
                } else {
                    let response = "HTTP/1.1 400 Bad Request\r\n\r\nMissing code parameter";
                    stream.write_all(response.as_bytes()).ok();
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => return Err(format!("Listener error: {e}")),
        }
    };

    eprintln!("[forge-auth] Got auth code, exchanging for token...");
    eprintln!("[forge-auth] code={}, verifier_len={}, challenge={}",
        &auth_code[..auth_code.len().min(10)], verifier.len(), challenge);

    // Exchange code for token (form-urlencoded, NOT JSON -- OAuth standard).
    let form_body = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}&state={}",
        urlencod(&auth_code),
        urlencod(&redirect_uri),
        urlencod(CLIENT_ID),
        urlencod(&verifier),
        urlencod(&state),
    );

    eprintln!("[forge-auth] POST {} (body len={})", TOKEN_URL, form_body.len());

    let resp = match ureq::post(TOKEN_URL)
        .set("content-type", "application/x-www-form-urlencoded")
        .send_bytes(form_body.as_bytes())
    {
        Ok(r) => r,
        Err(ureq::Error::Status(code, resp)) => {
            let error_body = resp.into_string().unwrap_or_default();
            eprintln!("[forge-auth] Token exchange HTTP {code}: {error_body}");
            return Err(format!("Token exchange failed (HTTP {code}): {error_body}"));
        }
        Err(e) => return Err(format!("Token exchange request failed: {e}")),
    };

    let body: serde_json::Value = resp.into_json()
        .map_err(|e| format!("Failed to parse token response: {e}"))?;

    let access_token = body.get("access_token")
        .and_then(|v| v.as_str())
        .ok_or("No access_token in response")?
        .to_string();

    let refresh_token = body.get("refresh_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let expires_in = body.get("expires_in")
        .and_then(|v| v.as_u64())
        .unwrap_or(3600);

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    // Exchange access token for an API key (required for API calls).
    eprintln!("[forge-auth] Creating API key from OAuth token...");
    let api_key = match create_api_key(&access_token) {
        Ok(key) => {
            eprintln!("[forge-auth] API key created successfully");
            Some(key)
        }
        Err(e) => {
            eprintln!("[forge-auth] API key creation failed: {e} (will use Bearer token)");
            None
        }
    };

    let tokens = OAuthTokens {
        access_token,
        refresh_token,
        expires_at_ms: Some(now_ms + expires_in * 1000),
        email: body.get("email").and_then(|v| v.as_str()).map(|s| s.to_string()),
        api_key,
    };

    tokens.save();
    eprintln!("[forge-auth] Authentication successful!");

    Ok(tokens)
}

/// Exchange OAuth access token for a persistent API key.
fn create_api_key(access_token: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "name": "forge-editor"
    });

    let resp = match ureq::post("https://api.anthropic.com/api/oauth/claude_cli/create_api_key")
        .set("Authorization", &format!("Bearer {access_token}"))
        .set("anthropic-version", "2023-06-01")
        .set("content-type", "application/json")
        .send_json(body)
    {
        Ok(r) => r,
        Err(ureq::Error::Status(code, resp)) => {
            let error_body = resp.into_string().unwrap_or_default();
            return Err(format!("HTTP {code}: {error_body}"));
        }
        Err(e) => return Err(format!("{e}")),
    };

    let result: serde_json::Value = resp.into_json()
        .map_err(|e| format!("Parse error: {e}"))?;

    result.get("api_key")
        .or_else(|| result.get("key"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("No api_key in response: {result}"))
}

/// Refresh an expired access token.
pub fn refresh(tokens: &OAuthTokens) -> Result<OAuthTokens, String> {
    let refresh_token = tokens.refresh_token.as_deref()
        .ok_or("No refresh token available")?;

    let form_body = format!(
        "grant_type=refresh_token&refresh_token={}&client_id={}&scope={}",
        urlencod(refresh_token),
        urlencod(CLIENT_ID),
        urlencod(SCOPES),
    );

    let resp = ureq::post(TOKEN_URL)
        .set("content-type", "application/x-www-form-urlencoded")
        .send_bytes(form_body.as_bytes())
        .map_err(|e| format!("Token refresh failed: {e}"))?;

    let body: serde_json::Value = resp.into_json()
        .map_err(|e| format!("Failed to parse refresh response: {e}"))?;

    let access_token = body.get("access_token")
        .and_then(|v| v.as_str())
        .ok_or("No access_token in refresh response")?
        .to_string();

    let new_refresh = body.get("refresh_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| tokens.refresh_token.clone());

    let expires_in = body.get("expires_in")
        .and_then(|v| v.as_u64())
        .unwrap_or(3600);

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let new_tokens = OAuthTokens {
        access_token,
        refresh_token: new_refresh,
        expires_at_ms: Some(now_ms + expires_in * 1000),
        email: tokens.email.clone(),
        api_key: tokens.api_key.clone(),
    };

    new_tokens.save();
    Ok(new_tokens)
}

/// Get auth credentials. Returns (header_name, header_value).
/// Tries: our saved API key -> our OAuth token -> Claude Code's credentials.
pub fn get_auth_header() -> Result<(String, String), String> {
    // 1. Check our own saved tokens.
    if let Some(tokens) = OAuthTokens::load() {
        if let Some(key) = &tokens.api_key {
            return Ok(("x-api-key".to_string(), key.clone()));
        }
    }

    // 2. Try to read Claude Code's credentials and create an API key.
    if let Some(cc_token) = read_claude_code_token() {
        match create_api_key(&cc_token) {
            Ok(key) => {
                // Save for future use.
                let tokens = OAuthTokens {
                    access_token: cc_token,
                    refresh_token: None,
                    expires_at_ms: None,
                    email: None,
                    api_key: Some(key.clone()),
                };
                tokens.save();
                return Ok(("x-api-key".to_string(), key));
            }
            Err(_) => {}
        }
    }

    Err("No API key found. Set api_key in ~/.config/forge/settings.json or get one from console.anthropic.com".to_string())
}

/// Read Claude Code's OAuth access token from ~/.claude/.credentials.json.
fn read_claude_code_token() -> Option<String> {
    let path = dirs::home_dir()?.join(".claude").join(".credentials.json");
    let data = fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&data).ok()?;
    v.get("claudeAiOauth")?
        .get("accessToken")?
        .as_str()
        .map(|s| s.to_string())
}

fn extract_param(request: &str, name: &str) -> Option<String> {
    let query = request.split('?').nth(1)?;
    let query = query.split(' ').next()?; // stop at HTTP/1.1
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        if parts.next()? == name {
            return parts.next().map(|v| urlencod_decode(v));
        }
    }
    None
}

fn urlencod(s: &str) -> String {
    let mut result = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", b));
            }
        }
    }
    result
}

fn urlencod_decode(s: &str) -> String {
    let mut result = Vec::new();
    let mut chars = s.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let h = chars.next().unwrap_or(0);
            let l = chars.next().unwrap_or(0);
            let hex = [h, l];
            if let Ok(s) = std::str::from_utf8(&hex) {
                if let Ok(v) = u8::from_str_radix(s, 16) {
                    result.push(v);
                    continue;
                }
            }
        } else if b == b'+' {
            result.push(b' ');
        } else {
            result.push(b);
        }
    }
    String::from_utf8_lossy(&result).to_string()
}
