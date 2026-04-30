//! Generic OpenAI-compatible provider — covers OpenRouter, Ollama,
//! LM Studio, llama-server, Jan, and similar local servers exposing a
//! `/v1/models` + `/v1/chat/completions` surface.
//!
//! The shape is identical to `openai.rs`, but we do NOT filter the model
//! list (local servers expose exactly what the user has loaded, often with
//! quantisation suffixes) and we use a conservative capability default.

use crate::llm::ProviderCapabilities;

const TIMEOUT_SECS: u64 = 5;

/// Run the GET <base_url>/v1/models call against an OpenAI-compatible
/// endpoint and return whatever it lists with conservative capabilities.
pub fn list_models(api_key: &str, base_url: &str) -> Result<Vec<crate::llm::ProviderModel>, String> {
    let base = base_url.trim_end_matches('/');
    // Allow callers to pass either the bare host or a path that already
    // contains `/v1`; do not double-append.
    let url = if base.ends_with("/v1") {
        format!("{base}/models")
    } else {
        format!("{base}/v1/models")
    };

    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
        .build();

    let mut req = agent.get(&url).set("Accept", "application/json");
    if !api_key.is_empty() {
        req = req.set("Authorization", &format!("Bearer {api_key}"));
    }
    let resp = req.call();

    let resp = match resp {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            let body = r.into_string().unwrap_or_default();
            return Err(format!("{code}: {}", body.chars().take(200).collect::<String>()));
        }
        Err(e) => return Err(format!("Network: {e}")),
    };

    let v: serde_json::Value = resp.into_json().map_err(|e| format!("Parse: {e}"))?;

    // OpenRouter exposes richer per-model metadata ({context_length, ...});
    // local servers expose only ids. Read both shapes opportunistically.
    let arr = v
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or("missing data array")?;

    let mut out = Vec::new();
    for m in arr {
        let id = match m.get("id").and_then(|s| s.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };

        // OpenRouter style: {context_length, top_provider.max_completion_tokens, ...}
        let ctx = m
            .get("context_length")
            .and_then(|n| n.as_u64())
            .unwrap_or(8_192) as u32;
        let max_out = m
            .get("top_provider")
            .and_then(|t| t.get("max_completion_tokens"))
            .and_then(|n| n.as_u64())
            .unwrap_or((ctx as u64).min(4_096)) as u32;

        let display = m
            .get("name")
            .and_then(|s| s.as_str())
            .unwrap_or(&id)
            .to_string();

        out.push(crate::llm::ProviderModel {
            id: id.clone(),
            display_name: display,
            // HARDCODED: capability flags default conservatively because most
            // OpenAI-compat servers (Ollama / LM Studio) don't report them.
            capabilities: ProviderCapabilities {
                context_window: ctx,
                max_output: max_out,
                tokenizer_kind: "unknown".into(),
                supports_caching: false,
                supports_tools: true,
                supports_vision: false,
            },
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}
