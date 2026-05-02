import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
// Self-hosted fonts. Each weight imported individually so we ship only
// the weights actually used by the UI; ordered light→bold within each
// family. Vite copies the .woff2 files into /assets and ships them with
// the rest of the bundle, so first paint is no longer gated on a
// network round trip to fonts.googleapis.com.
import "@fontsource/manrope/400.css";
import "@fontsource/manrope/500.css";
import "@fontsource/manrope/600.css";
import "@fontsource/manrope/700.css";
import "@fontsource/newsreader/400.css";
import "@fontsource/newsreader/600.css";
import "@fontsource/newsreader/700.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import "katex/dist/katex.min.css";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
