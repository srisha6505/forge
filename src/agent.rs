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
        "SELECT c.file_path, c.heading, substr(c.content, 1, 300), f.rank \
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

    if rel_path.is_empty() {
        return ToolResult { content: "No path provided".into(), is_error: true };
    }

    let full_path = ctx.vault_path.join(rel_path);

    // Path traversal check.
    match full_path.canonicalize() {
        Ok(canonical) => {
            if !canonical.starts_with(&ctx.vault_path) {
                return ToolResult {
                    content: "Path is outside the vault".into(),
                    is_error: true,
                };
            }
        }
        Err(_) => {
            return ToolResult {
                content: format!("File not found: {rel_path}"),
                is_error: true,
            };
        }
    }

    match fs::read_to_string(&full_path) {
        Ok(content) => {
            // Truncate very large files.
            let truncated = if content.len() > 8000 {
                format!("{}...\n\n[truncated, {} bytes total]", &content[..8000], content.len())
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

    let full_path = ctx.vault_path.join(rel_path);

    // Path traversal check.
    match full_path.canonicalize() {
        Ok(canonical) => {
            if !canonical.starts_with(&ctx.vault_path) {
                return ToolResult {
                    content: "Path is outside the vault".into(),
                    is_error: true,
                };
            }
        }
        Err(_) => {
            return ToolResult {
                content: format!("File not found: {rel_path}"),
                is_error: true,
            };
        }
    }

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
    let mut current_heading = String::new();
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
                current_heading = trimmed.to_string();
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

// ── Agent loop ──

/// Default system prompt for the research agent.
pub fn default_system_prompt(vault_name: &str) -> String {
    format!(
        "You are a research assistant with access to the user's note vault \"{vault_name}\". \
         Use the available tools to search and read notes before answering questions. \
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
