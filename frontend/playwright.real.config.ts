import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  testMatch: ["task16-real-server.spec.ts", "kyoku-results-real-server.spec.ts"],
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: [["list"], ["html", { outputFolder: "playwright-real-report", open: "never" }]],
  outputDir: "test-results/task-16-playwright",
  use: {
    channel: "chromium", // Match the full browser used for actual WebGL gameplay.
    trace: "off",
    screenshot: "only-on-failure",
  },
});
