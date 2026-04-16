import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  FileText,
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

interface Props {
  open: boolean;
  onClose: () => void;
  onOpenFile: (path: string, options?: { newTab?: boolean }) => void;
}

const DEBOUNCE_MS = 150;

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

// ── Memoised result row. Only re-renders if its hit / selected state /
// callbacks change. Result list re-renders are gated to debounced
// search completions, not every keystroke. ──────────────────────────────

interface RowProps {
  hit: SearchHit;
  isSelected: boolean;
  onSelect: () => void;
  onOpen: (newTab: boolean) => void;
}

const ResultRow = memo(function ResultRow({
  hit,
  isSelected,
  onSelect,
  onOpen,
}: RowProps) {
  return (
    <button
      onMouseEnter={onSelect}
      onClick={(e) => onOpen(e.ctrlKey || e.metaKey)}
      className={`w-full text-left px-4 py-3 border-b border-[var(--background-modifier-border)] flex items-start gap-3 ${
        isSelected
          ? "bg-[var(--background-modifier-active)]"
          : "hover:bg-[var(--background-modifier-hover)]"
      }`}
    >
      <FileText
        size={15}
        strokeWidth={1.8}
        className={`mt-0.5 flex-shrink-0 ${
          isSelected
            ? "text-[var(--text-accent)]"
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
            {hit.title}
          </div>
          <div className="text-[10px] text-[var(--text-faint)] tabular-nums flex-shrink-0">
            {hit.score.toFixed(2)}
          </div>
        </div>
        {hit.heading && (
          <div className="text-[10px] uppercase tracking-wider text-[var(--text-muted)] truncate mb-1">
            {hit.heading}
          </div>
        )}
        <div className="text-[12px] text-[var(--text-muted)] line-clamp-2 leading-snug">
          {snippetToPlain(hit.snippet)}
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
  onSelect: (i: number) => void;
  onOpen: (path: string, newTab: boolean) => void;
  onClose: () => void;
}

const ResultsList = memo(function ResultsList({
  hits,
  selected,
  onSelect,
  onOpen,
  onClose,
}: ResultsProps) {
  return (
    <>
      {hits.map((hit, i) => (
        <ResultRow
          key={`${hit.path}-${i}`}
          hit={hit}
          isSelected={i === selected}
          onSelect={() => onSelect(i)}
          onOpen={(newTab) => {
            onOpen(hit.path, newTab);
            onClose();
          }}
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
        setHits(results);
        setSelected(0);
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

  // Stable callbacks for memoised children.
  const handleSelect = useCallback((i: number) => setSelected(i), []);
  const handleOpen = useCallback(
    (path: string, newTab: boolean) =>
      onOpenFile(path, { newTab }),
    [onOpenFile],
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
      handleOpen(hits[selected].path, e.ctrlKey || e.metaKey);
      onClose();
    }
  };

  const placeholder = useMemo(() => {
    if (!status?.indexed) return "Search vault — first run will build the index…";
    const mode = status.vectors_available ? "hybrid" : "BM25";
    return `Search ${status.chunk_count} chunks (${mode}) — "quotes" for keyword only`;
  }, [status]);

  if (!open) return null;

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

        <div className="flex-1 overflow-y-auto">
          {error && (
            <div className="m-4 p-3 rounded-md bg-[var(--background-modifier-error)] text-[var(--text-error)] text-[12px] font-mono whitespace-pre-wrap">
              {error}
            </div>
          )}
          {!error && hits.length === 0 && searchQuery.trim() && !busy && (
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
          {!error && hits.length === 0 && !searchQuery.trim() && (
            <div className="px-4 py-16 text-center text-[12px] text-[var(--text-faint)]">
              Type to search across your vault.
              <br />
              Wrap in &ldquo;quotes&rdquo; for keyword-only (BM25) search.
            </div>
          )}
          <ResultsList
            hits={hits}
            selected={selected}
            onSelect={handleSelect}
            onOpen={handleOpen}
            onClose={onClose}
          />
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
