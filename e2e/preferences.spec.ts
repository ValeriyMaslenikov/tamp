// PREFERENCES journey: a behavior toggle persists via save_settings; a DUPLICATE
// preset name shows the error toast and does NOT save; an out-of-range
// recents-limit shows its validation error (and isn't saved).
import { test, expect } from "./fixtures";

test.describe("preferences", () => {
  test("a behavior toggle calls save_settings", async ({ tamp, page }) => {
    await tamp.goto();
    await page.locator("#tab-prefs").click();
    await expect(page.locator(".view-prefs")).toBeVisible();

    // "Copy result to clipboard" defaults on in the canned settings; flipping it
    // off persists copyToClipboard:false.
    const toggle = page.getByText("Copy result to clipboard");
    await toggle.click();

    await expect.poll(async () => {
      const calls = await tamp.calls();
      const save = calls.filter((c) => c.cmd === "save_settings").pop();
      return (save?.args?.settings as { copyToClipboard?: boolean } | undefined)
        ?.copyToClipboard;
    }).toBe(false);
  });

  test("a duplicate preset name shows the error toast and does NOT save", async ({
    tamp,
    page,
  }) => {
    await tamp.goto();
    await page.locator("#tab-prefs").click();
    await expect(page.locator(".view-prefs")).toBeVisible();

    // Edit the first preset and rename it to collide with the OTHER preset's
    // name (case-insensitively): "Slack (25MB)" -> " slack (25mb) ".
    await page.locator(".preset-card").first().getByText("Edit").click();
    const editor = page.locator(".preset-editor");
    await expect(editor).toBeVisible();
    const nameInput = editor.locator("input.input").first();
    await nameInput.fill(" slack (25mb) ");
    await editor.getByText("Save", { exact: true }).click();

    // The duplicate-name error toast shows (interpolated with the typed name).
    const toast = page.locator("#toast");
    await expect(toast).toContainText("already exists");

    // Crucially: no save_settings was issued for this blocked rename.
    const calls = await tamp.calls();
    expect(calls.some((c) => c.cmd === "save_settings")).toBe(false);
  });

  test("an out-of-range recents-limit shows its error and isn't saved", async ({
    tamp,
    page,
  }) => {
    await tamp.goto();
    await page.locator("#tab-prefs").click();
    await expect(page.locator(".view-prefs")).toBeVisible();

    // "Recent videos shown" accepts 1–200; 999 is out of range.
    const field = page
      .locator(".field")
      .filter({ hasText: "Recent videos shown" });
    const limitInput = field.locator("input");
    await limitInput.fill("999");
    await limitInput.blur();

    const toast = page.locator("#toast");
    await expect(toast).toContainText("from 1 to 200");

    const calls = await tamp.calls();
    expect(calls.some((c) => c.cmd === "save_settings")).toBe(false);
    // The control snaps back to the persisted value (50).
    await expect(limitInput).toHaveValue("50");
  });
});
