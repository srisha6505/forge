// Code-block chrome: small header bar above each fenced code block
// showing the info-string language tag and a copy-to-clipboard button.
// Per-language syntax highlighting itself comes from lang-markdown's
// codeLanguages mapping (wired in Editor.tsx) — this file only adds the
// chrome row.
//
// Same architecture as cm-markdown-render: a StateField produces a
// DecorationSet, exposed via EditorView.decorations.from(field) so CM6
// accepts block decorations.

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

class CodeChromeWidget extends WidgetType {
  constructor(
    readonly lang: string,
    readonly content: string,
  ) {
    super();
  }
  eq(other: CodeChromeWidget) {
    return other.lang === this.lang && other.content === this.content;
  }
  toDOM() {
    const wrap = document.createElement("div");
    wrap.className = "cm-codechrome";
    wrap.setAttribute("aria-hidden", "true");

    const label = document.createElement("span");
    label.className = "cm-codechrome-lang";
    label.textContent = this.lang || "text";
    wrap.appendChild(label);

    const copyBtn = document.createElement("button");
    copyBtn.type = "button";
    copyBtn.className = "cm-codechrome-copy";
    copyBtn.title = "Copy code";
    copyBtn.innerHTML = `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg><span class="cm-codechrome-copylabel">Copy</span>`;

    const content = this.content;
    copyBtn.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
    });
    copyBtn.addEventListener("click", async (e) => {
      e.preventDefault();
      e.stopPropagation();
      try {
        await navigator.clipboard.writeText(content);
        const labelEl = copyBtn.querySelector(".cm-codechrome-copylabel");
        if (labelEl) {
          const original = labelEl.textContent;
          labelEl.textContent = "Copied";
          copyBtn.classList.add("is-copied");
          setTimeout(() => {
            labelEl.textContent = original ?? "Copy";
            copyBtn.classList.remove("is-copied");
          }, 1200);
        }
      } catch (err) {
        console.error("[codechrome] clipboard write failed", err);
      }
    });
    wrap.appendChild(copyBtn);

    return wrap;
  }
  ignoreEvent() {
    // Let our own listeners run; CM6 should not steal mouse events for
    // cursor placement on the chrome bar.
    return true;
  }
}

interface DecoItem {
  from: number;
  to: number;
  deco: Decoration;
}

function buildAll(state: EditorState): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const doc = state.doc;
  const tree = syntaxTree(state);
  const items: DecoItem[] = [];

  tree.iterate({
    enter: (node) => {
      if (node.name !== "FencedCode") return;

      const openLine = doc.lineAt(node.from);
      const closeLine = doc.lineAt(Math.min(node.to, doc.length) - 1);

      const m = /^\s*(?:```+|~~~+)\s*([^\s`~]*)/.exec(openLine.text);
      const lang = (m?.[1] ?? "").toLowerCase();

      // html-widget owns its own rendering surface (cm-htmlwidget.ts
      // replaces the entire fence with an iframe). mermaid likewise
      // (cm-mermaid.ts replaces with an inline SVG). Don't paint code
      // chrome on top of either — that produces a stale "HTML-WIDGET /
      // Copy" header bar floating above the live render.
      if (lang === "js-widget" || lang === "html-widget" || lang === "mermaid") return false;

      const contentFrom = Math.min(openLine.to + 1, doc.length);
      const contentTo =
        closeLine.number > openLine.number
          ? Math.max(contentFrom, closeLine.from - 1)
          : doc.length;
      const content = doc.sliceString(contentFrom, contentTo);

      // Place the chrome bar as a block widget at the start of the
      // opener line (side: -1 means "before the line"). This keeps the
      // fence markup editable — the opener `\`\`\`python` line stays in
      // place underneath the chrome.
      items.push({
        from: openLine.from,
        to: openLine.from,
        deco: Decoration.widget({
          widget: new CodeChromeWidget(lang, content),
          block: true,
          side: -1,
        }),
      });
      return false;
    },
  });

  items.sort((a, b) => {
    if (a.from !== b.from) return a.from - b.from;
    return a.to - b.to;
  });
  for (const item of items) builder.add(item.from, item.to, item.deco);
  return builder.finish();
}

const codeChromeField = StateField.define<DecorationSet>({
  create(state) {
    if (!syntaxTreeAvailable(state)) return Decoration.none;
    try {
      return buildAll(state);
    } catch (e) {
      console.error("[codechrome] build failed (create):", e);
      return Decoration.none;
    }
  },
  update(value, tr) {
    const treeChanged = syntaxTree(tr.startState) !== syntaxTree(tr.state);
    if (!tr.docChanged && !treeChanged) return value;
    if (!syntaxTreeAvailable(tr.state)) return Decoration.none;
    try {
      return buildAll(tr.state);
    } catch (e) {
      console.error("[codechrome] build failed (update):", e);
      return value;
    }
  },
  provide: (f) => EditorView.decorations.from(f),
});

const codeChromeTheme = EditorView.theme({
  ".cm-codechrome": {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    padding: "4px 12px",
    margin: "8px 0 0 0",
    fontFamily: "var(--font-monospace)",
    fontSize: "11px",
    fontWeight: "500",
    color: "var(--text-muted)",
    background: "var(--code-background)",
    borderTopLeftRadius: "6px",
    borderTopRightRadius: "6px",
    borderBottom:
      "1px solid var(--background-modifier-border)",
    userSelect: "none",
    // Layout containment so the chrome's flex layout doesn't bubble
    // reflow up into the editor's main flow. The chrome is small and
    // its content is fixed in size, so layout-only containment gives
    // the win without risking webkit2gtk's CM measurement bug noted
    // in cm-image.ts.
    contain: "layout",
  },
  ".cm-codechrome-lang": {
    textTransform: "uppercase",
    letterSpacing: "0.04em",
    color: "var(--text-faint)",
  },
  ".cm-codechrome-copy": {
    display: "inline-flex",
    alignItems: "center",
    gap: "4px",
    padding: "2px 8px",
    border: "0",
    borderRadius: "4px",
    background: "transparent",
    color: "var(--text-muted)",
    cursor: "pointer",
    fontFamily: "inherit",
    fontSize: "11px",
    fontWeight: "500",
    transition: "background-color 80ms ease, color 80ms ease",
  },
  ".cm-codechrome-copy:hover": {
    background: "var(--background-modifier-hover)",
    color: "var(--text-normal)",
  },
  ".cm-codechrome-copy.is-copied": {
    color: "var(--text-success)",
  },
});

export const codeBlockChromeExtension: Extension = [
  codeChromeField,
  codeChromeTheme,
];
