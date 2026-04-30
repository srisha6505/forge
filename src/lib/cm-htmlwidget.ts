// Interactive HTML widgets inside markdown.
//
// A fenced code block whose info string starts with `html-widget` is
// replaced with a sandboxed `<iframe srcdoc>` carrying the user's HTML
// + CSS + JS. The iframe runs with `sandbox="allow-scripts"` ONLY:
// no same-origin, no forms, no top navigation, no network beyond what
// data: URLs allow. Even if an LLM emits hostile code the blast radius
// is contained to that iframe.
//
// Markdown stays portable — open the file in any editor and you see a
// regular fenced code block. The widget is purely a render-side affordance.
//
// Info-string syntax:
//   ```html-widget                       (default 400px tall)
//   ```html-widget height=480            (custom height)
//
// PRE-INJECTED RUNTIME (always available inside the iframe, no imports):
//   - Tailwind CSS (Play CDN, full JIT) — class="bg-slate-900 p-4 …" works
//   - Preact + Hooks bound as globals: h, render, Component, Fragment,
//     useState, useEffect, useRef, useMemo, useCallback, useReducer
//   - HTM bound to Preact's h, exposed as global `html` — write
//       html`<div class="p-4">${count}</div>`
//     instead of JSX. No transpile step.
//
// THEME PARITY: bg/text colors on html+body are forced via !important so
// agents that hard-code `body { background: white }` cannot break dark mode.
// All `--color-*` and Obsidian-style `--background-primary` vars are forwarded.
// Theme switches require a document edit to refresh (StateField is built
// from EditorState only — we trade live theme reactivity for simplicity).
//
// CANVAS AUTO-FIX: <canvas> elements without explicit width/height attrs
// get resized to their bounding box on DOMContentLoaded. Counters the
// most common LLM bug (forgetting to set canvas dimensions, getting a
// silent 300×150 default).

import { syntaxTree, syntaxTreeAvailable } from "@codemirror/language";
import {
  type EditorState,
  type Extension,
  RangeSetBuilder,
  StateField,
} from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  WidgetType,
} from "@codemirror/view";
import { activeLinesField } from "./cm-active-lines";
import preactSrc from "./widget-runtime/preact.js?raw";
import preactHooksSrc from "./widget-runtime/preact-hooks.js?raw";
import htmSrc from "./widget-runtime/htm.js?raw";
import tailwindSrc from "./widget-runtime/tailwind.js?raw";
import jsxgraphSrc from "./widget-runtime/jsxgraph.js?raw";
import jsxgraphCss from "./widget-runtime/jsxgraph.css?raw";
import chartSrc from "./widget-runtime/chart.js?raw";
import p5Src from "./widget-runtime/p5.js?raw";

const DEFAULT_HEIGHT_PX = 320;
const MIN_HEIGHT_PX = 80;
// Hard cap. Widgets taller than this clip (the iframe sets overflow:hidden
// + scrolling="no", so there is no internal scroll). 600px gives a generous
// canvas for richer widgets while still keeping controls and visualisation
// in the same viewport on a normal screen. Content beyond this is invisible
// — the contract is "design widgets compactly", not "let them grow forever".
const MAX_HEIGHT_PX = 600;
// Hard cap on how many widgets a single note may render. Beyond this,
// further html-widget blocks render an inert placeholder. Each widget is
// its own iframe with its own JS context — running 15+ of them tanks
// scroll, RAM, and battery. The right answer is to split the topic into
// multiple notes, not to make the runtime forgive abuse.
const MAX_WIDGETS_PER_DOC = 8;

// Tokens we forward into the widget. Keep this list explicit so the
// widget contract is documented — these are the names agents should
// generate against.
const FORWARDED_TOKENS = [
  "--background-primary",
  "--background-primary-alt",
  "--background-secondary",
  "--background-modifier-border",
  "--background-modifier-hover",
  "--text-normal",
  "--text-muted",
  "--text-faint",
  "--text-accent",
  "--text-error",
  "--text-success",
  "--text-warning",
  "--text-info",
  "--text-link",
  "--interactive-accent",
  "--interactive-accent-hover",
  "--code-background",
  "--font-text",
  "--font-monospace",
  "--font-interface",
];

function readToken(name: string, fallback: string): string {
  if (typeof document === "undefined") return fallback;
  // Forge applies `.theme-dark` / `.theme-light` to <body>, NOT <html>.
  // Reading from documentElement would always see the `:root` defaults
  // (light) and break dark-mode parity. Try body first, fall back to
  // documentElement only if body isn't ready (very early init).
  const target = document.body ?? document.documentElement;
  const v = getComputedStyle(target).getPropertyValue(name).trim();
  return v || fallback;
}

// Build a small <style> block that exposes BOTH our internal token
// names AND a friendlier `--color-*` alias set so widgets written
// against either convention work. Background + text on html/body are
// `!important` so agents that hard-code `background: white` cannot
// defeat theme parity.
function buildThemeCss(): string {
  const lines: string[] = [];
  for (const t of FORWARDED_TOKENS) lines.push(`${t}: ${readToken(t, "")};`);
  lines.push(`--color-bg: ${readToken("--background-primary", "#fff")};`);
  lines.push(`--color-bg-alt: ${readToken("--background-primary-alt", "#f5f5f5")};`);
  lines.push(`--color-text: ${readToken("--text-normal", "#222")};`);
  lines.push(
    `--color-text-secondary: ${readToken("--text-muted", "#555")};`,
  );
  lines.push(`--color-accent: ${readToken("--interactive-accent", "#c08a2e")};`);
  lines.push(`--color-link: ${readToken("--text-link", "#c08a2e")};`);
  lines.push(`--color-error: ${readToken("--text-error", "#d04030")};`);
  lines.push(`--color-success: ${readToken("--text-success", "#3a8a3a")};`);
  lines.push(`--color-border: ${readToken("--background-modifier-border", "#ddd")};`);
  return `:root { ${lines.join(" ")} }
    *, *::before, *::after { box-sizing: border-box; }
    /* Suppress widget-internal scroll. The iframe auto-resizes to fit content
       (up to MAX_HEIGHT_PX); content beyond that is clipped, NOT scrolled.
       A scrollbar inside the widget breaks the user's scroll context — they
       expect to scroll the doc, not nest a second viewport. If a widget
       legitimately needs more space, the right answer is to redesign the
       widget compactly, not to give it its own scroll. */
    html, body { overflow: hidden !important; }
    html { background: var(--color-bg) !important; color: var(--color-text) !important; }
    body { margin: 0 !important; padding: 12px !important;
           background: var(--color-bg) !important;
           color: var(--color-text) !important;
           font-family: var(--font-text), system-ui, sans-serif;
           font-size: 14px; line-height: 1.5; }
    button { font: inherit; padding: 4px 10px;
             background: var(--color-bg-alt);
             color: var(--color-text);
             border: 1px solid var(--color-border);
             border-radius: 4px; cursor: pointer; }
    button:hover { background: var(--color-accent); color: var(--color-bg); border-color: var(--color-accent); }
    canvas { display: block; max-width: 100%; }
    a { color: var(--color-link); }`;
}

// Glue script: bind ergonomic globals so widget code can use
//   html`<div/>`, useState, render, h
// directly without any import boilerplate. Mirrors the conventions used
// by Claude artifacts so the LLM can reuse known patterns.
const RUNTIME_GLUE = `(function(){
  if (!window.preact || !window.preactHooks || !window.htm) return;
  var p = window.preact, ph = window.preactHooks;
  window.html = window.htm.bind(p.h);
  window.h = p.h; window.render = p.render;
  window.Component = p.Component; window.Fragment = p.Fragment;
  window.useState = ph.useState; window.useEffect = ph.useEffect;
  window.useRef = ph.useRef; window.useMemo = ph.useMemo;
  window.useCallback = ph.useCallback; window.useReducer = ph.useReducer;
})();`;

// Auto-resize <canvas> elements that didn't get explicit width/height
// attributes — by far the most common LLM mistake (silent 300x150 default
// gets stretched by CSS, draws appear off-screen). Runs once after parse.
const CANVAS_AUTOFIX = `document.addEventListener('DOMContentLoaded', function() {
  document.querySelectorAll('canvas').forEach(function(c) {
    var r = c.getBoundingClientRect();
    if (!c.hasAttribute('width'))  c.width  = Math.max(1, Math.round(r.width  || 800));
    if (!c.hasAttribute('height')) c.height = Math.max(1, Math.round(r.height || 400));
  });
});`;

// Auto-size the iframe to fit content. Measures document.body's full
// scrollHeight then posts to the parent which resizes the <iframe>.
//
// One-shot strategy: take 5 measurements at staggered intervals after
// load (covers async font/image/Tailwind-JIT settling), then STOP.
// After ~1.5s the layout is stable; running a permanent ResizeObserver
// past that point only feeds the iframe -> parent -> iframe reflow loop
// during page scrolls. Trade: if the widget content grows AFTER 1.5s
// (e.g. user opens a collapsible panel inside it), parent height won't
// follow. Acceptable — scroll smoothness wins.
const SIZE_REPORTER = `(function(){
  var lastH = 0;
  function measure() {
    var b = document.body;
    if (!b) return 0;
    return Math.max(b.scrollHeight, b.offsetHeight, document.documentElement.scrollHeight);
  }
  function report() {
    var h = measure();
    if (h <= 0 || Math.abs(h - lastH) < 4) return;
    lastH = h;
    try { parent.postMessage({ type: 'forge-widget-size', h: h }, '*'); } catch (e) {}
  }
  document.addEventListener('DOMContentLoaded', function(){
    report();
    setTimeout(report, 80);
    setTimeout(report, 320);
    setTimeout(report, 800);
    setTimeout(report, 1500);
  });
})();`;

// Forge widget kit — small set of pre-built Preact components registered
// as globals so the LLM can use a friendly vocabulary instead of hand-
// wiring every input/readout/panel from scratch. Themed to Forge's
// palette via the same `--color-*` vars exposed by buildThemeCss().
//
// Every component is a Preact functional component. Agents use them as:
//   html`<${Slider} label="Mass" value=${m} setValue=${setM} min=0 max=10/>`
//
// Plot wraps Chart.js. It is a NO-OP when `needs=chart` was not declared
// (logs an error so the agent sees what's missing).
const WIDGET_KIT = `(function(){
  if (typeof window.html !== 'function') return;
  var html = window.html, useEffect = window.useEffect, useRef = window.useRef;

  // Slider: label + range input + live numeric readout. The value is a
  // number; setValue receives a number. Accent uses Forge's amber.
  window.Slider = function(props){
    var label = props.label, value = props.value, setValue = props.setValue;
    var min = props.min != null ? props.min : 0;
    var max = props.max != null ? props.max : 1;
    var step = props.step != null ? props.step : 0.01;
    var unit = props.unit || '';
    var digits = props.digits != null ? props.digits : 2;
    var disp = (typeof value === 'number') ? value.toFixed(digits) : String(value);
    return html\`<label class="grid grid-cols-[8rem_1fr_5rem] gap-3 items-center py-1.5 text-sm">
      <span class="opacity-70">\${label}</span>
      <input type="range" min=\${min} max=\${max} step=\${step} value=\${value}
             onInput=\${function(e){ setValue(parseFloat(e.target.value)); }}
             class="w-full accent-amber-500"/>
      <span class="text-right font-mono">\${disp}\${unit ? ' ' + unit : ''}</span>
    </label>\`;
  };

  // Readout: large monospace value display with label and unit.
  window.Readout = function(props){
    var label = props.label, value = props.value, unit = props.unit || '';
    var digits = props.digits != null ? props.digits : 2;
    var disp;
    if (typeof props.format === 'function') disp = props.format(value);
    else if (typeof value === 'number') {
      var abs = Math.abs(value);
      disp = (abs !== 0 && (abs < 1e-3 || abs >= 1e6)) ? value.toExponential(digits) : value.toFixed(digits);
    } else disp = String(value);
    return html\`<div class="flex items-baseline gap-2 py-2 font-mono">
      <span class="text-xs uppercase tracking-wider opacity-60">\${label}</span>
      <span class="text-3xl">\${disp}</span>
      \${unit ? html\`<span class="text-sm opacity-60">\${unit}</span>\` : null}
    </div>\`;
  };

  // Panel: titled section with subtle border + alt background.
  window.Panel = function(props){
    return html\`<div class="rounded-lg border p-4 my-2"
         style="border-color:var(--color-border);background:var(--color-bg-alt);">
      \${props.title ? html\`<div class="text-xs uppercase tracking-wider opacity-60 mb-3">\${props.title}</div>\` : null}
      \${props.children}
    </div>\`;
  };

  // Grid: simple n-column auto-grid layout.
  window.Grid = function(props){
    var cols = props.cols || 2, gap = props.gap != null ? props.gap : 4;
    var style = 'display:grid; gap:' + (gap * 0.25) + 'rem; grid-template-columns:repeat(' + cols + ', minmax(0, 1fr));';
    return html\`<div style=\${style}>\${props.children}</div>\`;
  };

  // Plot: thin Chart.js wrapper. Requires \`needs=chart\` in info string.
  // Accepts THREE input shapes — pick whichever the model wrote:
  //   1. Chart.js native: data={labels:[...], datasets:[{label,data,...}]}
  //   2. data=[{name, values:[{x,y}]}]   (gpt-5.2's invented shape)
  //   3. series=[{points:[{x,y}], label, color}]   (gpt-4o's invented shape)
  // Normalised internally to (1). Models hallucinate the API often;
  // accepting all three removes a whole class of "no curve, just axes"
  // failures that look identical from the outside.
  window.Plot = function(props){
    var ref = useRef(null);
    var instance = useRef(null);
    function normaliseData() {
      var d = props.data;
      var s = props.series;
      // Shape 3 — series=[{points,label,color}]
      if (Array.isArray(s) && s.length && s[0] && Array.isArray(s[0].points)) {
        var labels = (s[0].points || []).map(function(p){ return p.x; });
        var datasets = s.map(function(ser){
          return {
            label: ser.label || ser.name || '',
            data: (ser.points || []).map(function(p){ return p.y; }),
            borderColor: ser.color,
            backgroundColor: ser.color,
            borderWidth: 2,
            tension: 0.1,
            fill: false,
          };
        });
        return { labels: labels, datasets: datasets };
      }
      // Shape 4: array of series, each has a 'label' string and a
      // 'data' array of {x,y} objects (gpt-5.2 v2 invention).
      // Distinguish from Chart.js native (where data.datasets exists)
      // by requiring an array whose first item has a 'data' array of
      // x/y-shaped objects.
      if (Array.isArray(d) && d.length && d[0] && Array.isArray(d[0].data)
          && d[0].data.length && d[0].data[0] && typeof d[0].data[0] === 'object'
          && 'x' in d[0].data[0] && 'y' in d[0].data[0]) {
        var labels4 = d[0].data.map(function(p){ return p.x; });
        var datasets4 = d.map(function(ser){
          return {
            label: ser.label || ser.name || '',
            data: ser.data.map(function(p){ return p.y; }),
            borderColor: ser.color || ser.borderColor,
            backgroundColor: ser.color || ser.backgroundColor,
            borderWidth: 2,
            tension: 0.1,
            fill: false,
          };
        });
        return { labels: labels4, datasets: datasets4 };
      }
      // Shape 2 — data=[{name, values:[{x,y}]}]
      if (Array.isArray(d) && d.length && d[0] && Array.isArray(d[0].values)) {
        var labels2 = (d[0].values || []).map(function(p){ return p.x; });
        var datasets2 = d.map(function(ser){
          return {
            label: ser.label || ser.name || '',
            data: (ser.values || []).map(function(p){ return p.y; }),
            borderWidth: 2,
            tension: 0.1,
            fill: false,
          };
        });
        return { labels: labels2, datasets: datasets2 };
      }
      // Shape 1 — already Chart.js native, pass through.
      return d;
    }
    useEffect(function(){
      if (typeof window.Chart === 'undefined') {
        console.error('[forge-widget] <Plot/> requires needs=chart in the html-widget info string');
        return;
      }
      if (instance.current) instance.current.destroy();
      var chartData = normaliseData();
      instance.current = new window.Chart(ref.current, {
        type: props.type || 'line',
        data: chartData,
        options: Object.assign({
          responsive: true,
          maintainAspectRatio: false,
          plugins: { legend: { labels: { color: getComputedStyle(document.body).getPropertyValue('--color-text') } } },
          scales: {
            x: { title: { display: !!props.xLabel, text: props.xLabel || '',
                          color: getComputedStyle(document.body).getPropertyValue('--color-text-secondary') },
                 ticks: { color: getComputedStyle(document.body).getPropertyValue('--color-text-secondary') },
                 grid:  { color: getComputedStyle(document.body).getPropertyValue('--color-border') } },
            y: { title: { display: !!props.yLabel, text: props.yLabel || '',
                          color: getComputedStyle(document.body).getPropertyValue('--color-text-secondary') },
                 ticks: { color: getComputedStyle(document.body).getPropertyValue('--color-text-secondary') },
                 grid:  { color: getComputedStyle(document.body).getPropertyValue('--color-border') } }
          }
        }, props.options || {})
      });
      return function(){ if (instance.current) { instance.current.destroy(); instance.current = null; } };
    }, [JSON.stringify(props.data), JSON.stringify(props.series), JSON.stringify(props.options), props.type]);
    var height = props.height || 300;
    return html\`<div style=\${'position:relative;height:' + height + 'px;width:100%;'}><canvas ref=\${ref}/></div>\`;
  };
})();`;

type Parsed = {
  isWidget: boolean;
  height: number;
  needs: ReadonlyArray<string>;
};

const KNOWN_NEEDS = new Set(["jsxgraph", "chart", "p5"]);

function parseInfo(infoLine: string): Parsed {
  // Info line still has the opening ``` prefix when sliced from the
  // source. Strip leading fence + whitespace.
  const after = infoLine.replace(/^\s*(?:```+|~~~+)\s*/, "");
  if (!/^(?:js-widget|html-widget)(\b|$)/.test(after))
    return { isWidget: false, height: DEFAULT_HEIGHT_PX, needs: [] };
  let height = DEFAULT_HEIGHT_PX;
  const heightMatch = after.match(/\bheight\s*=\s*(\d+)/i);
  if (heightMatch) {
    const n = parseInt(heightMatch[1], 10);
    if (Number.isFinite(n)) {
      height = Math.max(MIN_HEIGHT_PX, Math.min(MAX_HEIGHT_PX, n));
    }
  }
  // needs=jsxgraph,chart,p5 — comma-separated, tolerant of spaces.
  // Unknown values are silently ignored to keep the contract forgiving.
  let needs: string[] = [];
  const needsMatch = after.match(/\bneeds\s*=\s*([a-z0-9,\s]+)/i);
  if (needsMatch) {
    needs = needsMatch[1]
      .split(",")
      .map((s) => s.trim().toLowerCase())
      .filter((s) => KNOWN_NEEDS.has(s));
  }
  return { isWidget: true, height, needs };
}

// Build the lazy-injected lib block based on the widget's `needs=` flag.
// Order matters: stylesheets first, then scripts. Each lib is only
// included if explicitly requested — keeps simple widgets cheap.
function buildLazyLibs(needs: ReadonlyArray<string>): string {
  const parts: string[] = [];
  if (needs.includes("jsxgraph")) {
    parts.push(`<style>${jsxgraphCss}</style>`);
    parts.push(`<script>${jsxgraphSrc}</script>`);
  }
  if (needs.includes("chart")) parts.push(`<script>${chartSrc}</script>`);
  if (needs.includes("p5")) parts.push(`<script>${p5Src}</script>`);
  return parts.join("\n");
}

// One iframe per widget. Carries the user HTML, the active theme CSS,
// and any opt-in heavy libs declared via `needs=` in the info string.
class HtmlWidget extends WidgetType {
  constructor(
    readonly src: string,
    readonly height: number,
    readonly themeCss: string,
    readonly needs: ReadonlyArray<string>,
  ) {
    super();
  }
  eq(other: HtmlWidget) {
    return (
      other.src === this.src &&
      other.height === this.height &&
      other.themeCss === this.themeCss &&
      other.needs.length === this.needs.length &&
      other.needs.every((v, i) => v === this.needs[i])
    );
  }
  toDOM() {
    const wrap = document.createElement("div");
    wrap.className = "cm-htmlwidget-wrap";
    // Browser-level offscreen optimisation: `content-visibility: auto`
    // tells the browser to entirely skip layout/paint for this subtree
    // when it's outside the viewport. `contain-intrinsic-size` gives the
    // browser a placeholder size so scroll height stays accurate without
    // needing the iframe to actually layout. Combined with the
    // IntersectionObserver below this is the dominant scroll-perf win
    // when a doc has 5+ widgets — the browser does the right thing
    // natively without JS bookkeeping on every scroll tick.
    wrap.style.contentVisibility = "auto";
    wrap.style.containIntrinsicSize = `${this.height}px`;
    // Containment isolates this subtree's layout/paint cost from the
    // rest of the editor. Edits here can't trigger reflow elsewhere.
    wrap.style.contain = "layout paint";
    const iframe = document.createElement("iframe");
    iframe.className = "cm-htmlwidget-frame";
    // sandbox="allow-scripts" ONLY. Without allow-same-origin the
    // widget cannot read cookies, vault files, parent DOM, etc.
    iframe.setAttribute("sandbox", "allow-scripts");
    iframe.setAttribute("loading", "lazy");
    iframe.setAttribute("referrerpolicy", "no-referrer");
    // Belt-and-suspenders against widget-internal scroll. CSS overflow:hidden
    // in the iframe body handles modern browsers; this attribute covers the
    // legacy paths and removes the scrollbar gutter even before CSS loads.
    iframe.setAttribute("scrolling", "no");
    iframe.style.width = "100%";
    iframe.style.height = `${this.height}px`;
    // No border / radius — the iframe should disappear into the markdown.
    // Background matches the page so it blends seamlessly even before the
    // widget content has loaded.
    iframe.style.border = "none";
    iframe.style.background = "var(--background-primary)";
    iframe.style.display = "block";

    // Build the srcdoc up front so we can detach + restore on viewport
    // entry/exit (see IntersectionObserver below). Off-screen iframes
    // otherwise keep Tailwind JIT + Preact runtime hot, eating layout
    // /paint cost on every editor scroll — the dominant scroll-perf
    // win when a doc has 3+ widgets.
    //
    // Load order:
    //  1. theme CSS (defines --color-* before user CSS reads them)
    //  2. preact -> hooks (hooks UMD looks up window.preact at load)
    //  3. htm
    //  4. runtime glue (binds html/useState/render globals)
    //  5. widget kit (Slider/Readout/Panel/Grid/Plot — depends on glue)
    //  6. tailwind play (sets up MutationObserver — finds classes after body parse)
    //  7. lazy libs (only if declared via `needs=`)
    //  8. canvas auto-fix + size reporter (DOMContentLoaded handlers)
    //  9. user body
    const lazyBlock = buildLazyLibs(this.needs);
    const fullSrcdoc = `<!doctype html>
<html><head><meta charset="utf-8"/>
<style>${this.themeCss}</style>
<script>${preactSrc}</script>
<script>${preactHooksSrc}</script>
<script>${htmSrc}</script>
<script>${RUNTIME_GLUE}</script>
<script>${WIDGET_KIT}</script>
<script>${tailwindSrc}</script>
${lazyBlock}
<script>${CANVAS_AUTOFIX}</script>
<script>${SIZE_REPORTER}</script>
</head><body>${this.src}</body></html>`;
    // NOTE: srcdoc is intentionally NOT set yet — the IntersectionObserver
    // below assigns it the first time the wrapper enters the viewport.
    // This means a doc with 20 widgets only ever loads the 1-2 currently
    // on screen, instead of all 20 at first render.

    // Listen for height messages from the iframe (auto-resize). The
    // iframe's SIZE_REPORTER posts {type:'forge-widget-size', h: number}
    // on first paint and (throttled) on content size change. We clamp to
    // MIN/MAX_HEIGHT_PX to bound malicious or runaway sizes. The author's
    // explicit height=N still wins on first paint; auto-resize only kicks
    // in if the content needs more room.
    const onMessage = (e: MessageEvent) => {
      if (e.source !== iframe.contentWindow) return;
      const data = e.data as { type?: string; h?: number } | undefined;
      if (!data || data.type !== "forge-widget-size") return;
      const h = Math.max(MIN_HEIGHT_PX, Math.min(MAX_HEIGHT_PX, Math.round(data.h ?? 0)));
      if (!Number.isFinite(h) || h <= 0) return;
      iframe.style.height = h + "px";
    };
    window.addEventListener("message", onMessage);
    // Best-effort cleanup when the widget is replaced. CM6 doesn't call
    // destroy() reliably; relying on the iframe being GC'd is fine since
    // the listener checks `e.source !== iframe.contentWindow`.

    // Lifecycle: load srcdoc only when wrapper enters viewport, AND only
    // during browser idle time. Two-stage so the expensive mount (HTML
    // parse + Tailwind JIT + Preact) NEVER runs synchronously during a
    // scroll — that's the source of the "flash + jank" you'd see when
    // a below-fold widget enters the viewport mid-scroll.
    //
    // Stage 1: IntersectionObserver detects entry/exit. On entry, schedule
    // a deferred mount via requestIdleCallback (or setTimeout fallback).
    // On exit, cancel any pending schedule + tear down if mounted.
    //
    // Stage 2: the idle callback runs only when the main thread is free
    // — between scroll ticks, after layout, etc. It checks "still in
    // viewport" before committing. If the user scrolled past, it's a
    // no-op and the work was avoided entirely.
    //
    // rootMargin is 0px: only widgets actually entering the viewport
    // start the schedule. Combined with `content-visibility: auto` on
    // the wrapper, off-screen cost is essentially zero.
    if (typeof IntersectionObserver !== "undefined") {
      let mounted = false;
      let pending: number | null = null;
      let lastSeenIntersecting = false;
      const ric = (cb: () => void): number => {
        const w = window as unknown as {
          requestIdleCallback?: (
            cb: () => void,
            opts?: { timeout: number },
          ) => number;
        };
        if (typeof w.requestIdleCallback === "function") {
          return w.requestIdleCallback(cb, { timeout: 800 });
        }
        return window.setTimeout(cb, 200);
      };
      const cic = (id: number) => {
        const w = window as unknown as {
          cancelIdleCallback?: (id: number) => void;
        };
        if (typeof w.cancelIdleCallback === "function") {
          w.cancelIdleCallback(id);
        } else {
          window.clearTimeout(id);
        }
      };
      const cancelPending = () => {
        if (pending !== null) {
          cic(pending);
          pending = null;
        }
      };
      const scheduleMount = () => {
        if (mounted || pending !== null) return;
        pending = ric(() => {
          pending = null;
          // Re-check intersection at fire time — the user may have
          // scrolled past in the meantime, in which case mounting would
          // be wasted work.
          if (!lastSeenIntersecting || mounted) return;
          iframe.srcdoc = fullSrcdoc;
          mounted = true;
        });
      };
      // Deferred teardown: when the widget exits viewport, don't tear
      // down immediately. Instead schedule a teardown 8 seconds later.
      // If the user scrolls back into the widget within 8 s, we cancel
      // the teardown — no re-mount cost, scroll-up stays smooth. If the
      // widget genuinely stays off-screen, the timer fires and frees
      // its iframe document.
      let teardownTimer: number | null = null;
      const cancelTeardown = () => {
        if (teardownTimer !== null) {
          window.clearTimeout(teardownTimer);
          teardownTimer = null;
        }
      };
      const scheduleTeardown = () => {
        cancelTeardown();
        if (!mounted) return;
        teardownTimer = window.setTimeout(() => {
          teardownTimer = null;
          // Re-check: still off-screen? Otherwise abort.
          if (lastSeenIntersecting || !mounted) return;
          iframe.srcdoc = "<!doctype html><html><body></body></html>";
          mounted = false;
        }, 8000);
      };
      const io = new IntersectionObserver(
        (entries) => {
          for (const entry of entries) {
            lastSeenIntersecting = entry.isIntersecting;
            if (entry.isIntersecting) {
              cancelTeardown();
              scheduleMount();
            } else {
              cancelPending();
              scheduleTeardown();
            }
          }
        },
        { rootMargin: "0px", threshold: 0 },
      );
      io.observe(wrap);
    } else {
      // No IntersectionObserver (very old browser) — fall back to eager load.
      iframe.srcdoc = fullSrcdoc;
    }

    wrap.appendChild(iframe);
    return wrap;
  }
  ignoreEvent() {
    // Let click/key events inside the iframe stay there. CM6 should
    // not steal them for cursor placement.
    return true;
  }
}

// Inert replacement when a doc exceeds MAX_WIDGETS_PER_DOC. No iframe,
// no JS — just a styled message telling the author to split the topic.
// The block remains in the source markdown so the author still owns the
// content; we just refuse to mount it as a live widget.
class WidgetCapPlaceholder extends WidgetType {
  constructor(readonly cap: number) {
    super();
  }
  eq(other: WidgetCapPlaceholder) {
    return other.cap === this.cap;
  }
  toDOM() {
    const wrap = document.createElement("div");
    wrap.className = "cm-htmlwidget-cap";
    wrap.style.cssText = `
      margin: 0.75rem 0; padding: 0.75rem 1rem;
      border: 1px dashed var(--background-modifier-border, #ccc);
      border-radius: 6px;
      background: var(--background-primary-alt, #f5f5f5);
      color: var(--text-muted, #666);
      font-size: 0.85em; line-height: 1.45;
    `.trim();
    wrap.textContent = `Widget limit reached (max ${this.cap} per note). Move further widgets to a new note for performance.`;
    return wrap;
  }
  ignoreEvent() {
    return false;
  }
}

interface DecoItem {
  from: number;
  to: number;
  deco: Decoration;
}

function buildAll(state: EditorState): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const doc = state.doc;
  const tree = syntaxTree(state);
  const items: DecoItem[] = [];
  const active = state.field(activeLinesField);
  const themeCss = buildThemeCss();

  tree.iterate({
    enter: (node) => {
      if (node.name !== "FencedCode") return;
      const openLine = doc.lineAt(node.from);
      const closeLine = doc.lineAt(Math.min(node.to, doc.length) - 1);
      const parsed = parseInfo(openLine.text);
      if (!parsed.isWidget) return false;

      // Cursor on any line of the fence: render raw markup so the
      // author can edit. Match the cm-codeblock convention.
      let cursorInside = false;
      for (let l = openLine.number; l <= closeLine.number; l++) {
        if (active.has(l)) {
          cursorInside = true;
          break;
        }
      }
      if (cursorInside) return false;

      const contentFrom = Math.min(openLine.to + 1, doc.length);
      const contentTo =
        closeLine.number > openLine.number
          ? Math.max(contentFrom, closeLine.from - 1)
          : doc.length;
      const src = doc.sliceString(contentFrom, contentTo);

      // Auto-detect lib needs from the widget body. The model frequently
      // puts `needs=chart` as a prop on the Plot tag instead of on the
      // codeblock fence, or just forgets to declare it at all. Rather
      // than fail with "Plot requires needs=chart" the runtime sniffs
      // the source and loads what's referenced. Belt-and-suspenders:
      // explicit `needs=` on the fence still works and takes priority.
      const sniffed = new Set<string>(parsed.needs);
      if (/\bPlot\b|\bnew\s+Chart\b|window\.Chart\b/.test(src)) sniffed.add("chart");
      if (/\bJSXBoard\b|\bJXG\.JSXGraph\b/.test(src)) sniffed.add("jsxgraph");
      if (/\bp5\s*\(|new\s+p5\b|window\.p5\b/.test(src)) sniffed.add("p5");
      const effectiveNeeds = Array.from(sniffed);

      // Replace the entire block — opener line through closer line +
      // its trailing newline — with the iframe widget. block:true
      // demands both ends at line boundaries.
      const replaceFrom = openLine.from;
      const replaceTo =
        closeLine.number === doc.lines
          ? doc.length
          : Math.min(closeLine.to + 1, doc.length);

      // Hard cap on widget count. Beyond MAX_WIDGETS_PER_DOC, render an
      // inert placeholder instead of mounting another iframe. Each widget
      // costs an iframe + duplicated lib JS (Chart.js, p5, etc.) + its own
      // JS heap, so 15+ in one doc tanks scroll perf and RAM. The contract
      // for the user: one note = one focused topic; if you need more, make
      // a new note. We surface this in the UI rather than silently mounting.
      if (items.length >= MAX_WIDGETS_PER_DOC) {
        items.push({
          from: replaceFrom,
          to: replaceTo,
          deco: Decoration.replace({
            widget: new WidgetCapPlaceholder(MAX_WIDGETS_PER_DOC),
            block: true,
          }),
        });
        return false;
      }

      items.push({
        from: replaceFrom,
        to: replaceTo,
        deco: Decoration.replace({
          widget: new HtmlWidget(src, parsed.height, themeCss, effectiveNeeds),
          block: true,
        }),
      });
      return false;
    },
  });

  // Tolerance pass: small models occasionally write the widget block as an
  // HTML tag (`<html-widget ...>...</html-widget>`) instead of a fenced
  // code block. The markdown parser sees that as inline HTML and renders
  // the JS as plain text. Rather than punish the user for the model's
  // confusion, we sniff the doc directly for that pattern and decorate
  // the matching range as a widget — same iframe, same rendering.
  //
  // Skip ranges already covered by a fenced widget so we never double-
  // decorate. The tag form has identical attribute syntax to the fence
  // (`height=N`, `needs=...`).
  const covered = items.map((it) => [it.from, it.to] as const);
  const isCovered = (from: number, to: number) =>
    covered.some(([cf, ct]) => from < ct && to > cf);

  const docText = doc.toString();
  // Accept the new canonical `<js-widget>` tag form AND the legacy
  // `<html-widget>` form (so old vault notes keep rendering).
  const tagOpen = /<(?:js-widget|html-widget)(?:\s[^>]*)?>/g;
  let m: RegExpExecArray | null;
  while ((m = tagOpen.exec(docText)) !== null) {
    const openTagFrom = m.index;
    const openTag = m[0];
    const isJs = openTag.startsWith("<js-widget");
    // Try every plausible close form. Whichever appears first wins.
    // - Canonical: </js-widget> or </html-widget>
    // - Common model typo: </html> when it shortened the tag name
    const searchFrom = openTagFrom + openTag.length;
    const candidates = isJs
      ? ["</js-widget>", "</html-widget>", "</html>"]
      : ["</html-widget>", "</html>"];
    let bestIdx = -1, bestText = "";
    for (const c of candidates) {
      const i = docText.indexOf(c, searchFrom);
      if (i !== -1 && (bestIdx === -1 || i < bestIdx)) { bestIdx = i; bestText = c; }
    }
    if (bestIdx === -1) continue;
    const closeIdx = bestIdx;
    const closeTagText = bestText;
    const closeEnd = closeIdx + closeTagText.length;
    if (isCovered(openTagFrom, closeEnd)) continue;

    // Open/close must each be on their own line for a block decoration.
    const openLine = doc.lineAt(openTagFrom);
    const closeLine = doc.lineAt(closeIdx);
    if (openLine.text.trim() !== openTag.trim()) continue;
    if (closeLine.text.trim() !== closeTagText) continue;

    let cursorInside = false;
    for (let l = openLine.number; l <= closeLine.number; l++) {
      if (active.has(l)) { cursorInside = true; break; }
    }
    if (cursorInside) continue;

    const tagParsed = parseInfo("```" + openTag.replace(/^</, "").replace(/>$/, ""));
    if (!tagParsed.isWidget) continue;

    const contentFrom = Math.min(openLine.to + 1, doc.length);
    const contentTo = Math.max(contentFrom, closeLine.from - 1);
    const src = doc.sliceString(contentFrom, contentTo);

    const sniffed = new Set<string>(tagParsed.needs);
    if (/\bPlot\b|\bnew\s+Chart\b|window\.Chart\b/.test(src)) sniffed.add("chart");
    if (/\bJSXBoard\b|\bJXG\.JSXGraph\b/.test(src)) sniffed.add("jsxgraph");
    if (/\bp5\s*\(|new\s+p5\b|window\.p5\b/.test(src)) sniffed.add("p5");

    const replaceFrom = openLine.from;
    const replaceTo =
      closeLine.number === doc.lines ? doc.length : Math.min(closeLine.to + 1, doc.length);

    if (items.length >= MAX_WIDGETS_PER_DOC) {
      items.push({
        from: replaceFrom,
        to: replaceTo,
        deco: Decoration.replace({
          widget: new WidgetCapPlaceholder(MAX_WIDGETS_PER_DOC),
          block: true,
        }),
      });
      continue;
    }

    items.push({
      from: replaceFrom,
      to: replaceTo,
      deco: Decoration.replace({
        widget: new HtmlWidget(src, tagParsed.height, themeCss, Array.from(sniffed)),
        block: true,
      }),
    });
  }

  items.sort((a, b) => a.from - b.from);
  for (const item of items) builder.add(item.from, item.to, item.deco);
  return builder.finish();
}

const htmlWidgetField = StateField.define<DecorationSet>({
  create(state) {
    if (!syntaxTreeAvailable(state)) return Decoration.none;
    try {
      return buildAll(state);
    } catch (e) {
      console.error("[cm-htmlwidget] build failed (create):", e);
      return Decoration.none;
    }
  },
  update(value, tr) {
    const treeChanged = syntaxTree(tr.startState) !== syntaxTree(tr.state);
    const activeChanged =
      tr.startState.field(activeLinesField, false) !==
      tr.state.field(activeLinesField, false);
    if (!tr.docChanged && !treeChanged && !activeChanged) return value;
    if (!syntaxTreeAvailable(tr.state)) return Decoration.none;
    try {
      return buildAll(tr.state);
    } catch (e) {
      console.error("[cm-htmlwidget] build failed (update):", e);
      return value;
    }
  },
  provide: (f) => EditorView.decorations.from(f),
});

const htmlWidgetTheme = EditorView.theme({
  ".cm-htmlwidget-wrap": {
    margin: "8px 0",
    width: "100%",
  },
  ".cm-htmlwidget-frame": {
    width: "100%",
    border: "none",
    background: "var(--background-primary)",
    colorScheme: "auto",
    display: "block",
  },
});

export const htmlWidgetExtension: Extension = [htmlWidgetField, htmlWidgetTheme];
