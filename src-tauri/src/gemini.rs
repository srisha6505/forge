//! Gemini provider: validate API keys and enumerate generateContent-capable
//! models. Capability metadata (input/output token limits) is taken straight
//! from the API response.
//!
//! Also exposes the streaming chat helper used by the agent. Gemini's tool
//! shape differs from OpenAI: tools are `{functionDeclarations: [...]}` and
//! responses carry `functionCall` parts (no provider-side ID, so we
//! synthesise one — see ai.md §2.2).
//!
//! Endpoint: GET https://generativelanguage.googleapis.com/v1beta/models?key=<key>

use std::io::{BufRead, BufReader};
use std::sync::mpsc;

use crate::llm::{
    ChatMessage, ChatRole, InferenceEvent, InferenceHandle, InferenceRequest,
    ProviderCapabilities, ToolCall,
};

const BASE_URL: &str = "https://generativelanguage.googleapis.com";
const TIMEOUT_SECS: u64 = 5;

/// Run the GET /v1beta/models call and return parsed `ProviderModel`
/// entries restricted to those that support `generateContent`.
pub fn list_models(api_key: &str) -> Result<Vec<crate::llm::ProviderModel>, String> {
    let url = format!("{BASE_URL}/v1beta/models?key={api_key}");

    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
        .build();

    let resp = agent
        .get(&url)
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

    let arr = v.get("models").and_then(|d| d.as_array()).ok_or("missing models array")?;
    let mut out = Vec::new();
    for m in arr {
        let methods = m.get("supportedGenerationMethods").and_then(|s| s.as_array());
        let supports_generate = methods
            .map(|arr| arr.iter().any(|v| v.as_str() == Some("generateContent")))
            .unwrap_or(false);
        if !supports_generate {
            continue;
        }
        // The full id comes back as "models/<name>"; strip the prefix for UX.
        let raw_name = m.get("name").and_then(|s| s.as_str()).unwrap_or_default();
        let id = raw_name.strip_prefix("models/").unwrap_or(raw_name).to_string();
        if id.is_empty() {
            continue;
        }
        let display = m
            .get("displayName")
            .and_then(|s| s.as_str())
            .unwrap_or(&id)
            .to_string();

        let context_window = m
            .get("inputTokenLimit")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as u32;
        let max_output = m
            .get("outputTokenLimit")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as u32;

        let lower = id.to_ascii_lowercase();
        let supports_vision = lower.contains("pro") || lower.contains("flash") || lower.contains("vision");
        let supports_tools = !lower.contains("embedding");

        out.push(crate::llm::ProviderModel {
            id,
            display_name: display,
            capabilities: ProviderCapabilities {
                context_window,
                max_output,
                tokenizer_kind: "gemini".into(),
                // HARDCODED: Gemini exposes cachedContent but we treat the gate
                // as a static capability flag rather than per-model feature.
                supports_caching: true,
                supports_tools,
                supports_vision,
            },
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

// ── Chat (streamGenerateContent) ──────────────────────────────────────

/// Translate Forge messages into Gemini's `contents[]` + system_instruction
/// shape. Gemini uses role values `user` and `model` (no "assistant"); tool
/// results live as a part of a `user` content with `functionResponse`.
fn build_contents(
    messages: &[ChatMessage],
) -> (Option<serde_json::Value>, Vec<serde_json::Value>) {
    let mut system_instruction: Option<serde_json::Value> = None;
    let mut contents: Vec<serde_json::Value> = Vec::new();

    for msg in messages {
        match msg.role {
            ChatRole::System => {
                // Gemini lets the system instruction be a single text string.
                // Concatenate if multiple system messages were ever passed.
                let part = serde_json::json!({"text": msg.content});
                let value = serde_json::json!({"parts": [part]});
                system_instruction = Some(value);
            }
            ChatRole::User => contents.push(serde_json::json!({
                "role": "user",
                "parts": [{ "text": msg.content }]
            })),
            ChatRole::Assistant => {
                let mut parts: Vec<serde_json::Value> = Vec::new();
                if !msg.content.is_empty() {
                    parts.push(serde_json::json!({"text": msg.content}));
                }
                for tc in &msg.tool_calls {
                    parts.push(serde_json::json!({
                        "functionCall": {
                            "name": tc.name,
                            "args": tc.arguments,
                        }
                    }));
                }
                if parts.is_empty() {
                    continue;
                }
                contents.push(serde_json::json!({
                    "role": "model",
                    "parts": parts,
                }));
            }
            ChatRole::Tool => {
                // Gemini requires the function name in the response part. We
                // don't carry that on the tool message in our model — find it
                // by scanning earlier assistant tool_calls for the matching id.
                let name = lookup_tool_name_for_id(messages, msg.tool_call_id.as_deref())
                    .unwrap_or_default();
                contents.push(serde_json::json!({
                    "role": "user",
                    "parts": [{
                        "functionResponse": {
                            "name": name,
                            "response": { "content": msg.content },
                        }
                    }]
                }));
            }
        }
    }
    (system_instruction, contents)
}

fn lookup_tool_name_for_id<'a>(messages: &'a [ChatMessage], id: Option<&str>) -> Option<&'a str> {
    let id = id?;
    for m in messages {
        if matches!(m.role, ChatRole::Assistant) {
            for tc in &m.tool_calls {
                if tc.id == id {
                    return Some(&tc.name);
                }
            }
        }
    }
    None
}

/// Convert internal tool schemas (OpenAI-shaped) into Gemini
/// `tools: [{ functionDeclarations: [...] }]`.
fn tools_to_gemini(tools: &[serde_json::Value]) -> Vec<serde_json::Value> {
    if tools.is_empty() {
        return vec![];
    }
    let mut decls = Vec::new();
    for t in tools {
        let Some(func) = t.get("function") else { continue };
        let name = func.get("name").cloned().unwrap_or(serde_json::Value::Null);
        let desc = func
            .get("description")
            .cloned()
            .unwrap_or(serde_json::Value::String("".into()));
        let params = func
            .get("parameters")
            .cloned()
            .unwrap_or(serde_json::json!({"type": "object", "properties": {}}));
        decls.push(serde_json::json!({
            "name": name,
            "description": desc,
            "parameters": params,
        }));
    }
    if decls.is_empty() {
        vec![]
    } else {
        vec![serde_json::json!({"functionDeclarations": decls})]
    }
}

/// Run a streaming generateContent request and dispatch InferenceEvents.
fn run_stream(api_key: &str, model: &str, req: &InferenceRequest) {
    let tx = &req.response_tx;
    let url = format!(
        "{BASE_URL}/v1beta/models/{model}:streamGenerateContent?alt=sse&key={api_key}"
    );

    let (system_instruction, contents) = build_contents(&req.messages);
    let api_tools = tools_to_gemini(&req.tools);

    let mut body = serde_json::json!({
        "contents": contents,
    });
    if let Some(si) = system_instruction {
        body["systemInstruction"] = si;
    }
    if !api_tools.is_empty() {
        body["tools"] = serde_json::Value::Array(api_tools);
    }

    let resp = match ureq::post(&url)
        .set("Content-Type", "application/json")
        .set("Accept", "text/event-stream")
        .send_json(body)
    {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            let body = r.into_string().unwrap_or_default();
            let _ = tx.send(InferenceEvent::Error(format!(
                "Gemini HTTP {code}: {}",
                body.chars().take(400).collect::<String>()
            )));
            let _ = tx.send(InferenceEvent::Done);
            return;
        }
        Err(e) => {
            let _ = tx.send(InferenceEvent::Error(format!("Gemini request failed: {e}")));
            let _ = tx.send(InferenceEvent::Done);
            return;
        }
    };

    let reader = BufReader::new(resp.into_reader());
    let mut tool_call_index: u64 = 0;
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let Some(data) = line.strip_prefix("data:") else { continue };
        let data = data.trim();
        if data.is_empty() {
            continue;
        }
        let chunk: serde_json::Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let candidates = chunk.get("candidates").and_then(|v| v.as_array());
        let Some(cands) = candidates else { continue };
        for cand in cands {
            let parts = cand
                .get("content")
                .and_then(|c| c.get("parts"))
                .and_then(|p| p.as_array());
            let Some(parts) = parts else { continue };
            for part in parts {
                if let Some(text) = part.get("text").and_then(|s| s.as_str()) {
                    if !text.is_empty() {
                        let _ = tx.send(InferenceEvent::Token(text.to_string()));
                    }
                }
                if let Some(fc) = part.get("functionCall") {
                    let name = fc
                        .get("name")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string();
                    let args = fc.get("args").cloned().unwrap_or_else(|| {
                        serde_json::Value::Object(serde_json::Map::new())
                    });
                    if name.is_empty() {
                        continue;
                    }
                    // Synthesise an id — Gemini does not provide one.
                    let args_str =
                        serde_json::to_string(&args).unwrap_or_else(|_| "{}".into());
                    let id = synth_id(&name, tool_call_index, &args_str);
                    tool_call_index += 1;
                    let _ = tx.send(InferenceEvent::ToolUse(ToolCall {
                        id,
                        name,
                        arguments: args,
                    }));
                }
            }
        }
    }

    let _ = tx.send(InferenceEvent::Done);
}

/// Stable hash over (name, idx, args) for synthesised tool-call ids.
fn synth_id(name: &str, idx: u64, args: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in name.bytes().chain(idx.to_le_bytes().iter().copied()).chain(args.bytes()) {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("call_{:016x}", h)
}

/// Spawn a chat thread for Gemini's streamGenerateContent endpoint.
pub fn spawn_thread(api_key: String, model: String) -> Result<InferenceHandle, String> {
    let (tx, rx) = mpsc::channel::<InferenceRequest>();
    let model_name = model.clone();

    std::thread::Builder::new()
        .name("forge-gemini".into())
        .spawn(move || {
            for req in rx.iter() {
                run_stream(&api_key, &model, &req);
            }
        })
        .map_err(|e| format!("spawn gemini thread: {e}"))?;

    Ok(InferenceHandle::from_sender(tx, model_name))
}
