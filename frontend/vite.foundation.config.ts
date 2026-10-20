import { mergeConfig } from "vite";
import appConfig from "./vite.config.ts";

export default mergeConfig(appConfig, {
  optimizeDeps: { entries: [...appConfig.optimizeDeps!.entries as string[], "tests/fixtures/ui-foundation.html"] },
  cacheDir: "node_modules/.vite-foundation",
  server: { port: 4174, strictPort: true },
});
