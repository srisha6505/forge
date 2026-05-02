import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
// Browser bundle of mammoth ships without its own .d.ts; the API
// surface we use (convertToHtml) matches the typed root entry, so we
// re-declare it here to avoid pulling in Node-only typings.
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore -- browser submodule has no bundled types
import mammoth from "mammoth/mammoth.browser";
import {
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  ExternalLink,
  FileText,
  Loader2,
  Maximize2,
  RefreshCw,
  ZoomIn,
  ZoomOut,
} from "lucide-react";
import { open as shellOpen } from "@tauri-apps/plugin-shell";
import { extOf, assetUrl } from "../lib/file-types";

interface Props {
  path: string;
  title: string | null;
}

type MammothMessage = { type: "warning" | "error"; message: string };
type MammothResult = { value: string; messages: MammothMessage[] };

type State =
  | { kind: "loading" }
  | { kind: "ready"; html: string; messages: MammothMessage[] }
  | { kind: "error"; error: string }
  | { kind: "unsupported"; ext: string };

// Map a few common Word styles into semantic HTML tags. mammoth ships a
// reasonable default map; this only adds class hooks so theme CSS can
// target the title block specifically.
const STYLE_MAP = [
  "p[style-name='Title'] => h1.docx-title:fresh",
  "p[style-name='Subtitle'] => h2.docx-subtitle:fresh",
];

function DocxViewer({ path, title }: Props) {
  const ext = useMemo(() => extOf(path), [path]);
  const isDocx = ext === "docx";
  const url = useMemo(() => assetUrl(path), [path]);

  const [state, setState] = useState<State>(() =>
    isDocx ? { kind: "loading" } : { kind: "unsupported", ext },
  );
  const [notesOpen, setNotesOpen] = useState(false);
  // Bumped to force a re-fetch on retry without changing `path`.
  const [reloadTick, setReloadTick] = useState(0);
  // Zoom + fit-to-width — same vocabulary as PdfViewer.
  const MIN_SCALE = 0.5;
  const MAX_SCALE = 2.5;
  const [scale, setScale] = useState(1);
  const [fitWidth, setFitWidth] = useState(false);
  const zoomIn = useCallback(
    () => setScale((s) => Math.min(MAX_SCALE, Math.round((s + 0.1) * 100) / 100)),
    [],
  );
  const zoomOut = useCallback(
    () => setScale((s) => Math.max(MIN_SCALE, Math.round((s - 0.1) * 100) / 100)),
    [],
  );
  const zoomReset = useCallback(() => setScale(1), []);
  const toggleFit = useCallback(() => setFitWidth((f) => !f), []);

  // Track the latest in-flight conversion so a stale promise resolving
  // after a path change cannot overwrite the current state.
  const reqIdRef = useRef(0);

  useEffect(() => {
    if (!isDocx) {
      setState({ kind: "unsupported", ext });
      return;
    }

    setState({ kind: "loading" });
    setNotesOpen(false);

    const reqId = ++reqIdRef.current;
    const ac = new AbortController();

    (async () => {
      try {
        const res = await fetch(url, { signal: ac.signal });
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const buf = await res.arrayBuffer();
        if (reqId !== reqIdRef.current) return;

        const out = (await mammoth.convertToHtml(
          { arrayBuffer: buf },
          { styleMap: STYLE_MAP },
        )) as MammothResult;
        if (reqId !== reqIdRef.current) return;

        setState({
          kind: "ready",
          html: out.value || "<p><em>(empty document)</em></p>",
          messages: out.messages ?? [],
        });
      } catch (e) {
        if (ac.signal.aborted) return;
        if (reqId !== reqIdRef.current) return;
        const msg = e instanceof Error ? e.message : String(e);
        setState({ kind: "error", error: msg });
      }
    })();

    return () => {
      ac.abort();
    };
  }, [path, url, isDocx, ext, reloadTick]);

  const onRetry = useCallback(() => setReloadTick((n) => n + 1), []);

  const onOpenExternally = useCallback(async () => {
    try {
      // shell.open accepts plain absolute paths; the OS resolves the
      // default handler (Word, LibreOffice, Pages, etc.).
      await shellOpen(path);
    } catch {
      // Fallback to the asset URL if direct path open is rejected.
      try {
        await shellOpen(url);
      } catch {
        // swallow: nothing actionable here
      }
    }
  }, [path, url]);

  // Memoise the rendered body so toolbar/notes-panel state changes do
  // not reparse the HTML string into DOM nodes. The content is wrapped
  // in a "page" — same visual contract as PdfViewer: a centred sheet
  // with a soft shadow, sitting on the slightly-darker stage.
  const renderedBody = useMemo(() => {
    if (state.kind !== "ready") return null;
    return (
      <div className="docx-page">
        <div
          className="docx-content"
          dangerouslySetInnerHTML={{ __html: state.html }}
        />
      </div>
    );
  }, [state]);

  const formatLabel = ext.toUpperCase() || "DOC";
  const warnCount =
    state.kind === "ready"
      ? state.messages.filter((m) => m.type !== "error").length
      : 0;
  const errCount =
    state.kind === "ready"
      ? state.messages.filter((m) => m.type === "error").length
      : 0;

  const btnBase =
    "h-7 px-2 inline-flex items-center gap-1.5 rounded-md text-[12px] text-[var(--text-muted)] hover:text-[var(--text-normal)] hover:bg-[var(--background-modifier-hover)] transition-colors";
  // Square 32×32 icon button — same sizing/colours as PdfViewer's btnBase
  // so the two viewers feel like the same surface.
  const iconBtn =
    "w-8 h-8 flex items-center justify-center rounded-md transition-colors text-[var(--icon-color)] hover:text-[var(--icon-color-hover)] hover:bg-[var(--background-modifier-hover)] disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-[var(--icon-color)]";
  const iconBtnActive =
    "w-8 h-8 flex items-center justify-center rounded-md transition-colors text-[var(--text-accent)] bg-[var(--background-modifier-active)]";

  return (
    <div
      className="workspace-leaf-content flex-1 min-h-0 min-w-0 flex flex-col bg-[var(--background-primary)]"
      style={
        {
          "--docx-zoom": scale,
          "--docx-page-width": fitWidth ? "none" : "820px",
        } as React.CSSProperties
      }
    >
      <style>{`
        /* Mirror PdfViewer's .pdf-viewer-stage / .react-pdf__Page recipe
           exactly: stage and page share the same primary background, the
           "page lifted off the stage" effect comes purely from the
           shadow. PdfViewer uses bg-primary throughout — copying that
           here means the visual contract matches even on themes where
           primary and secondary are nearly identical. */
        .docx-stage { overflow-anchor: none; }
        .docx-page {
          background: var(--background-primary);
          color: var(--text-normal);
          max-width: var(--docx-page-width, 820px);
          margin: 16px auto;
          padding: 72px 88px;
          box-shadow:
            0 1px 3px rgba(0,0,0,0.18),
            0 4px 12px rgba(0,0,0,0.12);
          /* Zoom is implemented as font-size scaling — content reflows
             naturally inside the page, no transform: scale clipping or
             scrollbar weirdness. PdfViewer scales the canvas; HTML
             content scales the type instead. */
          font-size: calc(var(--font-text-size, 16px) * var(--docx-zoom, 1));
          line-height: 1.6;
          font-family: var(--font-text, inherit);
        }
        .docx-content { color: var(--text-normal); }
        .docx-content h1.docx-title {
          font-size: 1.9em;
          font-weight: 700;
          color: var(--text-title-h1);
          border-bottom: 1px solid var(--background-modifier-border);
          padding-bottom: 0.25em;
          margin-top: 0;
        }
        .docx-content h2.docx-subtitle {
          font-size: 1.25em;
          font-weight: 500;
          color: var(--text-muted);
          margin-top: 0.25em;
        }
        .docx-content h1, .docx-content h2, .docx-content h3,
        .docx-content h4, .docx-content h5, .docx-content h6 {
          color: var(--text-title-h1);
          margin-top: 1.4em;
          margin-bottom: 0.5em;
          line-height: 1.25;
        }
        .docx-content p { margin: 0.6em 0; }
        .docx-content table {
          border-collapse: collapse;
          margin: 0.75em 0;
          width: 100%;
        }
        .docx-content table td, .docx-content table th {
          border: 1px solid var(--background-modifier-border);
          padding: 0.45em 0.7em;
        }
        .docx-content table th {
          background: var(--background-secondary);
          font-weight: 600;
        }
        .docx-content img { max-width: 100%; height: auto; }
        .docx-content a { color: var(--text-accent); }
        .docx-content ul, .docx-content ol { padding-left: 1.5em; margin: 0.5em 0; }
      `}</style>

      {/* Title banner — slim, mirrors PdfViewer. */}
      <div className="flex-shrink-0 pt-3 pb-2 px-8 w-full">
        <h1 className="text-[18px] font-semibold text-[var(--text-title-h1)] leading-tight tracking-tight truncate flex items-center gap-2">
          <FileText
            size={16}
            strokeWidth={1.8}
            className="text-[var(--text-faint)] flex-shrink-0"
          />
          <span className="truncate">{title ?? "Untitled"}</span>
        </h1>
      </div>

      {/* Toolbar — same chrome as PdfViewer: zoom, fit-width, format
          label on the left, conversion notes / open-externally on the
          right. h-10, border-y, secondary bg, identical button sizing. */}
      <div className="flex-shrink-0 flex items-center gap-1 px-3 h-10 border-y border-[var(--background-modifier-border)] bg-[var(--background-secondary)]">
        <button
          className={iconBtn}
          onClick={zoomOut}
          disabled={scale <= MIN_SCALE}
          title="Zoom out"
        >
          <ZoomOut size={16} strokeWidth={1.8} />
        </button>
        <button
          className={iconBtn}
          onClick={zoomReset}
          title="Reset zoom"
        >
          <span className="text-[11px] tabular-nums text-[var(--text-muted)]">
            {Math.round(scale * 100)}%
          </span>
        </button>
        <button
          className={iconBtn}
          onClick={zoomIn}
          disabled={scale >= MAX_SCALE}
          title="Zoom in"
        >
          <ZoomIn size={16} strokeWidth={1.8} />
        </button>
        <button
          className={fitWidth ? iconBtnActive : iconBtn}
          onClick={toggleFit}
          title="Fit to width"
        >
          <Maximize2 size={16} strokeWidth={1.8} />
        </button>

        <div className="w-px h-5 bg-[var(--background-modifier-border)] mx-2" />

        <span className="text-[11px] tabular-nums text-[var(--text-faint)] px-1">
          {formatLabel}
        </span>

        {state.kind === "ready" && (warnCount > 0 || errCount > 0) && (
          <button
            className="text-[11px] text-[var(--text-faint)] hover:text-[var(--text-muted)] px-1 inline-flex items-center gap-1"
            onClick={() => setNotesOpen((o) => !o)}
            title="Toggle conversion notes"
          >
            {notesOpen ? (
              <ChevronDown size={12} strokeWidth={1.8} />
            ) : (
              <ChevronRight size={12} strokeWidth={1.8} />
            )}
            <span>
              {warnCount + errCount} note{warnCount + errCount === 1 ? "" : "s"}
            </span>
          </button>
        )}

        <div className="ml-auto flex items-center gap-1">
          <button
            className={btnBase}
            onClick={onOpenExternally}
            title="Open in default application"
          >
            <ExternalLink size={14} strokeWidth={1.8} />
            <span>Open externally</span>
          </button>
        </div>
      </div>

      {/* Body — scroll stage that holds the page-style content. Same
          colour as outer container; the page is lifted off it visually
          via shadow only, identical to PdfViewer's contract. */}
      <div className="docx-stage flex-1 min-h-0 min-w-0 overflow-auto">
        {state.kind === "loading" && (
          <div className="h-full flex items-center justify-center text-[var(--text-faint)]">
            <Loader2 size={20} className="animate-spin" />
          </div>
        )}

        {state.kind === "error" && (
          <div className="h-full flex items-center justify-center px-6 text-center">
            <div>
              <div className="font-medium text-[var(--text-normal)] mb-1">
                Could not load document
              </div>
              <div className="text-[12px] text-[var(--text-faint)] mb-3 max-w-md mx-auto break-words">
                {state.error}
              </div>
              <div className="flex items-center justify-center gap-2">
                <button
                  className="h-8 px-3 inline-flex items-center gap-1.5 rounded-md text-[12px] text-[var(--text-normal)] bg-[var(--background-modifier-hover)] hover:bg-[var(--background-modifier-active)] transition-colors"
                  onClick={onRetry}
                >
                  <RefreshCw size={14} strokeWidth={1.8} />
                  <span>Retry</span>
                </button>
                <button
                  className="h-8 px-3 inline-flex items-center gap-1.5 rounded-md text-[12px] text-[var(--text-muted)] hover:text-[var(--text-normal)] hover:bg-[var(--background-modifier-hover)] transition-colors"
                  onClick={onOpenExternally}
                >
                  <ExternalLink size={14} strokeWidth={1.8} />
                  <span>Open externally</span>
                </button>
              </div>
            </div>
          </div>
        )}

        {state.kind === "unsupported" && (
          <div className="h-full flex items-center justify-center px-6 text-center">
            <div className="max-w-md">
              <div className="flex items-center justify-center mb-2 text-[var(--text-faint)]">
                <AlertTriangle size={20} strokeWidth={1.8} />
              </div>
              <div className="font-medium text-[var(--text-normal)] mb-1">
                .{state.ext} is not supported in-browser
              </div>
              <div className="text-[12px] text-[var(--text-faint)] mb-4">
                {state.ext === "doc"
                  ? "The legacy binary .doc format cannot be rendered here. Save the file as .docx in Word or LibreOffice to view it inline."
                  : "OpenDocument (.odt) is not yet rendered inline. Save as .docx in LibreOffice to view it here."}
              </div>
              <button
                className="h-8 px-3 inline-flex items-center gap-1.5 rounded-md text-[12px] text-[var(--text-normal)] bg-[var(--background-modifier-hover)] hover:bg-[var(--background-modifier-active)] transition-colors"
                onClick={onOpenExternally}
              >
                <ExternalLink size={14} strokeWidth={1.8} />
                <span>Open externally</span>
              </button>
            </div>
          </div>
        )}

        {state.kind === "ready" && (
          <>
            {notesOpen && (warnCount > 0 || errCount > 0) && (
              <div className="mx-auto max-w-[820px] mt-4 mx-4 rounded-md border border-[var(--background-modifier-border)] bg-[var(--background-primary)] text-[12px]">
                <ul className="px-4 py-2 space-y-0.5 text-[var(--text-faint)] max-h-40 overflow-auto">
                  {state.messages.map((m, i) => (
                    <li key={i} className="leading-snug">
                      <span
                        className={
                          m.type === "error"
                            ? "text-[var(--text-error)] mr-1"
                            : "text-[var(--text-warning,var(--text-faint))] mr-1"
                        }
                      >
                        [{m.type}]
                      </span>
                      {m.message}
                    </li>
                  ))}
                </ul>
              </div>
            )}
            {renderedBody}
          </>
        )}
      </div>
    </div>
  );
}

export default memo(DocxViewer);
