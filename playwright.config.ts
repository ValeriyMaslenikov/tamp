import { defineConfig, devices } from "@playwright/test";

// The Playwright UI E2E suite drives the REAL frontend (Vite dev server) with
// the Tauri IPC mocked (see e2e/mock-ipc.ts), so the app boots in a plain
// Chromium with deterministic, per-test-overridable backend responses.
//
// Port: 1430, NOT 1420. Port 1420 is strictPort-reserved for `tauri dev`
// (vite.config.ts), so the E2E web server uses its own port to avoid colliding
// with a running dev session. The `--port`/`--strictPort` CLI flags override
// vite.config.ts's server.port.
const PORT = 1430;

export default defineConfig({
  testDir: "./e2e",
  // Keep CI honest: no .only sneaks through, and flaky-by-retry is opt-in.
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["github"], ["list"]] : "list",
  use: {
    baseURL: `http://localhost:${PORT}`,
    trace: "on-first-retry",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: {
    // Vite dev server (real frontend bundle) on the dedicated E2E port.
    command: `bun run vite --port ${PORT} --strictPort`,
    url: `http://localhost:${PORT}`,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
});
