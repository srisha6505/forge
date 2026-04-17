//! Tauri command handlers. Each function is a thin wrapper that exposes
//! a business-logic call to the React frontend. Errors are converted to
//! `Result<T, String>` because Tauri serialises strings over IPC well and
//! the frontend just renders them as error toasts.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, State, Window};

use crate::{llm, settings::Settings, AppState};

// ── Settings ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    Ok(state.settings.lock().unwrap().clone())
}

#[tauri::command]
pub fn set_settings(state: State<'_, AppState>, new: Settings) -> Result<(), String> {
    let mut s = state.settings.lock().unwrap();
    *s = new;
    s.save();
    Ok(())
}

// ── Vault ───────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VaultEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TreeNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub children: Vec<TreeNode>,
}

#[tauri::command]
pub fn current_vault(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state
        .vault_path
        .lock()
        .unwrap()
        .as_ref()
        .map(|p| p.to_string_lossy().to_string()))
}

#[tauri::command]
pub fn open_vault(state: State<'_, AppState>, path: String) -> Result<Vec<VaultEntry>, String> {
    let pb = PathBuf::from(&path);
    if !pb.is_dir() {
        return Err(format!("Not a directory: {path}"));
    }
    *state.vault_path.lock().unwrap() = Some(pb.clone());
    {
        let mut settings = state.settings.lock().unwrap();
        settings.set_vault(&pb);
    }
    list_vault_files(state, Some(path))
}

#[tauri::command]
pub fn list_vault_tree(state: State<'_, AppState>) -> Result<TreeNode, String> {
    let vault = state
        .vault_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "No vault open".to_string())?;
    build_tree(&vault).map_err(|e| e.to_string())
}

fn build_tree(root: &Path) -> std::io::Result<TreeNode> {
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.to_string_lossy().to_string());
    let mut children = Vec::new();
    if root.is_dir() {
        let mut entries: Vec<_> = fs::read_dir(root)?
            .filter_map(|r| r.ok())
            .collect();
        entries.sort_by_key(|e| {
            let is_dir = e.path().is_dir();
            let name = e.file_name().to_string_lossy().to_lowercase();
            (!is_dir, name)
        });
        for dent in entries {
            let p = dent.path();
            let n = dent.file_name().to_string_lossy().to_string();
            if n.starts_with('.') {
                continue;
            }
            if p.is_dir() {
                if let Ok(child) = build_tree(&p) {
                    if !child.children.is_empty() {
                        children.push(child);
                    }
                }
            } else {
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "md" || ext == "markdown" {
                    children.push(TreeNode {
                        name: n,
                        path: p.to_string_lossy().to_string(),
                        is_dir: false,
                        children: Vec::new(),
                    });
                }
            }
        }
    }
    Ok(TreeNode {
        name,
        path: root.to_string_lossy().to_string(),
        is_dir: root.is_dir(),
        children,
    })
}

#[tauri::command]
pub fn list_vault_files(
    state: State<'_, AppState>,
    sub_path: Option<String>,
) -> Result<Vec<VaultEntry>, String> {
    let vault = state
        .vault_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "No vault open".to_string())?;
    let base = match sub_path {
        Some(p) => PathBuf::from(p),
        None => vault.clone(),
    };
    let base = resolve_within_vault(&vault, &base)?;
    let mut entries = Vec::new();
    for dent in fs::read_dir(&base).map_err(|e| e.to_string())? {
        let dent = dent.map_err(|e| e.to_string())?;
        let path = dent.path();
        let name = dent.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let is_dir = path.is_dir();
        // Only surface markdown files + directories to the UI.
        if !is_dir {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "md" && ext != "markdown" {
                continue;
            }
        }
        entries.push(VaultEntry {
            name,
            path: path.to_string_lossy().to_string(),
            is_dir,
        });
    }
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    Ok(entries)
}

// ── File IO ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn read_file(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let vault = state
        .vault_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "No vault open".to_string())?;
    let pb = resolve_within_vault(&vault, &PathBuf::from(&path))?;
    fs::read_to_string(&pb).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_file(
    state: State<'_, AppState>,
    path: String,
    content: String,
) -> Result<(), String> {
    let vault = state
        .vault_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "No vault open".to_string())?;
    let pb = PathBuf::from(&path);
    let full = if pb.is_absolute() {
        resolve_within_vault(&vault, &pb)?
    } else {
        vault.join(&pb)
    };
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let vault_canon = vault.canonicalize().map_err(|e| e.to_string())?;
    let parent_canon = full
        .parent()
        .ok_or_else(|| "Invalid path".to_string())?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !parent_canon.starts_with(&vault_canon) {
        return Err("Path escapes vault".into());
    }
    fs::write(&full, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_file(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> Result<(), String> {
    let vault = state
        .vault_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "No vault open".to_string())?;
    let from_pb = resolve_within_vault(&vault, &PathBuf::from(&from))?;
    let to_pb = vault.join(&to);
    if let Some(parent) = to_pb.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let vault_canon = vault.canonicalize().map_err(|e| e.to_string())?;
    let to_parent_canon = to_pb
        .parent()
        .ok_or_else(|| "Invalid target path".to_string())?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !to_parent_canon.starts_with(&vault_canon) {
        return Err("Target path escapes vault".into());
    }
    fs::rename(&from_pb, &to_pb).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_file(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let vault = state
        .vault_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "No vault open".to_string())?;
    let pb = resolve_within_vault(&vault, &PathBuf::from(&path))?;
    if pb.is_dir() {
        fs::remove_dir_all(&pb).map_err(|e| e.to_string())
    } else {
        fs::remove_file(&pb).map_err(|e| e.to_string())
    }
}

// ── Search ──────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SearchHit {
    pub path: String,
    pub title: String,
    pub heading: String,
    pub snippet: String,
    pub score: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SearchStatus {
    pub indexed: bool,
    pub chunk_count: usize,
    pub vectors_available: bool,
}

fn search_paths() -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let cfg_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("forge");
    fs::create_dir_all(&cfg_dir).map_err(|e| e.to_string())?;
    let db_path = cfg_dir.join("search.db");
    let index_path = cfg_dir.join("search.usearch");
    Ok((cfg_dir, db_path, index_path))
}

/// Build (or rebuild) the vault search index from scratch. Slow on first
/// call due to embedding model download + content scan. Subsequent calls
/// reuse the on-disk SQLite + usearch index. Wrapped in `catch_unwind`
/// because the embedder / hf-hub stack has been observed to panic on
/// network or ONNX runtime errors; we'd rather surface a string error
/// to the UI than crash the whole Tauri app.
#[tauri::command]
pub fn reindex_vault(state: State<'_, AppState>) -> Result<SearchStatus, String> {
    let vault = state
        .vault_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "No vault open".to_string())?;
    let (_cfg_dir, db_path, index_path) = search_paths()?;
    let search_arc = std::sync::Arc::clone(&state.search);

    // Build index WITHOUT holding the mutex so a panic can't poison the
    // lock and so other search calls can access the old index while
    // rebuild is in progress.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut vs = crate::search::VaultSearch::new(&db_path, &index_path)
            .map_err(|e| format!("open index: {e}"))?;
        vs.build_vault(&vault)
            .map_err(|e| format!("build vault: {e}"))?;
        vs.save_index(&index_path)
            .map_err(|e| format!("save index: {e}"))?;
        let status = SearchStatus {
            indexed: true,
            chunk_count: vs.chunk_count(),
            vectors_available: vs.vectors_available(),
        };
        // Only swap in under the lock — if this panics the mutex would
        // poison but the new vs is already saved to disk so next call
        // will recover.
        let mut guard = search_arc.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(vs);
        Ok::<SearchStatus, String>(status)
    }));

    match result {
        Ok(Ok(s)) => Ok(s),
        Ok(Err(e)) => Err(e),
        Err(panic) => {
            let msg = if let Some(s) = panic.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic during reindex".to_string()
            };
            Err(format!("Reindex panicked: {msg}"))
        }
    }
}

#[tauri::command]
pub fn search_status(state: State<'_, AppState>) -> Result<SearchStatus, String> {
    let guard = state.search.lock().unwrap();
    Ok(match guard.as_ref() {
        Some(vs) => SearchStatus {
            indexed: vs.chunk_count() > 0,
            chunk_count: vs.chunk_count(),
            vectors_available: vs.vectors_available(),
        },
        None => SearchStatus {
            indexed: false,
            chunk_count: 0,
            vectors_available: false,
        },
    })
}

#[tauri::command]
pub fn search_vault(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<SearchHit>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let vault = state
        .vault_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "No vault open".to_string())?;
    let (_cfg_dir, db_path, index_path) = search_paths()?;
    let search_arc = std::sync::Arc::clone(&state.search);

    // Check if search needs initialization (peek without holding lock long).
    let needs_init = {
        let guard = search_arc.lock().unwrap_or_else(|e| e.into_inner());
        guard.is_none()
    };

    if needs_init {
        // Build VaultSearch OUTSIDE the mutex to avoid poisoning on panic.
        let init_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut vs = crate::search::VaultSearch::new(&db_path, &index_path)
                .map_err(|e| format!("search init failed: {e}"))?;
            if vs.chunk_count() == 0 {
                vs.build_vault(&vault)
                    .map_err(|e| format!("vault index build failed: {e}"))?;
                vs.save_index(&index_path)
                    .map_err(|e| format!("save index failed: {e}"))?;
            }
            Ok::<_, String>(vs)
        }));

        match init_result {
            Ok(Ok(vs)) => {
                let mut guard = search_arc.lock().unwrap_or_else(|e| e.into_inner());
                *guard = Some(vs);
            }
            Ok(Err(e)) => return Err(e),
            Err(panic) => {
                let msg = if let Some(s) = panic.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "search init panicked".to_string()
                };
                return Err(format!("Search init failed: {msg}"));
            }
        }
    }

    let guard = search_arc.lock().unwrap_or_else(|e| e.into_inner());
    let vs = match guard.as_ref() {
        Some(vs) => vs,
        None => return Err("Search not initialized".into()),
    };
    let results = vs.search(trimmed, limit.unwrap_or(20))
        .map_err(|e| format!("search failed: {e}"))?;

    Ok(results
        .into_iter()
        .map(|r| {
            let title = r
                .chunk
                .file_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            // UTF-8-safe truncation: clamp to char boundary, not byte boundary.
            let snippet = if r.chunk.content.chars().count() > 240 {
                let mut end = 0usize;
                for (i, _) in r.chunk.content.char_indices().take(240) {
                    end = i;
                }
                format!("{}…", r.chunk.content[..end].trim_end())
            } else {
                r.chunk.content.clone()
            };
            SearchHit {
                path: r.chunk.file_path.to_string_lossy().to_string(),
                title,
                heading: r.chunk.heading,
                snippet,
                score: r.score,
            }
        })
        .collect())
}

// ── Inference / chat ────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConnectResult {
    pub model_name: String,
}

#[tauri::command]
pub fn connect_inference(
    state: State<'_, AppState>,
) -> Result<ConnectResult, String> {
    let settings = state.settings.lock().unwrap().clone();
    let provider = settings.ai_provider.as_str();

    let handle = if provider == "anthropic" || provider == "claude" {
        let auth = if let Some(key) = &settings.api_key {
            llm::AnthropicAuth::ApiKey(key.clone())
        } else {
            return Err("No Anthropic credentials configured".into());
        };
        llm::spawn_anthropic_thread(auth, &settings.api_model)
            .map_err(|e| e.to_string())?
    } else {
        let path = settings
            .model_path
            .clone()
            .ok_or_else(|| "No model_path set in settings".to_string())?;
        llm::spawn_inference_thread(&path, settings.gpu_layers, settings.ctx_size)
            .map_err(|e| e.to_string())?
    };

    let name = handle.model_name.clone();
    *state.inference.lock().unwrap() = Some(handle);
    Ok(ConnectResult { model_name: name })
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatTurn {
    pub role: String,
    pub content: String,
}

/// Fire-and-forget: spawns a background thread that runs the agent loop
/// and emits `chat://token` / `chat://tool` / `chat://done` events as it
/// progresses. The frontend subscribes and renders the stream.
#[tauri::command]
pub fn send_chat_message(
    state: State<'_, AppState>,
    window: Window,
    history: Vec<ChatTurn>,
) -> Result<(), String> {
    let inference = state
        .inference
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Inference not connected. Call connect_inference first.".to_string())?;

    let vault = state
        .vault_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "No vault open".to_string())?;

    let db_path = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("forge")
        .join("forge.db");

    // Paths for the shared vault search index used by the agent's
    // `search_vault` tool. Same files the search panel uses.
    let (_cfg, search_db_path, search_index_path) = search_paths()?;
    let search_arc = std::sync::Arc::clone(&state.search);

    // Convert frontend history to LLM ChatMessages. The first turn must be
    // the system prompt; if not present we synthesise one.
    let mut messages: Vec<llm::ChatMessage> = Vec::with_capacity(history.len() + 1);
    let vault_name = vault
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "vault".into());
    messages.push(llm::ChatMessage::system(crate::agent::default_system_prompt(&vault_name)));
    for turn in history {
        match turn.role.as_str() {
            "user" => messages.push(llm::ChatMessage::user(&turn.content)),
            "assistant" => messages.push(llm::ChatMessage::assistant(&turn.content)),
            _ => {}
        }
    }

    let tools = crate::agent::tool_schemas();
    let max_iters = state.settings.lock().unwrap().max_tool_iterations;

    std::thread::Builder::new()
        .name("forge-agent".into())
        .spawn(move || {
            let (tx, rx) = std::sync::mpsc::channel::<crate::agent::AgentEvent>();
            let ctx = crate::agent::ToolContext {
                vault_path: vault,
                db_path,
                search: search_arc,
                search_db_path,
                search_index_path,
            };
            let mut msgs = messages;

            // Run the agent in a nested thread so we can forward events.
            let agent_thread = std::thread::spawn(move || {
                crate::agent::run_agent_loop(&inference, &mut msgs, &tools, &ctx, max_iters, &tx);
            });

            for event in rx.iter() {
                let _ = forward_agent_event(&window, &event);
                // Notify the frontend after any vault-modifying tool runs so
                // the sidebar tree can refresh without manual reload.
                if let crate::agent::AgentEvent::ToolCallResult { name, is_error, .. } = &event {
                    if !is_error
                        && matches!(
                            name.as_str(),
                            "write_file" | "edit_file" | "rename_file" | "delete_file"
                        )
                    {
                        let _ = window.emit("vault://changed", ());
                    }
                }
                if matches!(event, crate::agent::AgentEvent::Finished { .. } | crate::agent::AgentEvent::Error(_)) {
                    break;
                }
            }
            let _ = agent_thread.join();
        })
        .map_err(|e| e.to_string())?;

    Ok(())
}

fn forward_agent_event(window: &Window, event: &crate::agent::AgentEvent) -> tauri::Result<()> {
    use crate::agent::AgentEvent;
    match event {
        AgentEvent::Token(t) => window.emit("chat://token", t.clone()),
        AgentEvent::Thinking(t) => window.emit("chat://thinking", t.clone()),
        AgentEvent::ToolCallStarted { name, args } => window.emit(
            "chat://tool-start",
            serde_json::json!({ "name": name, "args": args }),
        ),
        AgentEvent::ToolCallResult { name, content, is_error } => window.emit(
            "chat://tool-result",
            serde_json::json!({ "name": name, "content": content, "is_error": is_error }),
        ),
        AgentEvent::Finished { .. } => window.emit("chat://done", ()),
        AgentEvent::Error(msg) => window.emit("chat://error", msg.clone()),
    }
}

// ── Speech-to-text ────────────────────────────────────────────────────

fn ensure_whisper_loaded(state: &State<'_, AppState>) -> Result<(), String> {
    let settings = state.settings.lock().unwrap().clone();
    let model_path = settings
        .whisper_model_path
        .clone()
        .ok_or_else(|| "No whisper_model_path in settings".to_string())?;
    let whisper = std::sync::Arc::clone(&state.whisper);
    let needs_init = {
        let g = whisper.0.lock().unwrap_or_else(|e| e.into_inner());
        g.is_none()
    };
    if !needs_init { return Ok(()); }
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::stt::Whisper::new(&model_path)
    }));
    match built {
        Ok(Ok(w)) => {
            let mut g = whisper.0.lock().unwrap_or_else(|e| e.into_inner());
            *g = Some(w);
            Ok(())
        }
        Ok(Err(e)) => Err(e),
        Err(_) => Err("Whisper init panicked".into()),
    }
}

/// Start capturing mic audio via cpal. Stored in state until stop.
#[tauri::command]
pub fn start_recording(state: State<'_, AppState>) -> Result<(), String> {
    let mic = std::sync::Arc::clone(&state.mic);
    let mut g = mic.0.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_some() { return Err("Already recording".into()); }
    let s = crate::stt::MicSession::start()?;
    *g = Some(s);
    Ok(())
}

#[tauri::command]
pub fn stop_recording_and_transcribe(state: State<'_, AppState>) -> Result<String, String> {
    let settings = state.settings.lock().unwrap().clone();
    let language = settings.whisper_language.clone();

    let mic = std::sync::Arc::clone(&state.mic);
    let sess = {
        let mut g = mic.0.lock().unwrap_or_else(|e| e.into_inner());
        g.take().ok_or_else(|| "Not recording".to_string())?
    };
    let (raw, rate, channels) = sess.stop_and_take()?;
    if raw.is_empty() { return Ok(String::new()); }
    let pcm16 = crate::stt::to_mono_16khz(&raw, rate, channels);

    ensure_whisper_loaded(&state)?;
    let guard = state.whisper.0.lock().unwrap_or_else(|e| e.into_inner());
    let w = guard.as_ref().ok_or("whisper not loaded")?;
    let lang = if language.is_empty() || language == "auto" { None } else { Some(language.as_str()) };
    w.transcribe(&pcm16, lang)
}

/// Legacy: accept WAV bytes from frontend (unused now, kept for fallback).
#[tauri::command]
pub fn transcribe_audio(state: State<'_, AppState>, wav_bytes: Vec<u8>) -> Result<String, String> {
    let settings = state.settings.lock().unwrap().clone();
    let language = settings.whisper_language.clone();
    let pcm = crate::stt::decode_wav(&wav_bytes)?;
    if pcm.is_empty() { return Ok(String::new()); }
    ensure_whisper_loaded(&state)?;
    let guard = state.whisper.0.lock().unwrap_or_else(|e| e.into_inner());
    let w = guard.as_ref().ok_or("whisper not loaded")?;
    let lang = if language.is_empty() || language == "auto" { None } else { Some(language.as_str()) };
    w.transcribe(&pcm, lang)
}

// ── Voice conversation mode ────────────────────────────────────────────

fn home_path(p: &str) -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp")).join(p)
}

#[tauri::command]
pub fn voice_start(state: State<'_, AppState>, window: Window) -> Result<(), String> {
    voice_start_impl(state, window, false)
}

#[tauri::command]
pub fn voice_start_wake(state: State<'_, AppState>, window: Window) -> Result<(), String> {
    voice_start_impl(state, window, true)
}

fn voice_start_impl(state: State<'_, AppState>, window: Window, use_wake: bool) -> Result<(), String> {
    let settings = state.settings.lock().unwrap().clone();

    let whisper_bin = std::env::var("FORGE_WHISPER_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_path(".forge/bin/whisper-cli"));
    let whisper_model = settings.whisper_model_path.clone()
        .ok_or_else(|| "whisper_model_path missing in settings".to_string())?;
    let piper_bin = settings.piper_bin_path.clone()
        .unwrap_or_else(|| home_path(".forge/bin/piper"));
    let piper_voice = settings.piper_voice_path.clone()
        .unwrap_or_else(|| home_path(".forge/models/piper/voice.onnx"));

    if !whisper_bin.exists() { return Err(format!("whisper-cli missing: {}", whisper_bin.display())); }
    if !piper_bin.exists() { return Err(format!("piper missing: {}", piper_bin.display())); }
    if !piper_voice.exists() { return Err(format!("piper voice missing: {}", piper_voice.display())); }

    let vh = std::sync::Arc::clone(&state.voice);
    {
        let mut g = vh.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(old) = g.take() {
            eprintln!("[voice] replacing stale session");
            old.stop();
        }
    }

    let inference = state.inference.lock().unwrap().clone()
        .ok_or_else(|| "connect_inference first".to_string())?;
    let vault = state.vault_path.lock().unwrap().clone()
        .ok_or_else(|| "no vault open".to_string())?;
    let search_arc = std::sync::Arc::clone(&state.search);
    let (_cfg, search_db_path, search_index_path) = search_paths()?;
    let db_path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."))
        .join("forge").join("forge.db");
    let max_iters = settings.max_tool_iterations;
    let window_arc = std::sync::Arc::new(window);
    let window_for_closure = std::sync::Arc::clone(&window_arc);

    let wake_word = if use_wake && !settings.wake_word.is_empty() {
        Some(settings.wake_word.clone())
    } else { None };
    let cfg = crate::voice::VoiceConfig {
        whisper_bin,
        whisper_model,
        piper_bin,
        piper_voice,
        language: settings.whisper_language,
        wake_word,
    };

    // Accumulate chat history across the session.
    let history: std::sync::Arc<std::sync::Mutex<Vec<crate::llm::ChatMessage>>> =
        std::sync::Arc::new(std::sync::Mutex::new({
            let vault_name = vault.file_name().map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "vault".into());
            vec![crate::llm::ChatMessage::system(crate::agent::default_system_prompt(&vault_name))]
        }));
    let history_closure = std::sync::Arc::clone(&history);
    let tools = crate::agent::tool_schemas();

    let (tx, rx) = std::sync::mpsc::channel::<crate::voice::VoiceEvent>();

    // Forward voice events to UI.
    let fw_window = std::sync::Arc::clone(&window_arc);
    std::thread::Builder::new().name("forge-voice-forward".into())
        .spawn(move || {
            use tauri::Emitter;
            for ev in rx.iter() {
                match ev {
                    crate::voice::VoiceEvent::State(s) => { let _ = fw_window.emit("voice://state", s); }
                    crate::voice::VoiceEvent::Transcript(t) => { let _ = fw_window.emit("voice://transcript", t); }
                    crate::voice::VoiceEvent::AssistantText(t) => { let _ = fw_window.emit("voice://assistant-text", t); }
                    crate::voice::VoiceEvent::TtsChunk(b64) => {
                        let s = String::from_utf8_lossy(&b64).to_string();
                        let _ = fw_window.emit("voice://tts-chunk", s);
                    }
                    crate::voice::VoiceEvent::BargeIn => { let _ = fw_window.emit("voice://barge-in", ()); }
                    crate::voice::VoiceEvent::Error(e) => { let _ = fw_window.emit("voice://error", e); }
                    crate::voice::VoiceEvent::Stopped => { let _ = fw_window.emit("voice://stopped", ()); break; }
                }
            }
        }).ok();

    let on_prompt = move |transcript: String, event_tx: &std::sync::mpsc::Sender<crate::voice::VoiceEvent>| -> Result<String, String> {
        let mut h = history_closure.lock().unwrap_or_else(|e| e.into_inner());
        h.push(crate::llm::ChatMessage::user(&transcript));
        let ctx = crate::agent::ToolContext {
            vault_path: vault.clone(),
            db_path: db_path.clone(),
            search: std::sync::Arc::clone(&search_arc),
            search_db_path: search_db_path.clone(),
            search_index_path: search_index_path.clone(),
        };
        let (agent_tx, agent_rx) = std::sync::mpsc::channel::<crate::agent::AgentEvent>();
        let messages_clone: Vec<crate::llm::ChatMessage> = h.clone();
        drop(h);
        let inf = inference.clone();
        let tools_clone = tools.clone();
        let win = std::sync::Arc::clone(&window_for_closure);
        std::thread::Builder::new().name("voice-agent".into()).spawn(move || {
            let mut msgs = messages_clone;
            crate::agent::run_agent_loop(&inf, &mut msgs, &tools_clone, &ctx, max_iters, &agent_tx);
            // Also emit chat events for the UI panel.
            let _ = win;
        }).map_err(|e| format!("spawn agent: {e}"))?;

        let mut final_text = String::new();
        let mut updated_history: Option<Vec<crate::llm::ChatMessage>> = None;
        use tauri::Emitter;
        for ev in agent_rx.iter() {
            match ev {
                crate::agent::AgentEvent::Token(t) => {
                    final_text.push_str(&t);
                    let _ = window_for_closure.emit("chat://token", t);
                }
                crate::agent::AgentEvent::Thinking(_) => {}
                crate::agent::AgentEvent::ToolCallStarted { name, args } => {
                    let _ = window_for_closure.emit("chat://tool-start",
                        serde_json::json!({"name": name, "args": args}));
                }
                crate::agent::AgentEvent::ToolCallResult { name, content, is_error } => {
                    let _ = window_for_closure.emit("chat://tool-result",
                        serde_json::json!({"name": name, "content": content, "is_error": is_error}));
                }
                crate::agent::AgentEvent::Finished { messages } => {
                    updated_history = messages;
                    let _ = window_for_closure.emit("chat://done", ());
                    break;
                }
                crate::agent::AgentEvent::Error(e) => {
                    let _ = event_tx.send(crate::voice::VoiceEvent::Error(e));
                    break;
                }
            }
        }
        if let Some(m) = updated_history {
            let mut h = history_closure.lock().unwrap_or_else(|e| e.into_inner());
            *h = m;
        }
        Ok(final_text.trim().to_string())
    };

    let session = crate::voice::VoiceSession::start(cfg, tx, on_prompt);
    {
        let mut g = vh.0.lock().unwrap_or_else(|e| e.into_inner());
        *g = Some(session);
    }
    Ok(())
}

#[tauri::command]
pub fn voice_stop(state: State<'_, AppState>) -> Result<(), String> {
    let vh = std::sync::Arc::clone(&state.voice);
    let mut g = vh.0.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = g.take() { s.stop(); }
    Ok(())
}

/// Interrupt current TTS playback; loop returns to listening on next iter.
#[tauri::command]
pub fn voice_interrupt(state: State<'_, AppState>) -> Result<(), String> {
    let vh = std::sync::Arc::clone(&state.voice);
    let g = vh.0.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = g.as_ref() { s.interrupt(); }
    Ok(())
}

/// Set mute: loop keeps listening but discards utterances (no LLM/TTS).
#[tauri::command]
pub fn voice_set_muted(state: State<'_, AppState>, muted: bool) -> Result<(), String> {
    let vh = std::sync::Arc::clone(&state.voice);
    let g = vh.0.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = g.as_ref() { s.set_muted(muted); }
    Ok(())
}

#[tauri::command]
pub fn stop_chat(_state: State<'_, AppState>) -> Result<(), String> {
    // TODO: cooperative cancellation. For now, a new send_chat_message
    // call supersedes the previous one at the frontend level.
    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn resolve_within_vault(vault: &Path, path: &Path) -> Result<PathBuf, String> {
    let full = if path.is_absolute() {
        path.to_path_buf()
    } else {
        vault.join(path)
    };
    let canonical = full
        .canonicalize()
        .map_err(|e| format!("Cannot resolve {}: {e}", full.display()))?;
    let vault_canon = vault
        .canonicalize()
        .map_err(|e| format!("Cannot resolve vault root: {e}"))?;
    if !canonical.starts_with(&vault_canon) {
        return Err(format!(
            "Path escapes vault: {}",
            canonical.display()
        ));
    }
    Ok(canonical)
}
