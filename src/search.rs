use std::path::{Path, PathBuf};
use std::fs;
use std::time::UNIX_EPOCH;
use std::collections::HashMap;

use crate::embedder::LocalEmbedder;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Debug)]
pub struct Chunk {
    pub id: u64,
    pub file_path: PathBuf,
    pub heading: String,
    pub content: String,
    pub byte_start: usize,
    pub byte_end: usize,
}

#[derive(Clone, Debug)]
pub struct SearchResult {
    pub chunk: Chunk,
    pub score: f32,
}

pub struct VaultSearch {
    db: rusqlite::Connection,
    hnsw: usearch::Index,
    embedder: Option<LocalEmbedder>,
    next_id: u64,
    vectors_enabled: bool,
    index_path: PathBuf,
}

impl VaultSearch {
    pub fn new(db_path: &Path, index_path: &Path) -> Result<Self> {
        let db = rusqlite::Connection::open(db_path)?;

        db.execute_batch("
            CREATE TABLE IF NOT EXISTS chunks (
                id INTEGER PRIMARY KEY,
                file_path TEXT NOT NULL,
                heading TEXT NOT NULL,
                content TEXT NOT NULL,
                byte_start INTEGER NOT NULL,
                byte_end INTEGER NOT NULL,
                mtime INTEGER NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
                content, tokenize='porter unicode61'
            );
        ")?;

        let opts = usearch::IndexOptions {
            dimensions: 384,
            metric: usearch::MetricKind::Cos,
            ..Default::default()
        };
        let hnsw = usearch::new_index(&opts)?;

        if index_path.exists() && fs::metadata(index_path).map(|m| m.len() > 0).unwrap_or(false) {
            hnsw.load(index_path.to_str().ok_or("invalid index path")?)?;
        }

        // Try to load the local embedding model. If it fails (e.g. first run,
        // model download fails), fall back to BM25-only.
        let (embedder, vectors_enabled) = match LocalEmbedder::new() {
            Ok(e) => {
                eprintln!("[forge] Embedding model loaded (384 dims)");
                (Some(e), true)
            }
            Err(e) => {
                eprintln!("[forge] Embedding model failed: {}. BM25 only.", e);
                (None, false)
            }
        };

        let next_id: u64 = db.query_row(
            "SELECT COALESCE(MAX(id), 0) FROM chunks",
            [],
            |row| row.get::<_, i64>(0),
        )? as u64 + 1;

        Ok(Self {
            db,
            hnsw,
            embedder,
            next_id,
            vectors_enabled,
            index_path: index_path.to_path_buf(),
        })
    }

    pub fn build_vault(&mut self, vault_root: &Path) -> Result<()> {
        let md_files = collect_md_files(vault_root);
        for path in md_files {
            self.index_file(&path)?;
        }
        let index_path = self.index_path.clone();
        self.save_index(&index_path)?;
        Ok(())
    }

    pub fn index_file(&mut self, path: &Path) -> Result<()> {
        let content = fs::read_to_string(path)?;
        let mtime = fs::metadata(path)?
            .modified()?
            .duration_since(UNIX_EPOCH)?
            .as_secs() as i64;

        let path_str = path.to_str().ok_or("invalid file path")?;

        let existing_mtime: Option<i64> = self.db.query_row(
            "SELECT mtime FROM chunks WHERE file_path = ? LIMIT 1",
            rusqlite::params![path_str],
            |row| row.get(0),
        ).ok();

        if let Some(existing) = existing_mtime {
            if existing == mtime {
                return Ok(());
            }
        }

        let old_ids: Vec<u64> = {
            let mut stmt = self.db.prepare("SELECT id FROM chunks WHERE file_path = ?")?;
            let ids: std::result::Result<Vec<u64>, _> = stmt
                .query_map(rusqlite::params![path_str], |row| row.get::<_, i64>(0))?
                .map(|r| r.map(|v| v as u64))
                .collect();
            ids?
        };

        for &id in &old_ids {
            self.db.execute("DELETE FROM chunks_fts WHERE rowid = ?", rusqlite::params![id as i64])?;
            let _ = self.hnsw.remove(id);
        }

        self.db.execute("DELETE FROM chunks WHERE file_path = ?", rusqlite::params![path_str])?;

        let chunks = chunk_file(&content);

        for (heading, chunk_text, byte_start, byte_end) in chunks {
            if chunk_text.len() < 20 {
                continue;
            }

            let id = self.next_id;
            self.next_id += 1;

            self.db.execute(
                "INSERT INTO chunks (id, file_path, heading, content, byte_start, byte_end, mtime) VALUES (?, ?, ?, ?, ?, ?, ?)",
                rusqlite::params![id as i64, path_str, heading, chunk_text, byte_start as i64, byte_end as i64, mtime],
            )?;

            self.db.execute(
                "INSERT INTO chunks_fts (rowid, content) VALUES (?, ?)",
                rusqlite::params![id as i64, chunk_text],
            )?;

            if self.vectors_enabled {
                if let Some(vector) = self.embedder.as_ref().and_then(|e| e.embed(&chunk_text).ok()) {
                    if self.hnsw.capacity() < self.hnsw.size() + 1 {
                        self.hnsw.reserve(self.hnsw.capacity().max(64) * 2)?;
                    }
                    self.hnsw.add(id, &vector)?;
                }
            }
        }

        Ok(())
    }

    pub fn remove_file(&mut self, path: &Path) -> Result<()> {
        let path_str = path.to_str().ok_or("invalid file path")?;

        let ids: Vec<u64> = {
            let mut stmt = self.db.prepare("SELECT id FROM chunks WHERE file_path = ?")?;
            let ids: std::result::Result<Vec<u64>, _> = stmt
                .query_map(rusqlite::params![path_str], |row| row.get::<_, i64>(0))?
                .map(|r| r.map(|v| v as u64))
                .collect();
            ids?
        };

        for &id in &ids {
            self.db.execute("DELETE FROM chunks_fts WHERE rowid = ?", rusqlite::params![id as i64])?;
            let _ = self.hnsw.remove(id);
        }

        self.db.execute("DELETE FROM chunks WHERE file_path = ?", rusqlite::params![path_str])?;

        Ok(())
    }

    pub fn search(&self, query: &str, k: usize) -> Result<Vec<SearchResult>> {
        let limit = (k * 2) as i64;

        // Build FTS5 query.
        // Starts with " → BM25-only mode (prefix matching, no vectors).
        // Otherwise → hybrid (prefix BM25 + vector search).
        let trimmed = query.trim();
        let bm25_only = trimmed.starts_with('"');
        let search_text = if bm25_only {
            trimmed.trim_matches('"').trim()
        } else {
            trimmed
        };
        let fts_query: String = search_text.split_whitespace()
            .filter(|w| !w.is_empty())
            .map(|w| {
                let clean: String = w.chars().filter(|c| c.is_alphanumeric() || *c == '_').collect();
                if clean.is_empty() { String::new() } else { format!("{}*", clean) }
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");

        if fts_query.is_empty() {
            return Ok(Vec::new());
        }

        let mut bm25_map: HashMap<u64, (f32, Chunk)> = HashMap::new();

        {
            let mut stmt = self.db.prepare(
                "SELECT c.id, c.file_path, c.heading, c.content, c.byte_start, c.byte_end, f.rank \
                 FROM chunks_fts f \
                 JOIN chunks c ON c.id = f.rowid \
                 WHERE chunks_fts MATCH ?1 \
                 ORDER BY f.rank \
                 LIMIT ?2"
            )?;

            let rows = stmt.query_map(rusqlite::params![fts_query, limit], |row| {
                let id: i64 = row.get(0)?;
                let file_path: String = row.get(1)?;
                let heading: String = row.get(2)?;
                let content: String = row.get(3)?;
                let byte_start: i64 = row.get(4)?;
                let byte_end: i64 = row.get(5)?;
                let rank: f64 = row.get(6)?;
                Ok((id as u64, file_path, heading, content, byte_start as usize, byte_end as usize, rank as f32))
            })?;

            for row in rows {
                let (id, file_path, heading, content, byte_start, byte_end, rank) = row?;
                let bm25_score = 1.0_f32 / (1.0 - rank);
                let chunk = Chunk {
                    id,
                    file_path: PathBuf::from(file_path),
                    heading,
                    content,
                    byte_start,
                    byte_end,
                };
                bm25_map.insert(id, (bm25_score, chunk));
            }
        }

        if !self.vectors_enabled || bm25_only {
            let mut scored: Vec<(u64, f32)> = bm25_map
                .iter()
                .map(|(&id, (score, _))| (id, *score))
                .collect();
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            scored.truncate(k);

            let results = scored
                .into_iter()
                .filter_map(|(id, score)| {
                    bm25_map.get(&id).map(|(_, chunk)| SearchResult {
                        chunk: chunk.clone(),
                        score,
                    })
                })
                .collect();

            return Ok(results);
        }

        let query_vector = match self.embedder.as_ref().and_then(|e| e.embed(query).ok()) {
            Some(v) => v,
            None => {
                // Ollama went away mid-session; fall back to BM25 only
                let mut scored: Vec<(u64, f32)> = bm25_map
                    .iter()
                    .map(|(&id, (score, _))| (id, *score))
                    .collect();
                scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                scored.truncate(k);

                let results = scored
                    .into_iter()
                    .filter_map(|(id, score)| {
                        bm25_map.get(&id).map(|(_, chunk)| SearchResult {
                            chunk: chunk.clone(),
                            score,
                        })
                    })
                    .collect();

                return Ok(results);
            }
        };

        let vec_k = k * 2;
        let matches = self.hnsw.search(&query_vector, vec_k)?;

        let mut vector_map: HashMap<u64, f32> = HashMap::new();
        for (key, distance) in matches.keys.iter().zip(matches.distances.iter()) {
            let score = 1.0 - distance;
            vector_map.insert(*key, score);
        }

        let mut final_scores: HashMap<u64, f32> = HashMap::new();

        // BM25 gets higher weight -- it's more precise for keyword queries.
        // Vectors help for semantic/fuzzy matches but shouldn't override exact hits.
        for (&id, (bm25_score, _)) in &bm25_map {
            *final_scores.entry(id).or_insert(0.0) += 0.7 * bm25_score;
        }

        for (&id, &vec_score) in &vector_map {
            *final_scores.entry(id).or_insert(0.0) += 0.3 * vec_score;
        }

        let vec_ids_needing_lookup: Vec<u64> = vector_map.keys()
            .filter(|id| !bm25_map.contains_key(id))
            .copied()
            .collect();

        let mut extra_chunks: HashMap<u64, Chunk> = HashMap::new();
        for id in vec_ids_needing_lookup {
            let result: std::result::Result<(String, String, String, i64, i64), _> = self.db.query_row(
                "SELECT file_path, heading, content, byte_start, byte_end FROM chunks WHERE id = ?",
                rusqlite::params![id as i64],
                |row| Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                )),
            );
            if let Ok((file_path, heading, content, byte_start, byte_end)) = result {
                extra_chunks.insert(id, Chunk {
                    id,
                    file_path: PathBuf::from(file_path),
                    heading,
                    content,
                    byte_start: byte_start as usize,
                    byte_end: byte_end as usize,
                });
            }
        }

        let mut scored: Vec<(u64, f32)> = final_scores.into_iter().collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);

        let results = scored
            .into_iter()
            .filter_map(|(id, score)| {
                let chunk = if let Some((_, chunk)) = bm25_map.get(&id) {
                    chunk.clone()
                } else if let Some(chunk) = extra_chunks.get(&id) {
                    chunk.clone()
                } else {
                    return None;
                };
                Some(SearchResult { chunk, score })
            })
            .collect();

        Ok(results)
    }

    pub fn save_index(&self, path: &Path) -> Result<()> {
        self.hnsw.save(path.to_str().ok_or("invalid index path")?)?;
        Ok(())
    }

    pub fn chunk_count(&self) -> usize {
        self.db
            .query_row("SELECT COUNT(*) FROM chunks", [], |row| row.get::<_, i64>(0))
            .unwrap_or(0) as usize
    }

    pub fn vectors_available(&self) -> bool {
        self.vectors_enabled
    }
}

fn collect_md_files(root: &Path) -> Vec<PathBuf> {
    let mut results = Vec::new();
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                results.extend(collect_md_files(&path));
            } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                results.push(path);
            }
        }
    }
    results
}

fn chunk_file(content: &str) -> Vec<(String, String, usize, usize)> {
    let mut chunks: Vec<(String, String, usize, usize)> = Vec::new();

    let mut current_heading = "(top)".to_string();
    let mut current_lines: Vec<&str> = Vec::new();
    let mut current_byte_start: usize = 0;
    let mut byte_offset: usize = 0;

    for line in content.split_inclusive('\n') {
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
        if trimmed.starts_with('#') {
            let chunk_text = current_lines.join("");
            let chunk_byte_end = byte_offset;
            if chunk_text.trim().len() >= 20 {
                chunks.push((
                    current_heading.clone(),
                    chunk_text,
                    current_byte_start,
                    chunk_byte_end,
                ));
            }
            current_heading = trimmed.to_string();
            current_lines = vec![line];
            current_byte_start = byte_offset;
        } else {
            current_lines.push(line);
        }
        byte_offset += line.len();
    }

    let chunk_text = current_lines.join("");
    if chunk_text.trim().len() >= 20 {
        chunks.push((
            current_heading,
            chunk_text,
            current_byte_start,
            byte_offset,
        ));
    }

    chunks
}
