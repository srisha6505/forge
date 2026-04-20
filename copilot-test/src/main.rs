use axum::{extract::State, routing::{get, post}, Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, sync::Arc, time::{SystemTime, UNIX_EPOCH}};
use tokio::sync::Mutex;
use tower_http::services::ServeDir;

const CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
const TOKEN_FILE: &str = "tokens.json";

#[derive(Clone, Serialize, Deserialize, Default)]
struct Tokens {
    github_token: Option<String>,
    copilot_token: Option<String>,
    copilot_expires_at: Option<i64>,
}

#[derive(Clone)]
struct AppState {
    tokens: Arc<Mutex<Tokens>>,
    device_code: Arc<Mutex<Option<String>>>,
    http: reqwest::Client,
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

fn load_tokens() -> Tokens {
    fs::read_to_string(TOKEN_FILE)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_tokens(t: &Tokens) {
    let _ = fs::write(TOKEN_FILE, serde_json::to_string_pretty(t).unwrap());
}

#[tokio::main]
async fn main() {
    let state = AppState {
        tokens: Arc::new(Mutex::new(load_tokens())),
        device_code: Arc::new(Mutex::new(None)),
        http: reqwest::Client::new(),
    };

    let app = Router::new()
        .route("/auth/start", post(auth_start))
        .route("/auth/poll", post(auth_poll))
        .route("/auth/status", get(auth_status))
        .route("/auth/logout", post(auth_logout))
        .route("/models", get(list_models))
        .route("/chat", post(chat))
        .fallback_service(ServeDir::new("static"))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("→ http://127.0.0.1:3000");
    axum::serve(listener, app).await.unwrap();
}

async fn auth_start(State(s): State<AppState>) -> Json<Value> {
    let res: Value = s.http
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .form(&[("client_id", CLIENT_ID), ("scope", "read:user")])
        .send().await.unwrap()
        .json().await.unwrap();

    if let Some(code) = res["device_code"].as_str() {
        *s.device_code.lock().await = Some(code.to_string());
    }

    Json(json!({
        "user_code": res["user_code"],
        "verification_uri": res["verification_uri"],
        "interval": res["interval"],
        "raw": res,
    }))
}

async fn auth_poll(State(s): State<AppState>) -> Json<Value> {
    let Some(code) = s.device_code.lock().await.clone() else {
        return Json(json!({"status": "no_device_code"}));
    };

    let res: Value = s.http
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&[
            ("client_id", CLIENT_ID),
            ("device_code", code.as_str()),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ])
        .send().await.unwrap()
        .json().await.unwrap();

    if let Some(token) = res["access_token"].as_str() {
        let mut t = s.tokens.lock().await;
        t.github_token = Some(token.to_string());
        t.copilot_token = None;
        t.copilot_expires_at = None;
        save_tokens(&t);
        return Json(json!({"status": "ok"}));
    }

    Json(json!({"status": res["error"].as_str().unwrap_or("pending")}))
}

async fn auth_status(State(s): State<AppState>) -> Json<Value> {
    let t = s.tokens.lock().await;
    Json(json!({"authenticated": t.github_token.is_some()}))
}

async fn auth_logout(State(s): State<AppState>) -> Json<Value> {
    let mut t = s.tokens.lock().await;
    *t = Tokens::default();
    save_tokens(&t);
    Json(json!({"ok": true}))
}

async fn get_copilot_token(s: &AppState) -> Result<String, String> {
    let now = now_secs();
    {
        let t = s.tokens.lock().await;
        if let (Some(ct), Some(exp)) = (&t.copilot_token, t.copilot_expires_at) {
            if exp > now + 60 {
                return Ok(ct.clone());
            }
        }
    }

    let gh = s.tokens.lock().await.github_token.clone()
        .ok_or("not authenticated")?;

    let res = s.http
        .get("https://api.github.com/copilot_internal/v2/token")
        .header("Authorization", format!("token {}", gh))
        .header("User-Agent", "GithubCopilot/1.155.0")
        .header("Editor-Version", "vscode/1.95.0")
        .header("Editor-Plugin-Version", "copilot-chat/0.22.0")
        .send().await.map_err(|e| e.to_string())?;

    let status = res.status();
    let body: Value = res.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("copilot token {}: {}", status, body));
    }

    let token = body["token"].as_str().ok_or("no token field")?.to_string();
    let expires = body["expires_at"].as_i64().unwrap_or(now + 1500);

    let mut t = s.tokens.lock().await;
    t.copilot_token = Some(token.clone());
    t.copilot_expires_at = Some(expires);
    save_tokens(&t);

    Ok(token)
}

async fn list_models(State(s): State<AppState>) -> Json<Value> {
    let token = match get_copilot_token(&s).await {
        Ok(t) => t,
        Err(e) => return Json(json!({"error": e})),
    };
    let res = s.http
        .get("https://api.githubcopilot.com/models")
        .header("Authorization", format!("Bearer {}", token))
        .header("Editor-Version", "vscode/1.95.0")
        .header("Editor-Plugin-Version", "copilot-chat/0.22.0")
        .header("Copilot-Integration-Id", "vscode-chat")
        .send().await;
    match res {
        Ok(r) => {
            let status = r.status();
            let v: Value = r.json().await.unwrap_or(json!({"parse_error": true}));
            if !status.is_success() {
                return Json(json!({"error": format!("{}", status), "body": v}));
            }
            Json(v)
        }
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

#[derive(Deserialize)]
struct ChatReq {
    message: String,
    #[serde(default)]
    model: Option<String>,
}

async fn chat(State(s): State<AppState>, Json(req): Json<ChatReq>) -> Json<Value> {
    let token = match get_copilot_token(&s).await {
        Ok(t) => t,
        Err(e) => return Json(json!({"error": e})),
    };

    let model = req.model.unwrap_or_else(|| "gpt-4o".to_string());
    let body = json!({
        "model": model,
        "messages": [{"role": "user", "content": req.message}],
        "stream": true,
    });

    let res = s.http
        .post("https://api.githubcopilot.com/chat/completions")
        .header("Authorization", format!("Bearer {}", token))
        .header("Editor-Version", "vscode/1.95.0")
        .header("Editor-Plugin-Version", "copilot-chat/0.22.0")
        .header("Copilot-Integration-Id", "vscode-chat")
        .header("OpenAI-Intent", "conversation-panel")
        .header("Content-Type", "application/json")
        .header("Accept", "text/event-stream")
        .json(&body)
        .send().await;

    match res {
        Ok(r) => {
            let status = r.status();
            let text = r.text().await.unwrap_or_default();
            if !status.is_success() {
                eprintln!("[chat] model={} status={} body={}", model, status, text);
                let body: Value = serde_json::from_str(&text).unwrap_or(Value::String(text.clone()));
                return Json(json!({"error": format!("{}", status), "body": body, "model": model}));
            }
            let mut reply = String::new();
            for line in text.lines() {
                let Some(data) = line.strip_prefix("data:") else { continue };
                let data = data.trim();
                if data == "[DONE]" || data.is_empty() { continue }
                let Ok(chunk) = serde_json::from_str::<Value>(data) else { continue };
                if let Some(c) = chunk["choices"][0]["delta"]["content"].as_str() {
                    reply.push_str(c);
                }
            }
            if reply.is_empty() {
                eprintln!("[chat] model={} empty reply, raw: {}", model, text);
                return Json(json!({"error": "empty reply", "body": text, "model": model}));
            }
            Json(json!({"reply": reply}))
        }
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}
