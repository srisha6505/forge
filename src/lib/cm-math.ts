// KaTeX rendering for `$inline$` and `$$display$$` math.
//
// Architecture:
//   - StateField, not ViewPlugin. Decorations are computed for the
//     whole document on doc / active-line / readOnly changes only.
//     CM6 still virtualizes RENDERING by viewport — what we avoid is
//     rebuilding decorations on every scroll.
//   - Active-line awareness comes from the shared activeLinesField,
//     so a cursor sitting on a math line shows the raw `$..$` source
//     and KaTeX widgets render everywhere else.
//   - Code regions (fenced + indented) are excluded via the lezer tree
//     instead of regex line-scanning, so unbalanced fences elsewhere
//     in the doc can't poison the skip set.

import katex from "katex";
import { syntaxTree, syntaxTreeAvailable } from "@codemirror/language";
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
import { activeLinesField, anyLineActive } from "./cm-active-lines";

// Inline `$..$` with the usual rules: not preceded by `\`, no
// whitespace adjacent to the opening/closing `$`, and the body
// must contain at least one non-`$` non-newline char.
const INLINE_RE = /(?<!\\)\$(?!\s)([^$\n]+?)(?<!\s|\\)\$/g;
// Display `$$..$$`, may span multiple lines.
const DISPLAY_RE = /(?<!\\)\$\$([\s\S]+?)\$\$/g;

class MathWidget extends WidgetType {
  constructor(
    readonly src: string,
    readonly displayMode: boolean,
  ) {
    super();
  }
  eq(other: MathWidget) {
    return other.src === this.src && other.displayMode === this.displayMode;
  }
  toDOM() {
    const el = document.createElement(this.displayMode ? "div" : "span");
    el.className = this.displayMode ? "cm-math-display" : "cm-math-inline";
    try {
      el.innerHTML = katex.renderToString(this.src, {
        displayMode: this.displayMode,
        throwOnError: false,
        output: "html",
      });
    } catch {
      el.textContent = this.displayMode ? `$$${this.src}$$` : `$${this.src}$`;
      el.classList.add("cm-math-error");
    }
    return el;
  }
  ignoreEvent() {
    return false;
  }
}

type MathMatch = {
  from: number;
  to: number;
  src: string;
  displayMode: boolean;
};

// Use the syntax tree to find code regions to skip. Both fenced code
// (`FencedCode`, `CodeBlock`) and inline code (`InlineCode`) — math
// inside backticks should remain literal.
function buildSkipRanges(state: EditorState): Array<[number, number]> {
  const skips: Array<[number, number]> = [];
  const tree = syntaxTree(state);
  tree.iterate({
    enter: (node) => {
      if (
        node.name === "FencedCode" ||
        node.name === "CodeBlock" ||
        node.name === "InlineCode"
      ) {
        skips.push([node.from, node.to]);
        return false;
      }
    },
  });
  return skips;
}

function inSkip(skips: Array<[number, number]>, from: number, to: number) {
  for (const [s, e] of skips) {
    if (from < e && to > s) return true;
  }
  return false;
}

function collectMatches(state: EditorState): MathMatch[] {
  const text = state.doc.sliceString(0, state.doc.length);
  const skips = buildSkipRanges(state);
  const matches: MathMatch[] = [];
  const taken: Array<[number, number]> = [];

  DISPLAY_RE.lastIndex = 0;
  let dm: RegExpExecArray | null;
  while ((dm = DISPLAY_RE.exec(text)) !== null) {
    const from = dm.index;
    const to = from + dm[0].length;
    if (inSkip(skips, from, to)) continue;
    const src = dm[1].trim();
    if (!src) continue;
    matches.push({ from, to, src, displayMode: true });
    taken.push([from, to]);
  }

  INLINE_RE.lastIndex = 0;
  let im: RegExpExecArray | null;
  while ((im = INLINE_RE.exec(text)) !== null) {
    const from = im.index;
    const to = from + im[0].length;
    if (inSkip(skips, from, to)) continue;
    let overlapped = false;
    for (const [s, e] of taken) {
      if (from < e && to > s) {
        overlapped = true;
        break;
      }
    }
    if (overlapped) continue;
    const src = im[1];
    matches.push({ from, to, src, displayMode: false });
  }

  matches.sort((a, b) => a.from - b.from);
  return matches;
}

function buildDecorations(state: EditorState): DecorationSet {
  const matches = collectMatches(state);
  if (matches.length === 0) return Decoration.none;
  const builder = new RangeSetBuilder<Decoration>();
  const active = state.field(activeLinesField);
  const doc = state.doc;

  for (const m of matches) {
    const fromLine = doc.lineAt(m.from).number;
    const toLine = doc.lineAt(m.to).number;
    if (anyLineActive(active, fromLine, toLine)) {
      builder.add(
        m.from,
        m.to,
        Decoration.mark({
          class: m.displayMode
            ? "cm-math-raw cm-math-raw-display"
            : "cm-math-raw",
        }),
      );
    } else {
      builder.add(
        m.from,
        m.to,
        Decoration.replace({
          widget: new MathWidget(m.src, m.displayMode),
          // Multi-line `$$..$$` blocks must declare block:true so CM6
          // accepts them as line-spanning replacements.
          block: m.displayMode && m.from < m.to && fromLine !== toLine,
        }),
      );
    }
  }
  return builder.finish();
}

const mathField = StateField.define<DecorationSet>({
  create(state) {
    if (!syntaxTreeAvailable(state)) return Decoration.none;
    try {
      return buildDecorations(state);
    } catch (e) {
      console.error("[cm-math] build failed (create):", e);
      return Decoration.none;
    }
  },
  update(value, tr) {
    const treeChanged = syntaxTree(tr.startState) !== syntaxTree(tr.state);
    const activeChanged =
      tr.startState.field(activeLinesField, false) !==
      tr.state.field(activeLinesField, false);
    if (!tr.docChanged && !treeChanged && !activeChanged) return value;
    if (!syntaxTreeAvailable(tr.state)) return Decoration.none;
    try {
      return buildDecorations(tr.state);
    } catch (e) {
      console.error("[cm-math] build failed (update):", e);
      return value;
    }
  },
  provide: (f) => EditorView.decorations.from(f),
});

const mathTheme = EditorView.theme({
  ".cm-math-inline": {
    cursor: "pointer",
    padding: "0 2px",
    borderRadius: "3px",
  },
  ".cm-math-inline:hover": {
    backgroundColor: "var(--background-modifier-hover)",
  },
  ".cm-math-display": {
    display: "block",
    cursor: "pointer",
    padding: "8px 0",
    margin: "8px 0",
    textAlign: "center",
  },
  ".cm-math-display:hover": {
    backgroundColor: "var(--background-modifier-hover)",
  },
  ".cm-math-raw": {
    color: "var(--text-accent)",
    fontFamily: "var(--font-monospace)",
  },
  ".cm-math-error": {
    color: "var(--text-error)",
  },
});

export default function cmMath(): Extension[] {
  return [mathField, mathTheme];
}
