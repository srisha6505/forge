import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useTransition,
} from "react";
import {
  FileText,
  Hash,
  Loader2,
  RefreshCw,
  Search as SearchIcon,
  X,
} from "lucide-react";
import {
  reindexVault,
  searchStatus,
  searchVault,
  type SearchHit,
  type SearchStatus,
} from "../lib/tauri";
import { SearchSnippet } from "./SearchSnippet";

interface Props {
  open: boolean;
  onClose: () => void;
  onOpenFile: (
    path: string,
    options?: {
      newTab?: boolean;
      jumpToLine?: number;
      highlight?: string[];
    },
  ) => void;
}

const DEBOUNCE_MS = 150;

// ── Memoised result row. Only re-renders if its hit / selected state /
// callbacks change. Result list re-renders are gated to debounced
// search completions, not every keystroke. ──────────────────────────────

interface RowProps {
  hit: SearchHit;
  idx: number;
  isSelected: boolean;
}

const ResultRow = memo(function ResultRow({ hit, idx, isSelected }: RowProps) {
  const isVector = hit.source === "vector";
  return (
    <button
      data-idx={idx}
      className={`row w-full text-left px-4 py-3 border-b border-[var(--background-modifier-border)] flex items-start gap-3 transition-colors ${
        isSelected
          ? "bg-[var(--background-modifier-active)]"
          : "hover:bg-[var(--background-modifier-hover)]"
      }`}
    >
      <FileText
        size={14}
        strokeWidth={1.8}
        className={`mt-1 flex-shrink-0 ${
          isSelected
            ? "text-[var(--text-accent)]"
            : isVector
              ? "text-[var(--text-faint)] opacity-60"
              : "text-[var(--text-faint)]"
        }`}
      />
      <div className="flex-1 min-w-0">
        <div className="flex items-center justify-between gap-2 mb-1">
          <div
            className={`text-[14px] font-semibold truncate ${
              isSelected
                ? "text-[var(--text-accent)]"
                : "text-[var(--text-normal)]"
            }`}
          >
            <SearchSnippet
              source={hit.title}
              highlightTerms={hit.matched_terms}
              plain
            />
          </div>
          <div className="flex items-center gap-2 flex-shrink-0">
            {isVector && (
              <span
                className="text-[9px] uppercase tracking-[0.12em] px-1.5 py-0.5 rounded text-[var(--text-faint)] border border-[var(--background-modifier-border)]"
                title="Semantic neighbour — no keyword hit in this chunk"
              >
                related
              </span>
            )}
            <div className="text-[10px] text-[var(--text-faint)] tabular-nums">
              {hit.score.toFixed(2)}
            </div>
          </div>
        </div>
        {hit.heading && hit.heading !== "(top)" && (
          <div className="flex items-center gap-1 text-[10px] uppercase tracking-wider text-[var(--text-muted)] truncate mb-1">
            <Hash size={10} strokeWidth={2.2} className="flex-shrink-0" />
            <SearchSnippet
              source={hit.heading.replace(/^#+\s*/, "")}
              highlightTerms={hit.matched_terms}
              plain
            />
          </div>
        )}
        <div className="text-[12px] text-[var(--text-muted)] line-clamp-2 leading-snug">
          <SearchSnippet
            source={hit.snippet}
            highlightTerms={hit.matched_terms}
          />
        </div>
      </div>
    </button>
  );
});

// ── Results list — memoised so typing in the input doesn't re-render
// the (potentially long) list of hits. ────────────────────────────────

interface ResultsProps {
  hits: SearchHit[];
  selected: number;
}

const ResultsList = memo(function ResultsList({
  hits,
  selected,
}: ResultsProps) {
  // Group: keyword/literal first, then vector ("related") below a
  // visual divider. Both keep their original ranking inside the group.
  const direct: { hit: SearchHit; idx: number }[] = [];
  const related: { hit: SearchHit; idx: number }[] = [];
  hits.forEach((hit, idx) => {
    if (hit.source === "vector") related.push({ hit, idx });
    else direct.push({ hit, idx });
  });

  return (
    <>
      {direct.map(({ hit, idx }) => (
        <ResultRow
          key={`${hit.path}-${idx}`}
          hit={hit}
          idx={idx}
          isSelected={idx === selected}
        />
      ))}
      {related.length > 0 && direct.length > 0 && (
        <div className="px-4 py-2 text-[10px] uppercase tracking-[0.12em] text-[var(--text-faint)] bg-[var(--background-secondary)] border-b border-[var(--background-modifier-border)]">
          Related (semantic)
        </div>
      )}
      {related.length > 0 && direct.length === 0 && (
        <div className="px-4 py-2 text-[10px] uppercase tracking-[0.12em] text-[var(--text-faint)] bg-[var(--background-secondary)] border-b border-[var(--background-modifier-border)]">
          No keyword match — semantic neighbours
        </div>
      )}
      {related.map(({ hit, idx }) => (
        <ResultRow
          key={`${hit.path}-${idx}`}
          hit={hit}
          idx={idx}
          isSelected={idx === selected}
        />
      ))}
    </>
  );
});

export default function SearchModal({ open, onClose, onOpenFile }: Props) {
  // Uncontrolled input — the DOM owns the value, React doesn't re-render
  // per keystroke. We schedule the debounced searchQuery update from a
  // single onChange handler. This is the only way to make typing in a
  // webkit2gtk webview feel native; controlled inputs add a render +
  // diff per keystroke which webkit2gtk can't keep up with at full
  // typing speed.
  const [searchQuery, setSearchQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState(0);
  const [status, setStatus] = useState<SearchStatus | null>(null);
  const [reindexing, setReindexing] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const debounceTimer = useRef<number | null>(null);
  const requestId = useRef(0);
  // React 18 transition: deprioritises the result re-render so a
  // burst of keystrokes doesn't queue behind a 30-row reconciliation.
  // The input stays at default priority and never freezes.
  const [, startTransition] = useTransition();
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  // Latest hits in a ref so the delegated click handler can read them
  // without becoming part of its dep list (which would re-bind handlers).
  const hitsRef = useRef<SearchHit[]>([]);
  hitsRef.current = hits;
  // Progressive render. Bigger lists than this are rare with 30-result
  // backend cap, but the cost shape is what matters: rendering 12 rows
  // is ~3x cheaper than rendering 30 on webkit2gtk, and the user almost
  // never scrolls past the first page — so most searches do 12 rows of
  // work, not 30.
  const PAGE_SIZE = 12;
  const [visibleCount, setVisibleCount] = useState(PAGE_SIZE);

  // Open / close lifecycle.
  useEffect(() => {
    if (open) {
      requestAnimationFrame(() => inputRef.current?.focus());
      setSelected(0);
      searchStatus().then(setStatus).catch(() => {});
    } else {
      setSearchQuery("");
      setHits([]);
      setBusy(false);
      setError(null);
      // Reset DOM value too since the input is uncontrolled.
      if (inputRef.current) inputRef.current.value = "";
    }
  }, [open]);

  // onChange schedules the debounced search query update directly from
  // the DOM event. No React state for the input value → no re-render
  // per keystroke.
  const handleInputChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const val = e.target.value;
      if (debounceTimer.current !== null) {
        window.clearTimeout(debounceTimer.current);
      }
      debounceTimer.current = window.setTimeout(() => {
        setSearchQuery(val);
      }, DEBOUNCE_MS);
    },
    [],
  );

  // Run search when searchQuery changes.
  useEffect(() => {
    if (!open) return;
    const id = ++requestId.current;
    if (!searchQuery.trim()) {
      setHits([]);
      setBusy(false);
      setError(null);
      return;
    }
    setBusy(true);
    setError(null);
    searchVault(searchQuery, 30)
      .then((results) => {
        if (id !== requestId.current) return;
        // Mark the heavy result re-render as a transition so React keeps
        // the input handler responsive. Without this the typing cursor
        // freezes for 50-150ms while 30 rows reconcile on webkit2gtk.
        startTransition(() => {
          setHits(results);
          setSelected(0);
          setVisibleCount(PAGE_SIZE);
          // Snap the scroll container back to the top so the first page
          // is what the user sees after a new query.
          if (scrollContainerRef.current) {
            scrollContainerRef.current.scrollTop = 0;
          }
        });
        searchStatus().then(setStatus).catch(() => {});
      })
      .catch((e) => {
        if (id !== requestId.current) return;
        setError(String(e));
        setHits([]);
      })
      .finally(() => {
        if (id === requestId.current) setBusy(false);
      });
  }, [open, searchQuery]);

  // Scroll the keyboard-selected row into view. Runs after the DOM
  // commit so the row's offset is final. `block: "nearest"` keeps
  // movement minimal — only scrolls when the row is actually clipped.
  useEffect(() => {
    const root = scrollContainerRef.current;
    if (!root) return;
    const el = root.querySelector<HTMLButtonElement>(
      `button[data-idx="${selected}"]`,
    );
    if (!el) return;
    el.scrollIntoView({ block: "nearest", behavior: "auto" });
  }, [selected, hits]);

  const handleReindex = useCallback(async () => {
    setReindexing(true);
    setError(null);
    try {
      const s = await reindexVault();
      setStatus(s);
      // Force re-run by bumping the search query (read current input via ref).
      const current = inputRef.current?.value ?? "";
      if (current.trim()) {
        setSearchQuery((q) => (q === current ? `${q} ` : current));
        setTimeout(() => setSearchQuery(current), 0);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setReindexing(false);
    }
  }, []);

  // Reveal another page when the scroll container nears its bottom.
  // Reads scroll metrics directly off the event target — cheap, no
  // IntersectionObserver setup. Threshold 200px gives the next page
  // time to render before the user sees a flash of empty space.
  const handleScroll = useCallback(
    (e: React.UIEvent<HTMLDivElement>) => {
      const el = e.currentTarget;
      const remaining = el.scrollHeight - el.scrollTop - el.clientHeight;
      if (remaining < 200) {
        setVisibleCount((c) => {
          if (c >= hitsRef.current.length) return c;
          return Math.min(c + PAGE_SIZE, hitsRef.current.length);
        });
      }
    },
    [],
  );

  // When the user keyboard-navigates past the visible window, expand
  // the window so the selected row can actually render and scroll into
  // view. Without this, ArrowDown past idx 11 selects nothing visible.
  useEffect(() => {
    if (selected >= visibleCount && selected < hits.length) {
      setVisibleCount(
        Math.min(
          Math.max(selected + 1, visibleCount + PAGE_SIZE),
          hits.length,
        ),
      );
    }
  }, [selected, visibleCount, hits.length]);

  const visibleHits = useMemo(
    () => hits.slice(0, visibleCount),
    [hits, visibleCount],
  );

  // Single delegated handlers — never rebind, so ResultRow's memo can
  // skip every row except the previously-/newly-selected one on arrow
  // navigation. The rows carry their `idx` as a data attribute and the
  // handler reads it from the closest button ancestor.
  const handleListMouseOver = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const btn = (e.target as HTMLElement).closest<HTMLButtonElement>(
        "button[data-idx]",
      );
      if (!btn) return;
      const idx = parseInt(btn.dataset.idx ?? "", 10);
      if (Number.isFinite(idx)) setSelected(idx);
    },
    [],
  );

  const handleListClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const btn = (e.target as HTMLElement).closest<HTMLButtonElement>(
        "button[data-idx]",
      );
      if (!btn) return;
      const idx = parseInt(btn.dataset.idx ?? "", 10);
      const hit = hitsRef.current[idx];
      if (!hit) return;
      onOpenFile(hit.path, {
        newTab: e.ctrlKey || e.metaKey,
        jumpToLine: hit.line_start,
        highlight: hit.matched_terms,
      });
      onClose();
    },
    [onOpenFile, onClose],
  );

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelected((s) => Math.min(hits.length - 1, s + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelected((s) => Math.max(0, s - 1));
    } else if (e.key === "Enter" && hits[selected]) {
      e.preventDefault();
      const hit = hits[selected];
      onOpenFile(hit.path, {
        newTab: e.ctrlKey || e.metaKey,
        jumpToLine: hit.line_start,
        highlight: hit.matched_terms,
      });
      onClose();
    }
  };

  const placeholder = useMemo(() => {
    if (!status?.indexed) return "Search vault — first run will build the index…";
    const mode = status.vectors_available ? "hybrid" : "BM25";
    return `Search ${status.chunk_count} chunks (${mode}) — "quotes" for exact match`;
  }, [status]);

  if (!open) return null;

  const hasQuery = searchQuery.trim().length > 0;
  const isQuoted = searchQuery.trim().startsWith('"');

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-[6vh] bg-[hsla(0,0%,0%,0.45)]"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      onKeyDown={onKeyDown}
    >
      <div className="w-[860px] max-w-[92vw] h-[82vh] flex flex-col rounded-xl shadow-2xl bg-[var(--background-primary)] border border-[var(--background-modifier-border)] overflow-hidden">
        <div className="flex-shrink-0 flex items-center gap-3 px-5 py-4 border-b border-[var(--background-modifier-border)]">
          <SearchIcon
            size={20}
            strokeWidth={2}
            className="text-[var(--text-faint)] flex-shrink-0"
          />
          <input
            ref={inputRef}
            type="text"
            defaultValue=""
            onChange={handleInputChange}
            placeholder={placeholder}
            className="flex-1 text-[18px] py-1 outline-none bg-transparent text-[var(--text-normal)] placeholder:text-[var(--text-faint)]"
          />
          {isQuoted && (
            <span
              className="text-[10px] uppercase tracking-wider px-2 py-0.5 rounded-md bg-[var(--text-accent)]/15 text-[var(--text-accent)] flex-shrink-0"
              title="Quoted query → exact substring, no stemming, no semantic"
            >
              exact
            </span>
          )}
          {busy && (
            <Loader2
              size={18}
              strokeWidth={2}
              className="text-[var(--text-faint)] animate-spin flex-shrink-0"
            />
          )}
          <button
            onClick={onClose}
            className="text-[var(--text-faint)] hover:text-[var(--text-normal)] p-1 rounded hover:bg-[var(--background-modifier-hover)] flex-shrink-0"
            title="Close (Esc)"
          >
            <X size={18} strokeWidth={2.2} />
          </button>
        </div>

        <div
          ref={scrollContainerRef}
          className="flex-1 overflow-y-auto"
          onMouseOver={handleListMouseOver}
          onClick={handleListClick}
          onScroll={handleScroll}
        >
          {error && (
            <div className="m-4 p-3 rounded-md bg-[var(--background-modifier-error)] text-[var(--text-error)] text-[12px] font-mono whitespace-pre-wrap">
              {error}
            </div>
          )}
          {!error && hits.length === 0 && hasQuery && !busy && (
            <div className="px-4 py-12 text-center">
              <div className="text-[13px] text-[var(--text-muted)] mb-2">
                No results for &ldquo;{searchQuery}&rdquo;
              </div>
              <div className="text-[11px] text-[var(--text-faint)] mb-4">
                The index may be out of date if you&apos;ve added or edited
                files recently.
              </div>
              <button
                onClick={handleReindex}
                disabled={reindexing}
                className="text-[11px] px-3 py-1.5 rounded-md border border-[var(--background-modifier-border)] hover:border-[var(--interactive-accent)] hover:text-[var(--text-accent)] text-[var(--text-muted)] disabled:opacity-50 transition-colors inline-flex items-center gap-1.5"
              >
                <RefreshCw
                  size={12}
                  strokeWidth={2}
                  className={reindexing ? "animate-spin" : ""}
                />
                {reindexing ? "Rebuilding…" : "Rebuild index"}
              </button>
            </div>
          )}
          {!error && hits.length === 0 && !hasQuery && (
            <div className="px-4 py-16 text-center text-[12px] text-[var(--text-faint)] leading-relaxed">
              Type to search across your vault.
              <br />
              Wrap in &ldquo;quotes&rdquo; for exact-substring match (no
              stemming, no semantic).
            </div>
          )}
          <ResultsList hits={visibleHits} selected={selected} />
          {visibleCount < hits.length && (
            <div className="px-4 py-3 text-center text-[10px] text-[var(--text-faint)] tabular-nums">
              {hits.length - visibleCount} more — scroll to load
            </div>
          )}
        </div>

        <div className="flex-shrink-0 px-4 py-2 border-t border-[var(--background-modifier-border)] flex items-center justify-between text-[11px] text-[var(--text-faint)]">
          <span>↑↓ navigate · Enter open · Ctrl+Enter new tab · Esc close</span>
          <div className="flex items-center gap-3">
            <span className="tabular-nums">
              {hits.length} result{hits.length === 1 ? "" : "s"}
            </span>
            <button
              onClick={handleReindex}
              disabled={reindexing}
              title="Rebuild search index from current vault"
              className="text-[var(--text-muted)] hover:text-[var(--text-accent)] disabled:opacity-50 inline-flex items-center gap-1"
            >
              <RefreshCw
                size={11}
                strokeWidth={2}
                className={reindexing ? "animate-spin" : ""}
              />
              <span>{reindexing ? "Rebuilding" : "Reindex"}</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
