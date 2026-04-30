// Render `\`\`\`mermaid` fenced code blocks as inline SVG diagrams.
//
// Unlike cm-htmlwidget (which mounts an iframe), Mermaid output is a
// static SVG document — safe to drop directly into the editor's DOM.
// No script execution, no sandbox needed. The trade vs html-widget is
// non-interactivity: a Mermaid diagram is a frozen picture, not a
// playground. That's the right answer for "show me the structure of
// this system" — interactivity adds nothing for a flowchart.
//
// Mermaid is loaded lazily on first use (~600KB gzipped). Subsequent
// fences in the same session reuse the loaded module.

import {
  Decoration,
  type DecorationSet,
  EditorView,
  WidgetType,
} from "@codemirror/view";
import { type EditorState, RangeSetBuilder, StateField } from "@codemirror/state";
import {
  syntaxTree,
  syntaxTreeAvailable,
} from "@codemirror/language";
import type { Extension } from "@codemirror/state";
import { activeLinesField } from "./cm-active-lines";

// Mermaid singleton. Loaded once per page on first render call.
let mermaidPromise: Promise<typeof import("mermaid").default> | null = null;
function getMermaid(): Promise<typeof import("mermaid").default> {
  if (mermaidPromise) return mermaidPromise;
  mermaidPromise = import("mermaid").then((mod) => {
    const m = mod.default;
    m.initialize({
      startOnLoad: false,
      securityLevel: "strict",
      theme: "base",
      themeVariables: readThemeVariables(),
      // No flowchart-specific overrides — let user prompt drive variety.
      // Diagram-type-specific config can be added here when warranted.
    });
    return m;
  });
  return mermaidPromise;
}

// Pull the editor's theme tokens into Mermaid's themeVariables so the
// diagram visually matches the rest of the note. Reading from <body>
// because Forge applies theme classes there, not on documentElement.
function readThemeVariables(): Record<string, string> {
  if (typeof document === "undefined") return {};
  const target = document.body ?? document.documentElement;
  const cs = getComputedStyle(target);
  const v = (name: string, fallback: string) => {
    const got = cs.getPropertyValue(name).trim();
    return got || fallback;
  };
  const bg = v("--background-primary", "#ffffff");
  const bgAlt = v("--background-primary-alt", "#f5f5f5");
  const text = v("--text-normal", "#222222");
  const textMuted = v("--text-muted", "#666666");
  const accent = v("--interactive-accent", "#c08a2e");
  const border = v("--background-modifier-border", "#dddddd");
  return {
    primaryColor: bgAlt,
    primaryTextColor: text,
    primaryBorderColor: border,
    lineColor: textMuted,
    secondaryColor: bg,
    tertiaryColor: bgAlt,
    background: bg,
    mainBkg: bgAlt,
    nodeBorder: accent,
    clusterBkg: bg,
    clusterBorder: border,
    titleColor: text,
    edgeLabelBackground: bg,
    textColor: text,
    fontFamily: v("--font-text", "system-ui, sans-serif"),
  };
}

class MermaidWidget extends WidgetType {
  // Stable id per source — avoids redundant re-renders when CM rebuilds
  // the decoration set but the source hasn't changed.
  private static counter = 0;
  readonly id: string;
  constructor(readonly src: string) {
    super();
    this.id = `forge-mermaid-${++MermaidWidget.counter}`;
  }
  eq(other: MermaidWidget) {
    return other.src === this.src;
  }
  toDOM() {
    const wrap = document.createElement("div");
    wrap.className = "cm-mermaid-wrap";
    // Placeholder while Mermaid loads + renders. Matches the visual
    // weight of the final diagram so layout doesn't jump on swap-in.
    wrap.style.cssText = `
      margin: 0.75rem 0; padding: 1rem;
      background: var(--background-primary-alt, #f5f5f5);
      border: 1px solid var(--background-modifier-border, #ddd);
      border-radius: 6px;
      color: var(--text-muted, #666);
      font-size: 0.85em;
      text-align: center;
      min-height: 80px;
      display: flex; align-items: center; justify-content: center;
    `.trim();
    wrap.textContent = "rendering diagram…";

    getMermaid()
      .then(async (m) => {
        try {
          // Mermaid v10+ returns { svg, bindFunctions? } from render().
          const result = await m.render(this.id, this.src);
          wrap.innerHTML = result.svg;
          wrap.style.cssText = `
            margin: 0.75rem 0; padding: 0.5rem;
            background: var(--background-primary, transparent);
            display: flex; justify-content: center;
            overflow-x: auto;
          `.trim();
          // SVGs render at intrinsic size by default; force responsive.
          const svg = wrap.querySelector("svg");
          if (svg) {
            svg.removeAttribute("height");
            svg.style.maxWidth = "100%";
            svg.style.height = "auto";
          }
          if (result.bindFunctions) {
            result.bindFunctions(wrap);
          }
        } catch (err) {
          // Surface parse errors clearly so the author knows what to fix.
          // No "silent fall back to source" — that hides bugs and trains
          // the model to write loose syntax.
          wrap.style.cssText = `
            margin: 0.75rem 0; padding: 0.75rem 1rem;
            background: var(--background-primary-alt, #fff5f0);
            border: 1px solid var(--text-error, #d04030);
            border-radius: 6px;
            color: var(--text-error, #d04030);
            font-family: var(--font-mono, monospace);
            font-size: 0.85em;
            white-space: pre-wrap;
          `.trim();
          wrap.textContent = `Mermaid parse error: ${err instanceof Error ? err.message : String(err)}`;
        }
      })
      .catch((err) => {
        wrap.textContent = `Mermaid load failed: ${err}`;
      });

    return wrap;
  }
  ignoreEvent() {
    return false;
  }
}

interface DecoItem {
  from: number;
  to: number;
  deco: Decoration;
}

function parseMermaidInfo(infoLine: string): boolean {
  const after = infoLine.replace(/^\s*(?:```+|~~~+)\s*/, "");
  return /^mermaid\b/.test(after);
}

function buildAll(state: EditorState): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const doc = state.doc;
  const tree = syntaxTree(state);
  const items: DecoItem[] = [];
  const active = state.field(activeLinesField);

  tree.iterate({
    enter: (node) => {
      if (node.name !== "FencedCode") return;
      const openLine = doc.lineAt(node.from);
      const closeLine = doc.lineAt(Math.min(node.to, doc.length) - 1);
      if (!parseMermaidInfo(openLine.text)) return false;

      // Cursor inside the fence: render raw markup so the author can edit.
      // Same convention as cm-codeblock and cm-htmlwidget.
      let cursorInside = false;
      for (let l = openLine.number; l <= closeLine.number; l++) {
        if (active.has(l)) {
          cursorInside = true;
          break;
        }
      }
      if (cursorInside) return false;

      const contentFrom = Math.min(openLine.to + 1, doc.length);
      const contentTo =
        closeLine.number > openLine.number
          ? Math.max(contentFrom, closeLine.from - 1)
          : doc.length;
      const src = doc.sliceString(contentFrom, contentTo).trim();
      if (!src) return false;

      const replaceFrom = openLine.from;
      const replaceTo =
        closeLine.number === doc.lines
          ? doc.length
          : Math.min(closeLine.to + 1, doc.length);

      items.push({
        from: replaceFrom,
        to: replaceTo,
        deco: Decoration.replace({
          widget: new MermaidWidget(src),
          block: true,
        }),
      });
      return false;
    },
  });

  items.sort((a, b) => a.from - b.from);
  for (const item of items) builder.add(item.from, item.to, item.deco);
  return builder.finish();
}

const mermaidField = StateField.define<DecorationSet>({
  create(state) {
    if (!syntaxTreeAvailable(state)) return Decoration.none;
    try {
      return buildAll(state);
    } catch (e) {
      console.error("[cm-mermaid] build failed (create):", e);
      return Decoration.none;
    }
  },
  update(value, tr) {
    if (!tr.docChanged && !tr.selection) return value;
    if (!syntaxTreeAvailable(tr.state)) return value;
    try {
      return buildAll(tr.state);
    } catch (e) {
      console.error("[cm-mermaid] build failed (update):", e);
      return value;
    }
  },
  provide: (f) => EditorView.decorations.from(f),
});

const mermaidTheme = EditorView.theme({
  ".cm-mermaid-wrap svg": {
    maxWidth: "100%",
    height: "auto",
  },
});

export const mermaidExtension: Extension = [mermaidField, mermaidTheme];
