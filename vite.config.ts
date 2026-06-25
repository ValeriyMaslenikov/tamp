/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import { configDefaults } from "vitest/config";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({

  // Vitest reads this config. The E2E specs are NOT unit tests and use other
  // runners' globals, which vitest's default *.spec.ts glob would otherwise try
  // (and fail) to run — exclude them: e2e/ is Playwright (@playwright/test), and
  // e2e-native/ is the tauri-driver/WebdriverIO smoke suite (@wdio/globals, only
  // installed inside e2e-native/). Keep both out of the unit run.
  test: {
    exclude: [...configDefaults.exclude, "e2e/**", "e2e-native/**"],
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
