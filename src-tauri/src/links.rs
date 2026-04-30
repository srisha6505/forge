//! Vault link indexing. Scans all .md files for `[[Wikilinks]]`, builds
//! a graph of note-to-note references, and resolves target names back
//! to concrete file paths.
//!
//! The regex is loose: `[[target]]`, `[[target|alias]]`, `[[target#heading]]`,
//! `[[target|alias#heading]]`. We strip the alias and heading; only the
//! target name matters for resolution.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct LinkHit {
    /// Absolute path to the linking file.
    pub path: String,
    /// Display name (file stem).
    pub name: String,
    /// A short snippet of the line the link appears on.
    pub snippet: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct GraphNode {
    pub id: String,   // absolute path, unique key
    pub name: String, // file stem
    pub degree: u32,  // in + out count (sizing hint)
}

#[derive(Serialize, Clone, Debug)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct LinkGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

/// Walk the vault collecting every .md file.
fn collect_md_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') { continue; }
            if p.is_dir() { walk(&p, out); continue; }
            let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
            if ext == "md" || ext == "markdown" { out.push(p); }
        }
    }
    walk(root, &mut out);
    out
}

fn file_stem(p: &Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Extract wikilink targets from `text`. Returns `(target, line_snippet)`
/// pairs. Target has alias and heading stripped and is lowercased for
/// resolution.
fn extract_wikilinks(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let bytes = line.as_bytes();
        let mut i = 0;
        while i + 3 < bytes.len() {
            if bytes[i] == b'[' && bytes[i + 1] == b'[' {
                if let Some(close) = line[i + 2..].find("]]") {
                    let raw = &line[i + 2..i + 2 + close];
                    // Skip empty or obviously malformed.
                    if !raw.is_empty() && !raw.contains('\n') {
                        // target|alias  →  target
                        let target = raw.split('|').next().unwrap_or("");
                        // target#heading  →  target
                        let target = target.split('#').next().unwrap_or("").trim();
                        if !target.is_empty() {
                            let snippet = if line.len() > 140 {
                                let mut end = 0;
                                for (j, _) in line.char_indices().take(140) { end = j; }
                                format!("{}...", line[..end].trim_end())
                            } else {
                                line.trim().to_string()
                            };
                            out.push((target.to_lowercase(), snippet));
                        }
                    }
                    i += 2 + close + 2;
                    continue;
                }
            }
            i += 1;
        }
    }
    out
}

/// Build a name → canonical path lookup for the whole vault. Matches
/// the frontend's resolver: case-insensitive stem match, shortest path
/// wins on ambiguity.
fn build_resolver(files: &[PathBuf]) -> HashMap<String, PathBuf> {
    let mut map: HashMap<String, PathBuf> = HashMap::new();
    for f in files {
        let stem_lc = file_stem(f).to_lowercase();
        match map.get(&stem_lc) {
            Some(existing) if existing.as_os_str().len() < f.as_os_str().len() => {}
            _ => {
                map.insert(stem_lc, f.clone());
            }
        }
    }
    map
}

/// Return the files that link TO `target_path`. Each entry includes the
/// file path, name, and a line snippet showing the link in context.
pub fn list_backlinks(vault_root: &Path, target_path: &Path) -> Vec<LinkHit> {
    let files = collect_md_files(vault_root);
    let resolver = build_resolver(&files);
    let target_stem = file_stem(target_path).to_lowercase();

    let mut out = Vec::new();
    for f in &files {
        if f == target_path { continue; }
        let Ok(text) = fs::read_to_string(f) else { continue };
        let links = extract_wikilinks(&text);
        for (target, snippet) in links {
            // Resolve the link to a path. Match either by exact stem
            // or by resolver lookup.
            let resolved = resolver.get(&target).cloned();
            let resolved_to_us = match resolved {
                Some(p) => p == target_path,
                None => target == target_stem,
            };
            if resolved_to_us {
                out.push(LinkHit {
                    path: f.to_string_lossy().to_string(),
                    name: file_stem(f),
                    snippet,
                });
                break; // one entry per file is enough
            }
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// Build the whole-vault link graph. Nodes = every .md file. Edges =
/// every resolved [[link]] pointing from one file to another. Unresolved
/// links are dropped (we don't create ghost nodes).
pub fn build_link_graph(vault_root: &Path) -> LinkGraph {
    let files = collect_md_files(vault_root);
    let resolver = build_resolver(&files);
    let mut degree: HashMap<String, u32> = HashMap::new();
    let mut edges: Vec<GraphEdge> = Vec::new();
    let mut seen_edges: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();

    for f in &files {
        let from = f.to_string_lossy().to_string();
        *degree.entry(from.clone()).or_insert(0) += 0; // ensure node exists
        let Ok(text) = fs::read_to_string(f) else { continue };
        for (target, _) in extract_wikilinks(&text) {
            let Some(to_path) = resolver.get(&target) else { continue };
            if to_path == f { continue; } // self-links drop
            let to = to_path.to_string_lossy().to_string();
            let key = (from.clone(), to.clone());
            if seen_edges.insert(key) {
                *degree.entry(from.clone()).or_insert(0) += 1;
                *degree.entry(to.clone()).or_insert(0) += 1;
                edges.push(GraphEdge { source: from.clone(), target: to });
            }
        }
    }

    let nodes: Vec<GraphNode> = files
        .iter()
        .map(|f| {
            let id = f.to_string_lossy().to_string();
            let deg = *degree.get(&id).unwrap_or(&0);
            GraphNode { id, name: file_stem(f), degree: deg }
        })
        .collect();

    LinkGraph { nodes, edges }
}
