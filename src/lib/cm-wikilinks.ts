// Detect [[Target]] and [[Target|alias]] in the document. On lines the
// cursor isn't on, render them as a styled link widget. On the cursor
// line, leave them as raw text but mark them with a class so the
// highlight style matches.
//
// Click (or Ctrl+click) on a wikilink widget invokes the onOpen
// callback with the target string. App.tsx walks the vault tree to
// resolve it to a real file path.

import type { Extension } from "@codemirror/state";
import { RangeSetBuilder } from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  ViewPlugin,
  type ViewUpdate,
  WidgetType,
} from "@codemirror/view";

const WIKILINK_RE = /\[\[([^\]|\n]+?)(?:\|([^\]\n]+?))?\]\]/g;

class WikilinkWidget extends WidgetType {
  constructor(
    readonly display: string,
    readonly target: string,
  ) {
    super();
  }
  eq(other: WikilinkWidget) {
    return other.display === this.display && other.target === this.target;
  }
  toDOM() {
    const span = document.createElement("span");
    span.className = "cm-wikilink cm-wikilink-widget";
    span.textContent = this.display;
    span.dataset.target = this.target;
    span.title = this.target;
    return span;
  }
  ignoreEvent() {
    return false;
  }
}

// One match in the visible area. `line` is the 1-based line number the
// match sits on; it's used by the selection-gating short-circuit.
type WikilinkMatch = {
  from: number;
  to: number;
  line: number;
  display: string;
  target: string;
};

// Scan the visible ranges once and return all wikilink matches in
// document order.
function collectVisibleWikilinks(view: EditorView): WikilinkMatch[] {
  const doc = view.state.doc;
  const matches: WikilinkMatch[] = [];

  for (const { from, to } of view.visibleRanges) {
    let pos = from;
    while (pos <= to) {
      const line = doc.lineAt(pos);
      if (line.from >= to) break;
      const text = line.text;
      WIKILINK_RE.lastIndex = 0;
      let m;
      while ((m = WIKILINK_RE.exec(text)) !== null) {
        const absFrom = line.from + m.index;
        const absTo = absFrom + m[0].length;
        const target = m[1].trim();
        const alias = m[2]?.trim();
        const display = alias || target;
        matches.push({
          from: absFrom,
          to: absTo,
          line: line.number,
          display,
          target,
        });
      }
      pos = line.to + 1;
    }
  }
  return matches;
}

// Build the decoration set from a precomputed match list, with the
// active line treated as "raw".
function buildFromMatches(
  view: EditorView,
  matches: WikilinkMatch[],
): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const selLine = view.state.doc.lineAt(view.state.selection.main.head).number;

  for (const m of matches) {
    if (m.line === selLine) {
      builder.add(
        m.from,
        m.to,
        Decoration.mark({ class: "cm-wikilink cm-wikilink-raw" }),
      );
    } else {
      builder.add(
        m.from,
        m.to,
        Decoration.replace({
          widget: new WikilinkWidget(m.display, m.target),
        }),
      );
    }
  }
  return builder.finish();
}

// Does the cursor sit on a line that has a wikilink on it? Used to
// decide whether a cursor move should force a rebuild — only the
// flip from "on a link line" to "off a link line" (or vice versa)
// matters.
function cursorOnWikilinkLine(
  view: EditorView,
  matches: WikilinkMatch[],
): boolean {
  if (matches.length === 0) return false;
  const selLine = view.state.doc.lineAt(view.state.selection.main.head).number;
  for (const m of matches) {
    if (m.line === selLine) return true;
  }
  return false;
}

export function createWikilinkPlugin(
  onOpen: (target: string) => void,
): Extension {
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      matches: WikilinkMatch[];
      prevCursorLine: number;
      prevCursorOnLink: boolean;

      constructor(view: EditorView) {
        this.matches = collectVisibleWikilinks(view);
        this.prevCursorLine = view.state.doc.lineAt(
          view.state.selection.main.head,
        ).number;
        this.prevCursorOnLink = cursorOnWikilinkLine(view, this.matches);
        this.decorations = buildFromMatches(view, this.matches);
      }

      update(update: ViewUpdate) {
        if (update.docChanged || update.viewportChanged) {
          this.matches = collectVisibleWikilinks(update.view);
          this.prevCursorLine = update.state.doc.lineAt(
            update.state.selection.main.head,
          ).number;
          this.prevCursorOnLink = cursorOnWikilinkLine(
            update.view,
            this.matches,
          );
          this.decorations = buildFromMatches(update.view, this.matches);
          return;
        }
        if (update.selectionSet) {
          const newLine = update.state.doc.lineAt(
            update.state.selection.main.head,
          ).number;
          if (newLine === this.prevCursorLine) return;
          const nowOnLink = cursorOnWikilinkLine(update.view, this.matches);
          // Rebuild only if the cursor either entered or left a wikilink
          // line. Moving between two non-link lines doesn't change the
          // decoration set.
          if (nowOnLink || this.prevCursorOnLink) {
            this.decorations = buildFromMatches(update.view, this.matches);
          }
          this.prevCursorLine = newLine;
          this.prevCursorOnLink = nowOnLink;
        }
      }
    },
    {
      decorations: (v) => v.decorations,
      eventHandlers: {
        // Use mousedown so the open fires before CM6's click-to-place-cursor
        // logic. Without `event.preventDefault()` CM6 still moves the
        // cursor; we explicitly prevent and stop propagation so the click
        // is exclusively the wikilink-open action.
        mousedown(event) {
          // Only left-click.
          if (event.button !== 0) return false;
          const target = event.target as HTMLElement | null;
          if (!target) return false;
          const wikilink = target.closest(".cm-wikilink-widget") as
            | HTMLElement
            | null;
          if (!wikilink) return false;
          const t = wikilink.dataset.target;
          if (!t) return false;
          event.preventDefault();
          event.stopPropagation();
          try {
            onOpen(t);
          } catch (e) {
            console.error("[wikilink] onOpen failed for", t, e);
          }
          return true;
        },
      },
    },
  );
}
