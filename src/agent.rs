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
    /// Agent finished (no more tool calls).
    Finished,
    /// An error occurred.
    Error(String),
}

// ── Tool context ──

pub struct ToolContext {
    pub vault_path: PathBuf,
    pub db_path: PathBuf,
}

// ── Tool result ──

pub struct ToolResult {
    pub content: String,
    pub is_error: bool,
}

// ── Path validation helper ──

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
                if !canonical.starts_with(vault_root) {
                    return Err(ToolResult {
                        content: "Path is outside the vault".into(),
                        is_error: true,
                    });
                }
                Ok(canonical)
            }
            Err(_) => Err(ToolResult {
                content: format!("File not found: {rel_path}"),
                is_error: true,
            }),
        }
    } else {
        // For paths that may not exist yet, canonicalize the parent.
        let parent = full_path.parent().unwrap_or(vault_root);
        // Ensure parent exists (or at least the vault root prefix resolves).
        let canonical_parent = if parent.exists() {
            parent.canonicalize().unwrap_or_else(|_| parent.to_path_buf())
        } else {
            // Walk up to find an existing ancestor.
            let mut ancestor = parent.to_path_buf();
            while !ancestor.exists() {
                if let Some(p) = ancestor.parent() {
                    ancestor = p.to_path_buf();
                } else {
                    break;
                }
            }
            let canonical_ancestor = ancestor.canonicalize().unwrap_or(ancestor);
            if !canonical_ancestor.starts_with(vault_root) {
                return Err(ToolResult {
                    content: "Path is outside the vault".into(),
                    is_error: true,
                });
            }
            // Reconstruct by replacing the resolved ancestor portion.
            canonical_ancestor
        };

        if !canonical_parent.starts_with(vault_root) {
            return Err(ToolResult {
                content: "Path is outside the vault".into(),
                is_error: true,
            });
        }

        Ok(full_path)
    }
}

// ── Tool schemas (OpenAI function-calling format) ──

pub fn tool_schemas() -> Vec<serde_json::Value> {
    serde_json::json!([
        {
            "type": "function",
            "function": {
                "name": "search_vault",
                "description": "Search the user's note vault using keyword and semantic search. Returns matching chunks with file paths, headings, and content snippets.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The search query"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of results (default 5)"
                        }
                    },
                    "required": ["query"]
                }
            }
        },
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
    .unwrap_or_default()
}

// ── Tool execution ──

pub fn execute_tool(tool_call: &ToolCall, ctx: &ToolContext) -> ToolResult {
    match tool_call.name.as_str() {
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

fn exec_search_vault(tc: &ToolCall, ctx: &ToolContext) -> ToolResult {
    let query = tc.arguments.get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let limit = tc.arguments.get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(5) as i64;

    if query.is_empty() {
        return ToolResult { content: "Empty search query".into(), is_error: true };
    }

    // Open a read-only connection to the search DB.
    let db = match rusqlite::Connection::open_with_flags(
        &ctx.db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(db) => db,
        Err(e) => return ToolResult {
            content: format!("Failed to open search database: {e}"),
            is_error: true,
        },
    };

    // Build FTS5 query with prefix matching.
    let fts_query: String = query.split_whitespace()
        .filter(|w| !w.is_empty())
        .map(|w| {
            let clean: String = w.chars().filter(|c| c.is_alphanumeric() || *c == '_').collect();
            if clean.is_empty() { String::new() } else { format!("{clean}*") }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    if fts_query.is_empty() {
        return ToolResult { content: "No valid search terms".into(), is_error: true };
    }

    let mut stmt = match db.prepare(
        "SELECT c.file_path, c.heading, substr(c.content, 1, 150), f.rank \
         FROM chunks_fts f \
         JOIN chunks c ON c.id = f.rowid \
         WHERE chunks_fts MATCH ?1 \
         ORDER BY f.rank \
         LIMIT ?2"
    ) {
        Ok(s) => s,
        Err(e) => return ToolResult {
            content: format!("Query failed: {e}"),
            is_error: true,
        },
    };

    let mut results = String::new();
    let rows = stmt.query_map(rusqlite::params![fts_query, limit], |row| {
        let path: String = row.get(0)?;
        let heading: String = row.get(1)?;
        let snippet: String = row.get(2)?;
        let rank: f64 = row.get(3)?;
        Ok((path, heading, snippet, rank))
    });

    match rows {
        Ok(rows) => {
            let mut count = 0;
            for row in rows.flatten() {
                let (path, heading, snippet, _rank) = row;
                // Show path relative to vault.
                let rel = path.strip_prefix(ctx.vault_path.to_str().unwrap_or(""))
                    .unwrap_or(&path)
                    .trim_start_matches('/');
                results.push_str(&format!("--- {rel} | {heading} ---\n{snippet}\n\n"));
                count += 1;
            }
            if count == 0 {
                results = "No results found.".into();
            }
        }
        Err(e) => return ToolResult {
            content: format!("Search error: {e}"),
            is_error: true,
        },
    }

    ToolResult { content: results, is_error: false }
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
            // Truncate very large files.
            let truncated = if content.len() > 800 {
                format!("{}...\n\n[truncated, {} bytes total]", &content[..800], content.len())
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

    // Path traversal check.
    if let Ok(canonical) = dir.canonicalize() {
        if !canonical.starts_with(&ctx.vault_path) {
            return ToolResult {
                content: "Directory is outside the vault".into(),
                is_error: true,
            };
        }
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

fn exec_write_file(tc: &ToolCall, ctx: &ToolContext) -> ToolResult {
    let rel_path = tc.arguments.get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let content = tc.arguments.get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if rel_path.is_empty() {
        return ToolResult { content: "No path provided".into(), is_error: true };
    }

    let full_path = match validate_vault_path(&ctx.vault_path, rel_path, false) {
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

    match fs::write(&full_path, content) {
        Ok(()) => ToolResult {
            content: format!("Wrote {} bytes to {rel_path}", content.len()),
            is_error: false,
        },
        Err(e) => ToolResult {
            content: format!("Failed to write {rel_path}: {e}"),
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

    // Parse DuckDuckGo HTML results. Each result is inside a
    // <div class="result ..."> block with:
    //   <a class="result__a" ...>TITLE</a>
    //   <a class="result__snippet">SNIPPET</a>
    let mut results = Vec::new();

    // Split on result blocks.
    for chunk in body.split("class=\"result__a\"") {
        if results.len() >= num_results {
            break;
        }
        // Skip the first chunk (before any result).
        if !chunk.contains("result__snippet") && results.is_empty() && !chunk.contains('>') {
            continue;
        }

        // Extract title: text between > and </a>.
        let title = extract_between(chunk, ">", "</a>")
            .map(|t| strip_html_tags(&t))
            .unwrap_or_default();

        if title.is_empty() {
            continue;
        }

        // Extract snippet: look for result__snippet content.
        let snippet = if let Some(snippet_start) = chunk.find("result__snippet") {
            let rest = &chunk[snippet_start..];
            extract_between(rest, ">", "</")
                .map(|s| strip_html_tags(&s))
                .unwrap_or_default()
        } else {
            String::new()
        };

        // Extract URL from href.
        let href = extract_between(chunk, "href=\"", "\"")
            .unwrap_or_default();
        // DuckDuckGo uses redirect URLs; try to extract the actual URL.
        let url = if href.contains("uddg=") {
            href.split("uddg=").nth(1)
                .and_then(|u| u.split('&').next())
                .map(|u| urldecoded(u))
                .unwrap_or(href)
        } else {
            href
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
    format!(
        "You are a research assistant with access to the user's note vault \"{vault_name}\". \
         You have the following capabilities:\n\
         - Search and read notes (search_vault, read_file, read_section, list_files, grep_vault)\n\
         - Create and modify notes (write_file, edit_file, rename_file, delete_file)\n\
         - Search the web for information (web_search)\n\n\
         Use the available tools to search and read notes before answering questions. \
         When creating or editing files, confirm the action with the user first if the request is ambiguous. \
         Be concise and cite sources by file name. \
         When you have enough information, synthesize a clear answer."
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

    loop {
        if iterations >= max_iterations {
            let _ = event_tx.send(AgentEvent::Error(
                format!("Tool iteration limit reached ({max_iterations})")
            ));
            let _ = event_tx.send(AgentEvent::Finished);
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
                    let _ = event_tx.send(AgentEvent::ToolCallStarted {
                        name: tc.name.clone(),
                        args: args_str,
                    });

                    // Execute the tool.
                    let result = execute_tool(&tc, ctx);

                    let _ = event_tx.send(AgentEvent::ToolCallResult {
                        name: tc.name.clone(),
                        content: result.content.clone(),
                        is_error: result.is_error,
                    });

                    // Append assistant message with tool call + tool result to history.
                    messages.push(ChatMessage::assistant_with_tool_calls(
                        accumulated_text.clone(),
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

                    got_tool_use = true;
                    // Continue draining until Done.
                }
                Ok(InferenceEvent::Done) => {
                    break;
                }
                Ok(InferenceEvent::Error(e)) => {
                    let _ = event_tx.send(AgentEvent::Error(e));
                    let _ = event_tx.send(AgentEvent::Finished);
                    return;
                }
                Err(_) => {
                    // Channel closed unexpectedly.
                    let _ = event_tx.send(AgentEvent::Error("Inference channel closed".into()));
                    let _ = event_tx.send(AgentEvent::Finished);
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
        let _ = event_tx.send(AgentEvent::Finished);
        return;
    }
}
