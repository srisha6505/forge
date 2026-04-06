# Forge Codebase Index

Quick reference for finding where things live. Update this file whenever files/types/functions change.

## Project

A pure-Rust, GPU-accelerated markdown editor built on GPUI (Zed's rendering engine). Reads Obsidian-style vaults.

## Architecture (bottom-up)

```
┌────────────────────────────────────────┐
│ app.rs — ForgeApp: window, sidebar,    │
│          tabs, file management         │
├────────────────────────────────────────┤
│ editor.rs — Editor: GPUI component,    │
│             rendering, cursor, input   │
├────────────────────────────────────────┤
│ buffer.rs — Buffer: rope text buffer,  │
│             cursor, selection, undo    │
├────────────────────────────────────────┤
│ markdown.rs — Parser: pulldown-cmark   │
│               → per-line block info    │
│ settings.rs — Persistent config        │
│ theme.rs    — Design tokens            │
│ icons.rs    — Inline div-drawn icons   │
└────────────────────────────────────────┘
```

---

## File Index

### `src/main.rs` (11 lines)
Entry point. Declares modules, calls `app::run_app()`.

**Modules declared:** `app`, `buffer`, `editor`, `graph`, `icons`, `links`, `markdown`, `settings`, `theme`

---

### `src/buffer.rs` (694 lines)
**Pure data layer.** Rope-based text buffer, cursor, selection, undo/redo.
No GPUI dependencies. All text editing logic lives here.

**Key types:**
- `Buffer` — text buffer backed by `ropey::Rope`. Owns selection + undo stack.
- `Selection { anchor, head }` — two byte offsets, can be empty (cursor) or range.

**Buffer API (where to look for editing operations):**

| Category | Methods |
|---|---|
| **Accessors** | `rope()`, `text()`, `len_bytes()`, `len_lines()`, `line_str(i)`, `line_to_byte(l)`, `byte_to_line(b)`, `selection()`, `cursor()`, `is_dirty()` |
| **Cursor movement** | `move_left()`, `move_right()`, `move_up()`, `move_down()`, `move_home()`, `move_end()`, `move_word_left()`, `move_word_right()` |
| **Selection** | `select_left()`, `select_right()`, `select_up()`, `select_down()`, `select_word_left()`, `select_word_right()`, `select_home()`, `select_end()`, `select_all()`, `select_word_at()`, `select_line_at()` |
| **Editing** | `insert(text)`, `backspace()`, `delete()`, `backspace_word()`, `delete_word()`, `enter()`, `indent()`, `dedent()`, `duplicate_line()`, `toggle_wrap(marker)`, `insert_at_line_start(prefix)` |
| **History** | `undo()`, `redo()` |
| **Clipboard** | `selected_text()` |

**Internal:**
- `Edit { offset, deleted, inserted }` — single reversible edit
- `EditGroup` — groups of edits that undo together (grouped by 300ms timeout)

---

### `src/editor.rs` (1854 lines)
**GPUI rendering layer.** This is the big one. Renders the Buffer, handles keyboard/mouse, parses markdown inline styling.

**Key types:**
- `Editor` — GPUI entity wrapping a Buffer. Holds render state.
- `EditorElement` — custom GPUI Element that paints the editor.
- `DisplayLine` — one content line's rendering: display_text + TextRuns + display↔content mapping
- `RenderItem` — enum: `Line(RenderLine)` or `Table(RenderTable)` — hybrid rendering items
- `RenderLine` — a single line with wrapped text, display line, y-origin, height
- `RenderTable` — a table widget: rows, col_x, col_widths, y-origin, total_height
- `RenderTableRow` — one row: cells, kind (Header/Separator/Data), y_in_table, height
- `RenderCell` — one cell: lines (Vec<ShapedLine>), col index
- `TableRowKind` — enum: NotTable, Header, Separator, Data
- `EditorPrepaint` — state passed from prepaint to paint

**Sections of the file:**

| Line range | Contents |
|---|---|
| 1-12 | Imports + LINE_HEIGHT (22px) + SCROLL_LINES (3.0) constants |
| 14-42 | Actions: all editor actions (movement, selection, editing, formatting, inserts) |
| 44-135 | `Editor` struct + `new()` + cursor blink timer + set_content + reparse |
| 137-345 | Action handler methods (`on_move_left`, `on_backspace`, `on_toggle_bold`, etc.) - make `pub` so `app.rs` can forward |
| 347-425 | EntityInputHandler impl (OS keyboard input → buffer edits) |
| 427-495 | TableRowKind + RenderItem + RenderLine + RenderTable + RenderTableRow + RenderCell |
| 497-570 | DisplayLine struct + build_display_line() entry point |
| 572-830 | `build_display_line` logic: empty line / table / raw mode / HR / stripped mode + list bullet/checkbox injection |
| 832-920 | `process_list_item_display` (currently unused — list handling is inline in stripped path) |
| 922-1060 | `wrap_at_word_boundaries`, `slice_runs`, `build_cell_inline` (pulldown-cmark parse inline markers) |
| 1062-1170 | `cell_effective_length`, `build_table_render` (computes col widths, wraps cells) |
| 1172-1260 | `build_table_runs`, `build_text_runs_raw/display/inner` (per-byte style runs) |
| 1262-1310 | `parse_table_cells`, `is_table_separator`, `pad_table_line` (legacy, for active-table raw rendering) |
| 1312-1395 | `EditorElement` + `EditorPrepaint` struct |
| 1397-1570 | `EditorElement::prepaint` — builds RenderItems, cursor, selections |
| 1572-1780 | `EditorElement::paint` — draws decorations (code blocks), selections, RenderItems (Line + Table), cursor |
| 1782-1854 | `impl Render for Editor` — wires up div + actions + mouse handlers |

**Where to edit specific features:**

| Feature | Location |
|---|---|
| Add a new editor action | `actions!` macro (~line 15), add `on_xxx` method (~line 137+), register in render impl (~line 1800), bind key in `app.rs` `bind_keys` |
| Change inline markdown styling (bold, italic, code, links) | `build_text_runs_inner` (~line 1220) |
| Change heading sizes | `build_display_line`'s font_size match (~line 550) |
| Change list bullet/checkbox chars | `build_display_line` list_replacement logic (~line 610) |
| Change table colors/borders/styling | `EditorElement::paint` table rendering (~line 1680) |
| Change table column width distribution | `build_table_render` col_widths computation (~line 1100) |
| Change cell wrapping | `wrap_at_word_boundaries` (~line 940) |
| Change cursor appearance/blinking | cursor paint in prepaint (~line 1500) + blink timer in Editor::new (~line 55) |
| Change selection highlight | selection paint in prepaint (~line 1520) |

---

### `src/markdown.rs` (394 lines)
**Markdown parser.** Wraps `pulldown-cmark` to produce per-line annotations.

**Key types:**
- `BlockType` — enum: Paragraph, Heading(1-6), ListItem, CodeBlock, BlockQuote, HorizontalRule, Table, MathBlock (`$$...$$`), ImageEmbed (whole-line `![[img.png]]` or `![alt](url)`)
- `BlockInfo { block_type, prefix_len }` — which block a line belongs to + syntax prefix length
- `SpanStyle` — enum: Bold, Italic, BoldItalic, InlineCode, Strikethrough, Link
- `InlineSpan { range, style }` — byte range within line
- `WikiLink { range, target, heading, alias }` — `[[Target]]`, `[[Target|alias]]`, `[[Target#heading]]`
- `LineInfo { block, spans, wikilinks }` — per-line annotations

**Public functions:**
- `parse_lines(text, line_starts) -> Vec<LineInfo>` — main parser entry point (also scans wikilinks, math blocks, image embeds)
- `block_range(n_lines, cursor_line, is_blank, line_info) -> (start, end)` — find block containing cursor
- `marker_lens(style) -> (prefix_bytes, suffix_bytes)` — how many bytes to strip per span style
- `parse_image_embed(trimmed) -> Option<String>` — extract path/target from a whole-line image embed

**Tests:** heading, bold, list, inline_code, code_block, table(s), wikilink_simple, wikilink_with_alias, wikilink_with_heading, wikilink_heading_and_alias, wikilink_multiple, wikilink_ignored_in_inline_code, wikilink_ignored_in_code_block, wikilink_empty_target_skipped, wikilink_unclosed_skipped

---

### `src/graph.rs` (~310 lines)
**Graph view.** Renders a force-free circular node-edge graph built from the LinkIndex.

**Key types:**
- `GraphView` — GPUI entity; holds nodes, edges, pan, zoom, hover state
- `GraphNode { path, label, x, y, radius }` — one note, with layout + size (scales with degree)
- `GraphEvent::OpenNote(PathBuf)` — emitted when user clicks a node
- `GraphElement` — custom Element that paints: bg quad, edges (stroked paths), nodes (rounded quads), labels (ShapedLine)

**API:**
- `GraphView::new(cx)` — empty graph
- `set_data(&LinkIndex)` — rebuild nodes + edges + circular layout
- `render_graph(&Entity<GraphView>, cx)` — wraps GraphView in mouse/scroll handlers, node/edge count HUD

**Interaction:**
- Drag background: pan. Scroll wheel: zoom (0.2–3.0). Click node: emits `OpenNote` → ForgeApp switches to Files panel and opens tab.

---

### `src/links.rs` (~290 lines)
**Wikilink index.** Resolves `[[Target]]` to file paths + tracks backlinks across the vault.

**Key types:**
- `LinkIndex` — main struct; holds `name_to_paths`, `outgoing`, `backlinks` hashmaps
- `LinkRef { source, line, context, target }` — one wikilink occurrence

**API:**
- `LinkIndex::scan_vault(root) -> Self` — walk all `.md` files, parse wikilinks, build index
- `update_file(path, content)` — incremental update on save / file change
- `remove_file(path)` — drop a file from the index
- `resolve(target) -> Option<&Path>` — case-insensitive basename → path (shortest path on collision)
- `exists(target) -> bool`
- `backlinks_for_path(path) -> Vec<&LinkRef>` — all places linking to `path`

**Tests:** resolve_basic, resolve_collision_prefers_shortest_path, reindex_builds_backlinks, reindex_replaces_previous_entries, remove_file_clears_index, self_links_excluded_from_backlinks

---

### `src/app.rs` (909 lines)
**Application shell.** Window, sidebar, file tree, tabs, vault management, keybindings.

**Key types:**
- `ForgeApp` — main app entity
- `FileTreeEntry` — enum: File { name, path } or Folder { name, path, children }
- `Tab { path, name }` — one open tab

**Public functions:**
- `run_app()` — creates window, binds keys, opens editor (line ~690+)

**ForgeApp fields:**
- `vault_path`, `vault_name`, `files`, `file_tree`, `tabs`, `active_tab`, `editor: Entity<Editor>`, `sidebar_visible`, `collapsed_folders`, `settings`, `title_input`, `renaming_title`, `readable_width`, `_watcher` (file notify), `link_index: LinkIndex`, `backlinks_visible: bool`

**Methods grouped:**

| Category | Methods |
|---|---|
| **Vault** | `open_folder`, `load_vault_sync`, `refresh_file_tree`, `refresh_vault`, `start_watcher` |
| **Tabs** | `open_path_as_tab`, `switch_to_tab`, `load_active_tab`, `close_tab_at`, `close_current_tab`, `next_tab`, `prev_tab`, `persist_tabs` |
| **Files** | `save`, `new_file`, `delete_file`, `start_rename`, `commit_rename` |
| **Toggles** | `toggle_theme`, `toggle_sidebar`, `toggle_readable_width`, `toggle_read_mode` |
| **Editor action forwarders** | `fwd_cut`, `fwd_copy`, `fwd_paste`, `fwd_select_all`, `fwd_undo`, `fwd_bold`, `fwd_italic`, etc. — so context menu actions reach the editor |
| **Render** | `render_file_tree`, `impl Render for ForgeApp` (builds sidebar + tabs + content + status bar) |

**Where to edit specific features:**

| Feature | Location |
|---|---|
| Add a keybinding | `run_app` → `cx.bind_keys([...])` (~line 770) |
| Add a context menu item | `context_menu` closure in Render impl (~line 640) |
| Change sidebar layout | `render_file_tree` + sidebar construction in Render impl (~line 480) |
| Change tab bar | Tab bar construction in Render impl (~line 560) |
| Change topbar | Topbar construction in Render impl (~line 590) |
| Change status bar | Status bar construction in Render impl (~line 680) |

**Actions defined:** OpenFolder, Save, Quit, ToggleTheme, ToggleSidebar, NewFile, DeleteFile, CloseTab, NextTab, PrevTab, RefreshVault, ToggleReadableWidth, ToggleBacklinks, ShowFiles, ShowGraph, ShowSettings

**SidePanel enum:** `Files | Graph | Settings`. Narrow 44px icon rail on far left switches between: file tree + editor (Files), graph view (Graph), settings pane (Settings). Search icon present but inactive.

---

### `src/settings.rs` (65 lines)
**Persistent config** stored in `~/.config/forge/settings.json`.

**Fields:**
- `last_vault_path`, `theme`, `open_tabs`, `active_tab`

**Methods:**
- `load()` — read from disk
- `save()` — write to disk
- `resolved_vault_path()` — last vault or fallback (`/home/code/Production/sfa/research`)
- `set_vault(path)` — switch vault, clear tabs, save

---

### `src/theme.rs` (37 lines)
**Design tokens.** Change values here to restyle the app.

**Constants:**
- Font sizes: `FONT_EDITOR`, `FONT_UI`, `FONT_SM`, `FONT_TINY`, `FONT_TITLE`, `FONT_EMPTY_STATE`
- Sidebar: `SIDEBAR_WIDTH`, `SIDEBAR_HEADER_HEIGHT`, `SIDEBAR_ITEM_HEIGHT`, `SIDEBAR_INDENT_PER_LEVEL`, `SIDEBAR_PADDING_LEFT`
- Tabs: `TAB_BAR_HEIGHT`, `TAB_MAX_WIDTH`
- Status bar: `STATUSBAR_HEIGHT`
- Content: `CONTENT_MAX_WIDTH`, `CONTENT_PADDING_X`, `CONTENT_PADDING_X_WIDE`, `CONTENT_PADDING_TOP`, `TITLE_PADDING_BOTTOM`
- Radii: `RADIUS_SM`, `RADIUS_MD`, `RADIUS_LG`

---

### `src/icons.rs` (120 lines)
**Inline icon shapes** built from GPUI divs (no SVG assets).

**Public items:**
- `ICON_W`, `ICON_H` — size constants
- `file_icon(color)` — document with 3 lines inside
- `folder_icon(color)` — tab + body
- `image_icon(color)` — rect with sun dot + mountain
- `pdf_icon(color)` — document with colored bar at bottom
- `code_icon(color)` — `{}` in rectangle
- `icon_for_path(path, color)` — dispatcher based on file extension
- `chevron_right_char()`, `chevron_down_char()`, `close_char()` — unicode string helpers

**Extensions mapped:** `.md`→file, `.png/.jpg/.gif/.webp/.svg`→image, `.pdf`→pdf, `.rs/.py/.js/.ts/.go/.java/.cpp` etc.→code

---

## Keybindings Reference

### App level (ForgeApp context)
| Key | Action |
|---|---|
| Ctrl+O | Open folder |
| Ctrl+S | Save |
| Ctrl+Q | Quit |
| Ctrl+N | New file |
| Ctrl+W | Close tab |
| Ctrl+Tab / Ctrl+Shift+Tab | Next/Prev tab |
| Ctrl+Shift+T | Toggle theme |
| Ctrl+B | Toggle sidebar |
| Ctrl+Shift+R | Toggle readable width |
| Ctrl+R / F5 | Refresh vault |
| Ctrl+E | Toggle read mode |
| Ctrl+Shift+B | Toggle backlinks panel |

### Editor context
| Key | Action |
|---|---|
| Arrow keys | Move cursor |
| Home/End | Line start/end |
| Ctrl+Left/Right | Word move |
| Shift+arrows | Extend selection |
| Ctrl+Shift+Left/Right | Word select |
| Shift+Home/End | Select to line start/end |
| Ctrl+A | Select all |
| Ctrl+L | Select current line |
| Backspace / Delete | Delete |
| Ctrl+Backspace / Ctrl+Delete | Delete word |
| Enter | Smart enter (continues lists) |
| Tab / Shift+Tab | Indent/Dedent |
| Ctrl+C / X / V | Copy/Cut/Paste |
| Ctrl+Z / Ctrl+Y | Undo/Redo |
| Ctrl+Shift+D | Duplicate line |
| Ctrl+B | Bold (wrap with `**`) |
| Ctrl+I | Italic |
| Ctrl+Shift+C | Inline code |
| Ctrl+Shift+S | Strikethrough |
| Up/Down | (autocomplete open) navigate list |
| Enter / Tab | (autocomplete open) insert selected wikilink |
| Esc | (autocomplete open) close popup |

---

## Dependencies (Cargo.toml)

| Crate | Purpose |
|---|---|
| `gpui = "0.2.2"` | Zed's GPU-accelerated UI framework |
| `gpui-component = "0.5"` | UI components (Input, PopupMenu, ContextMenu), themes |
| `ropey = "1.6"` | Rope text buffer |
| `pulldown-cmark = "0.12"` | Markdown parser |
| `notify = "7"` | File system watcher (external changes to vault) |
| `serde`, `serde_json` | Settings serialization |
| `dirs = "5"` | Platform config directory (~/.config/forge) |
| `unicode-segmentation = "1.12"` | Grapheme-aware text ops |

---

## How to add X

### Add a new editor action
1. Add variant to `actions!` macro in `editor.rs`
2. Add `pub fn on_xxx(&mut self, _: &Xxx, w: &mut Window, cx: &mut Context<Self>)` handler
3. Register with `.on_action(cx.listener(Self::on_xxx))` in `impl Render for Editor`
4. Add keybinding in `app.rs` `cx.bind_keys([...])`
5. (Optional) Add forwarder on ForgeApp so context menu can dispatch it

### Add a new block type render
1. Add variant to `RenderItem` enum in `editor.rs`
2. Detect in prepaint loop, build item, add to `items` Vec
3. Paint it in the paint loop match

### Change colors/styling
- Design tokens: `src/theme.rs`
- Editor text colors: hardcoded at top of `EditorElement::prepaint`
- Editor decoration colors: hardcoded at top of `EditorElement::paint`
- Theme colors: use `cx.theme().foreground`, `cx.theme().accent`, etc. from `gpui-component`

### Change the default vault
`src/settings.rs` → `Settings::default_test_vault()`

### Add a file type with its own icon
`src/icons.rs` → `icon_for_path()` match arm
