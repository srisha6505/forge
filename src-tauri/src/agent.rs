//! Agent loop and tool execution. Adapted from Claurst's query loop pattern.
//!
//! The agent sends messages to the inference thread, handles streaming tokens,
//! detects tool calls, executes them, appends results, and loops until the
//! model finishes or the iteration cap is reached.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::llm::{ChatMessage, InferenceEvent, InferenceHandle, ToolCall};

// ── Agent events (sent to UI) ──

#[derive(Clone, Debug)]
pub enum AgentEvent {
    /// Streaming text chunk from the model.
    Token(String),
    /// Model's internal thinking/reasoning (hidden by default).
    Thinking(String),
    /// Model is calling a tool.
    ToolCallStarted { name: String, args: String },
    /// Tool execution finished.
    ToolCallResult { name: String, content: String, is_error: bool },
    /// Agent finished (no more tool calls). Carries the updated message
    /// history so the UI can persist it for subsequent turns.
    Finished { messages: Option<Vec<ChatMessage>> },
    /// An error occurred.
    Error(String),
}

// ── Helpers ──

/// Pull a filename hint out of a user message. Matches the first token
/// that looks like a markdown path (`<word>.md` or `<dir>/<word>.md`).
/// Returns None if no plausible filename is mentioned. Conservative on
/// purpose: false positives create wrong filenames, false negatives just
/// fall through to heading-derivation.
fn extract_filename_hint(text: &str) -> Option<String> {
    // Token = run of [A-Za-z0-9_./-] ending in `.md`. Avoid words like
    // "test.md," by trimming trailing punctuation. We do this by hand
    // rather than pulling regex into the dep tree.
    for raw in text.split(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '`') {
        let token = raw.trim_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '/' || c == '_' || c == '-' || c == '.'));
        if !token.ends_with(".md") {
            continue;
        }
        if token.len() < 4 {
            continue;
        }
        // Reject anything that starts with `.` (hidden files) or `/`
        // (absolute paths) — vault paths are always relative.
        if token.starts_with('.') || token.starts_with('/') {
            continue;
        }
        let valid = token.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '/' || c == '_' || c == '-' || c == '.'
        });
        if !valid {
            continue;
        }
        return Some(token.to_string());
    }
    None
}

// ── Tool context ──

pub struct ToolContext {
    pub vault_path: PathBuf,
    pub db_path: PathBuf,
    /// Shared vault search index. Same instance the search panel uses,
    /// so the agent benefits from already-built indexes and the embedder
    /// model that's already loaded in memory. Compiled out on Windows
    /// alongside the rest of the search subsystem (see Cargo.toml).
    #[cfg(not(target_os = "windows"))]
    pub search: std::sync::Arc<std::sync::Mutex<Option<crate::search::VaultSearch>>>,
    /// Path to the on-disk usearch vector index (used to lazy-init the
    /// shared `search` field if it hasn't been opened yet).
    #[cfg(not(target_os = "windows"))]
    pub search_index_path: PathBuf,
    /// Path to the on-disk SQLite chunks DB (paired with `search_index_path`).
    #[cfg(not(target_os = "windows"))]
    pub search_db_path: PathBuf,
    /// Filename hint extracted from the most recent user message (e.g.
    /// "grav.md" when the user said "make grav.md"). Used by write_file
    /// when the model drops the `path` arg. Set by the agent loop once
    /// per turn before dispatching tools.
    pub user_filename_hint: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    /// Last successful write_file path within this session. Used as the
    /// default path when the model retries write_file without a path —
    /// small models (Gemma 4 E4B) iteratively rewrite content and drop
    /// the path each time; reusing the prior path means iterative drafts
    /// land in one file instead of spawning N new files per retry.
    pub last_write_path: std::sync::Arc<std::sync::Mutex<Option<String>>>,
}

// ── Tool result ──

pub struct ToolResult {
    pub content: String,
    pub is_error: bool,
}

// ── Path validation helper ──

/// Strip the Windows UNC prefix `\\?\` so that `Path::starts_with` works
/// against a non-prefixed vault root. `std::fs::canonicalize` on Windows
/// returns extended-length paths (e.g. `\\?\C:\Users\code\vault`) while
/// the rest of the codebase keeps `vault_path` un-prefixed, so the
/// bounds check `canonical.starts_with(vault_root)` was rejecting every
/// list_files / read_file the agent attempted on Windows. No-op on
/// Linux/Mac.
#[cfg(target_os = "windows")]
fn strip_unc(p: PathBuf) -> PathBuf {
    if let Some(s) = p.to_str() {
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    p
}
#[cfg(not(target_os = "windows"))]
fn strip_unc(p: PathBuf) -> PathBuf { p }

/// Canonicalize both sides of a vault-bounds check before comparing.
/// Returns true if `candidate` resolves to a path inside (or equal to)
/// `vault_root`. False on any canonicalize failure or out-of-bounds.
fn is_within_vault(candidate: &Path, vault_root: &Path) -> bool {
    let Ok(c) = candidate.canonicalize() else { return false; };
    let v = vault_root.canonicalize().unwrap_or_else(|_| vault_root.to_path_buf());
    strip_unc(c).starts_with(strip_unc(v))
}

/// Validate that a path resolves within the vault root. For existing paths uses
/// canonicalize(); for new paths (write_file, rename target) canonicalizes the
/// parent directory and appends the file name.
fn validate_vault_path(vault_root: &Path, rel_path: &str, must_exist: bool) -> Result<PathBuf, ToolResult> {
    if rel_path.is_empty() {
        return Err(ToolResult { content: "No path provided".into(), is_error: true });
    }

    let full_path = vault_root.join(rel_path);

    if must_exist {
        match full_path.canonicalize() {
            Ok(canonical) => {
                if !is_within_vault(&full_path, vault_root) {
                    return Err(ToolResult {
                        content: "Path is outside the vault".into(),
                        is_error: true,
                    });
                }
                Ok(strip_unc(canonical))
            }
            Err(_) => Err(ToolResult {
                content: format!("File not found: {rel_path}"),
                is_error: true,
            }),
        }
    } else {
        // For paths that may not exist yet, canonicalize the parent.
        let parent = full_path.parent().unwrap_or(vault_root);
        if parent.exists() {
            if !is_within_vault(parent, vault_root) {
                return Err(ToolResult {
                    content: "Path is outside the vault".into(),
                    is_error: true,
                });
            }
        } else {
            // Walk up to find an existing ancestor and check that's in
            // the vault. Catches `../../etc/passwd`-style escapes even
            // when the literal full_path doesn't exist.
            let mut ancestor = parent.to_path_buf();
            while !ancestor.exists() {
                if let Some(p) = ancestor.parent() {
                    ancestor = p.to_path_buf();
                } else {
                    break;
                }
            }
            if !is_within_vault(&ancestor, vault_root) {
                return Err(ToolResult {
                    content: "Path is outside the vault".into(),
                    is_error: true,
                });
            }
        }

        Ok(full_path)
    }
}

// ── Tool schemas (OpenAI function-calling format) ──

pub fn tool_schemas() -> Vec<serde_json::Value> {
    let mut tools = serde_json::json!([
        // search_vault is appended below on non-Windows targets only.
        // Windows builds skip the entire vault-search subsystem (see
        // Cargo.toml comment), so the tool is hidden from the model
        // rather than registered as a no-op.
        {
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read the full content of a markdown file from the vault.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path relative to vault root, e.g. 'notes/topic.md'"
                        }
                    },
                    "required": ["path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_files",
                "description": "List markdown files in a vault directory.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "directory": {
                            "type": "string",
                            "description": "Directory path relative to vault root (default: root)"
                        }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "read_section",
                "description": "Read a specific heading section from a markdown file.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path relative to vault root"
                        },
                        "heading": {
                            "type": "string",
                            "description": "The heading text to find (e.g. '## Overview')"
                        }
                    },
                    "required": ["path", "heading"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "Create or overwrite a file in the vault. Creates parent directories if needed.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path relative to vault root"
                        },
                        "content": {
                            "type": "string",
                            "description": "The content to write to the file"
                        }
                    },
                    "required": ["path", "content"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "edit_file",
                "description": "Replace the first occurrence of old_text with new_text in a file. Returns an error if old_text is not found.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path relative to vault root"
                        },
                        "old_text": {
                            "type": "string",
                            "description": "The text to find and replace"
                        },
                        "new_text": {
                            "type": "string",
                            "description": "The replacement text"
                        }
                    },
                    "required": ["path", "old_text", "new_text"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "rename_file",
                "description": "Rename or move a file within the vault.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "old_path": {
                            "type": "string",
                            "description": "Current file path relative to vault root"
                        },
                        "new_path": {
                            "type": "string",
                            "description": "New file path relative to vault root"
                        }
                    },
                    "required": ["old_path", "new_path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "delete_file",
                "description": "Delete a file from the vault.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path relative to vault root"
                        }
                    },
                    "required": ["path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "web_search",
                "description": "Search the web using DuckDuckGo. Returns titles and snippets from search results.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The search query"
                        },
                        "num_results": {
                            "type": "integer",
                            "description": "Number of results to return (default 5)"
                        }
                    },
                    "required": ["query"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "grep_vault",
                "description": "Search for a text pattern across all vault files. Returns matching lines with file:line format. Case insensitive.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "The text pattern to search for"
                        },
                        "file_glob": {
                            "type": "string",
                            "description": "File extension filter, e.g. '*.md' (default '*.md')"
                        }
                    },
                    "required": ["pattern"]
                }
            }
        }
    ])
    .as_array()
    .cloned()
    .unwrap_or_default();

    #[cfg(not(target_os = "windows"))]
    tools.push(serde_json::json!({
        "type": "function",
        "function": {
            "name": "search_vault",
            "description": "Search the user's note vault using keyword and semantic search. Returns matching chunks with file paths, headings, and content snippets.",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "The search query" },
                    "limit": { "type": "integer", "description": "Maximum number of results (default 5)" }
                },
                "required": ["query"]
            }
        }
    }));

    tools
}

// ── Tool execution ──

pub fn execute_tool(tool_call: &ToolCall, ctx: &ToolContext) -> ToolResult {
    // Sanitize: if value is literally the word "thought" (model leaking),
    // blank it out so list_files/directory="" lists root, etc.
    let mut sanitized = tool_call.clone();
    if let Some(obj) = sanitized.arguments.as_object_mut() {
        for (_key, val) in obj.iter_mut() {
            if let Some(s) = val.as_str() {
                let trimmed = s.trim();
                if trimmed == "thought" || trimmed == "Thought" {
                    *val = serde_json::Value::String(String::new());
                }
            }
        }
    }
    let tool_call = &sanitized;
    match tool_call.name.as_str() {
        #[cfg(not(target_os = "windows"))]
        "search_vault" => exec_search_vault(tool_call, ctx),
        "read_file" => exec_read_file(tool_call, ctx),
        "list_files" => exec_list_files(tool_call, ctx),
        "read_section" => exec_read_section(tool_call, ctx),
        "write_file" => exec_write_file(tool_call, ctx),
        "edit_file" => exec_edit_file(tool_call, ctx),
        "rename_file" => exec_rename_file(tool_call, ctx),
        "delete_file" => exec_delete_file(tool_call, ctx),
        "web_search" => exec_web_search(tool_call),
        "grep_vault" => exec_grep_vault(tool_call, ctx),
        other => ToolResult {
            content: format!("Unknown tool: {other}"),
            is_error: true,
        },
    }
}

#[cfg(not(target_os = "windows"))]
fn exec_search_vault(tc: &ToolCall, ctx: &ToolContext) -> ToolResult {
    let query = tc.arguments.get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let limit = tc.arguments.get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(8) as usize;

    if query.is_empty() {
        return ToolResult { content: "Empty search query".into(), is_error: true };
    }

    // Check if init needed without holding the lock long.
    let needs_init = {
        let guard = ctx.search.lock().unwrap_or_else(|e| e.into_inner());
        guard.is_none()
    };

    // Build VaultSearch OUTSIDE the mutex. Prevents deadlock with concurrent
    // reindex/search calls and stops a build panic from poisoning the lock.
    if needs_init {
        let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut vs = crate::search::VaultSearch::new(&ctx.search_db_path, &ctx.search_index_path)
                .map_err(|e| format!("open index: {e}"))?;
            if vs.chunk_count() == 0 {
                vs.build_vault(&ctx.vault_path)
                    .map_err(|e| format!("build vault: {e}"))?;
                let _ = vs.save_index(&ctx.search_index_path);
            }
            Ok::<_, String>(vs)
        }));
        match built {
            Ok(Ok(vs)) => {
                let mut guard = ctx.search.lock().unwrap_or_else(|e| e.into_inner());
                *guard = Some(vs);
            }
            Ok(Err(e)) => return ToolResult { content: e, is_error: true },
            Err(_) => return ToolResult {
                content: "Search init panicked".into(),
                is_error: true,
            },
        }
    }

    let guard = ctx.search.lock().unwrap_or_else(|e| e.into_inner());
    let vs = match guard.as_ref() {
        Some(vs) => vs,
        None => return ToolResult {
            content: "Search not available".into(),
            is_error: true,
        },
    };
    let results = match vs.search(query, limit) {
        Ok(r) => r,
        Err(e) => return ToolResult {
            content: format!("Search error: {e}"),
            is_error: true,
        },
    };

    if results.is_empty() {
        return ToolResult { content: "No results.".into(), is_error: false };
    }

    let mut out = String::new();
    let vault_str = ctx.vault_path.to_string_lossy().to_string();
    for r in results {
        let path_str = r.chunk.file_path.to_string_lossy().to_string();
        let rel = path_str
            .strip_prefix(&vault_str)
            .unwrap_or(&path_str)
            .trim_start_matches('/');
        // UTF-8-safe truncation: clamp to char boundary.
        let snippet = if r.chunk.content.chars().count() > 320 {
            let mut end = 0usize;
            for (i, _) in r.chunk.content.char_indices().take(320) {
                end = i;
            }
            format!("{}…", r.chunk.content[..end].trim_end())
        } else {
            r.chunk.content.clone()
        };
        out.push_str(&format!(
            "## {rel} — {} (score {:.2})\n{snippet}\n\n",
            r.chunk.heading, r.score
        ));
    }
    ToolResult { content: out, is_error: false }
}

fn exec_read_file(tc: &ToolCall, ctx: &ToolContext) -> ToolResult {
    let rel_path = tc.arguments.get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let full_path = match validate_vault_path(&ctx.vault_path, rel_path, true) {
        Ok(p) => p,
        Err(e) => return e,
    };

    match fs::read_to_string(&full_path) {
        Ok(content) => {
            const MAX_CHARS: usize = 60_000;
            let truncated = if content.chars().count() > MAX_CHARS {
                let mut end = 0usize;
                for (i, _) in content.char_indices().take(MAX_CHARS) {
                    end = i;
                }
                format!(
                    "{}…\n\n[truncated at {MAX_CHARS} chars; {} bytes total]",
                    &content[..end],
                    content.len()
                )
            } else {
                content
            };
            ToolResult { content: truncated, is_error: false }
        }
        Err(e) => ToolResult {
            content: format!("Failed to read {rel_path}: {e}"),
            is_error: true,
        },
    }
}

fn exec_list_files(tc: &ToolCall, ctx: &ToolContext) -> ToolResult {
    let rel_dir = tc.arguments.get("directory")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let dir = if rel_dir.is_empty() {
        ctx.vault_path.clone()
    } else {
        ctx.vault_path.join(rel_dir)
    };

    // Path traversal check. is_within_vault canonicalizes both sides
    // and strips the Windows UNC prefix; without that the bare
    // `canonical.starts_with(vault)` always failed on Windows since
    // canonicalize returned `\\?\C:\...` and the vault didn't.
    if !is_within_vault(&dir, &ctx.vault_path) {
        return ToolResult {
            content: "Directory is outside the vault".into(),
            is_error: true,
        };
    }

    if !dir.is_dir() {
        return ToolResult {
            content: format!("Not a directory: {rel_dir}"),
            is_error: true,
        };
    }

    let mut entries = Vec::new();
    if let Ok(read_dir) = fs::read_dir(&dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            let name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if name.starts_with('.') { continue; }

            if path.is_dir() {
                entries.push(format!("{name}/"));
            } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                entries.push(name);
            }
        }
    }

    entries.sort();

    if entries.is_empty() {
        ToolResult { content: "Directory is empty or has no .md files.".into(), is_error: false }
    } else {
        ToolResult { content: entries.join("\n"), is_error: false }
    }
}

fn exec_read_section(tc: &ToolCall, ctx: &ToolContext) -> ToolResult {
    let rel_path = tc.arguments.get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let heading = tc.arguments.get("heading")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if rel_path.is_empty() || heading.is_empty() {
        return ToolResult { content: "Both path and heading are required".into(), is_error: true };
    }

    let full_path = match validate_vault_path(&ctx.vault_path, rel_path, true) {
        Ok(p) => p,
        Err(e) => return e,
    };

    let content = match fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(e) => return ToolResult {
            content: format!("Failed to read {rel_path}: {e}"),
            is_error: true,
        },
    };

    // Find section by heading match (case-insensitive substring).
    let heading_lower = heading.to_lowercase();
    let mut found_section = None;
    let mut current_text = String::new();
    let mut in_target = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            if in_target {
                // Hit next heading, stop collecting.
                found_section = Some(current_text.clone());
                break;
            }
            if trimmed.to_lowercase().contains(&heading_lower) {
                in_target = true;
                current_text = format!("{line}\n");
                continue;
            }
        }
        if in_target {
            current_text.push_str(line);
            current_text.push('\n');
        }
    }

    if in_target && found_section.is_none() {
        found_section = Some(current_text);
    }

    match found_section {
        Some(text) => ToolResult { content: text, is_error: false },
        None => ToolResult {
            content: format!("Heading '{heading}' not found in {rel_path}"),
            is_error: true,
        },
    }
}

// ── New tool implementations ──

/// Clean up content emitted by small models that mis-escape on the way
/// through JSON serialisation. Two distinct cases that both showed up
/// repeatedly with Gemma 4 E4B:
///
/// 1. The model wraps the whole content in an extra pair of `"` chars
///    (treats the content slot as needing its own JSON encoding). After
///    parsing, the file starts with a literal `"` and ends with one.
///    Strip when BOTH ends are `"` — markdown content essentially never
///    starts and ends with a quote at the same time.
///
/// 2. Single backslashes that should be double: `\frac` → JSON parser
///    eats the `\f` as the form-feed control character (U+000C). The
///    rendered note shows `Gfrac{...}` because the invisible char gets
///    dropped on display. Same trap with `\b` (U+0008), `\v` (U+000B),
///    `\t` (U+0009 — kept; tabs are legitimate). We replace these
///    control chars with their LaTeX-style backslash-letter form so the
///    common math-mode escapes survive.
fn sanitize_model_content(raw: &str) -> String {
    let mut s = raw.to_string();
    // Strip outer-quote wrapping if present on both ends.
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        let inner: String = s[1..s.len() - 1].to_string();
        // Only strip if the inner doesn't itself start with `"` — avoids
        // mangling content that genuinely begins with a quoted string.
        if !inner.starts_with('"') {
            s = inner;
        }
    }
    // Recover backslash-letter LaTeX escapes that JSON tokenized as control chars.
    s = s
        .replace('\u{0008}', "\\b")
        .replace('\u{000B}', "\\v")
        .replace('\u{000C}', "\\f")
        // Bell isn't standard LaTeX but it's the same model bug for `\a`.
        .replace('\u{0007}', "\\a");
    s
}

fn exec_write_file(tc: &ToolCall, ctx: &ToolContext) -> ToolResult {
    // The contract is simple: model gives us `content` and `path`, we write
    // the bytes to disk. No sanitization, no escape decoding, no path
    // guessing, no content rescue. If the model gives bad args, we fail
    // and let the model see the error and retry — that's how an agent
    // loop is supposed to work.
    let mut rel_path = tc.arguments.get("path")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    let content = tc.arguments.get("content")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    // The only fallback we keep: if the model omits `path` but the user's
    // message explicitly named a file (e.g., "create foo.md"), use that.
    // Gemma 4 E4B sometimes drops the path field on long write_file calls
    // and the user's intent is unambiguous in that case. No other path
    // fallbacks — slug derivation, timestamp, last-write reuse all gone.
    if rel_path.is_empty() {
        if let Some(hint) = ctx.user_filename_hint.lock().ok().and_then(|g| g.clone()) {
            eprintln!("[forge-agent] write_file using user filename hint: {:?}", hint);
            rel_path = hint;
        }
    }
    if rel_path.is_empty() {
        return ToolResult {
            content: "write_file requires a non-empty `path` argument.".to_string(),
            is_error: true,
        };
    }

    // Force `.md` extension. The vault is a markdown notebook — anything
    // the agent writes that lacks `.md`/`.markdown` gets `.md` appended
    // so the file shows up in the sidebar (which filters by extension).
    {
        let lower = rel_path.to_lowercase();
        if !lower.ends_with(".md") && !lower.ends_with(".markdown") {
            rel_path.push_str(".md");
        }
    }

    let full_path = match validate_vault_path(&ctx.vault_path, &rel_path, false) {
        Ok(p) => p,
        Err(e) => return e,
    };

    // Create parent directories if needed.
    if let Some(parent) = full_path.parent() {
        if !parent.exists() {
            if let Err(e) = fs::create_dir_all(parent) {
                return ToolResult {
                    content: format!("Failed to create directories: {e}"),
                    is_error: true,
                };
            }
        }
    }

    eprintln!("[forge-agent] write_file: writing {} bytes to {}", content.len(), rel_path);

    match fs::write(&full_path, &content) {
        Ok(()) => {
            // Best-effort incremental search index update so the file is
            // findable immediately. Failure here doesn't fail the tool.
            // Skipped on Windows where the search subsystem is compiled out.
            #[cfg(not(target_os = "windows"))]
            if let Ok(mut guard) = ctx.search.lock() {
                if let Some(vs) = guard.as_mut() {
                    let _ = vs.index_file(&full_path);
                    let _ = vs.save_index(&ctx.search_index_path);
                }
            }
            // Remember this path so a follow-up write_file with a missing
            // path lands here instead of spawning a new file.
            if let Ok(mut g) = ctx.last_write_path.lock() {
                *g = Some(rel_path.clone());
            }
            ToolResult {
                content: format!("Successfully wrote {} bytes to {}", content.len(), rel_path),
                is_error: false,
            }
        }
        Err(e) => ToolResult {
            content: format!("Failed to write {}: {}", rel_path, e),
            is_error: true,
        },
    }
}

fn exec_edit_file(tc: &ToolCall, ctx: &ToolContext) -> ToolResult {
    let rel_path = tc.arguments.get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let old_text = tc.arguments.get("old_text")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let new_text = tc.arguments.get("new_text")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if rel_path.is_empty() {
        return ToolResult { content: "No path provided".into(), is_error: true };
    }
    if old_text.is_empty() {
        return ToolResult { content: "old_text cannot be empty".into(), is_error: true };
    }

    let full_path = match validate_vault_path(&ctx.vault_path, rel_path, true) {
        Ok(p) => p,
        Err(e) => return e,
    };

    let content = match fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(e) => return ToolResult {
            content: format!("Failed to read {rel_path}: {e}"),
            is_error: true,
        },
    };

    if !content.contains(old_text) {
        return ToolResult {
            content: format!("old_text not found in {rel_path}"),
            is_error: true,
        };
    }

    // Replace first occurrence only.
    let updated = if let Some(pos) = content.find(old_text) {
        let mut result = String::with_capacity(content.len() - old_text.len() + new_text.len());
        result.push_str(&content[..pos]);
        result.push_str(new_text);
        result.push_str(&content[pos + old_text.len()..]);
        result
    } else {
        // Should not reach here due to the contains check above.
        content
    };

    match fs::write(&full_path, &updated) {
        Ok(()) => ToolResult {
            content: format!("Edited {rel_path}: replaced {} bytes with {} bytes",
                old_text.len(), new_text.len()),
            is_error: false,
        },
        Err(e) => ToolResult {
            content: format!("Failed to write {rel_path}: {e}"),
            is_error: true,
        },
    }
}

fn exec_rename_file(tc: &ToolCall, ctx: &ToolContext) -> ToolResult {
    let old_rel = tc.arguments.get("old_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let new_rel = tc.arguments.get("new_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if old_rel.is_empty() || new_rel.is_empty() {
        return ToolResult {
            content: "Both old_path and new_path are required".into(),
            is_error: true,
        };
    }

    // Old path must exist.
    let old_full = match validate_vault_path(&ctx.vault_path, old_rel, true) {
        Ok(p) => p,
        Err(e) => return e,
    };

    // New path may not exist yet.
    let new_full = match validate_vault_path(&ctx.vault_path, new_rel, false) {
        Ok(p) => p,
        Err(e) => return e,
    };

    // Create parent directories for new path if needed.
    if let Some(parent) = new_full.parent() {
        if !parent.exists() {
            if let Err(e) = fs::create_dir_all(parent) {
                return ToolResult {
                    content: format!("Failed to create directories: {e}"),
                    is_error: true,
                };
            }
        }
    }

    match fs::rename(&old_full, &new_full) {
        Ok(()) => ToolResult {
            content: format!("Renamed {old_rel} -> {new_rel}"),
            is_error: false,
        },
        Err(e) => ToolResult {
            content: format!("Failed to rename: {e}"),
            is_error: true,
        },
    }
}

fn exec_delete_file(tc: &ToolCall, ctx: &ToolContext) -> ToolResult {
    let rel_path = tc.arguments.get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let full_path = match validate_vault_path(&ctx.vault_path, rel_path, true) {
        Ok(p) => p,
        Err(e) => return e,
    };

    match fs::remove_file(&full_path) {
        Ok(()) => ToolResult {
            content: format!("Deleted {rel_path}"),
            is_error: false,
        },
        Err(e) => ToolResult {
            content: format!("Failed to delete {rel_path}: {e}"),
            is_error: true,
        },
    }
}

fn exec_web_search(tc: &ToolCall) -> ToolResult {
    let query = tc.arguments.get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let num_results = tc.arguments.get("num_results")
        .and_then(|v| v.as_u64())
        .unwrap_or(5) as usize;

    if query.is_empty() {
        return ToolResult { content: "Empty search query".into(), is_error: true };
    }

    let url = format!("https://html.duckduckgo.com/html/?q={}", urlencoded(query));

    let response = match ureq::get(&url)
        .set("User-Agent", "Mozilla/5.0 (compatible; ForgeAgent/0.1)")
        .timeout(std::time::Duration::from_secs(15))
        .call()
    {
        Ok(r) => r,
        Err(e) => return ToolResult {
            content: format!("Web search request failed: {e}"),
            is_error: true,
        },
    };

    let body = match response.into_string() {
        Ok(b) => b,
        Err(e) => return ToolResult {
            content: format!("Failed to read search response: {e}"),
            is_error: true,
        },
    };

    // Parse DuckDuckGo HTML results. Each result block has:
    //   <a ... class="result__a" href="...">TITLE</a>
    //   <a class="result__snippet" href="...">SNIPPET with optional <b> tags</a>
    // We split on `class="result__a"`, skip the first chunk (pre-results
    // HTML), and parse each subsequent chunk as one result.
    let mut results = Vec::new();

    for chunk in body.split("class=\"result__a\"").skip(1) {
        if results.len() >= num_results {
            break;
        }

        // Title: text between the opening tag's `>` and `</a>`. Titles
        // rarely contain nested tags so this is safe.
        let title = extract_between(chunk, ">", "</a>")
            .map(|t| strip_html_tags(&t))
            .unwrap_or_default();

        if title.is_empty() {
            continue;
        }

        // URL: from href attribute, with DuckDuckGo redirect unwrapping.
        let href = extract_between(chunk, "href=\"", "\"").unwrap_or_default();
        let url = if href.contains("uddg=") {
            href.split("uddg=").nth(1)
                .and_then(|u| u.split('&').next())
                .map(|u| urldecoded(u))
                .unwrap_or(href)
        } else {
            href
        };

        // Snippet: find `result__snippet`, advance past the opening tag's
        // closing `>`, then capture until `</a>` specifically. Using
        // `</a>` (not generic `</`) prevents truncation at inner `<b>`
        // highlight tags.
        let snippet = if let Some(pos) = chunk.find("result__snippet") {
            let rest = &chunk[pos..];
            if let Some(gt) = rest.find('>') {
                let content = &rest[gt + 1..];
                let end = content.find("</a>").unwrap_or(content.len());
                strip_html_tags(&content[..end]).trim().to_string()
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        results.push(format!("{}. {}\n   {}\n   {}", results.len() + 1, title, url, snippet));
    }

    if results.is_empty() {
        ToolResult {
            content: "No search results found.".into(),
            is_error: false,
        }
    } else {
        ToolResult {
            content: results.join("\n\n"),
            is_error: false,
        }
    }
}

fn exec_grep_vault(tc: &ToolCall, ctx: &ToolContext) -> ToolResult {
    let pattern = tc.arguments.get("pattern")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let file_glob = tc.arguments.get("file_glob")
        .and_then(|v| v.as_str())
        .unwrap_or("*.md");

    if pattern.is_empty() {
        return ToolResult { content: "Empty search pattern".into(), is_error: true };
    }

    // Extract the extension from the glob (simple handling for *.ext patterns).
    let extension = if file_glob.starts_with("*.") {
        Some(&file_glob[2..])
    } else if file_glob == "*" {
        None
    } else {
        // Try to extract extension from more complex globs.
        file_glob.rsplit('.').next()
    };

    let pattern_lower = pattern.to_lowercase();
    let mut matches = Vec::new();
    let max_matches = 50;

    // Recursively walk the vault directory.
    let mut dirs_to_visit = vec![ctx.vault_path.clone()];

    while let Some(dir) = dirs_to_visit.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            // Skip hidden files/directories.
            if name.starts_with('.') {
                continue;
            }

            if path.is_dir() {
                dirs_to_visit.push(path);
                continue;
            }

            // Check extension filter.
            if let Some(ext) = extension {
                let file_ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if file_ext != ext {
                    continue;
                }
            }

            // Skip large files — reading 100MB+ as string explodes memory.
            const MAX_GREP_BYTES: u64 = 2_000_000;
            if let Ok(meta) = fs::metadata(&path) {
                if meta.len() > MAX_GREP_BYTES {
                    continue;
                }
            }
            // Read file and search for pattern.
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let rel_path = path.strip_prefix(&ctx.vault_path)
                .unwrap_or(&path)
                .to_string_lossy();

            for (line_num, line) in content.lines().enumerate() {
                if line.to_lowercase().contains(&pattern_lower) {
                    matches.push(format!("{rel_path}:{}: {}", line_num + 1, line.trim()));
                    if matches.len() >= max_matches {
                        break;
                    }
                }
            }

            if matches.len() >= max_matches {
                break;
            }
        }

        if matches.len() >= max_matches {
            break;
        }
    }

    if matches.is_empty() {
        ToolResult {
            content: format!("No matches found for '{pattern}'"),
            is_error: false,
        }
    } else {
        let count = matches.len();
        let truncated = if count >= max_matches {
            format!("\n\n[Results capped at {max_matches} matches]")
        } else {
            String::new()
        };
        ToolResult {
            content: format!("{}{truncated}", matches.join("\n")),
            is_error: false,
        }
    }
}

// ── HTML / URL helpers ──

/// Minimal URL-encoding for query strings (spaces, special chars).
fn urlencoded(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            b' ' => result.push('+'),
            _ => {
                result.push('%');
                result.push_str(&format!("{b:02X}"));
            }
        }
    }
    result
}

/// Minimal percent-decoding.
fn urldecoded(s: &str) -> String {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(val) = u8::from_str_radix(
                &String::from_utf8_lossy(&bytes[i + 1..i + 3]), 16
            ) {
                result.push(val);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            result.push(b' ');
        } else {
            result.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&result).into_owned()
}

/// Extract text between two delimiters (first occurrence).
fn extract_between(s: &str, start: &str, end: &str) -> Option<String> {
    let start_pos = s.find(start)? + start.len();
    let rest = &s[start_pos..];
    let end_pos = rest.find(end)?;
    Some(rest[..end_pos].to_string())
}

/// Strip HTML tags from a string, also decode common HTML entities.
fn strip_html_tags(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    // Decode common HTML entities.
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

// ── Agent loop ──

/// Default system prompt for the research agent.
pub fn default_system_prompt(vault_name: &str) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Simple YYYY-MM-DD from unix timestamp (UTC).
    let days = now / 86400;
    let (year, month, day) = {
        let mut d = days as i64 + 719468;
        let era = if d >= 0 { d } else { d - 146096 } / 146097;
        let doe = (d - era * 146097) as u64;
        let yoe = (doe - doe/1460 + doe/36524 - doe/146096) / 365;
        let y = yoe as i64 + era * 400;
        let doy = doe - (365*yoe + yoe/4 - yoe/100);
        let mp = (5*doy + 2) / 153;
        let day = (doy - (153*mp + 2)/5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let year = if month <= 2 { y + 1 } else { y };
        (year, month, day)
    };
    format!(
        "You are a research assistant for the Obsidian-style vault \"{vault_name}\".\n\
         \n\
         STATE:\n\
         - Today's date: {year:04}-{month:02}-{day:02}\n\
         - Vault name: {vault_name}\n\
         - Your files live INSIDE this vault. You do NOT know what is in the vault until you call list_files or search_vault.\n\
         \n\
         CRITICAL: NEVER invent or hallucinate file names. Always call list_files or search_vault first to see the REAL files. If the tool returns a list, USE that exact list — do not substitute made-up names like \"pasta_carbonara.md\" or \"stoicism.md\". Those are NOT in the vault unless the tool returned them.\n\
         \n\
         NEVER output your thinking process. No \"thought\" blocks. No narration. Just act.\n\
         \n\
         WORKFLOW:\n\
         - User asks \"what's in my vault?\" → call list_files with directory:\"\" (empty string = root) → report the actual results.\n\
         - User asks to write a note → call web_search if needed → call write_file with valid path + content.\n\
         - User asks about a specific topic → call search_vault → read_file for details → answer.\n\
         \n\
         TOOL RULES:\n\
         1. Tool arguments = DATA ONLY. Never put reasoning/thinking in arguments.\n\
         2. list_files: directory arg should be \"\" (empty) for root, or a subfolder name. Never put thinking there.\n\
         3. Search queries: 3-8 keywords. Example: \"Iran US Strait Hormuz 2025\"\n\
         4. NEVER repeat the same tool call with identical arguments.\n\
         5. write_file: REQUIRED fields are 'path' (e.g. \"notes/topic.md\") AND 'content'. Path must end in .md.\n\
         6. After tool results, USE the results. Do not ignore them and hallucinate.\n\
         \n\
         RESPONSE RULES:\n\
         - Report what the tool actually returned. If list_files returned files A, B, C, say A, B, C.\n\
         - Cite actual file paths, not invented ones.\n\
         - Be concise. Synthesize, don't dump raw results.\n\
         - NO emojis anywhere (headings, body, widgets).\n\
         - NO em-dashes (use a comma, period, or colon instead).\n\
         \n\
         INTERACTIVE WIDGETS (mandatory format — read carefully):\n\
         The ONLY way to render an interactive widget is a triple-backtick FENCED CODE BLOCK with info string `js-widget`. Inside that fence, put EVERYTHING the widget needs — sliders, canvases, divs, scripts. Sliders outside the fence cannot communicate with scripts inside it (the widget runs in an iframe, sliders in markdown live in the parent DOM).\n\
         \n\
         RIGHT (do this):\n\
         ```js-widget height=320\n\
         <canvas id=\"c\" style=\"width:100%;height:200px\"></canvas>\n\
         <input id=\"f\" type=\"range\" min=\"0.1\" max=\"5\" value=\"1\">\n\
         <script>const c=document.getElementById('c'); const f=document.getElementById('f'); /* ... */</script>\n\
         ```\n\
         \n\
         WRONG (none of these work — the renderer ignores them):\n\
         - Bare <canvas>/<script> in the markdown body without a fence\n\
         - Slider <input> placed BEFORE/OUTSIDE the fence (script can't see it)\n\
         - Using ```html or no info string instead of ```js-widget\n\
         \n\
         The info string `js-widget` is intentionally not `html`, don't shorten or substitute. If you put any widget code outside a js-widget fence, NOTHING runs and the user sees raw HTML source text.\n\
         \n\
         WIDGET STYLING (mandatory, both themes must work):\n\
         - Use ONLY theme CSS variables for colors. Available inside the iframe:\n\
             var(--color-bg)              page background\n\
             var(--color-bg-alt)          canvas background, cards\n\
             var(--color-text)            primary text and lines\n\
             var(--color-text-secondary)  axes, grid, ticks, secondary text\n\
             var(--color-accent)          ONE highlight color (the data line, the active marker)\n\
             var(--color-success)         optional second series\n\
             var(--color-error)           optional warning/loss color\n\
             var(--color-border)          borders, separators\n\
         - In canvas drawing, read these via getComputedStyle:\n\
             const css = getComputedStyle(document.documentElement);\n\
             const accent = css.getPropertyValue('--color-accent').trim();\n\
         - NEVER hardcode hex colors (no '#fff', no 'black', no 'rgb(...)' literals).\n\
         - NEVER produce rainbow series with hsl(i/N*360, ...). Use accent for the primary curve, secondary for everything else. Color is signal, not decoration.\n\
         \n\
         WIDGET SCALES (mandatory for any plot):\n\
         - Draw axis lines (left edge and bottom edge) in var(--color-text-secondary).\n\
         - Draw 4-5 tick labels on each axis with their numeric values.\n\
         - Draw a faint grid in var(--color-border) at the tick positions.\n\
         - Label the axes (xLabel, yLabel) so the viewer knows what the axes represent.\n\
         - Reserve ~40px on the left and ~30px at the bottom for labels and ticks.\n\
         - A naked curve on a colored canvas with no scale is unacceptable."
    )
}

/// Run the agent loop in a blocking fashion (call from a background thread).
/// Sends AgentEvents through the provided sender so the UI can update.
pub fn run_agent_loop(
    inference: &InferenceHandle,
    messages: &mut Vec<ChatMessage>,
    tools: &[serde_json::Value],
    ctx: &ToolContext,
    max_iterations: usize,
    event_tx: &mpsc::Sender<AgentEvent>,
) {
    let mut iterations = 0;
    // Loop-break guard: remember the last tool call signature and
    // consecutive failure count so we can bail out if the model keeps
    // retrying the same failing call.
    let mut last_call_sig: Option<String> = None;
    let mut consecutive_failures = 0usize;
    let mut seen_calls: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Pull a filename hint out of the user's most recent message: any
    // bareword ending in `.md` (e.g. "make grav.md", "save it as
    // research/x.md"). This is the highest-priority fallback for
    // path-less write_file calls — covers the case where the model drops
    // the path arg but the user did say what to call the file.
    {
        let mut hint: Option<String> = None;
        for m in messages.iter().rev() {
            if !matches!(m.role, crate::llm::ChatRole::User) {
                continue;
            }
            let re_hint = extract_filename_hint(&m.content);
            if re_hint.is_some() {
                hint = re_hint;
                break;
            }
        }
        if let Some(h) = &hint {
            eprintln!("[forge-agent] user filename hint extracted: {:?}", h);
        }
        if let Ok(mut g) = ctx.user_filename_hint.lock() {
            *g = hint;
        }
        // Reset last_write_path each turn so a stale path from a
        // previous user turn doesn't leak into a fresh request.
        if let Ok(mut g) = ctx.last_write_path.lock() {
            *g = None;
        }
    }

    loop {
        if iterations >= max_iterations {
            let _ = event_tx.send(AgentEvent::Error(
                format!("Tool iteration limit reached ({max_iterations})")
            ));
            let _ = event_tx.send(AgentEvent::Finished {
                messages: Some(messages.clone()),
            });
            return;
        }

        // Send messages to inference thread, get streaming response.
        let response_rx = inference.generate(messages.clone(), tools.to_vec());

        let mut accumulated_text = String::new();
        let mut got_tool_use = false;

        // Drain the response stream.
        loop {
            match response_rx.recv() {
                Ok(InferenceEvent::Token(t)) => {
                    accumulated_text.push_str(&t);
                    let _ = event_tx.send(AgentEvent::Token(t));
                }
                Ok(InferenceEvent::Thinking(t)) => {
                    let _ = event_tx.send(AgentEvent::Thinking(t));
                }
                Ok(InferenceEvent::ToolUse(tc)) => {
                    let args_str = serde_json::to_string_pretty(&tc.arguments)
                        .unwrap_or_default();
                    let call_sig = format!("{}::{}", tc.name, args_str);

                    // Dedup: if same name+args already called, feed error back
                    // to the model so it generates different output. Only
                    // applies to read-style tools — write/edit operations
                    // are idempotent (they overwrite) and small models like
                    // Gemma 4 E4B legitimately retry a write_file when the
                    // first call's content was truncated by streaming. We
                    // must NOT block those retries.
                    let is_idempotent_write = matches!(
                        tc.name.as_str(),
                        "write_file" | "edit_file" | "create_note" | "rename_file"
                    );
                    if !is_idempotent_write && seen_calls.contains(&call_sig) {
                        let _ = event_tx.send(AgentEvent::ToolCallResult {
                            name: tc.name.clone(),
                            content: "Already called with same arguments. Use a different query or proceed to synthesize an answer.".into(),
                            is_error: true,
                        });
                        let preamble = std::mem::take(&mut accumulated_text);
                        messages.push(ChatMessage::assistant_with_tool_calls(
                            preamble,
                            vec![tc.clone()],
                        ));
                        messages.push(ChatMessage::tool_result(
                            &tc.id,
                            "Error: duplicate tool call. Try different query or write the answer.".to_string(),
                        ));
                        got_tool_use = true;
                        continue;
                    }
                    seen_calls.insert(call_sig.clone());

                    let _ = event_tx.send(AgentEvent::ToolCallStarted {
                        name: tc.name.clone(),
                        args: args_str.clone(),
                    });

                    // Execute the tool.
                    eprintln!("[forge-agent] executing tool: {} args={}", tc.name, args_str);
                    let result = execute_tool(&tc, ctx);
                    eprintln!("[forge-agent] tool {} done, is_error={}, content_len={}", tc.name, result.is_error, result.content.len());

                    if result.is_error {
                        if last_call_sig.as_deref() == Some(&call_sig) {
                            consecutive_failures += 1;
                        } else {
                            consecutive_failures = 1;
                            last_call_sig = Some(call_sig);
                        }
                    } else {
                        consecutive_failures = 0;
                        last_call_sig = None;
                    }

                    let _ = event_tx.send(AgentEvent::ToolCallResult {
                        name: tc.name.clone(),
                        content: result.content.clone(),
                        is_error: result.is_error,
                    });

                    // Append assistant message with tool call + tool result to history.
                    // Drain accumulated_text so subsequent tool calls in the
                    // same turn don't re-push the same prose text.
                    let preamble = std::mem::take(&mut accumulated_text);
                    messages.push(ChatMessage::assistant_with_tool_calls(
                        preamble,
                        vec![tc.clone()],
                    ));
                    messages.push(ChatMessage::tool_result(
                        &tc.id,
                        if result.is_error {
                            format!("Error: {}", result.content)
                        } else {
                            result.content
                        },
                    ));

                    if consecutive_failures >= 2 {
                        let _ = event_tx.send(AgentEvent::Error(
                            format!("Tool call {} failed twice with the same arguments. Stopping to avoid an infinite loop.", tc.name)
                        ));
                        let _ = event_tx.send(AgentEvent::Finished {
                            messages: Some(messages.clone()),
                        });
                        return;
                    }

                    got_tool_use = true;
                    // Continue draining until Done.
                }
                Ok(InferenceEvent::Done) => {
                    break;
                }
                Ok(InferenceEvent::Error(e)) => {
                    let _ = event_tx.send(AgentEvent::Error(e));
                    let _ = event_tx.send(AgentEvent::Finished {
                        messages: Some(messages.clone()),
                    });
                    return;
                }
                Err(_) => {
                    // Channel closed unexpectedly.
                    let _ = event_tx.send(AgentEvent::Error("Inference channel closed".into()));
                    let _ = event_tx.send(AgentEvent::Finished {
                        messages: Some(messages.clone()),
                    });
                    return;
                }
            }
        }

        if got_tool_use {
            // Tool was called, loop back for next inference turn.
            iterations += 1;
            continue;
        }

        // No tool call: model is done. Append final assistant message.
        if !accumulated_text.is_empty() {
            messages.push(ChatMessage::assistant(accumulated_text));
        }
        let _ = event_tx.send(AgentEvent::Finished {
            messages: Some(messages.clone()),
        });
        return;
    }
}
