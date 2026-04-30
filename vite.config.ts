import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

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
}));
