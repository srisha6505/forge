import {
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Document, Page, pdfjs } from "react-pdf";
import "react-pdf/dist/Page/AnnotationLayer.css";
import "react-pdf/dist/Page/TextLayer.css";
import {
  ChevronLeft,
  ChevronRight,
  Loader2,
  Maximize2,
  ScrollText,
  Type,
  ZoomIn,
  ZoomOut,
} from "lucide-react";
import { assetUrl } from "../lib/file-types";

// pdfjs worker version pinning. `vite.config.ts` aliases `pdfjs-dist`
// to react-pdf's nested copy, so `pdfjs.version` reflects the exact
// version react-pdf's Document expects. Fetching the matching worker
// from unpkg is simpler than self-hosting (the ?url asset import
// conflicts with Vite's dep optimizer on bare specifiers).
pdfjs.GlobalWorkerOptions.workerSrc = `https://unpkg.com/pdfjs-dist@${pdfjs.version}/build/pdf.worker.min.mjs`;

interface Props {
  path: string;
  title: string | null;
  // Optional extra toolbar content. Rendered on the right side of the
  // toolbar, before the mode label. Used by LatexViewer to inject
  // Recompile / Edit source / Export PDF buttons so the LaTeX surface
  // reuses PdfViewer's chrome instead of stacking a second toolbar.
  extraToolbar?: React.ReactNode;
  // When true, suppress the top title banner. Embedders that already
  // show their own title (LatexViewer) set this to avoid duplication.
  hideTitle?: boolean;
}

const MIN_SCALE = 0.25;
const MAX_SCALE = 3;
const SCALE_STEP = 0.1;
const PAGE_GAP = 16;
const HORIZONTAL_PADDING = 48;
// IntersectionObserver rootMargin for continuous-mode virtualisation.
// Pages enter the "rendered" set this many px before they scroll into
// view, giving pdfjs time to rasterise. Once in, they stay until they
// scroll this far past the opposite edge — so quick back-and-forth
// scrolling doesn't thrash mount/unmount.
const VIRT_ROOT_MARGIN_PX = 1200;
// Cap canvas rendering resolution. Retina / 4K screens report DPR 2-3,
// which means pdf.js rasterises each page at 4-9× the CSS pixel area.
// Cap at 1.5 for a solid quality/perf tradeoff; users needing crisper
// output can zoom instead.
const MAX_DEVICE_PIXEL_RATIO = 1.5;

type Mode = "single" | "continuous";

function clampScale(s: number): number {
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, s));
}

function PdfViewer({ path, title, extraToolbar, hideTitle }: Props) {
  const url = useMemo(() => assetUrl(path), [path]);
  // Memoised options object: react-pdf compares by reference; a fresh
  // object on every render triggers a full document refetch.
  const docOptions = useMemo(() => ({}), []);

  const [numPages, setNumPages] = useState(0);
  const [currentPage, setCurrentPage] = useState(1);
  const [pageInput, setPageInput] = useState("1");
  const [scale, setScale] = useState(1);
  const [fitWidth, setFitWidth] = useState(false);
  const [mode, setMode] = useState<Mode>("single");
  const [error, setError] = useState<string | null>(null);
  // Text-layer / annotation-layer toggle. OFF by default — these are
  // expensive (each glyph becomes a DOM node) and most readers don't
  // need text selection. Toggle on via the "Type" button in the toolbar.
  const [textLayerOn, setTextLayerOn] = useState(false);
  // Native (scale=1) page width/height in CSS pixels, captured from the
  // first rendered page. Used for fit-to-width and for placeholder
  // sizing in virtualised continuous mode.
  const [basePageWidth, setBasePageWidth] = useState<number | null>(null);
  const [basePageHeight, setBasePageHeight] = useState<number | null>(null);

  const scrollRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const pageRefs = useRef<Map<number, HTMLDivElement>>(new Map());
  // Pages currently marked as "render as real Page component" by the
  // IntersectionObserver. The observer uses a generous rootMargin so
  // pages enter this set before they scroll into view and linger after
  // they leave — scrolling fast through a doc does not cause constant
  // mount/unmount churn.
  const [visiblePages, setVisiblePages] = useState<Set<number>>(
    () => new Set([1]),
  );

  const onDocumentLoad = useCallback(
    ({ numPages: n }: { numPages: number }) => {
      setNumPages(n);
      setCurrentPage(1);
      setPageInput("1");
      setError(null);
    },
    [],
  );

  const onDocumentError = useCallback((e: Error) => {
    setError(e.message || "Failed to load PDF");
  }, []);

  // Reset transient state when the file changes.
  useEffect(() => {
    setNumPages(0);
    setCurrentPage(1);
    setPageInput("1");
    setError(null);
    setBasePageWidth(null);
    setBasePageHeight(null);
    setVisiblePages(new Set([1]));
  }, [path]);

  useEffect(() => {
    setPageInput(String(currentPage));
  }, [currentPage]);

  const goToPage = useCallback(
    (n: number) => {
      if (numPages === 0) return;
      const clamped = Math.min(numPages, Math.max(1, n));
      setCurrentPage(clamped);
      if (mode === "continuous") {
        const el = pageRefs.current.get(clamped);
        if (el && scrollRef.current) {
          // Instant jump, not smooth. Smooth scroll fires many scroll
          // events in quick succession, which compounds with pdfjs
          // rendering work triggered as pages intersect — feels laggy.
          scrollRef.current.scrollTop = el.offsetTop - 8;
        }
      } else if (scrollRef.current) {
        scrollRef.current.scrollTop = 0;
      }
    },
    [numPages, mode],
  );

  // Fit-to-width: derive scale from container width vs base page width.
  const recomputeFit = useCallback(() => {
    if (!fitWidth || !basePageWidth || !containerRef.current) return;
    const w = containerRef.current.clientWidth - HORIZONTAL_PADDING;
    if (w <= 0) return;
    setScale(clampScale(w / basePageWidth));
  }, [fitWidth, basePageWidth]);

  useLayoutEffect(() => {
    recomputeFit();
  }, [recomputeFit]);

  useEffect(() => {
    if (!containerRef.current) return;
    const ro = new ResizeObserver(() => recomputeFit());
    ro.observe(containerRef.current);
    return () => ro.disconnect();
  }, [recomputeFit]);

  const onFirstPageRender = useCallback(
    (page: { originalWidth: number; originalHeight: number; width: number }) => {
      // page.originalWidth/Height are unscaled CSS px at scale=1
      if (basePageWidth == null) setBasePageWidth(page.originalWidth);
      if (basePageHeight == null) setBasePageHeight(page.originalHeight);
    },
    [basePageWidth, basePageHeight],
  );

  // Continuous-mode virtualisation via IntersectionObserver. One
  // observer watches every page wrapper; as pages intersect the
  // viewport (+ rootMargin buffer) they join `visiblePages` and render
  // as real Page components. Pages that scroll past the buffer drop out.
  //
  // currentPage (for the toolbar indicator) = smallest visible page.
  // Derived from the same observer state, no separate scroll scan.
  //
  // Re-runs whenever numPages or mode changes; observers are recreated
  // so refs (populated as page wrappers mount) get freshly observed.
  useEffect(() => {
    if (mode !== "continuous" || !scrollRef.current || numPages === 0) return;
    const sc = scrollRef.current;
    const io = new IntersectionObserver(
      (entries) => {
        setVisiblePages((prev) => {
          const next = new Set(prev);
          let changed = false;
          for (const entry of entries) {
            const n = Number(
              (entry.target as HTMLElement).dataset.page ?? 0,
            );
            if (!n) continue;
            if (entry.isIntersecting) {
              if (!next.has(n)) {
                next.add(n);
                changed = true;
              }
            } else {
              if (next.has(n)) {
                next.delete(n);
                changed = true;
              }
            }
          }
          return changed ? next : prev;
        });
      },
      {
        root: sc,
        rootMargin: `${VIRT_ROOT_MARGIN_PX}px 0px`,
        threshold: 0,
      },
    );
    // Observe every page wrapper that's currently mounted. New wrappers
    // added later get picked up by the effect re-run on `numPages`
    // change (because the Array.from render depends on numPages).
    pageRefs.current.forEach((el) => io.observe(el));
    return () => io.disconnect();
  }, [mode, numPages]);

  // Toolbar page indicator: smallest visible page. Recomputed via a
  // cheap memo of the visible set.
  useEffect(() => {
    if (mode !== "continuous" || visiblePages.size === 0) return;
    let min = Infinity;
    visiblePages.forEach((n) => {
      if (n < min) min = n;
    });
    if (Number.isFinite(min)) {
      setCurrentPage((p) => (p === min ? p : (min as number)));
    }
  }, [mode, visiblePages]);

  const zoomIn = useCallback(() => {
    setFitWidth(false);
    setScale((s) => clampScale(s + SCALE_STEP));
  }, []);
  const zoomOut = useCallback(() => {
    setFitWidth(false);
    setScale((s) => clampScale(s - SCALE_STEP));
  }, []);
  const zoomReset = useCallback(() => {
    setFitWidth(false);
    setScale(1);
  }, []);
  const toggleFit = useCallback(() => {
    setFitWidth((f) => !f);
  }, []);
  const toggleMode = useCallback(() => {
    setMode((m) => (m === "single" ? "continuous" : "single"));
  }, []);

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      switch (e.key) {
        case "ArrowLeft":
        case "PageUp":
          if (mode === "single") {
            e.preventDefault();
            goToPage(currentPage - 1);
          }
          break;
        case "ArrowRight":
        case "PageDown":
          if (mode === "single") {
            e.preventDefault();
            goToPage(currentPage + 1);
          }
          break;
        case "Home":
          e.preventDefault();
          goToPage(1);
          break;
        case "End":
          e.preventDefault();
          goToPage(numPages);
          break;
        case "+":
        case "=":
          e.preventDefault();
          zoomIn();
          break;
        case "-":
        case "_":
          e.preventDefault();
          zoomOut();
          break;
        case "0":
          e.preventDefault();
          zoomReset();
          break;
        case "f":
        case "F":
          e.preventDefault();
          toggleFit();
          break;
      }
    },
    [currentPage, numPages, mode, goToPage, zoomIn, zoomOut, zoomReset, toggleFit],
  );

  const submitPageInput = () => {
    const n = parseInt(pageInput, 10);
    if (Number.isFinite(n)) goToPage(n);
    else setPageInput(String(currentPage));
  };

  const btnBase =
    "w-8 h-8 flex items-center justify-center rounded-md transition-colors text-[var(--icon-color)] hover:text-[var(--icon-color-hover)] hover:bg-[var(--background-modifier-hover)] disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-[var(--icon-color)]";
  const btnActive =
    "w-8 h-8 flex items-center justify-center rounded-md transition-colors text-[var(--text-accent)] bg-[var(--background-modifier-active)]";

  const loadingNode = (
    <div className="flex-1 flex items-center justify-center text-[var(--text-faint)]">
      <Loader2 size={20} className="animate-spin" />
    </div>
  );

  const setPageRef = (n: number) => (el: HTMLDivElement | null) => {
    if (el) pageRefs.current.set(n, el);
    else pageRefs.current.delete(n);
  };

  return (
    <div
      ref={containerRef}
      tabIndex={0}
      onKeyDown={onKeyDown}
      className="workspace-leaf-content flex-1 min-h-0 min-w-0 flex flex-col bg-[var(--background-primary)] outline-none"
    >
      <style>{`
        /* overflow-anchor: none — critical. Stops the browser from
           auto-compensating scroll position when pages above the viewport
           change height (placeholder → real canvas swap). Without this,
           every canvas render during scroll causes a second scroll "jump"
           that feels laggy and cut-by-cut. */
        .pdf-viewer-stage { overflow-anchor: none; }
        .pdf-viewer-stage .react-pdf__Document { display: flex; flex-direction: column; align-items: center; gap: ${PAGE_GAP}px; }
        .pdf-viewer-stage .react-pdf__Page { background: var(--background-primary); box-shadow: 0 1px 3px rgba(0,0,0,0.18), 0 4px 12px rgba(0,0,0,0.12); }
        .pdf-viewer-stage .react-pdf__Page__canvas { display: block; }
        /* Page wrappers isolate layout + paint so intersection/render
           work in one page can't trigger relayout of siblings. */
        .pdf-viewer-stage [data-page] { contain: layout paint; overflow: hidden; }
      `}</style>

      {/* Title banner. Slim — editor uses more, this is a read-only
          surface. Embedders set hideTitle to suppress entirely. */}
      {!hideTitle && (
        <div className="flex-shrink-0 pt-3 pb-2 px-8 w-full">
          <h1 className="text-[18px] font-semibold text-[var(--text-title-h1)] leading-tight tracking-tight truncate flex items-center gap-2">
            {title ?? "Untitled"}
          </h1>
        </div>
      )}

      {/* Toolbar */}
      <div className="flex-shrink-0 flex items-center gap-1 px-3 h-10 border-y border-[var(--background-modifier-border)] bg-[var(--background-secondary)]">
        <button
          className={btnBase}
          onClick={() => goToPage(currentPage - 1)}
          disabled={numPages === 0 || currentPage <= 1}
          title="Previous page (←/PgUp)"
        >
          <ChevronLeft size={16} strokeWidth={1.8} />
        </button>
        <button
          className={btnBase}
          onClick={() => goToPage(currentPage + 1)}
          disabled={numPages === 0 || currentPage >= numPages}
          title="Next page (→/PgDn)"
        >
          <ChevronRight size={16} strokeWidth={1.8} />
        </button>

        <div className="flex items-center gap-1 ml-2 text-[12px] text-[var(--text-muted)]">
          <input
            type="text"
            value={pageInput}
            onChange={(e) => setPageInput(e.target.value)}
            onBlur={submitPageInput}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                submitPageInput();
                (e.target as HTMLInputElement).blur();
              }
            }}
            className="w-12 h-6 px-1 text-center rounded bg-[var(--background-primary)] border border-[var(--background-modifier-border)] text-[var(--text-normal)] focus:outline-none focus:border-[var(--interactive-accent)]"
          />
          <span className="text-[var(--text-faint)]">/</span>
          <span className="tabular-nums">{numPages || "—"}</span>
        </div>

        <div className="w-px h-5 bg-[var(--background-modifier-border)] mx-2" />

        <button
          className={btnBase}
          onClick={zoomOut}
          disabled={scale <= MIN_SCALE}
          title="Zoom out (−)"
        >
          <ZoomOut size={16} strokeWidth={1.8} />
        </button>
        <button
          className={btnBase}
          onClick={zoomReset}
          title="Reset zoom (0)"
        >
          <span className="text-[11px] tabular-nums text-[var(--text-muted)]">
            {Math.round(scale * 100)}%
          </span>
        </button>
        <button
          className={btnBase}
          onClick={zoomIn}
          disabled={scale >= MAX_SCALE}
          title="Zoom in (+)"
        >
          <ZoomIn size={16} strokeWidth={1.8} />
        </button>
        <button
          className={fitWidth ? btnActive : btnBase}
          onClick={toggleFit}
          title="Fit to width (f)"
        >
          <Maximize2 size={16} strokeWidth={1.8} />
        </button>

        <div className="w-px h-5 bg-[var(--background-modifier-border)] mx-2" />

        <button
          className={mode === "continuous" ? btnActive : btnBase}
          onClick={toggleMode}
          title={`Mode: ${mode === "single" ? "single page" : "continuous"}`}
        >
          <ScrollText size={16} strokeWidth={1.8} />
        </button>
        <button
          className={textLayerOn ? btnActive : btnBase}
          onClick={() => setTextLayerOn((v) => !v)}
          title={`Text layer ${textLayerOn ? "on" : "off"} (enables text selection; slower)`}
        >
          <Type size={16} strokeWidth={1.8} />
        </button>

        <div className="ml-auto flex items-center gap-2 min-w-0">
          {extraToolbar && (
            <>
              {extraToolbar}
              <div className="w-px h-5 bg-[var(--background-modifier-border)] mx-1" />
            </>
          )}
          <span className="text-[11px] text-[var(--text-faint)] truncate">
            {mode === "single" ? "Single page" : "Continuous"}
          </span>
        </div>
      </div>

      {/* Body */}
      <div
        ref={scrollRef}
        className="pdf-viewer-stage flex-1 min-h-0 min-w-0 overflow-auto py-4"
      >
        {error ? (
          <div className="h-full flex items-center justify-center text-[var(--text-muted)] text-[13px] px-6 text-center">
            <div>
              <div className="font-medium text-[var(--text-normal)] mb-1">
                Could not load PDF
              </div>
              <div className="text-[var(--text-faint)]">{error}</div>
            </div>
          </div>
        ) : (
          <Document
            file={url}
            onLoadSuccess={onDocumentLoad}
            onLoadError={onDocumentError}
            options={docOptions}
            loading={loadingNode}
            error={
              <div className="flex-1 flex items-center justify-center text-[var(--text-muted)] text-[13px]">
                Failed to load PDF
              </div>
            }
          >
            {numPages > 0 && mode === "single" && (
              <div ref={setPageRef(currentPage)}>
                <Page
                  pageNumber={currentPage}
                  scale={scale}
                  devicePixelRatio={Math.min(
                    window.devicePixelRatio || 1,
                    MAX_DEVICE_PIXEL_RATIO,
                  )}
                  onRenderSuccess={
                    currentPage === 1 ? onFirstPageRender : undefined
                  }
                  loading={loadingNode}
                  renderAnnotationLayer={textLayerOn}
                  renderTextLayer={textLayerOn}
                />
              </div>
            )}
            {numPages > 0 &&
              mode === "continuous" &&
              Array.from({ length: numPages }, (_, i) => i + 1).map((n) => {
                // IntersectionObserver-driven virtualisation. Render the
                // real Page only when the wrapper is intersecting the
                // viewport (or within the configured rootMargin buffer).
                // Page 1 is always in the set because it seeds
                // `basePageWidth/Height` used for placeholder sizing.
                const visible = visiblePages.has(n);
                const phW = basePageWidth ? basePageWidth * scale : 612 * scale;
                const phH = basePageHeight
                  ? basePageHeight * scale
                  : 792 * scale;
                return (
                  <div
                    key={n}
                    ref={setPageRef(n)}
                    data-page={n}
                    style={{
                      // Pin wrapper to exact size. Fixed width/height
                      // (not min) means the child (placeholder or
                      // rendered Page canvas) can never grow the
                      // wrapper. With contain+overflow:hidden in the
                      // style block above, a canvas rendering at
                      // fractionally different dims cannot displace
                      // siblings — the wrapper absorbs the diff.
                      // Eliminates the "scroll then jump" artefact.
                      width: phW,
                      height: phH,
                    }}
                  >
                    {visible ? (
                      <Page
                        pageNumber={n}
                        scale={scale}
                        devicePixelRatio={Math.min(
                          window.devicePixelRatio || 1,
                          MAX_DEVICE_PIXEL_RATIO,
                        )}
                        onRenderSuccess={
                          n === 1 ? onFirstPageRender : undefined
                        }
                        loading={loadingNode}
                        renderAnnotationLayer={textLayerOn}
                        renderTextLayer={textLayerOn}
                      />
                    ) : (
                      <div
                        aria-hidden
                        style={{
                          width: phW,
                          height: phH,
                          background: "var(--background-primary)",
                          boxShadow:
                            "0 1px 3px rgba(0,0,0,0.18), 0 4px 12px rgba(0,0,0,0.12)",
                        }}
                      />
                    )}
                  </div>
                );
              })}
          </Document>
        )}
      </div>
    </div>
  );
}

export default memo(PdfViewer);
