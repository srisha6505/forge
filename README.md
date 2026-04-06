# Forge

A fast, GPU-accelerated markdown editor built in Rust. Reads Obsidian-style vaults. Single binary, no Electron, no browser runtime.

Built on [GPUI](https://github.com/zed-industries/zed) (Zed's rendering engine) for native performance.

## Features

- **Live preview** -- renders headings, bold, italic, code, lists, tables, horizontal rules with syntax hidden on inactive lines
- **Wikilinks** -- `[[Note]]` links with autocomplete, click-to-navigate, and backlink tracking
- **Graph view** -- force-directed visualization of note connections (pan, zoom, drag nodes, click to open)
- **Inline images** -- renders `![[image.png]]` embeds directly in the editor
- **Sidebar** -- file tree with folder collapse, right-click context menus (open, rename, duplicate, delete, reveal in file manager)
- **Tabs** -- open multiple notes, Ctrl+Tab/Shift+Tab to switch, Ctrl+W to close
- **Navigation history** -- Alt+Left/Right to go back/forward across opened files
- **Backlinks panel** -- see all notes that link to the current note
- **Settings panel** -- font family selection (body, interface, monospace), font size, resizable sidebar, persistent config
- **Dark/light themes** -- Ctrl+Shift+T to toggle, persists across sessions
- **Zoom** -- Ctrl+=/Ctrl+- to zoom, Ctrl+0 to reset
- **File watcher** -- auto-refreshes the file tree when files change externally

## Building

Requires Rust 1.80+ and a working C linker.

```bash
cargo build --release
./target/release/forge
```

### Linux dependencies

On Linux, you need these development packages (or equivalent):

- `libxkbcommon-dev`
- `libxkbcommon-x11-dev`
- `libxcb-dev` (or `libxcb1-dev`)

On Ubuntu/Debian:
```bash
sudo apt install libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev
```

### macOS / Windows

Should build out of the box with `cargo build --release`. Tested on Linux x86_64; macOS ARM and Windows x86_64 are supported targets but not primary test platforms yet.

## Usage

1. Run `forge`
2. Press **Ctrl+O** to open a folder (any folder with `.md` files works, including Obsidian vaults)
3. Click a note in the sidebar to open it
4. Start editing -- the live preview renders automatically

### Keyboard shortcuts

| Key | Action |
|---|---|
| Ctrl+O | Open vault |
| Ctrl+S | Save |
| Ctrl+N | New file |
| Ctrl+W | Close tab |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab |
| Alt+Left / Alt+Right | Navigate back / forward |
| Ctrl+Shift+T | Toggle dark/light theme |
| Ctrl+B | Toggle sidebar |
| Ctrl+Shift+B | Toggle backlinks panel |
| Ctrl+E | Toggle read mode |
| Ctrl+= / Ctrl+- / Ctrl+0 | Zoom in / out / reset |
| PageUp / PageDown | Scroll by page |
| Ctrl+Home / Ctrl+End | Jump to document start / end |
| Ctrl+B (in editor) | Bold |
| Ctrl+I | Italic |
| Ctrl+Shift+C | Inline code |
| Ctrl+Shift+S | Strikethrough |
| Ctrl+Z / Ctrl+Y | Undo / redo |
| Ctrl+Shift+D | Duplicate line |
| F5 / Ctrl+R | Refresh vault |

## Architecture

```
src/
  main.rs        -- entry point
  app.rs         -- window shell, sidebar, tabs, keybindings
  buffer.rs      -- rope-based text buffer, cursor, selection, undo
  editor.rs      -- GPUI rendering, live preview, cursor, input handling
  markdown.rs    -- pulldown-cmark parser, per-line block/inline annotations
  links.rs       -- wikilink index, backlink tracking
  graph.rs       -- force-directed graph view
  icons.rs       -- div-based icon shapes
  settings.rs    -- persistent config (~/.config/forge/settings.json)
  theme.rs       -- design tokens (sizes, spacing, radii)
```

Three layers:
1. **Buffer** -- pure data, no UI dependencies. Rope text storage, selection, undo/redo.
2. **Editor** -- custom GPUI Element. Renders the buffer with markdown styling, handles keyboard/mouse input.
3. **App** -- GPUI window. Sidebar, tabs, file management, graph view, settings.

## Configuration

Settings are stored in `~/.config/forge/settings.json` and include:

- Last opened vault path
- Theme (light/dark)
- Open tabs (restored on launch)
- Font families (body, interface, monospace)
- Font size
- Sidebar width

## Status

v0.1 -- usable for daily note editing. Not feature-complete with Obsidian but covers the core workflow: open a vault, browse files, edit markdown with live preview, follow wikilinks, view backlinks and graph.

## License

AGPL-3.0
