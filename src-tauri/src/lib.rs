//! Forge Tauri backend. Re-exports the core business-logic modules
//! (inference, agent, search, embedder, settings, auth, link parsing)
//! and wires them to Tauri commands declared in `commands`.

#![allow(dead_code)]

// Switch the process-wide allocator to mimalloc. The default glibc
// malloc fragments under our workload (chunked SQLite reads + candle
// tensor lifetimes + embedder span allocations), which shows up as
// climbing RSS over a long session and slower retrieval. mimalloc is
// drop-in, pure-Rust wrapper, no extra runtime to ship.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub mod agent;
pub mod auth;
pub mod binaries;
pub mod chat;
pub mod commands;
pub mod copilot;
// Vault semantic search subsystem — see Cargo.toml comment. Disabled
// on Windows until usearch 2.x ships a fix for the MAP_FAILED MSVC
// build break.
#[cfg(not(target_os = "windows"))]
pub mod embedder;
pub mod gemini;
pub mod latex;
pub mod links;
pub mod llm;
pub mod models;
pub mod openai;
pub mod openai_compat;
#[cfg(not(target_os = "windows"))]
pub mod search;
pub mod settings;
pub mod skills;
pub mod stt;
pub mod terminal;
pub mod voice;

use std::sync::{Arc, Mutex};

/// Shared application state held across Tauri command invocations.
pub struct AppState {
    pub inference: Mutex<Option<llm::InferenceHandle>>,
    pub settings: Mutex<settings::Settings>,
    pub vault_path: Mutex<Option<std::path::PathBuf>>,
    /// Lazy-initialised vault search index. Wrapped in Arc so the agent
    /// thread can share the same instance instead of opening its own
    /// (which would mean two copies of the embedder model in memory).
    #[cfg(not(target_os = "windows"))]
    pub search: Arc<Mutex<Option<search::VaultSearch>>>,
    pub whisper: Arc<stt::WhisperHandle>,
    pub mic: Arc<stt::MicHandle>,
    pub voice: Arc<voice::VoiceHandle>,
    pub downloads: Arc<models::ActiveDownloads>,
    pub copilot_pending: Arc<copilot::PendingAuth>,
    /// Cancel flag for the in-flight binary install (whisper-cli or piper).
    pub binary_install: Arc<Mutex<Option<(String, Arc<std::sync::atomic::AtomicBool>)>>>,
}

impl AppState {
    fn new() -> Self {
        let settings = settings::Settings::load();
        let vault_path = settings.resolved_vault_path();
        Self {
            inference: Mutex::new(None),
            settings: Mutex::new(settings),
            vault_path: Mutex::new(vault_path),
            #[cfg(not(target_os = "windows"))]
            search: Arc::new(Mutex::new(None)),
            whisper: Arc::new(stt::WhisperHandle::default()),
            mic: Arc::new(stt::MicHandle::default()),
            voice: Arc::new(voice::VoiceHandle::default()),
            downloads: Arc::new(models::ActiveDownloads::default()),
            copilot_pending: Arc::new(copilot::PendingAuth::default()),
            binary_install: Arc::new(Mutex::new(None)),
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // WebKitGTK on Linux ships with conservative defaults: software
    // compositing in many distros, no DMA-BUF zero-copy, and the GL
    // backend off. Forge is paint-heavy (CodeMirror decorations, large
    // markdown previews, streaming chat), so we force every available
    // GPU path on. Mac (WKWebView) and Windows (WebView2) ignore these
    // and GPU-composite by default. Must run before tauri::Builder so
    // wry/webkit2gtk reads the values during webview init.
    #[cfg(target_os = "linux")]
    unsafe {
        // WebKitGTK compositing path. Required for webkit2gtk to actually
        // touch the GPU; without these the surface falls through to a
        // CPU compositor.
        std::env::set_var("WEBKIT_FORCE_COMPOSITING_MODE", "1");
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "0");
        std::env::set_var("WEBKIT_USE_GLES", "1");
        std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "0");

        // NVIDIA-proprietary X11 path tunings. No-ops on AMD/Intel/Mesa
        // and on Wayland (the variables are read by the NVIDIA libGL
        // only). Material on this hardware:
        //   __GL_SYNC_TO_VBLANK=1       eliminate scroll-tear
        //   __GL_THREADED_OPTIMIZATIONS=1
        //                                offload GL work to a side
        //                                thread; ~5-10% smoother under
        //                                paint-heavy load
        //   __GL_YIELD=USLEEP          gentler driver thread on
        //                                contended cores (vs. NOTHING)
        // We set them unconditionally; the NVIDIA libGL ignores them
        // when not driving the surface, and on non-NVIDIA setups the
        // generic libGL never reads these names.
        std::env::set_var("__GL_SYNC_TO_VBLANK", "1");
        std::env::set_var("__GL_THREADED_OPTIMIZATIONS", "1");
        std::env::set_var("__GL_YIELD", "USLEEP");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
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
            #[cfg(not(target_os = "windows"))]
            commands::search_vault,
            #[cfg(not(target_os = "windows"))]
            commands::reindex_vault,
            #[cfg(not(target_os = "windows"))]
            commands::search_status,
            commands::connect_inference,
            commands::send_chat_message,
            commands::stop_chat,
            commands::transcribe_audio,
            commands::start_recording,
            commands::stop_recording_and_transcribe,
            commands::voice_start,
            commands::voice_stop,
            commands::voice_interrupt,
            commands::voice_set_muted,
            commands::voice_start_wake,
            commands::compile_latex,
            commands::latex_status,
            commands::open_in_text_editor,
            commands::copilot_status,
            commands::copilot_login_start,
            commands::copilot_login_poll,
            commands::copilot_logout,
            commands::copilot_models,
            commands::list_models,
            commands::start_model_download,
            commands::cancel_model_download,
            commands::delete_model,
            commands::detect_gpu,
            commands::runtime_capabilities,
            commands::binary_status,
            commands::install_whisper_cpp,
            commands::install_piper,
            commands::cancel_binary_install,
            commands::list_backlinks,
            commands::link_graph,
            settings::get_app_settings,
            settings::set_app_settings,
            settings::get_vault_settings,
            settings::set_vault_settings,
            settings::migrate_vault_settings,
            chat::save_chat,
            chat::load_chat,
            chat::list_chats,
            chat::delete_chat,
            chat::export_chat_as_note,
            llm::test_anthropic,
            llm::test_openai,
            llm::test_gemini,
            llm::test_copilot,
            llm::test_openai_compat,
            llm::list_provider_models,
            terminal::spawn_terminal,
            terminal::write_terminal,
            terminal::resize_terminal,
            terminal::kill_terminal,
            terminal::list_terminals,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
