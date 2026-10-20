import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { resolve } from "node:path";

const e2eServerOrigin = process.env.DRIICHI_E2E_SERVER_ORIGIN;

export default defineConfig({
  resolve: { alias: { "@": resolve(import.meta.dirname, "src") } },
  // Scan the Worker too so cold optimization cannot reload an active Human session.
  optimizeDeps: { entries: ["index.html", "src/game/three-table-worker.tsx"] },
  // The native Worker shares these modules; Fast Refresh's runtime requires Window.
  plugins: [react({ exclude: [
    /\/node_modules\//,
    /\/src\/game\/(?:three-table-worker|three-table-scene|table-center|table-appearance)\.tsx$/,
  ] }), tailwindcss()],
  build: {
    manifest: true,
    rolldownOptions: {
      // Preserve the real store module for production-browser state checks.
      input: { app: resolve(import.meta.dirname, "index.html"), store: resolve(import.meta.dirname, "src/game/store.ts") },
      preserveEntrySignatures: "strict",
    },
  },
  server: {
    port: 4173,
    proxy: e2eServerOrigin
      ? {
          "/api": { target: e2eServerOrigin, changeOrigin: false },
          "/assets": { target: e2eServerOrigin, changeOrigin: false },
          "/ws": { target: e2eServerOrigin, changeOrigin: false, ws: true },
          "/status": { target: e2eServerOrigin, changeOrigin: false },
        }
      : undefined,
  },
  test: {
    maxWorkers: 2, // Bound jsdom/Three worker memory on 16 GB development machines.
    environment: "jsdom",
    setupFiles: "./src/test/setup.ts",
    include: ["src/**/*.test.{ts,tsx}"],
  },
});
