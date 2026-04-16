# Forge

A markdown editor with a built-in local AI research agent. Reads Obsidian-style vaults. Theme-compatible with the Obsidian community theme ecosystem.

Built on **Tauri 2 + React 18 + CodeMirror 6** for the frontend, with a Rust backend for inference, search, and file IO. Runs as a native desktop app with a small binary and low memory footprint.

## Migration from GPUI

Forge v0.1 and v0.2 were built on [GPUI](https://github.com/zed-industries/zed) (Zed's GPU-accelerated UI framework). v0.3 ports the entire frontend to **Tauri + React + TypeScript**.

### Why the migration

- **UI iteration speed.** GPUI has a steep learning curve and a small ecosystem. Every UI change required custom layout math, custom widgets, and Rust recompiles. With React, we get the entire frontend ecosystem (CodeMirror, react-markdown, lucide-react, Tailwind) and hot module reload during development.
- **Theme compatibility with Obsidian.** Implementing CSS custom properties and class selectors in GPUI was awkward. With a real DOM and CSS, Forge can host community themes from the Obsidian ecosystem with the same variable namespace they already target.
- **Editor maturity.** Building a serious markdown editor in GPUI meant implementing a rope buffer, syntax highlighter, IME handling, accessibility, line wrapping, and selection from scratch. CodeMirror 6 handles all of that and adds a mature plugin ecosystem.
- **Distribution.** Tauri produces single-binary installers per platform with built-in update mechanisms. The GPUI build needed Vulkan/MoltenVK linkage decisions per platform that we kept getting wrong.

### What was preserved

The Rust core was kept and ported into the Tauri backend:

| Module | Status |
|---|---|
| `llm.rs` (llama-cpp-2 inference, dedicated thread, mpsc channels) | preserved |
| `agent.rs` (tool-use loop, vault tools, schemas) | preserved + extended (vector search tool, auto `.md` extension on writes) |
| `search.rs` (SQLite FTS5 + usearch hybrid search) | preserved |
| `embedder.rs` (candle + all-MiniLM-L6-v2) | preserved |
| `auth.rs` (Anthropic OAuth + API key) | preserved |
| `settings.rs` (persisted config) | preserved + extended |

### What was rebuilt

| Old (GPUI) | New (Tauri + React) |
|---|---|
| `app.rs` (window, sidebar, tabs) | `App.tsx` + `Sidebar.tsx` + `LeftRail.tsx` + `ResizeHandle.tsx` |
| `editor.rs` (custom GPUI element, ~1850 lines) | `Editor.tsx` + CodeMirror 6 |
| `chat.rs` | `Chat.tsx` |
| `markdown.rs` (pulldown-cmark + custom highlight) | `cm-markdown-render.ts` (CM6 StateField + decorations) |
| `links.rs` (wikilink resolution) | `cm-wikilinks.ts` + `App.tsx::resolveInTree` |
| `graph.rs` (force-directed graph view) | dropped for v0.3 (will return) |
| `theme.rs` (design tokens) | `index.css` with Obsidian-compatible CSS variables |
| `icons.rs` (div-based icons) | `lucide-react` |

### Deferred features

The v0.1/v0.2 graph view, backlinks panel, and inline image embeds were dropped during the migration. They will return as React components when the editor + search + chat surfaces are stable.

## Highlights

- Opens Obsidian vaults as-is, no migration
- CodeMirror 6 markdown editing with Obsidian-style live preview (markers hide on inactive lines)
- Local AI chat agent with any GGUF model via llama.cpp (Vulkan GPU acceleration)
- Hybrid BM25 + vector search across all notes (Ctrl+Shift+F popup, plus a sidebar panel)
- Read mode toggle (Ctrl+E), readable-width toggle (Ctrl+Shift+R)
- Light + dark themes with full Obsidian variable namespace — community themes load with minimal edits
- Multi-tab editing, ctrl-click for new tab in sidebar

## Installation

Requires Rust 1.80+, Node 18+, and a C/C++ toolchain.

```bash
# System dependencies (Ubuntu/Debian)
sudo apt install cmake pkg-config libvulkan-dev glslc \
    libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
    librsvg2-dev libsoup-3.0-dev

# Build
git clone https://github.com/srisha6505/forge.git
cd forge
npm install
npm run tauri build

# Or run in dev mode
npm run tauri dev
```

#### Build dependencies by distro

| Distro | Packages |
|---|---|
| Ubuntu/Debian | `cmake pkg-config libvulkan-dev glslc libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libsoup-3.0-dev` |
| Fedora | `cmake gcc-c++ vulkan-devel glslc webkit2gtk4.1-devel gtk3-devel libayatana-appindicator3-devel librsvg2-devel libsoup3-devel` |
| Arch | `cmake vulkan-devel shaderc webkit2gtk-4.1 gtk3 libayatana-appindicator librsvg libsoup3` |

## Getting Started

1. Run Forge (`npm run tauri dev` for dev, or the built binary)
2. Click the **Open** button in the sidebar to pick a vault folder (any folder with `.md` files, including Obsidian vaults)
3. Click a note in the sidebar to open it
4. Start editing

### Setting up the AI agent

Forge runs AI models locally using [llama.cpp](https://github.com/ggerganov/llama.cpp). No API keys, no internet, your data stays on your machine.

1. Download a GGUF model:

```bash
# Gemma 4 E4B (recommended for ≤12GB VRAM)
pip install huggingface_hub
hf download unsloth/gemma-4-E4B-it-GGUF \
    --local-dir ~/.forge/models/gemma-4-E4B \
    --include "*Q4_K_M*"
```

2. Add the model path to `~/.config/forge/settings.json`:

```json
{
    "model_path": "/home/you/.forge/models/gemma-4-E4B/gemma-4-E4B-it-Q4_K_M.gguf",
    "gpu_layers": 99,
    "ctx_size": 8192,
    "ai_provider": "local"
}
```

3. Click the message-bubble icon in the left rail or press **Ctrl+Shift+L** to toggle the chat panel
4. Click **Connect** in the chat panel header
5. Ask questions — the agent searches your notes via the vault search tool

#### Anthropic API alternative

If you'd rather use Anthropic's hosted models:

```json
{
    "ai_provider": "anthropic",
    "api_key": "sk-ant-...",
    "api_model": "claude-sonnet-4-6"
}
```

## Keyboard Shortcuts

### Global

| Key | Action |
|---|---|
| Ctrl+S | Force-save current file (writes are also auto-debounced) |
| Ctrl+B | Toggle sidebar |
| Ctrl+Shift+L | Toggle chat panel |
| Ctrl+Shift+F | Open search modal |
| Ctrl+Shift+P | Show files in sidebar |
| Ctrl+, | Show settings (placeholder) |
| Ctrl+W | Close current tab |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab |
| Ctrl+E | Toggle read mode |
| Ctrl+Shift+R | Toggle readable width |

### Sidebar

- **Click** a file → open in current tab
- **Ctrl+click** or **middle-click** a file → open in new tab

### Search modal (Ctrl+Shift+F)

| Key | Action |
|---|---|
| ↑ / ↓ | Navigate results |
| Enter | Open selected |
| Ctrl+Enter | Open in new tab |
| Esc | Close |

Quote-prefix a query (`"exact phrase"`) for BM25 keyword-only mode.

## Architecture

```
forge/
├── src/                       # React frontend
│   ├── App.tsx                # shell, tabs, state machine, shortcuts
│   ├── main.tsx               # React root
│   ├── index.css              # Obsidian-namespace CSS variables, themes
│   ├── components/
│   │   ├── LeftRail.tsx       # icon ribbon (files / search / chat / settings / theme)
│   │   ├── Sidebar.tsx        # file tree
│   │   ├── Editor.tsx         # CodeMirror 6 wrapper
│   │   ├── Chat.tsx           # streaming chat with tool blocks
│   │   ├── Search.tsx         # in-sidebar search panel
│   │   ├── SearchModal.tsx    # Ctrl+Shift+F popup
│   │   ├── MarkdownPreview.tsx # read-mode renderer (react-markdown)
│   │   ├── ResizeHandle.tsx   # CSS-variable-based pane drag
│   │   └── ErrorBoundary.tsx  # catches CM6 render exceptions
│   └── lib/
│       ├── tauri.ts           # typed invoke wrappers + event listeners
│       ├── cm-theme.ts        # CodeMirror theme + extension orchestration
│       ├── cm-markdown-render.ts # StateField that emits all decorations
│       ├── cm-wikilinks.ts    # [[wikilinks]] widget + click-to-open
│       └── cm-hyperlinks.ts   # Ctrl+click on [text](url) → shell open
└── src-tauri/                 # Rust backend
    ├── Cargo.toml
    ├── tauri.conf.json
    ├── capabilities/default.json
    └── src/
        ├── main.rs            # entry point
        ├── lib.rs             # AppState, command registry
        ├── commands.rs        # 16 Tauri commands (settings, vault, files, search, chat)
        ├── llm.rs             # llama-cpp-2 inference + Anthropic API
        ├── agent.rs           # tool-use loop, 10 vault tools
        ├── search.rs          # SQLite FTS5 + usearch HNSW
        ├── embedder.rs        # candle + bge-style local embeddings
        ├── auth.rs            # Anthropic OAuth (PKCE)
        └── settings.rs        # persisted config
```

### IPC contract

The frontend never calls `invoke()` directly. All Rust commands go through typed wrappers in `src/lib/tauri.ts`. Streaming events from the agent (`chat://token`, `chat://tool-start`, `chat://tool-result`, `chat://done`, `chat://error`, `vault://changed`) are subscribed via `listen()` wrappers in the same file.

### Data flow (chat)

```
User types in chat
  → sendChatMessage(history) IPC
    → spawn forge-agent thread
      → run_agent_loop drives InferenceHandle.generate()
        → tokens stream back via mpsc → chat://token events
        → tool calls parsed → execute_tool → chat://tool-start / chat://tool-result
        → vault writes also emit vault://changed
      → chat://done on completion
  → frontend renders streaming tokens + tool blocks
```

## Configuration

All settings live in `~/.config/forge/settings.json`:

```json
{
    "last_vault_path": "/path/to/vault",
    "theme": "light",
    "open_tabs": [],
    "active_tab": null,
    "body_font": "Inter",
    "interface_font": "Inter",
    "mono_font": "JetBrains Mono",
    "font_size": 15.0,
    "sidebar_width": 260.0,
    "chat_panel_width": 420.0,
    "model_path": "/path/to/model.gguf",
    "gpu_layers": 99,
    "ctx_size": 8192,
    "max_tool_iterations": 10,
    "ai_provider": "local",
    "api_key": null,
    "api_model": "claude-sonnet-4-6"
}
```

## Recommended models

| Model | Size | VRAM | Quality | Speed | Notes |
|---|---|---|---|---|---|
| Gemma 4 E4B Q4_K_M | 5 GB | 6 GB | Good | Fast | Default recommendation |
| Qwen 2.5 7B Q4_K_M | 4.5 GB | 6 GB | Strong tool calling | Fast | Best for agentic tool use |
| Gemma 4 26B-A4B IQ4_XS | ~10 GB | 12 GB | High | Fast (MoE) | Best quality under 12 GB |
| Llama 3.2 3B Q4_K_M | 2 GB | 3 GB | Decent | Fastest | Low-end hardware |

## Theme compatibility

Forge's CSS uses the Obsidian variable namespace (`--background-primary`, `--text-accent`, `--text-title-h1..h6`, `--interactive-accent`, etc.) and tags DOM elements with Obsidian-compatible class names (`.nav-file-title`, `.workspace-leaf`, `.markdown-source-view.cm-s-obsidian`).

Most popular Obsidian themes (Tokyonight, Wasp, Minimal, Catppuccin, Things, AnuPpuccin) drop in with their core palette + heading styles working. Plugin-specific UI selectors (`.workspace-tab-header-container`, `.vertical-tab-nav-item`, `.community-modal-info`) won't match anything because that UI doesn't exist in Forge — those rules just no-op.

To install a theme: drop a `.css` file into `~/.config/forge/themes/<Name>/theme.css` (loader is the next milestone — for now you can paste theme CSS into a `<style>` block via dev tools to test).

## License

AGPL-3.0
