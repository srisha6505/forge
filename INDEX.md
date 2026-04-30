# Forge Codebase Index

Living map of the project. Update when files, types, commands, or IPC events change.

## Project

**Forge** — Tauri 2 + React + TypeScript markdown editor with an embedded AI research agent.
Frontend is React/CodeMirror 6. Backend is Rust, wired to the frontend via Tauri commands.
The app talks to either a local GGUF model (`llama-cpp-2` + Vulkan) or the Anthropic API.

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│  React UI  (src/)                                        │
│  App.tsx ── LeftRail, Sidebar, Editor, Chat, Resize      │
│     │                                                    │
│     │ invoke(...) / listen("chat://...")                 │
│     ▼                                                    │
├──────────────────────────────────────────────────────────┤
│  Tauri bridge  (src/lib/tauri.ts)                        │
│  Typed wrappers around invoke + event listeners          │
│     │                                                    │
│     │ IPC                                                │
│     ▼                                                    │
├──────────────────────────────────────────────────────────┤
│  Rust backend  (src-tauri/src/)                          │
│  lib.rs  ── AppState, Tauri builder, handler registry    │
│  commands.rs ── Tauri command handlers                   │
│  settings.rs ── persisted ~/.config/forge/settings.json  │
│  llm.rs    ── llama-cpp-2 inference thread + Anthropic   │
│  agent.rs  ── agent loop, tool execution, system prompt  │
│  search.rs ── vault embedding + vector search            │
│  embedder.rs ── local embeddings via candle              │
│  auth.rs   ── Anthropic OAuth (unused by current UI)     │
└──────────────────────────────────────────────────────────┘
```

---

## Frontend Index (React / TypeScript)

### `src/main.tsx` (10 lines)
Entry point. Mounts `<App/>` into `#root` with React StrictMode.

---

### `src/App.tsx` (171 lines)
Top-level shell. Holds vault/tree/active-file/sidebar/chat state in local `useState` and
assembles the four-pane layout: `[LeftRail] [Sidebar?] [Editor] [Chat?]`.

**State:**
- `vault: string | null`, `tree: TreeNode | null`
- `activePath: string | null`, `activeContent: string`
- `activeTab: "files" | "search" | "chat" | "settings"`
- `sidebarVisible`, `chatVisible`, `sidebarWidth`, `chatWidth`
- Bounds: `MIN_SIDEBAR=180`, `MAX_SIDEBAR=480`, `MIN_CHAT=280`, `MAX_CHAT=640`

**Behaviour:**
- On mount → `currentVault()` + `listVaultTree()` to restore last vault
- `pickVault()` → Tauri dialog `open({directory:true})` → `openVault()` → reload tree
- `openFile(path)` → `readFile()` → sets `activePath` + `activeContent`
- `saveActive(content)` → `writeFile()` on each change (no debounce yet)
- `handleRailChange(tab)` → toggles sidebar when clicking current tab, otherwise switches

---

### `src/components/LeftRail.tsx` (66 lines)
Vertical 48px icon rail. Four buttons: Files, Search, Chat, Settings.
- Files / Search / Settings → switch sidebar `activeTab`
- Chat → calls `onToggleChat` (right panel, not sidebar)
- Active tab gets `bg-[var(--bg-hover)] text-[var(--text-accent)]`
- Inline SVG icons (no asset files)

---

### `src/components/Sidebar.tsx` (112 lines)
Left panel showing the vault file tree. Markdown files only.

**Components:**
- `Sidebar` — header with vault name + "Open" button, scrollable file list
- `TreeNodeView` — recursive node. Depth 0 folders open by default. `.md`/`.mdx` extensions hidden from display

**Props:** `vaultName`, `tree: TreeNode`, `activePath`, `onPickVault`, `onOpenFile`

**Where to edit things:**
- Change what files show → this component + `commands::build_tree` backend filter
- Add context menu → new handler in `TreeNodeView`
- Change indent / hover styles → button classNames

---

### `src/components/Editor.tsx` (54 lines)
CodeMirror 6 markdown editor.
- `@uiw/react-codemirror` wrapper
- `markdown({ base: markdownLanguage, codeLanguages: languages })`
- `EditorView.lineWrapping`
- Custom theme + highlight from `lib/cm-theme.ts`
- Empty state when `path === null`
- Header strip shows full path; floating bottom-right label shows file basename

**Shortcomings / edit points:**
- No debouncing on `onChange` → hits `writeFile` every keystroke
- No status bar, no word count, no save indicator

---

### `src/components/Chat.tsx` (273 lines)
Right-side chat panel. Talks to `llm.rs` via Tauri events.

**Key types:**
```ts
type UiMessage =
  | { kind: "user"; content: string }
  | { kind: "assistant"; content: string; streaming: boolean }
  | { kind: "tool"; name: string; args: string; result?: string; isError?: boolean }
  | { kind: "error"; message: string };
```

**State:** `messages`, `input`, `busy`, `modelLabel`, `connected`

**IPC lifecycle:**
1. User clicks **Connect** → `connectInference()` → sets `modelLabel` + `connected`
2. User hits **Send** / Enter → `sendChatMessage(history)` (fire-and-forget)
3. Streaming events arrive:
   - `chat://token` → appended to last assistant bubble (creates one if missing)
   - `chat://tool-start` → pushes a `tool` UiMessage (`running…`)
   - `chat://tool-result` → walks backward to match unfulfilled tool by name, sets `result` + `isError`
   - `chat://done` → marks last assistant bubble as non-streaming; `busy=false`
   - `chat://error` → appends error; `busy=false`

**Rendering:**
- `MessageBlock` dispatches by `kind`
- User → right-aligned bubble (`bg-accent`, max 85% width)
- Assistant → full-width `prose-chat` with `ReactMarkdown` (remark-gfm + rehype-highlight)
- Tool → single line: `tool <name> <summary> <status>` with accent left border
- `summariseArgs(name, argsJson)` — per-tool one-line preview (query for search, path for read_file, etc.)

**Current bugs / rough edges:**
- Tool-result matching is loose (`name + no result yet`) — two parallel calls of same tool will mismatch
- No copy button on messages yet
- `modelLabel` only reflects the last `connect()` — no reconnect on settings change
- No abort / stop button

---

### `src/components/ResizeHandle.tsx` (66 lines)
3px vertical drag strip. `onMouseDown` captures pointer, emits `onResize(delta)` on each
`mousemove`, releases on `mouseup`. Parent clamps.
- `side: "left" | "right"` prop exists but unused
- Body cursor set to `col-resize` during drag

---

### `src/lib/tauri.ts` (122 lines)
Typed invoke wrappers + event listeners. **Single source of truth for the React↔Rust contract.**

**Types:**
- `VaultEntry { name, path, is_dir }`
- `TreeNode { name, path, is_dir, children }`
- `Settings { ... }` — full settings shape mirroring `src-tauri/src/settings.rs`
- `ChatTurn { role: "user"|"assistant", content }`
- `ConnectResult { model_name }`
- `ToolStartPayload { name, args }`, `ToolResultPayload { name, content, is_error }`
- `SearchHit { path, title, snippet, score }`

**Commands (invoke):**

| Category | Exports |
|---|---|
| Settings | `getSettings`, `setSettings` |
| Vault | `currentVault`, `openVault`, `listVaultFiles`, `listVaultTree` |
| Files | `readFile`, `writeFile`, `renameFile`, `deleteFile` |
| Search | `searchVault` (stub, returns []) |
| Inference | `connectInference`, `sendChatMessage`, `stopChat` |

**Chat events (listen):**

| Event | Payload | Handler |
|---|---|---|
| `chat://token` | `string` | `onChatToken` |
| `chat://thinking` | `string` | `onChatThinking` |
| `chat://tool-start` | `ToolStartPayload` | `onChatToolStart` |
| `chat://tool-result` | `ToolResultPayload` | `onChatToolResult` |
| `chat://done` | `void` | `onChatDone` |
| `chat://error` | `string` | `onChatError` |

---

### `src/lib/cm-theme.ts` (84 lines)
CodeMirror 6 theme + `HighlightStyle` using the warm-dark palette from `index.css`.
- `forgeTheme` — editor colours, caret, selection, hidden gutters
- `forgeMarkdownHighlight` — heading sizes, strong/em, monospace, links, list markers
- Export: `forgeMarkdownExtensions = [forgeTheme, syntaxHighlighting(forgeMarkdownHighlight)]`

---

### `src/index.css` (301 lines)
Tailwind base + custom palette + two prose sheets.

**CSS custom properties (`:root`):**

| Group | Variables |
|---|---|
| Backgrounds | `--bg-base`, `--bg-primary`, `--bg-secondary`, `--bg-tertiary`, `--bg-hover`, `--bg-active`, `--bg-modal` |
| Borders | `--border-subtle`, `--border`, `--border-strong` |
| Text | `--text-normal`, `--text-muted`, `--text-faint`, `--text-accent`, `--text-on-accent` |
| Accents | `--accent`, `--accent-hover`, `--accent-soft` |
| Semantic | `--error`, `--success` |

**Prose sheets:**
- `.prose-chat` (lines 82-192) — react-markdown output in the chat panel
- `.cm-editor` token styles (lines 194-301) — `tok-heading`, `tok-strong`, `tok-emphasis`, `tok-inlineCode`, `tok-link`, `tok-list` etc.

---

## Backend Index (Rust / Tauri)

### `src-tauri/src/main.rs` (6 lines)
Thin wrapper: `fn main() { forge_lib::run(); }`

---

### `src-tauri/src/lib.rs` (61 lines)
Tauri builder + `AppState`.

**AppState:**
```rust
pub struct AppState {
    pub inference: Mutex<Option<llm::InferenceHandle>>,
    pub settings: Mutex<settings::Settings>,
    pub vault_path: Mutex<Option<PathBuf>>,
}
```

**Registered commands** (13): `get_settings`, `set_settings`, `open_vault`, `current_vault`,
`list_vault_files`, `list_vault_tree`, `read_file`, `write_file`, `rename_file`, `delete_file`,
`search_vault`, `connect_inference`, `send_chat_message`, `stop_chat`.

**Plugins:** `tauri-plugin-dialog`, `tauri-plugin-fs`, `tauri-plugin-shell`.

---

### `src-tauri/src/commands.rs` (433 lines)
Tauri command handlers. Every `#[tauri::command]` takes `State<'_, AppState>` and returns
`Result<T, String>` so the frontend gets stringified errors.

**Types:** `VaultEntry`, `TreeNode`, `SearchHit`, `ConnectResult`, `ChatTurn`.

**Commands by section:**

| Lines | Section | Commands |
|---|---|---|
| 16-27 | Settings | `get_settings`, `set_settings` |
| 46-173 | Vault | `current_vault`, `open_vault`, `list_vault_tree` + `build_tree`, `list_vault_files` |
| 177-247 | File IO | `read_file`, `write_file`, `rename_file`, `delete_file` (all `resolve_within_vault`-guarded) |
| 259-268 | Search | `search_vault` (stub) |
| 277-303 | Inference | `connect_inference` (local or Anthropic based on `ai_provider`) |
| 315-409 | Chat | `send_chat_message` (spawns `forge-agent` thread) + `forward_agent_event` + `stop_chat` (noop) |
| 414-432 | Helpers | `resolve_within_vault` (canonicalize + prefix check) |

**Chat plumbing:** `send_chat_message` builds `Vec<ChatMessage>`, prepends system prompt from
`agent::default_system_prompt(vault_name)`, spawns a thread which:
1. Creates a fresh mpsc channel
2. Spawns a nested thread running `agent::run_agent_loop`
3. Drains events and forwards each as `window.emit("chat://...")`
4. Breaks on `Finished` or `Error`

---

### `src-tauri/src/settings.rs` (144 lines)
Persisted config in `~/.config/forge/settings.json`.

**Struct fields** (all `#[serde(default)]`):
`last_vault_path`, `theme`, `open_tabs`, `active_tab`, `body_font`, `interface_font`,
`mono_font`, `font_size`, `sidebar_width`, `model_path`, `gpu_layers`, `ctx_size`,
`chat_panel_width`, `max_tool_iterations`, `ai_provider`, `api_key`, `api_model`.

**Defaults:**
- Fonts: `DejaVu Sans` (body + interface), `DejaVu Sans Mono` (mono), `15.0` px
- Sidebar: `260.0` px, Chat: `400.0` px
- Inference: `gpu_layers = 99`, `ctx_size = 8192`, `max_tool_iterations = 10`
- Provider: `"local"`, API fallback model: `"claude-sonnet-4-6"`

**Methods:** `load()`, `save()`, `resolved_vault_path()`, `set_vault(path)`.

**Constants:** `BODY_FONTS`, `MONO_FONTS` — curated family lists for a future settings UI.

---

### `src-tauri/src/llm.rs` (1429 lines)
Inference backend — both local GGUF and Anthropic API.

**Public types:**
- `ChatRole` — System | User | Assistant | Tool
- `ToolCall { id, name, arguments: serde_json::Value }`
- `ChatMessage { role, content, tool_calls, tool_call_id }` with constructors `system` / `user` / `assistant` / `assistant_with_tool_calls` / `tool_result`
- `InferenceRequest { messages, tools, response_tx }`
- `InferenceEvent` — Token | Thinking | ToolUse | Done | Error
- `InferenceHandle { tx, model_name }` with `generate(messages, tools) -> Receiver<InferenceEvent>`
- `AnthropicAuth` — ApiKey | OAuth

**Public functions:**
- `spawn_inference_thread(model_path, n_gpu_layers, n_ctx) -> Result<InferenceHandle, String>` — starts llama-cpp-2 thread with Vulkan
- `spawn_anthropic_thread(auth, model) -> Result<InferenceHandle, String>` — mimics the same channel interface against the HTTP API

**Pipeline (local, line ranges approx):**

| Lines | What |
|---|---|
| 107-140 | `spawn_inference_thread` — model load + thread spawn |
| 141-202 | `inference_loop` — owns `LlamaContext`, pulls from request channel |
| 203-413 | `process_request` — tokenise, sample, stream tokens, parse tool calls |
| 414-479 | `format_prompt` — Gemma chat template |
| 480-557 | `format_prompt_with_injected_tools` — injects tool list into system turn |
| 572-706 | `try_parse_tool_call` — full parser dispatch |
| 707-728 | `parse_gemma_tool_call` — `call:name{k:v,...}` native format |
| 729-751 | `parse_function_call_syntax` — `name(k=v, ...)` fallback |
| 752-894 | `parse_kv_anchored` + `find_key_anchor` + `clean_anchored_value` — position-anchored parser that survives unescaped quotes in values |
| 915-1102 | `parse_kv_map` + `parse_kv_value` — classical recursive-descent fallback |
| 1118-1152 | `find_json_end`, `parse_tool_json` — JSON tool-call fallback |
| 1153-1192 | `might_be_tool_call_start`, `extract_pre_tool_text`, `strip_tool_markers` — streaming splitter that keeps pre-call text and hides the call tag from the UI |
| 1194-1429 | Anthropic backend: `AnthropicAuth`, `spawn_anthropic_thread`, `anthropic_request` |

---

### `src-tauri/src/agent.rs` (1262 lines)
Agent loop and tool execution.

**Types:**
- `AgentEvent` — Token | Thinking | ToolCallStarted | ToolCallResult | Finished{messages} | Error
- `ToolContext { vault_path, db_path }`
- `ToolResult { content, is_error }`

**Public API:**
- `tool_schemas() -> Vec<serde_json::Value>` (line 114) — JSON schema for all 10 tools (passed to model)
- `execute_tool(tool_call, ctx) -> ToolResult` (line 325) — dispatches to `exec_*`
- `default_system_prompt(vault_name) -> String` (line 1104) — instructs the model to chain tools, prefer search, never guess paths
- `run_agent_loop(inference, messages, tools, ctx, max_iters, event_tx)` (line 1124) — blocking loop that streams tokens, executes tools, appends assistant+tool messages, breaks on Finished/Error or 2× consecutive same-call failures

**Tool executors (line → fn):**

| Line | Tool | Purpose |
|---|---|---|
| 344 | `exec_search_vault` | SQLite FTS search over vault chunks |
| 432 | `exec_read_file` | `fs::read_to_string`, vault-scoped |
| 459 | `exec_list_files` | `fs::read_dir`, `.md` only |
| 514 | `exec_read_section` | Reads a heading chunk from a file |
| 580 | `exec_write_file` | Create/overwrite with aggressive field aliases (`path`/`file`/`filename`/…) + filename rescue |
| 684 | `exec_edit_file` | Replace `old_text` → `new_text` in a file |
| 747 | `exec_rename_file` | Move inside vault |
| 798 | `exec_delete_file` | Delete inside vault |
| 820 | `exec_web_search` | DuckDuckGo HTML scraper (parses `<a class="result__a">` up to `</a>`) |
| 919 | `exec_grep_vault` | Recursive regex search across `.md` files |

**Helpers:** `validate_vault_path`, `urlencoded`, `urldecoded`, `extract_between`, `strip_html_tags`.

**Loop-break guard:** `last_call_sig` + `consecutive_failures`. If the same failing tool call
fires twice with identical args, the loop emits an Error + Finished and returns. Prevents
infinite loops on a parse-bug model.

---

### `src-tauri/src/search.rs` (458 lines)
Vault embedding + vector search. **Not yet wired into `search_vault` command** (stub returns []).

- `Chunk { id, path, title, heading, text, start, end }`
- `SearchResult { chunk, score }`
- `VaultSearch` — sqlite DB + `usearch` index
- `collect_md_files(root)`, `chunk_file(content)` — chunker splits by heading boundaries

---

### `src-tauri/src/embedder.rs` (67 lines)
Local embeddings via `candle-transformers`. Wraps a model loaded through `hf-hub`.
`LocalEmbedder` holds the model + tokenizer and exposes `embed(text) -> Vec<f32>`.

---

### `src-tauri/src/auth.rs` (428 lines)
Anthropic OAuth PKCE flow + API key creation. **Not currently invoked by the UI.**
`login()` runs a local callback server, exchanges the code, calls `create_api_key(access_token)`.
`get_auth_header()` returns `(header_name, header_value)` for either OAuth or classic API key.

---

## Tauri Config

### `src-tauri/Cargo.toml` (40 lines)
| Crate | Purpose |
|---|---|
| `tauri = "2"` + plugins `dialog`, `fs`, `shell` | Shell, plugins |
| `serde`, `serde_json` | IPC serialisation |
| `dirs = "5"` | Config dir |
| `rusqlite = "0.31"` (bundled) | Vault search DB |
| `usearch = "2"` | Vector index |
| `ureq = "2"` (json) | HTTP for Anthropic + DuckDuckGo + OAuth |
| `openssl = "0.10"` (vendored) | TLS |
| `candle-core` / `candle-nn` / `candle-transformers = "0.8"` | Local embeddings |
| `hf-hub = "0.5"`, `tokenizers = "0.20"` | Model + tokenizer downloads |
| `notify = "7"` | Filesystem watcher (not wired yet) |
| `llama-cpp-2 = "0.1"` (feature `vulkan`) | Local GGUF inference |

### `package.json` (40 lines)
**Runtime:** `react 18`, `@tauri-apps/api 2`, `@uiw/react-codemirror`, `@codemirror/lang-markdown`,
`@codemirror/state`, `@codemirror/view`, `@codemirror/language-data`, `react-markdown 9`,
`remark-gfm 4`, `rehype-highlight 7`.
**Dev:** `@tauri-apps/cli 2`, `vite 6`, `typescript 5.7`, `tailwindcss 3`, `highlight.js 11`.

### `src-tauri/tauri.conf.json`
Tauri window / bundle config (title, default size, bundle identifier). Review before release.

---

## IPC Contract Reference

### Invoke commands

| TS export | Rust handler | Args | Returns |
|---|---|---|---|
| `getSettings()` | `get_settings` | — | `Settings` |
| `setSettings(s)` | `set_settings` | `new: Settings` | `void` |
| `currentVault()` | `current_vault` | — | `string \| null` |
| `openVault(path)` | `open_vault` | `path: string` | `VaultEntry[]` |
| `listVaultFiles(subPath?)` | `list_vault_files` | `subPath?: string` | `VaultEntry[]` |
| `listVaultTree()` | `list_vault_tree` | — | `TreeNode` |
| `readFile(path)` | `read_file` | `path: string` | `string` |
| `writeFile(path, content)` | `write_file` | `path, content: string` | `void` |
| `renameFile(from, to)` | `rename_file` | `from, to: string` | `void` |
| `deleteFile(path)` | `delete_file` | `path: string` | `void` |
| `searchVault(query, limit?)` | `search_vault` | `query: string, limit?: number` | `SearchHit[]` (stub) |
| `connectInference()` | `connect_inference` | — | `ConnectResult` |
| `sendChatMessage(history)` | `send_chat_message` | `history: ChatTurn[]` | `void` (events fire on `chat://*`) |
| `stopChat()` | `stop_chat` | — | `void` (noop) |

### Chat events (backend → frontend)

| Event | Payload |
|---|---|
| `chat://token` | `string` |
| `chat://thinking` | `string` |
| `chat://tool-start` | `{ name, args }` |
| `chat://tool-result` | `{ name, content, is_error }` |
| `chat://done` | `void` |
| `chat://error` | `string` |

---

## Known gaps / obvious improvement targets

Use this as a worklist when the user says "make things better."

**Editor (`Editor.tsx`, `cm-theme.ts`):**
- No save debounce → every keystroke hits Tauri `writeFile`. Add `useDebouncedCallback` around `saveActive`.
- No dirty indicator, word count, or save status.
- CodeMirror theme does not implement Obsidian-style "live preview" (syntax markers stay visible at all times).
- No image preview, no embed rendering, no wikilink resolution.

**Sidebar (`Sidebar.tsx`):**
- Only `.md`/`.markdown` files are shown (enforced in backend `build_tree`). No folders-with-non-md, no images.
- No right-click context menu (new, rename, delete).
- No drag-and-drop re-order.
- Empty folders are dropped by `build_tree` (line 104), which surprises users.

**Chat (`Chat.tsx`):**
- Tool-result → tool-call matching walks by `name`, so parallel calls of the same tool get misaligned.
- No abort button; `stopChat` is a noop backend-side.
- No copy-to-clipboard on assistant messages.
- `connect()` label never updates when the user changes settings.
- No persistence — messages vanish on reload.
- No token count / cost readout.

**LeftRail / global:**
- Search, Chat (sidebar pane), Settings panes all say "coming soon."
- No keybindings yet (no `ctrl+s`, `ctrl+o`, `ctrl+p`).
- No command palette.

**Backend:**
- `search_vault` command is a stub (`Ok(vec![])`). Need to adapt `search.rs` to the Tauri state shape.
- `stop_chat` is a noop — needs cooperative cancellation.
- `list_vault_tree` drops empty folders (`commands.rs:104`) — fix if users want empty folders visible.
- `notify` crate is listed but no filesystem watcher is wired up → external file changes are invisible to the UI.

---

## How to add X (frontend)

### Add a new Tauri command
1. Write the Rust handler in `src-tauri/src/commands.rs`
2. Register it in `src-tauri/src/lib.rs` `tauri::generate_handler![...]`
3. Add a typed wrapper in `src/lib/tauri.ts`
4. Call it from a component via the typed wrapper (never raw `invoke`)

### Add a new chat event
1. Emit with `window.emit("chat://xxx", payload)` inside `forward_agent_event` (or directly)
2. Type the payload + wrapper in `src/lib/tauri.ts` (`onChatXxx`)
3. Handle it in `Chat.tsx`'s `useEffect` subscription list

### Change the palette
Edit `:root` CSS variables in `src/index.css`. All components read them via `var(--...)`.

### Add a new sidebar pane
1. Extend the `activeTab` union type in `App.tsx` (+ `LeftRail.tsx` props)
2. Add an icon button in `LeftRail.tsx`
3. Add a `{activeTab === "new" && (...)}` branch in `App.tsx`'s sidebar block

### Add a component-level keybinding
Currently none exist. The pattern will be:
- Global: `useEffect` on `window.addEventListener("keydown", ...)` in `App.tsx`
- Editor-local: CodeMirror `keymap.of([...])` extension in `cm-theme.ts`

---

## Addendum — modules not yet folded into the sections above (2026-04-23)

The above sections are stale relative to the current source tree. New files
that exist on disk but are not documented above. Each gets a one-line
summary; full per-section integration pending.

### Frontend components
- `src/components/BacklinksPanel.tsx` — backlinks for active file via `listBacklinks`. Uses `links.rs` indexer.
- `src/components/GraphView.tsx` (332 lines) — full vault graph, force-directed via `react-force-graph-2d` + `d3-force`, search, click-to-open.
- `src/components/MarkdownPreview.tsx` — md preview pane.
- `src/components/PdfViewer.tsx` (435 lines) — pdfjs-based, worker pinned (see commit `ba971b9`).
- `src/components/LatexViewer.tsx` (629 lines) — save .tex + recompile + render PDF.
- `src/components/ImageViewer.tsx` (365 lines) — image viewer.
- `src/components/DocxViewer.tsx` — DOCX render via mammoth.
- `src/components/SettingsModal.tsx` (964 lines), `AppearanceModal.tsx` — split settings.
- `src/components/SearchModal.tsx` (383 lines), `Search.tsx` (233 lines) — search UI.
- `src/components/VoiceInput.tsx`, `ConversationToggle.tsx` — voice input + conversation mode toggle.
- `src/components/ErrorBoundary.tsx` — top-level React error boundary.
- `src/components/chat/` — `ChatComposer.tsx`, `ChatToolbar.tsx`, `RunningIndicator.tsx`, `ToolCallCard.tsx`.

### Frontend lib
- `src/lib/cm-wikilinks.ts` (214 lines) — `[[target]]` / `[[target|alias]]` widget; cursor-line aware (Obsidian-style live preview).
- `src/lib/cm-hyperlinks.ts` (81 lines) — markdown hyperlink widgets.
- `src/lib/cm-math.ts` (256 lines) — inline + display math rendering.
- `src/lib/cm-markdown-render.ts` (427 lines) — main render layer.
- `src/lib/file-types.ts` — file type detection / routing.

### Rust modules
- `src-tauri/src/links.rs` (196 lines) — wikilink scanner + forward/reverse graph. Supports `[[t]]`, `[[t|alias]]`, `[[t#heading]]`.
- `src-tauri/src/copilot.rs` (234 lines) — GitHub Copilot API client; device-flow OAuth + token exchange to use a Copilot subscription as a chat backend.
- `src-tauri/src/voice.rs` (774 lines) — conversation orchestrator: continuous mic → VAD → STT → LLM → TTS hands-free loop with UI events.
- `src-tauri/src/vad.rs` (136 lines) — Silero VAD via ONNX Runtime.
- `src-tauri/src/stt.rs` (263 lines) — whisper.cpp wrapper, routes through `binaries::resolve_whisper_cli`.
- `src-tauri/src/deepgram.rs` (349 lines) — Deepgram cloud STT + TTS.
- `src-tauri/src/edge_tts.rs` (209 lines) — Microsoft Edge TTS (v1 primary TTS).
- `src-tauri/src/gtts.rs` (133 lines) — Google TTS fallback.
- `src-tauri/src/latex.rs` (190 lines) — LaTeX compile: tectonic → xelatex → pdflatex fallback. Cache per source under OS temp.
- `src-tauri/src/models.rs` (454 lines) — managed downloadable model catalog (GGUF, whisper, piper). Discover/download/delete/reference by ID.
- `src-tauri/src/binaries.rs` (354 lines) — installs whisper-cli (built from source via cmake) and piper into managed dir.

### v1 plan
- Full per-component build + test catalog at `<repo>/V1_COMPONENTS.md`. Read it before any v1 feature work. Scope decisions (what's bundled, what's downloaded on demand, what's deliberately out) are at the top of that file.

### Conversation persistence
- `<repo>/CONVCORRECT.md` + `.claude/log-turn.sh` + `.claude/conv-log.md` — Stop hook system that survives compaction. Read `conv-log.md` first on every session.
