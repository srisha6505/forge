// Ctrl/Cmd+click on a markdown link opens the URL via Tauri's shell
// plugin (xdg-open on Linux, `open` on macOS, start on Windows).
// Relative paths / vault-local file refs are passed to onOpenPath so
// App.tsx can handle them as tab opens instead of external browser.

import { syntaxTree } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { open as shellOpen } from "@tauri-apps/plugin-shell";

function extractUrl(view: EditorView, pos: number): string | null {
  const tree = syntaxTree(view.state);
  let node: ReturnType<typeof tree.resolveInner> | null = tree.resolveInner(pos);
  while (node) {
    if (node.name === "URL") {
      return view.state.doc.sliceString(node.from, node.to);
    }
    if (node.name === "Link" || node.name === "Image") {
      // Walk children to find URL
      const cur = node.cursor();
      if (cur.firstChild()) {
        do {
          if (cur.name === "URL") {
            return view.state.doc.sliceString(cur.from, cur.to);
          }
        } while (cur.nextSibling());
      }
      break;
    }
    node = node.parent as typeof node;
  }
  return null;
}

function looksExternal(url: string): boolean {
  return /^(https?:|mailto:|ftp:|file:)/i.test(url);
}

export function createHyperlinkClickHandler(
  onOpenPath: (target: string) => void,
) {
  return EditorView.domEventHandlers({
    mousedown(event, view) {
      const el = event.target as HTMLElement | null;
      // Markdown link inside a rendered table cell: plain click opens it.
      if (el) {
        const mdLink = el.closest(".cm-md-link") as HTMLElement | null;
        if (mdLink) {
          const url = mdLink.dataset.url;
          if (url) {
            event.preventDefault();
            event.stopPropagation();
            if (looksExternal(url)) {
              shellOpen(url).catch((e) =>
                console.error("shell open failed", e),
              );
            } else {
              onOpenPath(url);
            }
            return true;
          }
        }
      }

      // Editor text: require Ctrl/Cmd + click for ambiguity with caret placement
      if (!(event.ctrlKey || event.metaKey)) return false;
      if (event.button !== 0) return false;
      const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
      if (pos == null) return false;
      const url = extractUrl(view, pos);
      if (!url) return false;
      event.preventDefault();
      event.stopPropagation();
      if (looksExternal(url)) {
        shellOpen(url).catch((e) => console.error("shell open failed", e));
      } else {
        onOpenPath(url);
      }
      return true;
    },
  });
}
