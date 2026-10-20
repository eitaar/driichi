import { defineConfig } from "vite";
import { resolve } from "node:path";
import productConfig from "../../vite.config.ts";

export default defineConfig({
  ...productConfig,
  build: { emptyOutDir: false, rolldownOptions: { input: [resolve(import.meta.dirname, "renderer-worker-product.html"), resolve(import.meta.dirname, "../../index.html")] } },
  worker: { format: "es" },
  preview: {
    headers: { "Content-Security-Policy": "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'; media-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none'" },
  },
});
