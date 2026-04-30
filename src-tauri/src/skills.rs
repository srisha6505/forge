//! Skill loader.
//!
//! Skills are user-authored markdown files in `<vault>/.forge/skills/`
//! that get keyword-matched against the user's most recent message and
//! appended to the system prompt for that turn only. Lets us keep the
//! base system prompt small while still giving the agent rich,
//! domain-specific instructions when relevant.
//!
//! Skill file format:
//! ```
//! ---
//! name: flowchart
//! triggers: [flowchart, diagram, mermaid, sequence diagram]
//! ---
//! body markdown that gets appended when any trigger matches...
//! ```
//!
//! Matching is case-insensitive whole-word/phrase. Cap at MAX_SKILLS_PER_TURN
//! so the prompt can never balloon if the user's message hits many triggers.

use std::fs;
use std::path::Path;

const MAX_SKILLS_PER_TURN: usize = 3;

#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub triggers: Vec<String>,
    pub body: String,
}

/// Read every `.md` file in `<vault>/.forge/skills/` and parse its
/// frontmatter. Silently ignores files that fail to parse — a malformed
/// skill should never break the agent loop.
pub fn load_skills(vault: &Path) -> Vec<Skill> {
    let dir = vault.join(".forge").join("skills");
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let raw = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if let Some(skill) = parse_skill(&raw, &path) {
            out.push(skill);
        }
    }
    out
}

/// Parse `---\nfrontmatter\n---\nbody`. Frontmatter is YAML-ish but we
/// only care about `name:` and `triggers:` (a bracketed list or one
/// item per dash). We do not pull in a full YAML dep for this.
fn parse_skill(raw: &str, path: &Path) -> Option<Skill> {
    let stripped = raw.strip_prefix("---")?;
    let mut chars = stripped.char_indices();
    // Skip the first newline after `---`.
    while let Some((_, c)) = chars.next() {
        if c == '\n' {
            break;
        }
    }
    let after_first_marker = &stripped[chars.next().map(|(i, _)| i).unwrap_or(0)..];
    // Find the closing `\n---` on its own line.
    let close_idx = after_first_marker.find("\n---")?;
    let frontmatter = &after_first_marker[..close_idx];
    // Skip the closing marker + its trailing newline to land on the body.
    let after_close = &after_first_marker[close_idx + "\n---".len()..];
    let body = after_close.trim_start_matches('\n').trim_end().to_string();

    let mut name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let mut triggers = Vec::new();

    let mut in_triggers_list = false;
    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("name:") {
            name = rest.trim().trim_matches('"').trim_matches('\'').to_string();
            in_triggers_list = false;
        } else if let Some(rest) = trimmed.strip_prefix("triggers:") {
            let rest = rest.trim();
            if rest.starts_with('[') && rest.ends_with(']') {
                // Inline form: triggers: [a, b, c]
                let inner = &rest[1..rest.len() - 1];
                for t in inner.split(',') {
                    let t = t.trim().trim_matches('"').trim_matches('\'').to_string();
                    if !t.is_empty() {
                        triggers.push(t);
                    }
                }
                in_triggers_list = false;
            } else {
                // Block form starts on next line.
                in_triggers_list = true;
            }
        } else if in_triggers_list {
            if let Some(item) = trimmed.strip_prefix('-') {
                let t = item.trim().trim_matches('"').trim_matches('\'').to_string();
                if !t.is_empty() {
                    triggers.push(t);
                }
            } else {
                in_triggers_list = false;
            }
        }
    }

    if body.is_empty() || triggers.is_empty() {
        return None;
    }
    Some(Skill {
        name,
        triggers,
        body,
    })
}

/// Match `user_msg` against the trigger lists of the loaded skills.
/// Returns up to MAX_SKILLS_PER_TURN matched skills, ordered by the
/// position of the first matched trigger in the message (earlier
/// matches win — usually closer to the user's actual ask).
///
/// Match is case-insensitive whole-word / phrase. A trigger like
/// "state machine" matches only when those exact words appear in
/// sequence; "machine learning" alone won't trigger it.
pub fn select_skills(skills: &[Skill], user_msg: &str) -> Vec<Skill> {
    let lower = user_msg.to_lowercase();
    let mut hits: Vec<(usize, &Skill)> = Vec::new();
    for s in skills {
        let mut earliest: Option<usize> = None;
        for trigger in &s.triggers {
            let t = trigger.to_lowercase();
            if let Some(pos) = find_word_match(&lower, &t) {
                earliest = match earliest {
                    Some(p) if p <= pos => Some(p),
                    _ => Some(pos),
                };
            }
        }
        if let Some(pos) = earliest {
            hits.push((pos, s));
        }
    }
    hits.sort_by_key(|(p, _)| *p);
    hits.into_iter()
        .take(MAX_SKILLS_PER_TURN)
        .map(|(_, s)| s.clone())
        .collect()
}

/// Whole-word/phrase match. Looks for `needle` in `haystack` such that
/// each end of the match is at a word boundary (start, end, or non-
/// alphanumeric character). Avoids "art" matching inside "chart".
fn find_word_match(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    let mut start = 0;
    while let Some(rel) = haystack[start..].find(needle) {
        let pos = start + rel;
        let end = pos + needle.len();
        let before_ok = pos == 0
            || !haystack.as_bytes()[pos - 1].is_ascii_alphanumeric();
        let after_ok = end == haystack.len()
            || !haystack.as_bytes()[end].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return Some(pos);
        }
        start = pos + 1;
    }
    None
}

/// Compose a system prompt from the base prompt + any matched skills.
/// Each skill is appended under a `## SKILL: <name>` heading so the
/// model can reason about them as discrete sections.
pub fn assemble_prompt(base: &str, matched: &[Skill]) -> String {
    if matched.is_empty() {
        return base.to_string();
    }
    let mut out = String::with_capacity(base.len() + matched.iter().map(|s| s.body.len() + 64).sum::<usize>());
    out.push_str(base);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("\n═══ MATCHED SKILLS ═══\n");
    out.push_str("The user's message matched these skill cards. Apply them.\n");
    for s in matched {
        out.push_str("\n## SKILL: ");
        out.push_str(&s.name);
        out.push('\n');
        out.push_str(&s.body);
        out.push('\n');
    }
    out
}
