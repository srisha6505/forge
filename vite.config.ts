import { defineConfig } from "vite";
// SWC backend instead of Babel. Identical API surface for our use, but
// HMR is ~5-10× faster and dev-server cold start drops noticeably. No
// runtime impact — the swap only affects how Vite transforms TSX.
import react from "@vitejs/plugin-react-swc";

// @ts-ignore process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// react-pdf and the top-level hoisted pdfjs-dist are now version-aligned
// (both 5.4.296), so the previous alias to a nested copy is no longer
// needed — npm hoisting puts a single pdfjs-dist at the top level. If a
// future version mismatch reappears, restore the alias.

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // Vite should never watch the Rust target directory (millions of
      // build artifacts from llama-cpp shaders blow past the inotify
      // limit), the legacy GPUI source, or any node/cargo state.
      ignored: [
        "**/src-tauri/**",
        "**/target/**",
        "**/target-old/**",
        "**/src-old/**",
        "**/node_modules/**",
        "**/Cargo.lock",
        "**/Cargo.lock.old",
        "**/Cargo.toml.old",
        // NOTE: do NOT ignore "**/lib/**" — that pattern matches
        // src/lib/ where our CodeMirror extensions live, and edits
        // there silently miss Vite's watcher.
        "**/.cargo/**",
        "**/dist/**",
      ],
    },
  },
  resolve: {
    alias: {
      "@": "/src",
    },
  },
  build: {
    // Tauri's webview is always recent (webkit2gtk 2.50+, WebView2,
    // WKWebView). Modern ES target lets esbuild skip down-level
    // transforms that old browsers need.
    target: "es2022",
    // Splitting heavy vendors into their own chunks: webkit2gtk parses
    // chunks in parallel and the user's HTTP cache survives across
    // rebuilds for anything they didn't touch. Pre-split the bundle
    // showed a 5.4 MB monolithic index-*.js which dominated cold-start
    // parse cost on webkit2gtk; carving out the four heaviest gets that
    // down to ~1.5 MB main + parallel vendor chunks.
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes("node_modules")) return undefined;
          if (id.includes("/mermaid/")) return "vendor-mermaid";
          if (id.includes("/cytoscape")) return "vendor-cytoscape";
          if (id.includes("/@codemirror/") || id.includes("/codemirror/"))
            return "vendor-codemirror";
          if (id.includes("/katex/")) return "vendor-katex";
          if (id.includes("/highlight.js/")) return "vendor-highlight";
          if (id.includes("/react-pdf/") || id.includes("/pdfjs-dist/"))
            return "vendor-pdf";
          if (
            id.includes("/react-markdown/") ||
            id.includes("/remark") ||
            id.includes("/rehype") ||
            id.includes("/micromark") ||
            id.includes("/mdast") ||
            id.includes("/hast")
          )
            return "vendor-markdown";
          if (id.includes("/d3-") || id.includes("/react-force-graph"))
            return "vendor-graph";
          if (id.includes("/lucide-react/")) return "vendor-icons";
          if (id.includes("/mammoth/")) return "vendor-docx";
          if (
            id.includes("/react/") ||
            id.includes("/react-dom/") ||
            id.includes("/scheduler/")
          )
            return "vendor-react";
          return undefined;
        },
      },
    },
  },
}));
