import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "node:path";

export default defineConfig({
  plugins: [react()],
  build: { emptyOutDir: false, rolldownOptions: { input: [resolve(import.meta.dirname, "renderer-worker-product.html"), resolve(import.meta.dirname, "../../index.html")] } },
  worker: { format: "es" },
  preview: {
    headers: { "Content-Security-Policy": "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'; media-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none'" },
  },
});
