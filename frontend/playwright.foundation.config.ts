import { defineConfig } from "@playwright/test";
import appConfig from "./playwright.config.ts";

export default defineConfig(appConfig, {
  testIgnore: [],
  testMatch: "**/ui-foundation.spec.ts",
  use: { baseURL: "http://127.0.0.1:4174" },
  webServer: {
    command: "npm run dev -- --config vite.foundation.config.ts --host 127.0.0.1",
    url: "http://127.0.0.1:4174",
    reuseExistingServer: false,
  },
});
