# Forge — Brainstorm Backlog

Living dump of every genuinely interesting idea from our planning sessions, so we
stop losing them when threads drift. Each item has: status, summary, my time
estimate (wall-clock for me, NOT human dev-days), and rough ROI.

Status legend: 🆕 new idea · 🟡 decided, not built · ✅ shipped · ❌ rejected with reason

---

## 1. Performance & perceptual feel

### 1.1 Typing-lag data-flow fix 🟡 ~25 min
The single biggest "this is a webapp" tell. Diagnosed: `App.tsx:703-726` clones
the entire tabs array (with content) on every keystroke, App.tsx re-renders,
`<CodeMirror value={content}>` does an O(N) doc.toString() compare,
`extractHeadings(content)` re-runs.

3 edits: (1) `onEditorChangeImpl` only flips dirty bit on clean→dirty transition,
content lives in `pendingWrites.current` ref. (2) `flushPending` writes saved
content back into `tab.content` so reads see latest. (3) `<CodeMirror spellCheck="false">`.

Expected: 25-40 ms per keystroke → 1-3 ms.

### 1.2 Streaming token batcher (chat panel) 🟡 ~30 min
Right now every Gemma token re-renders the entire growing assistant message via
ReactMarkdown. At length N, cost per token = O(N). Buffer tokens in a ref,
schedule a `requestAnimationFrame` flush. Final result identical, perceived
smoothness 5-10x.

### 1.3 WebKitGTK env vars (Linux) ✅ shipped
Set in `lib.rs` before `tauri::Builder`: `WEBKIT_FORCE_COMPOSITING_MODE=1`,
`WEBKIT_DISABLE_DMABUF_RENDERER=0`, `WEBKIT_USE_GLES=1`,
`LIBGL_ALWAYS_SOFTWARE=0`. ~2x compositing on Linux.

### 1.4 CSS containment for chat scroll ✅ shipped
`.forge-chat__scroll { contain: layout paint; transform: translateZ(0); }` in
index.css. Mirrors existing pattern for preview view + nav folder.

### 1.5 Virtualize chat message list 🆕 ~30 min
`@tanstack/react-virtual`. Currently every message renders even when scrolled
off. Painful at 100+ messages.

### 1.6 React Compiler (auto-memoization) 🆕 ~30-60 min
Stable as of 2026. One Vite plugin entry. Catches re-render waste with no code
changes. Expect 1.2-1.5x render perf without touching components.

### 1.7 CodeMirror RangeSet diffing 🆕 ~1-2 hr
If our custom decorations (cm-wikilinks, cm-htmlwidget) rebuild full RangeSets
per change, switch to `RangeSet.compare` for deltas. Cuts decoration cost 5-10x
on big docs.

### 1.8 Lazy-load KaTeX + highlight.js 🆕 ~30 min
Heavy markdown plugins. Dynamic import only when first math block / code fence
appears in viewport.

---

## 2. Native-feel polish (kill the "this is a webapp" smell)

### 2.1 Custom title bar with native drag region 🆕 ~45 min
`decorations: false` in tauri.conf.json + render our own title bar with
`data-tauri-drag-region`. Traffic-lights on Mac, OS-style on Linux. Single
biggest "this is native" perceptual signal.

### 2.2 Bundle fonts, kill Bunny @import 🆕 ~30 min
Currently Bunny Fonts loads from CDN at boot → ~200ms FOUC. Bundle Inter,
JetBrains Mono, Newsreader as woff2, `@font-face` locally with
`font-display: block`.

### 2.3 System font fallback first 🆕 ~10 min
`-apple-system, "Segoe UI Variable", "Inter", sans-serif`. Each OS user sees
their native font. App instantly belongs on their machine.

### 2.4 Strip decorative transitions 🆕 ~30 min
Audit every `transition:` in index.css and inline styles. Keep focus rings,
modal enter (≤80ms), list reorder. Kill: hover fade-ins, button color
transitions, sidebar slide. Native UIs are mostly hard cuts.

### 2.5 Replace spinners with skeletons (or nothing) 🆕 ~40 min
"Loading..." for vault list → skeleton items. "Saving..." for file write →
nothing visible (dirty bit clears). Long ops (model load) → progress bar, not
spinner.

### 2.6 Density audit 🆕 ~1-2 hr
Inline styles use generous web-spacing. Tighten paddings ~20%, gaps to 4-6px.
Tabular-nums on number columns.

### 2.7 CSS `:active` for click feedback 🆕 ~30 min
Don't drive button "pressed" through React state. Use `.button:active { ... }`
for sub-16ms feedback. React-driven costs 3-5 frames.

### 2.8 Pre-render hidden panels 🆕 ~20 min
`visibility: hidden` + `transform: translateX(-100%)` instead of `display: none`
for sidebar/chat panel toggles. Reappear in 0 frames.

### 2.9 Optimistic UI everywhere 🆕 ~1-2 hr
New note appears in tree before file write returns. Tiny "syncing" dot, rollback
on failure. Native apps do this; webapps wait.

### 2.10 Native menu bar (Mac) 🆕 ~1 hr
`tauri::Menu::os_default()` + custom items. File/Edit/View/Window with proper
shortcuts. Massive native-feel win on Mac.

### 2.11 Native context menu via Tauri API 🆕 ~45 min
Replace custom React `ContextMenu` with `tauri::menu::Submenu`. OS-native
rendering, instant.

### 2.12 OS drag-and-drop file open 🆕 ~30 min
`tauri-plugin-fs` + `webview.on_window_event`. Drop .md from Finder/Explorer
onto Forge → opens.

### 2.13 OS notifications 🆕 ~20 min
`tauri-plugin-notification`. Long AI generation finishes while user is in
another app → real OS notification fires.

### 2.14 Boot time optimization 🆕 ~1 hr
Pre-warm `connect_inference` in background on app open. Lazy-route split.
Production build for testing perceived perf, not Vite dev mode.

---

## 3. Rendering / GPU acceleration paths

### 3.1 wgpu + wasm canvas for editor pane 🆕 6-12 months (post-v1 only)
Replace CodeMirror with custom GPU-rendered editor. Inside one `<canvas>`,
`cosmic-text` + `swash` + `wgpu` + `ropey` (Zed's stack). React still owns
sidebar, AI panel, settings.

Phases (see chat history for full breakdown):
- Foundation: 2-3 weeks
- Render loop + buffer: 3-4 weeks
- Editing core: 4-6 weeks
- Visual layer: 3-4 weeks
- Markdown highlight: 4-6 weeks
- Forge decorations: 4-6 weeks
- Widget overlay system: 4-5 weeks (high risk, scroll sync)
- IME: 3-5 weeks
- Multi-cursor: 2-3 weeks
- Code folding: 2-3 weeks
- Search: 1-2 weeks
- Accessibility: 3-5 weeks
- Polish: 4-6 weeks
- Migration: 2-3 weeks

Worth it only if "typing on huge files" becomes top user complaint post-v1.

### 3.2 GPUI rewrite ❌ rejected
Throws out iframe widgets, which are Forge's whole differentiation. Disqualified.

### 3.3 Servo / Verso (Rust webview) 🆕 watch quarterly
Eventual replacement for WebKitGTK on Linux. Alpha as of 2026. Tauri's `wry`
abstracts the webview, so swap should be config-level once Verso is production.

### 3.4 WebGL inside widget iframes 🆕 ~1 week
Heavy plots render via Canvas+WebGL inside the existing iframe. Additive,
contained. Cheap real win.

### 3.5 Bundle Chromium / Electron ❌ rejected
+100-150 MB binary, doubles RAM, abandons Tauri's reason to exist.

---

## 4. Innovation directions (the actual differentiation)

### 4.1 Bidirectional widgets 🟡 ~6-10 hr (highest novelty)
Widgets read AND write the note. Slide a parameter in a pendulum widget, the
note's `ω = 2.5 rad/s` updates. Edit the value in text, the widget animates to
match. Bidirectional binding. Genuinely novel — Notion can't, Obsidian can't.
Forge owns both markdown and widget runtime.

### 4.2 Python compute via Pyodide (JS render + Python compute) 🟡 ~2.5-4 hr
Tier-2 widgets: JS shell, Python computes. Pandas/scipy/sklearn/sympy where the
LLM is most reliable. Default JS, Python opt-in via skill anchors.

Architecture: lazy-load Pyodide on first use, shared host iframe pools the
runtime, JS owns DOM/animation/interaction.

Demo unlock: drop CSV → live dashboard in note, all local, all in markdown.

### 4.3 Voice + widget choreography 🆕 ~4-6 hr (hackathon-grade demo)
User says "explain Bayes". AI narrates while widget draws itself synchronized
to the narration. Audio + visual + interactive, all generated real-time from
one voice prompt. The kind of thing that makes a demo video go viral.

### 4.4 AI-curated curriculum / spaced repetition 🆕 ~10-15 hr
Forge tracks what user doesn't understand (re-reads, re-asks, AI gave up).
Builds personalized review schedule. Each morning offers 3 widgets to review.
Spaced repetition + generative content. Forge becomes a tutor, vault becomes
curriculum.

### 4.5 Spatial canvas / non-linear notes 🆕 ~12-20 hr
Notes as draggable cards on infinite canvas. AI auto-suggests semantic edges.
Heptabase + Obsidian + AI.

### 4.6 Cross-vault inference 🆕 ~6-10 hr
AI silently embeds entire vault. New note triggers inline panel of related
quotes from past notes, not just links. "Three weeks ago you wrote about Markov
chains; this is a continuous-time analogue."

### 4.7 Live notebook (code blocks execute) 🆕 ~8-12 hr
Code fences become live cells. Python via Pyodide, JavaScript native. Output
renders below. Markdown becomes a notebook + AI tutor + knowledge base in one
document model.

---

## 5. Local LLM inference optimization

### 5.1 KV cache quantization (Q8_0 / Q4_0) 🟡 ~30-60 min
`cache_type_k = "q8_0"`, `cache_type_v = "q8_0"` in llama-cpp-2 config. Halves
KV cache (2.5 GB → 1.25 GB at ctx=12288). Negligible quality. Frees ~1.25 GB.
Q4_0 = 4x compression, small quality loss.

### 5.2 Verify Flash Attention is on 🆕 ~15 min
`flash_attn = true`. Big speedup on prompts > 4K tokens.

### 5.3 Verify max GPU offload 🆕 ~10 min
`n_gpu_layers = -1` or max. Any layer on CPU = 10-100x latency penalty per token.

### 5.4 Speculative decoding 🆕 ~2-3 hr
Tiny draft model (Qwen 0.5B) proposes tokens, Gemma verifies in parallel.
1.5-3x throughput, no quality loss. +500 MB VRAM. llama.cpp supports via
`--model-draft`.

### 5.5 TurboQuant KV cache (3-bit) 🆕 ~3-5 hr (requires fork)
PolarQuant + QJL = 5x KV compression. Free 2 GB at current ctx, or expand to
ctx=64K. 20-30% slower at long contexts. Not in llama.cpp main; community
forks (Metal/CUDA/CPU) exist. Maintenance burden until upstream merges.

### 5.6 Drop quality tier (Q6_K weights) 🆕 ~30 min
Switch Gemma 4B from Q8_0 to Q6_K. ~6 GB → ~5 GB weights. Small quality drop.
Frees ~1.5 GB for context expansion.

### 5.7 Route widget turns to stronger API model 🟡 ~2-3 hr
Detect widget skill match, route that turn through configured Copilot/Claude
provider. Auto-fallback to local Gemma if no API key. Solves the model-capacity
ceiling for widgets without local hardware upgrade.

---

## 6. Widget reliability (Gemma 4B isn't smart enough alone)

### 6.1 Topic-fidelity prompting ✅ shipped
Pre-flight rules in `widget.md` skill: Black-Scholes → option price vs spot,
FFT → freq spectrum, etc. Prevents the "copy sine wave for everything" failure
mode.

### 6.2 Domain widget anchor library 🆕 ~3-5 hr
`.forge/widget-anchors/<topic>.md`. One canonical visualization per common
topic (black-scholes, fft, pendulum, schrödinger, diffusion, gradient-descent,
bayes, regression, monte-carlo, ode-system). Inject when topic detected.

Combined with 5.7 (route to stronger model) is the recommended combo for
widget quality.

### 6.3 `js-widget` canonical fence ✅ shipped
Renamed from `html-widget` to break collision with `</html>` typo. Parser
accepts both forms + lenient `</html>` close.

---

## 7. Security

### 7.1 Move API keys to OS keychain 🆕 ~1-2 hr
Currently keys in `~/.config/forge/settings.json` plaintext, also exposed to
JS heap (XSS = key exposure). Use `keyring` crate or `tauri-plugin-stronghold`.
Settings.json stores reference, not secret. Test endpoints take no key arg;
read from keychain.

### 7.2 Mask keys in UI after first save 🆕 ~30 min
Show "set" / "not set" only. Stop displaying full keys in AISettingsModal.

---

## 8. Architectural decisions made

### 8.1 Stay on Tauri (not GPUI) ✅ decided
GPUI throws out iframe widget runtime → kills Forge's main differentiation.
Tauri + WebKitGTK with env vars + selective wgpu pane post-v1 is the right
trajectory.

### 8.2 JS render + Python compute (when Python is added) ✅ decided
JS for animation/interaction/closed-form math. Python opt-in for
pandas/scipy/sklearn/sympy via tier-2 widgets. Pyodide lazy-loaded, shared host
iframe. Default keeps the lean fast feel; Python is dead code on disk until a
data-analytics prompt fires.

### 8.3 Per-response "Save as note" (not chat-level export) ✅ shipped
Each assistant response saves to `notes/<slug>-<date>.md`. Chat-level export
removed everywhere (toolbar, sidebar context menu, history sidebar).

### 8.4 Three-dots opens past-chats popup ✅ shipped
Lists all past chats with "open in sidebar" / "open in tab" actions.

---

## 9. Recommended sequencing for Kaggle Gemma + v1

**Now (~3 hrs of my work for the perceptual jump):**
1. Typing-lag fix (1.1)
2. Streaming token batcher (1.2)
3. Custom title bar (2.1)
4. Bundle fonts + system stack (2.2 + 2.3)
5. Strip decorative transitions (2.4)
6. Replace spinners (2.5)

**Then (~6-10 hrs for the innovation that justifies the demo):**
7. Bidirectional widgets (4.1)
8. Domain anchor library (6.2)
9. Route widget turns to stronger model (5.7)
10. Voice + widget choreography (4.3)

**Optional adders (~3-5 hrs each):**
11. Python compute (4.2) — adds the data-analytics demo
12. KV cache quant + Flash Attention verify (5.1 + 5.2) — local model headroom

**Post-v1 (months, not days):**
- wgpu canvas editor (3.1) — only if "typing on huge files" becomes top complaint
- Curriculum / spaced repetition (4.4) — long-term differentiator
- Spatial canvas (4.5) — bigger product surface
- Cross-vault inference (4.6) — needs vault embedding infra mature

---

## 10. Pitch crystallization

**Forge = a markdown editor where the AI builds you interactive lessons in
your notes. Voice in, widget out. Drag to learn.**

That sentence is the differentiator. Everything else is table stakes.
