import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useTransition,
} from "react";
import { FileText, Hash, RefreshCw, Search as SearchIcon, X } from "lucide-react";
import {
  reindexVault,
  searchStatus,
  searchVault,
  type SearchHit,
  type SearchStatus,
} from "../lib/tauri";
import { SearchSnippet } from "./SearchSnippet";

interface Props {
  onOpenFile: (
    path: string,
    options?: {
      newTab?: boolean;
      jumpToLine?: number;
      highlight?: string[];
    },
  ) => void;
}

const SEARCH_DEBOUNCE_MS = 250;

function Search({ onOpenFile }: Props) {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<SearchStatus | null>(null);
  const [reindexing, setReindexing] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const debounceTimer = useRef<number | null>(null);
  const requestId = useRef(0);
  const [, startTransition] = useTransition();
  const PAGE_SIZE = 12;
  const [visibleCount, setVisibleCount] = useState(PAGE_SIZE);
  const scrollRef = useRef<HTMLDivElement>(null);

  // Initial status check.
  useEffect(() => {
    searchStatus()
      .then((s) => setStatus(s))
      .catch(() => {});
  }, []);

  // Auto-focus on mount so the user can start typing immediately when
  // they open the search pane.
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const runSearch = useCallback(async (q: string) => {
    const id = ++requestId.current;
    if (!q.trim()) {
      setHits([]);
      setBusy(false);
      setError(null);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const results = await searchVault(q, 30);
      // Drop stale results — only the latest request wins.
      if (id !== requestId.current) return;
      // Mark the heavy reconciliation as a transition so input typing
      // stays at default priority on webkit2gtk.
      startTransition(() => {
        setHits(results);
        setVisibleCount(PAGE_SIZE);
        if (scrollRef.current) scrollRef.current.scrollTop = 0;
      });
      // Refresh status (chunk count may have changed if this was the
      // first call and triggered an index build).
      searchStatus().then((s) => setStatus(s)).catch(() => {});
    } catch (e) {
      if (id !== requestId.current) return;
      setError(String(e));
      setHits([]);
    } finally {
      if (id === requestId.current) setBusy(false);
    }
  }, []);

  // Debounced search on query change.
  useEffect(() => {
    if (debounceTimer.current !== null) {
      window.clearTimeout(debounceTimer.current);
    }
    debounceTimer.current = window.setTimeout(() => {
      runSearch(query);
    }, SEARCH_DEBOUNCE_MS);
    return () => {
      if (debounceTimer.current !== null) {
        window.clearTimeout(debounceTimer.current);
      }
    };
  }, [query, runSearch]);

  const handleReindex = async () => {
    setReindexing(true);
    setError(null);
    try {
      const s = await reindexVault();
      setStatus(s);
      // Re-run current query against the fresh index.
      if (query.trim()) runSearch(query);
    } catch (e) {
      setError(String(e));
    } finally {
      setReindexing(false);
    }
  };

  const placeholder = useMemo(() => {
    if (status?.indexed) {
      const mode = status.vectors_available ? "hybrid" : "BM25";
      return `Search ${status.chunk_count} chunks (${mode})…`;
    }
    return "Search vault…";
  }, [status]);

  const isQuoted = query.trim().startsWith('"');

  // Group keyword/literal hits before vector ones, with a divider.
  const direct: SearchHit[] = [];
  const related: SearchHit[] = [];
  for (const h of hits) {
    if (h.source === "vector") related.push(h);
    else direct.push(h);
  }

  // Latest hits in a ref so the delegated click handler doesn't need
  // to be re-bound every render (would re-mount memoised rows).
  const hitsRef = useRef(hits);
  hitsRef.current = hits;

  const onListClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const btn = (e.target as HTMLElement).closest<HTMLButtonElement>(
        "button[data-path]",
      );
      if (!btn) return;
      const path = btn.dataset.path ?? "";
      const hit = hitsRef.current.find((h) => h.path === path);
      if (!hit) return;
      onOpenFile(path, {
        newTab: e.ctrlKey || e.metaKey,
        jumpToLine: hit.line_start,
        highlight: hit.matched_terms,
      });
    },
    [onOpenFile],
  );

  const onListAuxClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (e.button !== 1) return;
      const btn = (e.target as HTMLElement).closest<HTMLButtonElement>(
        "button[data-path]",
      );
      if (!btn) return;
      e.preventDefault();
      const path = btn.dataset.path ?? "";
      const hit = hitsRef.current.find((h) => h.path === path);
      if (!hit) return;
      onOpenFile(path, {
        newTab: true,
        jumpToLine: hit.line_start,
        highlight: hit.matched_terms,
      });
    },
    [onOpenFile],
  );

  const onListScroll = useCallback((e: React.UIEvent<HTMLDivElement>) => {
    const el = e.currentTarget;
    const remaining = el.scrollHeight - el.scrollTop - el.clientHeight;
    if (remaining < 200) {
      setVisibleCount((c) => {
        if (c >= hitsRef.current.length) return c;
        return Math.min(c + PAGE_SIZE, hitsRef.current.length);
      });
    }
  }, []);

  const directVisible = direct.slice(0, visibleCount);
  const remainingForRelated = Math.max(0, visibleCount - direct.length);
  const relatedVisible = related.slice(0, remainingForRelated);
  const totalVisible = directVisible.length + relatedVisible.length;

  return (
    <div className="h-full flex flex-col bg-[var(--background-secondary)]">
      <header
        className="flex-shrink-0 flex items-center gap-2 px-3 border-b border-[var(--background-modifier-border)]"
        style={{ height: "var(--topbar-height)" }}
      >
        <div className="text-[10px] font-semibold uppercase tracking-wider text-[var(--text-faint)]">
          Search
        </div>
        <div className="flex-1" />
        <button
          onClick={handleReindex}
          disabled={reindexing}
          title="Rebuild search index"
          className="text-[var(--text-muted)] hover:text-[var(--text-accent)] disabled:opacity-50"
        >
          <RefreshCw
            size={13}
            strokeWidth={2}
            className={reindexing ? "animate-spin" : ""}
          />
        </button>
      </header>

      <div className="flex-shrink-0 px-3 py-2 border-b border-[var(--background-modifier-border)]">
        <div className="relative">
          <SearchIcon
            size={13}
            strokeWidth={2}
            className="absolute left-2 top-1/2 -translate-y-1/2 text-[var(--text-faint)] pointer-events-none"
          />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={placeholder}
            className="w-full text-[12px] pl-7 pr-7 py-1.5 rounded-md bg-[var(--background-modifier-form-field)] border border-[var(--background-modifier-border)] focus:border-[var(--interactive-accent)] outline-none text-[var(--text-normal)] placeholder:text-[var(--text-faint)]"
          />
          {query && (
            <button
              onClick={() => setQuery("")}
              className="absolute right-1.5 top-1/2 -translate-y-1/2 text-[var(--text-faint)] hover:text-[var(--text-normal)] p-0.5"
              title="Clear"
            >
              <X size={12} strokeWidth={2.2} />
            </button>
          )}
        </div>
        <div className="mt-1.5 flex items-center justify-between text-[10px] text-[var(--text-faint)]">
          <span>{busy ? "Searching…" : `${hits.length} results`}</span>
          <div className="flex items-center gap-2">
            {isQuoted && (
              <span
                className="px-1.5 rounded text-[var(--text-accent)] bg-[var(--text-accent)]/15"
                title="Quoted → exact substring, no stemming"
              >
                exact
              </span>
            )}
            {status && !status.vectors_available && status.indexed && (
              <span title="Local embedding model unavailable">BM25 only</span>
            )}
          </div>
        </div>
      </div>

      <div
        ref={scrollRef}
        className="flex-1 overflow-y-auto overflow-x-hidden"
        onClick={onListClick}
        onAuxClick={onListAuxClick}
        onScroll={onListScroll}
      >
        {error && (
          <div className="m-3 p-2 rounded-md bg-[var(--background-modifier-error)] text-[var(--text-error)] text-[11px] font-mono whitespace-pre-wrap">
            {error}
          </div>
        )}
        {!error && hits.length === 0 && query.trim() && !busy && (
          <div className="px-4 py-8 text-center text-[11px] text-[var(--text-faint)]">
            No results
          </div>
        )}
        {!error && hits.length === 0 && !query.trim() && (
          <div className="px-4 py-8 text-center text-[11px] text-[var(--text-faint)]">
            Type to search across your vault.{"\n"}
            Wrap in &quot;quotes&quot; for exact substring.
          </div>
        )}
        {directVisible.map((hit, i) => (
          <ResultRow key={`d-${hit.path}-${i}`} hit={hit} />
        ))}
        {relatedVisible.length > 0 && directVisible.length > 0 && (
          <div className="px-3 py-1.5 text-[9px] uppercase tracking-wider text-[var(--text-faint)] bg-[var(--background-secondary-alt)] border-b border-[var(--background-modifier-border)]">
            Related (semantic)
          </div>
        )}
        {relatedVisible.length > 0 && directVisible.length === 0 && (
          <div className="px-3 py-1.5 text-[9px] uppercase tracking-wider text-[var(--text-faint)] bg-[var(--background-secondary-alt)] border-b border-[var(--background-modifier-border)]">
            No keyword match — semantic neighbours
          </div>
        )}
        {relatedVisible.map((hit, i) => (
          <ResultRow key={`r-${hit.path}-${i}`} hit={hit} />
        ))}
        {totalVisible < hits.length && (
          <div className="px-3 py-2 text-center text-[9px] text-[var(--text-faint)] tabular-nums">
            {hits.length - totalVisible} more
          </div>
        )}
      </div>
    </div>
  );
}

interface RowProps {
  hit: SearchHit;
}

const ResultRow = memo(function ResultRow({ hit }: RowProps) {
  const isVector = hit.source === "vector";
  return (
    <button
      data-path={hit.path}
      className="w-full text-left px-3 py-2 border-b border-[var(--background-modifier-border)] hover:bg-[var(--background-modifier-hover)] transition-colors"
    >
      <div className="flex items-center justify-between gap-2 mb-0.5">
        <div className="flex items-center gap-1.5 min-w-0 flex-1">
          <FileText
            size={11}
            strokeWidth={1.8}
            className={`flex-shrink-0 ${
              isVector
                ? "text-[var(--text-faint)] opacity-60"
                : "text-[var(--text-faint)]"
            }`}
          />
          <div className="text-[12px] font-semibold text-[var(--text-normal)] truncate">
            <SearchSnippet
              source={hit.title}
              highlightTerms={hit.matched_terms}
              plain
            />
          </div>
        </div>
        <div className="flex items-center gap-1.5 flex-shrink-0">
          {isVector && (
            <span
              className="text-[8px] uppercase tracking-[0.12em] text-[var(--text-faint)] border border-[var(--background-modifier-border)] rounded px-1"
              title="Semantic neighbour"
            >
              rel
            </span>
          )}
          <div className="text-[10px] text-[var(--text-faint)] tabular-nums">
            {hit.score.toFixed(2)}
          </div>
        </div>
      </div>
      {hit.heading && hit.heading !== "(top)" && (
        <div className="flex items-center gap-1 text-[10px] uppercase tracking-wider text-[var(--text-accent)] truncate mb-1">
          <Hash size={9} strokeWidth={2.2} className="flex-shrink-0" />
          <SearchSnippet
            source={hit.heading.replace(/^#+\s*/, "")}
            highlightTerms={hit.matched_terms}
            plain
          />
        </div>
      )}
      <div className="text-[11px] text-[var(--text-muted)] line-clamp-2 leading-snug">
        <SearchSnippet
          source={hit.snippet}
          highlightTerms={hit.matched_terms}
        />
      </div>
    </button>
  );
});

export default memo(Search);
