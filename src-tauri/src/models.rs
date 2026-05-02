//! Managed model catalog: downloadable GGUF / whisper / piper weights
//! that live under a forge-controlled directory so the app can discover,
//! download, delete, and reference them by ID.
//!
//! Design notes:
//! - The catalog is compiled in (stable set of curated models). A future
//!   version could fetch it from a manifest on GitHub so new models can
//!   be added without a release.
//! - Downloads are streaming (chunked) so the UI can show progress. Each
//!   download spawns a worker thread that emits `model://download-progress`
//!   events keyed by model id.
//! - We never ship weights in the installer; they live on HuggingFace /
//!   GitHub Releases and the app fetches on demand. Users on slow or
//!   metered connections pick what they want.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tauri::Emitter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelKind {
    Stt,
    Tts,
}

impl ModelKind {
    pub fn dir(&self) -> &'static str {
        match self {
            ModelKind::Stt => "stt",
            ModelKind::Tts => "tts",
        }
    }
}

/// Static catalog entry. what's known to be downloadable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    pub kind: ModelKind,
    pub name: String,
    pub description: String,
    pub size_bytes: u64,
    pub filename: String,
    pub url: String,
    /// Piper voices need a companion `.json` config file next to the .onnx.
    pub config_url: Option<String>,
    pub config_filename: Option<String>,
}

/// Runtime status of a catalog entry: downloaded (and where), or not.
#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub kind: ModelKind,
    pub name: String,
    pub description: String,
    pub size_bytes: u64,
    pub url: String,
    pub filename: String,
    /// Absolute path if the primary file is on disk.
    pub local_path: Option<PathBuf>,
    pub downloaded: bool,
    /// Actual bytes on disk (may differ from advertised size_bytes).
    pub on_disk_bytes: Option<u64>,
}

/// Base data directory: `~/.local/share/forge` on Linux, OS equivalents
/// elsewhere. Created lazily.
pub fn data_dir() -> PathBuf {
    let base = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("forge");
    let _ = fs::create_dir_all(&base);
    base
}

pub fn models_dir(kind: ModelKind) -> PathBuf {
    let p = data_dir().join("models").join(kind.dir());
    let _ = fs::create_dir_all(&p);
    p
}

/// Full path where a catalog entry's primary file would live.
pub fn entry_path(entry: &CatalogEntry) -> PathBuf {
    models_dir(entry.kind).join(&entry.filename)
}

pub fn entry_config_path(entry: &CatalogEntry) -> Option<PathBuf> {
    entry.config_filename.as_ref().map(|f| models_dir(entry.kind).join(f))
}

/// The compiled-in catalog. Extend this list to add more downloadable
/// options. URLs point at Hugging Face / public mirrors.
pub fn catalog() -> Vec<CatalogEntry> {
    vec![
        // ── Whisper STT ───────────────────────────────────────────────
        CatalogEntry {
            id: "whisper-tiny".into(),
            kind: ModelKind::Stt,
            name: "Whisper tiny (multilingual)".into(),
            description: "~75 MB. Fastest whisper; rough accuracy. 15-30× realtime on CPU.".into(),
            size_bytes: 77_700_000,
            filename: "ggml-tiny.bin".into(),
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin".into(),
            config_url: None,
            config_filename: None,
        },
        CatalogEntry {
            id: "whisper-base".into(),
            kind: ModelKind::Stt,
            name: "Whisper base (multilingual)".into(),
            description: "~142 MB. Better than tiny. 10-20× realtime on CPU.".into(),
            size_bytes: 147_900_000,
            filename: "ggml-base.bin".into(),
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin".into(),
            config_url: None,
            config_filename: None,
        },
        CatalogEntry {
            id: "whisper-small".into(),
            kind: ModelKind::Stt,
            name: "Whisper small (multilingual)".into(),
            description: "~466 MB. Recommended. Near-pro accuracy, 3-5× realtime on CPU.".into(),
            size_bytes: 487_000_000,
            filename: "ggml-small.bin".into(),
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin".into(),
            config_url: None,
            config_filename: None,
        },
        // ── Piper voices (TTS) ────────────────────────────────────────
        CatalogEntry {
            id: "piper-en-amy-medium".into(),
            kind: ModelKind::Tts,
            name: "Piper. Amy (en-US, medium)".into(),
            description: "Natural female US English voice. ~63 MB.".into(),
            size_bytes: 63_200_000,
            filename: "en_US-amy-medium.onnx".into(),
            url: "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/amy/medium/en_US-amy-medium.onnx".into(),
            config_url: Some("https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/amy/medium/en_US-amy-medium.onnx.json".into()),
            config_filename: Some("en_US-amy-medium.onnx.json".into()),
        },
        CatalogEntry {
            id: "piper-en-ryan-medium".into(),
            kind: ModelKind::Tts,
            name: "Piper. Ryan (en-US, medium)".into(),
            description: "Natural male US English voice. ~63 MB.".into(),
            size_bytes: 63_200_000,
            filename: "en_US-ryan-medium.onnx".into(),
            url: "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/ryan/medium/en_US-ryan-medium.onnx".into(),
            config_url: Some("https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/ryan/medium/en_US-ryan-medium.onnx.json".into()),
            config_filename: Some("en_US-ryan-medium.onnx.json".into()),
        },
        CatalogEntry {
            id: "piper-en-kathleen-low".into(),
            kind: ModelKind::Tts,
            name: "Piper. Kathleen (en-GB, low)".into(),
            description: "Fast, lightweight British English voice. ~20 MB.".into(),
            size_bytes: 21_700_000,
            filename: "en_GB-alba-medium.onnx".into(),
            url: "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_GB/alba/medium/en_GB-alba-medium.onnx".into(),
            config_url: Some("https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_GB/alba/medium/en_GB-alba-medium.onnx.json".into()),
            config_filename: Some("en_GB-alba-medium.onnx.json".into()),
        },
    ]
}

pub fn get_entry(id: &str) -> Option<CatalogEntry> {
    catalog().into_iter().find(|e| e.id == id)
}

/// Return every catalog entry annotated with its on-disk status.
pub fn inventory() -> Vec<ModelInfo> {
    catalog()
        .into_iter()
        .map(|e| {
            let path = entry_path(&e);
            let (downloaded, on_disk_bytes) = match fs::metadata(&path) {
                Ok(m) if m.is_file() && m.len() > 0 => (true, Some(m.len())),
                _ => (false, None),
            };
            ModelInfo {
                id: e.id.clone(),
                kind: e.kind,
                name: e.name.clone(),
                description: e.description.clone(),
                size_bytes: e.size_bytes,
                url: e.url.clone(),
                filename: e.filename.clone(),
                local_path: downloaded.then(|| path.clone()),
                downloaded,
                on_disk_bytes,
            }
        })
        .collect()
}

/// Delete the primary + optional config file for a catalog entry. Returns
/// true if any file was actually removed.
pub fn delete(id: &str) -> Result<bool, String> {
    let entry = get_entry(id).ok_or_else(|| format!("unknown model id: {id}"))?;
    let mut removed = false;
    let primary = entry_path(&entry);
    if primary.exists() {
        fs::remove_file(&primary).map_err(|e| format!("rm {}: {e}", primary.display()))?;
        removed = true;
    }
    if let Some(cfg) = entry_config_path(&entry) {
        if cfg.exists() {
            fs::remove_file(&cfg).map_err(|e| format!("rm {}: {e}", cfg.display()))?;
            removed = true;
        }
    }
    Ok(removed)
}

// ── Download orchestration ──────────────────────────────────────────

/// Tracker so we can cancel an in-flight download.
#[derive(Default)]
pub struct ActiveDownloads(pub Mutex<std::collections::HashMap<String, Arc<AtomicBool>>>);

#[derive(Clone, Serialize)]
pub struct DownloadProgress {
    pub id: String,
    pub downloaded: u64,
    pub total: u64,
    pub phase: String,      // "primary" | "config" | "done" | "cancelled" | "error"
    pub error: Option<String>,
}

/// Kick off a background download. Returns immediately. Progress is
/// emitted via `model://download-progress` events.
pub fn start_download(
    window: tauri::Window,
    active: Arc<ActiveDownloads>,
    id: String,
) -> Result<(), String> {
    let entry = get_entry(&id).ok_or_else(|| format!("unknown model id: {id}"))?;

    {
        let mut g = active.0.lock().unwrap_or_else(|e| e.into_inner());
        if g.contains_key(&id) {
            return Err("already downloading".into());
        }
        g.insert(id.clone(), Arc::new(AtomicBool::new(false)));
    }
    let cancel = {
        let g = active.0.lock().unwrap_or_else(|e| e.into_inner());
        g.get(&id).cloned().expect("just inserted")
    };

    let active_for_thread = Arc::clone(&active);
    std::thread::Builder::new()
        .name(format!("forge-download-{id}"))
        .spawn(move || {
            let primary_path = entry_path(&entry);
            let config_path = entry_config_path(&entry);
            let result = (|| -> Result<(), String> {
                download_one(&window, &id, "primary", &entry.url, &primary_path, entry.size_bytes, &cancel)?;
                if let (Some(url), Some(path)) = (entry.config_url.as_deref(), config_path.as_ref()) {
                    // Config files are small. we don't know their size up
                    // front; pass 0 so the UI shows indeterminate progress
                    // for this phase (tiny anyway).
                    download_one(&window, &id, "config", url, path, 0, &cancel)?;
                }
                Ok(())
            })();

            // Cleanup partial files on cancel/error so the next attempt is clean.
            let cancelled = cancel.load(Ordering::Relaxed);
            match &result {
                Ok(()) => {
                    let _ = window.emit("model://download-progress", DownloadProgress {
                        id: id.clone(),
                        downloaded: entry.size_bytes,
                        total: entry.size_bytes,
                        phase: "done".into(),
                        error: None,
                    });
                }
                Err(msg) => {
                    let phase = if cancelled { "cancelled" } else { "error" };
                    let _ = fs::remove_file(&primary_path);
                    if let Some(p) = &config_path { let _ = fs::remove_file(p); }
                    let _ = window.emit("model://download-progress", DownloadProgress {
                        id: id.clone(),
                        downloaded: 0,
                        total: entry.size_bytes,
                        phase: phase.into(),
                        error: Some(msg.clone()),
                    });
                }
            }

            let mut g = active_for_thread.0.lock().unwrap_or_else(|e| e.into_inner());
            g.remove(&id);
        })
        .map_err(|e| format!("spawn download thread: {e}"))?;
    Ok(())
}

pub fn cancel_download(active: &ActiveDownloads, id: &str) -> bool {
    let g = active.0.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(flag) = g.get(id) {
        flag.store(true, Ordering::Relaxed);
        true
    } else {
        false
    }
}

fn download_one(
    window: &tauri::Window,
    id: &str,
    phase: &str,
    url: &str,
    dest: &Path,
    advertised_total: u64,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    eprintln!("[models] {id} {phase}: {} → {}", url, dest.display());
    let resp = ureq::get(url)
        .call()
        .map_err(|e| format!("fetch {url}: {e}"))?;
    let total: u64 = resp
        .header("Content-Length")
        .and_then(|s| s.parse().ok())
        .unwrap_or(advertised_total);

    if let Some(parent) = dest.parent() { let _ = fs::create_dir_all(parent); }
    // Write to a temp `.part` file then rename on success so a crashed
    // download doesn't leave a corrupt model that looks valid.
    let tmp = dest.with_extension("part");
    let mut out = fs::File::create(&tmp).map_err(|e| format!("create {}: {e}", tmp.display()))?;
    let mut reader = resp.into_reader();
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    let mut last_emit = std::time::Instant::now();

    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(out);
            let _ = fs::remove_file(&tmp);
            return Err("cancelled".into());
        }
        let n = reader.read(&mut buf).map_err(|e| format!("read: {e}"))?;
        if n == 0 { break; }
        out.write_all(&buf[..n]).map_err(|e| format!("write: {e}"))?;
        downloaded += n as u64;
        if last_emit.elapsed() >= std::time::Duration::from_millis(150) {
            let _ = window.emit("model://download-progress", DownloadProgress {
                id: id.to_string(),
                downloaded,
                total,
                phase: phase.to_string(),
                error: None,
            });
            last_emit = std::time::Instant::now();
        }
    }
    drop(out);
    fs::rename(&tmp, dest).map_err(|e| format!("rename {} -> {}: {e}", tmp.display(), dest.display()))?;

    // Final progress tick so UI shows 100% for this phase.
    let _ = window.emit("model://download-progress", DownloadProgress {
        id: id.to_string(),
        downloaded,
        total: downloaded.max(total),
        phase: phase.to_string(),
        error: None,
    });
    Ok(())
}

// ── GPU detection ───────────────────────────────────────────────────

#[derive(Serialize)]
pub struct GpuStatus {
    pub cuda_available: bool,
    pub details: String,
}

/// Best-effort CUDA presence check. We look for `nvidia-smi` on PATH or
/// a `CUDA_PATH` / `CUDA_HOME` env var. Compile-time CUDA support is a
/// separate question. even if CUDA is present at runtime, the app only
/// uses it if built with `--features cuda`.
pub fn detect_gpu() -> GpuStatus {
    let cuda_env = std::env::var("CUDA_PATH").ok()
        .or_else(|| std::env::var("CUDA_HOME").ok())
        .or_else(|| std::env::var("CUDA_ROOT").ok());

    let nvidia_smi = std::process::Command::new("nvidia-smi")
        .arg("--query-gpu=name,driver_version")
        .arg("--format=csv,noheader")
        .output()
        .ok()
        .and_then(|o| if o.status.success() {
            Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
        } else { None });

    match (nvidia_smi, cuda_env) {
        (Some(gpus), Some(path)) if !gpus.is_empty() =>
            GpuStatus { cuda_available: true, details: format!("CUDA ({path}) · {gpus}") },
        (Some(gpus), None) if !gpus.is_empty() =>
            GpuStatus { cuda_available: true, details: format!("CUDA runtime detected · {gpus}") },
        (None, Some(path)) =>
            GpuStatus { cuda_available: true, details: format!("CUDA installed at {path}, no nvidia-smi found") },
        _ =>
            GpuStatus { cuda_available: false,
                details: "No CUDA detected. running on CPU. Install NVIDIA drivers + CUDA for GPU acceleration.".into() },
    }
}
