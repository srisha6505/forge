import { useEffect, useMemo, useRef, useState } from "react";
import ForceGraph2D, { type ForceGraphMethods } from "react-force-graph-2d";
import * as d3 from "d3-force";
import { Minimize2, Search, X } from "lucide-react";
import { linkGraph, type GraphNode, type LinkGraph } from "../lib/tauri";

interface Props {
  open: boolean;
  onClose: () => void;
  activePath: string | null;
  onOpenFile: (path: string) => void;
  // Forwarded so this component remounts when the theme flips. The
  // canvas resolves CSS vars via getComputedStyle once per draw call,
  // but the colour memos (and any cached rgba strings) latch on first
  // open. Remount via key={theme} from the parent guarantees fresh
  // values without per-frame re-reads.
  theme?: string;
}

type FgNode = GraphNode & { x?: number; y?: number; vx?: number; vy?: number };
type FgLink = { source: string | FgNode; target: string | FgNode };

// Full-screen vault graph. Notes are nodes sized by link degree; each
// [[link]] renders as an edge. Physics via d3-force (baked into
// react-force-graph-2d). Clicks open the corresponding note.
export default function GraphView({ open, onClose, activePath, onOpenFile, theme: _theme }: Props) {
  // theme prop is consumed by the parent's `key={theme}` remount; we
  // only mark it used here so the prop survives lint and ts-noEmit.
  void _theme;
  const [graph, setGraph] = useState<LinkGraph | null>(null);
  const [hovered, setHovered] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const fgRef = useRef<ForceGraphMethods<FgNode, FgLink> | undefined>(undefined);
  const wrapperRef = useRef<HTMLDivElement | null>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });

  useEffect(() => {
    if (!open) return;
    linkGraph().then(setGraph).catch((e) => console.warn("graph", e));
  }, [open]);

  // Tune the d3-force simulation once the ref is live + graph data has
  // settled. Defaults send disconnected nodes flying off; clamp them.
  useEffect(() => {
    if (!open || !graph) return;
    const fg = fgRef.current;
    if (!fg) return;
    // Tuned for an Obsidian-ish look: gentle repulsion, short-ish
    // edges, gravity toward the center so orphans drift back.
    const charge = fg.d3Force("charge") as d3.ForceManyBody<FgNode> | undefined;
    charge?.strength(-45).distanceMax(240);
    const link = fg.d3Force("link") as
      | d3.ForceLink<FgNode, FgLink>
      | undefined;
    link?.distance(38).strength(0.9);
    fg.d3Force("x", d3.forceX(0).strength(0.08));
    fg.d3Force("y", d3.forceY(0).strength(0.08));
    fg.d3Force("collide", d3.forceCollide<FgNode>((n) => nodeR(n) + 10));
    fg.d3ReheatSimulation();
  }, [open, graph]);

  useEffect(() => {
    if (!open) return;
    const h = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [open, onClose]);

  useEffect(() => {
    if (!open) return;
    const measure = () => {
      const r = wrapperRef.current?.getBoundingClientRect();
      if (r) setSize({ w: r.width, h: r.height });
    };
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [open]);

  // Precompute neighbor sets so hover/active highlighting is O(1).
  const neighbors = useMemo(() => {
    const m = new Map<string, Set<string>>();
    if (!graph) return m;
    for (const n of graph.nodes) m.set(n.id, new Set());
    for (const e of graph.edges) {
      m.get(e.source)?.add(e.target);
      m.get(e.target)?.add(e.source);
    }
    return m;
  }, [graph]);

  const data = useMemo(() => {
    if (!graph) return { nodes: [], links: [] };
    return {
      nodes: graph.nodes.map((n) => ({ ...n }) as FgNode),
      links: graph.edges.map((e) => ({ source: e.source, target: e.target })),
    };
  }, [graph]);

  const filteredMatch = useMemo(() => {
    if (!query.trim() || !graph) return null;
    const q = query.trim().toLowerCase();
    return new Set(
      graph.nodes
        .filter((n) => n.name.toLowerCase().includes(q))
        .map((n) => n.id),
    );
  }, [query, graph]);

  if (!open) return null;

  // Node size: small base + capped sqrt(degree). Obsidian-like — hubs
  // are slightly bigger, but the variation is tight so the canvas
  // reads as a network, not a bubble chart.
  const nodeR = (n: FgNode) =>
    3 + Math.min(Math.sqrt(n.degree ?? 0), 4) * 0.7;

  const isHighlighted = (id: string) => {
    if (filteredMatch?.has(id)) return true;
    if (!hovered) return false;
    return id === hovered || neighbors.get(hovered)?.has(id) === true;
  };
  const isDimmed = (id: string) => {
    if (filteredMatch) return !filteredMatch.has(id);
    if (hovered) {
      return id !== hovered && !neighbors.get(hovered)?.has(id);
    }
    return false;
  };
  const linkHighlighted = (e: FgLink) => {
    const src = typeof e.source === "string" ? e.source : e.source.id;
    const tgt = typeof e.target === "string" ? e.target : e.target.id;
    if (hovered) return src === hovered || tgt === hovered;
    if (filteredMatch) return filteredMatch.has(src) && filteredMatch.has(tgt);
    return false;
  };

  return (
    <div className="fixed inset-0 z-40 bg-[var(--background-primary)] flex flex-col">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-2.5 border-b border-[var(--background-modifier-border)] bg-[var(--background-secondary)]">
        <div className="flex items-center gap-3">
          <span className="text-[11px] uppercase tracking-wider font-semibold text-[var(--text-muted)]">
            Graph view
          </span>
          {graph && (
            <span className="text-[10px] text-[var(--text-faint)] font-mono">
              {graph.nodes.length} notes, {graph.edges.length} links
            </span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <div className="flex items-center gap-1.5 px-2 py-1 rounded border border-[var(--background-modifier-border)] bg-[var(--background-primary)] w-[200px]">
            <Search size={12} className="text-[var(--text-faint)]" />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Filter notes"
              className="bg-transparent outline-none text-[12px] flex-1 min-w-0 text-[var(--text-normal)] placeholder:text-[var(--text-faint)]"
            />
          </div>
          <button
            onClick={() => fgRef.current?.zoomToFit(400, 40)}
            title="Fit to view"
            className="text-[11px] px-2 py-1 rounded border border-[var(--background-modifier-border)] text-[var(--text-muted)] hover:text-[var(--text-normal)] hover:bg-[var(--background-modifier-hover)]"
          >
            <Minimize2 size={12} />
          </button>
          <button
            onClick={onClose}
            title="Close (Esc)"
            className="text-[var(--text-muted)] hover:text-[var(--text-normal)] p-1"
          >
            <X size={16} />
          </button>
        </div>
      </div>

      {/* Graph canvas */}
      <div ref={wrapperRef} className="flex-1 relative overflow-hidden">
        {graph && size.w > 0 && (
          <ForceGraph2D<FgNode, FgLink>
            ref={fgRef as unknown as React.MutableRefObject<ForceGraphMethods<FgNode, FgLink>>}
            graphData={data}
            width={size.w}
            height={size.h}
            backgroundColor={cssVar("--background-primary")}
            enableNodeDrag
            cooldownTicks={120}
            warmupTicks={60}
            autoPauseRedraw={false}
            d3AlphaDecay={0.015}
            d3VelocityDecay={0.35}
            linkColor={(l: FgLink) => {
              if (linkHighlighted(l)) return cssVar("--interactive-accent");
              if (filteredMatch) return withAlpha(cssVar("--text-faint"), 0.1);
              // Thin Obsidian-style gray hairlines — visible but quiet.
              return withAlpha(cssVar("--text-muted"), 0.4);
            }}
            linkWidth={(l: FgLink) => (linkHighlighted(l) ? 1.8 : 0.6)}
            linkDirectionalParticles={(l: FgLink) => (linkHighlighted(l) ? 4 : 0)}
            linkDirectionalParticleSpeed={() => 0.008}
            linkDirectionalParticleWidth={() => 2.2}
            linkDirectionalParticleColor={() => cssVar("--interactive-accent")}
            nodeLabel={(n: FgNode) => n.name}
            onNodeHover={(n: FgNode | null) => setHovered(n?.id ?? null)}
            onNodeClick={(n: FgNode) => { onOpenFile(n.id); onClose(); }}
            nodeCanvasObject={(n: FgNode, ctx, scale) => {
              const id = n.id;
              const isActive = id === activePath;
              const isHovered = id === hovered;
              const baseR = nodeR(n);
              const dim = isDimmed(id);
              const hi = isHighlighted(id);
              const accent = cssVar("--interactive-accent");
              const fgNormal = cssVar("--text-normal");
              const fgMuted = cssVar("--text-muted");

              // Time-based pulse for hovered/active nodes. 1.2s cycle.
              const t = (Date.now() % 1200) / 1200;
              const pulse = 0.5 + 0.5 * Math.sin(t * Math.PI * 2);

              // Concentric halo rings on hover — the "circulation" look
              // without needing per-node particle animators.
              if (isHovered) {
                const haloMax = baseR + 10 + pulse * 6;
                const g = ctx.createRadialGradient(n.x!, n.y!, baseR, n.x!, n.y!, haloMax);
                g.addColorStop(0, withAlpha(accent, 0.35));
                g.addColorStop(1, withAlpha(accent, 0));
                ctx.fillStyle = g;
                ctx.beginPath();
                ctx.arc(n.x!, n.y!, haloMax, 0, 2 * Math.PI);
                ctx.fill();
                // Outer ring that breathes
                ctx.beginPath();
                ctx.arc(n.x!, n.y!, baseR + 5 + pulse * 2, 0, 2 * Math.PI);
                ctx.strokeStyle = withAlpha(accent, 0.55);
                ctx.lineWidth = 1.2;
                ctx.stroke();
              }

              // Fill
              const r = isHovered ? baseR + 0.8 : baseR;
              ctx.beginPath();
              ctx.arc(n.x!, n.y!, r, 0, 2 * Math.PI);
              ctx.fillStyle = isHovered || isActive
                ? accent
                : hi
                  ? withAlpha(accent, 0.85)
                  : dim
                    ? withAlpha(fgMuted, 0.22)
                    : withAlpha(fgNormal, 0.78);
              ctx.fill();

              // Steady amber ring on the active note (where you are).
              if (isActive && !isHovered) {
                ctx.beginPath();
                ctx.arc(n.x!, n.y!, r + 2.5, 0, 2 * Math.PI);
                ctx.strokeStyle = accent;
                ctx.lineWidth = 1.2;
                ctx.stroke();
              }

              // Labels: always-on but scale up when zoomed in or
              // highlighted. Size is readable at default zoom (~11px
              // effective, independent of canvas scale).
              const wantsEmphasis = isHovered || isActive || hi;
              const baseFont = wantsEmphasis ? 12 : 10.5;
              const showLabel = wantsEmphasis || scale > 0.55;
              if (showLabel) {
                const fontSize = baseFont / Math.max(scale, 0.75);
                ctx.font = `${wantsEmphasis ? 600 : 500} ${fontSize}px Inter, system-ui, sans-serif`;
                ctx.textAlign = "center";
                ctx.textBaseline = "top";
                ctx.fillStyle = dim
                  ? withAlpha(fgMuted, 0.45)
                  : wantsEmphasis
                    ? fgNormal
                    : withAlpha(fgNormal, 0.7);
                ctx.fillText(n.name, n.x!, n.y! + r + 3);
              }
            }}
            nodePointerAreaPaint={(n: FgNode, color, ctx) => {
              ctx.fillStyle = color;
              ctx.beginPath();
              ctx.arc(n.x!, n.y!, nodeR(n) + 4, 0, 2 * Math.PI);
              ctx.fill();
            }}
          />
        )}
        {!graph && (
          <div className="absolute inset-0 flex items-center justify-center text-[12px] text-[var(--text-faint)]">
            Building graph...
          </div>
        )}
        {graph && graph.nodes.length === 0 && (
          <div className="absolute inset-0 flex items-center justify-center text-[12px] text-[var(--text-faint)]">
            No notes in the vault yet.
          </div>
        )}
      </div>
    </div>
  );
}

// Resolve a CSS custom property to its concrete value. Needed because
// the force-graph canvas is raw Canvas2D, not styled via CSS.
function cssVar(name: string, fallback = "#888"): string {
  if (typeof window === "undefined") return fallback;
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return v || fallback;
}

// Apply alpha to a color string. Handles hex, rgb/rgba, and hsl/hsla.
// Forge's palette is HSL so this MUST handle hsl, not just hex.
function withAlpha(color: string, alpha: number): string {
  const c = color.trim();
  if (c.startsWith("#")) {
    const hex = c.slice(1);
    const full = hex.length === 3
      ? hex.split("").map((ch) => ch + ch).join("")
      : hex;
    const r = parseInt(full.slice(0, 2), 16);
    const g = parseInt(full.slice(2, 4), 16);
    const b = parseInt(full.slice(4, 6), 16);
    return `rgba(${r}, ${g}, ${b}, ${alpha})`;
  }
  const rgbMatch = c.match(/^rgba?\s*\(([^)]+)\)/);
  if (rgbMatch) {
    const parts = rgbMatch[1].split(",").map((s) => s.trim());
    const [r, g, b] = parts;
    return `rgba(${r}, ${g}, ${b}, ${alpha})`;
  }
  const hslMatch = c.match(/^hsla?\s*\(([^)]+)\)/);
  if (hslMatch) {
    const parts = hslMatch[1].split(",").map((s) => s.trim());
    const [h, s, l] = parts;
    return `hsla(${h}, ${s}, ${l}, ${alpha})`;
  }
  return `rgba(0,0,0,${alpha})`;
}
