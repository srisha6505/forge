//! Rope-based text buffer with cursor, selection, undo/redo.
//! This is the core editing engine -- no rendering, no GPUI, pure data.

use ropey::Rope;
use std::ops::Range;
use std::time::Instant;

/// How long between keystrokes before a new undo group starts.
const UNDO_GROUP_TIMEOUT_MS: u128 = 300;

/// A single edit operation, reversible.
#[derive(Clone, Debug)]
struct Edit {
    offset: usize,
    deleted: String,
    inserted: String,
}

impl Edit {
    fn reverse(&self) -> Edit {
        Edit {
            offset: self.offset,
            deleted: self.inserted.clone(),
            inserted: self.deleted.clone(),
        }
    }
}

/// A group of edits that undo/redo together.
#[derive(Clone, Debug)]
struct EditGroup {
    edits: Vec<Edit>,
    cursor_before: usize,
    cursor_after: usize,
}

/// Selection: anchor is where the selection started, head is where the cursor is.
#[derive(Clone, Debug)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    pub fn cursor(offset: usize) -> Self {
        Self { anchor: offset, head: offset }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    pub fn range(&self) -> Range<usize> {
        let start = self.anchor.min(self.head);
        let end = self.anchor.max(self.head);
        start..end
    }

    pub fn start(&self) -> usize {
        self.anchor.min(self.head)
    }

    pub fn end(&self) -> usize {
        self.anchor.max(self.head)
    }
}

pub struct Buffer {
    rope: Rope,
    selection: Selection,
    // Undo/redo stacks
    undo_stack: Vec<EditGroup>,
    redo_stack: Vec<EditGroup>,
    last_edit_time: Instant,
    /// Set to true after any modification
    dirty: bool,
}

impl Buffer {
    pub fn new() -> Self {
        Self {
            rope: Rope::new(),
            selection: Selection::cursor(0),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_edit_time: Instant::now(),
            dirty: false,
        }
    }

    pub fn from_str(s: &str) -> Self {
        let rope = Rope::from_str(s);
        Self {
            rope,
            selection: Selection::cursor(0),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_edit_time: Instant::now(),
            dirty: false,
        }
    }

    // ── Accessors ──

    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn len_bytes(&self) -> usize {
        self.rope.len_bytes()
    }

    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn line(&self, idx: usize) -> ropey::RopeSlice<'_> {
        self.rope.line(idx)
    }

    pub fn line_to_byte(&self, line: usize) -> usize {
        self.rope.line_to_byte(line)
    }

    pub fn byte_to_line(&self, byte: usize) -> usize {
        self.rope.byte_to_line(byte)
    }

    pub fn line_str(&self, line: usize) -> String {
        let s = self.rope.line(line).to_string();
        // Strip trailing newline if present
        if s.ends_with('\n') { s[..s.len()-1].to_string() } else { s }
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    pub fn cursor(&self) -> usize {
        self.selection.head
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    // ── Cursor movement ──

    pub fn set_cursor(&mut self, offset: usize) {
        let offset = offset.min(self.rope.len_bytes());
        self.selection = Selection::cursor(offset);
    }

    pub fn set_selection(&mut self, anchor: usize, head: usize) {
        let anchor = anchor.min(self.rope.len_bytes());
        let head = head.min(self.rope.len_bytes());
        self.selection = Selection { anchor, head };
    }

    pub fn select_to(&mut self, head: usize) {
        let head = head.min(self.rope.len_bytes());
        self.selection.head = head;
    }

    pub fn select_all(&mut self) {
        self.selection.anchor = 0;
        self.selection.head = self.rope.len_bytes();
    }

    /// Move cursor left by one grapheme cluster.
    pub fn move_left(&mut self) {
        if !self.selection.is_empty() {
            self.set_cursor(self.selection.start());
        } else if self.selection.head > 0 {
            self.set_cursor(self.prev_grapheme(self.selection.head));
        }
    }

    /// Move cursor right by one grapheme cluster.
    pub fn move_right(&mut self) {
        if !self.selection.is_empty() {
            self.set_cursor(self.selection.end());
        } else {
            self.set_cursor(self.next_grapheme(self.selection.head));
        }
    }

    pub fn move_up(&mut self) {
        let (line, col) = self.cursor_line_col();
        if line > 0 {
            self.set_cursor(self.line_col_to_byte(line - 1, col));
        } else {
            self.set_cursor(0);
        }
    }

    pub fn move_down(&mut self) {
        let (line, col) = self.cursor_line_col();
        if line + 1 < self.rope.len_lines() {
            self.set_cursor(self.line_col_to_byte(line + 1, col));
        } else {
            self.set_cursor(self.rope.len_bytes());
        }
    }

    pub fn move_home(&mut self) {
        let (line, _) = self.cursor_line_col();
        self.set_cursor(self.rope.line_to_byte(line));
    }

    pub fn move_end(&mut self) {
        let (line, _) = self.cursor_line_col();
        let line_start = self.rope.line_to_byte(line);
        let line_len = self.line_byte_len(line);
        self.set_cursor(line_start + line_len);
    }

    pub fn move_word_left(&mut self) {
        self.set_cursor(self.word_boundary_left(self.selection.head));
    }

    pub fn move_word_right(&mut self) {
        self.set_cursor(self.word_boundary_right(self.selection.head));
    }

    // ── Selection extensions ──

    pub fn select_left(&mut self) {
        if self.selection.head > 0 {
            self.selection.head = self.prev_grapheme(self.selection.head);
        }
    }

    pub fn select_right(&mut self) {
        self.selection.head = self.next_grapheme(self.selection.head);
    }

    pub fn select_up(&mut self) {
        let (line, col) = self.head_line_col();
        if line > 0 {
            self.selection.head = self.line_col_to_byte(line - 1, col);
        } else {
            self.selection.head = 0;
        }
    }

    pub fn select_down(&mut self) {
        let (line, col) = self.head_line_col();
        if line + 1 < self.rope.len_lines() {
            self.selection.head = self.line_col_to_byte(line + 1, col);
        } else {
            self.selection.head = self.rope.len_bytes();
        }
    }

    pub fn select_home(&mut self) {
        let (line, _) = self.head_line_col();
        self.selection.head = self.rope.line_to_byte(line);
    }

    pub fn select_end(&mut self) {
        let (line, _) = self.head_line_col();
        let line_start = self.rope.line_to_byte(line);
        self.selection.head = line_start + self.line_byte_len(line);
    }

    pub fn select_word_left(&mut self) {
        self.selection.head = self.word_boundary_left(self.selection.head);
    }

    pub fn select_word_right(&mut self) {
        self.selection.head = self.word_boundary_right(self.selection.head);
    }

    pub fn select_word_at(&mut self, offset: usize) {
        let left = self.word_boundary_left(offset);
        let right = self.word_boundary_right(offset);
        self.selection = Selection { anchor: left, head: right };
    }

    pub fn select_line_at(&mut self, offset: usize) {
        let line = self.rope.byte_to_line(offset.min(self.rope.len_bytes()));
        let start = self.rope.line_to_byte(line);
        let end = if line + 1 < self.rope.len_lines() {
            self.rope.line_to_byte(line + 1)
        } else {
            self.rope.len_bytes()
        };
        self.selection = Selection { anchor: start, head: end };
    }

    // ── Editing ──

    /// Insert text, replacing selection if any.
    pub fn insert(&mut self, text: &str) {
        let range = self.selection.range();
        self.edit(range, text);
    }

    /// Delete selection, or one char before cursor (backspace).
    pub fn backspace(&mut self) {
        if self.selection.is_empty() {
            let prev = self.prev_grapheme(self.selection.head);
            if prev == self.selection.head { return; }
            self.selection = Selection { anchor: prev, head: self.selection.head };
        }
        self.edit(self.selection.range(), "");
    }

    /// Delete selection, or one char after cursor.
    pub fn delete(&mut self) {
        if self.selection.is_empty() {
            let next = self.next_grapheme(self.selection.head);
            if next == self.selection.head { return; }
            self.selection = Selection { anchor: self.selection.head, head: next };
        }
        self.edit(self.selection.range(), "");
    }

    /// Delete word before cursor (Ctrl+Backspace).
    pub fn backspace_word(&mut self) {
        if self.selection.is_empty() {
            let target = self.word_boundary_left(self.selection.head);
            self.selection = Selection { anchor: target, head: self.selection.head };
        }
        self.edit(self.selection.range(), "");
    }

    /// Delete word after cursor (Ctrl+Delete).
    pub fn delete_word(&mut self) {
        if self.selection.is_empty() {
            let target = self.word_boundary_right(self.selection.head);
            self.selection = Selection { anchor: self.selection.head, head: target };
        }
        self.edit(self.selection.range(), "");
    }

    /// Smart enter: auto-continue lists, preserve indent.
    pub fn enter(&mut self) {
        let (line, _) = self.cursor_line_col();
        let line_text = self.line_str(line);
        let trimmed = line_text.trim_start();
        let indent: String = line_text.chars().take_while(|c| c.is_whitespace()).collect();

        // Continue bullet lists
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
            if trimmed.len() <= 2 {
                // Empty item -- remove it
                let ls = self.rope.line_to_byte(line);
                let le = ls + self.line_byte_len(line);
                self.edit(ls..le, "");
                return;
            }
            let bullet = &trimmed[..2];
            self.insert(&format!("\n{}{}", indent, bullet));
            return;
        }

        // Continue numbered lists
        if let Some(dot_pos) = trimmed.find(". ") {
            if let Ok(n) = trimmed[..dot_pos].parse::<u64>() {
                if trimmed.len() <= dot_pos + 2 {
                    let ls = self.rope.line_to_byte(line);
                    let le = ls + self.line_byte_len(line);
                    self.edit(ls..le, "");
                    return;
                }
                self.insert(&format!("\n{}{}. ", indent, n + 1));
                return;
            }
        }

        // Preserve indent
        if indent.is_empty() {
            self.insert("\n");
        } else {
            self.insert(&format!("\n{}", indent));
        }
    }

    /// Indent current line (Tab).
    pub fn indent(&mut self) {
        let (line, _) = self.cursor_line_col();
        let ls = self.rope.line_to_byte(line);
        let cursor = self.selection.head;
        self.set_cursor(ls);
        self.insert("    ");
        self.set_cursor(cursor + 4);
    }

    /// Dedent current line (Shift+Tab).
    pub fn dedent(&mut self) {
        let (line, _) = self.cursor_line_col();
        let ls = self.rope.line_to_byte(line);
        let line_text = self.line_str(line);
        let spaces = line_text.bytes().take_while(|b| *b == b' ').count().min(4);
        if spaces == 0 { return; }
        let cursor = self.selection.head;
        self.edit(ls..ls + spaces, "");
        self.set_cursor(cursor.saturating_sub(spaces));
    }

    /// Duplicate current line below.
    pub fn duplicate_line(&mut self) {
        let (line, _) = self.cursor_line_col();
        let line_text = self.line_str(line);
        let ls = self.rope.line_to_byte(line);
        let le = ls + self.line_byte_len(line);
        let insert_text = format!("\n{}", line_text);
        let cursor = self.selection.head;
        self.set_cursor(le);
        self.insert(&insert_text);
        self.set_cursor(cursor + insert_text.len());
    }

    /// Wrap selection with markers (toggle bold, italic, etc.)
    pub fn toggle_wrap(&mut self, marker: &str) {
        let range = self.selection.range();
        let m_len = marker.len();

        if range.is_empty() {
            let insert = format!("{}{}", marker, marker);
            self.insert(&insert);
            self.set_cursor(range.start + m_len);
            return;
        }

        let text: String = self.rope.slice(range.clone()).into();

        // Case 1: the selection itself is wrapped: "**hello**"
        if text.len() > m_len * 2 && text.starts_with(marker) && text.ends_with(marker) {
            let inner = text[m_len..text.len() - m_len].to_string();
            let inner_len = inner.len();
            self.edit(range.clone(), &inner);
            self.set_selection(range.start, range.start + inner_len);
            return;
        }

        // Case 2: markers surround the selection: **[hello]** (selection is just "hello")
        if range.start >= m_len && range.end + m_len <= self.rope.len_bytes() {
            let before: String = self.rope.slice(range.start - m_len..range.start).into();
            let after: String = self.rope.slice(range.end..range.end + m_len).into();
            if before == marker && after == marker {
                // Unwrap by removing surrounding markers
                self.edit(range.end..range.end + m_len, "");
                self.edit(range.start - m_len..range.start, "");
                self.set_selection(range.start - m_len, range.end - m_len);
                return;
            }
        }

        // Case 3: wrap
        let wrapped = format!("{}{}{}", marker, text, marker);
        let content_len = text.len();
        self.edit(range.clone(), &wrapped);
        self.set_selection(range.start + m_len, range.start + m_len + content_len);
    }

    /// Insert text at start of current line.
    pub fn insert_at_line_start(&mut self, prefix: &str) {
        let (line, _) = self.cursor_line_col();
        let ls = self.rope.line_to_byte(line);
        let cursor = self.selection.head;
        self.set_cursor(ls);
        self.insert(prefix);
        self.set_cursor(cursor + prefix.len());
    }

    // ── Undo/Redo ──

    pub fn undo(&mut self) {
        if let Some(group) = self.undo_stack.pop() {
            // Apply reversed edits in reverse order
            for edit in group.edits.iter().rev() {
                let rev = edit.reverse();
                self.apply_edit_raw(&rev);
            }
            self.set_cursor(group.cursor_before);
            self.redo_stack.push(group);
        }
    }

    pub fn redo(&mut self) {
        if let Some(group) = self.redo_stack.pop() {
            for edit in &group.edits {
                self.apply_edit_raw(edit);
            }
            self.set_cursor(group.cursor_after);
            self.undo_stack.push(group);
        }
    }

    // ── Clipboard helpers ──

    pub fn selected_text(&self) -> Option<String> {
        if self.selection.is_empty() { return None; }
        let range = self.selection.range();
        Some(self.rope.slice(range).to_string())
    }

    // ── Internal ──

    /// Core edit operation: replace byte range with new text.
    fn edit(&mut self, range: Range<usize>, text: &str) {
        let deleted: String = self.rope.slice(range.clone()).into();
        let edit = Edit {
            offset: range.start,
            deleted,
            inserted: text.to_string(),
        };

        let cursor_before = self.selection.head;
        self.apply_edit_raw(&edit);
        let cursor_after = range.start + text.len();
        self.set_cursor(cursor_after);

        // Group edits by time
        let now = Instant::now();
        if now.duration_since(self.last_edit_time).as_millis() < UNDO_GROUP_TIMEOUT_MS {
            if let Some(group) = self.undo_stack.last_mut() {
                group.edits.push(edit);
                group.cursor_after = cursor_after;
                self.last_edit_time = now;
                self.redo_stack.clear();
                return;
            }
        }

        self.undo_stack.push(EditGroup {
            edits: vec![edit],
            cursor_before,
            cursor_after,
        });
        if self.undo_stack.len() > 200 { self.undo_stack.remove(0); }
        self.redo_stack.clear();
        self.last_edit_time = now;
    }

    /// Apply an edit directly to the rope without recording undo.
    fn apply_edit_raw(&mut self, edit: &Edit) {
        if !edit.deleted.is_empty() {
            let end = edit.offset + edit.deleted.len();
            self.rope.remove(edit.offset..end);
        }
        if !edit.inserted.is_empty() {
            self.rope.insert(edit.offset, &edit.inserted);
        }
        self.dirty = true;
    }

    // ── Position helpers ──

    pub fn cursor_line_col(&self) -> (usize, usize) {
        let byte = self.selection.head.min(self.rope.len_bytes());
        let line = self.rope.byte_to_line(byte);
        let line_start = self.rope.line_to_byte(line);
        (line, byte - line_start)
    }

    fn head_line_col(&self) -> (usize, usize) {
        let byte = self.selection.head.min(self.rope.len_bytes());
        let line = self.rope.byte_to_line(byte);
        let line_start = self.rope.line_to_byte(line);
        (line, byte - line_start)
    }

    fn line_col_to_byte(&self, line: usize, col: usize) -> usize {
        let line = line.min(self.rope.len_lines().saturating_sub(1));
        let line_start = self.rope.line_to_byte(line);
        let line_len = self.line_byte_len(line);
        line_start + col.min(line_len)
    }

    fn line_byte_len(&self, line: usize) -> usize {
        let s = self.rope.line(line).to_string();
        if s.ends_with('\n') { s.len() - 1 } else { s.len() }
    }

    fn prev_grapheme(&self, offset: usize) -> usize {
        if offset == 0 { return 0; }
        // Simple: go back one byte, then back up to char boundary
        let s = self.rope.slice(..offset).to_string();
        let mut idx = s.len();
        while idx > 0 {
            idx -= 1;
            if s.is_char_boundary(idx) { return idx; }
        }
        0
    }

    fn next_grapheme(&self, offset: usize) -> usize {
        let len = self.rope.len_bytes();
        if offset >= len { return len; }
        let s = self.rope.slice(offset..).to_string();
        for (i, _) in s.char_indices().skip(1) {
            return offset + i;
        }
        len
    }

    fn word_boundary_left(&self, offset: usize) -> usize {
        if offset == 0 { return 0; }
        let s = self.rope.slice(..offset).to_string();
        let bytes = s.as_bytes();
        let mut pos = bytes.len();
        // Skip whitespace backwards
        while pos > 0 && bytes[pos - 1].is_ascii_whitespace() { pos -= 1; }
        // Skip word chars backwards
        while pos > 0 && !bytes[pos - 1].is_ascii_whitespace() { pos -= 1; }
        pos
    }

    fn word_boundary_right(&self, offset: usize) -> usize {
        let len = self.rope.len_bytes();
        if offset >= len { return len; }
        let s = self.rope.slice(offset..).to_string();
        let bytes = s.as_bytes();
        let mut pos = 0;
        // Skip word chars forward
        while pos < bytes.len() && !bytes[pos].is_ascii_whitespace() { pos += 1; }
        // Skip whitespace forward
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() { pos += 1; }
        offset + pos
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert() {
        let mut buf = Buffer::from_str("Hello world");
        buf.set_cursor(5);
        buf.insert(" beautiful");
        assert_eq!(buf.text(), "Hello beautiful world");
        assert_eq!(buf.cursor(), 15);
    }

    #[test]
    fn test_backspace() {
        let mut buf = Buffer::from_str("Hello");
        buf.set_cursor(5);
        buf.backspace();
        assert_eq!(buf.text(), "Hell");
    }

    #[test]
    fn test_selection_delete() {
        let mut buf = Buffer::from_str("Hello world");
        buf.set_selection(5, 11);
        buf.insert("!");
        assert_eq!(buf.text(), "Hello!");
    }

    #[test]
    fn test_undo() {
        let mut buf = Buffer::from_str("Hello");
        buf.set_cursor(5);
        // Force new undo group by setting last_edit_time far in the past
        buf.last_edit_time = Instant::now() - std::time::Duration::from_secs(10);
        buf.insert(" world");
        assert_eq!(buf.text(), "Hello world");
        buf.undo();
        assert_eq!(buf.text(), "Hello");
    }

    #[test]
    fn test_line_operations() {
        let buf = Buffer::from_str("line one\nline two\nline three");
        assert_eq!(buf.len_lines(), 3);
        assert_eq!(buf.line_str(0), "line one");
        assert_eq!(buf.line_str(1), "line two");
        assert_eq!(buf.byte_to_line(10), 1);
    }

    #[test]
    fn test_word_movement() {
        let mut buf = Buffer::from_str("hello world foo");
        buf.set_cursor(0);
        buf.move_word_right();
        assert_eq!(buf.cursor(), 6); // after "hello "
        buf.move_word_right();
        assert_eq!(buf.cursor(), 12); // after "world "
    }
}
