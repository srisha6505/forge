//! Persistent app settings stored in ~/.config/forge/settings.json

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub last_vault_path: Option<PathBuf>,
    #[serde(default)]
    pub theme: String,
    #[serde(default)]
    pub open_tabs: Vec<PathBuf>,
    #[serde(default)]
    pub active_tab: Option<usize>,
    /// Body / editor text font family.
    #[serde(default = "default_body_font")]
    pub body_font: String,
    /// Interface (sidebar, tabs, status bar) font family.
    #[serde(default = "default_interface_font")]
    pub interface_font: String,
    /// Monospace font (code blocks, tables).
    #[serde(default = "default_mono_font")]
    pub mono_font: String,
    /// Base body font size in px.
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    /// Sidebar width in px (resizable).
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: f32,
    /// Path to a GGUF model file for local inference.
    #[serde(default)]
    pub model_path: Option<PathBuf>,
    /// Number of model layers to offload to GPU (0 = CPU only, 99 = all).
    #[serde(default = "default_gpu_layers")]
    pub gpu_layers: u32,
    /// Context window size for the model.
    #[serde(default = "default_ctx_size")]
    pub ctx_size: u32,
    /// Chat panel width in px (resizable).
    #[serde(default = "default_chat_width")]
    pub chat_panel_width: f32,
    /// Maximum tool-use iterations per agent turn.
    #[serde(default = "default_max_tool_iters")]
    pub max_tool_iterations: usize,
}

pub fn default_body_font() -> String { "DejaVu Sans".to_string() }
pub fn default_interface_font() -> String { "DejaVu Sans".to_string() }
pub fn default_mono_font() -> String { "DejaVu Sans Mono".to_string() }
pub fn default_font_size() -> f32 { 15.0 }
pub fn default_sidebar_width() -> f32 { 260.0 }
pub fn default_gpu_layers() -> u32 { 99 }
pub fn default_ctx_size() -> u32 { 8192 }
pub fn default_chat_width() -> f32 { 400.0 }
pub fn default_max_tool_iters() -> usize { 10 }

impl Default for Settings {
    fn default() -> Self {
        Self {
            last_vault_path: None,
            theme: String::new(),
            open_tabs: Vec::new(),
            active_tab: None,
            body_font: default_body_font(),
            interface_font: default_interface_font(),
            mono_font: default_mono_font(),
            font_size: default_font_size(),
            sidebar_width: default_sidebar_width(),
            model_path: None,
            gpu_layers: default_gpu_layers(),
            ctx_size: default_ctx_size(),
            chat_panel_width: default_chat_width(),
            max_tool_iterations: default_max_tool_iters(),
        }
    }
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("forge")
        .join("settings.json")
}

/// Curated list of commonly-installed font families. The user can type
/// any family name; this list is shown in the settings panel for convenience.
pub const BODY_FONTS: &[&str] = &[
    "DejaVu Sans", "Noto Sans", "Inter", "Roboto", "Open Sans", "Lato",
    "IBM Plex Sans", "Source Sans 3", "Liberation Sans", "Cantarell",
    "Ubuntu", "Segoe UI", "Helvetica Neue", "Arial",
];
pub const MONO_FONTS: &[&str] = &[
    "DejaVu Sans Mono", "JetBrains Mono", "Fira Code", "Cascadia Code",
    "Source Code Pro", "IBM Plex Mono", "Inconsolata", "Hack",
    "Liberation Mono", "Ubuntu Mono", "Menlo", "Monaco", "Consolas",
    "Noto Sans Mono", "monospace",
];

impl Settings {
    pub fn load() -> Self {
        match fs::read_to_string(config_path()) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let path = config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            fs::write(path, s).ok();
        }
    }

    pub fn resolved_vault_path(&self) -> Option<PathBuf> {
        self.last_vault_path.clone().filter(|p| p.is_dir())
    }

    pub fn set_vault(&mut self, path: &Path) {
        self.last_vault_path = Some(path.to_path_buf());
        self.open_tabs.clear();
        self.active_tab = None;
        self.save();
    }
}
