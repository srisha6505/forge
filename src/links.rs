//! Wikilink index: resolves `[[Note]]` targets to file paths and tracks backlinks.
//!
//! Built once on vault load, then updated incrementally on save / file-watcher events.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::markdown;

/// A single occurrence of a wikilink in some source file.
#[derive(Clone, Debug)]
pub struct LinkRef {
    pub source: PathBuf,
    pub line: usize,
    /// Short preview of the line text containing the link (trimmed, ~120 chars).
    pub context: String,
    /// Target as written in the source (pre-resolution, for display).
    pub target: String,
}

/// Reverse-mapping index of wikilinks across the vault.
#[derive(Default, Debug)]
pub struct LinkIndex {
    /// All `.md` files in the vault, keyed by lowercased basename (without `.md`).
    /// Value is a list because two files can share a basename.
    name_to_paths: HashMap<String, Vec<PathBuf>>,
    /// For a source file: the wikilinks it contains.
    outgoing: HashMap<PathBuf, Vec<LinkRef>>,
    /// For a target name (lowercased basename): places that link to it.
    backlinks: HashMap<String, Vec<LinkRef>>,
}

impl LinkIndex {
    pub fn new() -> Self { Self::default() }

    /// List of all `.md` file paths known to the index.
    /// Returned as (absolute_path, relative_path_from_vault_root) pairs,
    /// where the relative path is derived by callers -- here we only store absolute paths.
    pub fn all_paths(&self) -> Vec<&Path> {
        let mut v: Vec<&Path> = self.name_to_paths
            .values()
            .flat_map(|paths| paths.iter().map(|p| p.as_path()))
            .collect();
        v.sort();
        v
    }

    /// Build a fresh index by walking the vault directory recursively.
    pub fn scan_vault(vault_root: &Path) -> Self {
        let mut idx = Self::default();
        let mut md_files: Vec<PathBuf> = Vec::new();
        collect_md_files(vault_root, &mut md_files);
        for path in &md_files {
            idx.register_path(path.clone());
        }
        for path in &md_files {
            if let Ok(content) = std::fs::read_to_string(path) {
                idx.reindex_content(path, &content);
            }
        }
        idx
    }

    /// Record that a file exists (so name resolution can find it) without
    /// parsing its contents yet.
    fn register_path(&mut self, path: PathBuf) {
        let key = basename_key(&path);
        if key.is_empty() { return; }
        let entry = self.name_to_paths.entry(key).or_default();
        if !entry.contains(&path) { entry.push(path); }
    }

    fn unregister_path(&mut self, path: &Path) {
        let key = basename_key(path);
        if let Some(list) = self.name_to_paths.get_mut(&key) {
            list.retain(|p| p != path);
            if list.is_empty() { self.name_to_paths.remove(&key); }
        }
    }

    /// Parse `content` for wikilinks and update outgoing + backlinks for `source`.
    fn reindex_content(&mut self, source: &Path, content: &str) {
        // Remove existing outgoing + backlinks for this source.
        self.purge_outgoing(source);

        // Build line_starts the same way the editor does.
        let line_starts = build_line_starts(content);
        let lines = markdown::parse_lines(content, &line_starts);

        let mut new_outgoing: Vec<LinkRef> = Vec::new();
        for (line_idx, info) in lines.iter().enumerate() {
            if info.wikilinks.is_empty() { continue; }
            let line_text = line_text(content, &line_starts, line_idx);
            for w in &info.wikilinks {
                let lref = LinkRef {
                    source: source.to_path_buf(),
                    line: line_idx,
                    context: truncate_context(line_text),
                    target: w.target.clone(),
                };
                let key = name_key(&w.target);
                self.backlinks.entry(key).or_default().push(lref.clone());
                new_outgoing.push(lref);
            }
        }
        if !new_outgoing.is_empty() {
            self.outgoing.insert(source.to_path_buf(), new_outgoing);
        }
    }

    /// Remove all outgoing links and corresponding backlink entries for `source`.
    fn purge_outgoing(&mut self, source: &Path) {
        if let Some(old) = self.outgoing.remove(source) {
            // For each old link, remove matching entries from backlinks.
            for lref in old {
                let key = name_key(&lref.target);
                if let Some(list) = self.backlinks.get_mut(&key) {
                    list.retain(|r| !(r.source == lref.source && r.line == lref.line));
                    if list.is_empty() { self.backlinks.remove(&key); }
                }
            }
        }
    }

    /// Incremental update: call after saving or on external file change.
    pub fn update_file(&mut self, path: &Path, content: &str) {
        self.register_path(path.to_path_buf());
        self.reindex_content(path, content);
    }

    /// Call when a file is deleted.
    pub fn remove_file(&mut self, path: &Path) {
        self.purge_outgoing(path);
        self.unregister_path(path);
    }

    /// Resolve a wikilink target (e.g. `"Other Note"`) to a file path.
    /// Case-insensitive basename match. Returns shortest path on collision.
    pub fn resolve(&self, target: &str) -> Option<&Path> {
        let key = name_key(target);
        let list = self.name_to_paths.get(&key)?;
        if list.is_empty() { return None; }
        // Prefer the shortest path (matches Obsidian's default heuristic loosely).
        list.iter().min_by_key(|p| p.as_os_str().len()).map(|p| p.as_path())
    }

    /// Whether a target name resolves to some file in the vault.
    pub fn exists(&self, target: &str) -> bool {
        self.resolve(target).is_some()
    }

    /// All places that link to a given note (by its file path).
    pub fn backlinks_for_path(&self, path: &Path) -> Vec<&LinkRef> {
        let key = basename_key(path);
        if key.is_empty() { return Vec::new(); }
        match self.backlinks.get(&key) {
            Some(list) => list.iter().filter(|r| r.source != path).collect(),
            None => Vec::new(),
        }
    }
}

/// Lowercased basename without `.md` extension. Empty if `path` has no file name.
fn basename_key(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default()
}

fn name_key(target: &str) -> String {
    target.trim().to_ascii_lowercase()
}

fn collect_md_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return; };
    for entry in entries.flatten() {
        let path = entry.path();
        // Skip hidden files and directories (.git, .obsidian, etc.).
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') { continue; }
        }
        if path.is_dir() {
            collect_md_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(path);
        }
    }
}

fn build_line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' { starts.push(i + 1); }
    }
    starts
}

fn line_text<'a>(text: &'a str, line_starts: &[usize], line: usize) -> &'a str {
    let start = line_starts[line];
    let end = if line + 1 < line_starts.len() {
        let e = line_starts[line + 1];
        if e > start && text.as_bytes().get(e - 1) == Some(&b'\n') { e - 1 } else { e }
    } else {
        text.len()
    };
    &text[start..end]
}

fn truncate_context(line: &str) -> String {
    const MAX: usize = 120;
    let trimmed = line.trim();
    if trimmed.chars().count() <= MAX {
        trimmed.to_string()
    } else {
        let mut out: String = trimmed.chars().take(MAX).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_basic() {
        let mut idx = LinkIndex::new();
        idx.register_path(PathBuf::from("/vault/Alpha.md"));
        idx.register_path(PathBuf::from("/vault/sub/Beta.md"));
        assert_eq!(idx.resolve("Alpha").unwrap(), Path::new("/vault/Alpha.md"));
        assert_eq!(idx.resolve("alpha").unwrap(), Path::new("/vault/Alpha.md"));
        assert_eq!(idx.resolve("Beta").unwrap(), Path::new("/vault/sub/Beta.md"));
        assert!(idx.resolve("Missing").is_none());
    }

    #[test]
    fn resolve_collision_prefers_shortest_path() {
        let mut idx = LinkIndex::new();
        idx.register_path(PathBuf::from("/vault/deep/nested/README.md"));
        idx.register_path(PathBuf::from("/vault/README.md"));
        assert_eq!(idx.resolve("README").unwrap(), Path::new("/vault/README.md"));
    }

    #[test]
    fn reindex_builds_backlinks() {
        let mut idx = LinkIndex::new();
        let a = PathBuf::from("/vault/A.md");
        let b = PathBuf::from("/vault/B.md");
        idx.register_path(a.clone());
        idx.register_path(b.clone());
        idx.reindex_content(&a, "Link to [[B]] here.\nAnd again [[b|alias]].");
        idx.reindex_content(&b, "Back to [[A]].");

        let b_backlinks = idx.backlinks_for_path(&b);
        assert_eq!(b_backlinks.len(), 2);
        assert!(b_backlinks.iter().all(|r| r.source == a));

        let a_backlinks = idx.backlinks_for_path(&a);
        assert_eq!(a_backlinks.len(), 1);
        assert_eq!(a_backlinks[0].source, b);
        assert_eq!(a_backlinks[0].line, 0);
    }

    #[test]
    fn reindex_replaces_previous_entries() {
        let mut idx = LinkIndex::new();
        let a = PathBuf::from("/vault/A.md");
        let b = PathBuf::from("/vault/B.md");
        idx.register_path(a.clone());
        idx.register_path(b.clone());
        idx.reindex_content(&a, "[[B]]");
        assert_eq!(idx.backlinks_for_path(&b).len(), 1);
        // Remove the link:
        idx.reindex_content(&a, "no links anymore");
        assert_eq!(idx.backlinks_for_path(&b).len(), 0);
    }

    #[test]
    fn remove_file_clears_index() {
        let mut idx = LinkIndex::new();
        let a = PathBuf::from("/vault/A.md");
        let b = PathBuf::from("/vault/B.md");
        idx.register_path(a.clone());
        idx.register_path(b.clone());
        idx.reindex_content(&a, "[[B]]");
        idx.remove_file(&a);
        assert_eq!(idx.backlinks_for_path(&b).len(), 0);
        assert!(idx.resolve("A").is_none());
    }

    #[test]
    fn self_links_excluded_from_backlinks() {
        let mut idx = LinkIndex::new();
        let a = PathBuf::from("/vault/A.md");
        idx.register_path(a.clone());
        idx.reindex_content(&a, "self ref [[A]]");
        assert_eq!(idx.backlinks_for_path(&a).len(), 0);
    }
}
