import { defineConfig } from "@playwright/test";

const dist = process.env.DRIICHI_WORKER_PROBE_DIST;
if (!dist) throw new Error("An exclusive worker probe build directory is required");

export default defineConfig({
  testDir: "./tests",
  testMatch: ["renderer-worker.spec.ts", "task12.spec.ts"],
  workers: 1,
  fullyParallel: false,
  retries: 0,
  reporter: [["list"]],
  use: { baseURL: "http://127.0.0.1:4173", channel: "chromium", trace: "off", screenshot: "only-on-failure" },
  webServer: {
    command: `npm exec -- vite preview --config tests/support/renderer-worker-build.config.ts --outDir "${dist}" --host 127.0.0.1 --port 4173 --strictPort`,
    url: "http://127.0.0.1:4173/tests/support/renderer-worker-product.html",
    reuseExistingServer: false,
  },
});
