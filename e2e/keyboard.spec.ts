// KEYBOARD model: Ctrl+Tab cycles the top tabs (arrows no longer do); on the
// Videos tab ←/→ expand/collapse the selected row (preset cycling moved off the
// arrows); and Esc on the Preferences tab hides the panel (was a no-op before).
import { test, expect } from "./fixtures";
import { sampleRecents } from "./canned";

test.describe("keyboard", () => {
  test("Ctrl+Tab / Ctrl+Shift+Tab cycle the top tabs", async ({ tamp, page }) => {
    await tamp.goto();
    await expect(page.locator("#tab-videos")).toHaveAttribute("aria-selected", "true");

    await page.keyboard.press("Control+Tab");
    await expect(page.locator("#tab-converted")).toHaveAttribute("aria-selected", "true");

    await page.keyboard.press("Control+Tab");
    await expect(page.locator("#tab-prefs")).toHaveAttribute("aria-selected", "true");

    await page.keyboard.press("Control+Tab"); // wraps back to the first
    await expect(page.locator("#tab-videos")).toHaveAttribute("aria-selected", "true");

    await page.keyboard.press("Control+Shift+Tab"); // backwards, wraps to the last
    await expect(page.locator("#tab-prefs")).toHaveAttribute("aria-selected", "true");
  });

  test("Videos: ↓ dives into the list, → expands the row, ← collapses it", async ({
    tamp,
    page,
  }) => {
    await tamp.setMock("list_recents", sampleRecents());
    await tamp.goto();
    const rows = page.locator(".view-videos .row");
    await expect(rows).toHaveCount(2);

    // Focus starts in the filter; ArrowDown blurs it and selects the first row.
    await page.keyboard.press("ArrowDown");
    await expect(rows.first()).toHaveClass(/is-selected/);

    // → expands the selected row; ← collapses it (no longer cycles presets).
    await page.keyboard.press("ArrowRight");
    await expect(rows.first()).toHaveClass(/is-expanded/);
    await page.keyboard.press("ArrowLeft");
    await expect(rows.first()).not.toHaveClass(/is-expanded/);
  });

  test("Preferences: Esc hides the panel", async ({ tamp, page }) => {
    await tamp.goto();
    await page.locator("#tab-prefs").click();
    await expect(page.locator(".view-prefs")).toBeVisible();

    await page.keyboard.press("Escape");
    await expect
      .poll(async () => {
        const calls = await tamp.calls();
        return calls.some((c) => c.cmd === "plugin:window|hide");
      })
      .toBe(true);
  });
});
