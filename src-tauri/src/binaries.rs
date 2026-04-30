//! Install the system binaries Forge needs (whisper-cli, piper) into a
//! managed directory. Two paths:
//!
//! - **whisper-cli**: upstream releases don't ship Linux/macOS binaries,
//!   so we clone the source and build it with cmake. Requires the user
//!   to have git, cmake, and a C++ toolchain installed. Emits progress.
//!
//! - **piper**: upstream ships prebuilt archives per platform. We pick
//!   the right one for the host, download, extract, copy the binary +
//!   espeak-ng-data into our managed dir.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::Emitter;

#[derive(Clone, Serialize)]
pub struct InstallEvent {
    pub id: String,          // "whisper-cli" | "piper"
    pub phase: String,       // cloning | building | downloading | extracting | done | error
    pub detail: String,
    pub progress: Option<f32>, // 0..1 when known
}

fn emit(window: &tauri::Window, ev: InstallEvent) {
    let _ = window.emit("binary://install", ev);
}

pub fn managed_bin_dir() -> PathBuf {
    let p = crate::models::data_dir().join("bin");
    let _ = fs::create_dir_all(&p);
    p
}

#[cfg(unix)]
fn set_exec(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn set_exec(_path: &Path) -> std::io::Result<()> { Ok(()) }

// ── whisper.cpp: build from source ──────────────────────────────────

pub fn install_whisper_cpp(window: tauri::Window, cancel: Arc<AtomicBool>) {
    let id = "whisper-cli".to_string();
    let result = (|| -> Result<PathBuf, String> {
        // Pre-flight: tools we need.
        for tool in ["git", "cmake"] {
            if Command::new(tool).arg("--version").output().is_err() {
                return Err(format!(
                    "`{tool}` not found on PATH. Install it first. On macOS: `brew install {tool}`. On Debian/Ubuntu: `sudo apt install {tool}`."
                ));
            }
        }

        let work = std::env::temp_dir().join(format!("forge-whisper-build-{}", std::process::id()));
        let _ = fs::remove_dir_all(&work);
        fs::create_dir_all(&work).map_err(|e| format!("mkdir tmp: {e}"))?;

        // 1. Clone.
        emit(&window, InstallEvent {
            id: id.clone(), phase: "cloning".into(),
            detail: "git clone whisper.cpp (shallow)…".into(), progress: None,
        });
        if cancel.load(Ordering::Relaxed) { return Err("cancelled".into()); }
        let clone = Command::new("git")
            .args(["clone", "--depth", "1",
                   "https://github.com/ggml-org/whisper.cpp.git",
                   work.to_str().unwrap()])
            .status().map_err(|e| format!("git clone: {e}"))?;
        if !clone.success() { return Err("git clone failed".into()); }

        // 2. Configure.
        emit(&window, InstallEvent {
            id: id.clone(), phase: "building".into(),
            detail: "cmake configure…".into(), progress: None,
        });
        if cancel.load(Ordering::Relaxed) { return Err("cancelled".into()); }
        let cfg = Command::new("cmake")
            .args(["-B", "build", "-DCMAKE_BUILD_TYPE=Release",
                   "-DWHISPER_BUILD_TESTS=OFF", "-DWHISPER_BUILD_EXAMPLES=ON"])
            .current_dir(&work)
            .status().map_err(|e| format!("cmake configure: {e}"))?;
        if !cfg.success() { return Err("cmake configure failed".into()); }

        // 3. Build the whisper-cli example target.
        emit(&window, InstallEvent {
            id: id.clone(), phase: "building".into(),
            detail: "compiling whisper-cli (this can take a few minutes)…".into(),
            progress: None,
        });
        if cancel.load(Ordering::Relaxed) { return Err("cancelled".into()); }
        let jobs = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).to_string();
        let build = Command::new("cmake")
            .args(["--build", "build", "--config", "Release",
                   "--target", "whisper-cli", "-j", &jobs])
            .current_dir(&work)
            .status().map_err(|e| format!("cmake build: {e}"))?;
        if !build.success() { return Err("cmake build failed".into()); }

        // 4. Find the resulting binary. layout varies per platform.
        let candidates = [
            work.join("build/bin/whisper-cli"),
            work.join("build/bin/Release/whisper-cli.exe"),
            work.join("build/bin/whisper-cli.exe"),
            work.join("build/examples/cli/whisper-cli"),
            work.join("build/examples/cli/Release/whisper-cli.exe"),
        ];
        let src = candidates.iter().find(|p| p.exists()).cloned()
            .ok_or_else(|| format!(
                "built binary not found. Checked: {}",
                candidates.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
            ))?;

        // 5. Copy to managed bin dir.
        emit(&window, InstallEvent {
            id: id.clone(), phase: "installing".into(),
            detail: format!("copying to {}", managed_bin_dir().display()),
            progress: None,
        });
        let dest_name = if cfg!(windows) { "whisper-cli.exe" } else { "whisper-cli" };
        let dest = managed_bin_dir().join(dest_name);
        let _ = fs::remove_file(&dest);
        fs::copy(&src, &dest).map_err(|e| format!("copy to {}: {e}", dest.display()))?;
        set_exec(&dest).ok();

        // 6. whisper.cpp loads shared libs from next to the binary on some
        //    platforms. Copy libggml*.so / .dylib / .dll if present.
        for entry in fs::read_dir(src.parent().unwrap()).into_iter().flatten().flatten() {
            let name = entry.file_name();
            let name_s = name.to_string_lossy();
            if name_s.starts_with("libggml") || name_s.starts_with("ggml") {
                let _ = fs::copy(entry.path(), managed_bin_dir().join(&*name_s));
            }
        }

        // Cleanup.
        let _ = fs::remove_dir_all(&work);
        Ok(dest)
    })();

    match result {
        Ok(path) => emit(&window, InstallEvent {
            id, phase: "done".into(),
            detail: format!("installed at {}", path.display()),
            progress: Some(1.0),
        }),
        Err(e) => emit(&window, InstallEvent {
            id, phase: "error".into(), detail: e, progress: None,
        }),
    }
}

// ── piper: download + extract prebuilt archive ──────────────────────

fn piper_archive_url() -> Result<&'static str, String> {
    let url = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") =>
            "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_linux_x86_64.tar.gz",
        ("linux", "aarch64") =>
            "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_linux_aarch64.tar.gz",
        ("linux", "arm") =>
            "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_linux_armv7l.tar.gz",
        ("macos", "aarch64") =>
            "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_macos_aarch64.tar.gz",
        ("macos", "x86_64") =>
            "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_macos_x64.tar.gz",
        ("windows", "x86_64") =>
            "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip",
        (os, arch) => return Err(format!("no piper prebuilt for {os}/{arch}")),
    };
    Ok(url)
}

pub fn install_piper(window: tauri::Window, cancel: Arc<AtomicBool>) {
    let id = "piper".to_string();
    let result = (|| -> Result<PathBuf, String> {
        let url = piper_archive_url()?;
        let is_zip = url.ends_with(".zip");

        emit(&window, InstallEvent {
            id: id.clone(), phase: "downloading".into(),
            detail: url.to_string(), progress: None,
        });

        let resp = ureq::get(url).call().map_err(|e| format!("fetch piper: {e}"))?;
        let total: u64 = resp.header("Content-Length")
            .and_then(|s| s.parse().ok()).unwrap_or(0);

        let tmp_archive = std::env::temp_dir()
            .join(format!("forge-piper-{}{}", std::process::id(),
                if is_zip { ".zip" } else { ".tar.gz" }));
        let mut out = fs::File::create(&tmp_archive)
            .map_err(|e| format!("create tmp: {e}"))?;
        let mut reader = resp.into_reader();
        let mut buf = [0u8; 64 * 1024];
        let mut downloaded: u64 = 0;
        let mut last_emit = std::time::Instant::now();
        loop {
            if cancel.load(Ordering::Relaxed) {
                let _ = fs::remove_file(&tmp_archive);
                return Err("cancelled".into());
            }
            let n = reader.read(&mut buf).map_err(|e| format!("read: {e}"))?;
            if n == 0 { break; }
            use std::io::Write;
            out.write_all(&buf[..n]).map_err(|e| format!("write: {e}"))?;
            downloaded += n as u64;
            if last_emit.elapsed() >= std::time::Duration::from_millis(200) {
                emit(&window, InstallEvent {
                    id: id.clone(), phase: "downloading".into(),
                    detail: format!("{} / {}", downloaded, total),
                    progress: if total > 0 { Some(downloaded as f32 / total as f32) } else { None },
                });
                last_emit = std::time::Instant::now();
            }
        }
        drop(out);

        // Extract into {data_dir}/piper/. piper needs its espeak-ng-data
        // next to the binary at runtime, so we don't move just the binary.
        emit(&window, InstallEvent {
            id: id.clone(), phase: "extracting".into(),
            detail: "unpacking archive…".into(), progress: None,
        });

        let dest_dir = crate::models::data_dir().join("piper");
        let _ = fs::remove_dir_all(&dest_dir);
        fs::create_dir_all(&dest_dir).map_err(|e| format!("mkdir: {e}"))?;

        let status = if is_zip {
            // Bundled `tar` on Win10+ can extract zip via `tar -xf`.
            Command::new("tar")
                .args(["-xf", tmp_archive.to_str().unwrap(),
                       "-C", dest_dir.to_str().unwrap()])
                .status()
        } else {
            Command::new("tar")
                .args(["-xzf", tmp_archive.to_str().unwrap(),
                       "-C", dest_dir.to_str().unwrap(),
                       "--strip-components=1"])
                .status()
        }.map_err(|e| format!("tar: {e}. Ensure tar is installed."))?;
        if !status.success() { return Err("extraction failed".into()); }
        let _ = fs::remove_file(&tmp_archive);

        // Find the piper binary. usually at dest/piper or dest/piper.exe,
        // but some archives nest under `piper/` with strip-components=1
        // already flattening, which is why we used that flag above.
        let exe_name = if cfg!(windows) { "piper.exe" } else { "piper" };
        let piper_bin = dest_dir.join(exe_name);
        if !piper_bin.exists() {
            // Fallback: some archives don't support strip-components; scan one level.
            let nested = dest_dir.join("piper").join(exe_name);
            if nested.exists() {
                // Move everything up.
                for e in fs::read_dir(dest_dir.join("piper")).into_iter().flatten().flatten() {
                    let dst = dest_dir.join(e.file_name());
                    let _ = fs::rename(e.path(), dst);
                }
                let _ = fs::remove_dir(dest_dir.join("piper"));
            }
        }
        if !piper_bin.exists() {
            return Err(format!(
                "piper binary not found after extraction at {}", piper_bin.display()
            ));
        }
        set_exec(&piper_bin).ok();

        // Symlink / copy the binary into managed bin dir so the resolver
        // finds it on PATH-lookup. Copy is safer across platforms.
        let linked = managed_bin_dir().join(exe_name);
        let _ = fs::remove_file(&linked);
        fs::copy(&piper_bin, &linked).map_err(|e| format!("copy to bin: {e}"))?;
        set_exec(&linked).ok();

        Ok(piper_bin)
    })();

    match result {
        Ok(path) => emit(&window, InstallEvent {
            id, phase: "done".into(),
            detail: format!("installed at {}", path.display()),
            progress: Some(1.0),
        }),
        Err(e) => emit(&window, InstallEvent {
            id, phase: "error".into(), detail: e, progress: None,
        }),
    }
}

// ── status ──────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct BinaryStatus {
    pub whisper_cli: Option<String>,
    pub piper: Option<String>,
}

pub fn resolve_whisper_cli() -> Option<PathBuf> {
    resolve("FORGE_WHISPER_BIN", &["whisper-cli", "whisper-cpp", "main"])
}

pub fn resolve_piper() -> Option<PathBuf> {
    resolve("FORGE_PIPER_BIN", &["piper", "piper-tts"])
}

pub fn status() -> BinaryStatus {
    BinaryStatus {
        whisper_cli: resolve_whisper_cli().map(|p| p.display().to_string()),
        piper: resolve_piper().map(|p| p.display().to_string()),
    }
}

fn resolve(env_var: &str, names: &[&str]) -> Option<PathBuf> {
    if let Ok(v) = std::env::var(env_var) {
        let p = PathBuf::from(&v);
        if p.exists() { return Some(p); }
    }
    let managed = managed_bin_dir();
    for name in names {
        for suffix in if cfg!(windows) { &[".exe", ""][..] } else { &[""][..] } {
            let candidate = managed.join(format!("{name}{suffix}"));
            if candidate.exists() { return Some(candidate); }
        }
    }
    let home = dirs::home_dir().unwrap_or_default().join(".forge/bin");
    for name in names {
        for suffix in if cfg!(windows) { &[".exe", ""][..] } else { &[""][..] } {
            let candidate = home.join(format!("{name}{suffix}"));
            if candidate.exists() { return Some(candidate); }
        }
    }
    if let Some(paths) = std::env::var_os("PATH") {
        let suffixes: &[&str] = if cfg!(windows) { &[".exe", ""] } else { &[""] };
        for dir in std::env::split_paths(&paths) {
            for name in names {
                for s in suffixes {
                    let p = dir.join(format!("{name}{s}"));
                    if p.is_file() { return Some(p); }
                }
            }
        }
    }
    None
}
