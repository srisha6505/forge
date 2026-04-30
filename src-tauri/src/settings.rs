//! Two-scope settings: AppSettings (global, in tauri config dir) and
//! VaultSettings (per-vault, in <vault>/.forge/settings.json).
//!
//! The legacy `Settings` struct is retained as long as commands.rs / voice.rs
//! still consume it. New code should target `AppSettings` / `VaultSettings`.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ── New scope: AppSettings (global) ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppSettings {
    #[serde(default)]
    pub last_opened_vault: Option<String>,
}

// ── New scope: VaultSettings (per-vault) ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VaultSettings {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: u32,
    #[serde(default = "default_chat_panel_width")]
    pub chat_panel_width: u32,
    #[serde(default)]
    pub recent_files: Vec<String>,
    #[serde(default)]
    pub ai: AiSettings,
    #[serde(default)]
    pub voice: VoiceSettings,
    #[serde(default)]
    pub system_prompt: String,
    #[serde(default = "default_tools_allowed")]
    pub tools_allowed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiSettings {
    #[serde(default = "default_provider")]
    pub default_provider: String,
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
    #[serde(default)]
    pub routing: RoutingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoutingConfig {
    #[serde(default)]
    pub chat: Option<RoutedModel>,
    #[serde(default)]
    pub fast: Option<RoutedModel>,
    #[serde(default)]
    pub summarise: Option<RoutedModel>,
    #[serde(default)]
    pub embed: Option<RoutedModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutedModel {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceSettings {
    pub stt_provider: String,
    pub whisper_model: String,
    pub tts_voice: String,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            stt_provider: "whisper".into(),
            whisper_model: "base.en".into(),
            tts_voice: "en-US-AriaNeural".into(),
        }
    }
}

fn default_theme() -> String { "dark".into() }
fn default_sidebar_width() -> u32 { 280 }
fn default_chat_panel_width() -> u32 { 380 }
fn default_provider() -> String { "anthropic".into() }
fn default_tools_allowed() -> Vec<String> {
    vec!["read_file", "edit_file", "list_dir", "grep", "search_vault"]
        .into_iter()
        .map(String::from)
        .collect()
}

// ── Path helpers ────────────────────────────────────────────────────────

fn app_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("forge")
}

fn app_settings_path() -> PathBuf {
    app_config_dir().join("settings.json")
}

fn vault_dotforge_dir(vault: &Path) -> PathBuf {
    vault.join(".forge")
}

fn vault_settings_path(vault: &Path) -> PathBuf {
    vault_dotforge_dir(vault).join("settings.json")
}

// Atomic write: write to .tmp sibling, fsync, rename. Avoids torn files
// if the process is killed mid-write.
fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create parent: {e}"))?;
    }
    let tmp = path.with_extension("json.tmp");
    let body = serde_json::to_string_pretty(value).map_err(|e| format!("serialize: {e}"))?;
    {
        let mut f = fs::File::create(&tmp).map_err(|e| format!("create tmp: {e}"))?;
        f.write_all(body.as_bytes()).map_err(|e| format!("write tmp: {e}"))?;
        f.sync_all().map_err(|e| format!("fsync tmp: {e}"))?;
    }
    fs::rename(&tmp, path).map_err(|e| format!("rename tmp: {e}"))?;
    Ok(())
}

fn read_json_or_default<T: serde::de::DeserializeOwned + Default>(path: &Path) -> T {
    match fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => T::default(),
    }
}

// ── Tauri commands: AppSettings ─────────────────────────────────────────

#[tauri::command]
pub fn get_app_settings() -> Result<AppSettings, String> {
    Ok(read_json_or_default::<AppSettings>(&app_settings_path()))
}

#[tauri::command]
pub fn set_app_settings(settings: AppSettings) -> Result<(), String> {
    atomic_write_json(&app_settings_path(), &settings)
}

// ── Tauri commands: VaultSettings ───────────────────────────────────────

#[tauri::command]
pub fn get_vault_settings(vault_path: String) -> Result<VaultSettings, String> {
    let vault = PathBuf::from(&vault_path);
    if !vault.is_dir() {
        return Err(format!("not a directory: {vault_path}"));
    }
    let dotforge = vault_dotforge_dir(&vault);
    fs::create_dir_all(&dotforge).map_err(|e| format!("create .forge: {e}"))?;
    Ok(read_json_or_default::<VaultSettings>(&vault_settings_path(&vault)))
}

#[tauri::command]
pub fn set_vault_settings(vault_path: String, settings: VaultSettings) -> Result<(), String> {
    let vault = PathBuf::from(&vault_path);
    if !vault.is_dir() {
        return Err(format!("not a directory: {vault_path}"));
    }
    atomic_write_json(&vault_settings_path(&vault), &settings)
}

// ── Migration: legacy global Settings → per-vault VaultSettings ─────────

#[tauri::command]
pub fn migrate_vault_settings(vault_path: String) -> Result<bool, String> {
    let vault = PathBuf::from(&vault_path);
    if !vault.is_dir() {
        return Err(format!("not a directory: {vault_path}"));
    }
    let target = vault_settings_path(&vault);
    if target.exists() {
        return Ok(false);
    }

    // Map legacy global → per-vault. The legacy file lives at
    // <config_dir>/forge/settings.json; load defaults if missing.
    let legacy = Settings::load();

    let mut providers: HashMap<String, ProviderConfig> = HashMap::new();
    if legacy.api_key.is_some() {
        providers.insert(
            "anthropic".to_string(),
            ProviderConfig {
                api_key: legacy.api_key.clone(),
                base_url: None,
                default_model: Some(legacy.api_model.clone()),
            },
        );
    }

    let provider_for_vault = match legacy.ai_provider.as_str() {
        "anthropic" | "claude" => "anthropic".to_string(),
        "copilot" => "copilot".to_string(),
        "local" => "local".to_string(),
        other if other.is_empty() => default_provider(),
        other => other.to_string(),
    };

    let theme = if legacy.theme.is_empty() { default_theme() } else { legacy.theme.clone() };

    let voice = VoiceSettings {
        // Legacy values "local"/"deepgram" don't map cleanly onto the new
        // "whisper"/... taxonomy, so fall back to default for non-whisper.
        stt_provider: if legacy.stt_provider == "local" { "whisper".into() } else { legacy.stt_provider.clone() },
        whisper_model: legacy
            .whisper_model_path
            .as_ref()
            .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_else(|| "base.en".to_string()),
        tts_voice: legacy.edge_tts_voice.clone(),
    };

    let migrated = VaultSettings {
        theme,
        sidebar_width: legacy.sidebar_width as u32,
        chat_panel_width: legacy.chat_panel_width as u32,
        recent_files: legacy
            .open_tabs
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
        ai: AiSettings {
            default_provider: provider_for_vault,
            providers,
            routing: RoutingConfig::default(),
        },
        voice,
        system_prompt: String::new(),
        tools_allowed: default_tools_allowed(),
    };

    atomic_write_json(&target, &migrated)?;
    Ok(true)
}

// ── Legacy struct (still consumed by commands.rs / voice.rs) ────────────

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
    #[serde(default = "default_body_font")]
    pub body_font: String,
    #[serde(default = "default_interface_font")]
    pub interface_font: String,
    #[serde(default = "default_mono_font")]
    pub mono_font: String,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default = "legacy_default_sidebar_width")]
    pub sidebar_width: f32,
    #[serde(default)]
    pub model_path: Option<PathBuf>,
    #[serde(default = "default_gpu_layers")]
    pub gpu_layers: u32,
    #[serde(default = "default_ctx_size")]
    pub ctx_size: u32,
    #[serde(default = "default_chat_width")]
    pub chat_panel_width: f32,
    #[serde(default = "default_max_tool_iters")]
    pub max_tool_iterations: usize,
    #[serde(default = "legacy_default_provider")]
    pub ai_provider: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_api_model")]
    pub api_model: String,
    #[serde(default)]
    pub whisper_model_path: Option<PathBuf>,
    #[serde(default = "default_whisper_language")]
    pub whisper_language: String,
    #[serde(default)]
    pub piper_bin_path: Option<PathBuf>,
    #[serde(default)]
    pub piper_voice_path: Option<PathBuf>,
    #[serde(default = "default_wake_word")]
    pub wake_word: String,
    #[serde(default = "default_copilot_model")]
    pub copilot_model: String,
    #[serde(default = "default_stt_provider")]
    pub stt_provider: String,
    #[serde(default = "default_tts_provider")]
    pub tts_provider: String,
    #[serde(default)]
    pub deepgram_api_key: Option<String>,
    #[serde(default = "default_deepgram_stt_model")]
    pub deepgram_stt_model: String,
    #[serde(default = "default_deepgram_tts_voice")]
    pub deepgram_tts_voice: String,
    #[serde(default = "default_edge_tts_voice")]
    pub edge_tts_voice: String,
    #[serde(default = "default_gtts_lang")]
    pub gtts_lang: String,
}

pub fn default_wake_word() -> String { "Riva".into() }
pub fn default_whisper_language() -> String { "auto".into() }
pub fn default_body_font() -> String { "DejaVu Sans".to_string() }
pub fn default_interface_font() -> String { "DejaVu Sans".to_string() }
pub fn default_mono_font() -> String { "DejaVu Sans Mono".to_string() }
pub fn default_font_size() -> f32 { 15.0 }
// Renamed to avoid collision with the new u32-typed default_sidebar_width().
pub fn legacy_default_sidebar_width() -> f32 { 260.0 }
pub fn default_gpu_layers() -> u32 { 99 }
pub fn default_ctx_size() -> u32 { 16384 }
pub fn default_chat_width() -> f32 { 400.0 }
pub fn default_max_tool_iters() -> usize { 10 }
pub fn legacy_default_provider() -> String { "local".into() }
pub fn default_api_model() -> String { "claude-sonnet-4-6".into() }
pub fn default_copilot_model() -> String { "claude-sonnet-4".into() }
pub fn default_stt_provider() -> String { "local".into() }
pub fn default_tts_provider() -> String { "edge".into() }
pub fn default_deepgram_stt_model() -> String { "nova-3".into() }
pub fn default_deepgram_tts_voice() -> String { "aura-2-thalia-en".into() }
pub fn default_edge_tts_voice() -> String { "en-US-AriaNeural".into() }
pub fn default_gtts_lang() -> String { "en".into() }

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
            sidebar_width: legacy_default_sidebar_width(),
            model_path: None,
            gpu_layers: default_gpu_layers(),
            ctx_size: default_ctx_size(),
            chat_panel_width: default_chat_width(),
            max_tool_iterations: default_max_tool_iters(),
            ai_provider: legacy_default_provider(),
            api_key: None,
            api_model: default_api_model(),
            whisper_model_path: None,
            whisper_language: default_whisper_language(),
            piper_bin_path: None,
            piper_voice_path: None,
            wake_word: default_wake_word(),
            copilot_model: default_copilot_model(),
            stt_provider: default_stt_provider(),
            tts_provider: default_tts_provider(),
            deepgram_api_key: None,
            deepgram_stt_model: default_deepgram_stt_model(),
            deepgram_tts_voice: default_deepgram_tts_voice(),
            edge_tts_voice: default_edge_tts_voice(),
            gtts_lang: default_gtts_lang(),
        }
    }
}

fn legacy_config_path() -> PathBuf {
    app_config_dir().join("settings.json")
}

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
        match fs::read_to_string(legacy_config_path()) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let path = legacy_config_path();
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
