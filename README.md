# Forge

A fast, GPU-accelerated markdown editor with a built-in local AI research agent. Reads Obsidian-style vaults. Single binary, no Electron, no browser runtime, no cloud dependency.

Built on [GPUI](https://github.com/zed-industries/zed) (Zed's GPU rendering engine) for native performance.

## Highlights

- Opens Obsidian vaults as-is, no migration needed
- Live preview markdown editing with syntax hiding on inactive lines
- Local AI chat agent powered by any GGUF model (Gemma, Llama, Qwen, Mistral, etc.)
- Hybrid search: BM25 keyword + vector semantic search across all notes
- Force-directed graph view of note connections
- Single ~15MB binary, starts in under a second

## Installation

### AppImage (recommended)

Download the latest `Forge-x86_64.AppImage` from [Releases](https://github.com/srisha6505/forge/releases), then:

```bash
chmod +x Forge-x86_64.AppImage
./Forge-x86_64.AppImage
```

No installation required. Works on any Linux distro with Vulkan GPU drivers.

### Build from source

Requires Rust 1.80+ and a C/C++ toolchain.

```bash
# System dependencies (Ubuntu/Debian)
sudo apt install cmake pkg-config libvulkan-dev glslc \
    libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev

# Build
git clone https://github.com/srisha6505/forge.git
cd forge
cargo build --release

# Run
./target/release/forge
```

#### Build dependencies by distro

| Distro | Packages |
|---|---|
| Ubuntu/Debian | `cmake pkg-config libvulkan-dev glslc libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev` |
| Fedora | `cmake gcc-c++ vulkan-devel glslc libxkbcommon-devel libxcb-devel` |
| Arch | `cmake vulkan-devel shaderc libxkbcommon libxcb` |

### Build AppImage locally

```bash
./packaging/build-appimage.sh
# Output: Forge-x86_64.AppImage
```

## Getting Started

1. Run Forge
2. Press **Ctrl+O** to open a folder (any folder with `.md` files, including Obsidian vaults)
3. Click a note in the sidebar to open it
4. Start editing

### Setting up the AI agent

Forge runs AI models locally using [llama.cpp](https://github.com/ggerganov/llama.cpp). No API keys, no internet, your data stays on your machine.

1. Download a GGUF model:

```bash
# Gemma 4 E4B (recommended for <=12GB VRAM)
pip install huggingface_hub
hf download unsloth/gemma-4-E4B-it-GGUF \
    --local-dir ~/.forge/models/gemma-4-E4B \
    --include "*Q4_K_M*"

# Or any other GGUF model:
# - Qwen 2.5 7B (great at tool calling)
# - Llama 3.2 3B (fastest)
# - Gemma 4 26B-A4B (best quality, needs 16GB+ VRAM)
```

2. Add the model path to your config:

```bash
# Edit ~/.config/forge/settings.json
{
    "model_path": "/home/you/.forge/models/gemma-4-E4B/gemma-4-E4B-it-Q4_K_M.gguf",
    "gpu_layers": 99,
    "ctx_size": 8192
}
```

3. Press **Ctrl+Shift+L** to open the chat panel
4. Ask questions about your vault -- the agent will search your notes and synthesize answers

#### GPU configuration

| Setting | Description |
|---|---|
| `gpu_layers: 99` | Offload all layers to GPU (fastest) |
| `gpu_layers: 0` | CPU only (works on any machine) |
| `gpu_layers: 20` | Partial offload (when model doesn't fit in VRAM) |
| `ctx_size: 8192` | Context window (higher = more memory, better for long conversations) |

## Features

### Editor
- Live preview with syntax hiding on inactive lines
- Headings, bold, italic, code spans, code blocks, tables, horizontal rules, lists
- Inline image rendering (`![[image.png]]`)
- Wikilinks (`[[Note]]`) with autocomplete and click-to-navigate
- LaTeX-to-Unicode rendering for math expressions
- Rope-based buffer with undo/redo
- Zoom in/out (Ctrl+=/-)

### AI Agent
- Local GGUF model inference via llama.cpp (Vulkan GPU acceleration)
- Vault-aware tools: search notes, read files, list directories, read sections
- Agentic loop: model calls tools, gets results, continues reasoning
- Multiple chat sessions (tabs)
- Streaming token display
- Collapsible thinking blocks
- Stop generation button
- Markdown rendering in chat responses

### Search
- Hybrid BM25 keyword + vector semantic search
- Local embedding model (all-MiniLM-L6-v2, downloads automatically)
- FTS5 full-text search with porter stemming
- Quote-prefix (`"query`) for keyword-only mode
- Results show file path, heading, and content snippet
- Click result to jump to exact location in file

### Navigation
- File tree sidebar with folders, right-click context menus
- Tab management (Ctrl+Tab, Ctrl+W)
- Back/forward history (Alt+Left/Right)
- Backlinks panel
- Force-directed graph view

### Settings
- Font family selection (body, interface, monospace)
- Font size
- Resizable sidebar and chat panel
- Dark/light theme toggle
- All settings persisted in `~/.config/forge/settings.json`

## Keyboard Shortcuts

### Global

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
| Ctrl+Shift+F | Open search |
| Ctrl+Shift+L | Toggle AI chat panel |
| F5 / Ctrl+R | Refresh vault |

### Editor

| Key | Action |
|---|---|
| Ctrl+B | Bold |
| Ctrl+I | Italic |
| Ctrl+Shift+C | Inline code |
| Ctrl+Z / Ctrl+Y | Undo / redo |
| Ctrl+Shift+D | Duplicate line |
| Ctrl+= / Ctrl+- / Ctrl+0 | Zoom in / out / reset |
| PageUp / PageDown | Scroll by page |
| Ctrl+Home / Ctrl+End | Jump to start / end |

### Chat

| Key | Action |
|---|---|
| Enter | Send message |
| Ctrl+Shift+L | Close panel |

## Architecture

```
src/
  main.rs        -- entry point, module declarations
  app.rs         -- window shell, sidebar, tabs, panels, keybindings
  buffer.rs      -- rope-based text buffer, cursor, selection, undo
  editor.rs      -- GPUI custom Element, live preview, cursor, input
  markdown.rs    -- pulldown-cmark parser, block/inline annotations
  links.rs       -- wikilink index, backlink tracking
  graph.rs       -- force-directed graph view (Fruchterman-Reingold)
  search.rs      -- SQLite FTS5 + usearch HNSW hybrid search
  embedder.rs    -- local embedding model (candle + all-MiniLM-L6-v2)
  llm.rs         -- GGUF model inference server (llama-cpp-2, dedicated thread)
  agent.rs       -- agentic tool-use loop, vault tools, tool schemas
  chat.rs        -- chat panel UI entity, message rendering, streaming
  icons.rs       -- div-based icon shapes
  settings.rs    -- persistent config (~/.config/forge/settings.json)
  theme.rs       -- design tokens
```

### Data flow

```
User types in chat
  -> ChatPanel sends message to agent thread
    -> Agent builds prompt with tool schemas
      -> InferenceHandle sends to dedicated llama.cpp thread
        -> Tokens stream back via mpsc channel
          -> Agent detects tool_use -> executes vault tools -> loops
        -> Text tokens stream to ChatPanel UI
      -> ChatPanel renders with markdown formatting
```

### Key design decisions

- **Single binary**: everything compiles into one executable, no runtime dependencies beyond Vulkan drivers
- **No async runtime**: GPUI has its own executor. Background work uses OS threads + mpsc channels
- **Dedicated inference thread**: `LlamaContext` is `!Send`, so it lives on one thread for its entire lifetime. Requests queue via channels
- **Lazy model loading**: model loads when chat panel first opens, not at startup
- **Read-only tool DB**: agent tools open their own SQLite connection to avoid sharing state with the main search index

## Configuration

All settings are in `~/.config/forge/settings.json`:

```json
{
    "last_vault_path": "/path/to/vault",
    "theme": "dark",
    "body_font": "Inter",
    "mono_font": "JetBrains Mono",
    "font_size": 15.0,
    "sidebar_width": 260.0,
    "model_path": "/path/to/model.gguf",
    "gpu_layers": 99,
    "ctx_size": 8192,
    "chat_panel_width": 400.0,
    "max_tool_iterations": 10
}
```

| Field | Description | Default |
|---|---|---|
| `model_path` | Path to GGUF model file | none |
| `gpu_layers` | Layers to offload to GPU (0 = CPU, 99 = all) | 99 |
| `ctx_size` | Model context window | 8192 |
| `chat_panel_width` | Chat panel width in pixels | 400 |
| `max_tool_iterations` | Max tool-use rounds per query | 10 |
| `body_font` | Editor body font family | DejaVu Sans |
| `mono_font` | Monospace font family | DejaVu Sans Mono |
| `font_size` | Base font size | 15.0 |
| `sidebar_width` | Sidebar width in pixels | 260 |

## Recommended Models

| Model | Size | VRAM | Quality | Speed | Best for |
|---|---|---|---|---|---|
| Gemma 4 E4B Q4_K_M | 5 GB | 6 GB | Good | Fast | General use |
| Qwen 2.5 7B Q4_K_M | 4.5 GB | 6 GB | Good (great at tools) | Fast | Tool calling |
| Gemma 4 26B-A4B IQ4_XS | ~10 GB | 12 GB | Great | Fast (MoE) | Best quality under 12GB |
| Llama 3.2 3B Q4_K_M | 2 GB | 3 GB | Decent | Fastest | Low-end hardware |

## License

AGPL-3.0
