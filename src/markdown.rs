//! Markdown parser that produces per-line styling info.
//! Uses pulldown-cmark, walks events, builds an index of blocks and spans.

use std::ops::Range;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// What kind of block a line belongs to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BlockType {
    Paragraph,
    Heading(u32),        // 1-6
    ListItem,
    CodeBlock,
    BlockQuote,
    HorizontalRule,
    Table,
    /// `$$...$$` block (multi-line math).
    MathBlock,
    /// Whole-line image embed: `![[img.png]]` or `![alt](url)`.
    ImageEmbed,
}

/// Per-line block info.
#[derive(Clone, Debug)]
pub struct BlockInfo {
    pub block_type: BlockType,
    /// Length of syntax prefix to visually de-emphasize:
    /// "## " = 3, "- " = 2, "1. " = 3, "> " = 2, 0 for other blocks
    pub prefix_len: usize,
}

impl Default for BlockInfo {
    fn default() -> Self {
        Self { block_type: BlockType::Paragraph, prefix_len: 0 }
    }
}

/// Inline span style.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpanStyle {
    Bold,
    Italic,
    BoldItalic,
    InlineCode,
    Strikethrough,
    Link,
    /// Inline math: `$...$`
    Math,
}

/// An inline span within a line (byte range is relative to line start).
#[derive(Clone, Debug)]
pub struct InlineSpan {
    pub range: Range<usize>,   // byte offsets within the line
    pub style: SpanStyle,
}

/// A wikilink `[[Target]]`, `[[Target|alias]]`, or `[[Target#heading]]`.
#[derive(Clone, Debug)]
pub struct WikiLink {
    /// Full byte range within the line, INCLUDING the surrounding `[[` and `]]`.
    pub range: Range<usize>,
    /// The target note basename (text before `|`, before `#`).
    pub target: String,
    /// Optional heading anchor (text after `#`, before `|`).
    pub heading: Option<String>,
    /// Optional display alias (text after `|`).
    pub alias: Option<String>,
}

/// Per-line annotations.
#[derive(Clone, Debug, Default)]
pub struct LineInfo {
    pub block: BlockInfo,
    pub spans: Vec<InlineSpan>,
    pub wikilinks: Vec<WikiLink>,
}

/// Parse the full text and return per-line info.
/// Uses line offsets provided by the caller to map byte ranges to line+col.
pub fn parse_lines(text: &str, line_starts: &[usize]) -> Vec<LineInfo> {
    let n_lines = line_starts.len();
    let mut lines: Vec<LineInfo> = (0..n_lines).map(|_| LineInfo::default()).collect();

    let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(text, opts);

    // Stack of (SpanStyle, start_byte_offset) for inline styles
    let mut style_stack: Vec<(SpanStyle, usize)> = Vec::new();
    let mut in_table = false;
    let mut in_code_block = false;
    let mut code_block_start_line: Option<usize> = None;

    for (event, range) in parser.into_offset_iter() {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    let line = line_for_offset(line_starts, range.start);
                    if line < n_lines {
                        lines[line].block.block_type = BlockType::Heading(level as u32);
                        // prefix = number of # + 1 space
                        let line_str = line_str(text, line_starts, line);
                        lines[line].block.prefix_len = count_heading_prefix(line_str);
                    }
                }
                Tag::Item => {
                    let line = line_for_offset(line_starts, range.start);
                    if line < n_lines {
                        lines[line].block.block_type = BlockType::ListItem;
                        let line_str = line_str(text, line_starts, line);
                        lines[line].block.prefix_len = count_list_prefix(line_str);
                    }
                }
                Tag::BlockQuote(_) => {
                    let start_line = line_for_offset(line_starts, range.start);
                    let end_line = line_for_offset(line_starts, range.end.saturating_sub(1).max(range.start));
                    for i in start_line..=end_line.min(n_lines - 1) {
                        if lines[i].block.block_type == BlockType::Paragraph {
                            lines[i].block.block_type = BlockType::BlockQuote;
                            let ls = line_str(text, line_starts, i);
                            lines[i].block.prefix_len = count_blockquote_prefix(ls);
                        }
                    }
                }
                Tag::CodeBlock(_) => {
                    in_code_block = true;
                    code_block_start_line = Some(line_for_offset(line_starts, range.start));
                    // Mark all lines from range.start to range.end as CodeBlock
                    let start_line = line_for_offset(line_starts, range.start);
                    let end_line = line_for_offset(line_starts, range.end.saturating_sub(1).max(range.start));
                    for i in start_line..=end_line.min(n_lines - 1) {
                        lines[i].block.block_type = BlockType::CodeBlock;
                    }
                }
                Tag::Table(_) => {
                    in_table = true;
                    let start_line = line_for_offset(line_starts, range.start);
                    let end_line = line_for_offset(line_starts, range.end.saturating_sub(1).max(range.start));
                    for i in start_line..=end_line.min(n_lines - 1) {
                        lines[i].block.block_type = BlockType::Table;
                    }
                }
                Tag::Strong => style_stack.push((SpanStyle::Bold, range.start)),
                Tag::Emphasis => style_stack.push((SpanStyle::Italic, range.start)),
                Tag::Strikethrough => style_stack.push((SpanStyle::Strikethrough, range.start)),
                Tag::Link { .. } => style_stack.push((SpanStyle::Link, range.start)),
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough | TagEnd::Link => {
                    if let Some((style, span_start)) = style_stack.pop() {
                        let span_end = range.end;
                        // Check for nested bold+italic
                        let effective = if (style == SpanStyle::Italic && style_stack.iter().any(|(s, _)| *s == SpanStyle::Bold))
                            || (style == SpanStyle::Bold && style_stack.iter().any(|(s, _)| *s == SpanStyle::Italic))
                        {
                            SpanStyle::BoldItalic
                        } else {
                            style
                        };
                        // Map to line-local ranges
                        add_span_to_lines(&mut lines, line_starts, text, span_start, span_end, effective);
                    }
                }
                TagEnd::CodeBlock => { in_code_block = false; code_block_start_line = None; }
                TagEnd::Table => { in_table = false; }
                _ => {}
            },
            Event::Code(_) => {
                // Inline code: range includes backticks
                add_span_to_lines(&mut lines, line_starts, text, range.start, range.end, SpanStyle::InlineCode);
            }
            Event::Rule => {
                let line = line_for_offset(line_starts, range.start);
                if line < n_lines {
                    lines[line].block.block_type = BlockType::HorizontalRule;
                }
            }
            _ => {}
        }
    }

    let _ = in_table;
    let _ = in_code_block;
    let _ = code_block_start_line;

    // Detect $$ math blocks (lines whose trimmed text is "$$" toggle the block).
    // Math block lines are only marked where they aren't already code block lines.
    {
        let mut in_math = false;
        for i in 0..n_lines {
            if lines[i].block.block_type == BlockType::CodeBlock { continue; }
            let ls = line_str(text, line_starts, i).trim();
            if ls == "$$" {
                // Fence line: mark as math block and toggle state.
                lines[i].block.block_type = BlockType::MathBlock;
                in_math = !in_math;
            } else if in_math {
                lines[i].block.block_type = BlockType::MathBlock;
            }
        }
    }

    // Detect whole-line image embeds: `![[img.png]]` or `![alt](url)`.
    // Only when the trimmed line is exactly one embed (no surrounding text).
    for i in 0..n_lines {
        match lines[i].block.block_type {
            BlockType::CodeBlock | BlockType::MathBlock | BlockType::Table => continue,
            _ => {}
        }
        let ls = line_str(text, line_starts, i).trim();
        if is_image_embed_line(ls) {
            lines[i].block.block_type = BlockType::ImageEmbed;
        }
    }

    // Second pass: scan each non-code/math line for wikilinks + inline math.
    for (i, info) in lines.iter_mut().enumerate() {
        if matches!(info.block.block_type, BlockType::CodeBlock | BlockType::MathBlock | BlockType::ImageEmbed) { continue; }
        let line_text = line_str(text, line_starts, i);
        let found = scan_wikilinks(line_text);
        info.wikilinks = found.into_iter().filter(|w| {
            !info.spans.iter().any(|s| {
                s.style == SpanStyle::InlineCode
                    && s.range.start < w.range.end
                    && w.range.start < s.range.end
            })
        }).collect();
        // Scan for inline math $...$  (single dollar, not $$)
        let bytes = line_text.as_bytes();
        let mut j = 0;
        while j < bytes.len() {
            if bytes[j] == b'$' && (j + 1 >= bytes.len() || bytes[j + 1] != b'$') {
                let start = j;
                j += 1;
                while j < bytes.len() && bytes[j] != b'$' { j += 1; }
                if j < bytes.len() && j > start + 1 {
                    // Don't add if overlapping with InlineCode span
                    let range = start..j + 1;
                    let overlaps_code = info.spans.iter().any(|s| {
                        s.style == SpanStyle::InlineCode && s.range.start < range.end && range.start < s.range.end
                    });
                    if !overlaps_code {
                        info.spans.push(InlineSpan { range, style: SpanStyle::Math });
                    }
                    j += 1;
                }
            } else {
                j += 1;
            }
        }
    }

    lines
}

/// True if `trimmed` is exactly one whole-line image embed.
/// Recognizes `![[file.ext]]` (Obsidian) and `![alt](url)` (CommonMark).
fn is_image_embed_line(trimmed: &str) -> bool {
    let exts = [".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".bmp"];
    // Obsidian-style: `![[something]]` where something ends with an image extension.
    if let Some(inner) = trimmed.strip_prefix("![[").and_then(|s| s.strip_suffix("]]")) {
        let path_part = inner.split('|').next().unwrap_or("");
        let lower = path_part.to_ascii_lowercase();
        return exts.iter().any(|e| lower.ends_with(e));
    }
    // Markdown-style: `![alt](url)` whole-line.
    if trimmed.starts_with("![") {
        if let Some(close_bracket) = trimmed.find("](") {
            if let Some(close_paren) = trimmed[close_bracket + 2..].rfind(')') {
                let end = close_bracket + 2 + close_paren + 1;
                if end == trimmed.len() {
                    let url = &trimmed[close_bracket + 2..end - 1];
                    let lower = url.to_ascii_lowercase();
                    // Image if URL ends with image extension OR starts with data:image
                    return exts.iter().any(|e| lower.contains(e)) || lower.starts_with("data:image");
                }
            }
        }
    }
    false
}

/// Extract the image path/target from an image embed line.
/// Returns None if not a valid embed line.
pub fn parse_image_embed(trimmed: &str) -> Option<String> {
    if let Some(inner) = trimmed.strip_prefix("![[").and_then(|s| s.strip_suffix("]]")) {
        let path_part = inner.split('|').next().unwrap_or("").trim();
        if !path_part.is_empty() { return Some(path_part.to_string()); }
    }
    if trimmed.starts_with("![") {
        if let Some(close_bracket) = trimmed.find("](") {
            if let Some(close_paren) = trimmed[close_bracket + 2..].rfind(')') {
                let end = close_bracket + 2 + close_paren + 1;
                if end == trimmed.len() {
                    let url = &trimmed[close_bracket + 2..end - 1];
                    return Some(url.trim().to_string());
                }
            }
        }
    }
    None
}

/// Scan a single line for `[[...]]` wikilinks.
/// Returns ranges INCLUDING the surrounding `[[` and `]]`.
/// Byte-level scanner (no pulldown-cmark) -- safe to call in hot paths.
pub fn scan_wikilinks(line: &str) -> Vec<WikiLink> {
    let bytes = line.as_bytes();
    let mut result = Vec::new();
    let mut i = 0;
    while i + 3 < bytes.len() {
        if bytes[i] == b'[' && bytes[i + 1] == b'[' {
            let start = i;
            let content_start = i + 2;
            let mut j = content_start;
            let mut found_close = false;
            while j + 1 < bytes.len() {
                // Don't let a new `[[` start inside; bail out.
                if bytes[j] == b'[' && bytes[j + 1] == b'[' { break; }
                if bytes[j] == b']' && bytes[j + 1] == b']' {
                    found_close = true;
                    break;
                }
                j += 1;
            }
            if found_close && j > content_start {
                let inner = &line[content_start..j];
                // Split on first `|` for alias, then on first `#` for heading.
                let (before_pipe, alias) = match inner.find('|') {
                    Some(p) => (&inner[..p], Some(inner[p + 1..].trim().to_string())),
                    None => (inner, None),
                };
                let (target_raw, heading) = match before_pipe.find('#') {
                    Some(p) => (&before_pipe[..p], Some(before_pipe[p + 1..].trim().to_string())),
                    None => (before_pipe, None),
                };
                let target = target_raw.trim().to_string();
                if !target.is_empty() {
                    result.push(WikiLink {
                        range: start..(j + 2),
                        target,
                        heading: heading.filter(|s| !s.is_empty()),
                        alias: alias.filter(|s| !s.is_empty()),
                    });
                }
                i = j + 2;
                continue;
            }
            i += 1;
        } else {
            i += 1;
        }
    }
    result
}

fn add_span_to_lines(lines: &mut Vec<LineInfo>, line_starts: &[usize], _text: &str, start: usize, end: usize, style: SpanStyle) {
    let start_line = line_for_offset(line_starts, start);
    let end_line = line_for_offset(line_starts, end.saturating_sub(1).max(start));
    for i in start_line..=end_line.min(lines.len() - 1) {
        let line_start = line_starts[i];
        let line_end = if i + 1 < line_starts.len() { line_starts[i + 1] } else { usize::MAX };
        let s = start.max(line_start).saturating_sub(line_start);
        let e = end.min(line_end).saturating_sub(line_start);
        if s < e {
            lines[i].spans.push(InlineSpan { range: s..e, style });
        }
    }
}

fn line_for_offset(line_starts: &[usize], offset: usize) -> usize {
    match line_starts.binary_search(&offset) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    }
}

fn line_str<'a>(text: &'a str, line_starts: &[usize], line: usize) -> &'a str {
    let start = line_starts[line];
    let end = if line + 1 < line_starts.len() {
        // Strip trailing newline
        let e = line_starts[line + 1];
        if e > start && text.as_bytes().get(e - 1) == Some(&b'\n') { e - 1 } else { e }
    } else {
        text.len()
    };
    &text[start..end]
}

fn count_heading_prefix(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i] == b'#' { i += 1; }
    if i > 0 && i < bytes.len() && bytes[i] == b' ' {
        i + 1
    } else {
        0
    }
}

fn count_list_prefix(line: &str) -> usize {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    let bytes = trimmed.as_bytes();
    // Bullet: - * +
    if bytes.len() >= 2 && matches!(bytes[0], b'-' | b'*' | b'+') && bytes[1] == b' ' {
        return indent + 2;
    }
    // Numbered: 1. 12.
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
    if i > 0 && i + 1 < bytes.len() && bytes[i] == b'.' && bytes[i + 1] == b' ' {
        return indent + i + 2;
    }
    0
}

fn count_blockquote_prefix(line: &str) -> usize {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    let bytes = trimmed.as_bytes();
    if bytes.first() == Some(&b'>') {
        if bytes.get(1) == Some(&b' ') { indent + 2 } else { indent + 1 }
    } else {
        0
    }
}

/// Find the range of lines forming the block containing `cursor_line`.
/// Returns (start_line, end_line) inclusive.
/// Blocks are separated by blank lines. Code blocks extend across their fences.
pub fn block_range(
    n_lines: usize,
    cursor_line: usize,
    is_line_blank: impl Fn(usize) -> bool,
    line_info: impl Fn(usize) -> Option<BlockType>,
) -> (usize, usize) {
    if n_lines == 0 { return (0, 0); }
    let cursor_line = cursor_line.min(n_lines - 1);

    // If cursor is in a code block, expand to full code block extent
    if matches!(line_info(cursor_line), Some(BlockType::CodeBlock)) {
        let mut start = cursor_line;
        while start > 0 && matches!(line_info(start - 1), Some(BlockType::CodeBlock)) {
            start -= 1;
        }
        let mut end = cursor_line;
        while end + 1 < n_lines && matches!(line_info(end + 1), Some(BlockType::CodeBlock)) {
            end += 1;
        }
        return (start, end);
    }

    // If cursor is in a table, expand to full table
    if matches!(line_info(cursor_line), Some(BlockType::Table)) {
        let mut start = cursor_line;
        while start > 0 && matches!(line_info(start - 1), Some(BlockType::Table)) {
            start -= 1;
        }
        let mut end = cursor_line;
        while end + 1 < n_lines && matches!(line_info(end + 1), Some(BlockType::Table)) {
            end += 1;
        }
        return (start, end);
    }

    // Normal block: contiguous non-blank lines
    let mut start = cursor_line;
    while start > 0 && !is_line_blank(start - 1) { start -= 1; }
    let mut end = cursor_line;
    while end + 1 < n_lines && !is_line_blank(end + 1) { end += 1; }
    (start, end)
}

/// Marker lengths to strip for each span style: (prefix_bytes, suffix_bytes).
pub fn marker_lens(style: SpanStyle) -> (usize, usize) {
    match style {
        SpanStyle::Bold => (2, 2),           // **text**
        SpanStyle::Italic => (1, 1),         // *text*
        SpanStyle::BoldItalic => (3, 3),     // ***text***
        SpanStyle::InlineCode => (1, 1),     // `text`
        SpanStyle::Strikethrough => (2, 2),  // ~~text~~
        SpanStyle::Link => (0, 0),           // keep [text](url) as-is for now
        SpanStyle::Math => (1, 1),           // $text$
    }
}

/// Build line_starts from text (similar to rope line indexing).
#[cfg(test)]
pub fn build_line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' { starts.push(i + 1); }
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heading() {
        let text = "# Hello\n## World\n";
        let starts = build_line_starts(text);
        let lines = parse_lines(text, &starts);
        assert_eq!(lines[0].block.block_type, BlockType::Heading(1));
        assert_eq!(lines[0].block.prefix_len, 2); // "# "
        assert_eq!(lines[1].block.block_type, BlockType::Heading(2));
        assert_eq!(lines[1].block.prefix_len, 3); // "## "
    }

    #[test]
    fn test_bold() {
        let text = "This is **bold** text";
        let starts = build_line_starts(text);
        let lines = parse_lines(text, &starts);
        assert!(!lines[0].spans.is_empty());
        assert_eq!(lines[0].spans[0].style, SpanStyle::Bold);
    }

    #[test]
    fn test_list() {
        let text = "- item one\n- item two\n";
        let starts = build_line_starts(text);
        let lines = parse_lines(text, &starts);
        assert_eq!(lines[0].block.block_type, BlockType::ListItem);
        assert_eq!(lines[0].block.prefix_len, 2); // "- "
    }

    #[test]
    fn test_inline_code() {
        let text = "Use `let x = 5;` here";
        let starts = build_line_starts(text);
        let lines = parse_lines(text, &starts);
        assert!(!lines[0].spans.is_empty());
        assert_eq!(lines[0].spans[0].style, SpanStyle::InlineCode);
    }

    #[test]
    fn test_code_block() {
        let text = "```rust\nfn main() {}\n```\n";
        let starts = build_line_starts(text);
        let lines = parse_lines(text, &starts);
        assert_eq!(lines[0].block.block_type, BlockType::CodeBlock);
        assert_eq!(lines[1].block.block_type, BlockType::CodeBlock);
        assert_eq!(lines[2].block.block_type, BlockType::CodeBlock);
    }

    #[test]
    fn test_table() {
        let text = "| A | B |\n|---|---|\n| 1 | 2 |\n";
        let starts = build_line_starts(text);
        let lines = parse_lines(text, &starts);
        assert_eq!(lines[0].block.block_type, BlockType::Table, "line 0");
        assert_eq!(lines[1].block.block_type, BlockType::Table, "line 1");
        assert_eq!(lines[2].block.block_type, BlockType::Table, "line 2");
    }

    #[test]
    fn test_table_with_spaces() {
        let text = "| Parameter | Detail |\n|------------|---------|\n| Manufacturer | Elbit Systems |\n";
        let starts = build_line_starts(text);
        let lines = parse_lines(text, &starts);
        assert_eq!(lines[0].block.block_type, BlockType::Table);
        assert_eq!(lines[1].block.block_type, BlockType::Table);
        assert_eq!(lines[2].block.block_type, BlockType::Table);
    }

    #[test]
    fn test_wikilink_simple() {
        let text = "See [[Other Note]] for details.";
        let starts = build_line_starts(text);
        let lines = parse_lines(text, &starts);
        assert_eq!(lines[0].wikilinks.len(), 1);
        let w = &lines[0].wikilinks[0];
        assert_eq!(w.target, "Other Note");
        assert_eq!(w.alias, None);
        assert_eq!(w.heading, None);
        assert_eq!(&text[w.range.clone()], "[[Other Note]]");
    }

    #[test]
    fn test_wikilink_with_alias() {
        let text = "Go to [[Target Note|click here]] now.";
        let starts = build_line_starts(text);
        let lines = parse_lines(text, &starts);
        assert_eq!(lines[0].wikilinks.len(), 1);
        let w = &lines[0].wikilinks[0];
        assert_eq!(w.target, "Target Note");
        assert_eq!(w.alias.as_deref(), Some("click here"));
    }

    #[test]
    fn test_wikilink_with_heading() {
        let text = "See [[Note#Section Two]] please.";
        let starts = build_line_starts(text);
        let lines = parse_lines(text, &starts);
        assert_eq!(lines[0].wikilinks.len(), 1);
        let w = &lines[0].wikilinks[0];
        assert_eq!(w.target, "Note");
        assert_eq!(w.heading.as_deref(), Some("Section Two"));
        assert_eq!(w.alias, None);
    }

    #[test]
    fn test_wikilink_heading_and_alias() {
        let text = "See [[Note#Sec|display]] text.";
        let starts = build_line_starts(text);
        let lines = parse_lines(text, &starts);
        let w = &lines[0].wikilinks[0];
        assert_eq!(w.target, "Note");
        assert_eq!(w.heading.as_deref(), Some("Sec"));
        assert_eq!(w.alias.as_deref(), Some("display"));
    }

    #[test]
    fn test_wikilink_multiple() {
        let text = "Link one [[A]] and link two [[B|beta]].";
        let starts = build_line_starts(text);
        let lines = parse_lines(text, &starts);
        assert_eq!(lines[0].wikilinks.len(), 2);
        assert_eq!(lines[0].wikilinks[0].target, "A");
        assert_eq!(lines[0].wikilinks[1].target, "B");
        assert_eq!(lines[0].wikilinks[1].alias.as_deref(), Some("beta"));
    }

    #[test]
    fn test_wikilink_ignored_in_inline_code() {
        let text = "Use `[[Not a link]]` here.";
        let starts = build_line_starts(text);
        let lines = parse_lines(text, &starts);
        assert_eq!(lines[0].wikilinks.len(), 0);
    }

    #[test]
    fn test_wikilink_ignored_in_code_block() {
        let text = "```\n[[Not a link]]\n```\n";
        let starts = build_line_starts(text);
        let lines = parse_lines(text, &starts);
        assert_eq!(lines[1].wikilinks.len(), 0);
    }

    #[test]
    fn test_wikilink_empty_target_skipped() {
        let text = "Bad [[]] here and [[ ]] too.";
        let starts = build_line_starts(text);
        let lines = parse_lines(text, &starts);
        assert_eq!(lines[0].wikilinks.len(), 0);
    }

    #[test]
    fn test_wikilink_unclosed_skipped() {
        let text = "Oops [[unfinished and then more text";
        let starts = build_line_starts(text);
        let lines = parse_lines(text, &starts);
        assert_eq!(lines[0].wikilinks.len(), 0);
    }

    #[test]
    fn test_table_with_surrounding_text() {
        let text = "Some paragraph text here.\n\n| Parameter | Detail |\n|------------|---------|\n| Manufacturer | Elbit Systems |\n\nMore text after.\n";
        let starts = build_line_starts(text);
        let lines = parse_lines(text, &starts);
        // line 0: paragraph
        assert_eq!(lines[0].block.block_type, BlockType::Paragraph);
        // line 1: blank
        // lines 2, 3, 4: table
        assert_eq!(lines[2].block.block_type, BlockType::Table, "line 2 should be Table");
        assert_eq!(lines[3].block.block_type, BlockType::Table, "line 3 should be Table");
        assert_eq!(lines[4].block.block_type, BlockType::Table, "line 4 should be Table");
    }
}
