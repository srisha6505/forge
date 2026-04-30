# Forge — AI MVP spec

Scope: the inference layer, tool harness, terminal, chat UI, and voice
surface. Paired with `mdeditor.md` which owns the render pipeline — this
doc never re-specifies rendering, only the content that flows into it.

Status: design. Nothing here is implemented yet. Update per-section.

The MVP's pitch is "Obsidian + a real agent + voice, local-first." This
doc is what "a real agent" means concretely.

---

## 1. Security posture (read first)

Three rules, violated at peril.

1. **Agent tools are vault-sandboxed.** No bash, no `exec`, no
   arbitrary-process spawn, no writes outside the vault, no sudo. A
   user who needs shell-level agency opens the built-in terminal
   (§3) and runs their own CLI agent (Claude Code, Codex, Gemini
   CLI, etc.). This separation IS the security model.
2. **API keys live in the OS keyring.** `keyring` crate —
   macOS Keychain / Windows Credential Store / Linux Secret Service.
   Plaintext-in-settings.json is acceptable only as a fallback when
   keyring is unavailable AND the UI shows a visible warning.
3. **Tool calls cross providers via one normalised internal shape:**
   `ToolCall { id, name, arguments: serde_json::Value }`. Provider
   threads normalise at the boundary. Agent loop is provider-agnostic.

---

## 2. Providers

### 2.1 Lineup

| Provider | Auth | Status today | Priority |
|---|---|---|---|
| Anthropic | API key (+ OAuth PKCE, dead code) | shipped | — |
| GitHub Copilot | device-flow OAuth | shipped | — |
| Local GGUF (in-process) | none | shipped | — |
| OpenAI | API key | **add** | P0 |
| OpenRouter | API key (OpenAI-compatible) | **add** | P0, trivial once OpenAI exists |
| Google Gemini | API key | **add** | P0 |
| OpenAI-compatible generic | API key + base URL | **add** | P0, covers Ollama / LM Studio / llama-server / Jan |

Generic OpenAI-compatible covers local-model servers too — user points
it at `http://localhost:11434/v1` for Ollama, `http://localhost:1234/v1`
for LM Studio, etc. No separate "Ollama" provider needed.

### 2.2 Tool-call normalisation gotchas

| Provider | Where tool calls live | ID present | Arguments type |
|---|---|---|---|
| OpenAI | `choices[].message.tool_calls[]` | yes | **JSON-encoded string** (parse it) |
| Anthropic | content block `tool_use` | yes | object |
| Gemini | `candidates[].content.parts[].functionCall` | **no — synthesise** | object |
| Copilot | OpenAI-compatible | yes | JSON string |
| OpenRouter | OpenAI-compatible | yes | JSON string |
| Local GGUF | model-emitted text, custom parsers in `llm.rs` | none — synthesise | object (after parse) |

Synthesise deterministically: `sha1(name + index + json(args))[..8]`.
Matters because tool-result-to-call matching is by ID (§4.4).

### 2.3 Model catalogue discovery

- OpenAI: `GET /v1/models` — filter to chat-completion models.
- OpenRouter: `GET /v1/models` — rich metadata including context window.
- Gemini: `GET /v1beta/models?key=...` — filter to generateContent-capable.
- Ollama: `GET /api/tags` for installed, `GET /api/show` for per-model metadata (context window, quantisation).
- Anthropic: **no public listing endpoint**. Hardcode a list in
  `models.rs`; document the update cadence (~2 models/quarter).
- Copilot: dedicated endpoint at `https://api.githubcopilot.com/models`.

UX: after a user pastes a key, hit the health-check endpoint, then
populate the model dropdown from the live catalogue. Cache the
response for 24 h.

### 2.4 Multimodal

Vision content formats differ per provider. Normalise at the edge.

Forge internal:
```
ContentBlock::Image {
    source: ImageSource::Base64 { media_type, data } | ImageSource::Url(url),
}
```

Per-provider payload shape (to build at the boundary):

- OpenAI: `{type: "image_url", image_url: {url: "data:image/png;base64,..." | "https://..."}}`
- Anthropic: `{type: "image", source: {type: "base64", media_type, data}}` or `{type: "url", url}`
- Gemini: `{inlineData: {mimeType, data}}` or `{fileData: {fileUri}}`

v1 coverage: PNG / JPEG / WebP. SVG via rasterise-first (skip v1).

### 2.5 Routing per task slot

Users pick a provider+model per task rather than switching mid-task.

| Slot | Typical pick |
|---|---|
| **Chat default** | Claude Sonnet / GPT-4o / Gemini 2 Pro |
| **Fast suggestions** | Haiku / GPT-4o-mini / Gemini Flash |
| **Summarise (context compactor)** | Haiku / Flash — cheap |
| **Embed** | `text-embedding-3-small` / Gemini `text-embedding-004` / local candle |

Stored as `settings.ai_routing = { chat, fast, summarise, embed }`.
Each entry is `{provider_id, model_id}`. Local candle embedding stays
available with no API key needed — that's the privacy default.

---

## 3. Built-in terminal

The escape hatch for users who want CLI agents (Claude Code / Codex /
Gemini CLI) or raw shell access.

**Renderer:** `xterm.js` (frontend). Already widely used, handles ANSI,
scrollback, copy-paste, resize.

**Backend:** `portable-pty` crate (sync or async — pick async to match
the voice pipeline). Tauri's built-in `shell` plugin does not do
interactive PTYs; don't try to force it.

**Defaults:**
- Working directory: vault root.
- Shell: `$SHELL` on Unix, `pwsh` / `cmd` on Windows (probed).
- Env inheritance: the process env, PLUS API keys from AI settings —
  `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`,
  `OPENROUTER_API_KEY`, `GITHUB_TOKEN` (if Copilot logged in). Opt-out
  toggle for privacy-first users.
- Multi-tab, resize, reflow, Ctrl-Shift-C / Ctrl-Shift-V for clipboard.

**Bind:** left-rail icon + `Ctrl+` ` ` ` to toggle a bottom panel, or a
full-pane terminal via the tab system.

**Security:** the terminal is user-operated. Forge's **agent** has no
way to reach it. Do not ever wire a "run_in_terminal" tool into the
agent — that defeats §1.1.

**Env attachment policy:** env vars are snapshotted at spawn time from
current settings. If the user disables a provider in AI settings, new
terminal sessions no longer inherit that key. Existing sessions keep
theirs (can't un-set env on a running process without killing it).

---

## 4. The Forge harness (agent tool set)

### 4.1 Design rules

- Every tool operates on vault-scoped paths. Rust-side, resolve through
  `commands::resolve_within_vault` (symlink-aware).
- No tool writes outside the vault.
- No tool spawns a subprocess except `compile_latex` (tectonic,
  tightly-scoped with `--no-shell-escape`).
- Every tool has a timeout (default 15 s, configurable per tool).
- Every tool returns `{ content: String, is_error: bool }` — same shape
  as today.
- Tools declarable-disabled per chat. Settings panel has a master list.

### 4.2 Inventory

| Tool | Status | Notes |
|---|---|---|
| `read_file(path)` | shipped | |
| `write_file(path, content)` | shipped | must go through the §3.2 atomic helper from mdeditor.md |
| `edit_file(path, old_text, new_text)` | shipped | error if `old` not unique |
| `rename_file(from, to)` | shipped | update backlinks |
| `list_files(dir?)` | shipped | |
| `read_section(path, heading)` | shipped | |
| `search_vault(query)` | shipped (vector) | |
| `grep_vault(pattern)` | shipped | Rust regex |
| `web_search(query)` | shipped (DDG) | |
| `bm25_search(query, k)` | **add** | SQLite FTS5 `bm25()`, cheap |
| `hybrid_search(query, k)` | **add** | vector + BM25 + filename + recency, weighted. The one the system prompt teaches as default. |
| `fetch_url(url)` | **add** | http(s), 5 MB cap, 15s timeout, strip to text |
| `get_backlinks(path)` | **add** | one-liner over `links.rs` |
| `link_neighbors(path, depth)` | **add** | local graph slice |
| `list_headings(path)` | **add** | cheap skim |
| `list_recent_files(n)` | **add** | mtime-sorted top-N |
| `chat_history_search(query)` | **add** | chats are `.md` — just `hybrid_search` scoped to `.forge/chats` |
| `now()` | **add** | ISO 8601 + TZ |
| `read_pdf(path)` | **add** | `pdf-extract` crate |
| `read_docx(path)` | **add** | `docx` / `mammoth` crate |
| `compile_latex(content, name)` | **add (v1 differentiator)** | writes `.tex`, runs tectonic with `--no-shell-escape`, returns pdf path or forwards errors to model for self-correction |
| `insert_at_cursor(text)` | **add** | writes into active editor at caret; IPC event to frontend |
| `open_tab(path)` | **add** | surface to user; doesn't read content |
| `create_from_template(template, vars)` | **add** | ties to templates feature |
| `trash_file(path)` | **add** | soft-delete to `.forge/trash/<ts>/` |
| `restore_file(trash_id)` | **add** | reverse of trash |
| `delete_file(path)` | **deprecate** | replaced by `trash_file` |

### 4.3 Explicitly NOT offering

- `run_command` / `exec` / `bash` — no. Terminal covers this.
- Raw socket / TCP — no.
- Anything that writes outside the vault — no.
- Anything requiring sudo — no.
- Clipboard read — maybe later, requires user consent UX.

### 4.4 Parallel tool calls — pre-existing bug

`agent.rs` matches tool results to calls by **name** today. Two parallel
same-name calls (`read_file(a.md)` + `read_file(b.md)` in the same
turn) misalign. Fix by matching on **call ID** (`ToolCall.id`). This
bug compounds with 5 new providers — fix before expanding the provider
count.

### 4.5 Cooperative cancellation — pre-existing bug

`stop_chat` is noop. Voice conversation mode + multi-chat both stress
this. Wire an `Arc<AtomicBool>` cancel flag, check between tokens, before
every tool dispatch, and inside every tool's inner loop (especially
`fetch_url`, `web_search`, `compile_latex`).

---

## 5. Context management

The hard one. Each model has a different context window (8k local to 2M
Gemini); one policy fails.

### 5.1 Per-provider metadata

At connect time, each provider thread reports:

```rust
struct ProviderCapabilities {
    context_window: usize,
    max_output_tokens: usize,
    tokenizer_kind: TokenizerKind,
    caching: CachingStrategy,
}
enum TokenizerKind {
    Cl100k,         // OpenAI
    AnthropicApprox,
    Gemma,          // local gemma family
    ByteHeuristic,  // fallback, bytes/3.8
}
enum CachingStrategy {
    None,
    ImplicitPrefix { min_tokens: usize },   // OpenAI (1024 tok)
    ExplicitBlock,                          // Anthropic cache_control
    CachedContentResource,                  // Gemini
}
```

### 5.2 Budgeting

- **Cheap estimator:** `bytes / 3.8 * 1.15` (15% safety). Good for
  under-85% budget checks.
- **Exact count:** only when the estimator puts us inside the 85-100%
  band. Use provider's counting endpoint:
  - OpenAI: `tiktoken` local (bundle) or `/v1/chat/completions` with
    `max_tokens=1` as probe
  - Anthropic: `/v1/messages/count_tokens`
  - Gemini: `models.countTokens`
- **Local GGUF:** llama-cpp-2 exposes `n_tokens`, use it.

### 5.3 Compaction policy

Tiered, triggered when budgeted tokens > 85% of window:

1. **Elide tool-result payloads.** Replace `tool_result.content` with
   `[elided: <orig_bytes> bytes]`. Keep the calling `tool_use`. This
   alone recovers 80% of tokens in long runs.
2. **Summarise turn blocks.** Every block of N turns older than the
   last 6 get replaced by one system message:
   `[summary of turns 3-18]\n- ...\n- ...`
   Summary runs on the user's configured `summarise` side model slot.
3. **Hard window.** If still over: keep system prompt + last 6 turns +
   current user message + prepended summary. Drop the rest.

Policy knobs in AI settings: tier threshold (%), summary N, side-model
picker, on/off per chat.

### 5.4 Prompt caching where free

- **Anthropic:** tag system prompt + first user turn with
  `cache_control: {type: "ephemeral"}` when they total > 1024 tok. 90%
  discount on reads. Cheap.
- **OpenAI:** implicit prefix caching for prompts > 1024 tok. No code
  changes needed — just keep the prefix stable turn-over-turn.
- **Gemini:** `cachedContent` resource requires an HTTP round-trip to
  create. Only worth it for system prompts > 32k tok. Skip for v1
  unless a user has a genuinely huge system prompt.

### 5.5 Per-chat token + cost readout

Every provider returns usage counts in stream or final response. Sum
per chat, display live:

`23,847 in · 4,108 out · ~$0.11`

Pricing table in `models.rs`, updated periodically. Cost estimate is
informational — users who hate it can hide via settings.

---

## 6. Chat-as-markdown

Chats ARE markdown files in the vault. Rendered by the same CM6
pipeline as notes (post-mdeditor.md refactor). This is the unification
bet — no parallel chat renderer.

### 6.1 On-disk format

```markdown
---
forge_chat: 1
created: 2026-04-23T21:14:00+05:30
updated: 2026-04-23T21:16:04+05:30
model: claude-sonnet-4-6
provider: anthropic
system_prompt: research-assistant
tools_allowed: [hybrid_search, read_file, fetch_url]
---

## [user] 2026-04-23T21:14:05

What did I decide about atomic writes?

## [tool:hybrid_search] 2026-04-23T21:14:07

```query
atomic write
```
### result
mdeditor.md#3-save-io  0.87
INDEX.md#save-io       0.62

## [assistant] 2026-04-23T21:14:08

You specced it in [[mdeditor#3-save-io|§3 Save IO]]:

1. Write to sibling `.tmp` in same dir.
2. `sync_all()` the tmp.
3. `rename(tmp, target)` — atomic on same FS.
4. Parent-dir `sync_all()` on Unix.

## [user] 2026-04-23T21:15:10

What about Windows?
```

### 6.2 Grammar rules

Strict enough to round-trip, loose enough to survive hand edits.

- Frontmatter YAML. Required keys: `forge_chat: 1`, `created`. Others
  optional; sensible defaults on load.
- Section header: `## [role] <ISO-8601-timestamp>` on its own line.
- `role` ∈ `{user, assistant, system, tool:<name>}`. Unknown role →
  treated as `user` (don't lose content).
- Section body: anything until the next `## [...]` header or EOF.
- Tool-call sections have an optional `### result` subsection that
  holds the tool's output. Both body and result are rendered
  code-block-fenced when they're machine-generated.

### 6.3 Streaming to disk

Writing on every token is an IO fire. Policy:

- In-memory buffer per chat (current tab state).
- Flush the file on: every tool-call boundary, stream end, explicit
  user save (Ctrl-S), tab switch, window blur, beforeunload.
- Use the §3 atomic-write helper from `mdeditor.md`. A half-written
  chat file is not a tragedy, but corruption-by-truncation is.
- On crash, reopened chat shows the last flushed state. Document this.

### 6.4 Location and naming

- Default: `<vault>/.forge/chats/YYYY/MM/YYYY-MM-DD-HHMMSS-<slug>.md`.
- Slug from first user message (30 chars, kebab-case).
- Hidden from sidebar by default (`.forge` prefix filter).
- Toggle: "show chat files in sidebar" in settings.
- User can move chat files anywhere — they're just files. The
  `forge_chat: 1` frontmatter flag is what identifies them, not path.

### 6.5 UI modes

- **Dock mode:** right-side panel (current). One chat visible.
- **Pane mode:** chat occupies the main editor area, tab-based. Ctrl-E
  still toggles read/edit pose on the chat `.md` file — users can edit
  their chats by hand, same keystrokes as notes.
- **Multi-chat:** each chat is a tab. Switch = switch tab. Tab state
  tracks which tabs are chats vs notes for distinct icon + colour.

### 6.6 Concurrency

`AppState.inference: Mutex<Option<InferenceHandle>>` serialises all
inference. Breaks multi-chat. Refactor to per-chat handles:

```rust
struct AppState {
    chats: Mutex<HashMap<ChatId, InferenceHandle>>,
    // ...
}
```

Each chat gets its own inference thread on first send. Threads are
cheap; let the OS schedule.

### 6.7 Promote a chat reply to a note

Chats are transient. Notes are canonical. The promotion flow lets a
user turn a good reply into a first-class vault note without manual
copy-paste.

Two paths, one primitive (write a markdown file into the vault):

**Path A — Export (one-click, cheap)**
- Per-assistant-message hover actions: `Copy` · `Export to note` ·
  `Regenerate`.
- `Export to note`:
  1. Take the message body verbatim (strip the `## [assistant] <ts>`
     header).
  2. Slug from the body's first `#`-heading or first 40 chars of the
     first line. On collision, append `-2`, `-3`, etc — no prompt.
  3. Write to `<vault>/<slug>.md` with frontmatter:
     ```yaml
     ---
     source_chat: .forge/chats/YYYY/MM/....md
     source_message: <iso-ts of the message>
     promoted_at: <now iso-ts>
     ---
     ```
  4. Open in a new tab, **edit pose**, caret at top. User MUST see what
     landed — prevents silently-bad notes accumulating.
  5. Append a marker back in the chat:
     `> exported to [[<slug>]] · <hh:mm>` — audit trail both ways.

**Path B — Expand (agent-driven, expensive)**
- Per-message `Expand into note` button OR natural-language trigger
  ("write a detailed note about X and save it").
- Runs the agent loop with a dedicated sub-system-prompt:
  > Produce a long-form, well-structured note. Research beyond this
  > chat using `hybrid_search` and `fetch_url`. Cite vault notes as
  > `[[wikilinks]]`; cite web sources as markdown hyperlinks at the
  > point of claim. Use headings, tables where apt, math in `$$...$$`.
  > End with a `## Sources` section listing every URL fetched. Mark
  > any unverifiable claim with `[^unverified]`.
- Writes via the same `write_file` atomic helper.
- Button tooltip shows estimated cost before firing (model + expected
  token usage range). User clicks = user authorised.

**Rules both paths share:**

- Wikilinks and web-link citations in the body carry over unchanged —
  they render identically because it's the same CM6 pipeline.
- Default destination: **vault root**. The point is the note joins the
  vault; a hidden `.forge/drafts/` default would subvert the flow. See
  OQ-AI-9 to revisit.
- Always open in edit pose after write. User verifies before trusting.
- Must respect cooperative cancellation (Phase 0 pre-req) — a
  cancelled Path B leaves a clean `[abandoned at <section>]` marker
  rather than a half-file.

**Explicit non-goals for v1:**

- Selection-range export (highlight part of a message, export that
  range). Defer to v2.
- De-duplication across promotions. Let users manage their vault.
- Auto-regenerating a promoted note when the source chat is edited
  later. One-shot copy at time of promotion; future edits diverge.
- **Two-way sync between chat and promoted note.** Chats freeze at
  promotion time. Editing the promoted note does not backport to the
  chat. The `source_chat` frontmatter is a backref, not a live link.
  This is a discipline, not a limitation. Blurring the staging
  boundary turns promotion into a maintenance nightmare.

### 6.8 System prompt templates

Named templates in AI settings:

- `research-assistant` — cites with `[[wikilinks]]`, prefers
  `hybrid_search`, returns tables and math in Forge format
- `writing-partner` — helps draft, minimal citations, preserves voice
- `code-explainer` — heavy on fenced code, annotations, diagrams
- `rubber-duck` — asks clarifying questions, rarely writes code
- `summariser` — the side-model role for §5.3 compaction

Users add their own. Each chat's `system_prompt` frontmatter names one;
default per-vault set in settings.

Every template's preamble includes:

> When citing a vault note, use `[[Note Name]]` or `[[Note#Heading]]`.
> For math, use `$...$` or `$$...$$`. For tables, use GFM pipe syntax.
> These render natively in Forge.
>
> **Citation discipline (hard rule):** before emitting any
> `[[wikilink]]`, verify the target exists via `list_files` or
> `hybrid_search`. Only cite notes that resolve. If you want to refer
> to a note that does not yet exist, use the form `[[+Proposed Name]]`
> so the reader understands it is a suggestion, not a claim.
> Hallucinated citations are the fastest way to lose the user's trust.

---

## 7. Voice simplification

### 7.1 Drop Deepgram

- Delete `src-tauri/src/deepgram.rs`.
- Remove settings fields: `deepgram_api_key`, `deepgram_stt_model`,
  `deepgram_tts_voice`.
- Strip UI from `SettingsModal.tsx`.
- Migration: on settings load, if `stt_provider == "deepgram"`, rewrite
  to `"whisper"`. If `tts_provider == "deepgram"`, rewrite to
  `"edge_tts"`. One-shot toast: "Deepgram removed — switched to local
  whisper / Edge TTS."

### 7.2 STT

- **Default (bundled):** `whisper-cli` + `whisper-base` multilingual
  model (~142 MB).
- **Upgrades (on-demand download via `models.rs`):** whisper-small
  (466 MB), whisper-medium (1.5 GB).
- **Hardware:** compile whisper-cli with CUDA (Linux/Windows), Metal
  (macOS), Vulkan (Linux). Fallback to CPU silently.
- **No cloud STT.** Removed.

### 7.3 TTS decision tree

```
online available?
├─ yes → edge_tts (primary)
└─ no  → piper installed?
          ├─ yes → piper
          └─ no  → prompt: "Install piper for offline TTS?"
                   [Install] → binaries::install_piper, then piper
                   [Cancel]  → fall back to muted state with a
                              non-blocking warning
```

`gtts` remains as a last-resort fallback if Edge's public endpoint
breaks (it has moved twice in two years). Not advertised in settings.

### 7.4 Push-to-talk + conversation mode

No changes from current voice.rs scope. Just retarget the provider
choice from the four-provider table to the simpler two-provider path
(whisper + edge_tts/piper).

---

## 8. AI settings panel

Single settings category, eight tabs:

1. **Providers** — card per provider, enable/disable, key entry
   (routed to keyring), health-check, auto-model-list.
2. **Routing** — per-slot provider+model picker (chat/fast/summarise/
   embed).
3. **Context** — compaction tier threshold, summary block size,
   caching enable-per-provider, per-chat override.
4. **Tools** — master toggle list, per-tool rate limit
   (web-search/chat, fetch-url/chat, etc.).
5. **System prompts** — template library + per-vault default.
6. **Voice** — whisper model picker, TTS provider, voice picker, VAD
   sensitivity, push-to-talk key.
7. **Terminal** — env var inheritance toggles (per provider), default
   shell, font size.
8. **Chat files** — location pattern, slug pattern, frontmatter
   defaults, "hide from sidebar" toggle.

---

## 9. Verification matrix

### 9.1 Providers

| # | Scenario | Expected |
|---|---|---|
| PR1 | Paste OpenAI key, hit health-check | 200 OK, model list populated |
| PR2 | Paste bad key | 401, error toast, no list |
| PR3 | Provider goes offline mid-stream | Clean error event, chat resumable |
| PR4 | Two chats talking to different providers simultaneously | Both stream concurrently, no blocking |
| PR5 | Disable a provider in settings | New terminals no longer inherit its key |
| PR6 | Anthropic `cache_control` on a 2 k-token system prompt | Second turn's usage shows cache hit |
| PR7 | Gemini tool call with no ID | Synthesised ID matches in tool-result |
| PR8 | OpenAI tool call, `arguments` as JSON string | Parsed to object before dispatch |

### 9.2 Harness

| # | Scenario | Expected |
|---|---|---|
| HR1 | `write_file` outside vault | Rejected with "Path escapes vault" |
| HR2 | `compile_latex` with `\write18{rm -rf ~}` | Tectonic `--no-shell-escape` silently ignores |
| HR3 | `fetch_url` to a 50 MB resource | Stops at 5 MB cap |
| HR4 | `web_search` times out after 15 s | Clean error, no hung thread |
| HR5 | Stop button mid-tool-run | Tool kill + stream end within 1 s |
| HR6 | Two parallel `read_file` calls in one turn | Results matched to correct calls by ID |
| HR7 | Agent tries to call `run_command` | Not in schema; model is told "no such tool" |

### 9.3 Context

| # | Scenario | Expected |
|---|---|---|
| CX1 | Chat crosses 85% window | Tier-1 elision kicks in, chat continues |
| CX2 | Chat crosses window after elision | Tier-2 summarisation fires |
| CX3 | Chat history fits comfortably | No compaction, no perf penalty |
| CX4 | Switch model mid-chat from 200k to 8k window | Compaction re-runs to fit new budget |

### 9.4 Chats

| # | Scenario | Expected |
|---|---|---|
| CH1 | New chat, send message | `.forge/chats/...md` created, frontmatter populated |
| CH2 | Open an old chat from sidebar | Renders via same CM6 path as notes |
| CH3 | Hand-edit a chat file, reopen | Parser tolerates, renders correctly |
| CH4 | Malformed `## [???]` header | Treated as user, content preserved |
| CH5 | Click `[[cited-note]]` in assistant reply | Opens note in new tab (per mdeditor §2) |
| CH6 | Multi-chat: two chats open, both streaming | Concurrent, neither blocks |
| CH7 | Kill app mid-stream | On restart, chat file shows state up to last flush |
| CH8 | Chat file grows to 500 kB | Still renders < 150 ms |
| CH9 | Export to note — click on an assistant reply | New note written, opened in edit pose, marker appended in chat |
| CH10 | Export to note where `<slug>.md` already exists | Writes `<slug>-2.md`, no prompt interruption |
| CH11 | Expand into note — agent runs to completion | Long-form note written with `## Sources`, cost visible |
| CH12 | Expand into note — user cancels mid-run | Partial note labeled `[abandoned at <section>]`, no corruption |
| CH13 | Promoted note's `[[wikilinks]]` | Click-resolves same as the originals did in the chat |

### 9.5 Voice

| # | Scenario | Expected |
|---|---|---|
| VO1 | Fresh install, push-to-talk | Works (whisper + edge_tts, bundled) |
| VO2 | Offline conversation | Edge fails; piper-install prompt appears |
| VO3 | Upgrade from a Deepgram-configured version | Auto-migrates to whisper + edge, one toast |
| VO4 | No mic | Clean error, no hang |

### 9.6 Terminal

| # | Scenario | Expected |
|---|---|---|
| TR1 | Open terminal | cwd is vault root, API keys in env |
| TR2 | Run `claude` | Works without extra config |
| TR3 | Run `htop` (full-screen) | Renders correctly via xterm |
| TR4 | Resize window | Terminal reflows |
| TR5 | Disable env inheritance | New shell has no API keys |

---

## 10. Implementation order

Each step is a PR. Don't skip the pre-req bugs before expanding.

### Phase 0 — pre-reqs (block everything else)

1. **Parallel tool-call fix** — match by ID in `agent.rs`. Required
   before expanding providers.
2. **Cooperative cancellation** — wire `Arc<AtomicBool>` into the
   agent loop + every tool. Required before multi-chat.
3. **API-key keyring migration** — `keyring` crate integration.
   Required before exposing new provider key entries.

### Phase 1 — providers

4. **OpenAI client** — full stream + tool_calls + multimodal.
5. **Generic OpenAI-compatible provider** — base-URL config; covers
   OpenRouter / Ollama / LM Studio / llama-server / Jan.
6. **Gemini client** — different URL + shape; synthesise call IDs.
7. **Routing slots** — settings + plumbing through `commands.rs`.

### Phase 2 — harness

8. **Drop Deepgram** — delete module, strip settings, migrate.
9. **New tools batch 1** — `hybrid_search`, `bm25_search`, `fetch_url`,
   `now`, `get_backlinks`, `list_headings`, `list_recent_files`.
10. **New tools batch 2** — `read_pdf`, `read_docx`, `compile_latex`.
11. **Trash lifecycle** — `trash_file` + `restore_file`, deprecate
    `delete_file`.

### Phase 3 — chats

12. **Per-chat `InferenceHandle`** — refactor `AppState` concurrency.
13. **Chat-file format parser + writer** — round-trip safe.
14. **Save chats as `.md`** — wire into the existing Chat component.
15. **Open chat from sidebar / tab system** — unify with editor path.
16. **System-prompt template library** — settings UI + default templates.

### Phase 4 — context

17. **Per-provider capability reporting** — fill in on connect.
18. **Token estimator + exact-count fallback** — cheap + expensive paths.
19. **Compaction tiers** — elision, summarisation, hard window.
20. **Prompt caching** — Anthropic first, OpenAI zero-code, Gemini skip.

### Phase 5 — terminal

21. **xterm.js + portable-pty integration.**
22. **Env inheritance + settings toggle.**
23. **Multi-tab terminals.**

### Phase 6 — polish

24. **Cost / usage readout in chat header.**
25. **Rate-limit retry + backoff across providers.**
26. **Voice simplification polish** — remove remaining Deepgram UI refs.

---

## 11. Open questions

Flag to user before starting the relevant phase.

- **OQ-AI-1:** keyring fallback — if keyring is unavailable, do we
  refuse to store a key, or accept with a warning? Default: accept
  with warning, document clearly.
- **OQ-AI-2:** generic OpenAI-compatible provider — should Ollama get
  a dedicated card in settings (with auto-detect of `localhost:11434`
  and `/api/tags`-populated model list) or just a generic OpenAI-compat
  card? Dedicated card is better UX for 80% of users.
- **OQ-AI-3:** chat file default location — `.forge/chats/` (hidden)
  or `Chats/` (visible by default)? Hidden is tidier, visible is more
  discoverable.
- **OQ-AI-4:** `insert_at_cursor` — security risk if a web-search result
  instructs the model to insert a prompt-injection payload into the
  user's active doc. Gate behind explicit "confirm" UI before every
  insert, or trust the model?
- **OQ-AI-5:** cost display default on or off? On is more honest, off
  is less anxiety-inducing.
- **OQ-AI-6:** Anthropic extended-thinking / OpenAI reasoning-mode
  support — roll in with v1 or defer? Defer is safer (different per
  provider, different UX implications).
- **OQ-AI-7:** Copilot provider marketing — keep it in the provider
  list prominently, or hide behind a "show experimental providers"
  toggle given ToS grey area?
- **OQ-AI-8:** shipped whisper model — `base` multilingual (142 MB) or
  `base.en` (142 MB, English-only)? Multilingual is more inclusive,
  English-only is slightly more accurate for English dictation.
- **OQ-AI-9:** `Export to note` default destination. Vault root
  (visible, discoverable, joins the vault immediately) or
  `.forge/drafts/` (hidden, tidy, requires a promote step to surface)?
  Leaning vault root.
- **OQ-AI-10:** visual marker on AI-promoted notes. Different node
  colour in graph view + small badge in sidebar? Helps users see
  vault composition at a glance (human-authored vs AI-promoted) and
  prune deliberately. Alternative: no marker, promoted notes are
  first-class and indistinguishable once edited. Leaning
  marker-with-subtle-styling so users can see the balance but the
  note itself is fully first-class.
