import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Dev server proxies /api to the axum backend (just dev-server). In
// production the backend serves this bundle itself (rust-embed).
export default defineConfig({
  plugins: [svelte()],
  server: {
    port: 5173,
    // Listen on all interfaces so the dev server is reachable from other
    // machines on the LAN (e.g. the app running on a dev box).
    host: true,
    proxy: {
      "/api": "http://127.0.0.1:8080",
    },
  },
  build: {
    target: "es2022",
    sourcemap: true,
  },
});
