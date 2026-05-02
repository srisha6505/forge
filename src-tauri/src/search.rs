use std::path::{Path, PathBuf};
use std::fs;
use std::io::Read;
use std::process::Command;
use std::time::UNIX_EPOCH;
use std::collections::HashMap;

use crate::embedder::LocalEmbedder;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Cosine-similarity floor for vector-only hits. all-MiniLM-L6-v2 routinely
/// pairs unrelated English text at ~0.3-0.5; below this threshold a hit is
/// indistinguishable from noise. Tuned against a small vault — raise if
/// users complain about junk matches, lower if related notes go missing.
const VECTOR_FLOOR: f32 = 0.55;

#[derive(Clone, Debug)]
pub struct Chunk {
    pub id: u64,
    pub file_path: PathBuf,
    pub heading: String,
    pub content: String,
    pub byte_start: usize,
    pub byte_end: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HitSource {
    /// Came from BM25 (FTS5) — the chunk literally contains a query token.
    Keyword,
    /// Came from vector similarity only — semantic neighbour, no BM25 hit.
    Vector,
    /// Came from literal substring match (quoted mode). Stronger guarantee
    /// than `Keyword` because there's no Porter stemming or prefix expansion.
    Literal,
}

#[derive(Clone, Debug)]
pub struct SearchResult {
    pub chunk: Chunk,
    pub score: f32,
    pub source: HitSource,
    /// Lowercased terms that actually match somewhere in the chunk. Used by
    /// the UI for highlighting. Empty for pure-vector hits.
    pub matched_terms: Vec<String>,
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

        if index_path.exists() && fs::metadata(index_path).map(|m| m.len() > 100).unwrap_or(false) {
            // usearch may panic on malformed index files. Isolate the load
            // so a bad file just means we rebuild rather than crash.
            let load_path = index_path.to_path_buf();
            let hnsw_clone = &hnsw;
            let load_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                hnsw_clone.load(load_path.to_str().ok_or_else(|| "invalid path".to_string())?)
                    .map_err(|e| e.to_string())
            }));
            match load_result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    eprintln!("[forge] hnsw load failed: {e}. Starting fresh.");
                    let _ = fs::remove_file(index_path);
                }
                Err(_) => {
                    eprintln!("[forge] hnsw load panicked. Starting fresh.");
                    let _ = fs::remove_file(index_path);
                }
            }
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
        let files = collect_indexable_files(vault_root);
        for (idx, path) in files.iter().enumerate() {
            // Per-file errors AND panics must not abort the whole walk
            // — a single corrupt PDF, password-protected DOCX, or worse
            // a panic inside candle/usearch on weird input shouldn't
            // kill the index build for the rest of the vault. The
            // catch_unwind boundary is critical: candle's BLAS path has
            // been observed to panic on certain edge tensors, and a
            // panic here would otherwise unwind through the Tauri
            // command thread and bring the whole app down.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.index_file(path)
            }));
            match result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    eprintln!("[forge] index_file error for {:?}: {}", path, e);
                }
                Err(_) => {
                    eprintln!("[forge] index_file PANICKED for {:?} — skipping", path);
                }
            }
            // Yield to the OS every few files. Without this, candle's
            // BLAS-backed BERT forward pass plus pdftotext spawns peg
            // every core for the entire build, starving webkit2gtk's
            // main thread and making the UI feel frozen / "crashed".
            // 10ms × every-5th-file = ~2% throughput hit on cold builds,
            // but the webview stays responsive throughout.
            if idx % 5 == 4 {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        let index_path = self.index_path.clone();
        self.save_index(&index_path)?;
        Ok(())
    }

    pub fn index_file(&mut self, path: &Path) -> Result<()> {
        let path_str = path.to_str().ok_or("invalid file path")?;

        // Check mtime BEFORE doing any expensive work. The previous
        // ordering ran pdftotext / DOCX-unzip on every file every boot,
        // which on a vault with 20+ PDFs spawned 20+ pdftotext processes
        // and read all that text into memory before even discovering
        // there was nothing to do. That spike is what was crashing the
        // app on warm-start.
        let metadata = fs::metadata(path)?;
        let mtime = metadata
            .modified()?
            .duration_since(UNIX_EPOCH)?
            .as_secs() as i64;

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

        // Hard cap on the on-disk size we'll feed through the extractor.
        // A 200 MB PDF can produce hundreds of MB of text, which then
        // gets duplicated through chunking and embedded chunk-by-chunk —
        // a guaranteed memory pressure event. Skip with a warning rather
        // than risking an OOM that takes down the whole app.
        const MAX_INDEXABLE_BYTES: u64 = 25 * 1024 * 1024;
        if metadata.len() > MAX_INDEXABLE_BYTES {
            return Err(format!(
                "skipping large file ({:.1} MB) {:?}",
                metadata.len() as f64 / 1_048_576.0,
                path
            )
            .into());
        }

        // Now safe to actually read + extract — only on a real change.
        let content = read_indexable_text(path)?;

        // Second guard: even within a 25 MB file, a 50000-page PDF could
        // produce many MB of text. Truncate at 4 MB of text — beyond that
        // we'd be embedding hundreds of chunks per file and the rest is
        // probably boilerplate / TOC anyway.
        let content = if content.len() > 4 * 1024 * 1024 {
            eprintln!(
                "[forge] {:?} text {:.1} MB → truncated for indexing",
                path,
                content.len() as f64 / 1_048_576.0
            );
            content.chars().take(4_000_000).collect::<String>()
        } else {
            content
        };

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
                // Isolate embedder + hnsw ops: one bad chunk must not kill
                // the whole indexing run.
                let chunk_text_copy = chunk_text.clone();
                let embedder = self.embedder.as_ref();
                let hnsw = &self.hnsw;
                let id_local = id;
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let vector = embedder?.embed(&chunk_text_copy).ok()?;
                    if hnsw.capacity() < hnsw.size() + 1 {
                        let _ = hnsw.reserve(hnsw.capacity().max(64) * 2);
                    }
                    let _ = hnsw.add(id_local, &vector);
                    Some(())
                }));
                if result.is_err() {
                    eprintln!("[forge] embed/add panicked on chunk {id_local}, skipping");
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
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        // Quoted mode → literal case-insensitive substring search across the
        // already-indexed chunk table. No FTS, no Porter stemming, no
        // prefix expansion. The user wants `"breach"` to mean exactly that.
        // Opening quote alone counts (so partially-typed `"brea` works the
        // moment the user starts the quote).
        if trimmed.starts_with('"') {
            let needle = trimmed.trim_matches('"').trim();
            if needle.is_empty() {
                return Ok(Vec::new());
            }
            return self.literal_search(needle, k);
        }

        // Split on every non-word char — not just whitespace. Critical for
        // hyphenated/compound terms: the FTS5 unicode61 tokenizer splits
        // `self-driving` into `self` and `driving` at index time, so the
        // query `selfdriving*` (what the old splitter produced by stripping
        // the hyphen) never matched anything. By splitting on non-alphanum
        // we get `["self","driving"]` and the FTS query `self* driving*`
        // matches the indexed tokens.
        let raw_terms: Vec<String> = trimmed
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase())
            .collect();

        if raw_terms.is_empty() {
            return Ok(Vec::new());
        }

        // FTS5 prefix-AND query: `term1* term2*`. With Porter stemming this
        // already handles plurals etc., so the `*` is mostly insurance for
        // partial words a user is typing.
        let fts_query = raw_terms
            .iter()
            .map(|t| format!("{}*", t))
            .collect::<Vec<_>>()
            .join(" ");

        let limit = (k * 4) as i64;
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
                // FTS5 rank is a negative number (more negative = better);
                // map it to a positive 0..1-ish band.
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

        // Filenames aren't part of the FTS5 content column. Without this
        // pass, a query that matches a file's name (but not its body) would
        // return nothing — a common surprise when the user types a topic
        // they know is *in the vault* but as a title. We boost the BM25
        // score so filename hits sort above semantic neighbours.
        for term in &raw_terms {
            let mut stmt = self.db.prepare(
                "SELECT id, file_path, heading, content, byte_start, byte_end \
                 FROM chunks \
                 WHERE file_path LIKE ?1 ESCAPE '\\' COLLATE NOCASE \
                 LIMIT 50"
            )?;
            let escaped = term
                .replace('\\', r"\\")
                .replace('%', r"\%")
                .replace('_', r"\_");
            let pattern = format!("%{}%", escaped);
            let rows = stmt.query_map(rusqlite::params![pattern], |row| {
                let id: i64 = row.get(0)?;
                let file_path: String = row.get(1)?;
                let heading: String = row.get(2)?;
                let content: String = row.get(3)?;
                let byte_start: i64 = row.get(4)?;
                let byte_end: i64 = row.get(5)?;
                Ok((id as u64, file_path, heading, content, byte_start as usize, byte_end as usize))
            })?;
            for row in rows {
                let (id, file_path, heading, content, byte_start, byte_end) = row?;
                let chunk = Chunk {
                    id,
                    file_path: PathBuf::from(file_path),
                    heading,
                    content,
                    byte_start,
                    byte_end,
                };
                // Blend with existing BM25 score if any; otherwise seed
                // a strong-but-not-overpowering filename score.
                bm25_map
                    .entry(id)
                    .and_modify(|(s, _)| *s = (*s).max(0.6))
                    .or_insert((0.6, chunk));
            }
        }

        // Vectors disabled or unavailable → keyword-only ranking.
        if !self.vectors_enabled {
            return Ok(rank_keyword_only(&bm25_map, &raw_terms, k));
        }

        // Skip the vector path for very short queries. The all-MiniLM-L6
        // forward pass costs 50-100ms on CPU and short queries don't carry
        // enough semantic signal to be useful anyway. Once the user has
        // typed a real word, semantic neighbours kick in.
        let total_query_chars: usize = raw_terms.iter().map(|t| t.chars().count()).sum();
        if total_query_chars < 3 {
            return Ok(rank_keyword_only(&bm25_map, &raw_terms, k));
        }

        let query_vector = match self.embedder.as_ref().and_then(|e| e.embed(trimmed).ok()) {
            Some(v) => v,
            None => return Ok(rank_keyword_only(&bm25_map, &raw_terms, k)),
        };

        if self.hnsw.size() == 0 {
            return Ok(rank_keyword_only(&bm25_map, &raw_terms, k));
        }

        let matches = self.hnsw.search(&query_vector, k * 4)?;

        let mut vector_map: HashMap<u64, f32> = HashMap::new();
        for (key, distance) in matches.keys.iter().zip(matches.distances.iter()) {
            // usearch cosine: distance ∈ [0, 2], similarity = 1 - distance
            // (negative possible for opposite vectors, ignore).
            let score = 1.0_f32 - distance;
            // Floor: only keep semantically-strong neighbours. all-MiniLM-L6
            // routinely produces 0.3-0.5 similarity for *unrelated* English
            // text, so anything under VECTOR_FLOOR is noise.
            if score >= VECTOR_FLOOR {
                vector_map.insert(*key, score);
            }
        }

        // Pull chunk records for vector-only hits we didn't already fetch.
        let vec_ids_needing_lookup: Vec<u64> = vector_map
            .keys()
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

        // Combine. Keyword hits dominate (their score gets a bonus so they
        // always rank above pure-vector hits with similar raw scores).
        let mut combined: HashMap<u64, (f32, HitSource, Chunk)> = HashMap::new();

        for (&id, (bm25_score, chunk)) in &bm25_map {
            let vec_boost = vector_map.get(&id).copied().unwrap_or(0.0);
            // Anchor keyword hits at >=1.0 so they sort above any vector-only
            // hit (which is bounded by 1.0 from the cosine similarity).
            let score = 1.0 + bm25_score + 0.3 * vec_boost;
            combined.insert(id, (score, HitSource::Keyword, chunk.clone()));
        }

        for (&id, &vec_score) in &vector_map {
            if combined.contains_key(&id) {
                continue;
            }
            if let Some(chunk) = extra_chunks.get(&id) {
                combined.insert(id, (vec_score, HitSource::Vector, chunk.clone()));
            }
        }

        let mut scored: Vec<(u64, f32, HitSource, Chunk)> = combined
            .into_iter()
            .map(|(id, (s, src, c))| (id, s, src, c))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);

        Ok(scored
            .into_iter()
            .map(|(_, score, source, chunk)| {
                let matched_terms = if matches!(source, HitSource::Keyword) {
                    terms_present_in(&chunk.content, &raw_terms)
                } else {
                    Vec::new()
                };
                SearchResult { chunk, score, source, matched_terms }
            })
            .collect())
    }

    /// Case-insensitive literal substring search over chunk content + heading
    /// + file path. Used when the query is wrapped in quotes — bypasses FTS
    /// entirely so the user gets exactly what they typed, no stemming, no
    /// prefix. `COLLATE NOCASE` makes LIKE itself case-insensitive at SQLite
    /// level (ASCII-only, fine for English) — much faster than wrapping the
    /// column in `LOWER()` per row.
    fn literal_search(&self, needle: &str, k: usize) -> Result<Vec<SearchResult>> {
        let needle_lower = needle.to_lowercase();
        let mut stmt = self.db.prepare(
            "SELECT id, file_path, heading, content, byte_start, byte_end \
             FROM chunks \
             WHERE content LIKE ?1 ESCAPE '\\' COLLATE NOCASE \
                OR heading LIKE ?1 ESCAPE '\\' COLLATE NOCASE \
                OR file_path LIKE ?1 ESCAPE '\\' COLLATE NOCASE \
             LIMIT 500"
        )?;
        // SQLite LIKE: % wildcard, _ single char. Escape both so a search
        // for `100%` doesn't become a wildcard.
        let escaped = needle_lower
            .replace('\\', r"\\")
            .replace('%', r"\%")
            .replace('_', r"\_");
        let pattern = format!("%{}%", escaped);

        let rows = stmt.query_map(rusqlite::params![pattern], |row| {
            let id: i64 = row.get(0)?;
            let file_path: String = row.get(1)?;
            let heading: String = row.get(2)?;
            let content: String = row.get(3)?;
            let byte_start: i64 = row.get(4)?;
            let byte_end: i64 = row.get(5)?;
            Ok((id as u64, file_path, heading, content, byte_start as usize, byte_end as usize))
        })?;

        let mut results: Vec<SearchResult> = Vec::new();
        for row in rows {
            let (id, file_path, heading, content, byte_start, byte_end) = row?;
            // Count occurrences for ranking. ASCII-fast lowercased compare.
            let count = count_occurrences_ci(&content, &needle_lower)
                + count_occurrences_ci(&heading, &needle_lower);
            if count == 0 {
                continue;
            }
            let chunk = Chunk {
                id,
                file_path: PathBuf::from(file_path),
                heading,
                content,
                byte_start,
                byte_end,
            };
            results.push(SearchResult {
                chunk,
                score: count as f32,
                source: HitSource::Literal,
                matched_terms: vec![needle.to_string()],
            });
        }

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(k);
        Ok(results)
    }

    pub fn save_index(&self, path: &Path) -> Result<()> {
        // usearch save on empty index can produce a malformed file that
        // crashes on next load. Only save if we have at least one vector.
        if self.hnsw.size() == 0 {
            return Ok(());
        }
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

/// File extensions we'll attempt to extract searchable text from. PDF
/// extraction needs the system `pdftotext` (poppler-utils) — if it's
/// missing we just skip that file with a warning. DOCX uses the `zip`
/// crate to read `word/document.xml` and strips tags inline.
fn is_indexable_ext(ext: &str) -> bool {
    matches!(ext, "md" | "markdown" | "mdx" | "pdf" | "docx" | "doc")
}

fn collect_indexable_files(root: &Path) -> Vec<PathBuf> {
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
                results.extend(collect_indexable_files(&path));
            } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if is_indexable_ext(&ext.to_lowercase()) {
                    results.push(path);
                }
            }
        }
    }
    results
}

/// Read a file's textual content for indexing. Markdown is read straight
/// off disk. DOCX has its `word/document.xml` unzipped and tag-stripped.
/// PDF is shelled out to `pdftotext`; if poppler isn't installed we
/// return an `Err` and the caller skips the file (the user can still
/// view PDFs; they just won't be searchable until pdftotext is on PATH).
fn read_indexable_text(path: &Path) -> Result<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "md" | "markdown" | "mdx" => Ok(fs::read_to_string(path)?),
        "docx" | "doc" => extract_docx_text(path),
        "pdf" => extract_pdf_text(path),
        _ => Err(format!("unsupported extension: {ext}").into()),
    }
}

fn extract_docx_text(path: &Path) -> Result<String> {
    let file = fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file)?;
    // word/document.xml carries the body text. Headers/footers/footnotes
    // each live in their own xml entry; we deliberately skip them — the
    // body is what users search for, and adding the rest just adds noise.
    let mut entry = zip.by_name("word/document.xml")
        .map_err(|e| format!("docx missing word/document.xml: {e}"))?;
    let mut xml = String::new();
    entry.read_to_string(&mut xml)?;
    Ok(strip_xml_tags(&xml))
}

/// Hand-rolled tag stripper. We can't use a full XML parser without
/// pulling in another crate; the OOXML schema is forgiving enough that
/// "drop everything between < and >, decode the common entities" gives a
/// faithful text dump for indexing purposes. Paragraph boundaries
/// (`</w:p>`) become newlines so our chunker can still find headings.
fn strip_xml_tags(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len() / 2);
    let mut in_tag = false;
    let mut tag_buf = String::new();
    for ch in xml.chars() {
        if ch == '<' {
            in_tag = true;
            tag_buf.clear();
        } else if ch == '>' {
            in_tag = false;
            // Paragraph close → newline. Tab character → tab.
            if tag_buf.starts_with("/w:p") || tag_buf.starts_with("w:br") {
                out.push('\n');
            } else if tag_buf.starts_with("w:tab") {
                out.push('\t');
            }
            tag_buf.clear();
        } else if in_tag {
            tag_buf.push(ch);
        } else {
            out.push(ch);
        }
    }
    decode_xml_entities(&out)
}

/// Single-pass entity decoder. Chained `.replace()` calls would each
/// allocate a fresh String — for large DOCX bodies that's 5× the text
/// in transient memory, which has been showing up as memory pressure
/// during boot reindex. This walks the buffer once and decodes the
/// five entities OOXML actually emits.
fn decode_xml_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            // Match against the known entities. None of them are longer
            // than 6 bytes (`&apos;`), so a tiny linear check is fine.
            if bytes[i..].starts_with(b"&amp;") {
                out.push('&');
                i += 5;
                continue;
            } else if bytes[i..].starts_with(b"&lt;") {
                out.push('<');
                i += 4;
                continue;
            } else if bytes[i..].starts_with(b"&gt;") {
                out.push('>');
                i += 4;
                continue;
            } else if bytes[i..].starts_with(b"&quot;") {
                out.push('"');
                i += 6;
                continue;
            } else if bytes[i..].starts_with(b"&apos;") {
                out.push('\'');
                i += 6;
                continue;
            }
        }
        // Push as a char to stay UTF-8 safe (we walk by char boundary
        // because str::char_indices is the safe way; here bytes[i] is
        // ASCII for the cases we care about, but for non-ASCII chars
        // we need the multi-byte sequence intact).
        let ch_start = i;
        // Find the next char boundary by skipping continuation bytes.
        i += 1;
        while i < bytes.len() && (bytes[i] & 0xC0) == 0x80 {
            i += 1;
        }
        out.push_str(&s[ch_start..i]);
    }
    out
}

fn extract_pdf_text(path: &Path) -> Result<String> {
    // `-layout` keeps reading order roughly stable across columns; `-` writes
    // to stdout. Anything that fails (binary missing, corrupt PDF, encrypted
    // file) bubbles up as Err so the caller can skip without crashing the
    // whole indexing run.
    let out = Command::new("pdftotext")
        .arg("-layout")
        .arg("-q")
        .arg(path)
        .arg("-")
        .output()
        .map_err(|e| format!("pdftotext not available: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "pdftotext exit {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Build a SearchResult list from BM25 hits only. Used when vectors are
/// unavailable and as a fallback when embedding the query fails.
fn rank_keyword_only(
    bm25_map: &HashMap<u64, (f32, Chunk)>,
    raw_terms: &[String],
    k: usize,
) -> Vec<SearchResult> {
    let mut scored: Vec<(u64, f32)> = bm25_map
        .iter()
        .map(|(&id, (score, _))| (id, *score))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(k);

    scored
        .into_iter()
        .filter_map(|(id, score)| {
            bm25_map.get(&id).map(|(_, chunk)| {
                let matched_terms = terms_present_in(&chunk.content, raw_terms);
                SearchResult {
                    chunk: chunk.clone(),
                    // Anchor at >=1.0 like the hybrid path so scores look
                    // consistent across modes.
                    score: 1.0 + score,
                    source: HitSource::Keyword,
                    matched_terms,
                }
            })
        })
        .collect()
}

/// Of `terms`, return the ones that appear (case-insensitive) anywhere in
/// `text`. Plain ASCII compare — Porter stems aren't available here, so a
/// search for "running" against text containing "ran" returns nothing.
/// That's fine for highlighting: better to highlight only literal matches
/// than to mislead the user about what matched.
fn terms_present_in(text: &str, terms: &[String]) -> Vec<String> {
    let lower = text.to_lowercase();
    terms
        .iter()
        .filter(|t| !t.is_empty() && lower.contains(t.as_str()))
        .cloned()
        .collect()
}

/// Count non-overlapping case-insensitive occurrences of `needle` in
/// `haystack`. Returns 0 if needle is empty.
fn count_occurrences_ci(haystack: &str, needle_lower: &str) -> usize {
    if needle_lower.is_empty() {
        return 0;
    }
    let hay_lower = haystack.to_lowercase();
    let mut n = 0;
    let mut start = 0;
    while let Some(idx) = hay_lower[start..].find(needle_lower) {
        n += 1;
        start += idx + needle_lower.len();
        if start >= hay_lower.len() {
            break;
        }
    }
    n
}

/// 1-based line number of `byte_pos` inside the file at `path`. Returns 1
/// on any read error so the caller can still navigate the user to the
/// top of the file rather than failing the whole hit.
pub fn line_for_chunk(path: &Path, byte_pos: usize) -> usize {
    line_number_for_byte(path, byte_pos)
}

/// Public wrapper so commands.rs can build a snippet without re-implementing
/// the windowing logic.
pub fn build_snippet_for_hit(
    content: &str,
    matched_terms: &[String],
    target_chars: usize,
) -> String {
    build_snippet(content, matched_terms, target_chars)
}

fn line_number_for_byte(path: &Path, byte_pos: usize) -> usize {
    let content = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return 1,
    };
    let clamped = byte_pos.min(content.len());
    1 + content[..clamped].bytes().filter(|&b| b == b'\n').count()
}

/// Build a snippet centred on the first occurrence of any matched term.
/// Falls back to the head of the chunk if no term is present (vector-only
/// hits). Returns plain markdown — the renderer is expected to still
/// understand bold/italic/code/wikilinks.
fn build_snippet(content: &str, matched_terms: &[String], target_chars: usize) -> String {
    let lower = content.to_lowercase();
    let mut earliest: Option<usize> = None;
    for term in matched_terms {
        if term.is_empty() {
            continue;
        }
        if let Some(idx) = lower.find(term.as_str()) {
            earliest = Some(match earliest {
                Some(e) => e.min(idx),
                None => idx,
            });
        }
    }

    let total_chars = content.chars().count();
    if total_chars <= target_chars {
        return content.trim().to_string();
    }

    let half = target_chars / 2;

    let (start_byte, prefix_ellipsis) = match earliest {
        // Vector-only / no match → take from the top.
        None => (0usize, false),
        Some(byte_idx) => {
            // Convert byte offset to char offset for symmetric windowing.
            let char_idx = content[..byte_idx].chars().count();
            if char_idx <= half {
                (0usize, false)
            } else {
                let target_char = char_idx - half;
                // Walk forward, tracking the most recent word/line boundary
                // so we can snap the start there instead of slicing mid-word.
                let mut last_boundary = 0usize;
                let mut ci = 0usize;
                for (bi, ch) in content.char_indices() {
                    if ch == '\n' || ch == ' ' || ch == '\t' {
                        last_boundary = bi + ch.len_utf8();
                    }
                    if ci >= target_char {
                        break;
                    }
                    ci += 1;
                    let _ = bi;
                }
                (last_boundary, true)
            }
        }
    };

    // Take up to target_chars chars starting from start_byte.
    let mut end_byte = content.len();
    let mut taken = 0usize;
    for (bi, _ch) in content[start_byte..].char_indices() {
        taken += 1;
        if taken > target_chars {
            end_byte = start_byte + bi;
            break;
        }
    }

    let mut out = String::new();
    if prefix_ellipsis {
        out.push('…');
    }
    out.push_str(content[start_byte..end_byte].trim());
    if end_byte < content.len() {
        out.push('…');
    }
    out
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
