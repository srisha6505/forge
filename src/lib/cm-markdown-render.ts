// Single source of truth for Forge's markdown render decorations:
//   - Line classes for headings (cm-line-heading-1..6), blockquote
//     (cm-line-quote), code fences (cm-line-code), HRs (cm-line-hr).
//   - Inline marker hides for HeaderMark, EmphasisMark, CodeMark,
//     StrikethroughMark, LinkMark on every line not under a selection.
//   - Block widgets that replace GFM Table nodes with rendered HTML
//     when the cursor isn't inside the table.
//
// CRITICAL ARCHITECTURE NOTE
// --------------------------
// CM6 forbids *block* decorations from any FUNCTION-based decorations
// provider. That includes both:
//   (a) `ViewPlugin.fromClass(cls, { decorations: v => v.decorations })`
//   (b) `EditorView.decorations.of(view => ...)`  -- standalone or via
//       `provide` inside a plugin.
// Both are "dynamic" providers; CM6 sets `disallowBlockEffectsFor=true`
// for any function provider and throws "Block decorations may not be
// specified via plugins" the moment a `Decoration.line()` or
// `Decoration.replace({block:true})` shows up in their output.
//
// The ONLY allowed path for block decorations is a STATIC provider:
//   `EditorView.decorations.from(stateField)`
// where `stateField` is a `StateField<DecorationSet>` whose value is
// recomputed inside `update(value, tr)`. CM6 treats this as static
// (it reads the field directly, not via a function call) and allows
// blocks.
//
// Tradeoff: state fields don't know the viewport. We iterate the whole
// syntax tree on every doc/selection/tree-parse-completion transaction
// instead of just the visible range. Acceptable for vault notes (most
// are under 2k lines); if perf bites, layer a viewport-aware view
// plugin on top later.

import {
  syntaxTree,
  syntaxTreeAvailable,
} from "@codemirror/language";
import {
  type EditorState,
  type Extension,
  RangeSetBuilder,
  StateField,
} from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  WidgetType,
} from "@codemirror/view";

// ── Inline marker config ───────────────────────────────────────────────

const HIDE = Decoration.replace({});

const MARKER_NODES = new Set([
  "EmphasisMark",
  "CodeMark",
  "StrikethroughMark",
  "LinkMark",
]);

// ── Line decoration helpers ────────────────────────────────────────────

const lineDeco = (cls: string) =>
  Decoration.line({ attributes: { class: cls } });

// ── Table widget ──────────────────────────────────────────────────────

function escapeHtml(s: string): string {
  return s.replace(
    /[&<>"']/g,
    (c) =>
      (
        {
          "&": "&amp;",
          "<": "&lt;",
          ">": "&gt;",
          '"': "&quot;",
          "'": "&#39;",
        } as Record<string, string>
      )[c],
  );
}

const WIKILINK_RE_TABLE = /\[\[([^\]|\n]+?)(?:\|([^\]\n]+?))?\]\]/g;
const MD_LINK_RE_TABLE = /\[([^\]\n]+?)\]\(([^)\n]+?)\)/g;
const CODE_RE_TABLE = /`([^`\n]+?)`/g;
const BOLD_RE_TABLE = /\*\*([^*\n]+?)\*\*/g;
const ITAL_RE_TABLE = /(?<!\*)\*([^*\n]+?)\*(?!\*)/g;

function renderCellHtml(raw: string): string {
  let s = escapeHtml(raw);
  s = s.replace(
    WIKILINK_RE_TABLE,
    (_, target, alias) =>
      `<span class="cm-wikilink cm-wikilink-widget" data-target="${escapeHtml(target.trim())}">${escapeHtml((alias || target).trim())}</span>`,
  );
  s = s.replace(
    MD_LINK_RE_TABLE,
    (_, text, url) =>
      `<a class="cm-md-link" data-url="${escapeHtml(url.trim())}">${escapeHtml(text.trim())}</a>`,
  );
  s = s.replace(CODE_RE_TABLE, (_, code) => `<code>${escapeHtml(code)}</code>`);
  s = s.replace(
    BOLD_RE_TABLE,
    (_, inner) => `<strong>${escapeHtml(inner)}</strong>`,
  );
  s = s.replace(
    ITAL_RE_TABLE,
    (_, inner) => `<em>${escapeHtml(inner)}</em>`,
  );
  return s;
}

function splitRow(line: string): string[] {
  const inner = line.replace(/^\s*\|/, "").replace(/\|\s*$/, "");
  return inner.split("|").map((c) => c.trim());
}

function parseMarkdownTable(
  text: string,
): { headers: string[]; rows: string[][] } | null {
  const lines = text.split("\n").filter((l) => l.trim());
  if (lines.length < 2) return null;
  const headers = splitRow(lines[0]);
  if (!/^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)+\|?\s*$/.test(lines[1])) {
    return null;
  }
  const rows = lines.slice(2).map(splitRow);
  const n = headers.length;
  const padded = rows.map((r) => {
    if (r.length === n) return r;
    if (r.length < n) return [...r, ...Array(n - r.length).fill("")];
    return r.slice(0, n);
  });
  return { headers, rows: padded };
}

class TableWidget extends WidgetType {
  constructor(
    readonly headers: string[],
    readonly rows: string[][],
  ) {
    super();
  }
  eq(other: TableWidget) {
    if (this.headers.length !== other.headers.length) return false;
    if (!this.headers.every((h, i) => h === other.headers[i])) return false;
    if (this.rows.length !== other.rows.length) return false;
    for (let i = 0; i < this.rows.length; i++) {
      const a = this.rows[i];
      const b = other.rows[i];
      if (a.length !== b.length) return false;
      for (let j = 0; j < a.length; j++) if (a[j] !== b[j]) return false;
    }
    return true;
  }
  toDOM() {
    const wrap = document.createElement("div");
    wrap.className = "cm-table-widget-wrap";
    const table = document.createElement("table");
    table.className = "cm-table-widget";
    const thead = document.createElement("thead");
    const thr = document.createElement("tr");
    for (const h of this.headers) {
      const th = document.createElement("th");
      th.innerHTML = renderCellHtml(h);
      thr.appendChild(th);
    }
    thead.appendChild(thr);
    table.appendChild(thead);
    const tbody = document.createElement("tbody");
    for (const row of this.rows) {
      const tr = document.createElement("tr");
      for (const cell of row) {
        const td = document.createElement("td");
        td.innerHTML = renderCellHtml(cell);
        tr.appendChild(td);
      }
      tbody.appendChild(tr);
    }
    table.appendChild(tbody);
    wrap.appendChild(table);
    return wrap;
  }
  ignoreEvent() {
    return false;
  }
}

// ── Build the decoration set from editor state ─────────────────────────

interface DecoItem {
  from: number;
  to: number;
  deco: Decoration;
}

function buildAll(state: EditorState): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const doc = state.doc;
  const tree = syntaxTree(state);

  const activeLines = new Set<number>();
  for (const range of state.selection.ranges) {
    const s = doc.lineAt(range.from).number;
    const e = doc.lineAt(range.to).number;
    for (let l = s; l <= e; l++) activeLines.add(l);
  }

  const items: DecoItem[] = [];
  const lineDecorated = new Set<number>();

  const addLine = (lineNumber: number, lineFrom: number, cls: string) => {
    if (lineDecorated.has(lineNumber)) return;
    lineDecorated.add(lineNumber);
    items.push({ from: lineFrom, to: lineFrom, deco: lineDeco(cls) });
  };

  tree.iterate({
    enter: (node) => {
      const startLineNo = doc.lineAt(node.from).number;
      const lineEndPos = doc.lineAt(node.from).to;

      // ── Headings ──────────────────────────────────────────────
      const headingMatch = node.name.match(/^ATXHeading(\d)$/);
      if (headingMatch) {
        const level = Number(headingMatch[1]);
        const lineFrom = doc.lineAt(node.from).from;
        addLine(startLineNo, lineFrom, `cm-line-heading-${level}`);
        return; // descend so we hit HeaderMark
      }

      if (node.name === "HeaderMark") {
        if (activeLines.has(startLineNo)) return false;
        let end = node.to;
        if (end < lineEndPos && doc.sliceString(end, end + 1) === " ") {
          end += 1;
        }
        if (end > lineEndPos) end = lineEndPos;
        if (end > node.from) {
          items.push({ from: node.from, to: end, deco: HIDE });
        }
        return false;
      }

      // ── Blockquote ────────────────────────────────────────────
      if (node.name === "Blockquote") {
        let pos = node.from;
        while (pos <= node.to && pos < doc.length) {
          const line = doc.lineAt(pos);
          addLine(line.number, line.from, "cm-line-quote");
          if (line.to >= doc.length) break;
          pos = line.to + 1;
        }
        return; // descend for inline marker hides
      }

      // ── Code fences ───────────────────────────────────────────
      if (node.name === "FencedCode" || node.name === "CodeBlock") {
        let pos = node.from;
        while (pos <= node.to && pos < doc.length) {
          const line = doc.lineAt(pos);
          addLine(line.number, line.from, "cm-line-code");
          if (line.to >= doc.length) break;
          pos = line.to + 1;
        }
        return false; // inside fences isn't markdown
      }

      // ── Horizontal rule ───────────────────────────────────────
      if (node.name === "HorizontalRule") {
        const lineFrom = doc.lineAt(node.from).from;
        addLine(startLineNo, lineFrom, "cm-line-hr");
        return false;
      }

      // ── Inline marker hides ───────────────────────────────────
      if (MARKER_NODES.has(node.name)) {
        if (activeLines.has(startLineNo)) return;
        if (node.to <= node.from) return;
        const safeEnd = Math.min(node.to, lineEndPos);
        if (safeEnd > node.from) {
          items.push({ from: node.from, to: safeEnd, deco: HIDE });
        }
        return;
      }

      // ── Tables ────────────────────────────────────────────────
      if (node.name === "Table") {
        const lastByte = Math.max(node.from, node.to - 1);
        const endLineNo = doc.lineAt(lastByte).number;
        let cursorInside = false;
        for (let l = startLineNo; l <= endLineNo; l++) {
          if (activeLines.has(l)) {
            cursorInside = true;
            break;
          }
        }
        if (cursorInside) return false;

        const text = doc.sliceString(node.from, node.to);
        const parsed = parseMarkdownTable(text);
        if (!parsed) return false;

        const lineStart = doc.lineAt(node.from).from;
        const lineEnd = doc.lineAt(lastByte).to;
        items.push({
          from: lineStart,
          to: lineEnd,
          deco: Decoration.replace({
            widget: new TableWidget(parsed.headers, parsed.rows),
            block: true,
          }),
        });
        for (let l = startLineNo; l <= endLineNo; l++) {
          lineDecorated.add(l);
        }
        return false;
      }
    },
  });

  // Sort by `from`. For ties, line decorations (zero-length, from===to)
  // come before mark/replace. Ties between two zero-length items can
  // happen via `addLine` dedup so we don't worry about them.
  items.sort((a, b) => {
    if (a.from !== b.from) return a.from - b.from;
    const aZero = a.from === a.to;
    const bZero = b.from === b.to;
    if (aZero && !bZero) return -1;
    if (!aZero && bZero) return 1;
    return 0;
  });

  for (const item of items) {
    builder.add(item.from, item.to, item.deco);
  }

  return builder.finish();
}

// ── State field ────────────────────────────────────────────────────────
//
// The state field is the source of truth for decorations. It rebuilds
// when the doc, selection, or syntax tree changes — and crucially,
// `EditorView.decorations.from(field)` exposes it as a static (non-
// function) provider, which CM6 allows for block decorations.

const decorationField = StateField.define<DecorationSet>({
  create(state) {
    if (!syntaxTreeAvailable(state)) return Decoration.none;
    try {
      return buildAll(state);
    } catch (e) {
      console.error("[markdownRender] build failed (create):", e);
      return Decoration.none;
    }
  },
  update(value, tr) {
    const treeChanged =
      syntaxTree(tr.startState) !== syntaxTree(tr.state);
    if (!tr.docChanged && !tr.selection && !treeChanged) return value;
    if (!syntaxTreeAvailable(tr.state)) {
      // Tree mid-parse — drop decorations to avoid stale-position renders.
      return Decoration.none;
    }
    try {
      return buildAll(tr.state);
    } catch (e) {
      console.error("[markdownRender] build failed (update):", e);
      return value;
    }
  },
  provide: (f) => EditorView.decorations.from(f),
});

// ── Click handler for table widgets ────────────────────────────────────

const tableClickHandler = EditorView.domEventHandlers({
  mousedown(event, view) {
    const el = event.target as HTMLElement | null;
    if (!el) return false;
    const wrap = el.closest(".cm-table-widget-wrap");
    if (!wrap) return false;
    if (el.closest(".cm-wikilink-widget")) return false;
    if (el.closest(".cm-md-link")) return false;
    const pos = view.posAtDOM(wrap as HTMLElement);
    if (pos == null) return false;
    event.preventDefault();
    view.dispatch({ selection: { anchor: pos, head: pos } });
    view.focus();
    return true;
  },
});

// Force CM6 to re-measure line heights after a transaction that changes
// the decoration field. Block decorations (line classes that change
// font-size, table widgets that replace several lines with one widget)
// alter line heights; if CM6 doesn't remeasure, click coordinates map
// to the wrong lines.
const remeasureOnDecorationChange = EditorView.updateListener.of((update) => {
  if (
    update.docChanged ||
    update.transactions.some((tr) =>
      tr.effects.some(() => true),
    ) ||
    update.startState.field(decorationField, false) !==
      update.state.field(decorationField, false)
  ) {
    update.view.requestMeasure();
  }
});

// ── Public extension ───────────────────────────────────────────────────

export const markdownRenderExtension: Extension = [
  decorationField,
  // atomicRanges still needs a function provider (cursor movement is a
  // view-level concern). Block decorations in this set are fine here —
  // atomicRanges doesn't render anything, it just gates cursor placement.
  EditorView.atomicRanges.of(
    (view) => view.state.field(decorationField, false) ?? Decoration.none,
  ),
  remeasureOnDecorationChange,
  tableClickHandler,
];
