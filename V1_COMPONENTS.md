# Forge v1 — Component Catalog

Self-contained build + test units for v1 scope. Work on one at a time, check
it off. Status: `[ ]` todo · `[~]` in progress · `[x]` done · `[-]` descoped.

## v1 scope decisions (2026-04-23)

- **Bundle whisper-cli + whisper-base model** (~150 MB) so voice input works
  on install.
- **Skip piper.** Use Edge TTS (`edge_tts.rs`) as the primary TTS provider;
  gtts as fallback.
- **Don't bundle tectonic.** Download on first LaTeX compile with progress UI.
- **Don't bundle any LLM.** User picks from a model catalog, downloads on
  demand via `models.rs`.
- **No plugin system.** Won't replicate Obsidian's ecosystem — that's a
  deliberate non-goal, not a gap.
- **Mobile is out of v1.**

Target v1 installer size: ~290 MB single-platform.

---

## 1. Editor core

| ID | Status | Component | Acceptance | Test approach |
|---|---|---|---|---|
| ED-01 | [ ] | File tree sidebar | Shows vault files, expand/collapse, active highlight, keyboard nav | Vitest + temp vault fixture |
| ED-02 | [ ] | Tabs | Open/close/reorder, persists across restart, dirty indicator per tab | Manual + settings JSON snapshot |
| ED-03 | [ ] | CodeMirror + markdown render layer | Live preview widgets, math, wikilinks, hyperlinks | Render 20 fixtures, assert widget presence |
| ED-04 | [ ] | **Save IO: debounced + atomic** | 400ms debounce, flush on blur/tab-switch/file-switch/quit, tmp→rename atomic, dirty dot | Kill mid-save, no corruption. Unit-test debounce triggers |
| ED-05 | [ ] | File ops (create/rename/delete/move) | Atomic updates to tree + tabs + backlinks + graph, vault-scoped | Rust integration test on `resolve_within_vault` |
| ED-06 | [ ] | Frontmatter parser | YAML read/write, property UI | Rust unit tests on edge cases |

## 2. Knowledge layer

| ID | Status | Component | Acceptance | Test |
|---|---|---|---|---|
| KN-01 | [ ] | Wikilink parser + renderer | `[[note]]`, `[[note\|alias]]`, `[[note#heading]]` as clickable widget | Fuzz regex on 1000 strings |
| KN-02 | [ ] | **Wikilink autocomplete** (new) | `[[` triggers popup, filters vault tree, tab to alias | Vitest on autocomplete source |
| KN-03 | [ ] | Link indexer (`links.rs`) | Incremental on save, handles renames | 100-note temp vault, rename, assert backlinks updated |
| KN-04 | [ ] | Backlinks panel | Notes linking to current + context snippet | Fixture vault |
| KN-05 | [ ] | Graph view | Force-directed, search, click-to-open, degree-sized | 10k-node perf test, <2s, 30fps pan |
| KN-06 | [ ] | Heading targets | `[[note#heading]]` scrolls on open | Integration test |
| KN-07 | [ ] | Embeds / transclusion `![[note]]` | Render embedded note inline in preview | Feature-flag if time-constrained |

## 3. Search

| ID | Status | Component | Acceptance | Test |
|---|---|---|---|---|
| SE-01 | [ ] | SQLite FTS | Incremental on save, <50ms on 10k notes | Benchmark harness |
| SE-02 | [ ] | Vector search (candle + usearch) | Semantic query works without keyword match | 30-query eval, recall@3 |
| SE-03 | [ ] | Fuzzy filename | fzf-style, typo-tolerant | Unit test scoring |
| SE-04 | [ ] | Hybrid ranker | FTS + vector + filename + heading + recency | Same eval, compare to standalone |
| SE-05 | [ ] | Search UI (`SearchModal`) | Cmd-P style, live results, keyboard nav | Playwright E2E |
| SE-06 | [ ] | Reindex lifecycle | Incremental save, full on demand, non-blocking | Save 100 files rapidly, UI responsive |

## 4. AI core

| ID | Status | Component | Acceptance | Test |
|---|---|---|---|---|
| AI-01 | [ ] | Provider abstraction | Uniform `InferenceHandle` for local/Anthropic/Copilot | Trait contract per provider |
| AI-02 | [ ] | Model management (`models.rs`) | Download with progress, checksum verify, delete, hot-switch | Integration test on small model |
| AI-03 | [ ] | **Agent loop** | Streams, dispatches tools, caps iterations, breaks on repeat failures | Replay-log regression suite |
| AI-04 | [ ] | **Cooperative cancellation** | Stop kills token stream + in-flight tool <1s | Launch web_search, stop, assert thread dies |
| AI-05 | [ ] | **Tool result matching by call ID** | Parallel identical-name calls align correctly | Integration with 2 parallel calls |
| AI-06 | [ ] | Event streaming | Token/thinking/tool events render incrementally, no dropped frames | 10k-token stream, >30fps paint |
| AI-07 | [ ] | Chat persistence | Save to disk, resumable by ID, searchable | Unit + E2E restart/resume |
| AI-08 | [ ] | **Replay log** (new) | Append every (messages, tool_calls, results) tuple to JSONL | Grep log for expected entries |

## 5. AI tool set

| ID | Status | Tool | Contract | Test |
|---|---|---|---|---|
| TL-01 | [ ] | `read_file(path)` | Vault-scoped, content or error | Path traversal fuzz, binary reject |
| TL-02 | [ ] | `write_file(path, content)` | Vault-scoped, atomic, mkdir -p | Kill mid-write → no corruption |
| TL-03 | [ ] | `edit_file(path, old, new)` | Exact match, errors on miss/ambiguous | 50-scenario corpus |
| TL-04 | [ ] | `rename_file(from, to)` | Updates wikilinks pointing to `from` | Temp vault rename, assert backlinks |
| TL-05 | [ ] | `delete_file(path)` | Soft delete to `.forge/trash/<ts>/` | Assert recoverable |
| TL-06 | [ ] | `list_files(dir?)` | Tree or flat | Trivial |
| TL-07 | [ ] | `search_vault(query)` | Hybrid, ranked hits with snippets | Reuse SE-04 eval |
| TL-08 | [ ] | `grep_vault(pattern)` | Regex, returns path:line:match | Regex edge cases |
| TL-09 | [ ] | `read_section(path, heading)` | Just that section | Fixture-based |
| TL-10 | [ ] | `web_search(query)` | Top-N with title/url/snippet, 15s timeout | Mock + live smoke |
| TL-11 | [ ] | `fetch_url(url)` | Page text stripped, 5MB cap | Mock + live |
| TL-12 | [ ] | **`compile_latex(content, name)`** (new, differentiator) | Writes .tex, compiles, returns pdf_path, forwards errors for model self-correction | 10 AI snippets, assert compile or clean error |
| TL-13 | [ ] | **`apply_template(name, vars)`** (new) | Reads template, substitutes, creates note | Unit on substitution |
| TL-14 | [ ] | **`git_commit(message?)`** (optional v1) | Stages all, commits | Temp-repo integration |

## 6. Multimodal input

| ID | Status | Component | Acceptance | Test |
|---|---|---|---|---|
| MM-01 | [ ] | Image paste → chat | Cmd-V attaches, sent as content block | E2E paste |
| MM-02 | [ ] | Image drag-drop → chat | Same contract | E2E |
| MM-03 | [ ] | File attach via button | Images only for v1 | Manual |
| MM-04 | [ ] | Anthropic vision encoding | Base64 block matches API schema | Unit on payload shape |
| MM-05 | [ ] | Copilot vision | Same contract; graceful refuse if unsupported | Capability flag |
| MM-06 | [ ] | PDF text extract for AI | Right-click PDF → "Send to AI" | `pdf-extract` crate, 5-file test |
| MM-07 | [ ] | DOCX text extract for AI | Same pattern | Unit |
| MM-08 | [-] | Screenshot region | OS screenshot → attach | Descoped v1 |

## 7. Voice

| ID | Status | Component | Acceptance | Test |
|---|---|---|---|---|
| VO-01 | [ ] | Audio capture (`cpal`) | List devices, switch, handle disconnect mid-stream | Multi-device manual |
| VO-02 | [ ] | VAD gating (Silero) | Detect speech start/end, configurable sensitivity | Audio fixture, assert boundaries |
| VO-03 | [ ] | Whisper STT (bundled base) | Returns text + confidence, <2s on 10s audio | Fixture audio |
| VO-04 | [ ] | Deepgram STT (cloud fallback) | Same interface, <1s | Mock HTTP |
| VO-05 | [ ] | **Edge TTS** (primary, replaces piper) | Stream synthesis, voice pickable in settings | Manual + unit on HTTP client |
| VO-06 | [ ] | gtts fallback | On edge_tts error, fallback fires | Force error, assert fallback |
| VO-07 | [ ] | Voice input (push-to-talk) | Hold key → record → release → transcribe → insert | E2E |
| VO-08 | [ ] | Conversation mode (continuous) | VAD-gated full-turn loop, interruptible | 20-turn soak, no leaks |

## 8. Viewers

| ID | Status | Component | Acceptance | Test |
|---|---|---|---|---|
| VI-01 | [ ] | Markdown preview | GFM + math + wikilinks + embeds | Snapshot diff, 20 fixtures |
| VI-02 | [ ] | PDF viewer | Pin pdfjs worker, kill on file switch, no leak over 100 switches | RSS measurement loop |
| VI-03 | [ ] | Image viewer | EXIF rotation, large-file stream, zoom/pan | Rotated/huge/gif fixtures |
| VI-04 | [ ] | DOCX viewer | Mammoth.js render, fidelity warning | Known-good samples |
| VI-05 | [ ] | LaTeX viewer | Save .tex + recompile, tectonic downloads on first use, PDF crash-isolated | Compile 10 good + 3 bad, app survives |

## 9. Templates

| ID | Status | Component | Acceptance | Test |
|---|---|---|---|---|
| TP-01 | [ ] | `.forge/templates/` folder | Live-scanned, picker shows names | Trivial |
| TP-02 | [ ] | Variable substitution | `{{date}}`, `{{time}}`, `{{title}}`, `{{cursor}}`, `{{selection}}` | Unit per var |
| TP-03 | [ ] | New-from-template UI | Command palette + right-click | E2E |
| TP-04 | [ ] | Daily note | Opens/creates today's note at configured path | Integration |

## 10. Git sync

| ID | Status | Component | Acceptance | Test |
|---|---|---|---|---|
| GS-01 | [ ] | Repo init | Detects or inits; `.gitignore` for `.forge/cache` | Temp dir, assert repo state |
| GS-02 | [ ] | Auth | SSH key picker, PAT, GitHub device-flow OAuth | Mocked transports |
| GS-03 | [ ] | Auto-commit (debounced) | Every N min if changes | Simulated clock |
| GS-04 | [ ] | Pull on focus | Non-blocking, non-destructive to dirty active file | Integration + conflict |
| GS-05 | [ ] | Push | On commit if ahead, throttled | Same |
| GS-06 | [ ] | **Conflict resolution UI** | List conflicts, ours/theirs/3-way choice | Scripted conflict fixture |
| GS-07 | [ ] | Status UI | Ahead/behind/dirty/syncing in bottom bar | Visual |
| GS-08 | [ ] | Safety | Refuse push if secrets; LFS or ignore for large binaries | Scanner on common patterns |

## 11. Settings + infra

| ID | Status | Component | Acceptance | Test |
|---|---|---|---|---|
| IN-01 | [ ] | Settings persistence | Atomic write, schema-versioned, migrates | Migration unit tests |
| IN-02 | [ ] | Appearance modal | Theme/font/size live-applied | Manual |
| IN-03 | [ ] | AI provider modal | Provider select, model list, API key, Copilot device flow | E2E per provider |
| IN-04 | [ ] | Voice provider modal | STT/TTS provider, device picker | Manual |
| IN-05 | [ ] | Error boundary | React + Rust panics surfaced, not silent | Inject panic |
| IN-06 | [ ] | Crash reporting (opt-in) | Sentry or own endpoint, redact paths | Flag behavior |
| IN-07 | [ ] | Backup-before-destructive | AI write/edit/delete snapshots to `.forge/backups/<ts>/` | Assert backup after tool run |
| IN-08 | [ ] | Update mechanism | Tauri updater, signed releases | Release rehearsal |

---

## Dependency build order

```
IN-01 settings  ──┐
ED-04 save IO  ──┼──>  ED-01..03 editor  ──>  KN-01..07 knowledge  ──>  SE-01..06 search
                  │
                  ├──>  AI-01..08 AI core  ──>  TL-01..14 tools  ──>  MM-01..08 multimodal
                  │
                  ├──>  VO-01..08 voice  (parallel with AI)
                  │
                  ├──>  VI-01..05 viewers  (parallel)
                  │
                  ├──>  TP-01..04 templates  (depends on editor + file ops)
                  │
                  └──>  GS-01..08 git sync  (last; depends on everything stable)
```

## Per-component 5-step test template

Apply to every component:

1. **Define contract.** Input, output, error cases. Header comment on the module.
2. **Unit tests on pure logic.** No Tauri, no fs, no network.
3. **Integration test with temp fixture.** Rust: `tempfile::tempdir()`. TS: Vitest + mock Tauri.
4. **E2E smoke.** Playwright + `tauri-driver`. One happy path.
5. **Adversarial fixture.** Unicode filenames, 50MB files, nested links, conflicting git, mic disconnected. Assert graceful failure.

Ships when all 5 pass on Linux + macOS + Windows CI.

## Recommended execution order

1. ED-04 save IO + atomic write (1 day — prevents data loss, unblocks everything)
2. AI-08 replay log (1 day — makes everything else debuggable)
3. AI-04 + AI-05 cancellation + tool-ID matching (3 days — agent reliability)
4. KN-02 + TP-01..03 + SE-05 wikilink autocomplete, templates, search UI (1 week — UX wins)
5. TL-12 AI-generates-LaTeX (3 days — demo-ready differentiator)
6. GS-01..08 git sync (2-3 weeks)
7. Viewers + voice hardening in parallel throughout
