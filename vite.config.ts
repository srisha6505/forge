import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// @ts-ignore process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

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
        "**/lib/**",
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
