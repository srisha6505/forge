//! Forge Tauri backend. Re-exports the core business-logic modules
//! (inference, agent, search, embedder, settings, auth, link parsing)
//! and wires them to Tauri commands declared in `commands`.

#![allow(dead_code)]

pub mod agent;
pub mod auth;
pub mod commands;
pub mod embedder;
pub mod llm;
pub mod search;
pub mod settings;

use std::sync::{Arc, Mutex};

/// Shared application state held across Tauri command invocations.
pub struct AppState {
    pub inference: Mutex<Option<llm::InferenceHandle>>,
    pub settings: Mutex<settings::Settings>,
    pub vault_path: Mutex<Option<std::path::PathBuf>>,
    /// Lazy-initialised vault search index. Wrapped in Arc so the agent
    /// thread can share the same instance instead of opening its own
    /// (which would mean two copies of the embedder model in memory).
    pub search: Arc<Mutex<Option<search::VaultSearch>>>,
}

impl AppState {
    fn new() -> Self {
        let settings = settings::Settings::load();
        let vault_path = settings.resolved_vault_path();
        Self {
            inference: Mutex::new(None),
            settings: Mutex::new(settings),
            vault_path: Mutex::new(vault_path),
            search: Arc::new(Mutex::new(None)),
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_settings,
            commands::open_vault,
            commands::current_vault,
            commands::list_vault_files,
            commands::list_vault_tree,
            commands::read_file,
            commands::write_file,
            commands::rename_file,
            commands::delete_file,
            commands::search_vault,
            commands::reindex_vault,
            commands::search_status,
            commands::connect_inference,
            commands::send_chat_message,
            commands::stop_chat,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
