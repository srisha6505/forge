//! OpenAI provider: validate API keys and list chat-capable models.
//!
//! Endpoint: GET <base_url>/v1/models with `Authorization: Bearer <key>`.
//! The same shape is reused by `openai_compat` for OpenRouter / Ollama /
//! LM Studio / llama-server / Jan / etc.
//!
//! Also exposes the `/v1/chat/completions` streaming + tool_calls helper
//! reused by `openai_compat` and `copilot` so all OpenAI-shaped providers
//! emit `InferenceEvent::ToolUse` correctly.

use std::io::{BufRead, BufReader};
use std::sync::mpsc;

use crate::llm::{
    ChatMessage, ChatRole, InferenceEvent, InferenceRequest, ProviderCapabilities, ToolCall,
};

const DEFAULT_BASE_URL: &str = "https://api.openai.com";
const TIMEOUT_SECS: u64 = 5;

/// Hardcoded capability table for well-known OpenAI models. The `/v1/models`
/// endpoint does not expose context window or tokenizer info, so we keep
/// this map and fall back to sensible defaults for everything else.
// HARDCODED: OpenAI does not publish context windows over the API.
fn caps_for_id(id: &str) -> ProviderCapabilities {
    let lower = id.to_ascii_lowercase();
    let (ctx, max_out, vision, tools) = if lower.starts_with("gpt-4o-mini") {
        (128_000, 16_384, true, true)
    } else if lower.starts_with("gpt-4o") {
        (128_000, 16_384, true, true)
    } else if lower.starts_with("gpt-4.1-mini") {
        (1_047_576, 32_768, true, true)
    } else if lower.starts_with("gpt-4.1") {
        (1_047_576, 32_768, true, true)
    } else if lower.starts_with("gpt-4-turbo") {
        (128_000, 4_096, true, true)
    } else if lower.starts_with("gpt-4") {
        (8_192, 4_096, false, true)
    } else if lower.starts_with("o1-mini") {
        (128_000, 65_536, false, false)
    } else if lower.starts_with("o1") {
        (200_000, 100_000, true, false)
    } else if lower.starts_with("o3") || lower.starts_with("o4") {
        (200_000, 100_000, true, true)
    } else if lower.starts_with("gpt-3.5") {
        (16_385, 4_096, false, true)
    } else {
        // Unknowns default to a conservative 128k.
        (128_000, 4_096, false, true)
    };

    ProviderCapabilities {
        context_window: ctx,
        max_output: max_out,
        tokenizer_kind: "tiktoken_o200k".into(),
        supports_caching: true,
        supports_tools: tools,
        supports_vision: vision,
    }
}

/// Crude filter: keep models whose ids look like chat-completion models.
/// OpenAI mixes embeddings, tts, dall-e, whisper, etc. into /v1/models.
fn is_chat_model(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    if lower.contains("embed")
        || lower.contains("whisper")
        || lower.contains("tts")
        || lower.contains("dall-e")
        || lower.contains("davinci")
        || lower.contains("babbage")
        || lower.contains("moderation")
        || lower.contains("realtime")
        || lower.contains("audio")
        || lower.contains("transcribe")
        || lower.contains("image")
    {
        return false;
    }
    lower.starts_with("gpt-")
        || lower.starts_with("o1")
        || lower.starts_with("o3")
        || lower.starts_with("o4")
        || lower.starts_with("chatgpt")
}

/// Run the GET /v1/models call and return parsed `ProviderModel` entries.
pub fn list_models(api_key: &str, base_url: Option<&str>) -> Result<Vec<crate::llm::ProviderModel>, String> {
    let base = base_url.unwrap_or(DEFAULT_BASE_URL).trim_end_matches('/');
    let url = format!("{base}/v1/models");

    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
        .build();

    let resp = agent
        .get(&url)
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Accept", "application/json")
        .call();

    let resp = match resp {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            let body = r.into_string().unwrap_or_default();
            return Err(format!("{code}: {}", body.chars().take(200).collect::<String>()));
        }
        Err(e) => return Err(format!("Network: {e}")),
    };

    let v: serde_json::Value = resp
        .into_json()
        .map_err(|e| format!("Parse: {e}"))?;

    let arr = v.get("data").and_then(|d| d.as_array()).ok_or("missing data array")?;
    let mut out = Vec::new();
    for m in arr {
        let id = match m.get("id").and_then(|s| s.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        if !is_chat_model(&id) {
            continue;
        }
        let caps = caps_for_id(&id);
        out.push(crate::llm::ProviderModel {
            id: id.clone(),
            display_name: id,
            capabilities: caps,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

// ── Chat completions (shared by openai / openai_compat / copilot) ──────

/// Configuration for a streaming OpenAI-compatible chat call.
pub struct ChatRequestConfig<'a> {
    pub url: &'a str,
    pub auth_bearer: &'a str,
    pub model: &'a str,
    /// Extra headers (Copilot uses Editor-Version, etc.). Empty for plain OpenAI.
    pub extra_headers: &'a [(&'a str, String)],
}

/// Translate Forge `ChatMessage`s into the OpenAI `messages[]` shape, including
/// `tool_calls` on assistant turns and `role: "tool"` with `tool_call_id`.
pub fn build_messages_json(messages: &[ChatMessage]) -> Vec<serde_json::Value> {
    let mut out = Vec::with_capacity(messages.len());
    for msg in messages {
        match msg.role {
            ChatRole::System => out.push(serde_json::json!({
                "role": "system",
                "content": msg.content,
            })),
            ChatRole::User => out.push(serde_json::json!({
                "role": "user",
                "content": msg.content,
            })),
            ChatRole::Assistant => {
                if !msg.tool_calls.is_empty() {
                    let tool_calls: Vec<serde_json::Value> = msg.tool_calls.iter().map(|tc| {
                        // OpenAI requires arguments as a JSON-encoded string.
                        let args_str = serde_json::to_string(&tc.arguments)
                            .unwrap_or_else(|_| "{}".into());
                        serde_json::json!({
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": args_str,
                            }
                        })
                    }).collect();
                    let mut obj = serde_json::Map::new();
                    obj.insert("role".into(), serde_json::Value::String("assistant".into()));
                    // OpenAI rejects null content alongside tool_calls only on some endpoints; "" is safe everywhere.
                    obj.insert(
                        "content".into(),
                        serde_json::Value::String(msg.content.clone()),
                    );
                    obj.insert("tool_calls".into(), serde_json::Value::Array(tool_calls));
                    out.push(serde_json::Value::Object(obj));
                } else {
                    out.push(serde_json::json!({
                        "role": "assistant",
                        "content": msg.content,
                    }));
                }
            }
            ChatRole::Tool => out.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": msg.tool_call_id.clone().unwrap_or_default(),
                "content": msg.content,
            })),
        }
    }
    out
}

/// Convert internal tool schemas (already in OpenAI `{type, function: {...}}`
/// shape per agent::tool_schemas) — we just pass them through, but accept any
/// shape with .function.
pub fn tools_for_request(tools: &[serde_json::Value]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .filter(|t| t.get("function").is_some())
        .cloned()
        .collect()
}

/// Run a streaming chat-completions call, dispatching `InferenceEvent`s on
/// `tx`. Always sends a `Done` event before returning. Tool calls are
/// accumulated across `delta.tool_calls[]` chunks and emitted as a single
/// `ToolUse` per call when the stream signals `finish_reason: "tool_calls"`
/// or the stream ends.
pub fn run_chat_stream(
    cfg: &ChatRequestConfig<'_>,
    req: &InferenceRequest,
    tx: &mpsc::Sender<InferenceEvent>,
) {
    let messages_json = build_messages_json(&req.messages);
    let api_tools = tools_for_request(&req.tools);

    let mut body = serde_json::json!({
        "model": cfg.model,
        "messages": messages_json,
        "stream": true,
    });
    if !api_tools.is_empty() {
        body["tools"] = serde_json::Value::Array(api_tools);
    }

    let mut request = ureq::post(cfg.url)
        .set("Authorization", &format!("Bearer {}", cfg.auth_bearer))
        .set("Content-Type", "application/json")
        .set("Accept", "text/event-stream");
    for (k, v) in cfg.extra_headers {
        request = request.set(k, v);
    }

    let resp = match request.send_json(body) {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            let body = r.into_string().unwrap_or_default();
            let _ = tx.send(InferenceEvent::Error(format!(
                "HTTP {code}: {}",
                body.chars().take(400).collect::<String>()
            )));
            let _ = tx.send(InferenceEvent::Done);
            return;
        }
        Err(e) => {
            let _ = tx.send(InferenceEvent::Error(format!("Request failed: {e}")));
            let _ = tx.send(InferenceEvent::Done);
            return;
        }
    };

    // Accumulator for tool_calls being built up across delta chunks.
    // OpenAI streams deltas indexed by position; we mirror that map.
    struct PartialTool {
        id: String,
        name: String,
        args: String,
    }
    let mut partials: std::collections::BTreeMap<u64, PartialTool> = Default::default();
    let mut emitted_any_tool = false;
    let mut emit_pending_tools = |partials: &mut std::collections::BTreeMap<u64, PartialTool>,
                                  emitted_any_tool: &mut bool| {
        for (idx, p) in std::mem::take(partials) {
            if p.name.is_empty() {
                continue;
            }
            // Synthesise an id when the server omitted one (some openai-compat
            // implementations skip it). Format documented in ai.md §2.2.
            let id = if p.id.is_empty() {
                synth_tool_id(&p.name, idx, &p.args)
            } else {
                p.id
            };
            let arguments: serde_json::Value = if p.args.trim().is_empty() {
                serde_json::Value::Object(Default::default())
            } else {
                serde_json::from_str(&p.args)
                    .unwrap_or(serde_json::Value::Object(Default::default()))
            };
            let _ = tx.send(InferenceEvent::ToolUse(ToolCall {
                id,
                name: p.name,
                arguments,
            }));
            *emitted_any_tool = true;
        }
    };

    let reader = BufReader::new(resp.into_reader());
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let Some(data) = line.strip_prefix("data:") else { continue };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let chunk: serde_json::Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let choice = match chunk.get("choices").and_then(|c| c.as_array()).and_then(|a| a.first()) {
            Some(c) => c,
            None => continue,
        };
        let delta = choice.get("delta").cloned().unwrap_or(serde_json::Value::Null);

        // Streamed text content.
        if let Some(text) = delta.get("content").and_then(|v| v.as_str()) {
            if !text.is_empty() {
                let _ = tx.send(InferenceEvent::Token(text.to_string()));
            }
        }

        // Streamed tool_calls deltas. OpenAI shape:
        //   delta.tool_calls = [{ index, id?, type?, function: { name?, arguments? } }, ...]
        if let Some(tcs) = delta.get("tool_calls").and_then(|v| v.as_array()) {
            for tc in tcs {
                let idx = tc.get("index").and_then(|n| n.as_u64()).unwrap_or(0);
                let entry = partials.entry(idx).or_insert_with(|| PartialTool {
                    id: String::new(),
                    name: String::new(),
                    args: String::new(),
                });
                if let Some(id) = tc.get("id").and_then(|s| s.as_str()) {
                    if !id.is_empty() {
                        entry.id = id.to_string();
                    }
                }
                if let Some(func) = tc.get("function") {
                    if let Some(name) = func.get("name").and_then(|s| s.as_str()) {
                        if !name.is_empty() {
                            entry.name.push_str(name);
                        }
                    }
                    if let Some(args) = func.get("arguments").and_then(|s| s.as_str()) {
                        entry.args.push_str(args);
                    }
                }
            }
        }

        // finish_reason: "tool_calls" indicates the model wants to dispatch
        // the accumulated calls. Some servers emit it on the final delta.
        let finish_reason = choice
            .get("finish_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if finish_reason == "tool_calls" {
            emit_pending_tools(&mut partials, &mut emitted_any_tool);
        }
    }

    // End-of-stream: flush any pending tool calls that weren't bookended by
    // an explicit finish_reason. ureq closing the body counts as stream end.
    emit_pending_tools(&mut partials, &mut emitted_any_tool);
    let _ = tx.send(InferenceEvent::Done);
    // emitted_any_tool is unused for now but kept for future telemetry.
    let _ = emitted_any_tool;
}

/// Deterministic synthesised tool-call id. Matches the `sha1(name+idx+args)[..8]`
/// recipe specified in ai.md §2.2 — but using a tiny non-crypto hash to avoid
/// pulling sha1 in. Stability over collision-resistance is what matters.
fn synth_tool_id(name: &str, idx: u64, args: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in name.bytes().chain(idx.to_le_bytes().iter().copied()).chain(args.bytes()) {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("call_{:016x}", h)
}

// ── Public spawn helper ───────────────────────────────────────────────

/// Spawn a background thread that serves chat requests against
/// <base_url>/v1/chat/completions.
pub fn spawn_thread(
    api_key: String,
    base_url: Option<String>,
    model: String,
) -> Result<crate::llm::InferenceHandle, String> {
    let (tx, rx) = mpsc::channel::<InferenceRequest>();
    let model_name = model.clone();
    let base = base_url
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
        .trim_end_matches('/')
        .to_string();
    // Allow callers to pass either a bare host or one already ending in /v1.
    let url = if base.ends_with("/v1") {
        format!("{base}/chat/completions")
    } else {
        format!("{base}/v1/chat/completions")
    };

    std::thread::Builder::new()
        .name("forge-openai".into())
        .spawn(move || {
            for req in rx.iter() {
                let cfg = ChatRequestConfig {
                    url: &url,
                    auth_bearer: &api_key,
                    model: &model,
                    extra_headers: &[],
                };
                run_chat_stream(&cfg, &req, &req.response_tx);
            }
        })
        .map_err(|e| format!("spawn openai thread: {e}"))?;

    Ok(crate::llm::InferenceHandle::from_sender(tx, model_name))
}
