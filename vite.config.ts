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
  build: {
    target: "chrome111", // WebView2 on Windows 10/11 is evergreen Chromium
    sourcemap: false,
  },
});
