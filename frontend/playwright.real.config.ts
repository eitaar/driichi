import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  testMatch: "task16-real-server.spec.ts",
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: [["list"], ["html", { outputFolder: "playwright-real-report", open: "never" }]],
  outputDir: "test-results/task-16-playwright",
  use: {
    trace: "off",
    screenshot: "only-on-failure",
  },
});
