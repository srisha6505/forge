import { memo, useLayoutEffect, useMemo, useRef, useState } from "react";
import CodeMirror from "@uiw/react-codemirror";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { languages } from "@codemirror/language-data";
import { EditorView } from "@codemirror/view";
import { FileText } from "lucide-react";
import { buildEditorExtensions } from "../lib/cm-theme";

interface Props {
  path: string | null;
  title: string | null;
  content: string;
  readableWidth: boolean;
  onChange: (value: string) => void;
  onOpenPath: (target: string) => void;
}

function Editor({
  path,
  title,
  content,
  readableWidth,
  onChange,
  onOpenPath,
}: Props) {
  // Fresh-ref pattern: refs assigned during render so the CM6 extension
  // closures (built once via useMemo) always forward to the latest
  // callbacks without forcing a plugin rebuild.
  const onChangeRef = useRef(onChange);
  const onOpenRef = useRef(onOpenPath);
  onChangeRef.current = onChange;
  onOpenRef.current = onOpenPath;

  const stableOnChange = useMemo(
    () => (v: string) => onChangeRef.current(v),
    [],
  );
  const extensions = useMemo(
    () => [
      markdown({ base: markdownLanguage, codeLanguages: languages }),
      EditorView.lineWrapping,
      ...buildEditorExtensions((t) => onOpenRef.current(t)),
    ],
    [],
  );

  // Editor pixel height: measured from the actual scroll container so
  // we don't depend on a hardcoded chrome-height constant. ResizeObserver
  // catches window resizes, sidebar drags (no-op since width changes
  // don't change height), and devtools open/close.
  const scrollBoxRef = useRef<HTMLDivElement>(null);
  // Initial guess so CodeMirror renders on the first frame instead of
  // waiting for the post-mount measurement (which would flash blank).
  const [editorPx, setEditorPx] = useState(() =>
    Math.max(200, window.innerHeight - 160),
  );
  useLayoutEffect(() => {
    const el = scrollBoxRef.current;
    if (!el) return;
    let raf = 0;
    const measure = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => {
        const h = el.clientHeight;
        if (h > 0) setEditorPx((prev) => (prev === h ? prev : h));
      });
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => {
      ro.disconnect();
      cancelAnimationFrame(raf);
    };
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
      className={`markdown-source-view cm-s-obsidian workspace-leaf-content flex-1 min-h-0 min-w-0 flex flex-col bg-[var(--background-primary)] ${
        readableWidth ? "is-readable" : ""
      }`}
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
      <div ref={scrollBoxRef} className="flex-1 min-h-0 min-w-0">
        <CodeMirror
          value={content}
          onChange={stableOnChange}
          height={`${editorPx}px`}
          theme="none"
          basicSetup={{
            lineNumbers: false,
            foldGutter: false,
            highlightActiveLine: false,
            highlightActiveLineGutter: false,
            dropCursor: true,
          }}
          extensions={extensions}
        />
      </div>
    </div>
  );
}

export default memo(Editor);
