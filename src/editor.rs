//! GPUI editor component: renders a Buffer with virtual scrolling,
//! cursor, selection, and markdown-aware styling.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::*;
use gpui_component::ActiveTheme;
use crate::buffer::Buffer;
use crate::markdown::{self, BlockType, LineInfo, SpanStyle};

// Max rendered dimensions for inline images.
const IMAGE_MAX_WIDTH: f32 = 620.0;
const IMAGE_MAX_HEIGHT: f32 = 420.0;
const IMAGE_PADDING: f32 = 12.0;

const LINE_HEIGHT: f32 = 22.0;
/// Lines per wheel tick. Lower = finer control / smoother feel.
const SCROLL_LINES: f32 = 1.5;
/// Extra scrollable space below the last line, for comfort when scrolling to
/// the end of a document.
const BOTTOM_PADDING: f32 = 200.0;

/// Emitted to the parent (ForgeApp) for side-effects it needs to handle.
#[derive(Clone, Debug)]
pub enum EditorEvent {
    /// User clicked a wikilink. Parent should resolve and open the target.
    OpenWikilink { target: String, heading: Option<String> },
}

/// Vault file entry used by the wikilink autocomplete.
#[derive(Clone, Debug)]
pub struct VaultFile {
    /// Basename without `.md`, used when inserting a wikilink target.
    pub basename: String,
    /// Path relative to the vault root, with `.md` stripped. For display.
    pub rel_path: String,
    pub abs_path: PathBuf,
}

actions!(
    editor,
    [
        MoveLeft, MoveRight, MoveUp, MoveDown,
        MoveWordLeft, MoveWordRight,
        MoveHome, MoveEnd,
        SelectLeft, SelectRight, SelectUp, SelectDown,
        SelectWordLeft, SelectWordRight,
        SelectHome, SelectEnd, SelectAll, SelectLine,
        Backspace, Delete, BackspaceWord, DeleteWord,
        Enter, Indent, Dedent,
        Copy, Cut, Paste,
        Undo, Redo,
        DuplicateLine,
        ToggleBold, ToggleItalic, ToggleCode, ToggleStrikethrough,
        ToggleReadMode,
        InsertHeading1, InsertHeading2, InsertHeading3,
        InsertBulletList, InsertNumberedList,
        InsertTable, InsertCodeBlock, InsertHorizontalRule,
        AutocompleteCancel,
        ZoomIn, ZoomOut, ZoomReset,
        PageUp, PageDown,
        MoveDocStart, MoveDocEnd,
    ]
);

/// Wikilink autocomplete popup state.
#[derive(Clone, Debug)]
pub struct AutocompleteState {
    /// Current query (text after `[[`, same-line, up to cursor).
    pub query: String,
    /// Byte position of the first `[` in the `[[` trigger.
    pub trigger_byte: usize,
    /// Filtered vault_files indices, sorted by match score.
    pub matches: Vec<usize>,
    /// Currently highlighted row in `matches`.
    pub selected: usize,
}

pub struct Editor {
    focus_handle: FocusHandle,
    pub buffer: Buffer,
    scroll_offset: f32,
    /// Target scroll position the editor is animating toward.
    scroll_target: f32,
    viewport_height: f32,
    is_selecting: bool,
    cursor_visible: bool,
    last_bounds: Option<Bounds<Pixels>>,
    last_items: Vec<RenderItem>,
    last_first_line: usize,
    /// Per content-line rendered height in pixels. Seeded to LINE_HEIGHT,
    /// updated as items are built in prepaint. Used for scroll math so that
    /// wrapped lines and tables aren't treated as single-row for total height.
    line_heights: Vec<f32>,
    /// Cached sum of line_heights (without BOTTOM_PADDING). Delta-updated to
    /// avoid an O(n) scan on every scroll event.
    line_heights_sum: f32,
    /// Prefix sums of line_heights (len = line_heights.len() + 1). Enables
    /// O(log n) `first_visible_line` and O(1) `line_y`. Rebuilt in prepaint.
    cumulative_heights: Vec<f32>,
    /// Per-line cache of expensive shaped content. Reused across scroll frames
    /// so we don't re-shape every visible line on each animation tick.
    line_cache: Vec<Option<LineCacheEntry>>,
    /// Per-table cache, keyed by table's first content line.
    table_cache: HashMap<usize, TableCacheEntry>,
    /// Cached markdown line info. Rebuilt when needs_reparse=true.
    parsed_lines: Vec<LineInfo>,
    /// Column widths for tables (per line, empty if not a table row)
    table_col_widths: Vec<Vec<usize>>,
    /// Kind of table row (per line)
    table_row_kinds: Vec<TableRowKind>,
    /// Font size for each table (per line, 0.0 = default, only set for table lines)
    table_font_sizes: Vec<f32>,
    needs_reparse: bool,
    pub read_mode: bool,
    /// Zoom multiplier for fonts + line height. 1.0 = default, range 0.6..=2.2.
    pub zoom: f32,
    /// Lowercased basenames (without `.md`) of every note in the vault.
    /// Used to color wikilinks whose target exists vs. is missing.
    pub known_notes: HashSet<String>,
    /// All vault files, for wikilink autocomplete.
    pub vault_files: Vec<VaultFile>,
    /// Active wikilink autocomplete state (None when no popup is showing).
    pub autocomplete: Option<AutocompleteState>,
    /// Font families from settings. Changed by ForgeApp.
    pub body_font_family: SharedString,
    pub mono_font_family: SharedString,
    /// Body font size from settings (before zoom).
    pub base_font_size: f32,
    /// Vault root, used to resolve image embed paths.
    pub vault_root: Option<PathBuf>,
    /// Cache of decoded images keyed by the embed target string.
    /// Value holds the RenderImage + native (width, height). `None` = load failed.
    image_cache: HashMap<String, Option<(Arc<RenderImage>, u32, u32)>>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TableRowKind {
    NotTable,
    Header,
    Separator,
    Data,
}

// ── Hybrid render items ──

#[derive(Clone)]
pub enum RenderItem {
    Line(RenderLine),
    Table(RenderTable),
    Image(RenderImageItem),
}

#[derive(Clone)]
pub struct RenderImageItem {
    pub content_line: usize,
    pub image: Arc<RenderImage>,
    pub y_origin: f32,
    pub render_width: f32,
    pub render_height: f32,
    /// Full layout height (render_height + padding).
    pub total_height: f32,
}

#[derive(Clone)]
pub struct RenderLine {
    pub content_line: usize,
    pub wrapped: WrappedLine,
    pub display: DisplayLine,
    pub y_origin: f32,
    pub height: f32,
}

#[derive(Clone)]
pub struct RenderTable {
    pub content_start: usize,
    pub content_end: usize,
    pub col_x: Vec<f32>,         // left x of each column
    pub col_widths: Vec<f32>,
    pub rows: Vec<RenderTableRow>,
    pub y_origin: f32,
    pub total_height: f32,
    pub header_end_y: Option<f32>,
}

#[derive(Clone)]
pub struct RenderTableRow {
    pub content_line: usize,
    pub cells: Vec<RenderCell>,
    pub kind: TableRowKind,
    pub y_in_table: f32,
    pub height: f32,
}

#[derive(Clone)]
pub struct RenderCell {
    pub lines: Vec<ShapedLine>,
    pub col: usize,
}

/// Cached shaping output for one content line. Reusable across scroll frames
/// as long as wrap_width + show_raw match.
#[derive(Clone)]
pub struct LineCacheEntry {
    pub wrap_width: f32,
    pub show_raw: bool,
    pub display: DisplayLine,
    pub wrapped: WrappedLine,
    pub height: f32,
}

/// Cached render for one table, keyed by its first content line.
#[derive(Clone)]
pub struct TableCacheEntry {
    pub wrap_width: f32,
    pub content_end: usize,
    pub table: RenderTable,
}

impl RenderItem {
    pub fn y_origin(&self) -> f32 {
        match self {
            RenderItem::Line(l) => l.y_origin,
            RenderItem::Table(t) => t.y_origin,
            RenderItem::Image(i) => i.y_origin,
        }
    }
    pub fn height(&self) -> f32 {
        match self {
            RenderItem::Line(l) => l.height,
            RenderItem::Table(t) => t.total_height,
            RenderItem::Image(i) => i.total_height,
        }
    }
    pub fn contains_content_line(&self, line: usize) -> bool {
        match self {
            RenderItem::Line(l) => l.content_line == line,
            RenderItem::Table(t) => line >= t.content_start && line < t.content_end,
            RenderItem::Image(i) => i.content_line == line,
        }
    }
}

impl Editor {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // Cursor blink timer disabled (steady cursor is cheaper -- no notifies).

        Self {
            focus_handle: cx.focus_handle(),
            buffer: Buffer::new(),
            scroll_offset: 0.0,
            scroll_target: 0.0,
            viewport_height: 800.0,
            is_selecting: false,
            cursor_visible: true,
            last_bounds: None,
            last_items: Vec::new(),
            last_first_line: 0,
            line_heights: Vec::new(),
            line_heights_sum: 0.0,
            cumulative_heights: Vec::new(),
            line_cache: Vec::new(),
            table_cache: HashMap::new(),
            parsed_lines: Vec::new(),
            table_col_widths: Vec::new(),
            table_row_kinds: Vec::new(),
            table_font_sizes: Vec::new(),
            needs_reparse: true,
            read_mode: false,
            zoom: 1.0,
            known_notes: HashSet::new(),
            vault_files: Vec::new(),
            autocomplete: None,
            vault_root: None,
            image_cache: HashMap::new(),
            body_font_family: "DejaVu Sans".into(),
            mono_font_family: "DejaVu Sans Mono".into(),
            base_font_size: 15.0,
        }
    }

    pub fn set_fonts(&mut self, body: &str, mono: &str, size: f32) {
        self.body_font_family = body.to_string().into();
        self.mono_font_family = mono.to_string().into();
        self.base_font_size = size;
    }

    /// Parent (ForgeApp) pushes the vault root so image embeds can resolve paths.
    pub fn set_vault_root(&mut self, root: Option<PathBuf>) {
        if self.vault_root != root {
            self.image_cache.clear();
            self.vault_root = root;
        }
    }

    /// Resolve an image embed target (e.g. `unnamed.jpg` or `assets/pic.png`)
    /// to an absolute path inside the vault. Returns None if not found.
    fn resolve_image_target(&self, target: &str) -> Option<PathBuf> {
        // Skip remote URLs.
        if target.starts_with("http://") || target.starts_with("https://") || target.starts_with("data:") {
            return None;
        }
        let p = Path::new(target);
        if p.is_absolute() && p.exists() { return Some(p.to_path_buf()); }
        let root = self.vault_root.as_ref()?;
        // Try relative to vault root first.
        let direct = root.join(target);
        if direct.exists() { return Some(direct); }
        // Basename search: walk vault for first matching file.
        let target_basename = p.file_name()?.to_str()?;
        find_file_by_basename(root, target_basename)
    }

    /// Load and decode an image by embed target. Cached so we decode once.
    /// Returns `(image, native_width, native_height)`.
    pub fn load_image(&mut self, target: &str) -> Option<(Arc<RenderImage>, u32, u32)> {
        if let Some(entry) = self.image_cache.get(target) {
            return entry.clone();
        }
        let decoded = self.resolve_image_target(target)
            .and_then(|path| decode_image(&path));
        self.image_cache.insert(target.to_string(), decoded.clone());
        decoded
    }

    /// Read-only cache lookup (doesn't decode). Returns None if not yet loaded.
    pub fn cached_image(&self, target: &str) -> Option<(Arc<RenderImage>, u32, u32)> {
        self.image_cache.get(target).and_then(|e| e.clone())
    }

    /// Eagerly decode every ImageEmbed's source referenced in the document,
    /// storing results in the cache. Called from prepaint before the main loop.
    pub fn preload_image_embeds(&mut self) {
        let n = self.parsed_lines.len();
        let mut targets: Vec<String> = Vec::new();
        for i in 0..n {
            if matches!(self.parsed_lines[i].block.block_type, BlockType::ImageEmbed) {
                let line_text = self.buffer.line_str(i);
                if let Some(t) = markdown::parse_image_embed(line_text.trim()) {
                    if !self.image_cache.contains_key(&t) { targets.push(t); }
                }
            }
        }
        for t in targets { let _ = self.load_image(&t); }
    }

    /// Parent (ForgeApp) pushes the set of known note basenames (lowercased, no `.md`)
    /// so we can color wikilinks differently when the target is missing.
    pub fn set_known_notes(&mut self, known: HashSet<String>) {
        self.known_notes = known;
    }

    /// Parent (ForgeApp) pushes the vault file list for autocomplete.
    pub fn set_vault_files(&mut self, files: Vec<VaultFile>) {
        self.vault_files = files;
    }

    pub fn set_content(&mut self, content: String) {
        self.buffer = Buffer::from_str(&content);
        self.scroll_offset = 0.0;
        self.scroll_target = 0.0;
        self.needs_reparse = true;
        self.autocomplete = None;
        // Don't rebuild caches here -- reparse (called in prepaint) will do it.
    }

    fn mark_dirty(&mut self) {
        self.needs_reparse = true;
    }

    /// Recompute autocomplete state based on current cursor position.
    /// Activates when cursor is immediately after a same-line `[[` with no
    /// closing `]]` between. Deactivates otherwise.
    pub fn update_autocomplete(&mut self) {
        let Some((trigger_byte, query)) = self.detect_wikilink_query() else {
            self.autocomplete = None;
            return;
        };
        // Preserve selection if query is unchanged; reset otherwise.
        let prev_selected = self.autocomplete.as_ref()
            .filter(|s| s.query == query && s.trigger_byte == trigger_byte)
            .map(|s| s.selected);
        let matches = self.filter_vault_files(&query);
        let selected = prev_selected.unwrap_or(0).min(matches.len().saturating_sub(1));
        self.autocomplete = Some(AutocompleteState { query, trigger_byte, matches, selected });
    }

    /// If cursor is in a `[[query` region, return (trigger_byte, query_string).
    fn detect_wikilink_query(&self) -> Option<(usize, String)> {
        let sel = self.buffer.selection();
        if !sel.is_empty() { return None; }
        let cursor = sel.head;
        let line = self.buffer.byte_to_line(cursor);
        let line_start = self.buffer.line_to_byte(line);
        let col = cursor - line_start;
        let line_text = self.buffer.line_str(line);
        let bytes = line_text.as_bytes();
        if col > bytes.len() { return None; }

        // Scan backwards from the cursor for the nearest `[[` on the same line.
        // Stop early if we hit `]]` or `[` without a matching second `[`.
        let mut i = col;
        while i >= 2 {
            if bytes[i - 1] == b']' { return None; }
            if bytes[i - 1] == b'[' && bytes[i - 2] == b'[' {
                // Found the trigger. The query is bytes [i, col).
                let trigger_byte = line_start + (i - 2);
                // Reject if the query contains `]` or a newline (newline won't happen here).
                let query = &line_text[i..col];
                if query.contains(']') || query.contains('\n') { return None; }
                return Some((trigger_byte, query.to_string()));
            }
            i -= 1;
        }
        None
    }

    /// Return indices into self.vault_files matching the query.
    /// Empty query returns all, capped. Otherwise substring match (case-insensitive).
    fn filter_vault_files(&self, query: &str) -> Vec<usize> {
        const MAX: usize = 50;
        let q = query.to_ascii_lowercase();
        if q.is_empty() {
            return (0..self.vault_files.len().min(MAX)).collect();
        }
        let mut scored: Vec<(i32, usize)> = Vec::new();
        for (idx, vf) in self.vault_files.iter().enumerate() {
            let basename_l = vf.basename.to_ascii_lowercase();
            let rel_l = vf.rel_path.to_ascii_lowercase();
            let score = if basename_l == q { 100 }
                else if basename_l.starts_with(&q) { 80 - (basename_l.len() as i32 - q.len() as i32).min(50) }
                else if basename_l.contains(&q) { 50 }
                else if rel_l.contains(&q) { 20 }
                else { -1 };
            if score >= 0 { scored.push((score, idx)); }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| {
            let an = self.vault_files[a.1].basename.to_ascii_lowercase();
            let bn = self.vault_files[b.1].basename.to_ascii_lowercase();
            an.cmp(&bn)
        }));
        scored.truncate(MAX);
        scored.into_iter().map(|(_, i)| i).collect()
    }

    /// Apply the currently selected autocomplete entry: replace `[[query` with
    /// `[[basename]]` and place the cursor past the closing `]]`.
    pub fn accept_autocomplete(&mut self) -> bool {
        let Some(state) = self.autocomplete.clone() else { return false; };
        let Some(&vf_idx) = state.matches.get(state.selected) else { return false; };
        let Some(vf) = self.vault_files.get(vf_idx) else { return false; };
        let basename = vf.basename.clone();
        // Replace bytes [trigger_byte, cursor) with "[[basename]]"
        let cursor = self.buffer.selection().head;
        self.buffer.set_selection(state.trigger_byte, cursor);
        let insertion = format!("[[{}]]", basename);
        self.buffer.insert(&insertion);
        self.autocomplete = None;
        self.needs_reparse = true;
        true
    }

    pub fn autocomplete_move(&mut self, delta: i32) -> bool {
        let Some(state) = self.autocomplete.as_mut() else { return false; };
        if state.matches.is_empty() { return true; }
        let n = state.matches.len() as i32;
        let s = state.selected as i32;
        let ns = ((s + delta) % n + n) % n;
        state.selected = ns as usize;
        true
    }

    pub fn cancel_autocomplete(&mut self) -> bool {
        if self.autocomplete.is_some() { self.autocomplete = None; true } else { false }
    }

    pub fn on_autocomplete_cancel(&mut self, _: &AutocompleteCancel, _: &mut Window, cx: &mut Context<Self>) {
        if self.cancel_autocomplete() { cx.notify(); }
    }

    fn apply_zoom(&mut self, new_zoom: f32, cx: &mut Context<Self>) {
        let z = new_zoom.clamp(0.6, 2.2);
        if (z - self.zoom).abs() < 0.001 { return; }
        self.zoom = z;
        // Invalidate everything: line heights + wrapping change when fonts scale.
        self.needs_reparse = true;
        self.line_cache.clear();
        self.table_cache.clear();
        cx.notify();
    }

    pub fn on_zoom_in(&mut self, _: &ZoomIn, _: &mut Window, cx: &mut Context<Self>) {
        self.apply_zoom(self.zoom * 1.1, cx);
    }
    pub fn on_zoom_out(&mut self, _: &ZoomOut, _: &mut Window, cx: &mut Context<Self>) {
        self.apply_zoom(self.zoom / 1.1, cx);
    }
    pub fn on_zoom_reset(&mut self, _: &ZoomReset, _: &mut Window, cx: &mut Context<Self>) {
        self.apply_zoom(1.0, cx);
    }

    /// Rebuild markdown line info from buffer content.
    fn reparse(&mut self) {
        let text = self.buffer.text();
        // Build line_starts from buffer
        let mut starts = Vec::with_capacity(self.buffer.len_lines());
        for i in 0..self.buffer.len_lines() {
            starts.push(self.buffer.line_to_byte(i));
        }
        self.parsed_lines = markdown::parse_lines(&text, &starts);
        self.compute_table_layouts();
        // Reset per-line heights; prepaint will refill the visible window.
        let n = self.buffer.len_lines();
        let lh = self.line_h();
        self.line_heights = vec![lh; n];
        self.line_heights_sum = lh * n as f32;
        self.rebuild_cumulative_heights();
        // Content changed -- all cached shapes are stale.
        self.line_cache = vec![None; n];
        self.table_cache.clear();
        self.needs_reparse = false;
    }

    /// Walk through parsed lines, find table blocks, compute column widths + row kinds + auto font size.
    fn compute_table_layouts(&mut self) {
        let n = self.parsed_lines.len();
        self.table_col_widths = vec![Vec::new(); n];
        self.table_row_kinds = vec![TableRowKind::NotTable; n];
        self.table_font_sizes = vec![0.0; n];
        let mut i = 0;
        while i < n {
            if matches!(self.parsed_lines[i].block.block_type, BlockType::Table) {
                let start = i;
                while i < n && matches!(self.parsed_lines[i].block.block_type, BlockType::Table) {
                    i += 1;
                }
                let end = i;
                // Compute max column widths across non-separator rows
                let mut col_widths: Vec<usize> = Vec::new();
                let mut found_header = false;
                for j in start..end {
                    let line = self.buffer.line_str(j);
                    let is_sep = is_table_separator(&line);
                    if is_sep {
                        self.table_row_kinds[j] = TableRowKind::Separator;
                    } else if !found_header {
                        self.table_row_kinds[j] = TableRowKind::Header;
                        found_header = true;
                    } else {
                        self.table_row_kinds[j] = TableRowKind::Data;
                    }
                    if is_sep { continue; }
                    let cells = parse_table_cells(&line);
                    for (k, cell) in cells.iter().enumerate() {
                        let w = cell.chars().count();
                        if k >= col_widths.len() {
                            col_widths.push(w);
                        } else if w > col_widths[k] {
                            col_widths[k] = w;
                        }
                    }
                }
                // Keep table font at same size as body text
                for j in start..end {
                    self.table_col_widths[j] = col_widths.clone();
                    self.table_font_sizes[j] = 14.0;
                }
            } else {
                i += 1;
            }
        }
    }

    fn reset_blink(&mut self) {
        self.cursor_visible = true;
        // Every cursor-affecting or editing action ends with reset_blink, so this
        // is the single place to refresh the wikilink autocomplete popup state.
        self.update_autocomplete();
    }

    /// Zoom-scaled line height.
    pub fn line_h(&self) -> f32 { LINE_HEIGHT * self.zoom }

    /// Sum of rendered heights across every content line. O(1) via cache.
    pub fn total_height(&self) -> f32 {
        self.line_heights_sum + BOTTOM_PADDING
    }

    /// Current scroll offset in pixels (where drawn, not target).
    pub fn scroll_offset(&self) -> f32 { self.scroll_offset }

    /// Last known viewport height of the editor bounds.
    pub fn viewport_height(&self) -> f32 { self.viewport_height }

    /// Rebuild the prefix-sum cache from line_heights. Called once per prepaint.
    fn rebuild_cumulative_heights(&mut self) {
        let n = self.line_heights.len();
        self.cumulative_heights.clear();
        self.cumulative_heights.reserve(n + 1);
        let mut acc = 0.0f32;
        self.cumulative_heights.push(0.0);
        for &h in &self.line_heights {
            acc += h;
            self.cumulative_heights.push(acc);
        }
        self.line_heights_sum = acc;
    }

    /// Content line index whose span contains `scroll_offset`. O(log n).
    fn first_visible_line(&self) -> usize {
        if self.cumulative_heights.is_empty() { return 0; }
        // cumulative_heights[i] = top-y of line i; cumulative_heights[i+1] = bottom-y.
        // Find the first i such that cumulative_heights[i+1] > scroll_offset.
        let target = self.scroll_offset;
        let cum = &self.cumulative_heights;
        // Binary search for target in cum[1..].
        let (mut lo, mut hi) = (0usize, self.line_heights.len());
        while lo < hi {
            let mid = (lo + hi) / 2;
            if cum[mid + 1] > target { hi = mid; } else { lo = mid + 1; }
        }
        lo.min(self.line_heights.len().saturating_sub(1))
    }

    /// Pixel y-origin of a content line in full-document coords. O(1).
    fn line_y(&self, line: usize) -> f32 {
        self.cumulative_heights.get(line).copied().unwrap_or(0.0)
    }

    fn scroll_by(&mut self, delta: f32, cx: &mut Context<Self>) {
        let max = (self.total_height() - self.viewport_height).max(0.0);
        self.scroll_target = (self.scroll_target + delta).clamp(0.0, max);
        cx.notify();
    }

    /// Jump to an absolute scroll position (used by scrollbar click/drag).
    pub fn set_scroll_target(&mut self, target: f32) {
        let max = (self.total_height() - self.viewport_height).max(0.0);
        self.scroll_target = target.clamp(0.0, max);
        // Snap immediately when user is dragging -- direct feedback is better
        // than easing for drag interactions.
        self.scroll_offset = self.scroll_target;
    }

    /// Advance scroll animation one step. Called from prepaint.
    /// Returns true if more animation frames are needed.
    fn tick_scroll_animation(&mut self) -> bool {
        let diff = self.scroll_target - self.scroll_offset;
        if diff.abs() < 0.5 {
            self.scroll_offset = self.scroll_target;
            return false;
        }
        // Lerp 28% per frame at display refresh (~60Hz) -> ~12 frames to converge = ~200ms.
        self.scroll_offset += diff * 0.28;
        true
    }

    fn ensure_cursor_visible(&mut self) {
        let (line, _) = self.buffer.cursor_line_col();
        let cursor_y = self.line_y(line);
        let cursor_h = self.line_heights.get(line).copied().unwrap_or(self.line_h());
        if cursor_y < self.scroll_target {
            // Cursor moved above viewport -- jump immediately (no animation for
            // keyboard-driven moves, feels more responsive).
            self.scroll_target = cursor_y;
            self.scroll_offset = cursor_y;
        } else if cursor_y + cursor_h > self.scroll_target + self.viewport_height {
            let new_target = cursor_y + cursor_h - self.viewport_height;
            self.scroll_target = new_target;
            self.scroll_offset = new_target;
        }
    }

    fn index_for_position(&self, position: Point<Pixels>) -> usize {
        let Some(bounds) = &self.last_bounds else { return 0; };
        let y_abs: f32 = (position.y - bounds.top()).into();
        let x_abs: f32 = (position.x - bounds.left()).into();

        // Find which item contains this y
        let item = self.last_items.iter().find(|it| {
            let y0 = it.y_origin();
            let h = it.height();
            y_abs >= y0 && y_abs < y0 + h
        }).or_else(|| self.last_items.last());
        let Some(item) = item else { return 0; };

        match item {
            RenderItem::Line(ln) => {
                let line = ln.content_line;
                let line_start = self.buffer.line_to_byte(line);
                let line_len = self.buffer.line_str(line).len();
                let local_y = y_abs - ln.y_origin;
                let pos = point(px(x_abs), px(local_y));
                let display_col = match ln.wrapped.closest_index_for_position(pos, px(self.line_h())) {
                    Ok(i) => i,
                    Err(i) => i,
                };
                let content_col = ln.display.display_to_content(display_col);
                line_start + content_col.min(line_len)
            }
            RenderItem::Table(t) => {
                // Find row by y within table
                let local_y = y_abs - t.y_origin;
                let row = t.rows.iter().find(|r| local_y >= r.y_in_table && local_y < r.y_in_table + r.height)
                    .or_else(|| t.rows.last());
                let Some(row) = row else { return self.buffer.line_to_byte(t.content_start); };
                // Snap to start of that line (cursor entering table activates it for editing)
                self.buffer.line_to_byte(row.content_line)
            }
            RenderItem::Image(img) => {
                // Click on image: snap cursor to that line (makes it editable).
                self.buffer.line_to_byte(img.content_line)
            }
        }
    }

    // ── Action handlers ──

    pub fn on_move_left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_left(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_move_right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_right(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_move_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        if self.autocomplete.is_some() { self.autocomplete_move(-1); cx.notify(); return; }
        self.buffer.move_up(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_move_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        if self.autocomplete.is_some() { self.autocomplete_move(1); cx.notify(); return; }
        self.buffer.move_down(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_move_word_left(&mut self, _: &MoveWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_word_left(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_move_word_right(&mut self, _: &MoveWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_word_right(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_move_home(&mut self, _: &MoveHome, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_home(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_move_end(&mut self, _: &MoveEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_end(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_page_up(&mut self, _: &PageUp, _: &mut Window, cx: &mut Context<Self>) {
        let lines = ((self.viewport_height / self.line_h().max(1.0)) as usize).max(1).saturating_sub(2);
        for _ in 0..lines.max(1) { self.buffer.move_up(); }
        self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_page_down(&mut self, _: &PageDown, _: &mut Window, cx: &mut Context<Self>) {
        let lines = ((self.viewport_height / self.line_h().max(1.0)) as usize).max(1).saturating_sub(2);
        for _ in 0..lines.max(1) { self.buffer.move_down(); }
        self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_move_doc_start(&mut self, _: &MoveDocStart, _: &mut Window, cx: &mut Context<Self>) {
        // Move to byte 0 using the selection API: set head+anchor to 0.
        let sel = self.buffer.selection();
        let new_head = 0;
        let _ = sel;
        // Reach byte 0 by doing multiple move_up's is ugly; use a dedicated API.
        // Use move_home + then step up until line 0, or use rope directly.
        while self.buffer.cursor_line_col().0 > 0 { self.buffer.move_up(); }
        self.buffer.move_home();
        let _ = new_head;
        self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_move_doc_end(&mut self, _: &MoveDocEnd, _: &mut Window, cx: &mut Context<Self>) {
        let total = self.buffer.len_lines();
        while self.buffer.cursor_line_col().0 + 1 < total { self.buffer.move_down(); }
        self.buffer.move_end();
        self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.select_left(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.select_right(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.select_up(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.select_down(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.select_word_left(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.select_word_right(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_select_home(&mut self, _: &SelectHome, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.select_home(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_select_end(&mut self, _: &SelectEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.select_end(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.select_all(); self.reset_blink(); cx.notify();
    }
    pub fn on_select_line(&mut self, _: &SelectLine, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.select_line_at(self.buffer.cursor());
        self.reset_blink(); cx.notify();
    }
    pub fn on_backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if self.read_mode { return; }
        self.buffer.backspace(); self.mark_dirty(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.delete(); self.mark_dirty(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_backspace_word(&mut self, _: &BackspaceWord, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.backspace_word(); self.mark_dirty(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_delete_word(&mut self, _: &DeleteWord, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.delete_word(); self.mark_dirty(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_enter(&mut self, _: &Enter, _: &mut Window, cx: &mut Context<Self>) {
        if self.autocomplete.is_some() {
            if self.accept_autocomplete() {
                self.mark_dirty(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
                return;
            }
        }
        self.buffer.enter(); self.mark_dirty(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_indent(&mut self, _: &Indent, _: &mut Window, cx: &mut Context<Self>) {
        if self.autocomplete.is_some() {
            if self.accept_autocomplete() {
                self.mark_dirty(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
                return;
            }
        }
        self.buffer.indent(); self.mark_dirty(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_dedent(&mut self, _: &Dedent, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.dedent(); self.mark_dirty(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.buffer.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }
    pub fn on_cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.buffer.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            self.buffer.insert("");
            self.mark_dirty(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
        }
    }
    pub fn on_paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
            self.buffer.insert(&text);
            self.mark_dirty(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
        }
    }
    pub fn on_undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.undo(); self.mark_dirty(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.redo(); self.mark_dirty(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_duplicate_line(&mut self, _: &DuplicateLine, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.duplicate_line(); self.mark_dirty(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_toggle_bold(&mut self, _: &ToggleBold, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.toggle_wrap("**"); self.mark_dirty(); self.reset_blink(); cx.notify();
    }
    pub fn on_toggle_italic(&mut self, _: &ToggleItalic, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.toggle_wrap("*"); self.mark_dirty(); self.reset_blink(); cx.notify();
    }
    pub fn on_toggle_code(&mut self, _: &ToggleCode, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.toggle_wrap("`"); self.mark_dirty(); self.reset_blink(); cx.notify();
    }
    pub fn on_toggle_strikethrough(&mut self, _: &ToggleStrikethrough, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.toggle_wrap("~~"); self.mark_dirty(); self.reset_blink(); cx.notify();
    }
    pub fn on_insert_h1(&mut self, _: &InsertHeading1, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.insert_at_line_start("# "); self.mark_dirty(); self.reset_blink(); cx.notify();
    }
    pub fn on_insert_h2(&mut self, _: &InsertHeading2, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.insert_at_line_start("## "); self.mark_dirty(); self.reset_blink(); cx.notify();
    }
    pub fn on_insert_h3(&mut self, _: &InsertHeading3, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.insert_at_line_start("### "); self.mark_dirty(); self.reset_blink(); cx.notify();
    }
    pub fn on_insert_bullet(&mut self, _: &InsertBulletList, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.insert_at_line_start("- "); self.mark_dirty(); self.reset_blink(); cx.notify();
    }
    pub fn on_insert_numbered(&mut self, _: &InsertNumberedList, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.insert_at_line_start("1. "); self.mark_dirty(); self.reset_blink(); cx.notify();
    }
    pub fn on_insert_table(&mut self, _: &InsertTable, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.insert("| Column 1 | Column 2 | Column 3 |\n| --- | --- | --- |\n| Cell | Cell | Cell |\n");
        self.mark_dirty(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_insert_code_block(&mut self, _: &InsertCodeBlock, _: &mut Window, cx: &mut Context<Self>) {
        let sel = self.buffer.selection().range();
        if sel.is_empty() {
            self.buffer.insert("```\n\n```");
            let c = self.buffer.cursor();
            self.buffer.set_cursor(c - 4);
        } else {
            let text = self.buffer.selected_text().unwrap_or_default();
            self.buffer.insert(&format!("```\n{}\n```", text));
        }
        self.mark_dirty(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }
    pub fn on_toggle_read_mode(&mut self, _: &ToggleReadMode, _: &mut Window, cx: &mut Context<Self>) {
        self.read_mode = !self.read_mode;
        cx.notify();
    }

    pub fn on_insert_hr(&mut self, _: &InsertHorizontalRule, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.insert("\n---\n"); self.mark_dirty(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }

    // ── Mouse handlers ──

    pub fn on_mouse_down(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.button != MouseButton::Left { return; }
        // Wikilink hit test first: single click on a rendered wikilink opens it
        // (instead of moving the cursor into the raw syntax).
        let mod_click = event.modifiers.control || event.modifiers.platform;
        if event.click_count == 1 && !event.modifiers.shift {
            if let Some((target, heading)) = self.hit_test_wikilink(event.position, mod_click) {
                cx.emit(EditorEvent::OpenWikilink { target, heading });
                return;
            }
        }
        self.is_selecting = true;
        let pos = self.index_for_position(event.position);
        match event.click_count {
            2 => self.buffer.select_word_at(pos),
            3 => self.buffer.select_line_at(pos),
            _ => {
                if event.modifiers.shift {
                    self.buffer.select_to(pos);
                } else {
                    self.buffer.set_cursor(pos);
                }
            }
        }
        self.reset_blink(); cx.notify();
    }

    /// If the click position falls inside a wikilink's visible content range
    /// (on a stripped line) or anywhere inside a wikilink (on a raw line, with
    /// modifier held), return its target + heading.
    fn hit_test_wikilink(&self, position: Point<Pixels>, force_open: bool) -> Option<(String, Option<String>)> {
        // First check: did the click hit a Table item? Tables aren't covered by
        // the `index_for_position` byte-granular hit-test, so handle them explicitly.
        if let Some((target, heading)) = self.hit_test_wikilink_in_table(position) {
            return Some((target, heading));
        }

        let pos = self.index_for_position(position);
        let line = self.buffer.byte_to_line(pos);
        let line_start = self.buffer.line_to_byte(line);
        let col = pos - line_start;
        let info = self.parsed_lines.get(line)?;
        if info.wikilinks.is_empty() { return None; }

        // Determine whether this line is currently showing raw (active block).
        let (active_start, active_end) = if self.read_mode || self.parsed_lines.is_empty() {
            (usize::MAX, 0)
        } else {
            let (cursor_line, _) = self.buffer.cursor_line_col();
            let n = self.parsed_lines.len();
            markdown::block_range(
                n,
                cursor_line,
                |i| self.buffer.line_str(i).trim().is_empty(),
                |i| self.parsed_lines.get(i).map(|l| l.block.block_type),
            )
        };
        let is_active = line >= active_start && line <= active_end;
        if is_active && !force_open { return None; }

        let line_text = self.buffer.line_str(line);
        for wl in &info.wikilinks {
            let hit = if is_active {
                col >= wl.range.start && col < wl.range.end
            } else {
                let (vs, ve) = wikilink_visible_content_range(&line_text, wl);
                col >= vs && col < ve
            };
            if hit {
                return Some((wl.target.clone(), wl.heading.clone()));
            }
        }
        None
    }

    /// Check if a click inside a Table item lands on a cell containing a wikilink.
    fn hit_test_wikilink_in_table(&self, position: Point<Pixels>) -> Option<(String, Option<String>)> {
        let bounds = self.last_bounds.as_ref()?;
        let y_abs: f32 = (position.y - bounds.top()).into();
        let x_abs: f32 = (position.x - bounds.left()).into();
        // Find the Table item under the cursor.
        let table = self.last_items.iter().find_map(|it| {
            if let RenderItem::Table(t) = it {
                let y0 = t.y_origin;
                if y_abs >= y0 && y_abs < y0 + t.total_height { return Some(t); }
            }
            None
        })?;
        // Find the row by local y.
        let local_y = y_abs - table.y_origin;
        let row = table.rows.iter().find(|r| local_y >= r.y_in_table && local_y < r.y_in_table + r.height)?;
        // Find the column by x.
        let mut col_idx: Option<usize> = None;
        for (i, &col_x) in table.col_x.iter().enumerate() {
            let col_w = table.col_widths.get(i).copied().unwrap_or(0.);
            if x_abs >= col_x && x_abs < col_x + col_w { col_idx = Some(i); break; }
        }
        let col_idx = col_idx?;
        // Get the cell's byte range within the row's line.
        let line_text = self.buffer.line_str(row.content_line);
        let cells = parse_table_cells_with_positions(&line_text);
        let (_, cell_start, cell_end) = cells.get(col_idx)?;
        // Wikilinks overlapping this cell.
        let info = self.parsed_lines.get(row.content_line)?;
        let in_cell: Vec<_> = info.wikilinks.iter()
            .filter(|w| w.range.start >= *cell_start && w.range.end <= *cell_end)
            .collect();
        if in_cell.is_empty() { return None; }
        // If one wikilink, return it. Otherwise, approximate by x: assume mono
        // character width, find which wikilink's visible range contains the click.
        if in_cell.len() == 1 {
            let w = in_cell[0];
            return Some((w.target.clone(), w.heading.clone()));
        }
        let mono_px = 8.4 * self.zoom;
        let col_x = table.col_x[col_idx] + 12.0; // cell padding
        let rel_x = (x_abs - col_x).max(0.0);
        let click_char_in_cell = (rel_x / mono_px) as usize;
        // Rough absolute byte offset in the cell's text.
        let click_byte_in_line = cell_start + click_char_in_cell;
        for w in &in_cell {
            let (vs, ve) = wikilink_visible_content_range(&line_text, w);
            if click_byte_in_line >= vs && click_byte_in_line < ve {
                return Some((w.target.clone(), w.heading.clone()));
            }
        }
        // Fallback: first wikilink in the cell.
        let w = in_cell[0];
        Some((w.target.clone(), w.heading.clone()))
    }

    pub fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    pub fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            let pos = self.index_for_position(event.position);
            self.buffer.select_to(pos);
            cx.notify();
        }
    }
}

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for Editor {
    fn text_for_range(&mut self, range_utf16: std::ops::Range<usize>, actual_range: &mut Option<std::ops::Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        actual_range.replace(range_utf16.clone());
        let text = self.buffer.text();
        Some(text.get(range_utf16.clone())?.to_string())
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        let sel = self.buffer.selection();
        Some(UTF16Selection { range: sel.start()..sel.end(), reversed: sel.head < sel.anchor })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<std::ops::Range<usize>> {
        None
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {}

    fn replace_text_in_range(&mut self, range: Option<std::ops::Range<usize>>, text: &str, _: &mut Window, cx: &mut Context<Self>) {
        if self.read_mode { return; }
        if let Some(r) = range {
            self.buffer.set_selection(r.start, r.end);
        }
        self.buffer.insert(text);
        self.mark_dirty(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }

    fn replace_and_mark_text_in_range(&mut self, range: Option<std::ops::Range<usize>>, text: &str, _: Option<std::ops::Range<usize>>, _: &mut Window, cx: &mut Context<Self>) {
        if self.read_mode { return; }
        if let Some(r) = range {
            self.buffer.set_selection(r.start, r.end);
        }
        self.buffer.insert(text);
        self.mark_dirty(); self.reset_blink(); self.ensure_cursor_visible(); cx.notify();
    }

    fn bounds_for_range(&mut self, range: std::ops::Range<usize>, bounds: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        let line = self.buffer.byte_to_line(range.start);
        let line_start = self.buffer.line_to_byte(line);
        let col = range.start - line_start;
        // Find the Line item containing this content line
        for it in &self.last_items {
            if let RenderItem::Line(ln) = it {
                if ln.content_line == line {
                    let display_col = ln.display.content_to_display(col);
                    let line_h = self.line_h();
                    let pos = ln.wrapped.position_for_index(display_col, px(line_h))?;
                    let y = bounds.top() + px(ln.y_origin) + pos.y;
                    return Some(Bounds::from_corners(
                        point(bounds.left() + pos.x, y),
                        point(bounds.left() + pos.x + px(8.), y + px(line_h)),
                    ));
                }
            }
        }
        None
    }

    fn character_index_for_point(&mut self, _: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        None
    }
}

// ── Display line: text + runs + display/content mapping ──

#[derive(Clone)]
pub struct DisplayLine {
    pub display_text: String,
    pub runs: Vec<TextRun>,
    /// display_to_content[display_byte] = content_byte
    /// Length = display_text.len() + 1
    pub display_to_content: Vec<usize>,
    /// Font size for this line
    pub font_size: f32,
}

impl DisplayLine {
    /// Map content byte offset to display byte offset.
    pub fn content_to_display(&self, content_byte: usize) -> usize {
        for (d, &c) in self.display_to_content.iter().enumerate() {
            if c >= content_byte { return d; }
        }
        self.display_text.len()
    }

    /// Map display byte offset to content byte offset.
    pub fn display_to_content(&self, display_byte: usize) -> usize {
        self.display_to_content.get(display_byte).copied()
            .unwrap_or_else(|| self.display_to_content.last().copied().unwrap_or(0))
    }
}

/// Compute the content-byte range that stays visible for a wikilink after
/// stripping the `[[ ]]` markers and (if present) the `target|` prefix.
/// Returns (visible_start, visible_end) in content bytes.
fn wikilink_visible_content_range(line: &str, wl: &markdown::WikiLink) -> (usize, usize) {
    let inner_start = wl.range.start + 2;
    let inner_end = wl.range.end.saturating_sub(2);
    if inner_end <= inner_start { return (inner_start, inner_end); }
    let bytes = line.as_bytes();
    // Search within the inner content for the first `|`.
    let mut pipe: Option<usize> = None;
    for i in inner_start..inner_end {
        if bytes[i] == b'|' { pipe = Some(i); break; }
    }
    let start = pipe.map(|p| p + 1).unwrap_or(inner_start);
    (start, inner_end)
}

/// Override runs in the display-byte range [start, end) with wikilink styling.
/// Splits runs at boundaries as needed, inserts a single run for the styled range.
fn apply_wikilink_style(runs: &mut Vec<TextRun>, start: usize, end: usize, base_font: &Font, color: Hsla) {
    if start >= end { return; }
    let mut out: Vec<TextRun> = Vec::with_capacity(runs.len() + 2);
    let mut pos = 0usize;
    for run in runs.drain(..) {
        let run_len = run.len;
        let run_end = pos + run_len;
        if run_end <= start || pos >= end {
            out.push(run);
        } else {
            if pos < start {
                let mut r = run.clone();
                r.len = start - pos;
                out.push(r);
            }
            let styled_start = pos.max(start);
            let styled_end = run_end.min(end);
            let styled_len = styled_end - styled_start;
            if styled_len > 0 {
                out.push(TextRun {
                    len: styled_len,
                    font: base_font.clone(),
                    color,
                    background_color: None,
                    underline: Some(UnderlineStyle { thickness: px(1.), color: None, wavy: false }),
                    strikethrough: None,
                });
            }
            if run_end > end {
                let mut r = run.clone();
                r.len = run_end - end;
                out.push(r);
            }
        }
        pos = run_end;
    }
    *runs = out;
}

/// Build a DisplayLine from a content line and its markdown info.
/// If `show_raw` is true, displays all markers. If false, strips them.
fn build_display_line(
    line: &str,
    info: Option<&LineInfo>,
    show_raw: bool,
    table_col_widths: &[usize],
    table_row_kind: TableRowKind,
    table_font_size: f32,
    base_font: &Font,
    fg: Hsla,
    muted: Hsla,
    code_bg: Hsla,
    link_color: Hsla,
    known_notes: &HashSet<String>,
    zoom: f32,
) -> DisplayLine {
    // Determine font size + heading flag from block type
    let (font_size, heading_bold) = match info.map(|i| i.block.block_type) {
        Some(BlockType::Heading(1)) => (26.0 * zoom, true),
        Some(BlockType::Heading(2)) => (22.0 * zoom, true),
        Some(BlockType::Heading(3)) => (19.0 * zoom, true),
        Some(BlockType::Heading(4)) => (17.0 * zoom, true),
        Some(BlockType::Heading(_)) => (15.0 * zoom, true),
        _ => (15.0 * zoom, false),
    };
    let is_code_block = matches!(
        info.map(|i| i.block.block_type),
        Some(BlockType::CodeBlock) | Some(BlockType::MathBlock)
    );
    let is_table = matches!(info.map(|i| i.block.block_type), Some(BlockType::Table));
    let is_image_embed = matches!(info.map(|i| i.block.block_type), Some(BlockType::ImageEmbed));

    if line.is_empty() {
        return DisplayLine {
            display_text: " ".into(),
            runs: vec![TextRun {
                len: 1, font: base_font.clone(), color: fg,
                background_color: None, underline: None, strikethrough: None,
            }],
            display_to_content: vec![0, 0],
            font_size,
        };
    }

    // Table line: monospace font with dim pipes + column alignment
    if is_table {
        // When active, show raw. When stripped, pad cells for alignment.
        let (display_text, display_to_content) = if show_raw || table_col_widths.is_empty() {
            (line.to_string(), (0..=line.len()).collect())
        } else {
            let is_sep = table_row_kind == TableRowKind::Separator;
            pad_table_line(line, table_col_widths, is_sep)
        };
        let is_header = !show_raw && table_row_kind == TableRowKind::Header;
        let is_sep_display = !show_raw && table_row_kind == TableRowKind::Separator;
        let runs = build_table_runs(&display_text, fg, muted, is_header, is_sep_display);
        let fs = if table_font_size > 0.0 { table_font_size * zoom } else { 14.0 * zoom };
        return DisplayLine {
            display_text,
            runs,
            display_to_content,
            font_size: fs,
        };
    }

    if show_raw {
        // Raw mode: display == content, markers dimmed
        let mut runs = build_text_runs_raw(line, info, base_font, fg, muted, code_bg, link_color, heading_bold, is_code_block);
        // Apply wikilink styling to the visible part of each wikilink.
        if let Some(info) = info {
            for wl in &info.wikilinks {
                let (vs, ve) = wikilink_visible_content_range(line, wl);
                let exists = known_notes.contains(&wl.target.to_ascii_lowercase());
                let color = if exists { link_color } else { muted };
                apply_wikilink_style(&mut runs, vs, ve, base_font, color);
            }
        }
        let display_to_content: Vec<usize> = (0..=line.len()).collect();
        return DisplayLine {
            display_text: line.to_string(),
            runs,
            display_to_content,
            font_size,
        };
    }


    // Image embed: show a placeholder with the filename (non-active lines only).
    if is_image_embed && !show_raw {
        let embed_target = markdown::parse_image_embed(line.trim()).unwrap_or_else(|| "image".to_string());
        let filename = std::path::Path::new(&embed_target)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&embed_target)
            .to_string();
        let display = format!("  🖼  {}  ", filename);
        let mono_font = Font {
            family: "DejaVu Sans Mono".into(),
            features: FontFeatures::default(),
            fallbacks: Some(FontFallbacks::from_fonts(vec![
                "Menlo".into(), "Monaco".into(), "Consolas".into(),
                "Liberation Mono".into(), "monospace".into(),
            ])),
            weight: FontWeight::NORMAL,
            style: FontStyle::Italic,
        };
        let runs = vec![TextRun {
            len: display.len(),
            font: mono_font,
            color: muted,
            background_color: Some(code_bg),
            underline: None,
            strikethrough: None,
        }];
        // Map everything in display text back to line start.
        let mut d2c: Vec<usize> = (0..=display.len()).map(|_| 0).collect();
        if let Some(last) = d2c.last_mut() { *last = line.len(); }
        return DisplayLine {
            display_text: display,
            runs,
            display_to_content: d2c,
            font_size: 14.0 * zoom,
        };
    }

    // Horizontal rule: render as a line of box-drawing dashes
    if matches!(info.map(|i| i.block.block_type), Some(BlockType::HorizontalRule)) {
        // HR is painted as a full-width divider quad in paint(); here we just
        // reserve a single blank line for it so layout math stays correct.
        return DisplayLine {
            display_text: " ".into(),
            runs: vec![TextRun {
                len: 1, font: base_font.clone(), color: fg,
                background_color: None, underline: None, strikethrough: None,
            }],
            display_to_content: vec![0, 0],
            font_size,
        };
    }

    // Stripped mode: hide markers
    // 1. Compute byte ranges to hide
    let prefix_len = info.map(|i| i.block.prefix_len).unwrap_or(0).min(line.len());
    let is_list_item = matches!(info.map(|i| i.block.block_type), Some(BlockType::ListItem));

    // List items: detect bullet vs task list, set replacement + extra hide
    let (list_replacement, list_extra_hide) = if is_list_item && prefix_len > 0 {
        let indent: String = line.bytes().take_while(|b| *b == b' ' || *b == b'\t').map(|b| b as char).collect();
        let marker_area = &line[indent.len()..];
        // Task list detection: bullet marker followed by "[ ] " or "[x] "
        let is_bullet_marker = marker_area.starts_with("- ") || marker_area.starts_with("* ") || marker_area.starts_with("+ ");
        let after_prefix = &line[prefix_len..];
        if is_bullet_marker && after_prefix.starts_with("[x] ") || after_prefix.starts_with("[X] ") {
            (format!("{}☑  ", indent), 4)
        } else if is_bullet_marker && after_prefix.starts_with("[ ] ") {
            (format!("{}☐  ", indent), 4)
        } else if is_bullet_marker {
            (format!("{}•  ", indent), 0)
        } else {
            // Numbered list: keep as-is (will be shown by not hiding the prefix)
            (String::new(), 0)
        }
    } else {
        (String::new(), 0)
    };

    // For numbered list items we want to KEEP the marker ("1. ", "2. ") visible
    // in stripped mode. is_list_item with no replacement and no task checkbox
    // means it's a numbered list -- skip hiding its prefix.
    let is_numbered_list = is_list_item && list_replacement.is_empty();
    let mut hide_ranges: Vec<std::ops::Range<usize>> = Vec::new();
    if !list_replacement.is_empty() {
        // Hide original prefix + checkbox syntax
        hide_ranges.push(0..prefix_len + list_extra_hide);
    } else if prefix_len > 0 && !is_numbered_list {
        hide_ranges.push(0..prefix_len);
    }
    if let Some(info) = info {
        for span in &info.spans {
            let (pre, suf) = markdown::marker_lens(span.style);
            let s = span.range.start;
            let e = span.range.end.min(line.len());
            if pre > 0 && s + pre <= e { hide_ranges.push(s..s + pre); }
            if suf > 0 && e >= suf && e.saturating_sub(suf) >= s + pre.min(e - s) {
                hide_ranges.push((e - suf)..e);
            }
        }
        // Hide wikilink brackets and (if aliased) the target|pipe prefix.
        for wl in &info.wikilinks {
            let (vs, ve) = wikilink_visible_content_range(line, wl);
            // Everything from [[ up to but not including the visible part.
            if wl.range.start < vs { hide_ranges.push(wl.range.start..vs); }
            // The trailing ]] and anything after the visible part before ]].
            let trailing_start = ve;
            let trailing_end = wl.range.end.min(line.len());
            if trailing_start < trailing_end { hide_ranges.push(trailing_start..trailing_end); }
        }
    }
    hide_ranges.sort_by_key(|r| r.start);

    // 2. Build display_text by copying line bytes except hidden ranges
    let mut display_text = String::new();
    let mut display_to_content: Vec<usize> = Vec::new();
    // Prepend list replacement if present
    if !list_replacement.is_empty() {
        for _ in 0..list_replacement.len() { display_to_content.push(0); }
        display_text.push_str(&list_replacement);
    }
    let mut pos = 0;
    let mut hr_idx = 0;
    while pos < line.len() {
        // Skip hide ranges that end before pos
        while hr_idx < hide_ranges.len() && hide_ranges[hr_idx].end <= pos {
            hr_idx += 1;
        }
        // If pos is inside a hide range, jump to its end
        if hr_idx < hide_ranges.len() && hide_ranges[hr_idx].start <= pos && pos < hide_ranges[hr_idx].end {
            pos = hide_ranges[hr_idx].end;
            continue;
        }
        // Take one character
        if let Some(c) = line[pos..].chars().next() {
            let char_len = c.len_utf8();
            for i in 0..char_len {
                display_to_content.push(pos + i);
            }
            display_text.push(c);
            pos += char_len;
        } else {
            break;
        }
    }
    display_to_content.push(line.len());

    // 3. Build runs for display_text
    let mut runs = build_text_runs_display(&display_text, &display_to_content, info, base_font, fg, muted, code_bg, link_color, heading_bold, is_code_block);

    // 4. Apply wikilink styling: for each wikilink, find the visible content range and map
    //    it to display bytes using content_to_display, then override the runs in that range.
    if let Some(info) = info {
        if !info.wikilinks.is_empty() {
            // Build O(1) content->display byte map in a single forward pass.
            // ctd[c] = smallest d such that display_to_content[d] >= c (matches the
            // previous per-call O(n) linear scan).
            let line_len = line.len();
            let mut ctd = vec![display_text.len(); line_len + 1];
            let mut prev_c_plus_1: usize = 0;
            for (d, &c) in display_to_content.iter().enumerate() {
                if c >= prev_c_plus_1 {
                    let end = c.min(line_len);
                    for slot in prev_c_plus_1..=end {
                        ctd[slot] = d;
                    }
                    prev_c_plus_1 = end + 1;
                    if prev_c_plus_1 > line_len { break; }
                }
            }
            for wl in &info.wikilinks {
                let (vs, ve) = wikilink_visible_content_range(line, wl);
                let d_start = ctd.get(vs).copied().unwrap_or(display_text.len());
                let d_end = ctd.get(ve).copied().unwrap_or(display_text.len());
                let exists = known_notes.contains(&wl.target.to_ascii_lowercase());
                let color = if exists { link_color } else { muted };
                apply_wikilink_style(&mut runs, d_start, d_end, base_font, color);
            }
        }
    }

    DisplayLine { display_text, runs, display_to_content, font_size }
}

/// Split a table row into cells.
/// Returns cells (trimmed content between pipes).
pub fn parse_table_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let without_edges = trimmed.trim_start_matches('|').trim_end_matches('|');
    without_edges.split('|').map(|s| s.trim().to_string()).collect()
}

/// Check if a table row is a separator row (like `| --- | --- |`).
fn is_table_separator(line: &str) -> bool {
    let cells = parse_table_cells(line);
    !cells.is_empty() && cells.iter().all(|c| {
        let chars: Vec<char> = c.chars().filter(|ch| !ch.is_whitespace()).collect();
        !chars.is_empty() && chars.iter().all(|ch| *ch == '-' || *ch == ':')
    })
}

/// Build a padded display version of a table line with aligned columns.
/// Pipes are replaced with spaces (hidden), cells aligned with monospace padding.
/// Returns (display_text, display_to_content).
fn pad_table_line(line: &str, col_widths: &[usize], is_separator: bool) -> (String, Vec<usize>) {
    if col_widths.is_empty() || line.trim().is_empty() {
        return (line.to_string(), (0..=line.len()).collect());
    }

    // Separator row: thin horizontal line matching total table width
    if is_separator {
        let total_w: usize = col_widths.iter().sum::<usize>() + 3 * (col_widths.len().saturating_sub(1)) + 4;
        let display = "─".repeat(total_w);
        let d2c: Vec<usize> = (0..=display.len()).map(|_| line.len()).collect();
        return (display, d2c);
    }

    let cells_with_pos = parse_table_cells_with_positions(line);
    if cells_with_pos.is_empty() {
        return (line.to_string(), (0..=line.len()).collect());
    }

    // Build display: cells padded to width, separated by 3 spaces (no pipes visible)
    let mut display = String::from("  ");
    let mut d2c: Vec<usize> = Vec::new();
    for _ in 0..display.len() { d2c.push(0); }

    for (i, (cell, cell_start, _cell_end)) in cells_with_pos.iter().enumerate() {
        if i > 0 {
            // 3-space separator between cells
            let sep = "   ";
            let sep_pos = *cell_start;
            for _ in 0..sep.len() { d2c.push(sep_pos); }
            display.push_str(sep);
        }
        let cell_bytes_start = *cell_start;
        for (bi, _) in cell.char_indices() {
            d2c.push(cell_bytes_start + bi);
        }
        display.push_str(cell);
        // Pad to column width
        let cell_char_count = cell.chars().count();
        let target_w = col_widths.get(i).copied().unwrap_or(cell_char_count);
        if cell_char_count < target_w {
            let pad = target_w - cell_char_count;
            let pad_pos = cell_bytes_start + cell.len();
            for _ in 0..pad { d2c.push(pad_pos); }
            display.push_str(&" ".repeat(pad));
        }
    }
    // Trailing spaces
    display.push_str("  ");
    d2c.push(line.len());
    d2c.push(line.len());
    d2c.push(line.len());

    (display, d2c)
}

/// Parse cells with their byte positions in the original line.
/// Returns (cell_text, byte_start, byte_end) for each cell.
fn parse_table_cells_with_positions(line: &str) -> Vec<(String, usize, usize)> {
    let bytes = line.as_bytes();
    let mut cells = Vec::new();
    let mut i = 0;
    let len = bytes.len();

    // Skip leading whitespace and optional leading |
    while i < len && (bytes[i] == b' ' || bytes[i] == b'\t') { i += 1; }
    if i < len && bytes[i] == b'|' { i += 1; }

    while i < len {
        // Skip leading whitespace in cell
        let content_start = i;
        while i < len && bytes[i] != b'|' { i += 1; }
        let content_end = i;
        let cell_slice = &line[content_start..content_end];
        let trimmed_start = content_start + (cell_slice.len() - cell_slice.trim_start().len());
        let trimmed_content = cell_slice.trim();
        let trimmed_end = trimmed_start + trimmed_content.len();
        // Only add non-empty trailing cells or all cells if not at end
        if !trimmed_content.is_empty() || (i < len && bytes[i] == b'|') {
            // Skip if this is the trailing empty cell after final |
            if !(trimmed_content.is_empty() && i >= len) {
                cells.push((trimmed_content.to_string(), trimmed_start, trimmed_end));
            }
        }
        if i < len && bytes[i] == b'|' { i += 1; }
    }

    // Remove trailing empty cells that come from trailing |
    while let Some((c, _, _)) = cells.last() {
        if c.is_empty() { cells.pop(); } else { break; }
    }

    cells
}

/// Process a list item line for display: replace markers with bullets/checkboxes.
/// Returns (display_text, display_to_content_mapping).
fn process_list_item_display(line: &str, prefix_len: usize) -> Option<(String, Vec<usize>)> {
    if prefix_len == 0 || prefix_len > line.len() { return None; }

    let prefix = &line[..prefix_len];
    let rest = &line[prefix_len..];

    // Detect indent (leading whitespace) and the marker character
    let indent_len: usize = prefix.bytes().take_while(|b| *b == b' ' || *b == b'\t').count();
    let marker_area = &prefix[indent_len..];

    // Task list: "- [ ] " or "- [x] "
    let is_bullet_marker = marker_area.starts_with("- ") || marker_area.starts_with("* ") || marker_area.starts_with("+ ");
    if is_bullet_marker && (rest.starts_with("[ ] ") || rest.starts_with("[x] ") || rest.starts_with("[X] ")) {
        let checked = rest.starts_with("[x] ") || rest.starts_with("[X] ");
        let checkbox = if checked { "☑  " } else { "☐  " };
        let content_after = &rest[4..]; // skip "[x] " or "[ ] "

        let mut display = String::new();
        let mut d2c: Vec<usize> = Vec::new();
        // Indent: each byte maps to its content position
        for i in 0..indent_len { display.push(' '); d2c.push(i); }
        // Checkbox bytes all map to start of marker
        for _ in 0..checkbox.len() { d2c.push(indent_len); }
        display.push_str(checkbox);
        // Content
        let content_start = prefix_len + 4;
        for (i, _) in content_after.char_indices() { d2c.push(content_start + i); }
        display.push_str(content_after);
        d2c.push(line.len());
        return Some((display, d2c));
    }

    // Bullet list
    if is_bullet_marker {
        let bullet = "•  ";
        let mut display = String::new();
        let mut d2c: Vec<usize> = Vec::new();
        for i in 0..indent_len { display.push(' '); d2c.push(i); }
        for _ in 0..bullet.len() { d2c.push(indent_len); }
        display.push_str(bullet);
        for (i, _) in rest.char_indices() { d2c.push(prefix_len + i); }
        display.push_str(rest);
        d2c.push(line.len());
        return Some((display, d2c));
    }

    // Numbered list: keep number visible but styled
    None
}

/// Build TextRuns for a table line.
/// `is_header`: bold the text. `is_separator`: muted color for the line.
fn build_table_runs(line: &str, fg: Hsla, muted: Hsla, is_header: bool, is_separator: bool) -> Vec<TextRun> {
    let mono_font = Font {
        family: "DejaVu Sans Mono".into(),
        features: FontFeatures::default(),
        fallbacks: Some(FontFallbacks::from_fonts(vec![
            "Menlo".into(),
            "Monaco".into(),
            "Consolas".into(),
            "SF Mono".into(),
            "Liberation Mono".into(),
            "Noto Sans Mono".into(),
            "monospace".into(),
        ])),
        weight: if is_header { FontWeight::BOLD } else { FontWeight::NORMAL },
        style: FontStyle::Normal,
    };

    if is_separator {
        // Entire line is a separator (just dashes): dim color
        return vec![TextRun {
            len: line.len(),
            font: mono_font,
            color: muted.opacity(0.4),
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
    }

    let mut runs = Vec::new();
    let len = line.len();
    if len == 0 { return runs; }
    let bytes = line.as_bytes();
    let mut pos = 0;
    while pos < len {
        let is_pipe = bytes[pos] == b'|';
        let mut end = pos + 1;
        while end < len {
            let next_is_pipe = bytes[end] == b'|';
            if next_is_pipe != is_pipe { break; }
            end += 1;
        }
        runs.push(TextRun {
            len: end - pos,
            font: mono_font.clone(),
            color: if is_pipe { muted.opacity(0.2) } else { fg },
            background_color: None,
            underline: None,
            strikethrough: None,
        });
        pos = end;
    }
    runs
}

/// Build TextRuns for raw display (all markers visible, dimmed).
fn build_text_runs_raw(
    line: &str,
    info: Option<&LineInfo>,
    base_font: &Font,
    fg: Hsla,
    muted: Hsla,
    code_bg: Hsla,
    link_color: Hsla,
    heading_bold: bool,
    is_code_block: bool,
) -> Vec<TextRun> {
    build_text_runs_inner(line, |byte| byte, info, base_font, fg, muted, code_bg, link_color, heading_bold, is_code_block)
}

/// Build TextRuns for stripped display. Maps display byte -> content byte.
fn build_text_runs_display(
    display_text: &str,
    display_to_content: &[usize],
    info: Option<&LineInfo>,
    base_font: &Font,
    fg: Hsla,
    muted: Hsla,
    code_bg: Hsla,
    link_color: Hsla,
    heading_bold: bool,
    is_code_block: bool,
) -> Vec<TextRun> {
    build_text_runs_inner(
        display_text,
        |d| display_to_content.get(d).copied().unwrap_or(0),
        info, base_font, fg, muted, code_bg, link_color, heading_bold, is_code_block,
    )
}

fn build_text_runs_inner(
    text: &str,
    map_to_content: impl Fn(usize) -> usize,
    info: Option<&LineInfo>,
    base_font: &Font,
    fg: Hsla,
    muted: Hsla,
    code_bg: Hsla,
    link_color: Hsla,
    heading_bold: bool,
    is_code_block: bool,
) -> Vec<TextRun> {
    let len = text.len();
    if len == 0 { return Vec::new(); }

    // Monospace font
    let mono_font = Font {
        family: "DejaVu Sans Mono".into(),
        features: FontFeatures::default(),
        fallbacks: Some(FontFallbacks::from_fonts(vec![
            "Menlo".into(),
            "Monaco".into(),
            "Consolas".into(),
            "SF Mono".into(),
            "Liberation Mono".into(),
            "Noto Sans Mono".into(),
            "monospace".into(),
        ])),
        weight: FontWeight::NORMAL,
        style: FontStyle::Normal,
    };

    if is_code_block {
        return vec![TextRun {
            len,
            font: mono_font,
            color: fg,
            background_color: Some(code_bg),
            underline: None,
            strikethrough: None,
        }];
    }

    // Per-byte style lookup using content mapping
    let prefix_len = info.map(|i| i.block.prefix_len).unwrap_or(0);
    let spans = info.map(|i| i.spans.as_slice()).unwrap_or(&[]);

    // Compute style at a content byte position
    let style_at = |content_byte: usize| -> (bool, bool, bool, bool, bool, bool) {
        // (is_prefix, is_bold, is_italic, is_code, is_strike, is_link)
        let is_prefix = content_byte < prefix_len;
        let mut is_bold = heading_bold;
        let mut is_italic = false;
        let mut is_code = false;
        let mut is_strike = false;
        let mut is_link = false;
        for span in spans {
            if span.range.start <= content_byte && content_byte < span.range.end {
                match span.style {
                    SpanStyle::Bold => is_bold = true,
                    SpanStyle::Italic => is_italic = true,
                    SpanStyle::BoldItalic => { is_bold = true; is_italic = true; }
                    SpanStyle::InlineCode => is_code = true,
                    SpanStyle::Strikethrough => is_strike = true,
                    SpanStyle::Link => is_link = true,
                }
            }
        }
        (is_prefix, is_bold, is_italic, is_code, is_strike, is_link)
    };

    // Walk through text bytes, group consecutive bytes with same style
    let mut runs = Vec::new();
    let mut pos = 0;
    while pos < len {
        if !text.is_char_boundary(pos) { pos += 1; continue; }
        let start_content = map_to_content(pos);
        let start_style = style_at(start_content);

        let mut end = pos + 1;
        while end < len {
            if text.is_char_boundary(end) {
                let content = map_to_content(end);
                if style_at(content) != start_style { break; }
            }
            end += 1;
        }
        // Ensure end is at char boundary
        while end < len && !text.is_char_boundary(end) { end += 1; }

        let (is_prefix, is_bold, is_italic, is_code, is_strike, is_link) = start_style;
        {
            let start = pos;

        let font = if is_code {
            mono_font.clone()
        } else {
            let mut f = base_font.clone();
            if is_bold { f.weight = FontWeight::BOLD; }
            if is_italic { f.style = FontStyle::Italic; }
            f
        };

        let color = if is_prefix {
            muted.opacity(0.5)
        } else if is_link {
            link_color
        } else {
            fg
        };

        let bg = if is_code { Some(code_bg) } else { None };

        let strikethrough = if is_strike {
            Some(StrikethroughStyle { thickness: px(1.), color: Some(muted) })
        } else { None };

        let underline = if is_link {
            Some(UnderlineStyle { thickness: px(1.), color: None, wavy: false })
        } else { None };

        runs.push(TextRun {
            len: end - start,
            font,
            color,
            background_color: bg,
            underline,
            strikethrough,
        });
        }
        pos = end;
    }

    if runs.is_empty() {
        let mut default_font = base_font.clone();
        if heading_bold { default_font.weight = FontWeight::BOLD; }
        runs.push(TextRun { len, font: default_font, color: fg, background_color: None, underline: None, strikethrough: None });
    }

    runs
}

/// Word-boundary-only wrapping: split text into segments where breaks happen ONLY at whitespace.
/// Returns Vec of (byte_start, byte_end) for each visual row.
fn wrap_at_word_boundaries(display: &str, max_chars: usize) -> Vec<(usize, usize)> {
    if display.is_empty() { return vec![(0, 0)]; }
    if max_chars == 0 { return vec![(0, display.len())]; }

    let mut rows: Vec<(usize, usize)> = Vec::new();
    let mut row_start = 0usize;
    let mut last_space: Option<usize> = None;
    let mut chars_in_row = 0usize;

    for (byte_idx, c) in display.char_indices() {
        if c == ' ' { last_space = Some(byte_idx); }
        chars_in_row += 1;
        if chars_in_row > max_chars {
            if let Some(sp) = last_space {
                if sp > row_start {
                    rows.push((row_start, sp));
                    row_start = sp + 1;
                    chars_in_row = display[row_start..byte_idx + c.len_utf8()].chars().count();
                    last_space = None;
                    continue;
                }
            }
            // No space found -- let this row overflow (word too long)
        }
    }
    if row_start < display.len() {
        rows.push((row_start, display.len()));
    } else if rows.is_empty() {
        rows.push((0, 0));
    }
    rows
}

/// Slice TextRuns to byte range [start, end).
fn slice_runs(runs: &[TextRun], start: usize, end: usize) -> Vec<TextRun> {
    let mut result = Vec::new();
    let mut pos = 0;
    for run in runs {
        let run_end = pos + run.len;
        if run_end <= start || pos >= end { pos = run_end; continue; }
        let s = start.max(pos) - pos;
        let e = end.min(run_end) - pos;
        if e > s {
            let mut new_run = run.clone();
            new_run.len = e - s;
            result.push(new_run);
        }
        pos = run_end;
    }
    result
}

/// Parse inline markdown in a cell (bold, italic, code, strikethrough) and return
/// the display text with markers stripped + styled TextRuns.
fn build_cell_inline(
    text: &str,
    base_font: &Font,
    fg: Hsla,
    muted: Hsla,
    link_color: Hsla,
    is_header: bool,
    known_notes: &HashSet<String>,
) -> (String, Vec<TextRun>) {
    // Fast path: no wikilinks, run the plain inline path directly.
    // Use the byte-level scanner (not parse_lines) so we don't spin up
    // pulldown-cmark twice per cell.
    let wls = markdown::scan_wikilinks(text);
    if wls.is_empty() {
        return build_cell_inline_plain(text, base_font, fg, muted, is_header);
    }
    // Split the cell into (non-wikilink segment, wikilink) pairs, process each part
    // separately, and concatenate display + runs.
    let mut display = String::new();
    let mut runs: Vec<TextRun> = Vec::new();
    let mut cursor = 0usize;
    for wl in &wls {
        if wl.range.start > cursor {
            let segment = &text[cursor..wl.range.start];
            let (seg_display, seg_runs) = build_cell_inline_plain(segment, base_font, fg, muted, is_header);
            // Drop the trailing placeholder space that plain emits for empty input.
            if !(segment.is_empty() && seg_display == " ") {
                display.push_str(&seg_display);
                runs.extend(seg_runs);
            }
        }
        // Emit the wikilink's visible text as one styled run.
        let (vs, ve) = wikilink_visible_content_range(text, wl);
        let visible = &text[vs..ve];
        if !visible.is_empty() {
            let exists = known_notes.contains(&wl.target.to_ascii_lowercase());
            let color = if exists { link_color } else { muted };
            let mut font = base_font.clone();
            if is_header { font.weight = FontWeight::BOLD; }
            runs.push(TextRun {
                len: visible.len(), font, color, background_color: None,
                underline: Some(UnderlineStyle { thickness: px(1.), color: None, wavy: false }),
                strikethrough: None,
            });
            display.push_str(visible);
        }
        cursor = wl.range.end;
    }
    if cursor < text.len() {
        let segment = &text[cursor..];
        let (seg_display, seg_runs) = build_cell_inline_plain(segment, base_font, fg, muted, is_header);
        if !(segment.is_empty() && seg_display == " ") {
            display.push_str(&seg_display);
            runs.extend(seg_runs);
        }
    }
    if display.is_empty() {
        let mut f = base_font.clone();
        if is_header { f.weight = FontWeight::BOLD; }
        display.push(' ');
        runs.push(TextRun { len: 1, font: f, color: fg, background_color: None, underline: None, strikethrough: None });
    }
    (display, runs)
}

/// Plain inline cell rendering (no wikilink support) -- the original body of
/// `build_cell_inline`.
fn build_cell_inline_plain(text: &str, base_font: &Font, fg: Hsla, muted: Hsla, is_header: bool) -> (String, Vec<TextRun>) {
    use pulldown_cmark::{Event, Parser, Options, Tag, TagEnd};

    if text.is_empty() {
        let mut f = base_font.clone();
        if is_header { f.weight = FontWeight::BOLD; }
        return (" ".to_string(), vec![TextRun {
            len: 1, font: f, color: fg, background_color: None, underline: None, strikethrough: None,
        }]);
    }

    let opts = Options::ENABLE_STRIKETHROUGH;
    let parser = Parser::new_ext(text, opts);

    let mut display = String::new();
    let mut runs: Vec<TextRun> = Vec::new();
    let mut bold_stack = 0u32;
    let mut italic_stack = 0u32;
    let mut strike_stack = 0u32;

    let make_run = |len: usize, is_bold: bool, is_italic: bool, is_strike: bool, is_code: bool| -> TextRun {
        let mut font = base_font.clone();
        font.weight = if is_bold || is_header { FontWeight::BOLD } else { FontWeight::NORMAL };
        font.style = if is_italic { FontStyle::Italic } else { FontStyle::Normal };
        let bg = if is_code { Some(hsla(0., 0., 0.5, 0.12)) } else { None };
        TextRun {
            len, font, color: fg, background_color: bg, underline: None,
            strikethrough: if is_strike { Some(StrikethroughStyle { thickness: px(1.), color: Some(muted) }) } else { None },
        }
    };

    for event in parser {
        match event {
            Event::Start(Tag::Strong) => bold_stack += 1,
            Event::End(TagEnd::Strong) => bold_stack = bold_stack.saturating_sub(1),
            Event::Start(Tag::Emphasis) => italic_stack += 1,
            Event::End(TagEnd::Emphasis) => italic_stack = italic_stack.saturating_sub(1),
            Event::Start(Tag::Strikethrough) => strike_stack += 1,
            Event::End(TagEnd::Strikethrough) => strike_stack = strike_stack.saturating_sub(1),
            Event::Text(t) => {
                let len = t.len();
                if len > 0 {
                    display.push_str(&t);
                    runs.push(make_run(len, bold_stack > 0, italic_stack > 0, strike_stack > 0, false));
                }
            }
            Event::Code(t) => {
                let len = t.len();
                if len > 0 {
                    display.push_str(&t);
                    runs.push(make_run(len, bold_stack > 0, italic_stack > 0, strike_stack > 0, true));
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                display.push(' ');
                runs.push(make_run(1, false, false, false, false));
            }
            _ => {}
        }
    }

    if display.is_empty() {
        display.push(' ');
        runs.push(make_run(1, false, false, false, false));
    }

    (display, runs)
}

// ── Table widget building ──

/// Count the "effective" length of a cell's text, stripping inline markdown markers
/// AND wikilink brackets (so `[[Target|alias]]` contributes `alias`'s length only).
fn cell_effective_length(text: &str) -> usize {
    // First strip wikilinks to their visible form.
    let stripped = strip_wikilinks_for_display(text);
    let mut count = 0usize;
    let mut chars = stripped.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' | '~' | '`' => {
                while chars.peek() == Some(&c) { chars.next(); }
            }
            _ => count += 1,
        }
    }
    count
}

/// Replace every `[[...]]` in `text` with its visible content (alias if present,
/// else whatever is between `[[` and `]]`). Used for width estimation.
fn strip_wikilinks_for_display(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 3 < bytes.len() && bytes[i] == b'[' && bytes[i + 1] == b'[' {
            // Find closing ]] on same line.
            let content_start = i + 2;
            let mut j = content_start;
            let mut closed = false;
            while j + 1 < bytes.len() {
                if bytes[j] == b'[' && bytes[j + 1] == b'[' { break; }
                if bytes[j] == b']' && bytes[j + 1] == b']' { closed = true; break; }
                j += 1;
            }
            if closed && j > content_start {
                let inner = &text[content_start..j];
                // Visible = after `|` if present, else full inner.
                let visible = match inner.find('|') {
                    Some(p) => &inner[p + 1..],
                    None => inner,
                };
                out.push_str(visible);
                i = j + 2;
                continue;
            }
        }
        // Copy one char.
        let c = text[i..].chars().next().unwrap();
        out.push(c);
        i += c.len_utf8();
    }
    out
}

fn build_table_render(
    editor: &Editor,
    start: usize,
    end: usize,
    available_width: f32,
    y_origin: f32,
    text_system: &WindowTextSystem,
    fg: Hsla,
    muted: Hsla,
    link_color: Hsla,
) -> RenderTable {
    // Determine column count from the widest row
    let mut n_cols: usize = 1;
    for i in start..end {
        let line = editor.buffer.line_str(i);
        let cells = parse_table_cells(&line);
        n_cols = n_cols.max(cells.len());
    }

    // Cell padding + table margins
    const CELL_PAD_H: f32 = 12.0;
    const CELL_PAD_V: f32 = 6.0;
    const TABLE_MARGIN: f32 = 8.0;

    // Per-column: max content length AND longest single word (for min width)
    const MONO_CHAR_PX: f32 = 8.4;
    let mut col_max_chars: Vec<usize> = vec![0; n_cols];
    let mut col_longest_word: Vec<usize> = vec![3; n_cols];
    for i in start..end {
        let kind = editor.table_row_kinds.get(i).copied().unwrap_or(TableRowKind::NotTable);
        if kind == TableRowKind::Separator { continue; }
        let line = editor.buffer.line_str(i);
        let cells = parse_table_cells(&line);
        for (k, cell) in cells.iter().enumerate().take(n_cols) {
            let eff = cell_effective_length(cell);
            if eff > col_max_chars[k] { col_max_chars[k] = eff; }
            let longest = cell.split_whitespace().map(|w| w.chars().count()).max().unwrap_or(0);
            if longest > col_longest_word[k] { col_longest_word[k] = longest; }
        }
    }

    // Minimum pixel width per column = longest word width + cell padding
    let col_min_px: Vec<f32> = col_longest_word.iter()
        .map(|&w| (w as f32 * MONO_CHAR_PX) + 2.0 * CELL_PAD_H + 4.0)
        .collect();

    let usable: f32 = (available_width - 2.0 * TABLE_MARGIN).max(200.0);
    let total_min: f32 = col_min_px.iter().sum();
    let total_content_chars: f32 = col_max_chars.iter().map(|&c| c.max(3) as f32).sum();

    let mut col_widths: Vec<f32> = vec![0.0; n_cols];
    if total_min >= usable {
        // Minimums don't fit — use min widths (table may overflow)
        col_widths = col_min_px.clone();
    } else {
        // Start with minimums, distribute extra space by content weight
        let extra = usable - total_min;
        for i in 0..n_cols {
            let weight = (col_max_chars[i].max(3) as f32) / total_content_chars.max(1.0);
            col_widths[i] = col_min_px[i] + extra * weight;
        }
    }

    let mut col_x: Vec<f32> = Vec::with_capacity(n_cols);
    let mut x_accum = TABLE_MARGIN;
    for i in 0..n_cols {
        col_x.push(x_accum);
        x_accum += col_widths[i];
    }

    let mono_font = Font {
        family: "DejaVu Sans Mono".into(),
        features: FontFeatures::default(),
        fallbacks: Some(FontFallbacks::from_fonts(vec![
            "Menlo".into(), "Monaco".into(), "Consolas".into(),
            "SF Mono".into(), "Liberation Mono".into(), "Noto Sans Mono".into(),
            "monospace".into(),
        ])),
        weight: FontWeight::NORMAL,
        style: FontStyle::Normal,
    };

    let mut rows: Vec<RenderTableRow> = Vec::new();
    let mut y: f32 = 0.0;
    let mut header_end_y: Option<f32> = None;

    for line_idx in start..end {
        let line_text = editor.buffer.line_str(line_idx);
        let kind = editor.table_row_kinds.get(line_idx).copied().unwrap_or(TableRowKind::NotTable);

        if kind == TableRowKind::Separator {
            // Skip in visible rows; render as horizontal rule between header and body
            continue;
        }

        let parsed = parse_table_cells(&line_text);
        let mut cell_renders: Vec<RenderCell> = Vec::new();
        let zoom = editor.zoom;
        let lh = LINE_HEIGHT * zoom;
        let mut max_cell_height: f32 = lh;

        for (col, cell_text) in parsed.iter().enumerate().take(n_cols) {
            let cell_inner_width = col_widths[col] - 2.0 * CELL_PAD_H;
            let is_header = kind == TableRowKind::Header;
            // Parse inline markdown and build display + runs
            let (display_text, runs) = build_cell_inline(cell_text, &mono_font, fg, muted, link_color, is_header, &editor.known_notes);
            // Word-boundary wrap using char count (monospace ≈ 8.4px per char at 14pt)
            const MONO_CHAR_PX: f32 = 8.4;
            let max_chars = ((cell_inner_width.max(40.0)) / MONO_CHAR_PX).floor() as usize;
            let rows_ranges = wrap_at_word_boundaries(&display_text, max_chars.max(1));
            let mut cell_lines: Vec<ShapedLine> = Vec::new();
            for (s, e) in &rows_ranges {
                let seg_text = &display_text[*s..*e];
                let seg_runs = slice_runs(&runs, *s, *e);
                let shared: SharedString = if seg_text.is_empty() { " ".to_string().into() } else { seg_text.to_string().into() };
                let final_runs = if seg_runs.is_empty() {
                    let mut f = mono_font.clone();
                    if is_header { f.weight = FontWeight::BOLD; }
                    vec![TextRun { len: shared.len(), font: f, color: fg, background_color: None, underline: None, strikethrough: None }]
                } else { seg_runs };
                let shaped = text_system.shape_line(shared, px(14. * zoom), &final_runs, None);
                cell_lines.push(shaped);
            }
            let cell_h = lh * cell_lines.len().max(1) as f32;
            max_cell_height = max_cell_height.max(cell_h);
            cell_renders.push(RenderCell { lines: cell_lines, col });
        }

        let row_height = max_cell_height + 2.0 * CELL_PAD_V;
        rows.push(RenderTableRow {
            content_line: line_idx,
            cells: cell_renders,
            kind,
            y_in_table: y,
            height: row_height,
        });
        y += row_height;

        if kind == TableRowKind::Header && header_end_y.is_none() {
            header_end_y = Some(y);
        }
    }

    RenderTable {
        content_start: start,
        content_end: end,
        col_x,
        col_widths,
        rows,
        y_origin,
        total_height: y,
        header_end_y,
    }
}

// ── Rendering ──

pub struct EditorElement {
    editor: Entity<Editor>,
}

pub struct EditorPrepaint {
    items: Vec<RenderItem>,
    first_line: usize,
    cursor: Option<PaintQuad>,
    selections: Vec<PaintQuad>,
    code_block_regions: Vec<(f32, f32)>, // (top_y, bottom_y) for code block bg
    hr_lines: Vec<f32>,                  // y (relative to bounds.top()) of each HR to paint
    autocomplete_popup: Option<AutocompletePopupData>,
}

/// Pre-shaped rows + origin for the wikilink autocomplete popup.
struct AutocompletePopupData {
    origin: Point<Pixels>,
    width: Pixels,
    rows: Vec<AutocompletePopupRow>,
}

struct AutocompletePopupRow {
    basename: String,
    rel_path: String,
    selected: bool,
}

impl IntoElement for EditorElement {
    type Element = Self;
    fn into_element(self) -> Self { self }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = EditorPrepaint;

    fn id(&self) -> Option<ElementId> { None }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> { None }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) -> EditorPrepaint {
        // Advance scroll animation (frame-driven, tied to vsync). If we haven't
        // converged on the target, request another frame; GPUI will coalesce this
        // with the natural render cadence so we get smooth 60Hz animation with
        // zero drift compared to a manual timer.
        let need_anim_frame = self.editor.update(cx, |ed, _| {
            if ed.needs_reparse || ed.parsed_lines.len() != ed.buffer.len_lines() {
                ed.reparse();
            }
            ed.preload_image_embeds();
            ed.tick_scroll_animation()
        });
        if need_anim_frame { window.request_animation_frame(); }

        let editor = self.editor.read(cx);
        let style = window.text_style();
        let font = style.font();
        let fg = style.color;
        let muted = hsla(0., 0., 0.5, 1.0);
        let selection_color = hsla(210. / 360., 0.7, 0.5, 0.3);
        let code_bg = hsla(0., 0., 0.5, 0.1);
        let link_color = hsla(210. / 360., 0.8, 0.55, 1.0);

        let viewport_h: f32 = bounds.size.height.into();
        let first_line = editor.first_visible_line();
        let first_line_y_abs = editor.line_y(first_line);
        let total_lines = editor.buffer.len_lines();
        let sel = editor.buffer.selection();
        let (cursor_line, cursor_col) = editor.buffer.cursor_line_col();
        let read_mode = editor.read_mode;

        // Determine active block range (in edit mode, around cursor)
        let (active_start, active_end) = if read_mode || editor.parsed_lines.is_empty() {
            (usize::MAX, 0) // no active block (all stripped)
        } else {
            let n = editor.parsed_lines.len();
            markdown::block_range(
                n,
                cursor_line,
                |i| editor.buffer.line_str(i).trim().is_empty(),
                |i| editor.parsed_lines.get(i).map(|l| l.block.block_type),
            )
        };

        let text_system = window.text_system();
        let wrap_width: Pixels = bounds.size.width;
        let line_h = editor.line_h();
        let line_height_px = px(line_h);
        let available_width_f: f32 = bounds.size.width.into();
        let mut items: Vec<RenderItem> = Vec::new();
        let mut code_block_regions: Vec<(f32, f32)> = Vec::new();
        let mut hr_lines: Vec<f32> = Vec::new();
        // Top of `first_line` in viewport-relative coords (<= 0).
        let mut current_y = first_line_y_abs - editor.scroll_offset;
        let mut code_block_start_y: Option<f32> = None;
        // Collect (line_index, rendered_height) pairs so we can update the cache.
        let mut height_updates: Vec<(usize, f32)> = Vec::new();
        // Cache write-backs to apply at end of prepaint.
        let mut new_line_cache: Vec<(usize, LineCacheEntry)> = Vec::new();
        let mut new_table_cache: Vec<(usize, TableCacheEntry)> = Vec::new();
        let wrap_width_f: f32 = bounds.size.width.into();

        let mut i = first_line;
        while i < total_lines {
            if current_y > viewport_h { break; }

            let line_info = editor.parsed_lines.get(i);
            let block_type = line_info.map(|li| li.block.block_type);
            let in_active_block = i >= active_start && i <= active_end;

            // Track code block + math block regions (same decoration treatment)
            if matches!(block_type, Some(BlockType::CodeBlock) | Some(BlockType::MathBlock)) {
                if code_block_start_y.is_none() { code_block_start_y = Some(current_y); }
            } else if let Some(start) = code_block_start_y {
                code_block_regions.push((start, current_y));
                code_block_start_y = None;
            }
            // Track HR y-positions (centered vertically in the line).
            if matches!(block_type, Some(BlockType::HorizontalRule)) {
                hr_lines.push(current_y + line_h * 0.5);
            }

            // Hybrid: table as widget (non-active, not read mode active)
            if matches!(block_type, Some(BlockType::Table)) && !in_active_block {
                let mut table_end = i + 1;
                while table_end < total_lines
                    && matches!(editor.parsed_lines.get(table_end).map(|l| l.block.block_type), Some(BlockType::Table))
                {
                    table_end += 1;
                }
                // Cache check: reuse previously-shaped table if wrap width + extent match.
                let cached_table: Option<RenderTable> = editor.table_cache.get(&i)
                    .filter(|c| (c.wrap_width - wrap_width_f).abs() < 0.5 && c.content_end == table_end)
                    .map(|c| {
                        let mut t = c.table.clone();
                        t.y_origin = current_y;
                        t
                    });
                let table = cached_table.unwrap_or_else(|| {
                    let t = build_table_render(
                        &editor, i, table_end, available_width_f, current_y,
                        text_system, fg, muted, link_color,
                    );
                    new_table_cache.push((i, TableCacheEntry {
                        wrap_width: wrap_width_f,
                        content_end: table_end,
                        table: t.clone(),
                    }));
                    t
                });
                let th = table.total_height;
                // Record per-row heights so scroll math reflects actual table size.
                let mut accounted = 0.0f32;
                for row in &table.rows {
                    height_updates.push((row.content_line, row.height));
                    accounted += row.height;
                }
                // Attribute leftover (header divider padding, etc.) to the table's first line.
                let leftover = th - accounted;
                if leftover > 0.0 {
                    if let Some((_, h)) = height_updates.iter_mut().rev().find(|(line, _)| *line == i) {
                        *h += leftover;
                    }
                }
                items.push(RenderItem::Table(table));
                current_y += th;
                i = table_end;
                continue;
            }

            // Hybrid: image embed (non-active only)
            if matches!(block_type, Some(BlockType::ImageEmbed)) && !in_active_block {
                let line_text = editor.buffer.line_str(i);
                let target = markdown::parse_image_embed(line_text.trim()).unwrap_or_default();
                if let Some((image, native_w, native_h)) = editor.cached_image(&target) {
                    let max_w = available_width_f.min(IMAGE_MAX_WIDTH);
                    let max_h = IMAGE_MAX_HEIGHT;
                    let nw = native_w as f32;
                    let nh = native_h as f32;
                    let (rw, rh) = if nw <= max_w && nh <= max_h {
                        (nw, nh)
                    } else {
                        let s = (max_w / nw).min(max_h / nh);
                        (nw * s, nh * s)
                    };
                    let total_h = rh + IMAGE_PADDING * 2.0;
                    items.push(RenderItem::Image(RenderImageItem {
                        content_line: i,
                        image,
                        y_origin: current_y,
                        render_width: rw,
                        render_height: rh,
                        total_height: total_h,
                    }));
                    height_updates.push((i, total_h));
                    current_y += total_h;
                    i += 1;
                    continue;
                }
                // If image load failed, fall through to normal line rendering
                // (placeholder box from build_display_line).
            }

            // Line item (normal text or active table row)
            let show_raw = in_active_block && !read_mode;
            // Cache check: reuse display + wrapped if shape-invariants match.
            let cached = editor.line_cache.get(i)
                .and_then(|c| c.as_ref())
                .filter(|c| (c.wrap_width - wrap_width_f).abs() < 0.5 && c.show_raw == show_raw);
            let (display, wrapped, height) = if let Some(c) = cached {
                (c.display.clone(), c.wrapped.clone(), c.height)
            } else {
                let line_text = editor.buffer.line_str(i);
                let empty_widths: Vec<usize> = Vec::new();
                let col_widths = editor.table_col_widths.get(i).unwrap_or(&empty_widths);
                let row_kind = editor.table_row_kinds.get(i).copied().unwrap_or(TableRowKind::NotTable);
                let table_fs = editor.table_font_sizes.get(i).copied().unwrap_or(0.0);
                let dl = build_display_line(&line_text, line_info, show_raw, col_widths, row_kind, table_fs, &font, fg, muted, code_bg, link_color, &editor.known_notes, editor.zoom);
                let shared: SharedString = dl.display_text.clone().into();
                let wrapped_list = text_system.shape_text(
                    shared, px(dl.font_size), &dl.runs, Some(wrap_width), None,
                ).unwrap_or_default();
                let wrapped = wrapped_list.into_iter().next().unwrap_or_default();
                let n_rows = wrapped.wrap_boundaries.len() + 1;
                let height = line_h * n_rows as f32;
                new_line_cache.push((i, LineCacheEntry {
                    wrap_width: wrap_width_f,
                    show_raw,
                    display: dl.clone(),
                    wrapped: wrapped.clone(),
                    height,
                }));
                (dl, wrapped, height)
            };

            items.push(RenderItem::Line(RenderLine {
                content_line: i,
                wrapped,
                display,
                y_origin: current_y,
                height,
            }));
            height_updates.push((i, height));
            current_y += height;
            i += 1;
        }
        // Close any pending code block region
        if let Some(start) = code_block_start_y { code_block_regions.push((start, current_y)); }

        // Cursor (hidden in read mode, shown only on Line items)
        let cursor = if !read_mode && sel.is_empty() && editor.cursor_visible {
            let mut cursor_quad = None;
            for it in &items {
                if let RenderItem::Line(ln) = it {
                    if ln.content_line == cursor_line {
                        let display_col = ln.display.content_to_display(cursor_col);
                        if let Some(pos) = ln.wrapped.position_for_index(display_col, line_height_px) {
                            let x = bounds.left() + pos.x;
                            let y = bounds.top() + px(ln.y_origin) + pos.y;
                            cursor_quad = Some(fill(Bounds::new(point(x, y), size(px(2.), line_height_px)), blue()));
                        }
                        break;
                    }
                }
            }
            cursor_quad
        } else { None };

        // Selections (only on Line items)
        let mut selections = Vec::new();
        if !sel.is_empty() {
            let sel_start_line = editor.buffer.byte_to_line(sel.start());
            let sel_end_line = editor.buffer.byte_to_line(sel.end());
            let sel_start_col = sel.start() - editor.buffer.line_to_byte(sel_start_line);
            let sel_end_col = sel.end() - editor.buffer.line_to_byte(sel_end_line);

            for it in &items {
                let RenderItem::Line(ln) = it else { continue };
                if ln.content_line < sel_start_line || ln.content_line > sel_end_line { continue; }
                let sc_content = if ln.content_line == sel_start_line { sel_start_col } else { 0 };
                let ec_content = if ln.content_line == sel_end_line { sel_end_col } else { editor.buffer.line_str(ln.content_line).len() };
                let sc = ln.display.content_to_display(sc_content);
                let ec = ln.display.content_to_display(ec_content);
                let Some(start_pos) = ln.wrapped.position_for_index(sc, line_height_px) else { continue };
                let Some(end_pos) = ln.wrapped.position_for_index(ec, line_height_px) else { continue };
                let line_origin_y = bounds.top() + px(ln.y_origin);
                if start_pos.y == end_pos.y {
                    selections.push(fill(
                        Bounds::new(point(bounds.left() + start_pos.x, line_origin_y + start_pos.y), size(end_pos.x - start_pos.x, line_height_px)),
                        selection_color,
                    ));
                } else {
                    selections.push(fill(
                        Bounds::new(point(bounds.left() + start_pos.x, line_origin_y + start_pos.y), size(wrap_width - start_pos.x, line_height_px)),
                        selection_color,
                    ));
                    let mut mid_y: f32 = start_pos.y.into();
                    mid_y += line_h;
                    let end_y_f32: f32 = end_pos.y.into();
                    while mid_y < end_y_f32 {
                        selections.push(fill(
                            Bounds::new(point(bounds.left(), line_origin_y + px(mid_y)), size(wrap_width, line_height_px)),
                            selection_color,
                        ));
                        mid_y += line_h;
                    }
                    selections.push(fill(
                        Bounds::new(point(bounds.left(), line_origin_y + end_pos.y), size(end_pos.x, line_height_px)),
                        selection_color,
                    ));
                }
            }
        }

        // Compute autocomplete popup anchor + content (uses `editor` immutable borrow).
        let autocomplete_popup = (|| -> Option<AutocompletePopupData> {
            let state = editor.autocomplete.as_ref()?;
            // Find the screen position of the `[[` trigger on the current line.
            let trigger_line = editor.buffer.byte_to_line(state.trigger_byte);
            let line_start = editor.buffer.line_to_byte(trigger_line);
            let trigger_col = state.trigger_byte - line_start;
            let mut anchor: Option<Point<Pixels>> = None;
            for it in &items {
                if let RenderItem::Line(ln) = it {
                    if ln.content_line == trigger_line {
                        let display_col = ln.display.content_to_display(trigger_col);
                        if let Some(pos) = ln.wrapped.position_for_index(display_col, line_height_px) {
                            let x = bounds.left() + pos.x;
                            let y = bounds.top() + px(ln.y_origin) + pos.y + line_height_px;
                            anchor = Some(point(x, y));
                        }
                        break;
                    }
                }
            }
            let origin = anchor?;
            let rows: Vec<AutocompletePopupRow> = state.matches.iter().enumerate().take(8).map(|(i, &file_idx)| {
                let vf = &editor.vault_files[file_idx];
                AutocompletePopupRow {
                    basename: vf.basename.clone(),
                    rel_path: vf.rel_path.clone(),
                    selected: i == state.selected,
                }
            }).collect();
            if rows.is_empty() { return None; }
            Some(AutocompletePopupData { origin, width: px(320.), rows })
        })();

        // Release the immutable borrow of editor before mutating via update().
        let _ = editor;

        // Store layout state for hit testing
        let items_clone = items.clone();
        self.editor.update(cx, |ed, _| {
            ed.last_bounds = Some(bounds);
            ed.last_items = items_clone;
            ed.last_first_line = first_line;
            ed.viewport_height = viewport_h;
            // Delta-update cached heights + running sum; only rebuild the prefix
            // sum if anything actually changed (cheap f32 != guard).
            let mut any_changed = false;
            for (line, h) in &height_updates {
                if let Some(slot) = ed.line_heights.get_mut(*line) {
                    if (*slot - *h).abs() > 0.01 {
                        ed.line_heights_sum += *h - *slot;
                        *slot = *h;
                        any_changed = true;
                    }
                }
            }
            if any_changed { ed.rebuild_cumulative_heights(); }
            // Apply shape cache write-backs.
            for (line, entry) in new_line_cache.drain(..) {
                if line < ed.line_cache.len() {
                    ed.line_cache[line] = Some(entry);
                }
            }
            for (key, entry) in new_table_cache.drain(..) {
                ed.table_cache.insert(key, entry);
            }
            // Clamp scroll against real total (O(1) via cached sum).
            let max = (ed.line_heights_sum + BOTTOM_PADDING - viewport_h).max(0.0);
            if ed.scroll_offset > max { ed.scroll_offset = max; }
            if ed.scroll_target > max { ed.scroll_target = max; }
        });

        EditorPrepaint {
            items, first_line, cursor, selections, code_block_regions, hr_lines, autocomplete_popup,
        }
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), state: &mut EditorPrepaint, window: &mut Window, cx: &mut App) {
        // Register input handler so OS sends keystrokes to our EntityInputHandler
        let input_handler = ElementInputHandler::new(bounds, self.editor.clone());
        window.handle_input(&self.editor.focus_handle(cx), input_handler, cx);

        let is_dark = cx.theme().mode.is_dark();
        let code_block_bg = if is_dark { hsla(0., 0., 1., 0.04) } else { hsla(0., 0., 0., 0.03) };
        // Modern minimal styling
        let table_row_divider = if is_dark { hsla(0., 0., 1., 0.08) } else { hsla(0., 0., 0., 0.08) };
        let table_header_divider = if is_dark { hsla(0., 0., 1., 0.2) } else { hsla(0., 0., 0., 0.2) };

        // Paint code block backgrounds
        for (top_y, bot_y) in &state.code_block_regions {
            let top = bounds.top() + px(*top_y);
            let h = px(*bot_y - *top_y);
            if h <= px(0.) { continue; }
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.left() - px(4.), top - px(2.)),
                    size(bounds.size.width + px(8.), h + px(4.)),
                ),
                code_block_bg,
            ));
        }

        // Paint horizontal rules (full content width).
        let hr_color = if is_dark { hsla(0., 0., 1., 0.18) } else { hsla(0., 0., 0., 0.18) };
        for hr_y in &state.hr_lines {
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.left(), bounds.top() + px(*hr_y) - px(0.5)),
                    size(bounds.size.width, px(1.0)),
                ),
                hr_color,
            ));
        }

        // Paint selections
        for sel in &state.selections {
            window.paint_quad(sel.clone());
        }

        let line_h = self.editor.read(cx).line_h();
        // Paint each render item
        for item in &state.items {
            match item {
                RenderItem::Line(ln) => {
                    let _ = ln.wrapped.paint(
                        point(bounds.left(), bounds.top() + px(ln.y_origin)),
                        px(line_h),
                        TextAlign::Left,
                        None,
                        window, cx,
                    );
                }
                RenderItem::Image(img) => {
                    // Center the image horizontally within the bounds.
                    let avail: f32 = bounds.size.width.into();
                    let x_pad = ((avail - img.render_width) * 0.5).max(0.0);
                    let origin = point(
                        bounds.left() + px(x_pad),
                        bounds.top() + px(img.y_origin + IMAGE_PADDING),
                    );
                    let img_bounds = Bounds::new(
                        origin,
                        size(px(img.render_width), px(img.render_height)),
                    );
                    let _ = window.paint_image(
                        img_bounds,
                        Corners::all(px(4.0)),
                        img.image.clone(),
                        0,
                        false,
                    );
                }
                RenderItem::Table(t) => {
                    let table_top = bounds.top() + px(t.y_origin);
                    let table_left = bounds.left() + px(8.0);
                    let table_width = bounds.size.width - px(16.0);
                    let header_count = if t.header_end_y.is_some() { 1 } else { 0 };

                    // Single divider under the header (slightly stronger)
                    if let Some(hend_y) = t.header_end_y {
                        window.paint_quad(fill(
                            Bounds::new(point(table_left, table_top + px(hend_y) - px(1.)), size(table_width, px(1.))),
                            table_header_divider,
                        ));
                    }

                    // Paint each row
                    for (row_idx, row) in t.rows.iter().enumerate() {
                        let row_top = table_top + px(row.y_in_table);

                        // Thin divider between data rows (not under header, not after last row)
                        let is_header = row_idx == 0 && header_count > 0;
                        if row_idx < t.rows.len() - 1 && !is_header {
                            window.paint_quad(fill(
                                Bounds::new(point(table_left, row_top + px(row.height)), size(table_width, px(1.))),
                                table_row_divider,
                            ));
                        }

                        // Paint cells (each cell may have multiple visual rows)
                        for cell in &row.cells {
                            let cell_x = bounds.left() + px(t.col_x[cell.col] + 12.0);
                            let col_w = t.col_widths.get(cell.col).copied().unwrap_or(0.);
                            let cell_inner_w = (col_w - 24.0).max(0.);
                            let cell_mask = ContentMask {
                                bounds: Bounds::new(
                                    point(cell_x, row_top),
                                    size(px(cell_inner_w), px(row.height)),
                                ),
                            };
                            let line_paints: Vec<_> = cell.lines.iter().cloned().collect();
                            window.with_content_mask(Some(cell_mask), |window| {
                                let mut line_y = row_top + px(6.0);
                                for shaped in &line_paints {
                                    let _ = shaped.paint(
                                        point(cell_x, line_y),
                                        px(line_h),
                                        window, cx,
                                    );
                                    line_y += px(line_h);
                                }
                            });
                        }
                    }
                }
            }
        }

        // Paint cursor
        if let Some(cursor) = &state.cursor {
            window.paint_quad(cursor.clone());
        }

        // Paint autocomplete popup (keyboard-driven, no mouse interaction for v1).
        if let Some(popup) = &state.autocomplete_popup {
            let is_dark = cx.theme().mode.is_dark();
            let fg = window.text_style().color;
            let muted = hsla(0., 0., 0.5, 1.0);
            let popup_bg = if is_dark { hsla(0., 0., 0.14, 1.0) } else { hsla(0., 0., 1.0, 1.0) };
            let border = if is_dark { hsla(0., 0., 1., 0.16) } else { hsla(0., 0., 0., 0.16) };
            let selected_bg = if is_dark { hsla(210. / 360., 0.5, 0.35, 0.4) } else { hsla(210. / 360., 0.7, 0.55, 0.16) };

            let row_h = px(36.);
            let popup_h = row_h * popup.rows.len() as f32;
            let popup_bounds = Bounds::new(popup.origin, size(popup.width, popup_h));

            // Background + border
            window.paint_quad(fill(popup_bounds, popup_bg));
            // Border (four thin quads)
            let bw = px(1.);
            window.paint_quad(fill(Bounds::new(popup.origin, size(popup.width, bw)), border));
            window.paint_quad(fill(Bounds::new(point(popup.origin.x, popup.origin.y + popup_h - bw), size(popup.width, bw)), border));
            window.paint_quad(fill(Bounds::new(popup.origin, size(bw, popup_h)), border));
            window.paint_quad(fill(Bounds::new(point(popup.origin.x + popup.width - bw, popup.origin.y), size(bw, popup_h)), border));

            let font = window.text_style().font();
            let basename_font_size = px(13.);
            let rel_font_size = px(11.);
            let pad_x = px(10.);
            let basename_y_offset = px(6.);
            let rel_y_offset = px(20.);
            let max_rel_chars = 40;

            // Shape all rows first (immutable window borrow), then paint.
            let shaped_rows: Vec<(ShapedLine, ShapedLine, bool)> = {
                let text_system = window.text_system();
                popup.rows.iter().map(|row| {
                    let basename_runs = vec![TextRun {
                        len: row.basename.len(), font: font.clone(), color: fg,
                        background_color: None, underline: None, strikethrough: None,
                    }];
                    let basename_shaped = text_system.shape_line(
                        row.basename.clone().into(), basename_font_size, &basename_runs, None,
                    );
                    let rel_display = if row.rel_path.chars().count() > max_rel_chars {
                        let tail: String = row.rel_path.chars().rev().take(max_rel_chars - 1).collect::<Vec<_>>().into_iter().rev().collect();
                        format!("…{}", tail)
                    } else {
                        row.rel_path.clone()
                    };
                    let rel_runs = vec![TextRun {
                        len: rel_display.len(), font: font.clone(), color: muted,
                        background_color: None, underline: None, strikethrough: None,
                    }];
                    let rel_shaped = text_system.shape_line(
                        rel_display.into(), rel_font_size, &rel_runs, None,
                    );
                    (basename_shaped, rel_shaped, row.selected)
                }).collect()
            };

            for (i, (basename_shaped, rel_shaped, selected)) in shaped_rows.into_iter().enumerate() {
                let row_origin = point(popup.origin.x, popup.origin.y + row_h * i as f32);
                if selected {
                    window.paint_quad(fill(
                        Bounds::new(row_origin, size(popup.width, row_h)),
                        selected_bg,
                    ));
                }
                let _ = basename_shaped.paint(
                    point(row_origin.x + pad_x, row_origin.y + basename_y_offset),
                    px(18.), window, cx,
                );
                let _ = rel_shaped.paint(
                    point(row_origin.x + pad_x, row_origin.y + rel_y_offset),
                    px(14.), window, cx,
                );
            }
        }

    }
}

impl Render for Editor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex().flex_col().size_full()
            .key_context("Editor")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            // Movement
            .on_action(cx.listener(Self::on_move_left))
            .on_action(cx.listener(Self::on_move_right))
            .on_action(cx.listener(Self::on_move_up))
            .on_action(cx.listener(Self::on_move_down))
            .on_action(cx.listener(Self::on_move_word_left))
            .on_action(cx.listener(Self::on_move_word_right))
            .on_action(cx.listener(Self::on_move_home))
            .on_action(cx.listener(Self::on_move_end))
            .on_action(cx.listener(Self::on_page_up))
            .on_action(cx.listener(Self::on_page_down))
            .on_action(cx.listener(Self::on_move_doc_start))
            .on_action(cx.listener(Self::on_move_doc_end))
            // Selection
            .on_action(cx.listener(Self::on_select_left))
            .on_action(cx.listener(Self::on_select_right))
            .on_action(cx.listener(Self::on_select_up))
            .on_action(cx.listener(Self::on_select_down))
            .on_action(cx.listener(Self::on_select_word_left))
            .on_action(cx.listener(Self::on_select_word_right))
            .on_action(cx.listener(Self::on_select_home))
            .on_action(cx.listener(Self::on_select_end))
            .on_action(cx.listener(Self::on_select_all))
            .on_action(cx.listener(Self::on_select_line))
            // Editing
            .on_action(cx.listener(Self::on_backspace))
            .on_action(cx.listener(Self::on_delete))
            .on_action(cx.listener(Self::on_backspace_word))
            .on_action(cx.listener(Self::on_delete_word))
            .on_action(cx.listener(Self::on_enter))
            .on_action(cx.listener(Self::on_indent))
            .on_action(cx.listener(Self::on_dedent))
            .on_action(cx.listener(Self::on_copy))
            .on_action(cx.listener(Self::on_cut))
            .on_action(cx.listener(Self::on_paste))
            .on_action(cx.listener(Self::on_undo))
            .on_action(cx.listener(Self::on_redo))
            .on_action(cx.listener(Self::on_duplicate_line))
            // Formatting
            .on_action(cx.listener(Self::on_toggle_bold))
            .on_action(cx.listener(Self::on_toggle_italic))
            .on_action(cx.listener(Self::on_toggle_code))
            .on_action(cx.listener(Self::on_toggle_strikethrough))
            // Insertions
            .on_action(cx.listener(Self::on_insert_h1))
            .on_action(cx.listener(Self::on_insert_h2))
            .on_action(cx.listener(Self::on_insert_h3))
            .on_action(cx.listener(Self::on_insert_bullet))
            .on_action(cx.listener(Self::on_insert_numbered))
            .on_action(cx.listener(Self::on_insert_table))
            .on_action(cx.listener(Self::on_insert_code_block))
            .on_action(cx.listener(Self::on_insert_hr))
            .on_action(cx.listener(Self::on_toggle_read_mode))
            .on_action(cx.listener(Self::on_autocomplete_cancel))
            .on_action(cx.listener(Self::on_zoom_in))
            .on_action(cx.listener(Self::on_zoom_out))
            .on_action(cx.listener(Self::on_zoom_reset))
            // Mouse
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let lh = this.line_h();
                let delta = match event.delta {
                    ScrollDelta::Lines(lines) => -lines.y * lh * SCROLL_LINES,
                    ScrollDelta::Pixels(d) => { let y: f32 = d.y.into(); -y }
                };
                this.scroll_by(delta, cx);
            }))
            .text_size(px(15. * self.zoom))
            .line_height(px(self.line_h()))
            .text_color(cx.theme().foreground)
            .overflow_hidden()
            .child(EditorElement { editor: cx.entity() })
    }
}

impl EventEmitter<EditorEvent> for Editor {}

// ── Image loading helpers ──

/// Decode an image file into a gpui `RenderImage`. Returns None on failure.
/// The image crate reads the format by extension/magic bytes; we convert to
/// BGRA (what the GPU uploader expects).
fn decode_image(path: &Path) -> Option<(Arc<RenderImage>, u32, u32)> {
    let bytes = std::fs::read(path).ok()?;
    let format = image::guess_format(&bytes).ok()?;
    let dyn_img = image::load_from_memory_with_format(&bytes, format).ok()?;
    let mut rgba = dyn_img.into_rgba8();
    let (w, h) = rgba.dimensions();
    // RGBA -> BGRA for the GPU sprite atlas.
    for pixel in rgba.chunks_exact_mut(4) { pixel.swap(0, 2); }
    let frame = image::Frame::new(rgba);
    let frames = smallvec::SmallVec::<[image::Frame; 1]>::from_elem(frame, 1);
    let render_image = Arc::new(RenderImage::new(frames));
    Some((render_image, w, h))
}

/// Recursive walk; return the first file whose file_name matches `basename`
/// (case-sensitive). Skips hidden directories. Bounded search depth.
fn find_file_by_basename(root: &Path, basename: &str) -> Option<PathBuf> {
    fn walk(dir: &Path, target: &str, depth: u32) -> Option<PathBuf> {
        if depth > 8 { return None; }
        let entries = std::fs::read_dir(dir).ok()?;
        let mut subdirs: Vec<PathBuf> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') { continue; }
                if path.is_file() && name == target { return Some(path); }
                if path.is_dir() { subdirs.push(path); }
            }
        }
        for sub in subdirs {
            if let Some(found) = walk(&sub, target, depth + 1) { return Some(found); }
        }
        None
    }
    walk(root, basename, 0)
}
