// One screenshot per README/wiki still, all framed identically via bootPanel.
import type { Browser, Page } from "@playwright/test";
import { join } from "node:path";
import { bootPanel } from "./harness";

// Let lazy thumbnails resolve + paint before shooting.
async function settle(page: Page): Promise<void> {
  await page.waitForTimeout(600);
}

export async function shootStills(browser: Browser, outDir: string): Promise<void> {
  // 1) Videos list (hero) — active-bar layout, matching the conversion GIF (the
  //    persistent "Compress with ‹Discord (10MB)›" bar is visible).
  {
    const { context, page } = await bootPanel(browser, { layout: "active-bar", still: true });
    await settle(page);
    await page.screenshot({ path: join(outDir, "panel.png") });
    await context.close();
  }

  // 2) Quick-pick preset menu open. Clicking a row in quick-pick mode opens the
  //    "Compress with…" overlay (1–9 presets).
  {
    const { context, page } = await bootPanel(browser, { layout: "quick-pick", still: true });
    await settle(page);
    await page.locator(".view-videos .row").first().click();
    await page.waitForSelector(".quickpick", { state: "visible" });
    await page.waitForTimeout(300);
    await page.screenshot({ path: join(outDir, "preset-quickpick.png") });
    await context.close();
  }

  // 3) Active-preset bar layout.
  {
    const { context, page } = await bootPanel(browser, { layout: "active-bar", still: true });
    await settle(page);
    await page.screenshot({ path: join(outDir, "preset-active-bar.png") });
    await context.close();
  }

  // 4) Preferences tab.
  {
    const { context, page } = await bootPanel(browser, { layout: "quick-pick", still: true });
    await page.locator("#tab-prefs").click();
    await page.waitForSelector(".view-prefs", { state: "visible" });
    await page.waitForTimeout(300);
    await page.screenshot({ path: join(outDir, "preferences.png") });
    await context.close();
  }

  // 5) Expanded row showing the enlarged preview + preset chips. Expand via the
  //    row chevron, then re-point the preview image at the high-res @big card.
  {
    const { context, page } = await bootPanel(browser, { layout: "quick-pick", still: true });
    await settle(page);
    await page.locator(".view-videos .row .row-chevron").first().click();
    await page.waitForSelector(".row.is-expanded .preview-thumb", { state: "visible" });
    await page.evaluate(() => {
      const img = document.querySelector<HTMLImageElement>(".row.is-expanded .preview-thumb");
      if (img && !img.src.endsWith("%40big")) img.src = img.src + "%40big";
    });
    await page.waitForTimeout(500);
    await page.screenshot({ path: join(outDir, "expanded.png") });
    await context.close();
  }
}
