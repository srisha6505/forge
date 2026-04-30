//! In-app terminal sessions backed by `portable-pty`.
//!
//! Each `spawn_terminal` call allocates a PTY pair, spawns the user's
//! login shell (or `cmd.exe` on Windows), and returns a numeric session
//! ID. A dedicated reader thread streams raw PTY output to the frontend
//! over the `terminal://output` event channel as base64-encoded chunks
//! (PTY output is arbitrary bytes including ANSI escapes, so a lossless
//! transport is required). Frontend keystrokes arrive as UTF-8 via
//! `write_terminal` and are fed straight back into the PTY's writer.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Mutex;

use base64::Engine;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::Serialize;
use tauri::{Emitter, Window};

pub type TerminalId = u32;

/// Live PTY session. We keep the master so we can resize, the writer so
/// we can forward input, and the child handle so kill is exact.
struct TerminalSession {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    // The writer is `Box<dyn Write + Send>` per portable-pty's API;
    // mutating it (write_all, flush) requires &mut access through the
    // outer Mutex.
    writer: Box<dyn Write + Send>,
}

#[derive(Default)]
struct TerminalRegistry {
    next_id: TerminalId,
    sessions: HashMap<TerminalId, TerminalSession>,
}

fn registry() -> &'static Mutex<TerminalRegistry> {
    // Single global registry so handlers can resolve sessions by ID.
    static R: std::sync::OnceLock<Mutex<TerminalRegistry>> = std::sync::OnceLock::new();
    R.get_or_init(|| Mutex::new(TerminalRegistry::default()))
}

#[derive(Serialize, Clone)]
struct TerminalOutputEvent {
    id: TerminalId,
    bytes_b64: String,
}

fn default_shell() -> CommandBuilder {
    // Linux/macOS: prefer $SHELL, fall back to /bin/bash. Windows: cmd.exe.
    #[cfg(unix)]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        let mut cmd = CommandBuilder::new(shell);
        // Login-style prompt + colors. -i so the shell sources rc files.
        cmd.env("TERM", "xterm-256color");
        cmd
    }
    #[cfg(windows)]
    {
        CommandBuilder::new("cmd.exe")
    }
}

fn home_dir() -> Option<String> {
    dirs::home_dir().and_then(|p| p.to_str().map(|s| s.to_string()))
}

#[tauri::command]
pub fn spawn_terminal(window: Window, vault_path: Option<String>) -> Result<TerminalId, String> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("openpty failed: {e}"))?;

    let mut cmd = default_shell();
    let cwd = vault_path
        .filter(|p| !p.is_empty())
        .or_else(home_dir);
    if let Some(dir) = cwd {
        cmd.cwd(dir);
    }

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("spawn shell failed: {e}"))?;

    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("take_writer failed: {e}"))?;

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("try_clone_reader failed: {e}"))?;

    let id = {
        let mut reg = registry().lock().map_err(|_| "registry poisoned")?;
        reg.next_id = reg.next_id.wrapping_add(1).max(1);
        let id = reg.next_id;
        reg.sessions.insert(
            id,
            TerminalSession {
                master: pair.master,
                child,
                writer,
            },
        );
        id
    };

    // Reader thread: synchronous std::thread, no async needed. Closes
    // when the PTY EOFs (shell exit) or read errors.
    let win = window.clone();
    std::thread::Builder::new()
        .name(format!("forge-term-{id}"))
        .spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let payload = TerminalOutputEvent {
                            id,
                            bytes_b64: base64::engine::general_purpose::STANDARD
                                .encode(&buf[..n]),
                        };
                        if win.emit("terminal://output", payload).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        })
        .map_err(|e| format!("spawn reader thread failed: {e}"))?;

    Ok(id)
}

#[tauri::command]
pub fn write_terminal(id: TerminalId, data: String) -> Result<(), String> {
    let mut reg = registry().lock().map_err(|_| "registry poisoned")?;
    let sess = reg
        .sessions
        .get_mut(&id)
        .ok_or_else(|| format!("no terminal {id}"))?;
    sess.writer
        .write_all(data.as_bytes())
        .map_err(|e| format!("write failed: {e}"))?;
    sess.writer.flush().map_err(|e| format!("flush failed: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn resize_terminal(id: TerminalId, cols: u16, rows: u16) -> Result<(), String> {
    let reg = registry().lock().map_err(|_| "registry poisoned")?;
    let sess = reg
        .sessions
        .get(&id)
        .ok_or_else(|| format!("no terminal {id}"))?;
    sess.master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("resize failed: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn kill_terminal(id: TerminalId) -> Result<(), String> {
    let mut reg = registry().lock().map_err(|_| "registry poisoned")?;
    if let Some(mut sess) = reg.sessions.remove(&id) {
        // Drop the writer first so the slave side sees EOF, then SIGKILL
        // the child to be sure. The reader thread will exit on its own
        // when the PTY closes.
        let _ = sess.writer.flush();
        drop(sess.writer);
        let _ = sess.child.kill();
        // Reap so the child isn't a zombie.
        let _ = sess.child.wait();
    }
    Ok(())
}

#[tauri::command]
pub fn list_terminals() -> Result<Vec<TerminalId>, String> {
    let reg = registry().lock().map_err(|_| "registry poisoned")?;
    let mut ids: Vec<TerminalId> = reg.sessions.keys().copied().collect();
    ids.sort_unstable();
    Ok(ids)
}
