// CodeMirror 6 theme + extension wiring. All colours resolve through
// Obsidian-compatible CSS variables in src/index.css so toggling
// .theme-light / .theme-dark on <body> flips the editor palette.
//
// Extension layers (in order):
//   forgeTheme                — colours, caret, selection, gutter hide
//   forgeMarkdownHighlight    — inline tag styles (bold, italic, code, link)
//   markdownRenderExtension   — line decorations + marker hides + table widgets
//                               (single tree-walking plugin + standalone facets)
//   createWikilinkPlugin      — widget rendering + click-to-open for [[..]]
//   createHyperlinkClickHandler — Ctrl+click on [text](url) opens in shell

import type { Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";
import { markdownRenderExtension } from "./cm-markdown-render";
import { createWikilinkPlugin } from "./cm-wikilinks";
import { createHyperlinkClickHandler } from "./cm-hyperlinks";

// Theme: only colours / borders / cursors. Height is intentionally NOT
// set here — @uiw/react-codemirror's `height` prop owns the editor's
// pixel height via its own injected theme rule.
export const forgeTheme = EditorView.theme(
  {
    "&": {
      color: "var(--text-normal)",
      backgroundColor: "var(--background-primary)",
    },
    ".cm-scroller": {
      fontFamily: "inherit",
    },
    ".cm-content": {
      caretColor: "var(--caret-color)",
    },
    ".cm-cursor, .cm-dropCursor": {
      borderLeftColor: "var(--caret-color)",
      borderLeftWidth: "2px",
    },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection":
      {
        backgroundColor: "var(--text-selection)",
      },
    ".cm-activeLine": { backgroundColor: "transparent" },
    ".cm-gutters": { display: "none" },
  },
  { dark: false },
);

// Inline tag highlighting. Block-level (line-wide) styling is handled
// by markdownRenderExtension since font-size on inline tags can't
// resize the line box.
export const forgeMarkdownHighlight = HighlightStyle.define([
  { tag: t.strong, fontWeight: "700", color: "var(--text-bold)" },
  { tag: t.emphasis, fontStyle: "italic" },
  {
    tag: t.strikethrough,
    textDecoration: "line-through",
    color: "var(--text-muted)",
  },
  {
    tag: [t.monospace, t.literal],
    fontFamily: "var(--font-monospace)",
    color: "var(--code-normal)",
    backgroundColor: "var(--code-background)",
  },
  { tag: t.link, color: "var(--text-link)", textDecoration: "underline" },
  { tag: t.url, color: "var(--text-faint)" },
  { tag: t.list, color: "var(--interactive-accent)" },
  {
    tag: [t.processingInstruction, t.meta],
    color: "var(--text-faint)",
  },
  { tag: t.heading, fontWeight: "700", color: "var(--text-title-h1)" },
  { tag: t.heading1, fontWeight: "700", color: "var(--text-title-h1)" },
  { tag: t.heading2, fontWeight: "700", color: "var(--text-title-h2)" },
  { tag: t.heading3, fontWeight: "700", color: "var(--text-title-h3)" },
  { tag: t.heading4, fontWeight: "700", color: "var(--text-title-h4)" },
  { tag: t.heading5, fontWeight: "600", color: "var(--text-title-h5)" },
  { tag: t.heading6, fontWeight: "600", color: "var(--text-title-h6)" },
]);

// Base extensions (no per-instance callback needed).
export const forgeMarkdownExtensions = [
  forgeTheme,
  syntaxHighlighting(forgeMarkdownHighlight),
  markdownRenderExtension,
];

/** Build the full extension set including click-to-open wiring. */
export function buildEditorExtensions(
  onOpenPath: (target: string) => void,
): Extension[] {
  return [
    ...forgeMarkdownExtensions,
    createWikilinkPlugin(onOpenPath),
    createHyperlinkClickHandler(onOpenPath),
  ];
}
