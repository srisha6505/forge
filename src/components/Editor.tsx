import { memo, useCallback, useMemo, useRef } from "react";
import CodeMirror from "@uiw/react-codemirror";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { languages } from "@codemirror/language-data";
import { EditorView, drawSelection } from "@codemirror/view";
import { EditorSelection } from "@codemirror/state";
import { FileText } from "lucide-react";
import { buildEditorExtensions } from "../lib/cm-theme";
import TOCPanel, { type Heading } from "./TOCPanel";
import BacklinksPanel from "./BacklinksPanel";

interface Props {
  path: string | null;
  title: string | null;
  content: string;
  readableWidth: boolean;
  fontScale?: number;
  tocOpen?: boolean;
  // When true, CodeMirror runs in a non-editable pose: no typing, no
  // caret. All decorations (wikilinks, math, tables, live-preview mark
  // hides) still render, so read mode is visually identical to edit
  // mode minus the caret. Per mdeditor.md §1: "one render pipeline,
  // two poses of the same view."
  readOnly?: boolean;
  onChange: (value: string) => void;
  onOpenPath: (target: string) => void;
  // Optional bridge for callers (App.tsx) that need a handle to the live
  // EditorView — used by the universal dictation flow to insert text at
  // the caret. Receives null when the editor unmounts.
  onEditorMount?: (view: EditorView | null) => void;
  // Synchronous resolver for `[short-ref]` wikilink-style targets — used
  // by cm-wikilinks to gate which short refs become clickable. Returns
  // the resolved file path on hit, null on miss.
  resolveTarget?: (target: string) => string | null;
}

// Extract h1-h6 headings from raw markdown via a line-scan regex. Not
// syntax-tree accurate (e.g. fenced code blocks could smuggle a # into
// the wrong role) but matches the "good enough for a TOC" bar, and
// avoids reaching into CodeMirror's tree for v1. Re-runs on every
// content change since React hands us the fresh value each render.
function extractHeadings(src: string): Heading[] {
  const out: Heading[] = [];
  const lines = src.split("\n");
  let inFence = false;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (/^\s*```/.test(line)) {
      inFence = !inFence;
      continue;
    }
    if (inFence) continue;
    const m = /^(#{1,6})\s+(.+?)\s*#*\s*$/.exec(line);
    if (!m) continue;
    const level = m[1].length as 1 | 2 | 3 | 4 | 5 | 6;
    out.push({ level, text: m[2], lineNumber: i + 1 });
  }
  return out;
}

function Editor({
  path,
  title,
  content,
  readableWidth,
  fontScale = 1,
  tocOpen = false,
  readOnly = false,
  onChange,
  onOpenPath,
  onEditorMount,
  resolveTarget,
}: Props) {
  // Fresh-ref pattern: refs assigned during render so the CM6 extension
  // closures (built once via useMemo) always forward to the latest
  // callbacks without forcing a plugin rebuild.
  const onChangeRef = useRef(onChange);
  const onOpenRef = useRef(onOpenPath);
  const pathRef = useRef(path);
  const viewRef = useRef<EditorView | null>(null);
  const resolveRef = useRef<((t: string) => string | null) | null>(
    resolveTarget ?? null,
  );
  onChangeRef.current = onChange;
  onOpenRef.current = onOpenPath;
  pathRef.current = path;
  resolveRef.current = resolveTarget ?? null;

  const stableOnChange = useMemo(
    () => (v: string) => onChangeRef.current(v),
    [],
  );
  const extensions = useMemo(
    () => [
      markdown({ base: markdownLanguage, codeLanguages: languages }),
      EditorView.lineWrapping,
      // cursorBlinkRate: 0 disables CM6's JS-driven blink (which sets
      // inline opacity on .cm-cursor and races with our CSS @keyframes).
      // Once disabled, the CSS animation in index.css owns the blink at
      // a fixed 1050ms cadence.
      drawSelection({ cursorBlinkRate: 0 }),
      // Disable browser spellcheck/autocorrect/autocapitalize on the
      // contentDOM. Webview spellcheck reflows on every doc change and
      // costs 5-15 ms per keystroke on long documents. We have no UI
      // for spellcheck anyway, and prose users want this off in code-
      // adjacent contexts (wikilinks, math, code fences).
      EditorView.contentAttributes.of({
        spellcheck: "false",
        autocorrect: "off",
        autocapitalize: "off",
      }),
      ...buildEditorExtensions(
        (t) => onOpenRef.current(t),
        () => pathRef.current,
        () => resolveRef.current,
      ),
    ],
    [],
  );

  const headings = useMemo(
    () => (tocOpen ? extractHeadings(content) : []),
    [tocOpen, content],
  );

  // Click-to-scroll wiring for the TOC. We dispatch a caret move + CM
  // scrollIntoView; CM measures and scrolls the .cm-scroller. The OUTER
  // markdown-scroller is a separate element though — CM only knows
  // about its own scroller — so as a belt-and-braces fallback we also
  // ask the heading line's DOM node to scrollIntoView, which walks
  // every ancestor scroller including ours.
  const handleHeadingClick = useCallback((h: Heading) => {
    const view = viewRef.current;
    if (!view || !h.lineNumber) return;
    const line = view.state.doc.line(
      Math.max(1, Math.min(h.lineNumber, view.state.doc.lines)),
    );
    view.dispatch({
      selection: EditorSelection.cursor(line.from),
      effects: EditorView.scrollIntoView(line.from, {
        y: "start",
        yMargin: 24,
      }),
    });
    view.focus();
    // The outer .markdown-scroller wraps CM and also holds the title
    // strip + backlinks. CM's scrollIntoView only touches its own
    // .cm-scroller; ask the line's DOM to scroll itself into view so
    // the outer scroller catches up.
    requestAnimationFrame(() => {
      const lineEl = view.domAtPos(line.from).node as HTMLElement | null;
      lineEl?.scrollIntoView?.({ block: "start", behavior: "auto" });
    });
  }, []);

  if (!path) {
    return (
      <div className="workspace-leaf-content flex-1 flex flex-col items-center justify-center gap-3 text-[var(--text-faint)]">
        <FileText size={40} strokeWidth={1.3} />
        <div className="text-[13px] font-medium">No file open</div>
        <div className="text-[11px] text-[var(--text-faint)]">
          Pick a note from the sidebar to start editing
        </div>
      </div>
    );
  }

  return (
    <div
      className={`markdown-source-view cm-s-obsidian workspace-leaf-content flex-1 min-h-0 min-w-0 flex overflow-hidden bg-[var(--background-primary)] ${
        readableWidth ? "is-readable" : ""
      } ${readOnly ? "is-read-pose" : ""}`}
      // Picked up by the .cm-editor rule as `font-size: calc(var(
      // --font-text-size) * var(--md-zoom, 1))`. Avoids self-referencing
      // --font-text-size and respects the user's configured base size.
      style={{ "--md-zoom": fontScale } as React.CSSProperties}
    >
      {tocOpen && (
        <TOCPanel
          title={title ?? ""}
          headings={headings}
          onHeadingClick={handleHeadingClick}
        />
      )}
      <div className="flex-1 min-h-0 min-w-0 flex flex-col overflow-hidden">
        {/* The single scroll container: title strip, CodeMirror, and the
            inline Backlinks block all live inside it so they scroll as
            one document. CodeMirror itself gets height:100% and the
            outer flex does the math — no pixel measurement needed. */}
        <div
          className="flex-1 overflow-y-auto min-h-0 markdown-scroller"
          // Disabling scroll anchoring is critical for fast scrolling
          // through long notes: when math / image / table widgets
          // settle their final size after first paint, the browser's
          // automatic anchor compensation jumps the scroll position,
          // which the user perceives as "distortion". Decoration
          // widget sizes are deterministic (CSS-bounded) so we don't
          // need anchoring to keep position stable.
          style={{ overflowAnchor: "none" }}
        >
          <div
            className={`flex-shrink-0 pt-10 pb-2 px-16 w-full ${
              readableWidth ? "max-w-[820px] mx-auto" : ""
            }`}
          >
            <h1 className="text-[34px] font-bold text-[var(--text-title-h1)] leading-[1.15] tracking-tight truncate">
              {title}
            </h1>
          </div>
          <CodeMirror
            value={content}
            onChange={stableOnChange}
            editable={!readOnly}
            readOnly={readOnly}
            height="auto"
            theme="none"
            onCreateEditor={(view) => {
              viewRef.current = view;
              onEditorMount?.(view);
            }}
            basicSetup={{
              lineNumbers: false,
              foldGutter: false,
              highlightActiveLine: false,
              highlightActiveLineGutter: false,
              dropCursor: !readOnly,
              // We register our own drawSelection above with
              // cursorBlinkRate: 0 to give the CSS animation full control
              // of the blink. Two drawSelection extensions would draw two
              // cursors.
              drawSelection: false,
            }}
            extensions={extensions}
          />
          <div
            className={`px-16 pb-20 ${
              readableWidth ? "max-w-[820px] mx-auto" : ""
            }`}
          >
            <BacklinksPanel path={path} onOpen={onOpenPath} />
          </div>
        </div>
      </div>
    </div>
  );
}

export default memo(Editor);
