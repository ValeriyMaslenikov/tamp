// tauri-driver smoke suite — proves the REAL built app boots end-to-end and the
// panel is drivable via WebDriver (WebView2 on Windows).
//
// Scope: happy-path only, 1-3 robust assertions. This is the highest layer of
// the test pyramid (full app, no mocks) and the most CI-dependent — keep it
// minimal so it stays a boot/wiring smoke, not a brittle UI-detail suite.
//
// The app was launched with TAMP_E2E=1 (see wdio.conf.ts), so lib.rs's
// apply_e2e_mode showed + pinned the panel; without that the release hide-on-
// blur handler would close the window the instant WebDriver's automation takes
// focus, and there'd be nothing to attach to.
//
// Selectors mirror the verified Playwright harness (../e2e/panel.spec.ts):
// `.seg .seg-btn` for the tab strip, and `#tab-videos` / `#tab-converted` /
// `#tab-prefs` for the individual tabs.
import { browser, $, $$, expect } from "@wdio/globals";

describe("tamp app boots and the panel is drivable", () => {
  it("renders the panel with the three tabs", async () => {
    // 1. The app booted and the WebView2 document is reachable: the tab strip
    //    is present. waitForExist tolerates the first-paint delay of a cold
    //    release launch.
    const tabStrip = await $(".seg");
    await tabStrip.waitForExist({ timeout: 30_000 });

    // 2. Exactly the three top-level tabs render (Videos / Converted /
    //    Preferences) — the panel chrome built. Assert by count + id rather than
    //    localized label text so a default-locale flip can't flake the smoke.
    const tabs = await $$(".seg .seg-btn");
    await expect(tabs).toBeElementsArrayOfSize(3);

    await expect($("#tab-videos")).toBeExisting();
    await expect($("#tab-converted")).toBeExisting();
    await expect($("#tab-prefs")).toBeExisting();
  });

  it("opens with the Videos tab active", async () => {
    // 3. Trivially-true-only-if-the-app-booted: the default active tab is
    //    Videos. main.ts selects it on startup, so a non-"true" value here means
    //    the frontend never finished its boot path against the real backend.
    const videos = await $("#tab-videos");
    await videos.waitForExist({ timeout: 15_000 });
    await expect(videos).toHaveAttribute("aria-selected", "true");
  });
});
