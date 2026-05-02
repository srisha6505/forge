// Widget-runtime source bundle. The ?raw imports below are statically
// bundled into THIS module's chunk by Vite. Because cm-htmlwidget.ts
// reaches us via a dynamic `import()` at first widget render, the
// resulting chunk (~2.6 MB of preact + tailwind + p5 + jsxgraph + chart
// strings) stays out of the main app bundle for users who never open
// a widget. Cold start cost: zero. First-widget cost: one chunk fetch
// + parse, after which the browser caches it for the session.
import preactSrc from "./preact.js?raw";
import preactHooksSrc from "./preact-hooks.js?raw";
import htmSrc from "./htm.js?raw";
import tailwindSrc from "./tailwind.js?raw";
import jsxgraphSrc from "./jsxgraph.js?raw";
import jsxgraphCss from "./jsxgraph.css?raw";
import chartSrc from "./chart.js?raw";
import p5Src from "./p5.js?raw";

export interface WidgetRuntimeSources {
  preact: string;
  preactHooks: string;
  htm: string;
  tailwind: string;
  jsxgraph: string;
  jsxgraphCss: string;
  chart: string;
  p5: string;
}

const sources: WidgetRuntimeSources = {
  preact: preactSrc,
  preactHooks: preactHooksSrc,
  htm: htmSrc,
  tailwind: tailwindSrc,
  jsxgraph: jsxgraphSrc,
  jsxgraphCss,
  chart: chartSrc,
  p5: p5Src,
};

export default sources;
