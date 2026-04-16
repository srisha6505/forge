import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { RefreshCw, Search as SearchIcon, X } from "lucide-react";
import {
  reindexVault,
  searchStatus,
  searchVault,
  type SearchHit,
  type SearchStatus,
} from "../lib/tauri";

// Strip basic markdown markers from a snippet so the result preview is
// readable in a one-line list. We render previews as plain text — full
// markdown rendering of arbitrary chunks would be too heavy for a
// debounced sidebar list.
function snippetToPlain(s: string): string {
  return s
    .replace(/\*\*([^*\n]+?)\*\*/g, "$1")
    .replace(/(?<!\*)\*([^*\n]+?)\*(?!\*)/g, "$1")
    .replace(/`([^`\n]+?)`/g, "$1")
    .replace(/\[\[([^\]|\n]+?)(?:\|([^\]\n]+?))?\]\]/g, (_m, t, a) => a || t)
    .replace(/\[([^\]\n]+?)\]\([^)\n]+?\)/g, "$1")
    .replace(/^#{1,6}\s+/gm, "")
    .replace(/\s+/g, " ")
    .trim();
}

interface Props {
  onOpenFile: (path: string, options?: { newTab?: boolean }) => void;
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
      setHits(results);
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
          {status && !status.vectors_available && status.indexed && (
            <span title="Local embedding model unavailable">BM25 only</span>
          )}
        </div>
      </div>

      <div className="flex-1 overflow-y-auto overflow-x-hidden">
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
            Wrap in &quot;quotes&quot; for exact-phrase BM25 only.
          </div>
        )}
        {hits.map((hit, i) => (
          <button
            key={`${hit.path}-${i}`}
            onClick={(e) =>
              onOpenFile(hit.path, { newTab: e.ctrlKey || e.metaKey })
            }
            onAuxClick={(e) => {
              if (e.button === 1) {
                e.preventDefault();
                onOpenFile(hit.path, { newTab: true });
              }
            }}
            className="w-full text-left px-3 py-2 border-b border-[var(--background-modifier-border)] hover:bg-[var(--background-modifier-hover)] transition-colors"
          >
            <div className="flex items-center justify-between gap-2 mb-0.5">
              <div className="text-[12px] font-semibold text-[var(--text-normal)] truncate">
                {hit.title}
              </div>
              <div className="text-[10px] text-[var(--text-faint)] tabular-nums flex-shrink-0">
                {hit.score.toFixed(2)}
              </div>
            </div>
            {hit.heading && (
              <div className="text-[10px] uppercase tracking-wider text-[var(--text-accent)] truncate mb-1">
                {hit.heading}
              </div>
            )}
            <div className="text-[11px] text-[var(--text-muted)] line-clamp-2 leading-snug">
              {snippetToPlain(hit.snippet)}
            </div>
          </button>
        ))}
      </div>
    </div>
  );
}

export default memo(Search);
