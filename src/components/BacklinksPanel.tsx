import { useEffect, useState } from "react";
import { ChevronDown, Link2 } from "./ui/Icons";
import { listBacklinks, type LinkHit } from "../lib/tauri";

interface Props {
  path: string | null;
  onOpen?: (path: string) => void;
}

// Inline backlinks block rendered at the bottom of the editor's scroll
// area (not as a floating panel). Matches
// forge_ui/Editor.jsx::BacklinksPanel: collapsed by default, chevron
// rotates when open, top border separates from the document body.
// Uses the existing Tauri `list_backlinks` command — no behavioural
// change vs the previous sidebar variant, only the shell.
export default function BacklinksPanel({ path, onOpen }: Props) {
  const [hits, setHits] = useState<LinkHit[]>([]);
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!path) {
      setHits([]);
      return;
    }
    let cancelled = false;
    setLoading(true);
    listBacklinks(path)
      .then((results) => {
        if (!cancelled) setHits(results);
      })
      .catch((e) => console.warn("backlinks", e))
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [path]);

  if (!path) return null;

  return (
    <div style={{ marginTop: 48, borderTop: "1px solid var(--hr-color)", paddingTop: 8 }}>
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          width: "100%",
          background: "transparent",
          border: 0,
          cursor: "pointer",
          padding: "6px 0",
          color: "var(--text-muted)",
          fontSize: "var(--font-ui-small)",
          fontWeight: 500,
          fontFamily: "var(--font-interface)",
        }}
      >
        <span
          style={{
            transition: "transform var(--motion-duration-fast) var(--motion-ease)",
            transform: open ? "rotate(0)" : "rotate(-90deg)",
            display: "flex",
          }}
        >
          <ChevronDown size={12} />
        </span>
        <Link2 size={14} />
        Backlinks
        <span style={{ color: "var(--text-faint)", fontWeight: 400 }}>
          ({loading ? "…" : hits.length})
        </span>
      </button>
      {open && hits.length > 0 && (
        <div style={{ padding: "4px 0 8px" }}>
          {hits.map((bl) => (
            <div
              key={bl.path}
              onClick={() => onOpen?.(bl.path)}
              style={{
                padding: "6px 8px",
                borderRadius: "var(--radius-s)",
                marginBottom: 2,
                cursor: onOpen ? "pointer" : "default",
                transition: "background var(--motion-duration-fast) var(--motion-ease)",
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.background = "var(--background-modifier-hover)";
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.background = "transparent";
              }}
            >
              <div
                style={{
                  fontSize: "var(--font-ui-small)",
                  fontWeight: 500,
                  color: "var(--text-accent)",
                }}
              >
                {bl.name}
              </div>
              {bl.snippet && (
                <div
                  style={{
                    fontSize: "var(--font-ui-smaller)",
                    color: "var(--text-faint)",
                    marginTop: 2,
                  }}
                >
                  {bl.snippet}
                </div>
              )}
            </div>
          ))}
        </div>
      )}
      {open && hits.length === 0 && (
        <div
          style={{
            padding: "8px 0",
            fontSize: "var(--font-ui-small)",
            color: "var(--text-faint)",
          }}
        >
          {loading ? "Scanning vault…" : "No backlinks found"}
        </div>
      )}
    </div>
  );
}
