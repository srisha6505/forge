# Forge — feature batch changelog

Started: 2026-04-23
Context: previous agent run crashed (power loss). Re-spawned 4 isolated-worktree agents
to finish/integrate the 4 features. This file logs what each agent produced.

**Read this file BEFORE editing anything in this batch** — `main` is mid-merge and
several files are scaffolded but not wired. See "Current state on main" at the bottom.

---

## Agent 1 — LaTeX split-pane editor (task #64)
Worktree branch: merged
Status: **DONE, on main** (commit `df97466`)

Wired: `src-tauri/src/latex.rs`, `commands::compile_latex`, `commands::latex_status`,
`src/components/LatexViewer.tsx`. `App.tsx` routes `.tex` files to `LatexViewer`.
Engine fallback (xelatex → pdflatex → tectonic).

---

## Agent 2 — DOCX/Word viewer (task #65)
Worktree branch: see `git worktree list` (locked)
Status: **scaffolded only on main, NOT wired**

`src/components/DocxViewer.tsx` exists but `App.tsx` does not route the `docx` branch
of `fileKind()` to it. No backend command. PdfViewer.tsx is in the same boat — present,
unrouted.

---

## Agent 3 — Markdown image embedding (task #66)
Worktree branch: see `git worktree list` (locked)
Status: **scaffolded only on main, NOT wired**

`src/components/ImageViewer.tsx` and `src/lib/cm-math.ts` present. `App.tsx` does not
route `image` branch of `fileKind()`. Inline `![](path.png)` rendering inside the
markdown editor is not implemented yet.

---

## Agent 4 — Agent panel mature UI (task #67)
Worktree branch: `worktree-agent-ac13c933` @ `c1d7119`
Status: **DONE in worktree, NOT merged to main**

Files exist only in the worktree:
- `src/components/chat/RunningIndicator.tsx`
- `src/components/chat/ToolCallCard.tsx`
- `src/components/chat/ChatToolbar.tsx`
- `src/components/chat/ChatComposer.tsx`
- rewritten `src/components/Chat.tsx`
- ~370 lines added to `src/index.css` (`.forge-chat__*`, `.forge-msg__*`, `.forge-tool-card__*`)

`tsc --noEmit` passes in the worktree. No new backend commands.

`src/components/chat/` exists on main but is empty.

---

## Merge + verify (task #68)
Status: **pending**

### Current state on main (as of 2026-04-23, commit `63b1242`)

What's broken if you try to build right now:

1. `src-tauri/src/lib.rs` does NOT declare these modules even though the files exist:
   `binaries`, `copilot`, `deepgram`, `edge_tts`, `gtts`, `links`, `models`, `vad`.
   None of their `#[tauri::command]`s are registered in `invoke_handler!`.
2. `src-tauri/Cargo.toml` is missing deps the new modules import:
   `tungstenite` (used by `edge_tts.rs`), `ort` (used by `vad.rs`).
3. `src/components/SettingsModal.tsx` references TS fields that don't exist on
   `Settings` in `src/lib/tauri.ts`: `copilot_model`, `stt_provider`, `tts_provider`,
   `deepgram_api_key`, `deepgram_stt_model`, `deepgram_tts_voice`, `edge_tts_voice`,
   `gtts_lang`. It also imports `copilotStatus`, `copilotLoginStart`, `copilotLoginPoll`,
   `copilotLogout`, `ModelInfo`, `BinaryStatus` which are not exported from `tauri.ts`.
4. `Settings` (Rust, `src-tauri/src/settings.rs`) is missing the same fields.
5. `App.tsx` does not route `pdf | image | docx` from `fileKind()` — those files fall
   through to plain `Editor` / `MarkdownPreview`.
6. `src-tauri/.cargo/config.toml` has uncommitted lib-path edits (working state, do not revert).

### Locked worktrees still present
8 locked worktrees under `.claude/worktrees/agent-*`. Don't `git worktree remove` them
until merge is done — they're the only copy of some agent output.

### Merge order (proposed)
1. Cherry-pick / copy chat panel files from `worktree-agent-ac13c933` into main.
2. Extend `Settings` (Rust + TS) with missing fields.
3. Add `tungstenite` + `ort` to `Cargo.toml`.
4. Declare modules in `lib.rs`, register commands.
5. Add `copilot*`, `models`, `binaries` typed wrappers in `lib/tauri.ts`.
6. Route `pdf | image | docx` in `App.tsx`.
7. Implement inline-image rendering for #66 (Editor + Preview lens).
8. `cargo build` (CUDA, slow), `tsc --noEmit`, `pnpm tauri dev`, smoke-test.
9. Update `INDEX.md`, mark tasks #65/66/67/68 complete, commit.
