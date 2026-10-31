import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Tauri expects a fixed dev port; TAURI_ENV_* vars are set by the Tauri CLI.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) } },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
    // Browser-only development: `npm run bridge` serves the same Rust backend over HTTP.
    proxy: { "/bridge": "http://127.0.0.1:1430" },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  // Windows 7 runs the last WebView2 that supports it (109, Chromium 109): lightningcss rewrites
  // newer CSS (oklch colors, color-mix) into what Chromium 109 understands.
  css: { lightningcss: { targets: { chrome: 109 << 16 } } },
  build: {
    target: "chrome109",
    cssMinify: "lightningcss",
    sourcemap: false,
  },
});
