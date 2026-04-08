//! Inference server: loads a GGUF model on a dedicated OS thread and serves
//! generation requests via channels.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use serde::{Deserialize, Serialize};

// ── Public types ──

/// Role in a chat conversation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

/// A single tool call produced by the model.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// A message in the conversation.
#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    /// For assistant messages that request tool calls.
    pub tool_calls: Vec<ToolCall>,
    /// For tool-role messages: which tool_call this responds to.
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: ChatRole::System, content: content.into(), tool_calls: vec![], tool_call_id: None }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: ChatRole::User, content: content.into(), tool_calls: vec![], tool_call_id: None }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: ChatRole::Assistant, content: content.into(), tool_calls: vec![], tool_call_id: None }
    }
    pub fn assistant_with_tool_calls(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self { role: ChatRole::Assistant, content: content.into(), tool_calls, tool_call_id: None }
    }
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self { role: ChatRole::Tool, content: content.into(), tool_calls: vec![], tool_call_id: Some(tool_call_id.into()) }
    }
}

/// A generation request sent from the UI thread to the inference thread.
pub struct InferenceRequest {
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<serde_json::Value>,
    pub response_tx: mpsc::Sender<InferenceEvent>,
}

/// Events streamed back from the inference thread.
#[derive(Clone, Debug)]
pub enum InferenceEvent {
    /// A chunk of generated text.
    Token(String),
    /// A thinking/reasoning block from the model.
    Thinking(String),
    /// Model requested a tool call (parsed from accumulated output).
    ToolUse(ToolCall),
    /// Generation finished.
    Done,
    /// An error occurred.
    Error(String),
}

/// Thread-safe handle for sending requests to the inference thread.
#[derive(Clone)]
pub struct InferenceHandle {
    tx: mpsc::Sender<InferenceRequest>,
    pub model_name: String,
}

impl InferenceHandle {
    /// Send a generation request. Returns a receiver for streaming events.
    pub fn generate(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<serde_json::Value>,
    ) -> mpsc::Receiver<InferenceEvent> {
        let (response_tx, response_rx) = mpsc::channel();
        let req = InferenceRequest { messages, tools, response_tx: response_tx.clone() };
        if self.tx.send(req).is_err() {
            let _ = response_tx.send(InferenceEvent::Error("Inference thread died".into()));
        }
        response_rx
    }
}

// ── Inference thread ──

/// Spawn a dedicated OS thread that owns the LlamaContext and processes
/// requests sequentially. Returns a handle for sending requests.
pub fn spawn_inference_thread(
    model_path: &Path,
    n_gpu_layers: u32,
    n_ctx: u32,
) -> Result<InferenceHandle, String> {
    use llama_cpp_2::context::params::LlamaContextParams;
    use llama_cpp_2::llama_backend::LlamaBackend;
    use llama_cpp_2::model::params::LlamaModelParams;
    use llama_cpp_2::model::LlamaModel;

    // Validate model file exists before spawning thread.
    if !model_path.exists() {
        return Err(format!("Model file not found: {}", model_path.display()));
    }

    let model_name = model_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let (tx, rx) = mpsc::channel::<InferenceRequest>();
    let path = model_path.to_path_buf();

    std::thread::Builder::new()
        .name("forge-inference".into())
        .spawn(move || {
            inference_loop(path, n_gpu_layers, n_ctx, rx);
        })
        .map_err(|e| format!("Failed to spawn inference thread: {e}"))?;

    Ok(InferenceHandle { tx, model_name })
}

fn inference_loop(
    model_path: PathBuf,
    n_gpu_layers: u32,
    n_ctx: u32,
    rx: mpsc::Receiver<InferenceRequest>,
) {
    use llama_cpp_2::context::params::LlamaContextParams;
    use llama_cpp_2::llama_backend::LlamaBackend;
    use llama_cpp_2::llama_batch::LlamaBatch;
    use llama_cpp_2::model::params::LlamaModelParams;
    use llama_cpp_2::model::{AddBos, LlamaModel, Special};
    use llama_cpp_2::sampling::LlamaSampler;

    // Initialize backend.
    let backend = match LlamaBackend::init() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[forge-llm] Backend init failed: {e}");
            // Drain requests with error.
            for req in rx.iter() {
                let _ = req.response_tx.send(InferenceEvent::Error(format!("Backend init failed: {e}")));
            }
            return;
        }
    };

    // Load model.
    let model_params = LlamaModelParams::default().with_n_gpu_layers(n_gpu_layers);
    let model = match LlamaModel::load_from_file(&backend, &model_path, &model_params) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[forge-llm] Model load failed: {e}");
            for req in rx.iter() {
                let _ = req.response_tx.send(InferenceEvent::Error(format!("Model load failed: {e}")));
            }
            return;
        }
    };

    eprintln!("[forge-llm] Model loaded: {}", model_path.display());

    // Create context.
    let ctx_params = LlamaContextParams::default().with_n_ctx(std::num::NonZero::new(n_ctx));
    let mut ctx = match model.new_context(&backend, ctx_params) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[forge-llm] Context creation failed: {e}");
            for req in rx.iter() {
                let _ = req.response_tx.send(InferenceEvent::Error(format!("Context failed: {e}")));
            }
            return;
        }
    };

    // Process requests.
    for req in rx.iter() {
        process_request(&model, &mut ctx, &req);
    }

    eprintln!("[forge-llm] Inference thread exiting.");
}

fn process_request(
    model: &llama_cpp_2::model::LlamaModel,
    ctx: &mut llama_cpp_2::context::LlamaContext,
    req: &InferenceRequest,
) {
    use llama_cpp_2::llama_batch::LlamaBatch;
    use llama_cpp_2::model::{AddBos, Special};
    use llama_cpp_2::sampling::LlamaSampler;

    let tx = &req.response_tx;

    // Format messages using the model's chat template + tool schemas.
    let prompt = match format_prompt(model, &req.messages, &req.tools) {
        Ok(p) => p,
        Err(e) => {
            let _ = tx.send(InferenceEvent::Error(format!("Prompt formatting failed: {e}")));
            let _ = tx.send(InferenceEvent::Done);
            return;
        }
    };

    // Tokenize.
    let tokens = match model.str_to_token(&prompt, AddBos::Always) {
        Ok(t) => t,
        Err(e) => {
            let _ = tx.send(InferenceEvent::Error(format!("Tokenization failed: {e}")));
            let _ = tx.send(InferenceEvent::Done);
            return;
        }
    };

    // Clear KV cache for fresh generation.
    ctx.clear_kv_cache();

    let n_ctx = ctx.n_ctx() as usize;
    if tokens.len() >= n_ctx {
        let _ = tx.send(InferenceEvent::Error(
            format!("Prompt too long ({} tokens, context is {})", tokens.len(), n_ctx)
        ));
        let _ = tx.send(InferenceEvent::Done);
        return;
    }

    // Feed prompt tokens.
    let mut batch = LlamaBatch::new(512, 1);
    let last_idx = tokens.len() - 1;
    for (i, &token) in tokens.iter().enumerate() {
        let is_last = i == last_idx;
        if let Err(e) = batch.add(token, i as i32, &[0], is_last) {
            let _ = tx.send(InferenceEvent::Error(format!("Batch add failed: {e}")));
            let _ = tx.send(InferenceEvent::Done);
            return;
        }

        // Flush batch when full.
        if batch.n_tokens() >= 512 || is_last {
            if let Err(e) = ctx.decode(&mut batch) {
                let _ = tx.send(InferenceEvent::Error(format!("Decode failed: {e}")));
                let _ = tx.send(InferenceEvent::Done);
                return;
            }
            batch.clear();
        }
    }

    // Generate tokens.
    // Gemma 4 recommended: temp=1.0, top_p=0.95, top_k=64
    // dist() at the end picks a token from filtered candidates.
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as u32)
        .unwrap_or(42);
    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::temp(1.0),
        LlamaSampler::top_k(64),
        LlamaSampler::top_p(0.95, 1),
        LlamaSampler::dist(seed),
    ]);

    let mut full_output = String::new(); // accumulate everything for tool call detection
    let mut n_decoded = 0usize;
    let max_gen = (n_ctx - tokens.len()).min(4096);
    let mut pos = tokens.len() as i32;
    let mut in_thinking = false;
    let mut thinking_buf = String::new();
    let mut text_buf = String::new(); // pending text not yet sent to UI

    loop {
        if n_decoded >= max_gen {
            break;
        }

        let token = sampler.sample(ctx, -1);
        sampler.accept(token);

        if model.is_eog_token(token) {
            break;
        }

        let piece = model.token_to_str(token, Special::Tokenize)
            .unwrap_or_default();

        full_output.push_str(&piece);

        if in_thinking {
            thinking_buf.push_str(&piece);
            // Check if thinking block ended.
            if let Some(end_pos) = thinking_buf.find("</channel>") {
                let thought = thinking_buf[..end_pos].to_string();
                let remainder = thinking_buf[end_pos + "</channel>".len()..].to_string();
                if !thought.trim().is_empty() {
                    let _ = tx.send(InferenceEvent::Thinking(thought));
                }
                thinking_buf.clear();
                text_buf.push_str(&remainder);
                in_thinking = false;
            }
        } else {
            text_buf.push_str(&piece);

            // Check if a thinking block starts.
            if let Some(start_pos) = text_buf.find("<channel>") {
                // Flush text before the tag.
                let before = text_buf[..start_pos].to_string();
                if !before.trim().is_empty() {
                    let _ = tx.send(InferenceEvent::Token(before));
                }
                // Move remainder into thinking buffer.
                thinking_buf = text_buf[start_pos + "<channel>".len()..].to_string();
                text_buf.clear();
                in_thinking = true;
            } else if text_buf.contains('<') && !text_buf.contains('>') {
                // Might be start of a tag, hold back.
            } else {
                // Check for tool call in full output.
                if let Some(tool_call) = try_parse_tool_call(&full_output) {
                    let pre = extract_pre_tool_text(&full_output);
                    // Send any unsent text before tool call.
                    if !text_buf.trim().is_empty() && !text_buf.contains("<tool_call>") {
                        let _ = tx.send(InferenceEvent::Token(text_buf.clone()));
                    }
                    let _ = tx.send(InferenceEvent::ToolUse(tool_call));
                    let _ = tx.send(InferenceEvent::Done);
                    return;
                }

                // Flush text to UI (but hold back if partial tag/tool call).
                if !might_be_tool_call_start(&text_buf) {
                    if !text_buf.is_empty() {
                        let _ = tx.send(InferenceEvent::Token(text_buf.clone()));
                        text_buf.clear();
                    }
                }
            }
        }

        // Prepare next token.
        batch.clear();
        if let Err(e) = batch.add(token, pos, &[0], true) {
            let _ = tx.send(InferenceEvent::Error(format!("Batch add failed: {e}")));
            break;
        }
        if let Err(e) = ctx.decode(&mut batch) {
            let _ = tx.send(InferenceEvent::Error(format!("Decode failed: {e}")));
            break;
        }

        pos += 1;
        n_decoded += 1;
    }

    // Try to parse tool call from full output one final time.
    if let Some(tool_call) = try_parse_tool_call(&full_output) {
        if !text_buf.trim().is_empty() && !text_buf.contains("<tool_call>") {
            let _ = tx.send(InferenceEvent::Token(text_buf));
        }
        let _ = tx.send(InferenceEvent::ToolUse(tool_call));
        let _ = tx.send(InferenceEvent::Done);
        return;
    }

    // Flush remaining text.
    if !text_buf.is_empty() {
        let _ = tx.send(InferenceEvent::Token(text_buf));
    }
    if in_thinking && !thinking_buf.is_empty() {
        let _ = tx.send(InferenceEvent::Thinking(thinking_buf));
    }
    let _ = tx.send(InferenceEvent::Done);
}

// ── Prompt formatting ──

fn format_prompt(
    model: &llama_cpp_2::model::LlamaModel,
    messages: &[ChatMessage],
    tools: &[serde_json::Value],
) -> Result<String, String> {
    // Build messages in the format expected by apply_chat_template.
    // Each message is a JSON object: {"role": "...", "content": "..."}
    let mut chat_msgs: Vec<llama_cpp_2::model::LlamaChatMessage> = Vec::new();

    for msg in messages {
        let role = match msg.role {
            ChatRole::System => "system",
            ChatRole::User => "user",
            ChatRole::Assistant => "assistant",
            ChatRole::Tool => "tool",
        };
        // For tool results, prepend the tool_call_id for context.
        let content = if msg.role.is_tool() && msg.tool_call_id.is_some() {
            format!("[tool_call_id: {}]\n{}", msg.tool_call_id.as_deref().unwrap_or(""), msg.content)
        } else if !msg.tool_calls.is_empty() {
            // Assistant message with tool calls: append tool call JSON.
            let tc_json = serde_json::to_string(&msg.tool_calls).unwrap_or_default();
            if msg.content.is_empty() {
                tc_json
            } else {
                format!("{}\n{}", msg.content, tc_json)
            }
        } else {
            msg.content.clone()
        };
        match llama_cpp_2::model::LlamaChatMessage::new(role.to_string(), content) {
            Ok(m) => chat_msgs.push(m),
            Err(e) => return Err(format!("Bad chat message: {e}")),
        }
    }

    // Get the model's chat template.
    let tmpl = model.chat_template(None)
        .map_err(|e| format!("No chat template in model: {e}"))?;

    // Try with tools first, fall back to without.
    if !tools.is_empty() {
        let tools_json = serde_json::to_string(tools).unwrap_or("[]".into());
        match model.apply_chat_template_with_tools_oaicompat(
            &tmpl, &chat_msgs, Some(&tools_json), None, true,
        ) {
            Ok(result) => {
                eprintln!("[forge-llm] Using native tool template");
                return Ok(result.prompt);
            }
            Err(e) => {
                eprintln!("[forge-llm] Native tool template failed ({e}), injecting tools into system prompt");
                // Inject tools into system prompt so the model knows about them.
                return format_prompt_with_injected_tools(model, &tmpl, messages, tools);
            }
        }
    }

    // No tools, plain chat template.
    model
        .apply_chat_template(&tmpl, &chat_msgs, true)
        .map_err(|e| format!("Chat template failed: {e}"))
}

/// Fallback: inject tool definitions into the system prompt when the model's
/// chat template doesn't natively support tools.
fn format_prompt_with_injected_tools(
    model: &llama_cpp_2::model::LlamaModel,
    tmpl: &llama_cpp_2::model::LlamaChatTemplate,
    messages: &[ChatMessage],
    tools: &[serde_json::Value],
) -> Result<String, String> {
    // Build a tool description block for the system prompt with few-shot examples.
    let mut tool_desc = String::from(
        "\n\n# TOOLS\n\
         You have tools to search and read the user's note vault. When the user asks about their notes, you MUST call a tool.\n\
         To call a tool, output ONLY this exact format with nothing else before or after:\n\
         <tool_call>\n{\"name\": \"tool_name\", \"arguments\": {\"key\": \"value\"}}\n</tool_call>\n\n\
         IMPORTANT: Output the <tool_call> block and STOP. Do NOT generate a fake response. The system will execute the tool and give you the real result.\n\n\
         Available tools:\n"
    );
    for tool in tools {
        if let Some(func) = tool.get("function") {
            let name = func.get("name").and_then(|n| n.as_str()).unwrap_or("?");
            let desc = func.get("description").and_then(|d| d.as_str()).unwrap_or("");
            let params = func.get("parameters")
                .map(|p| serde_json::to_string(p).unwrap_or_default())
                .unwrap_or_default();
            tool_desc.push_str(&format!("\n- {name}: {desc}\n  Parameters: {params}\n"));
        }
    }
    tool_desc.push_str("\n\
         ## Example\n\
         User: search my vault for security\n\
         Assistant:\n\
         <tool_call>\n{\"name\": \"search_vault\", \"arguments\": {\"query\": \"security\"}}\n</tool_call>\n\n\
         User: what is in the file technical-architecture?\n\
         Assistant:\n\
         <tool_call>\n{\"name\": \"read_file\", \"arguments\": {\"path\": \"technical-architecture.md\"}}\n</tool_call>\n\n\
         For general knowledge questions (not about the vault), answer directly without tools.\n");

    // Rebuild messages with tools injected into system prompt.
    let mut chat_msgs: Vec<llama_cpp_2::model::LlamaChatMessage> = Vec::new();
    let mut injected_system = false;

    for msg in messages {
        let role = match msg.role {
            ChatRole::System => "system",
            ChatRole::User => "user",
            ChatRole::Assistant => "assistant",
            ChatRole::Tool => "tool",
        };
        let content = if matches!(msg.role, ChatRole::System) && !injected_system {
            injected_system = true;
            format!("{}{}", msg.content, tool_desc)
        } else if msg.role.is_tool() && msg.tool_call_id.is_some() {
            format!("[tool_call_id: {}]\n{}", msg.tool_call_id.as_deref().unwrap_or(""), msg.content)
        } else if !msg.tool_calls.is_empty() {
            let tc_json = serde_json::to_string(&msg.tool_calls).unwrap_or_default();
            if msg.content.is_empty() { tc_json } else { format!("{}\n{}", msg.content, tc_json) }
        } else {
            msg.content.clone()
        };
        match llama_cpp_2::model::LlamaChatMessage::new(role.to_string(), content) {
            Ok(m) => chat_msgs.push(m),
            Err(e) => return Err(format!("Bad chat message: {e}")),
        }
    }

    // If no system message existed, prepend one with tool definitions.
    if !injected_system {
        if let Ok(sys_msg) = llama_cpp_2::model::LlamaChatMessage::new(
            "system".to_string(),
            format!("You are a helpful assistant.{tool_desc}"),
        ) {
            chat_msgs.insert(0, sys_msg);
        }
    }

    model
        .apply_chat_template(tmpl, &chat_msgs, true)
        .map_err(|e| format!("Chat template failed: {e}"))
}

impl ChatRole {
    fn is_tool(&self) -> bool {
        matches!(self, ChatRole::Tool)
    }
}

// ── Tool call parsing ──
//
// Models emit tool calls in various formats. We look for common patterns:
//   - <tool_call>{"name": "...", "arguments": {...}}</tool_call>
//   - {"name": "...", "arguments": {...}}  (raw JSON object)
//   - <|python_tag|>  (Llama 3 style)
//   - ```json\n{"name": "...", "arguments": {...}}\n```

fn try_parse_tool_call(text: &str) -> Option<ToolCall> {
    // Pattern 1: <tool_call>...</tool_call>
    if let Some(start) = text.find("<tool_call>") {
        if let Some(end) = text.find("</tool_call>") {
            let json_str = &text[start + "<tool_call>".len()..end].trim();
            return parse_tool_json(json_str);
        }
        // <tool_call> found but no closing tag yet -- check if the JSON inside is complete.
        let after = &text[start + "<tool_call>".len()..];
        let trimmed = after.trim();
        if trimmed.ends_with('}') {
            return parse_tool_json(trimmed);
        }
    }

    // Pattern 2: <functioncall> (Gemma style)
    if let Some(start) = text.find("<functioncall>") {
        if let Some(end) = text.find("</functioncall>") {
            let json_str = &text[start + "<functioncall>".len()..end].trim();
            return parse_tool_json(json_str);
        }
    }

    // Pattern 3: ```json code block with tool call JSON
    if let Some(start) = text.find("```json") {
        let after = &text[start + "```json".len()..];
        let end = after.find("```").unwrap_or(after.len());
        let json_str = after[..end].trim();
        if let Some(tc) = parse_tool_json(json_str) {
            return Some(tc);
        }
    }

    // Pattern 4: {"toolSpec": ... } (Gemma 4 native format)
    if text.contains("\"toolSpec\"") || text.contains("\"tool_call\"") {
        // Try to extract any JSON object with a "name" field
        for marker in ["{\"toolSpec\"", "{\"name\"", "{\"tool_call\""] {
            if let Some(start) = text.find(marker) {
                let candidate = &text[start..];
                // Find matching closing brace.
                if let Some(end) = find_json_end(candidate) {
                    if let Some(tc) = parse_tool_json(&candidate[..=end]) {
                        return Some(tc);
                    }
                }
            }
        }
    }

    // Pattern 5: Standalone JSON object with "name" key on its own line
    let trimmed = text.trim();
    if trimmed.ends_with('}') {
        if let Some(brace_start) = trimmed.rfind("\n{") {
            let candidate = &trimmed[brace_start + 1..];
            if let Some(tc) = parse_tool_json(candidate) {
                return Some(tc);
            }
        } else if trimmed.starts_with('{') {
            return parse_tool_json(trimmed);
        }
    }

    None
}

/// Find the position of the closing brace that matches the opening brace at position 0.
fn find_json_end(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, c) in s.char_indices() {
        if escape { escape = false; continue; }
        if c == '\\' && in_string { escape = true; continue; }
        if c == '"' { in_string = !in_string; continue; }
        if in_string { continue; }
        if c == '{' { depth += 1; }
        if c == '}' { depth -= 1; if depth == 0 { return Some(i); } }
    }
    None
}

fn parse_tool_json(json_str: &str) -> Option<ToolCall> {
    let v: serde_json::Value = serde_json::from_str(json_str).ok()?;
    let obj = v.as_object()?;

    let name = obj.get("name").and_then(|n| n.as_str())?.to_string();
    let arguments = obj.get("arguments")
        .or_else(|| obj.get("parameters"))
        .cloned()
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    Some(ToolCall {
        id: format!("call_{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)),
        name,
        arguments,
    })
}

fn might_be_tool_call_start(text: &str) -> bool {
    // Hold back flushing if we see partial markers that could be tag/tool call starts.
    let tail = if text.len() > 30 { &text[text.len() - 30..] } else { text };
    tail.contains("<tool_c")
        || tail.contains("<functionc")
        || tail.contains("<|python_t")
        || tail.contains("<chann")
        || tail.ends_with('<')
        || (tail.contains('<') && !tail.contains('>'))
}

fn extract_pre_tool_text(text: &str) -> String {
    // Return text before any tool call marker.
    for marker in ["<tool_call>", "<functioncall>", "<|python_tag|>"] {
        if let Some(pos) = text.find(marker) {
            return text[..pos].to_string();
        }
    }
    // If it's a raw JSON tool call, return empty (the whole thing is the tool call).
    String::new()
}
