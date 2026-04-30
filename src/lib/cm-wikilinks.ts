// Obsidian-style wikilinks. Covers the variants the user actually
// writes:
//
//   [[Target]]                       — simple link
//   [[Target|Alias]]                 — link with display alias
//   [[Target#Section]]               — link to a heading
//   [[Target#Section|Alias]]         — section + alias
//   [[Target^block]]                 — link to a block reference
//   [[Folder/Sub/Target]]            — paths
//   ![[Target]]                      — note embed (renders as a styled
//                                      pill that opens the target on
//                                      click)
//   ![[image.png]]                   — image embed (renders inline via
//                                      the same probe cache cm-image
//                                      uses, so the file loads once)
//
// Display formatting (when no explicit alias is given):
//   - `.md` / `.mdx` extension stripped
//   - `Target#Section`  →  `Target › Section`
//   - `Target^block`    →  `Target ¶ block`
//
// Architecture: StateField (not ViewPlugin), no viewport-driven
// rebuilds. Rebuilds only when doc / activeLines change. Click
// handler is a separate domEventHandler so concerns stay clean.

import { syntaxTreeAvailable } from "@codemirror/language";
import {
  type EditorState,
  type Extension,
  RangeSetBuilder,
  StateEffect,
  StateField,
} from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  WidgetType,
} from "@codemirror/view";
import { activeLinesField } from "./cm-active-lines";
import {
  getOrCreateProbe,
  imageMetaFor,
  isImagePath,
  resolveImagePath,
} from "./cm-image";

// Single regex matches both link form `[[..]]` and embed form `![[..]]`.
// Group 1: optional `!` (presence = embed). Group 2: target plus
// optional |alias up to the closing brackets.
const WIKI_RE = /(!?)\[\[([^\]\n]+?)\]\]/g;

// Short-reference form: `[target]` with NO `(url)` / `[ref]` / `:` after
// it, NOT preceded by `[`. Treated as a wikilink only when `target`
// resolves to a real file in the vault tree (we'd otherwise eat plain
// `[NOTE]`-style annotations and markdown link-reference labels).
const SHORT_RE = /(?<!\[)\[([^\[\]\n|]+)\](?![\[(:])/g;

type ParsedWikilink = {
  isEmbed: boolean;
  rawTarget: string;       // before # / ^ / |, no extension stripped
  fullTarget: string;      // target + #section or ^block, no alias
  section: string | null;  // text after # (if any)
  block: string | null;    // text after ^ (if any)
  alias: string | null;
  display: string;
};

function parseInner(inner: string, isEmbed: boolean): ParsedWikilink {
  // Split alias first because pipes can appear after a section.
  const pipeIdx = inner.indexOf("|");
  const before = pipeIdx >= 0 ? inner.slice(0, pipeIdx) : inner;
  const alias = pipeIdx >= 0 ? inner.slice(pipeIdx + 1).trim() : null;

  let rawTarget = before.trim();
  let section: string | null = null;
  let block: string | null = null;

  // Block ref `^id` takes priority — `^` can appear inside section text
  // legally, but in practice Obsidian uses one or the other.
  const caretIdx = rawTarget.indexOf("^");
  if (caretIdx >= 0) {
    block = rawTarget.slice(caretIdx + 1).trim();
    rawTarget = rawTarget.slice(0, caretIdx).trim();
  } else {
    const hashIdx = rawTarget.indexOf("#");
    if (hashIdx >= 0) {
      section = rawTarget.slice(hashIdx + 1).trim();
      rawTarget = rawTarget.slice(0, hashIdx).trim();
    }
  }

  const fullTarget = block
    ? `${rawTarget}^${block}`
    : section
      ? `${rawTarget}#${section}`
      : rawTarget;

  let display: string;
  if (alias) {
    display = alias;
  } else {
    const stripped = rawTarget.replace(/\.(md|mdx)$/i, "");
    if (section) display = `${stripped} › ${section}`;
    else if (block) display = `${stripped} ¶ ${block}`;
    else display = stripped;
  }

  return {
    isEmbed,
    rawTarget,
    fullTarget,
    section,
    block,
    alias,
    display,
  };
}

// ── Widgets ────────────────────────────────────────────────────────────

class WikilinkWidget extends WidgetType {
  constructor(
    readonly display: string,
    readonly fullTarget: string,
  ) {
    super();
  }
  eq(other: WikilinkWidget) {
    return (
      other.display === this.display && other.fullTarget === this.fullTarget
    );
  }
  toDOM() {
    const span = document.createElement("span");
    span.className = "cm-wikilink cm-wikilink-widget";
    span.textContent = this.display;
    span.dataset.target = this.fullTarget;
    span.title = this.fullTarget;
    return span;
  }
  ignoreEvent() {
    return false;
  }
}

class NoteEmbedWidget extends WidgetType {
  constructor(
    readonly display: string,
    readonly fullTarget: string,
  ) {
    super();
  }
  eq(other: NoteEmbedWidget) {
    return (
      other.display === this.display && other.fullTarget === this.fullTarget
    );
  }
  toDOM() {
    const wrap = document.createElement("span");
    wrap.className = "cm-wikilink cm-wikilink-embed";
    wrap.dataset.target = this.fullTarget;
    wrap.title = `Open ${this.fullTarget}`;
    const tag = document.createElement("span");
    tag.className = "cm-wikilink-embed-tag";
    tag.textContent = "embed";
    const label = document.createElement("span");
    label.className = "cm-wikilink-embed-label";
    label.textContent = this.display;
    wrap.appendChild(tag);
    wrap.appendChild(label);
    return wrap;
  }
  ignoreEvent() {
    return false;
  }
}

class ImageEmbedWidget extends WidgetType {
  constructor(
    readonly url: string,
    readonly alt: string,
    readonly raw: string,
  ) {
    super();
  }
  eq(other: ImageEmbedWidget) {
    return other.url === this.url && other.alt === this.alt;
  }
  toDOM() {
    const wrap = document.createElement("div");
    wrap.className = "cm-image-widget cm-image-embed";
    if (!this.url) {
      wrap.classList.add("is-error");
      const label = document.createElement("div");
      label.className = "cm-image-error-label";
      label.textContent = `Image not found: ${this.raw}`;
      wrap.appendChild(label);
      return wrap;
    }
    const meta = getOrCreateProbe(this.url);
    if (meta.failed) {
      wrap.classList.add("is-error");
      const label = document.createElement("div");
      label.className = "cm-image-error-label";
      label.textContent = `Image failed to load: ${this.raw}`;
      wrap.appendChild(label);
      return wrap;
    }
    const img = document.createElement("img");
    img.alt = this.alt;
    img.decoding = "async";
    if (meta.loaded && meta.width && meta.height) {
      img.width = meta.width;
      img.height = meta.height;
    }
    img.src = this.url;
    if (!meta.loaded) {
      img.addEventListener(
        "load",
        () => {
          meta.width = img.naturalWidth;
          meta.height = img.naturalHeight;
          meta.loaded = true;
          if (!img.getAttribute("width")) img.width = meta.width;
          if (!img.getAttribute("height")) img.height = meta.height;
        },
        { once: true },
      );
      img.addEventListener(
        "error",
        () => {
          meta.failed = true;
          wrap.classList.add("is-error");
          img.style.display = "none";
          const label = document.createElement("div");
          label.className = "cm-image-error-label";
          label.textContent = `Image failed to load: ${this.raw}`;
          wrap.appendChild(label);
        },
        { once: true },
      );
    }
    wrap.appendChild(img);
    return wrap;
  }
  ignoreEvent() {
    return false;
  }
}

// ── Match collection ───────────────────────────────────────────────────

type Match = {
  from: number;
  to: number;
  line: number;
  parsed: ParsedWikilink;
};

function collectMatches(
  state: EditorState,
  resolveTarget: ((t: string) => string | null) | null,
): Match[] {
  const out: Match[] = [];
  const doc = state.doc;
  for (let i = 1; i <= doc.lines; i++) {
    const line = doc.line(i);
    const text = line.text;

    // [[..]] / ![[..]]
    if (text.includes("[[")) {
      WIKI_RE.lastIndex = 0;
      let m: RegExpExecArray | null;
      while ((m = WIKI_RE.exec(text)) !== null) {
        const isEmbed = m[1] === "!";
        const inner = m[2];
        if (!inner.trim()) continue;
        out.push({
          from: line.from + m.index,
          to: line.from + m.index + m[0].length,
          line: i,
          parsed: parseInner(inner, isEmbed),
        });
      }
    }

    // [target] short refs — only promote to wikilinks if the target
    // resolves to a vault file. Without the vault check we'd swallow
    // plain `[TODO]` / `[note]` annotations the user never intended as
    // links. Skip lines starting with `[label]:` (markdown link-ref
    // definitions) and skip any region already consumed by a [[..]]
    // match earlier in the same line.
    if (resolveTarget && text.includes("[")) {
      const isLinkRefDef = /^\s*\[[^\]\n]+\]\s*:/.test(text);
      if (isLinkRefDef) continue;
      SHORT_RE.lastIndex = 0;
      let m: RegExpExecArray | null;
      while ((m = SHORT_RE.exec(text)) !== null) {
        const target = m[1].trim();
        if (!target) continue;
        const absFrom = line.from + m.index;
        const absTo = absFrom + m[0].length;
        // Skip if this range overlaps a previously-matched [[..]] pair.
        const overlaps = out.some(
          (existing) =>
            existing.line === i &&
            absFrom < existing.to &&
            absTo > existing.from,
        );
        if (overlaps) continue;
        // Skip if this looks like markdown image syntax (preceded by `!`)
        // — that's already handled elsewhere as a markdown image.
        if (m.index > 0 && text[m.index - 1] === "!") continue;
        // Resolve against vault. Only render as wikilink if it hits.
        if (!resolveTarget(target)) continue;
        out.push({
          from: absFrom,
          to: absTo,
          line: i,
          parsed: parseInner(target, false),
        });
      }
    }
  }
  // Sort by `from` so RangeSetBuilder receives ranges in order.
  out.sort((a, b) => a.from - b.from);
  return out;
}

// ── Decoration build ───────────────────────────────────────────────────

function buildDecorations(
  state: EditorState,
  docPath: string | null,
  resolveTarget: ((t: string) => string | null) | null,
): DecorationSet {
  const matches = collectMatches(state, resolveTarget);
  if (matches.length === 0) return Decoration.none;
  const builder = new RangeSetBuilder<Decoration>();
  const active = state.field(activeLinesField);

  for (const m of matches) {
    if (active.has(m.line)) {
      // Cursor on this line: keep raw markup so the user can edit it.
      builder.add(
        m.from,
        m.to,
        Decoration.mark({
          class: m.parsed.isEmbed
            ? "cm-wikilink cm-wikilink-raw cm-wikilink-raw-embed"
            : "cm-wikilink cm-wikilink-raw",
        }),
      );
      continue;
    }

    const { parsed } = m;

    // Image embed: `![[file.png]]` resolves and renders inline using
    // the same probe cache as cm-image so a file referenced both ways
    // only loads once.
    if (parsed.isEmbed && isImagePath(parsed.rawTarget)) {
      const url = resolveImagePath(parsed.rawTarget, docPath);
      if (url) getOrCreateProbe(url);
      builder.add(
        m.from,
        m.to,
        Decoration.replace({
          widget: new ImageEmbedWidget(url, parsed.display, parsed.rawTarget),
        }),
      );
      continue;
    }

    // Note / non-image embed: stylized pill, click opens the file.
    if (parsed.isEmbed) {
      builder.add(
        m.from,
        m.to,
        Decoration.replace({
          widget: new NoteEmbedWidget(parsed.display, parsed.fullTarget),
        }),
      );
      continue;
    }

    // Plain wikilink.
    builder.add(
      m.from,
      m.to,
      Decoration.replace({
        widget: new WikilinkWidget(parsed.display, parsed.fullTarget),
      }),
    );
  }
  return builder.finish();
}

// Force a wikilink decoration rebuild from outside (e.g., when the
// vault tree loads asynchronously *after* the editor mounted, so
// `[short-ref]` matches start resolving). Without this, the decoration
// only rebuilds on doc-change / active-line-change and short-refs render
// as raw `[text]` until the user types something. App.tsx dispatches
// this when `tree` transitions from null to non-null.
export const wikilinkRescanEffect = StateEffect.define<void>();

// ── State field ────────────────────────────────────────────────────────

function makeField(
  getDocPath: () => string | null,
  getResolveTarget: () => ((t: string) => string | null) | null,
) {
  return StateField.define<DecorationSet>({
    create(state) {
      if (!syntaxTreeAvailable(state)) return Decoration.none;
      try {
        return buildDecorations(state, getDocPath(), getResolveTarget());
      } catch (e) {
        console.error("[cm-wikilinks] build failed (create):", e);
        return Decoration.none;
      }
    },
    update(value, tr) {
      const activeChanged =
        tr.startState.field(activeLinesField, false) !==
        tr.state.field(activeLinesField, false);
      const rescan = tr.effects.some((e) => e.is(wikilinkRescanEffect));
      if (!tr.docChanged && !activeChanged && !rescan) return value;
      if (!syntaxTreeAvailable(tr.state)) return Decoration.none;
      try {
        return buildDecorations(tr.state, getDocPath(), getResolveTarget());
      } catch (e) {
        console.error("[cm-wikilinks] build failed (update):", e);
        return value;
      }
    },
    provide: (f) => EditorView.decorations.from(f),
  });
}

// Suppress unused-import lint when only the type is referenced.
void imageMetaFor;

// ── Click handler ──────────────────────────────────────────────────────

function clickHandler(onOpen: (target: string) => void): Extension {
  return EditorView.domEventHandlers({
    mousedown(event) {
      if (event.button !== 0) return false;
      const target = event.target as HTMLElement | null;
      if (!target) return false;
      const widget = target.closest(
        ".cm-wikilink-widget, .cm-wikilink-embed",
      ) as HTMLElement | null;
      if (!widget) return false;
      const t = widget.dataset.target;
      if (!t) return false;
      event.preventDefault();
      event.stopPropagation();
      try {
        onOpen(t);
      } catch (e) {
        console.error("[cm-wikilinks] onOpen failed for", t, e);
      }
      return true;
    },
  });
}

// ── Theme ──────────────────────────────────────────────────────────────

const wikilinkTheme = EditorView.theme({
  ".cm-wikilink-embed": {
    display: "inline-flex",
    alignItems: "center",
    gap: "6px",
    padding: "2px 8px",
    borderRadius: "4px",
    background: "var(--background-secondary)",
    border: "1px solid var(--background-modifier-border)",
    fontSize: "12px",
    color: "var(--text-link)",
    cursor: "pointer",
    transition:
      "background-color 80ms ease, border-color 80ms ease",
  },
  ".cm-wikilink-embed:hover": {
    background: "var(--background-modifier-hover)",
    borderColor: "var(--interactive-accent)",
  },
  ".cm-wikilink-embed-tag": {
    fontFamily: "var(--font-monospace)",
    fontSize: "10px",
    textTransform: "uppercase",
    letterSpacing: "0.04em",
    color: "var(--text-faint)",
  },
  ".cm-wikilink-embed-label": {
    fontWeight: "500",
  },
  ".cm-image-embed": {
    margin: "12px auto",
  },
  ".cm-wikilink-raw-embed": {
    color: "var(--text-accent)",
  },
});

// ── Public API ─────────────────────────────────────────────────────────

export function createWikilinkPlugin(
  onOpen: (target: string) => void,
  getDocPath: () => string | null,
  getResolveTarget: () => ((t: string) => string | null) | null,
): Extension {
  return [
    makeField(getDocPath, getResolveTarget),
    clickHandler(onOpen),
    wikilinkTheme,
  ];
}
