// LANGUAGE journey: the boot locale drives the rendered language, and switching
// the Preferences > Language radio to Українська persists locale:"uk" through
// save_settings — then, simulating the reload the app does, a boot with
// locale:"uk" renders Ukrainian text.
import { test, expect } from "./fixtures";

test.describe("language", () => {
  test("locale 'en' renders the English UI", async ({ tamp, page }) => {
    // canned defaultSettings() already pins locale:"en"; assert the tabs are
    // the English strings (not the i18n keys, not Ukrainian).
    await tamp.goto();
    await expect(page.locator("#tab-videos")).toHaveText("Videos");
    await expect(page.locator("#tab-converted")).toHaveText("Converted");
    await expect(page.locator("#tab-prefs")).toHaveText("Preferences");
    await expect(page.locator("html")).toHaveAttribute("lang", "en");
  });

  test("switching Language to Українська saves locale:'uk'", async ({ tamp, page }) => {
    await tamp.goto();

    // Open Preferences and find the Language radio group by its visible label.
    await page.locator("#tab-prefs").click();
    const prefs = page.locator(".view-prefs");
    await expect(prefs).toBeVisible();

    // The language radios carry the autonyms ("English"/"Українська") which stay
    // in their own language in every locale — a resilient, text-based selector.
    // The change handler persists then hard-reloads the webview; click (not
    // check, whose post-action stability wait fights the reload) and let the
    // page navigate. The harness mirrors calls into sessionStorage, so the
    // save_settings recorded before the reload survives into the fresh page.
    const ukRadio = page.getByRole("radio", { name: "Українська" });
    await expect(ukRadio).toBeVisible();
    await ukRadio.click();

    // Wait out the reload, then assert the persisted save carried locale:"uk".
    await page.waitForSelector(".panel .seg", { state: "attached" });
    await expect
      .poll(async () => {
        const calls = await tamp.calls();
        const save = calls.filter((c) => c.cmd === "save_settings").pop();
        return (save?.args?.settings as { locale?: string } | undefined)?.locale ?? null;
      })
      .toBe("uk");

    // And the reload re-read the just-saved locale: the UI is now Ukrainian.
    await expect(page.locator("#tab-prefs")).toHaveText("Параметри");
    await expect(page.locator("html")).toHaveAttribute("lang", "uk");
  });

  test("a boot with locale:'uk' renders Ukrainian text (post-reload)", async ({ tamp, page }) => {
    // The app applies a language change by persisting then hard-reloading so the
    // whole webview re-reads settings. We simulate that reloaded state directly:
    // boot with locale:"uk" and assert the Ukrainian tab labels render.
    await tamp.seedSettings({ locale: "uk" });
    await tamp.goto();

    await expect(page.locator("#tab-videos")).toHaveText("Відео");
    await expect(page.locator("#tab-converted")).toHaveText("Перетворені");
    await expect(page.locator("#tab-prefs")).toHaveText("Параметри");
    await expect(page.locator("html")).toHaveAttribute("lang", "uk");
  });
});
