//! GitHub Copilot auth + chat API client.
//!
//! Two-step auth:
//!   1. GitHub Device Flow to obtain a user OAuth token (long-lived).
//!   2. Exchange that token for a short-lived Copilot API token
//!      (~30 min TTL, auto-refreshed).
//!
//! Chat endpoint is OpenAI-compatible (`/v1/chat/completions`) with extra
//! editor-identification headers.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// VS Code Copilot plugin client id. same value used by every OSS project
/// that bridges Copilot.
pub const CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";

const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Tokens {
    /// Long-lived GitHub OAuth token from the device flow.
    pub github_token: Option<String>,
    /// Short-lived Copilot API token (for api.githubcopilot.com).
    pub copilot_token: Option<String>,
    /// Unix seconds when copilot_token expires.
    pub copilot_expires_at: Option<i64>,
    /// Most recent github account login (for display only).
    pub github_login: Option<String>,
}

fn tokens_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("forge")
        .join("copilot_tokens.json")
}

pub fn load_tokens() -> Tokens {
    fs::read_to_string(tokens_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_tokens(t: &Tokens) {
    let path = tokens_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(s) = serde_json::to_string_pretty(t) {
        let _ = fs::write(&path, s);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
    }
}

pub fn clear_tokens() {
    save_tokens(&Tokens::default());
}

pub fn is_signed_in() -> bool {
    load_tokens().github_token.is_some()
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64
}

// ── Device flow ──

#[derive(Serialize, Clone, Debug)]
pub struct DeviceCodeResponse {
    pub user_code: String,
    pub verification_uri: String,
    pub device_code: String,
    pub interval: u64,
    pub expires_in: u64,
}

/// In-memory pending device code (one at a time is fine for this use case).
pub struct PendingAuth(pub Mutex<Option<String>>);
impl Default for PendingAuth {
    fn default() -> Self { Self(Mutex::new(None)) }
}

/// Step 1: request a device code from GitHub. User visits verification_uri
/// and enters user_code; we poll for completion.
pub fn request_device_code() -> Result<DeviceCodeResponse, String> {
    let resp = ureq::post("https://github.com/login/device/code")
        .set("Accept", "application/json")
        .send_form(&[("client_id", CLIENT_ID), ("scope", "read:user")])
        .map_err(|e| format!("device code request: {e}"))?;
    let v: Value = resp.into_json().map_err(|e| format!("device code parse: {e}"))?;
    Ok(DeviceCodeResponse {
        user_code: v["user_code"].as_str().ok_or("missing user_code")?.to_string(),
        verification_uri: v["verification_uri"].as_str().ok_or("missing verification_uri")?.to_string(),
        device_code: v["device_code"].as_str().ok_or("missing device_code")?.to_string(),
        interval: v["interval"].as_u64().unwrap_or(5),
        expires_in: v["expires_in"].as_u64().unwrap_or(900),
    })
}

pub enum PollResult {
    Pending,
    SlowDown,
    Authorized { github_token: String },
    Denied,
    Expired,
    Other(String),
}

/// Step 2: poll the access_token endpoint until user authorises or denies.
pub fn poll_device_code(device_code: &str) -> Result<PollResult, String> {
    let resp = ureq::post("https://github.com/login/oauth/access_token")
        .set("Accept", "application/json")
        .send_form(&[
            ("client_id", CLIENT_ID),
            ("device_code", device_code),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ])
        .map_err(|e| format!("poll: {e}"))?;
    let v: Value = resp.into_json().map_err(|e| format!("poll parse: {e}"))?;
    if let Some(tok) = v["access_token"].as_str() {
        return Ok(PollResult::Authorized { github_token: tok.to_string() });
    }
    match v["error"].as_str().unwrap_or("") {
        "authorization_pending" => Ok(PollResult::Pending),
        "slow_down" => Ok(PollResult::SlowDown),
        "access_denied" => Ok(PollResult::Denied),
        "expired_token" => Ok(PollResult::Expired),
        other => Ok(PollResult::Other(other.to_string())),
    }
}

/// Fetch the authenticated GitHub user's login (for display).
pub fn fetch_github_login(gh_token: &str) -> Result<String, String> {
    let resp = ureq::get("https://api.github.com/user")
        .set("Authorization", &format!("token {gh_token}"))
        .set("User-Agent", "GithubCopilot/1.155.0")
        .call()
        .map_err(|e| format!("github /user: {e}"))?;
    let v: Value = resp.into_json().map_err(|e| format!("github /user parse: {e}"))?;
    Ok(v["login"].as_str().unwrap_or("").to_string())
}

/// Persist the github_token after a successful device flow; also fetches
/// the login for display.
pub fn finalize_auth(github_token: String) -> Result<(), String> {
    let login = fetch_github_login(&github_token).ok();
    let tokens = Tokens {
        github_token: Some(github_token),
        copilot_token: None,
        copilot_expires_at: None,
        github_login: login,
    };
    save_tokens(&tokens);
    Ok(())
}

// ── Copilot token (short-lived) ──

#[derive(Serialize, Clone, Debug)]
pub struct CopilotModel {
    pub id: String,
    pub name: String,
    pub vendor: String,
}

/// Fetch the list of chat-capable models for the signed-in Copilot account.
/// Endpoint: https://api.githubcopilot.com/models
pub fn list_models() -> Result<Vec<CopilotModel>, String> {
    let token = get_copilot_token()?;
    let resp = ureq::get("https://api.githubcopilot.com/models")
        .set("Authorization", &format!("Bearer {token}"))
        .set("User-Agent", "GithubCopilot/1.155.0")
        .set("Editor-Version", "vscode/1.95.0")
        .set("Editor-Plugin-Version", "copilot-chat/0.22.0")
        .set("Copilot-Integration-Id", "vscode-chat")
        .call()
        .map_err(|e| format!("copilot /models: {e}"))?;
    let v: Value = resp.into_json().map_err(|e| format!("copilot /models parse: {e}"))?;
    let arr = v["data"].as_array().ok_or("no data array")?;
    let mut out = Vec::new();
    for m in arr {
        // Only keep chat models (skip embeddings, etc.).
        let caps = &m["capabilities"];
        if caps["type"].as_str() != Some("chat") {
            continue;
        }
        let id = match m["id"].as_str() {
            Some(s) => s.to_string(),
            None => continue,
        };
        let name = m["name"].as_str().unwrap_or(&id).to_string();
        let vendor = m["vendor"].as_str().unwrap_or("").to_string();
        out.push(CopilotModel { id, name, vendor });
    }
    Ok(out)
}

/// Return a valid copilot_token, refreshing from the github_token if expired.
pub fn get_copilot_token() -> Result<String, String> {
    let mut tokens = load_tokens();
    let gh = tokens.github_token.clone().ok_or("not signed in to copilot")?;
    let now = now_secs();
    if let (Some(t), Some(exp)) = (&tokens.copilot_token, tokens.copilot_expires_at) {
        if exp > now + 60 {
            return Ok(t.clone());
        }
    }
    let resp = ureq::get(COPILOT_TOKEN_URL)
        .set("Authorization", &format!("token {gh}"))
        .set("User-Agent", "GithubCopilot/1.155.0")
        .set("Editor-Version", "vscode/1.95.0")
        .set("Editor-Plugin-Version", "copilot-chat/0.22.0")
        .call()
        .map_err(|e| format!("copilot token: {e}"))?;
    let v: Value = resp.into_json().map_err(|e| format!("copilot token parse: {e}"))?;
    let token = v["token"].as_str().ok_or("no token field")?.to_string();
    let expires = v["expires_at"].as_i64().unwrap_or(now + 1500);
    tokens.copilot_token = Some(token.clone());
    tokens.copilot_expires_at = Some(expires);
    save_tokens(&tokens);
    Ok(token)
}
