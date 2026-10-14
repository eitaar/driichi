import { defineConfig } from "@playwright/test";

const hardwareWebgl = process.env.DRIICHI_HARDWARE_WEBGL === "1";
const webglArgs = hardwareWebgl
  ? ["--enable-webgl"]
  : [
      "--use-angle=swiftshader",
      "--enable-webgl",
      "--enable-unsafe-swiftshader",
    ];

export default defineConfig({
  testDir: "./tests",
  // Built/native worker fixture owns its server and runs with playwright.worker.config.ts.
  testIgnore: ["**/renderer-worker.spec.ts"],
  fullyParallel: true,
  workers: 1, // Isolate SwiftShader scenes, particularly the renderer-budget probe.
  forbidOnly: !!process.env.CI,
  retries: 0,
  maxFailures: process.env.CI ? 1 : undefined, // Retain the first failure before the job bound expires.
  reporter: [["list"], ["html", { outputFolder: "playwright-report", open: "never" }]],
  use: {
    baseURL: "http://127.0.0.1:4173",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: {
        browserName: "chromium",
        channel: "chromium", // Full browser/new headless, rather than the headless-shell renderer.
        ...(hardwareWebgl ? { deviceScaleFactor: 2 } : {}),
        launchOptions: { args: webglArgs },
      },
    },
  ],
  webServer: {
    command: "npm run dev -- --host 127.0.0.1",
    url: "http://127.0.0.1:4173",
    reuseExistingServer: !process.env.CI,
  },
});
