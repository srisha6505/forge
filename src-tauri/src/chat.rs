//! Chats-as-markdown storage.
//!
//! Each chat is a single `.md` file under `<vault>/.forge/chats/YYYY/MM/`
//! with a small YAML frontmatter and `## [role] <ts>` section markers.
//! See ai.md §6 for the canonical format.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTurn {
    pub role: String,
    pub timestamp: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatHeader {
    #[serde(default = "default_schema_version")]
    pub forge_chat: u32,
    pub created: String,
    pub updated: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub tools_allowed: Vec<String>,
}

fn default_schema_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatFile {
    pub id: String,
    pub path: String,
    pub header: ChatHeader,
    pub turns: Vec<ChatTurn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSummary {
    pub id: String,
    pub path: String,
    pub title: String,
    pub created: String,
    pub updated: String,
    pub model: Option<String>,
    pub turn_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveChatPayload {
    pub vault_path: String,
    pub chat_id: Option<String>,
    pub header: ChatHeader,
    pub turns: Vec<ChatTurn>,
}

// ── Tauri commands ──────────────────────────────────────────────────────

#[tauri::command]
pub fn save_chat(payload: SaveChatPayload) -> Result<ChatSummary, String> {
    let vault = PathBuf::from(&payload.vault_path);
    if !vault.is_dir() {
        return Err(format!("not a directory: {}", payload.vault_path));
    }

    let target = match &payload.chat_id {
        Some(id) => find_chat_path(&vault, id)?
            // Falling back to a fresh slot lets a save-after-rename still land
            // somewhere predictable instead of erroring out.
            .unwrap_or_else(|| fresh_chat_path(&vault, &payload.turns, id)),
        None => {
            let id = generate_chat_id(&payload.turns);
            fresh_chat_path(&vault, &payload.turns, &id)
        }
    };

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create chat dir: {e}"))?;
    }

    let body = render_chat_file(&payload.header, &payload.turns);
    atomic_write(&target, body.as_bytes())?;

    let id = chat_id_from_path(&target);
    let title = derive_title(&payload.turns);
    Ok(ChatSummary {
        id,
        path: target.to_string_lossy().into_owned(),
        title,
        created: payload.header.created.clone(),
        updated: payload.header.updated.clone(),
        model: payload.header.model.clone(),
        turn_count: payload.turns.len() as u32,
    })
}

#[tauri::command]
pub fn load_chat(vault_path: String, chat_id: String) -> Result<ChatFile, String> {
    let vault = PathBuf::from(&vault_path);
    if !vault.is_dir() {
        return Err(format!("not a directory: {vault_path}"));
    }
    let path = find_chat_path(&vault, &chat_id)?
        .ok_or_else(|| format!("chat not found: {chat_id}"))?;
    let raw = fs::read_to_string(&path).map_err(|e| format!("read chat: {e}"))?;
    let (header, turns) = parse_chat_file(&raw)?;
    Ok(ChatFile {
        id: chat_id_from_path(&path),
        path: path.to_string_lossy().into_owned(),
        header,
        turns,
    })
}

#[tauri::command]
pub fn list_chats(vault_path: String) -> Result<Vec<ChatSummary>, String> {
    let vault = PathBuf::from(&vault_path);
    if !vault.is_dir() {
        return Err(format!("not a directory: {vault_path}"));
    }
    let root = chats_root(&vault);
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut out: Vec<ChatSummary> = Vec::new();
    walk_md_files(&root, &mut |p| {
        if let Ok(raw) = fs::read_to_string(p) {
            if let Ok(summary) = summarise_chat_file(p, &raw) {
                out.push(summary);
            }
        }
    });
    // Newest first by `updated`.
    out.sort_by(|a, b| b.updated.cmp(&a.updated));
    Ok(out)
}

#[tauri::command]
pub fn delete_chat(vault_path: String, chat_id: String) -> Result<(), String> {
    let vault = PathBuf::from(&vault_path);
    if !vault.is_dir() {
        return Err(format!("not a directory: {vault_path}"));
    }
    let path = find_chat_path(&vault, &chat_id)?
        .ok_or_else(|| format!("chat not found: {chat_id}"))?;
    // Refuse to delete anything outside .forge/chats/ — defensive guard
    // against a malformed id ever escaping the chats tree.
    let canon_root = chats_root(&vault);
    if !path.starts_with(&canon_root) {
        return Err("refusing to delete outside .forge/chats/".to_string());
    }
    fs::remove_file(&path).map_err(|e| format!("delete chat: {e}"))
}

#[tauri::command]
pub fn export_chat_as_note(
    vault_path: String,
    chat_id: String,
    dest_relpath: String,
) -> Result<String, String> {
    let vault = PathBuf::from(&vault_path);
    if !vault.is_dir() {
        return Err(format!("not a directory: {vault_path}"));
    }
    let src = find_chat_path(&vault, &chat_id)?
        .ok_or_else(|| format!("chat not found: {chat_id}"))?;
    let raw = fs::read_to_string(&src).map_err(|e| format!("read chat: {e}"))?;
    let (_header, turns) = parse_chat_file(&raw)?;

    // Block traversal — the destination must resolve inside the vault.
    let rel = sanitize_relpath(&dest_relpath)?;
    let dest = vault.join(rel);
    if !dest.starts_with(&vault) {
        return Err("destination escapes vault".to_string());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create dest dir: {e}"))?;
    }
    let title = derive_title(&turns);
    let rendered = render_export(&title, &turns);
    atomic_write(&dest, rendered.as_bytes())?;
    Ok(dest.to_string_lossy().into_owned())
}

// ── Path helpers ────────────────────────────────────────────────────────

fn chats_root(vault: &Path) -> PathBuf {
    vault.join(".forge").join("chats")
}

fn chat_id_from_path(p: &Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn find_chat_path(vault: &Path, chat_id: &str) -> Result<Option<PathBuf>, String> {
    let root = chats_root(vault);
    if !root.exists() {
        return Ok(None);
    }
    let mut found: Option<PathBuf> = None;
    walk_md_files(&root, &mut |p| {
        if found.is_some() {
            return;
        }
        if chat_id_from_path(p) == chat_id {
            found = Some(p.to_path_buf());
        }
    });
    Ok(found)
}

fn fresh_chat_path(vault: &Path, turns: &[ChatTurn], id: &str) -> PathBuf {
    let (year, month) = year_month_from_id(id).unwrap_or_else(now_year_month);
    let _ = turns; // slug already baked into id
    chats_root(vault)
        .join(format!("{year:04}"))
        .join(format!("{month:02}"))
        .join(format!("{id}.md"))
}

fn year_month_from_id(id: &str) -> Option<(u32, u32)> {
    // ids look like YYYY-MM-DD-HHMMSS-<slug>
    let mut parts = id.splitn(4, '-');
    let y = parts.next()?.parse::<u32>().ok()?;
    let m = parts.next()?.parse::<u32>().ok()?;
    Some((y, m))
}

fn now_year_month() -> (u32, u32) {
    let (y, mo, _, _, _, _) = utc_components(now_secs());
    (y, mo)
}

fn sanitize_relpath(rel: &str) -> Result<PathBuf, String> {
    let trimmed = rel.trim_start_matches('/');
    let pb = PathBuf::from(trimmed);
    for c in pb.components() {
        match c {
            std::path::Component::Normal(_) => {}
            std::path::Component::CurDir => {}
            _ => return Err("invalid relpath".to_string()),
        }
    }
    Ok(pb)
}

// ── Walk ────────────────────────────────────────────────────────────────

fn walk_md_files(dir: &Path, visit: &mut dyn FnMut(&Path)) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk_md_files(&p, visit);
        } else if p.extension().and_then(|s| s.to_str()) == Some("md") {
            visit(&p);
        }
    }
}

// ── Render ──────────────────────────────────────────────────────────────

fn render_chat_file(header: &ChatHeader, turns: &[ChatTurn]) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("forge_chat: {}\n", header.forge_chat));
    out.push_str(&format!("created: {}\n", header.created));
    out.push_str(&format!("updated: {}\n", header.updated));
    if let Some(m) = &header.model {
        out.push_str(&format!("model: {}\n", yaml_inline(m)));
    } else {
        out.push_str("model: \n");
    }
    if let Some(p) = &header.provider {
        out.push_str(&format!("provider: {}\n", yaml_inline(p)));
    } else {
        out.push_str("provider: \n");
    }
    match &header.system_prompt {
        Some(text) if !text.is_empty() => {
            out.push_str("system_prompt: |\n");
            for line in text.lines() {
                out.push_str("  ");
                out.push_str(line);
                out.push('\n');
            }
        }
        _ => {
            out.push_str("system_prompt: \n");
        }
    }
    out.push_str("tools_allowed: [");
    for (i, t) in header.tools_allowed.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(t);
    }
    out.push_str("]\n");
    out.push_str("---\n\n");

    for (i, turn) in turns.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&format!("## [{}] {}\n\n", turn.role, turn.timestamp));
        out.push_str(turn.body.trim_end_matches('\n'));
        out.push('\n');
    }
    out
}

fn render_export(title: &str, turns: &[ChatTurn]) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Chat: {title}\n\n"));
    for turn in turns {
        let role = match turn.role.as_str() {
            "user" => "You",
            "assistant" => "Assistant",
            "tool" => "Tool",
            other => other,
        };
        out.push_str(&format!("**{role}** ({}):\n\n", turn.timestamp));
        out.push_str(turn.body.trim_end_matches('\n'));
        out.push_str("\n\n");
    }
    out
}

fn yaml_inline(s: &str) -> String {
    // Quote when the value could be YAML-ambiguous (colons, leading spaces,
    // booleans, etc.). Plain alphanumeric+limited punctuation passes through.
    let plain = !s.is_empty()
        && s.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | '@')
        });
    if plain {
        s.to_string()
    } else {
        let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{escaped}\"")
    }
}

// ── Parse ───────────────────────────────────────────────────────────────

fn parse_chat_file(raw: &str) -> Result<(ChatHeader, Vec<ChatTurn>), String> {
    let (front, body) = split_frontmatter(raw)?;
    let header = parse_frontmatter(front)?;
    let turns = parse_turns(body);
    Ok((header, turns))
}

fn split_frontmatter(raw: &str) -> Result<(&str, &str), String> {
    let stripped = raw.strip_prefix("---\n").or_else(|| raw.strip_prefix("---\r\n"));
    let after = stripped.ok_or_else(|| "missing frontmatter open".to_string())?;
    // Find closing '---' on its own line.
    let mut idx = 0usize;
    let bytes = after.as_bytes();
    let mut line_start = 0usize;
    while idx < bytes.len() {
        if bytes[idx] == b'\n' {
            let line = &after[line_start..idx];
            let trimmed = line.trim_end_matches('\r');
            if trimmed == "---" {
                let body_start = idx + 1;
                return Ok((&after[..line_start], &after[body_start..]));
            }
            line_start = idx + 1;
        }
        idx += 1;
    }
    // EOF without trailing newline — handle a final "---" line.
    let tail = &after[line_start..];
    if tail.trim_end_matches('\r') == "---" {
        return Ok((&after[..line_start], ""));
    }
    Err("missing frontmatter close".to_string())
}

fn parse_frontmatter(front: &str) -> Result<ChatHeader, String> {
    let mut header = ChatHeader {
        forge_chat: 1,
        created: String::new(),
        updated: String::new(),
        model: None,
        provider: None,
        system_prompt: None,
        tools_allowed: Vec::new(),
    };

    let mut iter = front.lines().peekable();
    while let Some(line) = iter.next() {
        if line.trim().is_empty() {
            continue;
        }
        let (key, rest) = match line.split_once(':') {
            Some((k, v)) => (k.trim(), v),
            None => continue,
        };
        let value = rest.trim_start();
        match key {
            "forge_chat" => {
                header.forge_chat = value.trim().parse().unwrap_or(1);
            }
            "created" => {
                header.created = unquote(value.trim()).into_owned();
            }
            "updated" => {
                header.updated = unquote(value.trim()).into_owned();
            }
            "model" => {
                let v = unquote(value.trim());
                header.model = if v.is_empty() { None } else { Some(v.into_owned()) };
            }
            "provider" => {
                let v = unquote(value.trim());
                header.provider = if v.is_empty() { None } else { Some(v.into_owned()) };
            }
            "system_prompt" => {
                if value.trim() == "|" {
                    // Block scalar — gather subsequent lines that are
                    // indented (>= 2 spaces) until we hit something that
                    // isn't.
                    let mut text = String::new();
                    while let Some(next) = iter.peek() {
                        if next.starts_with("  ") {
                            let line = iter.next().unwrap();
                            text.push_str(&line[2..]);
                            text.push('\n');
                        } else if next.trim().is_empty() {
                            // Blank line inside a block scalar.
                            iter.next();
                            text.push('\n');
                        } else {
                            break;
                        }
                    }
                    let trimmed = text.trim_end_matches('\n').to_string();
                    header.system_prompt = if trimmed.is_empty() { None } else { Some(trimmed) };
                } else {
                    let v = unquote(value.trim());
                    header.system_prompt = if v.is_empty() { None } else { Some(v.into_owned()) };
                }
            }
            "tools_allowed" => {
                header.tools_allowed = parse_inline_list(value.trim());
            }
            _ => {}
        }
    }
    Ok(header)
}

fn parse_inline_list(s: &str) -> Vec<String> {
    let inner = s.trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(|t| unquote(t.trim()).into_owned())
        .filter(|t| !t.is_empty())
        .collect()
}

fn unquote(s: &str) -> std::borrow::Cow<'_, str> {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        let inner = &s[1..s.len() - 1];
        std::borrow::Cow::Owned(inner.replace("\\\"", "\"").replace("\\\\", "\\"))
    } else {
        std::borrow::Cow::Borrowed(s)
    }
}

fn parse_turns(body: &str) -> Vec<ChatTurn> {
    let mut turns: Vec<ChatTurn> = Vec::new();
    let mut current: Option<(String, String, String)> = None; // role, ts, body
    for line in body.split_inclusive('\n') {
        if let Some((role, ts)) = parse_turn_header(line.trim_end_matches(['\r', '\n'])) {
            if let Some((r, t, b)) = current.take() {
                turns.push(ChatTurn {
                    role: r,
                    timestamp: t,
                    body: b.trim_matches('\n').to_string(),
                });
            }
            current = Some((role, ts, String::new()));
        } else if let Some((_, _, b)) = current.as_mut() {
            b.push_str(line);
        }
        // Lines before the first header are dropped — frontmatter padding.
    }
    if let Some((r, t, b)) = current {
        turns.push(ChatTurn {
            role: r,
            timestamp: t,
            body: b.trim_matches('\n').to_string(),
        });
    }
    turns
}

fn parse_turn_header(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("## [")?;
    let close = rest.find(']')?;
    let role = &rest[..close];
    if !matches!(role, "user" | "assistant" | "tool") {
        return None;
    }
    let after = rest[close + 1..].trim();
    if after.is_empty() {
        return None;
    }
    Some((role.to_string(), after.to_string()))
}

fn summarise_chat_file(path: &Path, raw: &str) -> Result<ChatSummary, String> {
    let (front, body) = split_frontmatter(raw)?;
    let header = parse_frontmatter(front)?;
    // Cheap turn count + first-user-body scan without full body alloc.
    let mut turn_count = 0u32;
    let mut first_user_body: Option<String> = None;
    let mut current_role: Option<String> = None;
    let mut current_body = String::new();
    for line in body.split_inclusive('\n') {
        if let Some((role, _)) = parse_turn_header(line.trim_end_matches(['\r', '\n'])) {
            if let Some(r) = current_role.take() {
                if r == "user" && first_user_body.is_none() {
                    first_user_body = Some(current_body.trim().to_string());
                }
                current_body.clear();
            }
            current_role = Some(role);
            turn_count += 1;
        } else if current_role.is_some() {
            current_body.push_str(line);
        }
    }
    if let Some(r) = current_role {
        if r == "user" && first_user_body.is_none() {
            first_user_body = Some(current_body.trim().to_string());
        }
    }
    let title = first_user_body
        .map(|t| title_from_body(&t))
        .unwrap_or_else(|| "Untitled".to_string());
    Ok(ChatSummary {
        id: chat_id_from_path(path),
        path: path.to_string_lossy().into_owned(),
        title,
        created: header.created,
        updated: header.updated,
        model: header.model,
        turn_count,
    })
}

// ── Title / slug ────────────────────────────────────────────────────────

fn derive_title(turns: &[ChatTurn]) -> String {
    turns
        .iter()
        .find(|t| t.role == "user")
        .map(|t| title_from_body(&t.body))
        .unwrap_or_else(|| "Untitled".to_string())
}

fn title_from_body(body: &str) -> String {
    let collapsed: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return "Untitled".to_string();
    }
    if collapsed.chars().count() <= 80 {
        collapsed
    } else {
        collapsed.chars().take(80).collect()
    }
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in s.chars().take(50) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !out.is_empty() && !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "chat".to_string()
    } else {
        trimmed
    }
}

fn generate_chat_id(turns: &[ChatTurn]) -> String {
    let secs = now_secs();
    let (y, mo, d, h, mi, s) = utc_components(secs);
    let first_user = turns
        .iter()
        .find(|t| t.role == "user")
        .map(|t| t.body.as_str())
        .unwrap_or("");
    let slug = slugify(first_user);
    format!("{y:04}-{mo:02}-{d:02}-{h:02}{mi:02}{s:02}-{slug}")
}

// ── Atomic write ────────────────────────────────────────────────────────

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create parent: {e}"))?;
    }
    let tmp = path.with_extension("md.tmp");
    {
        let mut f = fs::File::create(&tmp).map_err(|e| format!("create tmp: {e}"))?;
        f.write_all(bytes).map_err(|e| format!("write tmp: {e}"))?;
        f.sync_all().map_err(|e| format!("fsync tmp: {e}"))?;
    }
    fs::rename(&tmp, path).map_err(|e| format!("rename tmp: {e}"))?;
    Ok(())
}

// ── Time ────────────────────────────────────────────────────────────────

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// Howard Hinnant's civil-from-days; mirrors edge_tts::timestamp.
fn utc_components(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let h = (rem / 3600) as u32;
    let mi = ((rem % 3600) / 60) as u32;
    let s = (rem % 60) as u32;
    let d = days + 719_468;
    let era = if d >= 0 { d } else { d - 146_096 } / 146_097;
    let doe = (d - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5) as u32 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (y + if month <= 2 { 1 } else { 0 }) as u32;
    (year, month, day, h, mi, s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_turns() -> Vec<ChatTurn> {
        vec![
            ChatTurn {
                role: "user".to_string(),
                timestamp: "2026-04-26T10:00:00Z".to_string(),
                body: "Hello, **world**!\n\nSecond paragraph.".to_string(),
            },
            ChatTurn {
                role: "assistant".to_string(),
                timestamp: "2026-04-26T10:00:05Z".to_string(),
                body: "Hi there.".to_string(),
            },
            ChatTurn {
                role: "tool".to_string(),
                timestamp: "2026-04-26T10:00:06Z".to_string(),
                body: "```json\n{ \"name\": \"read_file\" }\n```".to_string(),
            },
        ]
    }

    #[test]
    fn round_trip() {
        let header = ChatHeader {
            forge_chat: 1,
            created: "2026-04-26T10:00:00Z".to_string(),
            updated: "2026-04-26T10:00:06Z".to_string(),
            model: Some("claude-sonnet-4-7".to_string()),
            provider: Some("anthropic".to_string()),
            system_prompt: Some("You are helpful.\nBe terse.".to_string()),
            tools_allowed: vec!["read_file".to_string(), "edit_file".to_string()],
        };
        let turns = sample_turns();
        let raw = render_chat_file(&header, &turns);
        let (parsed_header, parsed_turns) = parse_chat_file(&raw).expect("parse");
        assert_eq!(parsed_header.forge_chat, 1);
        assert_eq!(parsed_header.created, header.created);
        assert_eq!(parsed_header.updated, header.updated);
        assert_eq!(parsed_header.model.as_deref(), Some("claude-sonnet-4-7"));
        assert_eq!(parsed_header.provider.as_deref(), Some("anthropic"));
        assert_eq!(
            parsed_header.system_prompt.as_deref(),
            Some("You are helpful.\nBe terse.")
        );
        assert_eq!(parsed_header.tools_allowed.len(), 2);
        assert_eq!(parsed_turns.len(), 3);
        assert_eq!(parsed_turns[0].role, "user");
        assert_eq!(parsed_turns[0].body, turns[0].body);
        assert_eq!(parsed_turns[2].body, turns[2].body);
    }

    #[test]
    fn slug_format() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify(""), "chat");
        assert_eq!(slugify("   ---   "), "chat");
    }

    #[test]
    fn id_year_month() {
        let (y, m) = year_month_from_id("2026-04-26-103000-hello").unwrap();
        assert_eq!((y, m), (2026, 4));
    }
}
