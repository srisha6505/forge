# Forge — markdown editor backbone spec

Scope: the `.md` editing surface and its save path. Not viewers for pdf /
image / docx / latex — those are separate components.

Status: design. Nothing in this file is implemented yet. Update status
per-section as work lands.

The editor is the backbone of the app. This is the surface with the most
screen area and the most keystrokes. Correctness beats feature count.
Everything in this document is a hard constraint unless explicitly marked
optional.

---

## 1. Mental model

There is **one** rendered view of a markdown file: the CodeMirror 6
instance with Forge's render extensions. It has **two poses**:

| Pose | Caret | Cursor on scroller | Selection | Focus ring |
|---|---|---|---|---|
| **Read** | hidden (`caret-color: transparent`) | `default` | enabled | none |
| **Edit** | visible | `text` | enabled | subtle |

Everything else is identical between the two poses. Same DOM, same
fonts, same widths, same line heights, same widgets. A screenshot of one
pose overlaid on the other differs only in caret and cursor glyph.

The CodeMirror document is always editable in the underlying sense
(`EditorState.readOnly = false`). The read pose hides the interaction
affordances but does not disable them — a stray keystroke in read pose
must still be absorbed, not bubble to the browser.

`MarkdownPreview.tsx` is deleted from the `.md` path. It may survive as a
dead component until the last caller is removed; do not add new callers.

### 1.1 Pose transitions

Read → Edit (automatic):

- Click on any body area outside a widget.
- Any arrow-key / alphanumeric / enter / backspace keystroke while the
  editor has focus. (Read pose absorbs these to change pose, then lets
  the next keystroke through normally.)
- `Ctrl+E`.

Edit → Read:

- `Esc`.
- `Ctrl+E`.
- Not on focus loss. Switching to the chat panel must not flip pose —
  users rely on the editor staying in edit pose across tool runs.
- Not on tab switch. Each tab remembers its own pose.

### 1.2 Default pose on open

| Open source | Default pose |
|---|---|
| File exists on disk, content loaded from fs | **Read** |
| New file created via UI / command, content `""` | **Edit** |
| File reopened from `settings.open_tabs` after restart | **Read** |
| File opened as a result of an agent tool (`write_file` etc.) | **Read** |

Detection rule for "new file": a `createdInSession: true` flag on the Tab
record, set only by the create-file code paths. Do not infer newness from
`content.length === 0` — an existing empty file must open in read pose.

---

## 2. Link behaviour

All link kinds render identically in both poses (widget on non-cursor
lines, raw on the cursor line in edit pose, always widget in read pose).

Click targets:

| Link kind | Source | Click action |
|---|---|---|
| `[[Target]]` | `cm-wikilinks.ts` | open `Target.md` in **new tab** |
| `[[Target\|alias]]` | same | same |
| `[[Target#heading]]` | same | open + scroll to heading |
| `[text](./relative.md)` | `cm-hyperlinks.ts` | open relative path in new tab |
| `[text](http://…)` or `https://` | same | open in OS browser |
| `[text](mailto:…)` | same | delegate to OS |

Rules, all mandatory:

1. Default click = **new tab**. This is inverted from Obsidian (where
   default is same-tab). Intentional per user spec.
2. `Ctrl+Click` / `Cmd+Click` = same-tab replacement.
3. If the target file is already in an open tab, focus that tab — do
   not create a duplicate.
4. Clicks on a wikilink widget are never interpreted as a pose
   transition. The link handler runs on `mousedown` with
   `preventDefault + stopPropagation`, before CM6's cursor-placement.
5. Clicks inside a rendered table cell's widget must route through the
   same link handler when the cell contains a wikilink or markdown link.
   `cm-markdown-render.ts` renders those inside the table widget;
   `tableClickHandler` already skips them — verify this path after
   refactor.
6. External URLs open via `@tauri-apps/plugin-opener`. Never embed a
   webview in-app.

New-tab routing lives in `App.tsx`'s `openByTarget`. Today it replaces
the active tab — change to `openFile(path, { newTab: true })`.

---

## 3. Save IO

### 3.1 Frontend contract

Single source of truth: a `Map<path, content>` + a debounced flush.

- Debounce: **400 ms** (today's 250 ms is too noisy; 400 stays below
  perceptible lag).
- On every editor change: update Map, (re)arm timer, mark tab dirty.
- IME composition extends the debounce: if `view.composing` is true when
  the timer fires, re-arm for another 400 ms.
- Flush triggers (fire and await):
  1. Timer expiry.
  2. `Ctrl+S`.
  3. Tab switch within Forge.
  4. Tab close.
  5. Vault pick.
  6. Window `blur`.
  7. Document `visibilitychange` → hidden.
  8. `beforeunload` (covers OS close / Alt-F4 / Tauri close).
  9. React unmount.

Failure handling:

- Save failure keeps the Map entry + keeps the tab `dirty: true`. Never
  silently drop. Surface via a persistent toast after 2nd consecutive
  failure on the same path.
- Retry policy for transient errors (`EBUSY`, `EACCES` on Windows):
  3 attempts with 50 / 200 / 1000 ms backoff.

### 3.2 Backend contract (`commands.rs::write_file`)

Current implementation is `fs::write` — not atomic. Replace with:

1. Resolve the vault-scoped target path (existing logic).
2. Compute sibling tmp path: `<target>.forge-tmp-<rand>` in the same
   directory. Same-dir is mandatory — `rename` across filesystems fails
   `EXDEV`.
3. `File::create(tmp)` → `write_all(content)` → `sync_all()`.
4. `fs::rename(tmp, target)`.
5. On Unix: open the parent dir and `sync_all()` it. On Windows: skip.
6. If any step fails, delete the tmp (best-effort) and return the
   original error string. Do not leak tmp files on error.

Additional constraints:

- Never normalise line endings. Preserve exactly what the frontend sent.
- Never auto-append a trailing newline.
- Reject writes where the content contains invalid UTF-8 (serde will
  already have rejected; belt-and-braces).

### 3.3 External change detection (v1 scope cut)

When a vault file is opened, stamp its `mtime` into the Tab record. On
save:

- If current `mtime` > stamped `mtime`, do not write. Surface a
  conflict modal: "this file changed on disk since you opened it."
  Options: overwrite / reload / show diff.
- Primary cause in today's codebase: the agent wrote the file via a
  `write_file` tool while the user had it open.

Longer-term: wire `notify` to push external-change events and refresh
the buffer proactively. Out of scope for the initial pose refactor.

---

## 4. Render correctness

Items called out specifically because the current renderer gets them
wrong or doesn't cover them.

### 4.1 Wikilinks inside code fences — **bug**

`cm-wikilinks.ts::collectVisibleWikilinks` regex-scans line text with no
syntax-tree check. Backtick-wrapped `[[note]]` inside a code span or
fenced block renders as a clickable widget.

Fix: for each match, check the CM6 syntax tree at the match position;
skip if the parent node is `InlineCode`, `FencedCode`, or `CodeBlock`.

### 4.2 Embedded links

v1 MVP (ship with the pose refactor):

- `![alt](./path.png)` — inline image via asset protocol.
- `![alt](https://…png)` — inline image, remote.

v1 later (separate PR, track in V1_COMPONENTS.md):

- `![[img.png]]` — Obsidian-style image embed.
- `![[note]]` / `![[note#section]]` — note transclusion.

Image widgets must participate in `atomicRanges` and trigger
`requestMeasure` on load, because their natural height is not known
until the image decodes. Without re-measure, click-to-pos coordinates
drift.

### 4.3 Features that must survive MarkdownPreview deletion

`MarkdownPreview` currently does these that the CM6 path does not:

- [ ] Syntax-highlight code fences (`rehype-highlight`). Bring via a
      CM6 highlight extension for fenced code language, or render via
      a widget using `highlight.js` per fence.
- [ ] GFM task-list checkboxes that are **click-to-toggle** in read
      pose. Currently CM6 renders the raw `- [ ]`. Needs a widget
      that, on click in read pose, toggles the checkbox and dispatches
      a doc change.
- [ ] Wider prose layout (`.prose-chat` equivalent). The CM6 path
      already has `readableWidth` via `.is-readable` + max-width —
      confirm parity on full rule-set.

Until this list is all green, the pose refactor is incomplete and the
"delete MarkdownPreview from .md path" step does not happen.

### 4.4 Performance budget

- Cold open of a 2 000-line note: < 150 ms to first paint on the test
  machine.
- Sustained typing on a 5 000-line note: no dropped frames at 60 Hz.
- Scroll of a 10 000-line note: 60 Hz sustained.

`cm-markdown-render.ts` iterates the full syntax tree per transaction
by design (state fields cannot see the viewport). If the 5 k-line
budget blows, layer a viewport-aware view plugin on top, keep the
state field as the authoritative source for block decorations.

---

## 5. Verification matrix

Run every row of this matrix before calling any related PR done.
"Verify twice" per user instruction: once manually, once scripted.

### 5.1 Pose

| # | Action | Expected |
|---|---|---|
| P1 | Open existing non-empty file from sidebar | Read pose, caret hidden, scroll at top |
| P2 | Open existing empty file | Read pose |
| P3 | Create new file via command | Edit pose, caret at position 0 |
| P4 | Ctrl+E from read → edit → read | Caret toggles, scroll preserved |
| P5 | Esc in edit pose | → read pose |
| P6 | Click in body of read pose | → edit pose, caret placed under cursor |
| P7 | Focus chat panel while in edit pose | Pose does not change |
| P8 | Switch tab and back | Prior tab's pose preserved |

### 5.2 Links

| # | Action | Expected |
|---|---|---|
| L1 | Click `[[Note]]` in read pose | New tab opens Note.md, original tab still active? (no — new tab is focused per spec) |
| L2 | Ctrl+Click `[[Note]]` | Same tab replaced |
| L3 | Click `[[Note]]` where Note.md is already open in another tab | Other tab gains focus, no dupe |
| L4 | Click `[[Note#heading]]` | Scrolls to heading in new tab |
| L5 | Click `[[missing]]` (no file exists) | Surface a create-or-cancel prompt |
| L6 | Click `[text](https://x.com)` | OS browser opens |
| L7 | Click `[[Note]]` inside a rendered table cell | Same as L1 |
| L8 | `` `[[Note]]` `` inside a code span | Renders as raw, NOT clickable |
| L9 | `[[Note]]` inside a fenced code block | Renders as raw, NOT clickable |

### 5.3 Save

| # | Action | Expected |
|---|---|---|
| S1 | Type a character, wait 400 ms | Disk content updated, tab no longer dirty |
| S2 | Type 10 chars in 100 ms | One fs write, not 10 |
| S3 | `kill -9` Tauri process 50 ms into a save | File on disk is either full new content or full old content, never truncated or mixed. Verify via `sha256sum` before and after |
| S4 | `chmod 000` the file, type a char, wait for flush | Error surfaced after 3 retries, tab stays dirty |
| S5 | Switch tab before debounce fires | Save flushed before switch completes |
| S6 | Alt+F4 the window mid-debounce | File saved before close |
| S7 | `Ctrl+Tab` away from Forge mid-debounce | Save flushed on blur |
| S8 | Agent `write_file` while user has same file open | Conflict prompt, not silent overwrite (see §3.3) |
| S9 | Open CRLF file, edit, save | File is still CRLF |
| S10 | Open LF file, edit, save | File is still LF |
| S11 | Type Chinese characters in an IME | Only commit-points trigger saves |

### 5.4 Render adversarial

| # | Input | Expected |
|---|---|---|
| R1 | `[[a[b]]]` | Does not crash; renders best-effort or raw |
| R2 | 5 000-line note | 60 Hz scroll, no dropped frames |
| R3 | Note with 500 wikilinks | Widgets render, cursor-line rebuild < 16 ms |
| R4 | Note where line 1 has a wikilink and line 2 has a wikilink, cursor moves line 1 → line 2 | Decoration rebuild runs once, not per-line |
| R5 | Note with an image `![](./huge.png)` 20 MB | Renders without blocking main thread |
| R6 | Malformed table | Renders as raw text, not crashed widget |

---

## 6. Implementation order (proposed)

Each step is a standalone PR. None depends on the next compiling —
atomic rollback possible.

1. **Backend atomic write.** `commands.rs::write_file` → tmp + rename +
   parent-dir fsync. Tests: S3 above.
2. **Frontend save flush triggers.** Add blur / visibilitychange /
   beforeunload / tab-switch handlers. Tests: S5-S7.
3. **Save failure surfacing.** Persistent toast, retry policy.
4. **Wikilink-in-code-fence fix.** Syntax-tree gate in
   `cm-wikilinks.ts`. Tests: L8, L9.
5. **`openByTarget` → new tab.** Single-line change in `App.tsx` +
   dedup-by-path check in `openFile`. Tests: L1, L3.
6. **Pose refactor.** Read/edit pose state per-tab, caret CSS, Ctrl-E
   toggle, click-to-edit, Esc-to-read. Tests: P1-P8.
7. **Feature parity before MarkdownPreview deletion.** Code-fence
   highlighting, task-list checkbox widget. §4.3 checklist.
8. **MarkdownPreview removal from `.md` path.** Delete the readMode
   branch in `App.tsx`, remove the component if no other caller.
9. **Inline image rendering in CM6.** `![](./path)` and
   `![](https://…)` widgets. Tests: R5.
10. **External change detection.** mtime stamp on open, conflict modal
    on save. Tests: S8.

Do **not** merge steps 7 and 8 until step 6 is stable in daily use for
at least a week. A regression here breaks the product's most-used
surface.

---

## 7. Open questions

Flag for user confirmation before starting step 5 onward.

- **OQ1:** new-tab-by-default click behaviour. Inverted from Obsidian.
  Confirmed or revisit?
- **OQ2:** on clicking `[[missing]]`, do we prompt to create, or
  silently create-and-open, or no-op?
- **OQ3:** Esc-to-read: does it also blur focus, or stay focused with
  caret hidden?
- **OQ4:** conflict-on-external-change default action — reload
  (lose local edits) vs overwrite (lose disk changes) vs show diff?
  Show-diff is the safe default but requires a diff view.
