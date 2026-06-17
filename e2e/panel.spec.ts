// Harness proof: the REAL frontend boots in a plain browser against the mocked
// Tauri IPC, the three tabs render, and the Videos tab shows both the empty and
// the list state driven by the mocked get_settings / list_recents.
import { test, expect } from "./fixtures";
import { sampleRecents } from "./canned";

test.describe("panel renders against mocked IPC", () => {
  test("shows the three tabs", async ({ tamp, page }) => {
    await tamp.goto();

    const tabs = page.locator(".seg .seg-btn");
    await expect(tabs).toHaveCount(3);
    await expect(page.locator("#tab-videos")).toHaveText("Videos");
    await expect(page.locator("#tab-converted")).toHaveText("Converted");
    await expect(page.locator("#tab-prefs")).toHaveText("Preferences");

    // The Videos tab is the default active one.
    await expect(page.locator("#tab-videos")).toHaveAttribute("aria-selected", "true");

    // Proves boot completed: get_settings was the first command the app invoked.
    const calls = await tamp.calls();
    expect(calls.map((c) => c.cmd)).toContain("get_settings");
    expect(calls.map((c) => c.cmd)).toContain("list_recents");
  });

  test("Videos tab shows the empty state when there are no recents", async ({ tamp, page }) => {
    // Default list_recents is [] — the empty notice should render.
    await tamp.goto();

    await expect(page.locator(".view-videos .empty")).toHaveText(
      "No videos in your watched folders yet — record something!",
    );
  });

  test("Videos tab lists the mocked recent videos", async ({ tamp, page }) => {
    await tamp.setMock("list_recents", sampleRecents());
    await tamp.goto();

    const rows = page.locator(".view-videos .row");
    await expect(rows).toHaveCount(2);
    await expect(page.locator(".view-videos .row-name-text").first()).toContainText(
      "screen-recording-1.mov",
    );
  });
});
