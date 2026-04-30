# Forge — design system and page brief

Scope: visual language, tokens, component rules, per-page direction. The
authoritative reference for any UI work. When this doc conflicts with
anything else, this doc wins for visual decisions.

Status: design. v1 palette is already implemented in `src/index.css` and
the user likes it. This document formalises what exists + fills gaps.

---

## 1. Design principles

1. **Quiet surface, loud content.** Chrome recedes. Colour, shadow, and
   motion appear only where they carry meaning. The user's writing is
   the foreground; UI is background.
2. **Typography is the UI.** Hierarchy comes from weight and size, not
   coloured boxes or borders. Borders are used, but sparingly.
3. **Warm and earthy, not clinical.** Hues 32-40 (warm ambers), never
   pure blue-greys. Accent is ochre, not blue. This is the brand.
4. **Dense but not cramped.** Row heights 28-32 px, padding 8-12 px on
   rows. Modals capped at 640 px wide. Users who want spacious can zoom.
5. **No AI slop.** No gradients on buttons, no glassmorphism, no drop
   shadows on everything, no bright saturated accents, no emoji icons,
   no "vibrant" anything. If it would look at home on a 2021 bootcamp
   project, reject it.
6. **One component per purpose.** Every UI surface that shows a menu
   of actions uses the same DropdownMenu. Every file tree uses the
   same TreeView. Every primary button is the same Button with the
   same variant prop. A component library of 45 primitives lives at
   `src/components/ui/`; features consume from there and never
   re-implement. See §6.0 for the full rules.

---

## 2. Colour tokens

All tokens live in `src/index.css`. The light set is on `:root`, dark
set on `.theme-dark`. `<body>` carries `theme-light` or `theme-dark`.

### 2.1 Light theme

**Surfaces:**
```
--background-primary        hsl(40, 0%, 100%)     pure white, editor surface
--background-primary-alt    hsl(40, 25%, 97%)     subtle off-white for cards
--background-secondary      hsl(40, 22%, 96%)     sidebar, panels
--background-secondary-alt  hsl(40, 20%, 92%)     deepest chrome surface
```

**Modifiers (overlays applied on top of surfaces):**
```
--background-modifier-border         hsl(40, 15%, 89%)    divider lines
--background-modifier-border-hover   hsl(40, 15%, 80%)    hovered dividers
--background-modifier-border-focus   hsl(38, 45%, 55%)    focus ring
--background-modifier-hover          hsl(40, 25%, 94%)    row / button hover
--background-modifier-active         hsl(40, 45%, 88%)    active row / pressed
--background-modifier-active-hover   hsl(40, 45%, 82%)    active + hover
--background-modifier-form-field     hsl(0, 0%, 100%)     input background
--background-modifier-form-field-highlighted  hsl(40, 25%, 98%)
--background-modifier-box-shadow     hsla(40, 20%, 10%, 0.08)
--background-modifier-error          hsla(8, 70%, 55%, 0.1)    error tint
--background-modifier-error-hover    hsla(8, 70%, 55%, 0.18)
--background-modifier-success        hsla(92, 40%, 45%, 0.1)   success tint
--background-modifier-message        hsla(40, 20%, 10%, 0.04)  info tint
```

**Text:**
```
--text-normal          hsl(32, 10%, 12%)    body text
--text-muted           hsl(32, 7%, 38%)     secondary text, labels
--text-faint           hsl(32, 6%, 58%)     placeholder, de-emphasised
--text-bold            hsl(32, 10%, 8%)     **bold** and headings
--text-italic          inherit
--text-accent          hsl(34, 72%, 40%)    links, active state
--text-accent-hover    hsl(34, 72%, 32%)
--text-on-accent       hsl(0, 0%, 100%)     text on accent-filled bg
--text-error           hsl(8, 68%, 48%)
--text-success         hsl(92, 42%, 38%)
--text-warning         hsl(34, 72%, 45%)
```

**Headings:**
```
--text-title-h1   hsl(32, 10%, 10%)
--text-title-h2   hsl(32, 10%, 10%)
--text-title-h3   hsl(32, 10%, 12%)
--text-title-h4   hsl(32, 10%, 15%)
--text-title-h5   hsl(32, 10%, 18%)
--text-title-h6   hsl(32, 10%, 22%)
```

**Links / selection:**
```
--text-link            hsl(34, 72%, 40%)
--text-link-external   hsl(34, 72%, 40%)
--text-highlight-bg    hsla(38, 85%, 60%, 0.35)    ==highlight==
--text-selection       hsla(34, 70%, 50%, 0.22)    text selection
```

**Interactive (buttons, pills, chips):**
```
--interactive-normal        hsl(40, 22%, 94%)
--interactive-hover         hsl(40, 22%, 89%)
--interactive-accent        hsl(34, 72%, 40%)    primary button bg
--interactive-accent-hover  hsl(34, 72%, 34%)
--interactive-accent-rgb    176, 110, 24         for rgba() mixing
```

**Code:**
```
--code-normal       hsl(34, 72%, 35%)      inline `code`
--code-background   hsl(40, 22%, 96%)
--code-comment      hsl(32, 6%, 58%)
--code-string       hsl(92, 42%, 38%)      green
--code-keyword      hsl(280, 60%, 45%)     purple
--code-function     hsl(210, 68%, 42%)     blue
--code-number       hsl(34, 72%, 35%)      amber
```

**Chrome:**
```
--hr-color                 hsl(40, 15%, 89%)
--caret-color              hsl(34, 72%, 40%)
--scrollbar-bg             transparent
--scrollbar-thumb-bg       hsla(32, 8%, 40%, 0.25)
--scrollbar-active-thumb-bg hsla(32, 8%, 40%, 0.45)
--icon-color              hsl(32, 7%, 42%)
--icon-color-hover        hsl(32, 10%, 12%)
--icon-color-active       hsl(34, 72%, 40%)
```

**Shadows:**
```
--shadow-s  0 1px 2px  hsla(32, 15%, 10%, 0.04)
--shadow-m  0 2px 8px  hsla(32, 15%, 10%, 0.06),
            0 1px 2px  hsla(32, 15%, 10%, 0.04)
--shadow-l  0 8px 24px hsla(32, 15%, 10%, 0.12),
            0 2px 6px  hsla(32, 15%, 10%, 0.06)
```

### 2.2 Dark theme

**Surfaces:**
```
--background-primary        hsl(35, 10%, 12%)    warm charcoal, editor
--background-primary-alt    hsl(35, 10%, 10%)
--background-secondary      hsl(35, 11%,  8%)    sidebar
--background-secondary-alt  hsl(35, 12%,  6%)
```

**Modifiers:**
```
--background-modifier-border         hsl(35, 8%, 18%)
--background-modifier-border-hover   hsl(35, 8%, 26%)
--background-modifier-border-focus   hsl(38, 55%, 55%)
--background-modifier-hover          hsl(35, 10%, 15%)
--background-modifier-active         hsl(35, 14%, 19%)
--background-modifier-active-hover   hsl(35, 14%, 24%)
--background-modifier-form-field     hsl(35, 11%,  8%)
--background-modifier-form-field-highlighted  hsl(35, 10%, 10%)
--background-modifier-box-shadow     hsla(0, 0%, 0%, 0.45)
--background-modifier-error          hsla(12, 70%, 60%, 0.12)
--background-modifier-error-hover    hsla(12, 70%, 60%, 0.2)
--background-modifier-success        hsla(92, 35%, 55%, 0.12)
--background-modifier-message        hsla(0, 0%, 100%, 0.04)
```

**Text:**
```
--text-normal          hsl(40, 25%, 82%)    warm off-white
--text-muted           hsl(38, 18%, 62%)
--text-faint           hsl(38, 12%, 42%)
--text-bold            hsl(42, 35%, 90%)
--text-italic          inherit
--text-accent          hsl(38, 62%, 62%)    brightened amber
--text-accent-hover    hsl(38, 70%, 72%)
--text-on-accent       hsl(35, 10%, 10%)    dark text on accent fill
--text-error           hsl(12, 68%, 62%)
--text-success         hsl(92, 38%, 60%)
--text-warning         hsl(38, 62%, 62%)
```

**Headings:**
```
--text-title-h1   hsl(42, 35%, 90%)
--text-title-h2   hsl(42, 35%, 88%)
--text-title-h3   hsl(42, 30%, 84%)
--text-title-h4   hsl(42, 25%, 80%)
--text-title-h5   hsl(40, 22%, 76%)
--text-title-h6   hsl(40, 18%, 70%)
```

**Links / selection:**
```
--text-link            hsl(38, 62%, 62%)
--text-link-external   hsl(38, 62%, 62%)
--text-highlight-bg    hsla(38, 85%, 62%, 0.25)
--text-selection       hsla(38, 62%, 60%, 0.22)
```

**Interactive:**
```
--interactive-normal        hsl(35, 10%, 15%)
--interactive-hover         hsl(35, 12%, 20%)
--interactive-accent        hsl(38, 62%, 62%)
--interactive-accent-hover  hsl(38, 72%, 72%)
--interactive-accent-rgb    212, 161, 95
```

**Code:**
```
--code-normal       hsl(38, 62%, 62%)
--code-background   hsl(35, 11%, 9%)
--code-comment      hsl(38, 12%, 45%)
--code-string       hsl(92, 38%, 60%)
--code-keyword      hsl(290, 50%, 70%)
--code-function     hsl(205, 60%, 66%)
--code-number       hsl(38, 62%, 62%)
```

**Chrome:**
```
--hr-color                 hsl(35, 8%, 18%)
--caret-color              hsl(38, 62%, 62%)
--scrollbar-bg             transparent
--scrollbar-thumb-bg       hsla(40, 20%, 70%, 0.18)
--scrollbar-active-thumb-bg hsla(40, 25%, 75%, 0.32)
--icon-color              hsl(38, 18%, 62%)
--icon-color-hover        hsl(40, 30%, 85%)
--icon-color-active       hsl(38, 62%, 62%)
```

**Shadows (dark):**
```
--shadow-s  0 1px 2px   hsla(0, 0%, 0%, 0.3)
--shadow-m  0 4px 12px  hsla(0, 0%, 0%, 0.35),
            0 1px 3px   hsla(0, 0%, 0%, 0.2)
--shadow-l  0 16px 40px hsla(0, 0%, 0%, 0.45),
            0 4px 10px  hsla(0, 0%, 0%, 0.25)
```

### 2.3 Tokens to add (gaps in the current palette)

These are not in `src/index.css` yet. Add them both themes.

```
/* Info (blue, informational states) */
--text-info              light: hsl(210, 68%, 42%)    dark: hsl(205, 60%, 66%)
--background-modifier-info  light: hsla(210, 68%, 50%, 0.1)
                            dark:  hsla(205, 60%, 66%, 0.12)

/* Focus ring (keyboard focus on interactive elements) */
--focus-ring             light: hsla(34, 72%, 45%, 0.55)
                         dark:  hsla(38, 62%, 62%, 0.55)
--focus-ring-offset      2px outside the element

/* Graph node colors (distinct from accent) */
--graph-node-default     light: hsl(32, 20%, 65%)    dark: hsl(38, 18%, 55%)
--graph-node-active      same as --text-accent
--graph-node-promoted    light: hsl(34, 62%, 55%)    dark: hsl(38, 55%, 65%)
                         (for AI-promoted notes per ai.md OQ-AI-10)
--graph-edge             light: hsla(32, 8%, 50%, 0.3)
                         dark:  hsla(40, 20%, 70%, 0.2)

/* Z-index scale */
--z-editor           0
--z-sidebar          10
--z-header           20
--z-dropdown         100
--z-popover          200
--z-modal-backdrop   900
--z-modal            1000
--z-toast            1100
--z-tooltip          1200
```

---

## 3. Typography

**Font choice (Claude-adjacent, OFL, bundled):**

- UI + body sans: **Manrope**. Warm humanist geometric sans. Closest
  open equivalent to Styrene (Claude's font). Reads well at 12-13 px.
- Optional serif (editor H1 only, per OQ-D-1): **Newsreader**. Variable,
  Tiempos-adjacent rhythm, OFL via Google Fonts.
- Code: **JetBrains Mono**. Kept.

All three bundled as webfonts in the installer. No CDN. Offline-first.
Font files live at `src/assets/fonts/`.

```
--font-interface    "Manrope", ui-sans-serif, system-ui, sans-serif
--font-text         "Manrope", ui-sans-serif, system-ui, sans-serif
--font-monospace    "JetBrains Mono", ui-monospace, Menlo, monospace
--font-serif        "Newsreader", ui-serif, Georgia, serif   /* optional */
```

Font-size scale (already defined, verified good):
```
--font-ui-smaller   11px     tooltips, secondary metadata
--font-ui-small     12px     labels, icons
--font-ui-medium    13px     body UI text (default)
--font-ui-large     15px     prominent UI text, editor
--font-ui-larger    17px     section titles
```

Editor content uses `--font-text-size` (default 16 px, user-configurable
via Ctrl+=, Ctrl-0, Ctrl+−).

Heading sizes (editor, responsive clamp):
```
--font-h1-size   clamp(1.75rem, 0.8vw + 1.5rem,  2.25rem)
--font-h2-size   clamp(1.4rem,  0.5vw + 1.25rem, 1.75rem)
--font-h3-size   clamp(1.15rem, 0.3vw + 1.05rem, 1.375rem)
--font-h4-size   1.05rem
```

Line heights:
```
--line-height-tight    1.2    headings
--line-height-snug     1.35   compact UI rows
--line-height-normal   1.55   body UI text
--line-height-relaxed  1.7    editor prose
```

Weight usage (be disciplined, avoid drift):
- 400 body, 500 UI rows, 600 titles and emphasis, 700 only for editor bold.

**Font bundling.** Ship Inter + JetBrains Mono as webfonts in the
installer. No CDN, offline-first.

---

## 4. Spacing, radii, motion

Spacing (already defined):
```
--size-2-1   2px
--size-2-2   4px
--size-2-3   6px
--size-4-1   4px
--size-4-2   8px
--size-4-3   12px
--size-4-4   16px
--size-4-5   20px
--size-4-6   24px
--size-4-8   32px
```

Radii:
```
--radius-s    4px     inputs, small chips, tight pills
--radius-m    6px     buttons, cards, menu items
--radius-l    8px     dialogs, large cards
--radius-xl  12px     hero panels (rare)
```

Layout primitives (already defined):
```
--topbar-height    36px
--tab-height       40px
--statusbar-height 26px
--ribbon-width     48px     left rail
--sidebar-width    260px    default, resizable 180-480
--chat-width       420px    default, resizable 280-640
```

Motion:
```
--motion-duration-fast   120ms   hover, focus state
--motion-duration-base   180ms   mode toggles, panel reveals
--motion-duration-slow   260ms   modal enter
--motion-ease            cubic-bezier(0.2, 0.0, 0.0, 1.0)    quick-out
--motion-ease-in-out     cubic-bezier(0.4, 0.0, 0.2, 1.0)
```

No bounce, no spring, no elaborate choreography. Transitions on
`opacity`, `transform`, `background-color` only. Never on `width`/
`height` (jank).

---

## 5. Iconography

- Library: **Lucide** (already installed). Never mix icon libraries.
- Default stroke width: **1.8**. Never less than 1.5 (thin strokes look
  weak at UI sizes).
- Sizes: **16 px** in rows / buttons, **18 px** in rail, **20 px** in
  toolbars, **24 px** in empty states.
- Colour: `--icon-color`. Hover: `--icon-color-hover`. Active:
  `--icon-color-active`.
- Never decorative. Every icon must pair with a text label or tooltip
  with a verb (`Open graph`, not `Graph`).

---

## 6. Components

This section defines the vocabulary. Every component lives under these
rules. When building a new component, inherit the rules; do not
re-invent.

### 6.0 Consistency principles (read first)

Three rules, hard. A UI drifts when people ignore these.

1. **Same purpose = same component.** If two surfaces show a menu of
   actions, they use the same DropdownMenu. If two surfaces show a
   tree, they use the same TreeView. If two surfaces need a primary
   button, they use the same Button with `variant="primary"`. Never
   copy-paste a component and tweak it inline. Never re-implement
   something that already exists in `src/components/ui/`.

2. **Primitives live in `src/components/ui/` and nowhere else.** Every
   component listed in §6 has exactly one implementation in that
   directory. Features consume primitives. Features never declare new
   base components. If a new primitive is needed, propose it in §6,
   get sign-off, implement in `ui/`, then consume.

3. **Variants are props, not copies.** A button with an icon is not
   "IconButton"; it is `<Button icon={<Plus/>}>`. A compact list row
   is not "DenseListRow"; it is `<ListRow density="compact">`.
   Variants are enumerable (`variant`, `size`, `density`) and
   exhaustive. No sixth option that does not belong to the enum.

Enforcement: any PR touching UI code reviews against §6 first. PR
checklist includes "did you introduce a new primitive? justify why
none of §6 fit."

### 6.0.1 Directory layout

```
src/components/
├── ui/                  primitives — the §6 vocabulary
│   ├── Button.tsx
│   ├── Input.tsx
│   ├── Select.tsx
│   ├── DropdownMenu.tsx
│   ├── ContextMenu.tsx
│   ├── CommandPalette.tsx
│   ├── Breadcrumbs.tsx
│   ├── SegmentedControl.tsx
│   ├── SplitButton.tsx
│   ├── Radio.tsx
│   ├── Checkbox.tsx
│   ├── Toggle.tsx
│   ├── Slider.tsx
│   ├── Tabs.tsx
│   ├── Modal.tsx
│   ├── Drawer.tsx
│   ├── Popover.tsx
│   ├── Toast.tsx
│   ├── Snackbar.tsx
│   ├── Banner.tsx
│   ├── Tooltip.tsx
│   ├── Chip.tsx
│   ├── Badge.tsx
│   ├── Kbd.tsx
│   ├── Card.tsx
│   ├── ListRow.tsx
│   ├── TreeView.tsx
│   ├── Table.tsx
│   ├── Accordion.tsx
│   ├── TagInput.tsx
│   ├── SearchInput.tsx
│   ├── NumberStepper.tsx
│   ├── DatePicker.tsx
│   ├── TimePicker.tsx
│   ├── FilePickerButton.tsx
│   ├── DropZone.tsx
│   ├── Pagination.tsx
│   ├── Stepper.tsx
│   ├── Divider.tsx
│   ├── Progress.tsx
│   ├── Spinner.tsx
│   ├── Skeleton.tsx
│   ├── StatusDot.tsx
│   ├── NotificationDot.tsx
│   ├── Avatar.tsx
│   ├── Link.tsx
│   └── EmptyState.tsx
├── chat/                feature-scoped, consumes ui/
├── editor/              feature-scoped, consumes ui/
├── settings/            feature-scoped, consumes ui/
└── ...
```

Feature components compose primitives. Features never declare new
primitives.

### 6.0.2 Naming and prop conventions

- Component file name = PascalCase, matches the export.
- Size prop: `size="sm" | "md" | "lg"` where relevant. Default "md".
- Density prop: `density="compact" | "default" | "loose"`. Default
  "default".
- Variant prop: `variant="primary" | "secondary" | "ghost" | "danger"`
  where the component has multiple visual treatments.
- Icon prop: `icon={<Lucide/>}` or `leading={<Lucide/>} trailing={<Lucide/>}`.
- Disabled: `disabled={true}`. Never `isDisabled` or `dimmed`.
- Loading: `loading={true}`. Swaps content for Spinner of matching size.
- `onChange(newValue)` signature for all value-carrying components.
  Never expose the raw event unless the feature genuinely needs it.



### 6.1 Button

Three variants.

**Primary (CTA).** One per surface, only for the single primary action.
```
bg:     var(--interactive-accent)
text:   var(--text-on-accent)
hover:  var(--interactive-accent-hover)
height: 32px   padding: 0 14px   radius: var(--radius-m)
font:   --font-ui-medium, weight 500
```

**Secondary.** Default choice for most actions.
```
bg:     var(--interactive-normal)
text:   var(--text-normal)
hover:  var(--interactive-hover)
height: 32px   padding: 0 12px   radius: var(--radius-m)
border: 1px solid var(--background-modifier-border)
```

**Ghost (icon-only or text-only, zero chrome).**
```
bg:     transparent
text:   var(--text-muted)
hover:  var(--background-modifier-hover), text var(--text-normal)
height: 28-32px   padding: 0 8px   radius: var(--radius-s)
```

Disabled state: 45% opacity, `cursor: not-allowed`.
Focus: 2 px `--focus-ring` outset, offset 2 px. Never on mouse, always
on keyboard (`:focus-visible`).

### 6.2 Input

Text, number, search.
```
bg:     var(--background-modifier-form-field)
text:   var(--text-normal)
border: 1px solid var(--background-modifier-border)
focus:  border var(--background-modifier-border-focus), ring --focus-ring
height: 32px   padding: 0 10px   radius: var(--radius-s)
font:   --font-ui-medium
placeholder: var(--text-faint)
```

Labels: `--font-ui-small`, `--text-muted`, weight 500, margin-bottom 6 px.

### 6.3 Select / dropdown

Same visual as Input. Chevron icon at right (12 px, `--text-muted`).
Menu opens under, attached (no gap). Menu uses Popover (§6.8).

### 6.4 Toggle switch

```
track bg off: var(--background-modifier-border)
track bg on:  var(--interactive-accent)
knob:         var(--background-primary)
width: 32px   height: 18px   radius: 999px
motion: transform 120ms --motion-ease
```

### 6.5 Checkbox

Square, `--radius-s`, 16 px, 1 px border, check icon in
`--text-on-accent` when checked (bg `--interactive-accent`).

### 6.6 Tab (horizontal, used in settings modals)

```
height: 36px   padding: 0 14px   border-bottom: 2px transparent
text:   var(--text-muted)
active: text var(--text-normal), border-bottom var(--text-accent)
hover:  bg var(--background-modifier-hover)
```

No pill tabs, no shadow, no rounded containers.

### 6.7 Modal

```
backdrop: hsla(35, 12%, 6%, 0.6)     light
          hsla(0, 0%, 0%, 0.7)        dark
surface:  var(--background-primary)
border:   1px solid var(--background-modifier-border)
radius:   var(--radius-l)
shadow:   var(--shadow-l)
max-width:    640px (most), 800px (settings), 1000px (graph)
max-height:   85vh, content scrolls
padding:      24px top/sides, 20px bottom
```

Header: title in `--font-ui-larger` weight 600, close X button (ghost)
top-right. No footer buttons at the top; actions go at the bottom
right, primary on the right.

### 6.8 Popover / menu

```
bg:       var(--background-primary)
border:   1px solid var(--background-modifier-border)
radius:   var(--radius-m)
shadow:   var(--shadow-m)
padding:  4px
min-width: 180px
item height: 30px   item padding: 0 10px
item hover: bg var(--background-modifier-hover)
separator: 1px var(--background-modifier-border), margin 4px 0
```

### 6.9 Toast

```
bg:       var(--background-primary)
border:   1px solid var(--background-modifier-border)
radius:   var(--radius-m)
shadow:   var(--shadow-m)
padding:  12px 14px
width:    320-400px
position: bottom-right, stacked upward
motion:   enter slide-up+fade 180ms, exit fade 120ms
```

Variants: neutral, success (border-left 3 px `--text-success`), error
(border-left 3 px `--text-error`), info (border-left 3 px
`--text-info`).

### 6.10 Tooltip

```
bg:      hsl(32, 10%, 12%)     (opposite theme, always dark)
text:    hsl(40, 25%, 90%)
padding: 4px 8px
radius:  var(--radius-s)
font:    --font-ui-smaller weight 500
delay:   500ms show, 0ms hide
```

### 6.11 Chip / badge

Tag-like.
```
bg:      var(--background-modifier-message)
text:    var(--text-muted)
padding: 2px 8px
radius:  999px   (pill)
font:    --font-ui-smaller weight 500
```

Active variant: bg `--background-modifier-active`, text
`--text-accent`.

### 6.12 Card

```
bg:      var(--background-primary-alt)
border:  1px solid var(--background-modifier-border)
radius:  var(--radius-m)
padding: 16px
```

No shadow on cards. Shadow is reserved for floating surfaces.

### 6.13 List row

The workhorse.
```
height:  32px (default), 28px (dense), 40px (loose)
padding: 0 12px
hover:   bg var(--background-modifier-hover)
active:  bg var(--background-modifier-active), text var(--text-accent)
```

### 6.14 Divider

1 px solid `--hr-color`. Never box-shadowed, never gradient.

### 6.15 Progress

Linear bar:
```
height: 3px   radius: 999px
track:  var(--background-modifier-border)
fill:   var(--interactive-accent)
```

Indeterminate: CSS animation, `2s linear infinite`.

### 6.16 Spinner

Circular, used inline in loading states. Two sizes.
```
sm: 12px, stroke 1.5   md: 16px, stroke 1.8
stroke: currentColor, 25% opacity base + 100% opacity 90deg arc
motion: rotate 0.8s linear infinite
```

Used automatically by Button `loading={true}`, and standalone in row-
level loading states.

### 6.17 Skeleton

Placeholder block for async-loading content.
```
bg:      var(--background-modifier-border)
radius:  match the real element (var(--radius-s) default)
motion:  background shimmer, 1.8s linear infinite, 8% opacity delta
```

Shapes: line (configurable width + 12 px height), block (configurable
w × h), circle (configurable size). Respect prefers-reduced-motion
(static, no shimmer).

### 6.18 Breadcrumbs

Path navigation for nested contexts (settings sections, file paths in
editor header, graph-view drill-down).
```
height:   24px
gap:      6px between segments
separator: "/" in var(--text-faint), 13px weight 400
segment text:   var(--text-muted), font-ui-small, weight 500
segment hover:  var(--text-normal), underline offset 2px
last segment:   var(--text-normal), not clickable
overflow:  collapse middle segments into "…" popover that expands on click
max width: parent width; truncate last segment with ellipsis if needed
```

Example: `Settings / Appearance / Theme`

### 6.19 ContextMenu (right-click)

Same visual as Popover (§6.8) but anchored at cursor position on
`contextmenu` event. Never appears from left-click; Popover does that.

```
bg, border, radius, shadow: identical to Popover (§6.8)
item height: 28px
item keyboard shortcut (right-aligned): Kbd (§6.24), var(--text-faint)
destructive item: var(--text-error), hover bg var(--background-modifier-error)
submenu indicator: chevron-right 12px, var(--text-muted), right-aligned
```

Accessible alternative: keyboard Menu key (Shift+F10) opens at focused
element's position. Close on Esc, outside click, or selection.

### 6.20 DropdownMenu (action menu)

Same visual as Popover (§6.8) and ContextMenu (§6.19). Distinct from
Select (§6.3) by purpose: a Select picks a value, a DropdownMenu fires
an action.

Trigger: any element (usually a ghost Button with a chevron or a "⋯"
Lucide `more-horizontal`). Opens on click, anchored below (or above if
no room). Arrow-key navigation, Enter to activate, Esc to close.

```
trigger gap: 4px between trigger bottom and menu top
min-width:   max(triggerWidth, 180px)
alignment:   start by default, configurable end | center
```

Mental model: `[Open...] ▾` opens a DropdownMenu. `[Theme: Dark ▾]`
opens a Select. They look near-identical but their callbacks have
different shapes (action handlers vs value change).

### 6.21 CommandPalette

Specialised Popover for keyboard-first navigation. Ctrl+P / Cmd+P.

```
floats at top: 120px from top of viewport
width:   600px
max-height: 60vh, results scroll
surface: Popover visuals + Input at top (borderless, ring on focus)
scope pills (§6.12 chips): optional, below input
empty state: "No commands match"
item height: 36px   item padding: 0 12px
item layout: leading icon (16px) | label | trailing Kbd shortcut
```

Close on Esc, outside click, selection, or blur.

### 6.22 Banner (inline, full-width)

Static info strip, not dismissible, sits inline in the UI.

```
height:  auto (min 36px)
padding: 10px 14px
radius:  var(--radius-m)
bg:      per variant
text:    var(--text-normal)
leading icon: 16px in the semantic colour

variants:
  info:    bg var(--background-modifier-info),    border-left 3px var(--text-info)
  success: bg var(--background-modifier-success), border-left 3px var(--text-success)
  warning: bg hsla(34, 72%, 55%, 0.12),           border-left 3px var(--text-warning)
  error:   bg var(--background-modifier-error),   border-left 3px var(--text-error)
```

Use for persistent conditions: "Unsaved changes", "Save failed —
retrying", "Working offline."

### 6.23 Snackbar

Dismissable variant of Toast (§6.9). Same visuals, plus a close X
button and optional action link. Used for transient feedback that
the user might want to undo.

```
right-aligned action: Link (§6.37) style in var(--text-accent)
close X: ghost button, 20px hit target
duration: 6000ms default, pause on hover, dismiss on action click
```

Example: "Exported to [[Slug]]  [Undo]  [×]"

### 6.24 Kbd (keyboard badge)

Visual for keyboard shortcuts in menus, tooltips, hints.
```
bg:      var(--background-modifier-border)
text:    var(--text-muted)
font:    --font-ui-smaller, weight 500
padding: 1px 5px
radius:  var(--radius-s)
border:  1px solid var(--background-modifier-border-hover), bottom 2px
min-width: 16px (for single-char keys)
```

Multiple keys: space-separated in one Kbd element (`⌘ K`) or
sequenced inline with a dim "then" label (`g then t`). Prefer the
compact grouping unless the shortcut is a sequence.

### 6.25 SegmentedControl

Value-picker alternative to Tabs or Select when there are 2-5 options
and horizontal space is available.

```
height:  28px
bg:      var(--background-modifier-hover)
padding: 2px (container)
radius:  var(--radius-m)
each segment:
  text:  var(--text-muted)
  hover: text var(--text-normal)
  active: bg var(--background-primary), text var(--text-normal),
          shadow var(--shadow-s)
  radius: var(--radius-s)
  padding: 0 12px
motion: 120ms ease on active shift
```

Use for: theme picker (Light/Dark/System), density (Compact/Normal/
Loose), view switch (Edit/Read if we ever surface it).

### 6.26 SplitButton

Primary button with a secondary chevron menu for related actions.
```
main: Button (§6.1) variant as appropriate
divider: 1px var(--background-modifier-border), full button height
chevron section: 32px wide, chevron-down 12px
chevron section opens DropdownMenu (§6.20)
```

Example: [Save ▾] where ▾ opens [Save · Save As… · Save All].

### 6.27 Radio group

Single-select from a small set. Prefer SegmentedControl for 2-5, Radio
for 3-8 if each option needs a description.

```
radio: 16px circle, 1px border, dot 8px when selected in --text-accent
label: --font-ui-medium, weight 500
description (optional): --font-ui-small, --text-muted, margin-top 2px
row padding: 8px 0
gap between options: 8px
```

### 6.28 Slider

Continuous or stepped range input.
```
track:    4px height, --radius, --background-modifier-border
fill:     --interactive-accent from 0 to value
thumb:    14px circle, --background-primary, 2px --interactive-accent border
          shadow --shadow-s on hover, scale 1.1 on active
keyboard: left/right 1 step, shift+arrow 10 steps
value badge on drag: --font-ui-smaller, --text-muted, above thumb
```

### 6.29 NumberStepper

Number input with − / + side buttons. Compact, for small adjustments.
```
container height: 28px
- / + buttons: 28px square, ghost Button, icon 12px
input: Input (§6.2) with text-align right, no spinner arrows
```

Use for: GPU layers, context size, token limits, font size. Anywhere
the user nudges a small integer.

### 6.30 SearchInput

Input variant with a leading search icon and an optional trailing
clear (X) button. Visually identical to Input (§6.2) otherwise.

### 6.31 TagInput

Multi-value input for free-form tags or selections (e.g. excluded
folders, route-overrides).
```
container: Input (§6.2) visuals, height auto min 32px
tag: Chip (§6.11), size small
  remove X: 10px, --text-faint, hover --text-error
input: last-child, no border, grows to fill
keyboard: Enter adds tag, Backspace on empty removes last, commas split
```

### 6.32 Accordion

Collapsible content sections. Used for: settings subsections, error
details, advanced options.
```
header: 36px, flex, chevron-right (collapsed) / chevron-down (open)
       var(--text-muted), 12px
header hover: bg var(--background-modifier-hover)
header click: toggles, 120ms ease on chevron rotate
body: padding 8px 14px, borders between siblings --hr-color
```

### 6.33 DatePicker / TimePicker

Click trigger (Input with calendar/clock leading icon) opens a Popover
with a calendar grid / time scroller. Lucide icons. No third-party
date picker library with its own styling — build from primitives.

### 6.34 Avatar

Circle, used for Copilot user, chat participants, future collab.
```
sizes: sm 20px, md 28px, lg 40px
radius: 999px
source: image URL, or initials (1-2 chars) on a hsl fallback computed
        from a hash of the name (not random, not rainbow)
fallback bg saturation: 35%, lightness: 62% light / 42% dark
```

### 6.35 StatusDot

Connection / readiness indicator. Always pairs with a text label.
```
size: 6px (rows), 8px (headers)
radius: 999px
variants:
  connected:    hsl(92, 42%, 45%)
  not-configured: var(--text-faint)
  error:        var(--text-error)
  busy:         var(--interactive-accent), pulse 1.6s ease
```

### 6.36 NotificationDot

Unread / attention indicator. Absolute-positioned on a parent
(icon, row, tab).
```
size: 6px, --radius 999px
bg:   --text-accent
position: top-right of the parent, 2px offset
```

### 6.37 Link

Styled anchor. Not a button.
```
text:        var(--text-link)
hover:       text var(--text-accent-hover), underline 1px offset 2px
visited:     same as normal (we don't differentiate)
external links: trailing external icon, 10px, var(--text-muted)
```

### 6.38 FilePickerButton

Button that opens the native file/folder dialog. Looks like a
secondary Button with a leading `folder-open` or `file-plus` icon.
Preview of the selected path to the right in `--text-muted`, or
ghost-text "No folder chosen" if empty.

### 6.39 DropZone

Drag-drop target for files.
```
default state: dashed border 2px var(--background-modifier-border),
               bg var(--background-primary-alt),
               text var(--text-muted)
hover-drag:    border var(--text-accent),
               bg var(--background-modifier-hover),
               text var(--text-normal)
min-height: 80px
radius:     var(--radius-m)
```

### 6.40 Pagination

Row of page buttons. Used in model catalogue, chat history if long.
```
button: 28px square, ghost Button, --font-ui-small
active: bg var(--background-modifier-active), text var(--text-accent)
prev/next: chevron icons
overflow: "… 5" compact form
```

### 6.41 TreeView

Recursive tree rendering. Used for file tree in sidebar, JSON viewer,
outline panel.
```
row:        ListRow (§6.13), 28px
chevron:    12px, var(--text-muted), left of label, 14px per level indent
leading icon: optional per node (folder / file-type)
drag targets: dashed outline on targets during a drag
selection:  single-select by default, multi-select with Ctrl/Shift
keyboard: arrow up/down navigate, left collapse, right expand
```

File tree in sidebar uses this component, nothing else.

### 6.42 Table

Data grid for rows of structured data. Used in Shortcuts settings,
Model catalogue, Chat history list.
```
header row: 32px, --background-secondary, --font-ui-small weight 600
body row:   ListRow (§6.13)
sort:       click header column, chevron-up/down 10px next to label
resize:     4px column-divider hover target, cursor col-resize
```

No borders between rows. Divider below header only.

### 6.43 Drawer

Side-sliding panel. Used for secondary settings surfaces, file
metadata pane, info drawers.
```
edge:   right (default) | left | bottom
width:  400-600 depending on content (right/left); height 40vh (bottom)
bg:     var(--background-primary)
border-left / right: 1px var(--background-modifier-border)
shadow: var(--shadow-l) on the inner edge
motion: 220ms translate, var(--motion-ease)
backdrop: none (drawers overlay, do not dim)
close:  X button top-right, Esc key, or outside click (configurable)
```

### 6.44 Stepper

Multi-step form progress indicator (onboarding, model install wizard,
first-launch setup).
```
step circle: 20px, border 1px, numeric label
active: bg --interactive-accent, text --text-on-accent, no border
completed: check icon, same fill
inactive: border --background-modifier-border, text --text-muted
connector: 2px line between steps, --text-accent on completed,
           --background-modifier-border on pending
```

### 6.45 EmptyState

Centred composition for empty surfaces.
```
max-width: 380px
icon:      Lucide 40px, --text-faint, stroke 1.4
title:     --font-ui-larger weight 600 --text-muted, margin-top 16px
body:      --font-ui-medium --text-faint, margin-top 6px, line-height 1.55
action:    optional primary Button, margin-top 16px
```

---

## 7. Layout rules

- **The editor always fills.** Never constrain editor width to a fixed
  max unless the user enables Readable Width (then max 820 px).
- **Sidebars are resizable.** Handles are 3 px wide, hover target
  extends to 8 px via pseudo-element, cursor `col-resize`.
- **Panel visibility is persistent.** Collapse state saved to settings.
- **Modals dim the rest of the UI.** Backdrop always, no half-modals.
- **No floating action buttons.** Claude-style aesthetic rejects them.
- **No hero headers.** Each page's content starts immediately.

---

## 8. Page direction

Every page is designed from this skeleton:

```
┌──────────────────────────────────────────────────────────────┐
│ [leftrail] [sidebar?] [content]              [chat-dock?]   │
│                                                              │
│                                                              │
│                                                              │
│                                                              │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│ [status strip]                                               │
└──────────────────────────────────────────────────────────────┘
```

Each page below is a concrete layout specification.

### 8.1 Shell

**Left rail** (48 px, `--background-secondary-alt`).
- Top section: Files, Search, Chat-list, Graph, Terminal, Settings
  icons. 18 px Lucide, centred. Tooltip on right showing label + binding
  (`Files ⌘B`).
- Bottom section: Voice toggle, theme toggle, user avatar (for
  logged-in Copilot).
- Active tab: 2 px accent border-left, `--background-modifier-active`
  background, `--icon-color-active`.

**Sidebar** (resizable 180-480, default 260, `--background-secondary`).
- 36 px header row with the sidebar-tab name + action buttons (new
  file, collapse all).
- Content area scrolls. No internal padding; the rows own their padding.
- Bottom: 26 px footer with vault name + switcher chevron.

**Content** (`--background-primary`).
- 40 px tab strip (Chromium-style tabs, not square Obsidian tabs).
- Body fills.

**Chat dock** (resizable 280-640, default 420, `--background-primary`).
- 36 px header: current chat title + model pill + overflow menu.
- Message list scrolls.
- Composer stuck to bottom.

**Status strip** (26 px, `--background-secondary-alt`).
- Left: vault name · current file · word count
- Right: active model · cost readout · voice state · save state

### 8.2 Files sidebar

```
┌─────────────────────────┐
│ Files      [+] [↓][···]│  header (36px)
├─────────────────────────┤
│ ▾ my-vault              │  folder row (28px)
│   ▾ projects            │
│     notes.md            │  file row (28px)
│     ideas.md            │
│   ▸ archive             │
│   atomic-writes.md ●    │  dirty dot at right
├─────────────────────────┤
│ my-vault            ▾   │  footer (26px)
└─────────────────────────┘
```

- Folder/file rows: 28 px, indent 14 px per level.
- Dirty dot: 6 px circle, `--text-accent`, right-edge with 10 px margin.
- AI-promoted note indicator: small glyph (2x2 dots or a tiny "✦") in
  `--text-faint`, left of filename. Optional per OQ-AI-10.
- Drag-drop target: dashed outline in `--text-accent` on the target
  folder row.

### 8.3 Search sidebar / Cmd-P modal

Two surfaces, same component.

**Sidebar mode:**
- Header has a single input (full-width, no border, 36 px), prompt
  "Search vault…".
- Below: scope pills (Files / Content / Headings / Tags / Chats).
- Results list: 52 px rows — filename on top line, snippet below
  (2 lines max, `--text-muted`), match highlights in
  `--text-highlight-bg`.

**Cmd-P (modal, 600 px wide, centred, 120 px from top):**
- Same layout, floats in a modal surface. No backdrop dimming (it's
  a palette, not a dialog). Close on Esc or click outside.

### 8.4 Graph view (modal, 1000 px)

```
┌─ Graph view ──────────────────────────── × ─┐
│ [search ────────] [filter: tags ▾] [local]  │
│                                              │
│                                              │
│                ·      ·                      │
│              · ★──·──·                       │    canvas fills
│              │    │                          │
│              ·    ·                          │
│                                              │
│  ─────────────────────────────────────────  │
│  138 notes · 412 links · 24 promoted (✦)    │
└──────────────────────────────────────────────┘
```

- Canvas takes 90% of modal height.
- Node sizes: proportional to degree, clamp 4-14 px.
- Colours: default `--graph-node-default`, active node
  `--graph-node-active`, AI-promoted `--graph-node-promoted`.
- Edges: 1 px `--graph-edge`.
- Interactions: click node to open (new tab), drag to pan, wheel to
  zoom, double-click to center.
- "Local" toggle: only show neighbors of active file.

### 8.5 Settings (general)

```
┌─ Settings ────────────────────────────── × ──┐
│ [Appearance] Vault  Editor  Shortcuts  About │
│ ─────────────                                │
│                                              │
│   Theme                                      │
│   ○ Light   ● Dark   ○ System                │
│                                              │
│   Interface font                             │
│   [Inter ▾]          [preview: sample]       │
│                                              │
│   Editor font                                │
│   [Inter ▾]                                  │
│                                              │
│   Base font size                             │
│   [────|──────] 15 px                        │
│                                              │
│   Readable line width                        │
│   [◉  toggle off]                            │
│                                              │
│   Zoom level (editor)                        │
│   [── 100% ──]                               │
│                                              │
│                            [Cancel] [Save]   │
└──────────────────────────────────────────────┘
```

Tabs left-to-right: Appearance, Vault, Editor, Shortcuts, About.

**Appearance tab.** Theme (segmented control), interface font, editor
font, base size, readable width toggle, zoom default, accent hue picker
(advanced, defaults off).

**Vault tab.** Current vault path + switcher, auto-open on launch,
excluded folders (tag-input list), hide dotfiles, show chat files in
sidebar.

**Editor tab.** Save debounce ms, show dirty indicator, default pose
(read/edit for existing), new-file default pose, atomic write toggle
(advanced, default on), wikilink new-tab vs same-tab default, trailing
newline behaviour.

**Shortcuts tab.** Table of command → binding, click a row to rebind
(modal prompt with key capture).

**About tab.** Version, build hash, license links, "open logs", "reset
settings" (confirms).

Width: 720 px. Height auto, max 80vh.

### 8.6 Settings (AI) — separate modal

```
┌─ AI Settings ────────────────────────────── × ──────────────┐
│ [Providers] Routing  Context  Tools  Prompts  Voice  Term.  │
│                                                              │
│   ┌─ Anthropic ────────────── [●connected]──────────────┐   │
│   │ API key  [····················] [test] [save]       │   │
│   │ Default model  [claude-sonnet-4-6 ▾]                │   │
│   │ Caching       [◉ ephemeral on first turn]           │   │
│   └──────────────────────────────────────────────────────┘   │
│                                                              │
│   ┌─ OpenAI ────────────────── [○not configured]──────────┐  │
│   │ API key  [                    ] [test]                │  │
│   │ Base URL [https://api.openai.com/v1]                  │  │
│   └───────────────────────────────────────────────────────┘  │
│                                                              │
│   ┌─ Gemini ────────────────── [○not configured]──────────┐  │
│   │ ...                                                    │  │
│   └────────────────────────────────────────────────────────┘ │
│                                                              │
│   ┌─ OpenRouter ──── ┐  ┌─ Copilot ──── [●logged in] ─────┐ │
│   │ ...              │  │ ...                             │ │
│   └──────────────────┘  └─────────────────────────────────┘ │
│                                                              │
│   ┌─ OpenAI-compatible (Ollama, LM Studio, etc.) ─────────┐  │
│   │ Base URL   [http://localhost:11434/v1]                │  │
│   │ Model list auto-populated from /api/tags or /v1/models│  │
│   └────────────────────────────────────────────────────────┘ │
│                                                              │
│   ┌─ Local GGUF (in-process) ────────────────────────────┐   │
│   │ Model   [Qwen 2.5 7B Q4 ▾]  [manage catalogue…]     │   │
│   │ GPU layers [99]  Context [8192]                      │   │
│   └──────────────────────────────────────────────────────┘   │
│                                                                │
└────────────────────────────────────────────────────────────────┘
```

Tabs: **Providers · Routing · Context · Tools · Prompts · Voice · Terminal · Chat files** (8, per ai.md §8).

Each provider is a card (§6.12). Connection dot: 6 px, green for
connected, grey for not-configured, red for error. Hover shows last
check timestamp.

**Routing tab.** Four cards, one per slot (Chat / Fast / Summarise /
Embed). Each card: provider select + model select (populated from
provider catalogue), small "test" button.

**Context tab.** Sliders for compaction threshold (%), summary block
size (N turns), toggle for prompt caching per provider, side-model for
summarisation.

**Tools tab.** Two-column list of all tools (§4.2 in ai.md). Each row:
name, one-line description, toggle, rate-limit input (per chat). Bulk
actions: "Enable safe-only", "Enable all", "Disable all".

**Prompts tab.** Template library. Left column: template names. Right:
template body editor (code-editor with markdown syntax). Actions: New,
Duplicate, Delete, Set as vault default.

**Voice tab.** Whisper model picker (download manager inline), TTS
provider (Edge / piper / gtts), voice picker (populated per provider),
VAD sensitivity slider, push-to-talk key capture.

**Terminal tab.** Env inheritance toggles (per provider, per env var),
default shell picker, font size.

**Chat files tab.** Storage location (path input + browse), slug
pattern, frontmatter default keys.

Width: 800 px. Height: 85vh fixed (lots of content).

### 8.7 Chat dock

```
┌─ Atomic writes discussion ─── [claude-sonnet-4-6] · [···]─┐
│                                                            │
│   you                                                      │
│   What did I decide about atomic writes?                   │
│                                                            │
│   ─────────                                                │
│                                                            │
│   🔍 hybrid_search "atomic write"                          │
│   ▸ 3 results                                              │
│                                                            │
│   claude                                                   │
│   You specced it in [[mdeditor#3-save-io|§3 Save IO]]:    │
│                                                            │
│   1. Write to sibling `.tmp`.                              │
│   ... [Copy] [Export to note] [Regenerate]                 │
│                                                            │
├────────────────────────────────────────────────────────────┤
│ [model▾] Ask anything...                           [send] │
└────────────────────────────────────────────────────────────┘
```

- Message spacing: 16 px between turns, 8 px within.
- User label: `--font-ui-small`, `--text-muted`, weight 500.
- Assistant content rendered by the unified CM6 pipeline (read pose).
- Tool calls rendered as a compact expandable row (collapsed by
  default, shows name + one-line summary). Expanded shows full args +
  result.
- Hover on assistant message reveals action row: Copy, Export to note,
  Regenerate. 24 px row, ghost buttons.
- Composer: multi-line textarea, auto-grow to max 6 lines. Model pill
  on the left is a popover. Send button on right, enabled only when
  input non-empty and not busy.

### 8.8 Terminal panel

Bottom panel, resizable 160-600 px tall, default 280.

- Header: 28 px, session tabs on left (each with X to close, + to new),
  overflow menu right.
- xterm canvas fills.
- Background: `--background-secondary-alt`. Foreground: `--text-normal`.
  ANSI palette: standard 16-colour, slightly desaturated to match warm
  palette.
- Font: `--font-monospace`, 13 px default.

### 8.9 Model catalogue / download manager

Sub-dialog from AI Settings → Providers → "manage catalogue".

```
┌─ Model catalogue ──────────── × ──┐
│ [All] Chat · Embed · Whisper · TTS │
│                                    │
│ ● Qwen 2.5 7B Q4      4.8 GB  [✓]  │
│ ○ Llama 3.1 8B Q4     4.5 GB  [↓]  │
│ ○ Phi-3.5 Mini Q4     2.3 GB  [↓]  │
│ ○ Gemma 2 9B Q4       5.1 GB  [↓]  │
│ ● Whisper base        142 MB  [✓]  │
│ ○ Whisper small       466 MB  [↓]  │
│ ○ Piper en-US-amy      63 MB  [↓]  │
│                                    │
│ [↓] while downloading shows bar    │
└────────────────────────────────────┘
```

Row height 36 px. Progress bar replaces download button during fetch.
Delete button appears on installed rows (ghost, `--text-error` on
hover).

### 8.10 Promote-to-note flow

No new UI per se. Hover action on assistant message → click "Export to
note" → toast appears bottom-right: "Exported to [[slug]]" with
"Undo" link. New tab opens with the promoted note in edit pose.

For "Expand into note": button below assistant message → confirmation
popover showing estimated cost → click confirm → progress toast
→ new tab opens when done.

### 8.11 Empty states

**No vault:**
Centered, 400 px wide. Forge wordmark (text), 2-line description,
primary button "Open vault". Secondary "Create new vault".

**Vault open, no file selected:**
Centered, muted. Lucide `file-text` icon 40 px,
`--text-faint`. "No file open" title, "Pick a note from the sidebar"
subtitle. That's it. No illustration, no mascot.

### 8.12 Error boundary fallback

Full-pane card, 480 px max. Red left border (3 px `--text-error`).
Title "Something went wrong." One-paragraph message. Two buttons:
"Reload" (primary), "Open logs" (secondary). Expandable "Technical
details" accordion with the stack trace in monospace.

---

## 9. Accessibility

- **Contrast minimums.** Body text against background: ≥ 7.0. UI text
  (labels, buttons): ≥ 4.5. Disabled text: ≥ 3.0. Verify per theme.
  Tooling: Contrast plugin in dev, axe-core on CI.
- **Keyboard focus.** `:focus-visible` ring on every interactive
  element. Never `outline: none` without a replacement.
- **Focus order.** Logical reading order. `tabindex="0"` only where
  needed. Modal traps focus while open.
- **Escape closes modals and popovers.** Always.
- **Screen reader labels.** `aria-label` on every icon-only button.
  Sidebar items announce file name + "modified" state.
- **Reduced motion.** Respect `prefers-reduced-motion: reduce`. Disable
  all non-essential transitions.
- **Colour is never the only signal.** Status dots have text label on
  hover. Error rows have both colour + icon.

---

## 10. What to avoid (the no-list)

- No gradients on chrome, ever. Gradients are reserved for avatar
  placeholders and the graph background.
- No glassmorphism (backdrop-blur + translucent). Cheap 2020 aesthetic.
- No drop shadow on non-floating surfaces. No `box-shadow` on cards,
  buttons, or rows.
- No border-radius > 12 px. Pills (999 px) are the exception.
- No bright saturated colours. Saturation ceiling: 72% on accents, 68%
  on semantic states.
- No emoji as UI (🚀, ✨, 💡). Use Lucide icons.
- No "AI sparkle" icons on AI features. The feature is the identity,
  not a sparkle.
- No soft pastels. No neon.
- No rounded-full icon containers (iOS app-icon aesthetic).
- No hero sections on any settings page.
- No floating action button.
- No pointless animations on mount (fade-in-from-bottom on every row).
- No dark-mode-only bright accents that vibrate against the dark
  background. Dark accents stay desaturated.

---

## 11. Design brief (the prompt)

Use this when briefing a designer or AI agent to produce or revise any
Forge page. It condenses §1-10 into a working brief.

```
You are designing a page for Forge, a Tauri + React markdown editor
with a native AI agent. Follow these rules without deviation.

VISUAL LANGUAGE
- Quiet surface, loud content. Chrome recedes; the user's writing is
  foreground.
- Typography is the UI. Hierarchy via weight/size, not coloured boxes.
- Warm and earthy. Hues 32-40 (amber/ochre/cream). Never cold blue-grey.
- Dense but not cramped. Row 28-32 px, padding 8-12, modals 640 max.
- Two themes: light (warm off-white background, near-black text) and
  dark (warm charcoal background, warm off-white text). Same ochre
  accent in both, brightened in dark.

TOKENS (from design.md §2)
- Use only existing CSS variables. Never hardcode colours.
- Common: --background-primary, --background-secondary,
  --background-modifier-hover, --text-normal, --text-muted,
  --text-accent, --interactive-accent.
- Shadows: --shadow-s, --shadow-m, --shadow-l. Only on floating
  surfaces (modal, popover, toast).

TYPOGRAPHY
- Font: Inter for UI + body, JetBrains Mono for code. No serif.
- Sizes: --font-ui-small (12) for labels, --font-ui-medium (13) for
  body UI, --font-ui-larger (17) for titles.
- Weights: 400 body, 500 UI rows, 600 titles, 700 only editor bold.

LAYOUT
- Inherit the shell: 48 px left rail, resizable sidebar, fill content,
  resizable chat dock, 26 px status strip.
- Modal widths: 600 for simple, 720 for settings, 800 for AI settings,
  1000 for graph. Height capped at 85vh, content scrolls.

COMPONENTS
- Reuse the vocabulary from design.md §6. Do not invent new components
  when an existing one fits.
- Full inventory (45 primitives, all in src/components/ui/):
  Button, Input, SearchInput, Select, DropdownMenu, ContextMenu,
  CommandPalette, SplitButton, Toggle, Checkbox, Radio,
  SegmentedControl, Slider, NumberStepper, TagInput, DatePicker,
  TimePicker, Breadcrumbs, Tabs, Modal, Drawer, Popover, Toast,
  Snackbar, Banner, Tooltip, Chip, Badge, Kbd, Card, ListRow,
  TreeView, Table, Accordion, FilePickerButton, DropZone, Pagination,
  Stepper, Divider, Progress, Spinner, Skeleton, StatusDot,
  NotificationDot, Avatar, Link, EmptyState.
- Primary button: one per surface. Everything else is secondary or
  ghost.
- **Same purpose means same component.** If the screen needs an
  action menu, it is DropdownMenu — not a bespoke popover. If it
  needs a right-click menu, it is ContextMenu. If it needs a value
  picker with 2-5 options, it is SegmentedControl. Every file tree
  is TreeView. Every shortcut displayed anywhere is Kbd. No
  exceptions without a new primitive added to §6 first.
- Variants are enumerated props (variant / size / density), never
  inline style overrides.

FORBIDDEN
- No gradients on chrome, no glassmorphism, no drop shadows on non-
  floating surfaces, no emoji icons, no "AI sparkle" accents, no
  border-radius > 12, no saturated colours (saturation ceiling 72%),
  no hero sections, no floating action buttons, no mount animations
  on list rows.

ACCESSIBILITY
- Contrast: body text ≥ 7.0, UI text ≥ 4.5.
- Every interactive element has :focus-visible.
- Icon-only buttons have aria-label.
- Respect prefers-reduced-motion.

WHAT TO PRODUCE
1. ASCII wireframe of the page or component.
2. Specific token references for every colour, size, and spacing.
3. Interaction notes (hover, active, focus, disabled, loading).
4. Keyboard navigation (tab order, shortcut keys, Esc behaviour).
5. Empty state, error state, loading state.
6. Light and dark both. Do not design light-only.

SPECIFIC PAGE BRIEFS
[Paste the relevant §8 section from design.md for the page being
designed, e.g. "8.6 Settings (AI)" for the AI settings modal.]
```

---

## 12. Implementation order

1. **Bundle fonts.** Add Manrope (variable) + Newsreader (variable) +
   JetBrains Mono to `src/assets/fonts/`, wire `@font-face` in
   `src/index.css`. Remove any remaining Inter references.
2. **Token audit.** Add §2.3 gaps to `src/index.css`: info colour,
   focus ring, graph node colours, z-index scale, motion durations
   and easing.
3. **ui/ primitives — tier 1 (most used).** Build first: Button,
   Input, SearchInput, Select, DropdownMenu, ContextMenu, Modal,
   Popover, Toast, Tooltip, Kbd, Divider, Spinner, ListRow, Tabs,
   Toggle, Checkbox, Radio, SegmentedControl.
4. **ui/ primitives — tier 2 (feature-unlocking).** CommandPalette,
   Breadcrumbs, Banner, Snackbar, Chip, Badge, Card, EmptyState,
   StatusDot, NotificationDot, Link, Progress, Skeleton.
5. **ui/ primitives — tier 3 (advanced).** TreeView, Table, Drawer,
   Accordion, TagInput, NumberStepper, Slider, DatePicker,
   TimePicker, FilePickerButton, DropZone, Pagination, Stepper,
   SplitButton, Avatar.
6. **Restyle features against the ui/ primitives.** Order: general
   Settings → AI settings → Shell (left rail, sidebar, tabs, status
   strip) → Chat dock → Search/Cmd-P → Graph view → Empty states.
7. **Delete legacy shims.** Remove Obsidian DOM class shims
   component-by-component as each feature is restyled. Not urgent.
8. **Storybook-equivalent doc page.** Build a `/design` internal
   route (only visible in dev) that renders every primitive in every
   variant. Catches regressions and onboards new contributors.

---

## 13. Open questions

- **OQ-D-1:** serif H1 for note titles (Newsreader or similar) or
  stay sans? Leaning sans for productivity consistency.
- **OQ-D-2:** should the accent hue be user-configurable (a hue slider
  in appearance)? Trivial to expose since everything is HSL, but may
  dilute brand. Leaning: no in v1, add in v2 behind an Advanced flag.
- **OQ-D-3:** should the AI settings be a separate top-level modal
  from general settings, or a tab inside the main settings modal?
  Leaning separate modal (too much content for a tab).
- **OQ-D-4:** AI-promoted note visual marker in the sidebar — subtle
  dot, small icon, or colour tint? Tied to OQ-AI-10 in ai.md. Leaning
  small "✦" glyph left of filename in `--text-faint`.
- **OQ-D-5:** status strip content — is word count worth the space,
  or cut for cost+model+voice+save? Leaning cut word count (rarely
  consulted, can live on a status-bar toggle).
