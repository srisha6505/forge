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
  RefreshCw,
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
  // not reparse the HTML string into DOM nodes.
  const renderedBody = useMemo(() => {
    if (state.kind !== "ready") return null;
    return (
      <div
        className="markdown-preview-view is-readable"
        style={{ height: "100%", overflow: "auto" }}
      >
        <div className="markdown-preview-sizer">
          <div
            className="docx-content"
            dangerouslySetInnerHTML={{ __html: state.html }}
          />
        </div>
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

  return (
    <div className="workspace-leaf-content flex-1 min-h-0 min-w-0 flex flex-col bg-[var(--background-primary)]">
      <style>{`
        .docx-content { font-size: var(--font-text-size, 16px); }
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
        .docx-content table {
          border-collapse: collapse;
          margin: 0.75em 0;
        }
        .docx-content table td, .docx-content table th {
          border: 1px solid var(--background-modifier-border);
          padding: 0.35em 0.6em;
        }
        .docx-content img { max-width: 100%; height: auto; }
      `}</style>

      {/* Title banner — mirrors Editor.tsx header spacing. */}
      <div className="flex-shrink-0 pt-6 pb-2 px-16 w-full">
        <h1 className="text-[22px] font-semibold text-[var(--text-title-h1)] leading-tight tracking-tight truncate flex items-center gap-2">
          <FileText
            size={18}
            strokeWidth={1.8}
            className="text-[var(--text-faint)] flex-shrink-0"
          />
          <span className="truncate">{title ?? "Untitled"}</span>
        </h1>
      </div>

      {/* Toolbar */}
      <div className="flex-shrink-0 flex items-center gap-1 px-3 h-10 border-y border-[var(--background-modifier-border)] bg-[var(--background-secondary)]">
        <span className="text-[11px] tabular-nums text-[var(--text-faint)] px-1">
          {formatLabel}
        </span>
        {state.kind === "ready" && (warnCount > 0 || errCount > 0) && (
          <span className="text-[11px] text-[var(--text-faint)] px-1">
            · {warnCount + errCount} note{warnCount + errCount === 1 ? "" : "s"}
          </span>
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

      {/* Body */}
      <div className="flex-1 min-h-0 min-w-0 flex flex-col">
        {state.kind === "loading" && (
          <div className="flex-1 flex items-center justify-center text-[var(--text-faint)]">
            <Loader2 size={20} className="animate-spin" />
          </div>
        )}

        {state.kind === "error" && (
          <div className="flex-1 flex items-center justify-center px-6 text-center">
            <div>
              <div className="font-medium text-[var(--text-normal)] mb-1">
                Could not load document
              </div>
              <div className="text-[12px] text-[var(--text-faint)] mb-3 max-w-md mx-auto break-words">
                {state.error}
              </div>
              <button
                className="h-8 px-3 inline-flex items-center gap-1.5 rounded-md text-[12px] text-[var(--text-normal)] bg-[var(--background-modifier-hover)] hover:bg-[var(--background-modifier-active)] transition-colors"
                onClick={onRetry}
              >
                <RefreshCw size={14} strokeWidth={1.8} />
                <span>Retry</span>
              </button>
            </div>
          </div>
        )}

        {state.kind === "unsupported" && (
          <div className="flex-1 flex items-center justify-center px-6 text-center">
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
            {(warnCount > 0 || errCount > 0) && (
              <div className="flex-shrink-0 mx-16 mt-3 rounded-md border border-[var(--background-modifier-border)] bg-[var(--background-secondary)] text-[12px]">
                <button
                  className="w-full flex items-center gap-1.5 px-3 py-1.5 text-[var(--text-muted)] hover:text-[var(--text-normal)]"
                  onClick={() => setNotesOpen((o) => !o)}
                >
                  {notesOpen ? (
                    <ChevronDown size={14} strokeWidth={1.8} />
                  ) : (
                    <ChevronRight size={14} strokeWidth={1.8} />
                  )}
                  <span>
                    Conversion notes ({warnCount + errCount})
                  </span>
                </button>
                {notesOpen && (
                  <ul className="px-4 pb-2 pt-0.5 space-y-0.5 text-[var(--text-faint)] max-h-40 overflow-auto">
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
                )}
              </div>
            )}
            <div className="flex-1 min-h-0 min-w-0">{renderedBody}</div>
          </>
        )}
      </div>
    </div>
  );
}

export default memo(DocxViewer);
