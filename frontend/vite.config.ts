import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

const e2eServerOrigin = process.env.DRIICHI_E2E_SERVER_ORIGIN;

export default defineConfig({
  plugins: [react()],
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
    environment: "jsdom",
    setupFiles: "./src/test/setup.ts",
    include: ["src/**/*.test.{ts,tsx}"],
  },
});
