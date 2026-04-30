# Forge V2 UI — Design & Implementation Notes

## Overview

This document captures every design decision, feature, and implementation detail for the Forge V2 prototype. It serves as the authoritative handoff reference for developers implementing these features in the real Tauri + React codebase.

---

## Design System Foundation

### Tokens (forge/tokens.css)

All tokens follow **design.md §2** exactly. Two themes:

- **Light** (`.theme-light`): warm off-white surfaces, near-black text, ochre accent `hsl(34, 72%, 40%)`
- **Dark** (`.theme-dark`): warm charcoal surfaces `hsl(35, 10%, 12%)`, warm off-white text, brightened amber accent `hsl(38, 62%, 62%)`

Key token categories:
- Surfaces: `--background-primary`, `--background-primary-alt`, `--background-secondary`, `--background-secondary-alt`
- Modifiers: `--background-modifier-hover`, `--background-modifier-active`, `--background-modifier-border`, etc.
- Text: `--text-normal`, `--text-muted`, `--text-faint`, `--text-accent`, `--text-on-accent`
- Interactive: `--interactive-accent`, `--interactive-accent-hover`, `--interactive-normal`
- Shadows: `--shadow-s`, `--shadow-m`, `--shadow-l` (floating surfaces only)
- Focus: `--focus-ring` with 2px offset
- Z-index scale: editor(0) → sidebar(10) → header(20) → dropdown(100) → popover(200) → modal(900/1000) → toast(1100) → tooltip(1200)

### Typography

| Role | Font | Sizes |
|------|------|-------|
| UI interface | Manrope | 11px (smaller), 12px (small), 13px (medium/default), 15px (large), 17px (larger) |
| Editor headings | Newsreader (serif) | h1: 34px, h2: 24px, h3: 18px |
| Code / status bar | JetBrains Mono | 13px default |

Weight discipline: 400 body, 500 UI rows, 600 titles, 700 editor bold only.

### Spacing & Radii

- Radii: 4px (small/inputs), 6px (buttons/cards), 8px (dialogs), 12px (hero, rare)
- Motion: 120ms fast, 180ms base, 260ms slow. Ease: `cubic-bezier(0.2, 0, 0, 1)`. No bounce/spring.

### Iconography

- Lucide icons throughout, stroke width 1.8
- Sizes: 12px (status bar), 14px (buttons/rows), 16px (tab bar), 18px (rail)

---

## Shell Layout (§8.1)

```
┌──────────────────────────────────────────────────────────────┐
│ [48px rail] [260px sidebar] [fill content]    [420px chat]  │
│                                                              │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│ [26px status strip]                                          │
└──────────────────────────────────────────────────────────────┘
```

### Left Rail (48px)
- Top: Files, Search, Chats, Graph — 18px Lucide icons, centered
- Active state: 2px accent left border + active background + accent icon color
- Bottom: Dictation (mic), Theme toggle (sun/moon), Settings (gear)
- All buttons have tooltips with labels

### Sidebar (260px, resizable per spec)
- Switches content based on active rail tab:
  - **Files**: TreeView with folder/file rows (28px height, 14px indent per level)
    - Dirty dot: 6px accent circle on modified files
    - Promoted glyph: "✦" in faint color for AI-promoted notes
    - Footer: vault name + chevron switcher
  - **Chats**: Chat history browser (see below)

### Content Area (fills remaining)
- 40px tab bar (Chromium-style tabs)
- Document body or Chat-as-tab view
- Tab bar right actions: Read/Edit/Width toggles, TOC toggle, Chat panel toggle

### Chat Dock (420px)
- 36px header with title, model chip, new conversation button, overflow menu
- Scrollable message list
- Composer with model selector chip, text input, send button

### Status Strip (26px)
- Left: Terminal toggle, vault name, current file
- Right: Dictation toggle, backlinks count, model name, cost, save state

---

## Features Implemented

### 1. Chat as Tab
**What**: Open any chat conversation as a full tab in the main editor area, alongside markdown files.

**How it works**:
- Click the ↗ (ArrowUpRight) icon on any assistant message action row in the chat dock
- Or click any chat in the Chat History sidebar
- Chat opens as a tab with a MessageSquare icon prefix in the tab label
- Tab ID is prefixed with `chat:` to distinguish from file tabs
- Full chat view includes: header bar (title + model chip + date), scrollable message history, composer at bottom
- Messages render at 720px max-width, same as documents

**Implementation**: `ChatTabView` component in Editor.jsx. Tab data structure: `{ type: "chat", id: string, title: string }`.

### 2. Chat History Browser
**What**: Sidebar panel to browse all past conversations.

**How it works**:
- Click "Chats" icon in the left rail
- Sidebar switches from file tree to chat history list
- Each row shows: MessageSquare icon, title, date, model name
- Click any row to open that chat as a tab
- Header has "New chat" button (MessageSquarePlus icon)
- Footer shows conversation count

**Implementation**: `ChatHistorySidebar` component in Chat.jsx. App.jsx switches sidebar content when `activeTab === "chats"`.

### 3. Read / Write Mode Toggle
**What**: Toggle between read-only and edit modes for documents.

**How it works**:
- Eye icon (read mode) and PenLine icon (edit mode) in tab bar, right side
- Small ghost buttons, flush with other tab-bar actions
- Read mode shows a "Read-only view" label below the title
- Separated from TOC/chat toggles by a 1px divider

**Implementation**: `mode` state in `DocumentView`, exposed via `window.__setEditorMode` for tab-bar buttons to call.

### 4. Reading Width Toggle
**What**: Switch between readable width (720px max) and full width.

**How it works**:
- AlignLeft icon button in the tab bar, right side
- Active state when readable width is on
- Toggles `maxWidth` on the document container between `720px` and `none`

**Implementation**: `readableWidth` state in `DocumentView`, exposed via `window.__toggleReadableWidth`.

### 5. New Conversation Button
**What**: Quick-create a new chat from the chat dock header.

**Where**: Chat dock header, right side — MessageSquarePlus icon, ghost button.

### 6. Universal Dictation
**What**: Single voice input that works anywhere with a text cursor — editor, chat composer, search, etc.

**Where it appears**:
- **Left rail**: Mic icon button with tooltip "Dictation — universal voice input"
- **Status bar**: "Dictate" button with mic icon (right side)

**Design rationale**: One universal dictation mode that auto-detects context, rather than separate AI/general modes. The system routes transcribed text to wherever the cursor is focused.

### 7. Collapsible Backlinks Panel
**What**: Shows incoming links to the current document.

**Where**: Bottom of each document, below all content.

**How it works**:
- Collapsed by default, shows chevron + Link2 icon + "Backlinks" + count
- Click to expand — shows list of linking files with context snippets
- Each backlink row: filename in accent color, context excerpt in faint text below
- Hover highlight on rows
- Empty state: "No backlinks found"

**Status bar**: Backlinks count also shown in status strip (Link2 icon + "N backlinks").

### 8. Table of Contents Panel
**What**: Collapsible document outline showing all headings.

**Where**: Left side of the document area, toggled via ListTree icon in tab bar.

**How it works**:
- 220px wide panel with "Contents" header
- Doc title shown first (bold)
- h2 headings: collapsible (chevron rotates), weight 500
- h3 headings: indented 28px, weight 400
- Hover highlight on all rows
- Collapsed h2s hide their child h3s (not yet connected — UI only)

---

## Modals

### General Settings (§8.5) — 720px wide
5 tabs: Appearance, Vault, Editor, Shortcuts, About

- **Appearance**: Theme (SegmentedControl: Light/Dark/System), interface font, editor font, base font size slider, readable width toggle, zoom slider
- **Vault**: Vault path + change button, auto-open toggle, hide dotfiles, show chat files, excluded folders input
- **Editor**: Save debounce slider, dirty indicator toggle, atomic writes toggle, wikilink new-tab toggle, default pose (SegmentedControl)
- **Shortcuts**: Table of command → keybinding (Kbd component), click to rebind
- **About**: Version, build hash, license/logs links, reset settings button

Footer: Cancel + Save buttons.

### AI Settings (§8.6) — 800px wide, 85vh height
8 tabs: Providers, Routing, Context, Tools, Prompts, Voice, Terminal, Chat files

- **Providers**: Cards for each provider (Anthropic, OpenAI, Gemini, OpenRouter, Copilot, OpenAI-compatible, Local GGUF). Each card has StatusDot (green/grey), API key input, model selector, test/save buttons.
- **Routing**: 2x2 grid of cards for Chat/Fast/Summarise/Embed slots, each with provider + model selectors
- **Context**: Compaction threshold slider, summary block size input
- **Tools**: Two-column grid of tool toggles with name (monospace) + description. Bulk actions: Enable safe-only, Enable all, Disable all.
- **Prompts/Voice/Terminal/Chat files**: Placeholder tabs

---

## Component Inventory (primitives used)

All from design.md §6:

| Component | File | Notes |
|-----------|------|-------|
| GhostBtn | Primitives.jsx | Icon-only or text-only, zero chrome button |
| SecondaryBtn | Primitives.jsx | Default action button with border |
| PrimaryBtn | Primitives.jsx | Single CTA per surface, accent fill |
| SegCtrl | Primitives.jsx | 2-5 option value picker |
| Toggle | Primitives.jsx | 32x18px switch |
| InputField | Primitives.jsx | Text/number/password input |
| Chip | Primitives.jsx | Pill tag, active variant |
| StatusDot | Primitives.jsx | 6px connection indicator |
| Divider | Primitives.jsx | 1px hr-color line |
| Kbd | Primitives.jsx | Keyboard shortcut badge |

---

## File Structure

```
forge/
├── index.html          Entry point, loads all scripts
├── tokens.css          All CSS custom properties (light + dark)
├── Icons.jsx           Lucide-style SVG icon components
├── Primitives.jsx      Shared UI primitives (Button, Toggle, Input, etc.)
├── Shell.jsx           LeftRail + FilesSidebar
├── Editor.jsx          EditorPane, DocumentView, TOCPanel, BacklinksPanel, ChatTabView
├── Chat.jsx            ChatDock + ChatHistorySidebar
├── SettingsModal.jsx   General Settings + AI Settings modals
├── App.jsx             Root component, state management, layout grid
├── tweaks-panel.jsx    Tweaks panel (theme/chat/sidebar toggles)
├── mark.svg            Forge mark
├── wordmark.svg        Forge wordmark (light)
└── wordmark-dark.svg   Forge wordmark (dark)
```

---

## What's Not Implemented (Prototype Scope)

These are specified in design.md but not built in this prototype:

- Graph view modal (§8.4)
- Command palette / Cmd+P (§8.3, §6.21)
- Terminal panel (§8.8)
- Model catalogue / download manager (§8.9)
- Actual file editing (contenteditable / CodeMirror)
- Drag-and-drop in file tree
- Keyboard navigation throughout
- Context menus (right-click)
- Toast notifications
- Error boundary fallback
- Sidebar resize handles
- Search sidebar content
- Voice recording / Whisper integration
- Real theme switching (prototype uses Tweaks panel)

---

## Design Decisions & Rationale

1. **Universal dictation over dual modes**: Rather than separate AI-prompt and general dictation, one mic button that routes to the focused input. Simpler mental model.

2. **Chat as tab, not separate pane**: Chats can be opened alongside files in the editor area. The dock remains for quick conversations; tabs are for reviewing/continuing longer threads.

3. **Read/Edit in tab bar, not toolbar**: Eliminated the extra 32px toolbar. Small icon buttons in the tab bar are sufficient and save vertical space — important for a dense editor UI.

4. **Terminal in status bar**: Follows VS Code convention. The status bar terminal toggle is discoverable and doesn't consume rail space.

5. **Backlinks at document bottom + status bar count**: The panel is contextual to the document; the count in the status bar provides at-a-glance awareness without scrolling.

6. **TOC as side panel**: Rather than a dropdown or popover, the TOC is a persistent 220px panel that can stay open while reading. Toggled from the tab bar.
