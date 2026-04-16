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
        *search_arc.lock().unwrap() = Some(vs);
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

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut search_guard = search_arc.lock().unwrap();
        if search_guard.is_none() {
            let mut vs = crate::search::VaultSearch::new(&db_path, &index_path)
                .map_err(|e| format!("search init failed: {e}"))?;
            if vs.chunk_count() == 0 {
                vs.build_vault(&vault)
                    .map_err(|e| format!("vault index build failed: {e}"))?;
                vs.save_index(&index_path)
                    .map_err(|e| format!("save index failed: {e}"))?;
            }
            *search_guard = Some(vs);
        }
        let vs = search_guard.as_ref().unwrap();
        vs.search(&query, limit.unwrap_or(20))
            .map_err(|e| format!("search failed: {e}"))
    }));

    let results = match result {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return Err(e),
        Err(panic) => {
            let msg = if let Some(s) = panic.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic during search".to_string()
            };
            return Err(format!("Search panicked: {msg}"));
        }
    };

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
