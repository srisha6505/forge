// LaTeX viewer. Renders the compiled PDF for a `.tex` file by delegating
// to PdfViewer for chrome + layout. This component owns only the
// compile lifecycle (first-open auto-compile, manual Recompile, error
// display) and injects LaTeX-specific buttons into PdfViewer's toolbar
// via its `extraToolbar` slot.
//
// Editing is explicitly out of scope. "Edit source" opens the `.tex`
// in the OS-default plain-text editor via the Rust
// `open_in_text_editor` command (TextEdit on macOS, Notepad on
// Windows, a common GUI editor on Linux). The AI agent can also edit
// via its `edit_file` tool. After any external edit, the user hits
// Recompile to re-render.

import { memo, useCallback, useEffect, useRef, useState } from "react";
import {
  AlertCircle,
  CheckCircle2,
  Download,
  FileText,
  Loader,
  RefreshCw,
} from "lucide-react";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { copyFile } from "@tauri-apps/plugin-fs";
import {
  compileLatex,
  latexStatus,
  openInTextEditor,
  type LatexCompileResult,
  type LatexStatus,
} from "../lib/tauri";
import PdfViewer from "./PdfViewer";
import ErrorBoundary from "./ErrorBoundary";

interface Props {
  path: string;
  title: string | null;
}

type CompileState = "idle" | "compiling" | "ok" | "error";

const LOG_PREVIEW_LINES = 30;

function LatexViewer({ path, title }: Props) {
  const [state, setState] = useState<CompileState>("idle");
  const [result, setResult] = useState<LatexCompileResult | null>(null);
  // Bumped on every successful compile so PdfViewer remounts and pdfjs
  // re-fetches the file. The asset URL is identical across rebuilds
  // (pdf_path is stable); without a key change pdfjs serves the cached
  // previous PDF and edits look skipped.
  const [compileTick, setCompileTick] = useState(0);
  const [errorLog, setErrorLog] = useState<string | null>(null);
  const [engineStatus, setEngineStatus] = useState<LatexStatus | null>(null);
  const [showFullLog, setShowFullLog] = useState(false);

  useEffect(() => {
    let cancelled = false;
    latexStatus()
      .then((s) => {
        if (!cancelled) setEngineStatus(s);
      })
      .catch(() => {
        if (!cancelled)
          setEngineStatus({ tectonic: false, xelatex: false, pdflatex: false });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const inflightRef = useRef(false);
  const runCompile = useCallback(async () => {
    if (inflightRef.current) return;
    inflightRef.current = true;
    setState("compiling");
    setErrorLog(null);
    try {
      const r = await compileLatex(path);
      setResult(r);
      setCompileTick((n) => n + 1);
      setState("ok");
    } catch (e: unknown) {
      const msg = typeof e === "string" ? e : (e as Error)?.message ?? String(e);
      setErrorLog(msg);
      setState("error");
    } finally {
      inflightRef.current = false;
    }
  }, [path]);

  // Compile once per path on open. After that, manual only — user
  // edits the source elsewhere, hits Recompile to re-render.
  const firstRunPathRef = useRef<string | null>(null);
  useEffect(() => {
    if (firstRunPathRef.current === path) return;
    if (engineStatus === null) return;
    const noEngine =
      !engineStatus.tectonic && !engineStatus.xelatex && !engineStatus.pdflatex;
    if (noEngine) return;
    firstRunPathRef.current = path;
    void runCompile();
  }, [path, engineStatus, runCompile]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      if (!mod) return;
      if (e.key === "Enter" || e.key.toLowerCase() === "r") {
        e.preventDefault();
        void runCompile();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [runCompile]);

  const editSource = useCallback(async () => {
    try {
      await openInTextEditor(path);
    } catch (e) {
      console.error("open-in-text-editor failed", e);
    }
  }, [path]);

  const exportPdf = useCallback(async () => {
    if (!result?.pdf_path) return;
    const suggested = (title ?? "document").replace(/\.[^./]+$/, "") + ".pdf";
    const dest = await saveDialog({
      defaultPath: suggested,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof dest !== "string") return;
    try {
      await copyFile(result.pdf_path, dest);
    } catch (e) {
      console.error("export failed", e);
    }
  }, [result, title]);

  const noEngine =
    engineStatus !== null &&
    !engineStatus.tectonic &&
    !engineStatus.xelatex &&
    !engineStatus.pdflatex;

  // Compact toolbar button pair. Matches the existing PdfViewer toolbar
  // aesthetic (h-8, ghost-ish) so the injected buttons feel native.
  const tbBtn =
    "h-8 px-2.5 inline-flex items-center gap-1.5 rounded-md text-[12px] font-medium transition-colors text-[var(--text-normal)] hover:bg-[var(--background-modifier-hover)] disabled:opacity-40 disabled:cursor-default disabled:hover:bg-transparent";
  const tbBtnAccent =
    "h-8 px-2.5 inline-flex items-center gap-1.5 rounded-md text-[12px] font-medium transition-colors text-[var(--text-on-accent)] bg-[var(--interactive-accent)] hover:bg-[var(--interactive-accent-hover)] disabled:opacity-40 disabled:cursor-default";

  const extraToolbar = (
    <>
      <button
        onClick={() => void runCompile()}
        disabled={state === "compiling" || noEngine}
        className={tbBtnAccent}
        title="Recompile (Ctrl+R or Ctrl+Enter)"
      >
        {state === "compiling" ? (
          <Loader size={13} strokeWidth={1.8} className="animate-spin" />
        ) : (
          <RefreshCw size={13} strokeWidth={1.8} />
        )}
        {state === "compiling" ? "Compiling" : "Recompile"}
      </button>
      <button
        onClick={editSource}
        className={tbBtn}
        title="Open source in OS default text editor"
      >
        <FileText size={13} strokeWidth={1.8} />
        Edit source
      </button>
      <button
        onClick={() => void exportPdf()}
        disabled={!result?.pdf_path}
        className={tbBtn}
        title="Save compiled PDF…"
      >
        <Download size={13} strokeWidth={1.8} />
        Export PDF
      </button>
      {state === "ok" && (
        <span className="inline-flex items-center gap-1 text-[11px] text-[var(--text-success)] ml-1">
          <CheckCircle2 size={11} strokeWidth={1.8} />
          Compiled
        </span>
      )}
      {state === "error" && (
        <span className="inline-flex items-center gap-1 text-[11px] text-[var(--text-error)] ml-1">
          <AlertCircle size={11} strokeWidth={1.8} />
          Error
        </span>
      )}
    </>
  );

  // Clamp the error log preview. LaTeX logs run into thousands of lines
  // and diagnostics tend to be near the tail.
  const logLines = errorLog ? errorLog.split("\n") : [];
  const logPreview =
    showFullLog || logLines.length <= LOG_PREVIEW_LINES
      ? errorLog ?? ""
      : logLines.slice(-LOG_PREVIEW_LINES).join("\n");

  if (noEngine) {
    return (
      <div className="workspace-leaf-content flex-1 min-h-0 min-w-0 flex flex-col bg-[var(--background-primary)]">
        <div className="flex-1 flex items-center justify-center p-8">
          <div className="max-w-[480px] text-center">
            <AlertCircle
              size={32}
              strokeWidth={1.5}
              className="text-[var(--text-error)] mx-auto mb-3"
            />
            <div className="text-[14px] font-semibold text-[var(--text-normal)] mb-2">
              No LaTeX engine installed
            </div>
            <div className="text-[12px] text-[var(--text-muted)] leading-[1.6]">
              Install tectonic (recommended) or a TeX distribution
              (MacTeX / TeX Live / MiKTeX), then reopen this file.
            </div>
          </div>
        </div>
      </div>
    );
  }

  if (state === "error" && errorLog) {
    return (
      <div className="workspace-leaf-content flex-1 min-h-0 min-w-0 flex flex-col bg-[var(--background-primary)]">
        <div className="flex-shrink-0 flex items-center gap-1 px-3 h-10 border-b border-[var(--background-modifier-border)] bg-[var(--background-secondary)]">
          <div className="ml-auto flex items-center gap-2">{extraToolbar}</div>
        </div>
        <div className="flex-1 min-h-0 overflow-auto p-6">
          <div className="max-w-[900px] mx-auto">
            <div className="flex items-center justify-between mb-2">
              <span className="text-[13px] font-semibold text-[var(--text-error)] inline-flex items-center gap-1.5">
                <AlertCircle size={14} strokeWidth={1.8} />
                Compile failed
              </span>
              {logLines.length > LOG_PREVIEW_LINES && (
                <button
                  onClick={() => setShowFullLog((v) => !v)}
                  className="text-[11px] text-[var(--text-muted)] hover:text-[var(--text-normal)] underline"
                >
                  {showFullLog
                    ? "Show last 30 lines"
                    : `Show all (${logLines.length} lines)`}
                </button>
              )}
            </div>
            <pre className="text-[11px] leading-[1.5] font-mono text-[var(--text-muted)] whitespace-pre-wrap break-words bg-[var(--background-modifier-error)] p-4 rounded-md">
              {logPreview}
            </pre>
          </div>
        </div>
      </div>
    );
  }

  if (!result?.pdf_path) {
    // First-open compile in flight, or idle before first compile.
    return (
      <div className="workspace-leaf-content flex-1 min-h-0 min-w-0 flex flex-col bg-[var(--background-primary)]">
        <div className="flex-shrink-0 flex items-center gap-1 px-3 h-10 border-b border-[var(--background-modifier-border)] bg-[var(--background-secondary)]">
          <div className="ml-auto flex items-center gap-2">{extraToolbar}</div>
        </div>
        <div className="flex-1 flex items-center justify-center gap-2 text-[var(--text-muted)] text-[13px]">
          {state === "compiling" ? (
            <>
              <Loader size={16} strokeWidth={1.8} className="animate-spin" />
              Compiling…
            </>
          ) : (
            <span>Press Recompile to render the PDF.</span>
          )}
        </div>
      </div>
    );
  }

  // Happy path: PdfViewer owns the full surface. Title is the .tex
  // filename. Our buttons live in PdfViewer's extra-toolbar slot, so
  // there is exactly one title + one toolbar visible.
  return (
    <ErrorBoundary>
      <PdfViewer
        key={compileTick}
        path={result.pdf_path}
        title={title}
        extraToolbar={extraToolbar}
      />
    </ErrorBoundary>
  );
}

export default memo(LatexViewer);
