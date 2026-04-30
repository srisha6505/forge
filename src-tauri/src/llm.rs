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
    /// Construct an InferenceHandle from an already-spawned thread's request
    /// sender. Used by per-provider modules (openai/gemini/copilot) that own
    /// their own threads but expose the unified handle to the agent loop.
    pub fn from_sender(tx: mpsc::Sender<InferenceRequest>, model_name: String) -> Self {
        Self { tx, model_name }
    }

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
///
/// Compiled out of the lite build. Without `local-llm`, the entry point
/// in `commands::connect_inference` short-circuits with a clear error
/// telling the user to use Ollama (over openai_compat) or an API
/// provider. Saves ~150-250 MB of bundled llama.cpp + CUDA libraries.
#[cfg(feature = "local-llm")]
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

/// Process-wide singleton llama backend. `LlamaBackend::init()` panics
/// the second time it's called (returns `BackendAlreadyInitialized`)
/// because llama.cpp's global ggml init can only happen once per process.
/// We hit this whenever the user switches models or reconnects, since
/// each connect spawns a fresh inference thread.
///
/// Hold the backend in a OnceLock and lend a `&'static LlamaBackend` to
/// every inference thread. The backend is Send+Sync; loading models and
/// creating contexts only borrow it.
#[cfg(feature = "local-llm")]
fn shared_backend()
    -> Result<&'static llama_cpp_2::llama_backend::LlamaBackend, String>
{
    use llama_cpp_2::llama_backend::LlamaBackend;
    use std::sync::OnceLock;
    static BACKEND: OnceLock<LlamaBackend> = OnceLock::new();
    if let Some(b) = BACKEND.get() {
        return Ok(b);
    }
    match LlamaBackend::init() {
        Ok(b) => {
            // get_or_init the cell. If a concurrent thread beat us to it
            // (rare — both initialize the same global ggml state), fall
            // back to whatever is now there.
            let _ = BACKEND.set(b);
            Ok(BACKEND.get().expect("backend just set"))
        }
        Err(e) => Err(format!("Backend init failed: {e}")),
    }
}

#[cfg(feature = "local-llm")]
fn inference_loop(
    model_path: PathBuf,
    n_gpu_layers: u32,
    n_ctx: u32,
    rx: mpsc::Receiver<InferenceRequest>,
) {
    use llama_cpp_2::context::params::LlamaContextParams;
    use llama_cpp_2::llama_batch::LlamaBatch;
    use llama_cpp_2::model::params::LlamaModelParams;
    use llama_cpp_2::model::{AddBos, LlamaModel, Special};
    use llama_cpp_2::sampling::LlamaSampler;

    // Use the process-wide backend (initialized on first spawn, reused
    // on every subsequent spawn).
    let backend = match shared_backend() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[forge-llm] {e}");
            for req in rx.iter() {
                let _ = req.response_tx.send(InferenceEvent::Error(e.clone()));
            }
            return;
        }
    };

    // Load model.
    let model_params = LlamaModelParams::default().with_n_gpu_layers(n_gpu_layers);
    let model = match LlamaModel::load_from_file(backend, &model_path, &model_params) {
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
    let mut ctx = match model.new_context(backend, ctx_params) {
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

#[cfg(feature = "local-llm")]
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
    //
    // Sampler tuned for tool-using agent work, NOT free-form chat.
    // Gemma 4's published creative defaults (temp=1.0, top_p=0.95) sample
    // low-probability tokens often — including the `<turn|>` end-of-turn
    // token mid-content. Result: tool-call args truncate randomly. Same
    // model at temp ≈ 0 produces full output reliably (verified against
    // tarang/llama-server which uses --temp 0).
    //
    // Greedy (temp=0) follows the model's most likely path through the
    // chat template, which for trained tool-call patterns means
    // emitting the close tag only after the content is genuinely
    // complete. Trade: less variety for free-form prose. We accept that
    // — Forge is an agent harness, not a creative writing tool. Free-
    // form prose generation still works fine because greedy doesn't
    // mean broken, just deterministic.
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as u32)
        .unwrap_or(42);
    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::temp(0.0),
        LlamaSampler::top_k(1),
        LlamaSampler::dist(seed),
    ]);

    let mut full_output = String::new(); // accumulate everything for tool call detection
    let mut n_decoded = 0usize;
    // Cap at 8192 (was 4096). Gemma 4 E4B can produce long markdown notes
    // (intro + explanation + math + widget) and emit `<|channel>` thinking
    // tokens that eat into the budget. With a 1000-token system prompt +
    // ~1500 tokens of injected tool definitions, plus user message, we're
    // around 2700 prompt tokens. 8192 generation cap leaves headroom for
    // ~1000 tokens of thinking AND a full tool_call with 4-6KB of content.
    let max_gen = (n_ctx - tokens.len()).min(8192);
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

        // Backstop: llama-cpp-2 v0.1's `is_eog_token` only checks the
        // primary eos_token_id (Gemma 4 = `<turn|>`, id=106). The model's
        // vocab has many other turn-ending special tokens that get rendered
        // as literal text when emitted. Token IDs verified by reading the
        // GGUF directly:
        //   id=1   `<eos>`            (type=NORMAL, leaks as text)
        //   id=49  `<tool_call|>`     (closes tool call, MUST stop here or
        //                              model rambles into thinking after)
        //   id=50  `<|tool_response>` (control)
        //   id=51  `<tool_response|>` (control)
        //   id=106 `<turn|>`          (already caught by is_eog_token)
        // Without this guard the model emits `<tool_call|>` to end its
        // tool call, then keeps generating thinking-channel text that eats
        // budget and confuses the parser. Stop on rendered-text match.
        let stripped = piece.trim();
        if stripped == "<eos>"
            || stripped == "<tool_call|>"
            || stripped == "<|tool_response>"
            || stripped == "<tool_response|>"
            || stripped == "<turn|>"
            || stripped == "</s>"
            || stripped == "<|end|>"
            || stripped == "<|endoftext|>"
        {
            break;
        }

        full_output.push_str(&piece);

        if in_thinking {
            thinking_buf.push_str(&piece);
            // Check if thinking block ended.
            // Gemma uses <channel|> as closing, other models use </channel>.
            let end_tag = thinking_buf.find("</channel>").map(|p| (p, "</channel>".len()))
                .or_else(|| thinking_buf.find("<channel|>").map(|p| (p, "<channel|>".len())))
                .or_else(|| thinking_buf.find("<|channel|>").map(|p| (p, "<|channel|>".len())));
            if let Some((end_pos, tag_len)) = end_tag {
                let thought = thinking_buf[..end_pos].to_string();
                let remainder = thinking_buf[end_pos + tag_len..].to_string();
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
            // Gemma uses <|channel>, other models use <channel>.
            let channel_start = text_buf.find("<|channel>").map(|p| (p, "<|channel>".len()))
                .or_else(|| text_buf.find("<channel>").map(|p| (p, "<channel>".len())));
            if let Some((start_pos, tag_len)) = channel_start {
                let before = text_buf[..start_pos].to_string();
                if !before.trim().is_empty() {
                    let _ = tx.send(InferenceEvent::Token(before));
                }
                thinking_buf = text_buf[start_pos + tag_len..].to_string();
                text_buf.clear();
                in_thinking = true;
            } else if text_buf.contains('<') && !text_buf.contains('>') {
                // Might be start of a tag, hold back.
            } else {
                // Check for tool call in full output.
                if let Some(tool_call) = try_parse_tool_call(&full_output) {
                    let pre = extract_pre_tool_text(&full_output);
                    let pre = strip_tool_markers(&pre);
                    if !pre.trim().is_empty() {
                        let _ = tx.send(InferenceEvent::Token(pre));
                    }
                    let _ = tx.send(InferenceEvent::ToolUse(tool_call));
                    let _ = tx.send(InferenceEvent::Done);
                    return;
                }

                // Flush text to UI (but hold back if partial tag/tool call).
                if !might_be_tool_call_start(&text_buf) {
                    if !text_buf.is_empty() {
                        let cleaned = strip_tool_markers(&text_buf);
                        if !cleaned.is_empty() {
                            let _ = tx.send(InferenceEvent::Token(cleaned));
                        }
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

    // Always dump the full raw model output at stream end so we can see
    // exactly what Gemma produced, including special tokens and formatting.
    eprintln!("[forge-llm] === FULL MODEL OUTPUT ===\n{}\n=== END OUTPUT ===", full_output);

    // Try to parse tool call from full output one final time.
    if let Some(tool_call) = try_parse_tool_call(&full_output) {
        let pre = extract_pre_tool_text(&full_output);
        let pre = strip_tool_markers(&pre);
        if !pre.trim().is_empty() {
            let _ = tx.send(InferenceEvent::Token(pre));
        }
        let _ = tx.send(InferenceEvent::ToolUse(tool_call));
        let _ = tx.send(InferenceEvent::Done);
        return;
    }

    // Flush remaining text, but strip any tool-call markers so raw model
    // tags never leak to the UI even if parsing failed.
    if !text_buf.is_empty() {
        let cleaned = strip_tool_markers(&text_buf);
        if !cleaned.trim().is_empty() {
            let _ = tx.send(InferenceEvent::Token(cleaned));
        }
    }
    if in_thinking && !thinking_buf.is_empty() {
        let _ = tx.send(InferenceEvent::Thinking(thinking_buf));
    }
    let _ = tx.send(InferenceEvent::Done);
}

// ── Prompt formatting ──

#[cfg(feature = "local-llm")]
fn format_prompt(
    model: &llama_cpp_2::model::LlamaModel,
    messages: &[ChatMessage],
    tools: &[serde_json::Value],
) -> Result<String, String> {
    // Build messages in the format expected by apply_chat_template.
    // Each message is a JSON object: {"role": "...", "content": "..."}
    let mut chat_msgs: Vec<llama_cpp_2::model::LlamaChatMessage> = Vec::new();

    for msg in messages {
        // Tool results: Gemma chat templates vary in how they handle role="tool".
        // Rewrite tool responses as user-role messages with explicit marker so
        // the model ALWAYS sees the data regardless of template handling.
        if msg.role.is_tool() {
            let body = format!(
                "TOOL_RESPONSE from tool call id={}:\n---\n{}\n---\nUse ONLY this data to answer. Do not invent file names, URLs, or facts not in the TOOL_RESPONSE above.",
                msg.tool_call_id.as_deref().unwrap_or(""),
                msg.content
            );
            match llama_cpp_2::model::LlamaChatMessage::new("user".to_string(), body) {
                Ok(m) => chat_msgs.push(m),
                Err(e) => return Err(format!("Bad tool message: {e}")),
            }
            continue;
        }

        let role = match msg.role {
            ChatRole::System => "system",
            ChatRole::User => "user",
            ChatRole::Assistant => "assistant",
            ChatRole::Tool => "tool", // unreachable per above
        };
        let content = if !msg.tool_calls.is_empty() {
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
    //
    // NOTE on enable_thinking: llama-cpp-2 v0.1.143's binding signature is
    // `apply_chat_template_with_tools_oaicompat(tmpl, msgs, tools_json,
    // json_schema, add_gen_prompt)`. The 4th param is JSON schema for
    // grammar-constrained output, NOT chat_template_kwargs. The binding
    // does NOT expose chat-template kwargs — to disable thinking like
    // tarang's `--chat-template-kwargs '{"enable_thinking":false}'`, we
    // would need to render the Jinja template ourselves (e.g. minijinja).
    // For Gemma 4 specifically, inspecting the GGUF chat template shows
    // `<|think|>` is only injected if enable_thinking is explicitly true,
    // so the default (undefined) already behaves like enable_thinking=false.
    // The thinking we still see is from the model's training emitting
    // `<|channel>...<channel|>` blocks unprompted; those are stripped at
    // streaming-time below, not via the template. Pass None for json_schema.
    if !tools.is_empty() {
        let tools_json = serde_json::to_string(tools).unwrap_or("[]".into());
        match model.apply_chat_template_with_tools_oaicompat(
            &tmpl, &chat_msgs, Some(&tools_json), None, true,
        ) {
            Ok(result) => {
                eprintln!("[forge-llm] Using native tool template");
                eprintln!("[forge-llm] === RENDERED PROMPT (last 2000 chars) ===\n{}\n=== END PROMPT ===",
                    if result.prompt.len() > 2000 {
                        &result.prompt[result.prompt.len()-2000..]
                    } else {
                        &result.prompt
                    });
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
#[cfg(feature = "local-llm")]
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
    // Pattern 0: Raw Gemma tool call without tags: "call:tool_name{...}"
    // Brace-balance the args, but TRACK STRING BOUNDARIES so braces inside
    // string values don't trip the balancer. Gemma emits string values
    // wrapped in `<|"|>...<|"|>` (special token id=52). When user content
    // has its own `{` and `}` (HTML widgets, JS arrow functions, JSON), the
    // raw `}` tokens would otherwise close the tool call prematurely.
    //
    // We match the literal byte sequence `<|"|>` (5 bytes) on the RAW text —
    // do not normalize it to `"` first. Generic `"` chars inside content
    // (HTML attrs etc.) are NOT delimiters; only the `<|"|>` token is.
    if let Some(call_start) = text.find("call:") {
        let after = &text[call_start + "call:".len()..];
        if let Some(brace_start) = after.find('{') {
            let bytes = after.as_bytes();
            let mut depth = 0i32;
            let mut end: Option<usize> = None;
            let mut in_string = false;
            let mut i = brace_start;
            while i < bytes.len() {
                // Toggle on the literal 5-byte sequence `<|"|>`.
                if i + 5 <= bytes.len() && &bytes[i..i + 5] == b"<|\"|>" {
                    in_string = !in_string;
                    i += 5;
                    continue;
                }
                if !in_string {
                    match bytes[i] {
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                end = Some(i);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                i += 1;
            }
            if let Some(end_pos) = end {
                // Pass the RAW candidate (with <|"|> markers intact) to the
                // strict parser. Normalizing here would erase the only
                // reliable string boundary and force the parser to fall back
                // to the legacy keyword-anchored scanner — which is exactly
                // the path that mistook `data=${{...}}` (JS template literal
                // inside widget HTML) for a new `data` arg key.
                let candidate_raw = &after[..=end_pos];
                eprintln!("[forge-llm] raw call: tool_call content: {:?}", candidate_raw);
                if let Some(tc) = parse_gemma_tool_call(candidate_raw) {
                    eprintln!("[forge-llm] parse_gemma_tool_call (raw) result: {:?}", (&tc.name, &tc.arguments));
                    return Some(tc);
                }
            }
        }
    }

    // Find tool call opening tag: <tool_call>, <|tool_call>, or <|tool_call|>
    let tc_start = text.find("<|tool_call>").map(|p| (p, "<|tool_call>".len()))
        .or_else(|| text.find("<tool_call>").map(|p| (p, "<tool_call>".len())));

    if let Some((start, tag_len)) = tc_start {
        let after = &text[start + tag_len..];
        // Find closing tag.
        let end_pos = after.find("</tool_call>").map(|p| (p, "</tool_call>".len()))
            .or_else(|| after.find("<tool_call|>").map(|p| (p, "<tool_call|>".len())))
            .or_else(|| after.find("<|tool_call|>").map(|p| (p, "<|tool_call|>".len())))
            .or_else(|| after.find("<eos>").map(|p| (p, "<eos>".len())));

        // CRITICAL: only treat the call as complete when we actually see a
        // closing tag. The streaming parser is called after EVERY emitted
        // token, so an `ends_with(')')` heuristic fires the moment the model
        // emits text containing a parenthetical (e.g., "the sampling rate
        // ($f_s$)"), exits the generation loop, and writes a truncated file.
        // This was the root cause of every Nyquist truncation: the model
        // wasn't stopping early — we were stopping IT early.
        let content = if let Some((end, _)) = end_pos {
            after[..end].trim()
        } else {
            // No close tag yet → incomplete tool call, let generation continue.
            ""
        };

        if !content.is_empty() {
            let trimmed = content.trim();
            eprintln!("[forge-llm] raw tool_call content: {:?}", trimmed);

            // Gemma format: call:tool_name{key:<|"|>value<|"|>, ...}
            // Pass RAW (un-normalized) text — keep <|"|> markers intact so the
            // strict state-machine parser can use them as string boundaries.
            if trimmed.starts_with("call:") {
                let after_call = &trimmed["call:".len()..];
                let parsed = parse_gemma_tool_call(after_call);
                eprintln!("[forge-llm] parse_gemma_tool_call result: {:?}", parsed.as_ref().map(|t| (&t.name, &t.arguments)));
                return parsed;
            }

            // Below paths are for non-Gemma formats (plain JSON, function-call
            // syntax). These don't use <|"|>, so normalization is safe here.
            let cleaned = trimmed.replace("<|\"|>", "\"").replace("<|'|>", "'");
            let cleaned = cleaned.trim();

            if let Some(tc) = parse_tool_json(cleaned) {
                eprintln!("[forge-llm] parse_tool_json result: name={}, args={}", tc.name, tc.arguments);
                return Some(tc);
            }
            for tool_name in KNOWN_TOOLS {
                if cleaned.starts_with(tool_name) {
                    let fn_args = &cleaned[tool_name.len()..];
                    if let Some(tc) = parse_function_call_syntax(tool_name, fn_args) {
                        eprintln!("[forge-llm] parse_function_call_syntax result: name={}, args={}", tc.name, tc.arguments);
                        return Some(tc);
                    }
                }
            }
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

    // Pattern 6: Gemma function-call syntax like:
    //   search_vault(query:{"training_report"})
    //   write_file(path:{"notes/new.md"}, content:{"..."})
    for tool_name in KNOWN_TOOLS {
        if let Some(pos) = trimmed.find(tool_name) {
            let after = &trimmed[pos + tool_name.len()..];
            if after.starts_with('(') {
                if let Some(tc) = parse_function_call_syntax(tool_name, after) {
                    return Some(tc);
                }
            }
        }
    }

    None
}

/// Complete list of tools the agent exposes. Parsers that try to match
/// function-call syntax need to know every tool name.
const KNOWN_TOOLS: &[&str] = &[
    "search_vault",
    "read_file",
    "list_files",
    "read_section",
    "write_file",
    "edit_file",
    "rename_file",
    "delete_file",
    "grep_vault",
    "web_search",
];

/// Parse Gemma 4 native tool call format: tool_name{key:<|"|>value<|"|>, key2:<|"|>value2<|"|>}
///
/// Gemma's chat template emits string values delimited by the special token
/// `<|"|>` (id=52, 5 raw bytes). That marker is the ONLY reliable string
/// boundary — regular `"` chars frequently appear inside content (HTML attrs,
/// JS strings) and must NOT be treated as delimiters. This parser keeps the
/// raw markers intact and runs a proper state-machine over the bytes:
///
///   - Outside any string: `{` increments depth, `}` decrements, `,` separates
///     args, `key:` or `key=` introduces a value.
///   - Inside `<|"|>...<|"|>`: every byte is content. No keyword matching, no
///     brace counting, no terminator detection. This is what makes the parser
///     robust to widget code that contains `data=`, `}`, `"`, etc.
///
/// On any failure we fall back to the legacy heuristic parsers, which handle
/// rendered/normalized text (e.g., previous-turn tool calls in conversation
/// history where `<|"|>` has already been collapsed to `"`).
fn parse_gemma_tool_call(s: &str) -> Option<ToolCall> {
    let brace = s.find('{')?;
    let name = s[..brace].trim().to_string();
    let after_open = &s[brace + 1..];

    // Strict path: use the raw <|"|>-aware state machine first.
    if let Some(arguments) = parse_gemma_args_strict(after_open) {
        return Some(ToolCall { id: new_call_id(), name, arguments });
    }

    // Fallback for already-normalized text (no <|"|> markers left).
    let inner = after_open.rsplit_once('}').map(|(before, _)| before).unwrap_or(after_open);
    let arguments = parse_kv_anchored(inner, &name)
        .or_else(|| parse_kv_map(inner))?;
    Some(ToolCall { id: new_call_id(), name, arguments })
}

/// Strict, string-aware parser for Gemma's args body.
///
/// Input: bytes immediately AFTER the opening `{` of `call:tool{...`. The
/// closing `}` (and anything after) is found by this function while honoring
/// `<|"|>` string boundaries — so a `}` inside a string value (widget JS,
/// nested JSON in content) does NOT close the args.
///
/// Returns None if no balanced close is found yet (caller should keep
/// generating). Once a close IS found, splits on top-level commas and parses
/// each `key:value` or `key=value` pair with the same string awareness.
fn parse_gemma_args_strict(after_open: &str) -> Option<serde_json::Value> {
    const STR_TOK: &[u8] = b"<|\"|>";   // 5 bytes
    const STR_TOK_SQ: &[u8] = b"<|'|>"; // single-quoted variant

    // Phase 1: find the balanced closing `}` while tracking string state.
    let bytes = after_open.as_bytes();
    let mut depth: i32 = 1;
    let mut in_str = false;
    let mut sq_in_str = false;
    let mut i = 0;
    let mut close: Option<usize> = None;
    while i < bytes.len() {
        if !sq_in_str && i + STR_TOK.len() <= bytes.len() && &bytes[i..i + STR_TOK.len()] == STR_TOK {
            in_str = !in_str;
            i += STR_TOK.len();
            continue;
        }
        if !in_str && i + STR_TOK_SQ.len() <= bytes.len() && &bytes[i..i + STR_TOK_SQ.len()] == STR_TOK_SQ {
            sq_in_str = !sq_in_str;
            i += STR_TOK_SQ.len();
            continue;
        }
        if !in_str && !sq_in_str {
            match bytes[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    let end = close?;
    let body = &after_open[..end];

    // Phase 2: split body into top-level key/value pairs at commas that are
    // OUTSIDE any string. Then parse each pair the same way.
    let pairs = split_top_level_commas(body);
    let mut map = serde_json::Map::new();
    for pair in pairs {
        let pair = pair.trim();
        if pair.is_empty() { continue; }
        let (key, raw_val) = split_key_value_string_aware(pair)?;
        let key = key.trim().trim_matches(|c: char| c == '"' || c == '\'').to_string();
        if key.is_empty() { continue; }
        let value = parse_gemma_value(raw_val.trim());
        map.insert(key, value);
    }
    if map.is_empty() { None } else { Some(serde_json::Value::Object(map)) }
}

/// Split `body` on commas that are OUTSIDE any `<|"|>...<|"|>` string and
/// outside any nested `{...}` (also string-aware).
fn split_top_level_commas(body: &str) -> Vec<&str> {
    const STR_TOK: &[u8] = b"<|\"|>";
    const STR_TOK_SQ: &[u8] = b"<|'|>";
    let bytes = body.as_bytes();
    let mut in_str = false;
    let mut sq_in_str = false;
    let mut depth: i32 = 0;
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if !sq_in_str && i + STR_TOK.len() <= bytes.len() && &bytes[i..i + STR_TOK.len()] == STR_TOK {
            in_str = !in_str;
            i += STR_TOK.len();
            continue;
        }
        if !in_str && i + STR_TOK_SQ.len() <= bytes.len() && &bytes[i..i + STR_TOK_SQ.len()] == STR_TOK_SQ {
            sq_in_str = !sq_in_str;
            i += STR_TOK_SQ.len();
            continue;
        }
        if !in_str && !sq_in_str {
            match bytes[i] {
                b'{' | b'[' => depth += 1,
                b'}' | b']' => depth -= 1,
                b',' if depth == 0 => {
                    out.push(&body[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        i += 1;
    }
    out.push(&body[start..]);
    out
}

/// Split `pair` on the first `:` or `=` that's outside any string.
fn split_key_value_string_aware(pair: &str) -> Option<(&str, &str)> {
    const STR_TOK: &[u8] = b"<|\"|>";
    const STR_TOK_SQ: &[u8] = b"<|'|>";
    let bytes = pair.as_bytes();
    let mut in_str = false;
    let mut sq_in_str = false;
    let mut i = 0;
    while i < bytes.len() {
        if !sq_in_str && i + STR_TOK.len() <= bytes.len() && &bytes[i..i + STR_TOK.len()] == STR_TOK {
            in_str = !in_str;
            i += STR_TOK.len();
            continue;
        }
        if !in_str && i + STR_TOK_SQ.len() <= bytes.len() && &bytes[i..i + STR_TOK_SQ.len()] == STR_TOK_SQ {
            sq_in_str = !sq_in_str;
            i += STR_TOK_SQ.len();
            continue;
        }
        if !in_str && !sq_in_str && (bytes[i] == b':' || bytes[i] == b'=') {
            return Some((&pair[..i], &pair[i + 1..]));
        }
        i += 1;
    }
    None
}

/// Parse a single Gemma value: `<|"|>...<|"|>` string, bare scalar, or nested.
///
/// For string values: strip the `<|"|>` markers and return the bytes between
/// them VERBATIM. The model is responsible for emitting valid file content;
/// our job is to deliver those bytes to disk unchanged. No escape decoding,
/// no escape encoding, no sanitization. If the model writes a real newline,
/// the file gets a real newline. If the model writes the literal two chars
/// `\` and `n`, the file gets those two chars. We don't second-guess.
fn parse_gemma_value(raw: &str) -> serde_json::Value {
    let raw = raw.trim();
    if let Some(stripped) = raw.strip_prefix("<|\"|>").and_then(|s| s.strip_suffix("<|\"|>")) {
        return serde_json::Value::String(stripped.to_string());
    }
    if let Some(stripped) = raw.strip_prefix("<|'|>").and_then(|s| s.strip_suffix("<|'|>")) {
        return serde_json::Value::String(stripped.to_string());
    }
    // Already-normalized text (no special markers left): take outer quotes off if present.
    if raw.len() >= 2 {
        let b = raw.as_bytes();
        let f = b[0]; let l = b[b.len() - 1];
        if (f == b'"' || f == b'\'') && f == l {
            return serde_json::Value::String(raw[1..raw.len() - 1].to_string());
        }
    }
    // Bare scalars
    if let Ok(n) = raw.parse::<i64>() { return serde_json::Value::Number(n.into()); }
    if let Ok(n) = raw.parse::<f64>() { if let Some(num) = serde_json::Number::from_f64(n) { return serde_json::Value::Number(num); } }
    if raw == "true" { return serde_json::Value::Bool(true); }
    if raw == "false" { return serde_json::Value::Bool(false); }
    if raw == "null" { return serde_json::Value::Null; }
    serde_json::Value::String(raw.to_string())
}

/// Parse Gemma-style function call: tool_name(key:"value", key2:"value2")
/// or tool_name(key:{"value"}, key2:{"value2"}).
fn parse_function_call_syntax(name: &str, args_str: &str) -> Option<ToolCall> {
    let trimmed = args_str.trim();
    let after_open = trimmed.strip_prefix('(')?;
    let close_pos = after_open.rfind(')').unwrap_or(after_open.len());
    let inner = &after_open[..close_pos];

    let arguments = parse_kv_anchored(inner, name)
        .or_else(|| parse_kv_map(inner))?;

    Some(ToolCall {
        id: new_call_id(),
        name: name.to_string(),
        arguments,
    })
}

/// Parse a tool-call argument body by anchoring on known key names rather
/// than walking string boundaries. For each known field of `tool_name`, find
/// the first position where that field appears followed by `:` or `=`, then
/// use the positions of *other* known keys as value end markers.
///
/// This sidesteps the "LLM emitted unescaped inner quotes" failure mode
/// where a quote-aware parser gets confused and truncates a content value.
fn parse_kv_anchored(inner: &str, tool_name: &str) -> Option<serde_json::Value> {
    let candidate_keys: &[&str] = match tool_name {
        "write_file" => &["path", "content", "file", "filename", "filepath", "file_path", "text", "body", "data"],
        "edit_file" => &["path", "old_text", "new_text", "old", "new"],
        "read_file" => &["path", "file", "filename"],
        "read_section" => &["path", "heading", "section"],
        "list_files" => &["directory", "path", "dir"],
        "search_vault" => &["query", "limit"],
        "grep_vault" => &["pattern", "file_glob", "glob"],
        "rename_file" => &["from", "to", "old_path", "new_path"],
        "delete_file" => &["path", "file"],
        "web_search" => &["query", "num_results", "limit"],
        _ => return None,
    };

    // Find the first anchor position for each candidate key.
    let mut hits: Vec<(usize, &'static str)> = Vec::new();
    for &key in candidate_keys {
        if let Some(pos) = find_key_anchor(inner, key) {
            hits.push((pos, key));
        }
    }
    if hits.is_empty() {
        return None;
    }
    hits.sort_by_key(|(p, _)| *p);

    let mut map = serde_json::Map::new();
    for (i, (start, key)) in hits.iter().enumerate() {
        let key_end = start + key.len();
        let after_key_bytes = &inner[key_end..];
        // Skip optional closing quote of the key, whitespace, then the separator.
        let after_quote = after_key_bytes.trim_start_matches(|c: char| c == '"' || c == '\'' || c.is_whitespace());
        let after_sep = match after_quote.strip_prefix(':').or_else(|| after_quote.strip_prefix('=')) {
            Some(s) => s.trim_start(),
            None => continue,
        };
        let value_start = inner.len() - after_sep.len();

        let value_end = if i + 1 < hits.len() {
            let next_start = hits[i + 1].0;
            if next_start <= value_start {
                continue;
            }
            // The comma right before the next key is the true boundary; if
            // there is no comma in the slice, the value butts up against
            // the next key directly.
            inner[value_start..next_start]
                .rfind(',')
                .map(|p| value_start + p)
                .unwrap_or(next_start)
        } else {
            inner.len()
        };

        if value_end <= value_start {
            continue;
        }

        let raw = inner[value_start..value_end].trim();
        let cleaned = clean_anchored_value(raw);

        if !map.contains_key(*key) {
            // Try to parse numbers/bools, otherwise store as string.
            let value = if let Ok(n) = cleaned.parse::<i64>() {
                serde_json::Value::Number(n.into())
            } else if cleaned == "true" {
                serde_json::Value::Bool(true)
            } else if cleaned == "false" {
                serde_json::Value::Bool(false)
            } else {
                serde_json::Value::String(cleaned)
            };
            map.insert(key.to_string(), value);
        }
    }

    if map.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(map))
    }
}

/// Find the first position in `haystack` where `key` appears as a standalone
/// field name — that is, preceded by a field boundary (start, comma, brace,
/// whitespace, or quote) and followed (possibly after a closing quote and
/// whitespace) by `:` or `=`.
fn find_key_anchor(haystack: &str, key: &str) -> Option<usize> {
    let mut search_from = 0;
    while let Some(rel) = haystack[search_from..].find(key) {
        let start = search_from + rel;
        let end = start + key.len();

        let before_ok = start == 0
            || matches!(
                haystack.as_bytes()[start - 1],
                b',' | b' ' | b'\t' | b'\n' | b'{' | b'"' | b'\''
            );

        if before_ok {
            let after = &haystack[end..];
            let after_trim = after.trim_start_matches(|c: char| c == '"' || c == '\'' || c.is_whitespace());
            if after_trim.starts_with(':') || after_trim.starts_with('=') {
                return Some(start);
            }
        }

        search_from = end;
    }
    None
}

/// Clean an anchored value: strip outer quotes, outer braces, decode common
/// escape sequences, and trim whitespace.
fn clean_anchored_value(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    // Strip surrounding braces `{"..."}` shape.
    if s.starts_with('{') && s.ends_with('}') {
        s = s[1..s.len() - 1].trim().to_string();
    }
    // Strip one layer of surrounding quotes.
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' || first == b'\'') && first == last {
            s = s[1..s.len() - 1].to_string();
        }
    }
    // Decode common escape sequences.
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(esc) = chars.next() {
                out.push(unescape(esc));
                continue;
            }
        }
        out.push(c);
    }
    out
}

fn new_call_id() -> String {
    format!(
        "call_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    )
}

/// Parse a key:value map from a brace/paren-less inner string. Handles:
///   - Quoted strings (single or double), with \n \t \r \\ \" \' escapes
///   - Braced values like `path:{"notes/file.md"}`
///   - Unquoted scalar values (numbers, bools, bare identifiers)
///   - Commas inside quoted strings (do not split there)
///   - Optional `=` as key/value separator in addition to `:`
///
/// Returns `None` if no key-value pairs could be extracted.
fn parse_kv_map(inner: &str) -> Option<serde_json::Value> {
    let mut map = serde_json::Map::new();
    let mut chars = inner.chars().peekable();

    loop {
        // Skip whitespace and separators.
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() || c == ',' {
                chars.next();
            } else {
                break;
            }
        }
        if chars.peek().is_none() {
            break;
        }

        // Parse key: either a quoted string or an identifier run up to `:`/`=`.
        let key = match chars.peek() {
            Some('"') | Some('\'') => {
                let delim = chars.next()?;
                let mut k = String::new();
                while let Some(c) = chars.next() {
                    if c == '\\' {
                        if let Some(esc) = chars.next() {
                            k.push(unescape(esc));
                        }
                        continue;
                    }
                    if c == delim {
                        break;
                    }
                    k.push(c);
                }
                k
            }
            _ => {
                let mut k = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ':' || c == '=' || c == ',' {
                        break;
                    }
                    chars.next();
                    k.push(c);
                }
                k.trim().to_string()
            }
        };

        if key.is_empty() {
            break;
        }

        // Consume the `:` or `=` separator.
        match chars.next() {
            Some(':') | Some('=') => {}
            _ => break,
        }

        // Skip whitespace.
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
            } else {
                break;
            }
        }

        let value = parse_kv_value(&mut chars)?;
        map.insert(key, value);
    }

    if map.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(map))
    }
}

/// Parse a single value: string, braced string, number, bool, or bare token.
fn parse_kv_value(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<serde_json::Value> {
    let first = *chars.peek()?;

    // Quoted string.
    if first == '"' || first == '\'' {
        let delim = chars.next()?;
        let mut val = String::new();
        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(esc) = chars.next() {
                    val.push(unescape(esc));
                }
                continue;
            }
            if c == delim {
                break;
            }
            val.push(c);
        }
        return Some(serde_json::Value::String(val));
    }

    // Braced value: `{"actual"}` or `{value}`.
    if first == '{' {
        chars.next();
        let mut val = String::new();
        let mut depth: i32 = 1;
        let mut in_str: Option<char> = None;
        while let Some(c) = chars.next() {
            if let Some(delim) = in_str {
                if c == '\\' {
                    if let Some(esc) = chars.next() {
                        val.push(unescape(esc));
                    }
                    continue;
                }
                if c == delim {
                    in_str = None;
                    continue;
                }
                val.push(c);
                continue;
            }
            if c == '"' || c == '\'' {
                in_str = Some(c);
                continue;
            }
            if c == '{' {
                depth += 1;
                val.push(c);
                continue;
            }
            if c == '}' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                val.push(c);
                continue;
            }
            val.push(c);
        }
        return Some(serde_json::Value::String(val.trim().to_string()));
    }

    // Unquoted token: read until a comma at the top level.
    let mut val = String::new();
    let mut depth: i32 = 0;
    while let Some(&c) = chars.peek() {
        if c == ',' && depth == 0 {
            break;
        }
        if c == '{' || c == '[' || c == '(' {
            depth += 1;
        }
        if c == '}' || c == ']' || c == ')' {
            if depth == 0 {
                break;
            }
            depth -= 1;
        }
        chars.next();
        val.push(c);
    }
    let val = val.trim().to_string();
    if val.is_empty() {
        return Some(serde_json::Value::String(String::new()));
    }
    if let Ok(n) = val.parse::<i64>() {
        return Some(serde_json::Value::Number(n.into()));
    }
    if let Ok(f) = val.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return Some(serde_json::Value::Number(n));
        }
    }
    if val == "true" {
        return Some(serde_json::Value::Bool(true));
    }
    if val == "false" {
        return Some(serde_json::Value::Bool(false));
    }
    if val == "null" {
        return Some(serde_json::Value::Null);
    }
    Some(serde_json::Value::String(val))
}

fn unescape(c: char) -> char {
    match c {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        '\\' => '\\',
        '"' => '"',
        '\'' => '\'',
        '/' => '/',
        '0' => '\0',
        other => other,
    }
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
    let tail = if text.len() > 80 { &text[text.len() - 80..] } else { text };
    // Raw Gemma tool-call patterns (no wrapper tags):
    //   call:tool_name{...}
    //   tool\ntool_name\ndone
    // These need to be held back so the raw syntax doesn't leak to UI.
    if tail.contains("call:") || tail.ends_with("cal") || tail.ends_with("call") {
        return true;
    }
    // "tool\n<name>\ndone" marker sequence from Gemma tool template.
    if tail.contains("\ntool\n") || tail.ends_with("\ntool") || tail.ends_with("tool\n") {
        return true;
    }
    tail.contains("<tool")
        || tail.contains("<func")
        || tail.contains("<|python")
        || tail.contains("<chan")
        || tail.ends_with('<')
        || tail.ends_with("<|")
        || (tail.rfind('<').is_some() && tail.rfind('<') > tail.rfind('>'))
}

fn extract_pre_tool_text(text: &str) -> String {
    // Return text before any tool call marker.
    for marker in ["<|tool_call>", "<tool_call>", "<functioncall>", "<|python_tag|>", "call:"] {
        if let Some(pos) = text.find(marker) {
            return text[..pos].to_string();
        }
    }
    // If it's a raw JSON tool call, return empty (the whole thing is the tool call).
    String::new()
}

/// Remove any tool-call wrapper tags from a string so raw protocol tokens
/// never appear in the user-visible output when parsing falls through.
fn strip_tool_markers(text: &str) -> String {
    let markers = [
        "<|tool_call|>", "<tool_call|>", "<|tool_call>", "<tool_call>",
        "</tool_call>", "<functioncall>", "</functioncall>",
        "<|python_tag|>", "<|channel>", "<channel|>", "<|channel|>", "</channel>",
        // Gemma special quote tokens rendered as literal text.
        "<|\"|>", "<|'|>",
    ];
    let mut out = text.to_string();
    for marker in markers {
        out = out.replace(marker, "");
    }
    // Also strip Gemma's raw "call:...{...}" and "tool\nname\ndone" patterns
    // in case they leak through (should be caught by parser but be defensive).
    // Remove any "call:toolname{...balanced...}" sequence.
    while let Some(start) = out.find("call:") {
        let after = &out[start..];
        if let Some(brace_start) = after.find('{') {
            let mut depth = 0i32;
            let mut end_idx: Option<usize> = None;
            for (i, &b) in after.as_bytes().iter().enumerate().skip(brace_start) {
                match b {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            end_idx = Some(i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if let Some(e) = end_idx {
                let absolute_end = start + e + 1;
                out.replace_range(start..absolute_end, "");
                continue;
            }
        }
        break;
    }
    // Strip "tool\n<word>\ndone" sequences.
    let re_patterns = ["\ntool\n", "tool\n"];
    for _ in 0..3 {
        for p in &re_patterns {
            if let Some(start) = out.find(p) {
                let after = &out[start + p.len()..];
                if let Some(done_rel) = after.find("done") {
                    let end = start + p.len() + done_rel + "done".len();
                    out.replace_range(start..end, "");
                }
            }
        }
    }
    out
}

// ── Anthropic API provider ──

/// Auth method for Anthropic API.
#[derive(Clone)]
pub enum AnthropicAuth {
    ApiKey(String),
    OAuth, // uses auth::get_valid_token() for each request
}

/// Spawn a background thread that serves requests via the Anthropic Messages API.
pub fn spawn_anthropic_thread(
    auth: AnthropicAuth,
    model: &str,
) -> Result<InferenceHandle, String> {
    let (tx, rx) = mpsc::channel::<InferenceRequest>();
    let model = model.to_string();
    let model_name = model.clone();

    std::thread::Builder::new()
        .name("forge-anthropic".into())
        .spawn(move || {
            eprintln!("[forge-api] Anthropic thread started, model: {model}");
            for req in rx.iter() {
                // Get auth header for each request (OAuth tokens may refresh).
                let (header_name, header_value) = match &auth {
                    AnthropicAuth::ApiKey(key) => ("x-api-key".to_string(), key.clone()),
                    AnthropicAuth::OAuth => {
                        match crate::auth::get_auth_header() {
                            Ok(header) => header,
                            Err(e) => {
                                let _ = req.response_tx.send(InferenceEvent::Error(
                                    format!("OAuth token error: {e}. Run login flow again.")
                                ));
                                let _ = req.response_tx.send(InferenceEvent::Done);
                                continue;
                            }
                        }
                    }
                };
                anthropic_request(&header_name, &header_value, &model, &req);
            }
        })
        .map_err(|e| format!("Failed to spawn Anthropic thread: {e}"))?;

    Ok(InferenceHandle { tx, model_name })
}

fn anthropic_request(
    auth_header: &str,
    auth_value: &str,
    model: &str,
    req: &InferenceRequest,
) {
    let tx = &req.response_tx;

    // Build request body.
    let mut messages_json = Vec::new();
    let mut system_prompt = String::new();

    for msg in &req.messages {
        match msg.role {
            ChatRole::System => {
                system_prompt = msg.content.clone();
                continue;
            }
            ChatRole::User => {
                messages_json.push(serde_json::json!({
                    "role": "user",
                    "content": msg.content
                }));
            }
            ChatRole::Assistant => {
                if !msg.tool_calls.is_empty() {
                    let mut content = Vec::new();
                    if !msg.content.is_empty() {
                        content.push(serde_json::json!({"type": "text", "text": msg.content}));
                    }
                    for tc in &msg.tool_calls {
                        content.push(serde_json::json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.name,
                            "input": tc.arguments
                        }));
                    }
                    messages_json.push(serde_json::json!({"role": "assistant", "content": content}));
                } else {
                    messages_json.push(serde_json::json!({
                        "role": "assistant",
                        "content": msg.content
                    }));
                }
            }
            ChatRole::Tool => {
                messages_json.push(serde_json::json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": msg.tool_call_id.as_deref().unwrap_or(""),
                        "content": msg.content
                    }]
                }));
            }
        }
    }

    // Build tools array for the API.
    let api_tools: Vec<serde_json::Value> = req.tools.iter().filter_map(|t| {
        let func = t.get("function")?;
        Some(serde_json::json!({
            "name": func.get("name")?,
            "description": func.get("description")?,
            "input_schema": func.get("parameters")?
        }))
    }).collect();

    let mut body = serde_json::json!({
        "model": model,
        "max_tokens": 4096,
        "stream": true,
        "messages": messages_json
    });

    if !system_prompt.is_empty() {
        body["system"] = serde_json::json!(system_prompt);
    }
    if !api_tools.is_empty() {
        body["tools"] = serde_json::json!(api_tools);
    }

    // Make the streaming request.
    let resp = match ureq::post("https://api.anthropic.com/v1/messages")
        .set(auth_header, auth_value)
        .set("anthropic-version", "2023-06-01")
        .set("content-type", "application/json")
        .send_json(body)
    {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.send(InferenceEvent::Error(format!("API request failed: {e}")));
            let _ = tx.send(InferenceEvent::Done);
            return;
        }
    };

    // Parse SSE stream.
    let buf_reader = std::io::BufReader::new(resp.into_reader());

    use std::io::BufRead;
    let mut current_tool_id = String::new();
    let mut current_tool_name = String::new();
    let mut current_tool_json = String::new();
    let mut in_tool_use = false;

    for line in buf_reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if !line.starts_with("data: ") {
            continue;
        }
        let data = &line["data: ".len()..];
        if data == "[DONE]" {
            break;
        }

        let event: serde_json::Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match event_type {
            "content_block_start" => {
                let block = event.get("content_block").unwrap_or(&serde_json::Value::Null);
                let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if block_type == "tool_use" {
                    in_tool_use = true;
                    current_tool_id = block.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    current_tool_name = block.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    current_tool_json.clear();
                } else if block_type == "thinking" {
                    // Extended thinking block starts.
                }
            }
            "content_block_delta" => {
                let delta = event.get("delta").unwrap_or(&serde_json::Value::Null);
                let delta_type = delta.get("type").and_then(|t| t.as_str()).unwrap_or("");

                match delta_type {
                    "text_delta" => {
                        if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                            let _ = tx.send(InferenceEvent::Token(text.to_string()));
                        }
                    }
                    "thinking_delta" => {
                        if let Some(text) = delta.get("thinking").and_then(|t| t.as_str()) {
                            let _ = tx.send(InferenceEvent::Thinking(text.to_string()));
                        }
                    }
                    "input_json_delta" => {
                        if let Some(json) = delta.get("partial_json").and_then(|t| t.as_str()) {
                            current_tool_json.push_str(json);
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                if in_tool_use {
                    let arguments: serde_json::Value = serde_json::from_str(&current_tool_json)
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                    let _ = tx.send(InferenceEvent::ToolUse(ToolCall {
                        id: current_tool_id.clone(),
                        name: current_tool_name.clone(),
                        arguments,
                    }));
                    in_tool_use = false;
                }
            }
            "message_stop" => {
                break;
            }
            "error" => {
                let msg = event.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown API error");
                let _ = tx.send(InferenceEvent::Error(msg.to_string()));
                break;
            }
            _ => {}
        }
    }

    let _ = tx.send(InferenceEvent::Done);
}

// ── GitHub Copilot ─────────────────────────────────────────────────────

/// Spawn a background thread that serves requests via the Copilot
/// /chat/completions endpoint (OpenAI-compatible). Tool calls are routed
/// through the shared openai chat-stream helper, so `tool_calls` deltas
/// from Copilot become real `InferenceEvent::ToolUse` events.
/// Token refresh is handled per-request by copilot::get_copilot_token.
pub fn spawn_copilot_thread(model: &str) -> Result<InferenceHandle, String> {
    let (tx, rx) = mpsc::channel::<InferenceRequest>();
    let model = model.to_string();
    let model_name = model.clone();

    std::thread::Builder::new()
        .name("forge-copilot".into())
        .spawn(move || {
            eprintln!("[forge-copilot] thread started, model: {model}");
            for req in rx.iter() {
                let resp_tx = req.response_tx.clone();
                let token = match crate::copilot::get_copilot_token() {
                    Ok(t) => t,
                    Err(e) => {
                        let _ = resp_tx.send(InferenceEvent::Error(format!("copilot token: {e}")));
                        let _ = resp_tx.send(InferenceEvent::Done);
                        continue;
                    }
                };
                // Copilot expects vscode-style editor headers + intent.
                let extra_headers: Vec<(&str, String)> = vec![
                    ("Editor-Version", "vscode/1.95.0".into()),
                    ("Editor-Plugin-Version", "copilot-chat/0.22.0".into()),
                    ("Copilot-Integration-Id", "vscode-chat".into()),
                    ("OpenAI-Intent", "conversation-panel".into()),
                ];
                let cfg = crate::openai::ChatRequestConfig {
                    url: "https://api.githubcopilot.com/chat/completions",
                    auth_bearer: &token,
                    model: &model,
                    extra_headers: &extra_headers,
                };
                crate::openai::run_chat_stream(&cfg, &req, &resp_tx);
            }
        })
        .map_err(|e| format!("Failed to spawn Copilot thread: {e}"))?;

    Ok(InferenceHandle::from_sender(tx, model_name))
}

// ── OpenAI / OpenAI-compat / Gemini ──────────────────────────────────

/// Spawn a chat thread that talks to OpenAI directly.
pub fn spawn_openai_thread(api_key: String, model: String) -> Result<InferenceHandle, String> {
    crate::openai::spawn_thread(api_key, None, model)
}

/// Spawn a chat thread for any OpenAI-compatible endpoint (OpenRouter,
/// Ollama, LM Studio, llama-server, Jan, etc.).
pub fn spawn_openai_compat_thread(
    api_key: String,
    base_url: String,
    model: String,
) -> Result<InferenceHandle, String> {
    crate::openai::spawn_thread(api_key, Some(base_url), model)
}

/// Spawn a chat thread that talks to Google Gemini's generateContent
/// streaming endpoint.
pub fn spawn_gemini_thread(api_key: String, model: String) -> Result<InferenceHandle, String> {
    crate::gemini::spawn_thread(api_key, model)
}

// ── Provider catalogue + capability discovery (Phase 3) ────────────────
//
// Public API: every supported provider gets a `test_<provider>` Tauri
// command that validates auth, lists models, and reports capability
// metadata. Results are cached for 24 h in the user's config dir under
// `provider-models.json` so settings reopens don't re-hit endpoints.

/// Per-model capability metadata used by the routing slot picker and the
/// context-window planner. Defaults are intentionally permissive on
/// `supports_tools` because most modern chat models do support tool use.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderCapabilities {
    #[serde(default)]
    pub context_window: u32,
    #[serde(default)]
    pub max_output: u32,
    #[serde(default)]
    pub tokenizer_kind: String,
    #[serde(default)]
    pub supports_caching: bool,
    #[serde(default)]
    pub supports_tools: bool,
    #[serde(default)]
    pub supports_vision: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderModel {
    pub id: String,
    pub display_name: String,
    pub capabilities: ProviderCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderTestResult {
    pub ok: bool,
    pub error: Option<String>,
    pub models: Vec<ProviderModel>,
}

const CACHE_FILENAME: &str = "provider-models.json";
const CACHE_TTL_SECS: u64 = 24 * 60 * 60;
const ANTHROPIC_TIMEOUT_SECS: u64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedModels {
    ts: u64,
    models: Vec<ProviderModel>,
}

fn cache_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("forge")
        .join(CACHE_FILENAME)
}

fn cache_key(provider: &str, base_url: Option<&str>) -> String {
    format!("{}|{}", provider, base_url.unwrap_or(""))
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn read_cache() -> std::collections::HashMap<String, CachedModels> {
    std::fs::read_to_string(cache_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_cache(map: &std::collections::HashMap<String, CachedModels>) {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(s) = serde_json::to_string_pretty(map) {
        let _ = std::fs::write(&path, s);
    }
}

fn cache_get(provider: &str, base_url: Option<&str>) -> Option<Vec<ProviderModel>> {
    let map = read_cache();
    let entry = map.get(&cache_key(provider, base_url))?;
    if now_secs().saturating_sub(entry.ts) < CACHE_TTL_SECS {
        Some(entry.models.clone())
    } else {
        None
    }
}

fn cache_put(provider: &str, base_url: Option<&str>, models: &[ProviderModel]) {
    let mut map = read_cache();
    map.insert(
        cache_key(provider, base_url),
        CachedModels { ts: now_secs(), models: models.to_vec() },
    );
    write_cache(&map);
}

// Anthropic does not publish per-model context windows in /v1/models, so we
// keep a hardcoded mapping for the families we care about and fall back to
// 200k for anything unrecognised.
// HARDCODED: Anthropic /v1/models lacks capability fields.
fn anthropic_caps_for_id(id: &str) -> ProviderCapabilities {
    let lower = id.to_ascii_lowercase();
    let (ctx, max_out) = if lower.contains("opus") {
        (200_000, 32_000)
    } else if lower.contains("haiku") {
        (200_000, 8_192)
    } else if lower.contains("sonnet") {
        (200_000, 64_000)
    } else {
        (200_000, 8_192)
    };
    ProviderCapabilities {
        context_window: ctx,
        max_output: max_out,
        tokenizer_kind: "claude".into(),
        supports_caching: true,
        supports_tools: true,
        supports_vision: true,
    }
}

fn anthropic_list(api_key: &str, base_url: Option<&str>) -> Result<Vec<ProviderModel>, String> {
    let base = base_url
        .unwrap_or("https://api.anthropic.com")
        .trim_end_matches('/');
    let url = format!("{base}/v1/models");

    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(ANTHROPIC_TIMEOUT_SECS))
        .build();
    let resp = agent
        .get(&url)
        .set("x-api-key", api_key)
        .set("anthropic-version", "2023-06-01")
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

    let v: serde_json::Value = resp.into_json().map_err(|e| format!("Parse: {e}"))?;
    let arr = v.get("data").and_then(|d| d.as_array()).ok_or("missing data array")?;
    let mut out = Vec::new();
    for m in arr {
        let id = match m.get("id").and_then(|s| s.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let display = m
            .get("display_name")
            .and_then(|s| s.as_str())
            .unwrap_or(&id)
            .to_string();
        out.push(ProviderModel {
            id: id.clone(),
            display_name: display,
            capabilities: anthropic_caps_for_id(&id),
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

fn copilot_caps_default() -> ProviderCapabilities {
    ProviderCapabilities {
        context_window: 128_000,
        max_output: 16_384,
        tokenizer_kind: "tiktoken_o200k".into(),
        supports_caching: false,
        supports_tools: true,
        supports_vision: true,
    }
}

fn copilot_list() -> Result<Vec<ProviderModel>, String> {
    // Reuse the existing copilot.rs OAuth flow + /models endpoint.
    let raw = crate::copilot::list_models()?;
    let mut out = Vec::with_capacity(raw.len());
    for m in raw {
        let display = if m.vendor.is_empty() {
            m.name.clone()
        } else {
            format!("{} ({})", m.name, m.vendor)
        };
        out.push(ProviderModel {
            id: m.id,
            display_name: display,
            capabilities: copilot_caps_default(),
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

// ── Tauri commands ────────────────────────────────────────────────────

#[tauri::command]
pub fn test_anthropic(
    api_key: String,
    base_url: Option<String>,
) -> Result<ProviderTestResult, String> {
    if api_key.trim().is_empty() {
        return Ok(ProviderTestResult { ok: false, error: Some("API key empty".into()), models: vec![] });
    }
    match anthropic_list(&api_key, base_url.as_deref()) {
        Ok(models) => {
            cache_put("anthropic", base_url.as_deref(), &models);
            Ok(ProviderTestResult { ok: true, error: None, models })
        }
        Err(e) => Ok(ProviderTestResult { ok: false, error: Some(e), models: vec![] }),
    }
}

#[tauri::command]
pub fn test_openai(
    api_key: String,
    base_url: Option<String>,
) -> Result<ProviderTestResult, String> {
    if api_key.trim().is_empty() {
        return Ok(ProviderTestResult { ok: false, error: Some("API key empty".into()), models: vec![] });
    }
    match crate::openai::list_models(&api_key, base_url.as_deref()) {
        Ok(models) => {
            cache_put("openai", base_url.as_deref(), &models);
            Ok(ProviderTestResult { ok: true, error: None, models })
        }
        Err(e) => Ok(ProviderTestResult { ok: false, error: Some(e), models: vec![] }),
    }
}

#[tauri::command]
pub fn test_gemini(api_key: String) -> Result<ProviderTestResult, String> {
    if api_key.trim().is_empty() {
        return Ok(ProviderTestResult { ok: false, error: Some("API key empty".into()), models: vec![] });
    }
    match crate::gemini::list_models(&api_key) {
        Ok(models) => {
            cache_put("gemini", None, &models);
            Ok(ProviderTestResult { ok: true, error: None, models })
        }
        Err(e) => Ok(ProviderTestResult { ok: false, error: Some(e), models: vec![] }),
    }
}

#[tauri::command]
pub fn test_copilot() -> Result<ProviderTestResult, String> {
    if !crate::copilot::is_signed_in() {
        return Ok(ProviderTestResult {
            ok: false,
            error: Some("Not signed in to Copilot".into()),
            models: vec![],
        });
    }
    match copilot_list() {
        Ok(models) => {
            cache_put("copilot", None, &models);
            Ok(ProviderTestResult { ok: true, error: None, models })
        }
        Err(e) => Ok(ProviderTestResult { ok: false, error: Some(e), models: vec![] }),
    }
}

#[tauri::command]
pub fn test_openai_compat(
    api_key: String,
    base_url: String,
) -> Result<ProviderTestResult, String> {
    if base_url.trim().is_empty() {
        return Ok(ProviderTestResult {
            ok: false,
            error: Some("Base URL empty".into()),
            models: vec![],
        });
    }
    match crate::openai_compat::list_models(&api_key, &base_url) {
        Ok(models) => {
            cache_put("openai_compat", Some(&base_url), &models);
            Ok(ProviderTestResult { ok: true, error: None, models })
        }
        Err(e) => Ok(ProviderTestResult { ok: false, error: Some(e), models: vec![] }),
    }
}

/// Cache-first model fetch. Hits the network only when the cache is missing
/// or older than 24 h, so opening AI settings is instant in the common case.
#[tauri::command]
pub fn list_provider_models(
    provider: String,
    api_key: Option<String>,
    base_url: Option<String>,
) -> Result<Vec<ProviderModel>, String> {
    if let Some(cached) = cache_get(&provider, base_url.as_deref()) {
        return Ok(cached);
    }
    match provider.as_str() {
        "anthropic" => {
            let key = api_key.ok_or("anthropic requires api_key")?;
            anthropic_list(&key, base_url.as_deref()).inspect(|m| cache_put("anthropic", base_url.as_deref(), m))
        }
        "openai" => {
            let key = api_key.ok_or("openai requires api_key")?;
            crate::openai::list_models(&key, base_url.as_deref())
                .inspect(|m| cache_put("openai", base_url.as_deref(), m))
        }
        "gemini" => {
            let key = api_key.ok_or("gemini requires api_key")?;
            crate::gemini::list_models(&key).inspect(|m| cache_put("gemini", None, m))
        }
        "copilot" => copilot_list().inspect(|m| cache_put("copilot", None, m)),
        "openai_compat" => {
            let url = base_url.ok_or("openai_compat requires base_url")?;
            crate::openai_compat::list_models(api_key.as_deref().unwrap_or(""), &url)
                .inspect(|m| cache_put("openai_compat", Some(&url), m))
        }
        other => Err(format!("unknown provider: {other}")),
    }
}
