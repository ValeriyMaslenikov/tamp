// Boots the REAL frontend in headless Chromium against the existing Tauri-IPC
// mock, seeded with the curated demo data, framed for a product shot. Thumbnails
// are served by intercepting the asset.localhost URLs the app resolves via
// convertFileSrc(). This mirrors e2e/fixtures.ts's two-addInitScript boot order
// (seed window.__E2E__, THEN install the mock) but as a standalone helper so it
// never runs under `playwright test`.
import type { Browser, BrowserContext, Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { installTauriMock } from "../../e2e/mock-ipc";
import { defaultSettings } from "../../e2e/canned";
import type { Settings } from "../../src/lib/ipc";
import { demoRecents, demoConversions, THUMB_BY_PATH } from "./demo-data";
import { FRAME_CSS, STILL_TWEAKS_CSS, VIEW_W, VIEW_H } from "./frame";

const HERE = dirname(fileURLToPath(import.meta.url));
const THUMBS = join(HERE, "thumbs");

// A dedicated Vite port so capturing never collides with `tauri dev` (1420) or
// the E2E suite (1430). shoot.ts starts the server on this port.
export const BASE_URL = "http://localhost:1431";

export interface BootOpts {
  layout?: "quick-pick" | "active-bar";
  settings?: Partial<Settings>;
  recordVideoDir?: string;
  still?: boolean;
}

export async function bootPanel(
  browser: Browser,
  opts: BootOpts = {},
): Promise<{ context: BrowserContext; page: Page }> {
  const layout = opts.layout ?? "quick-pick";
  const settings = {
    ...defaultSettings(),
    videosLayout: layout,
    theme: "dark",
    ...opts.settings,
  } as unknown as Record<string, unknown>;

  const context = await browser.newContext({
    viewport: { width: VIEW_W, height: VIEW_H },
    deviceScaleFactor: 2,
    ...(opts.recordVideoDir
      ? { recordVideo: { dir: opts.recordVideoDir, size: { width: VIEW_W, height: VIEW_H } } }
      : {}),
  });

  // Serve thumbnails: the app resolves a thumb path to https://asset.localhost/
  // <encodeURIComponent(path)> via convertFileSrc(). Decode, map the original
  // video path to its slug, and fulfill with the generated PNG. An "@big" suffix
  // selects the high-res preview card used by the expanded shot.
  await context.route("https://asset.localhost/**", async (route) => {
    const url = new URL(route.request().url());
    const original = decodeURIComponent(url.pathname.replace(/^\//, ""));
    const big = original.endsWith("@big");
    const videoPath = original.replace(/@big$/, "");
    const slug = THUMB_BY_PATH[videoPath];
    if (!slug) return route.fulfill({ status: 404, body: "" });
    const file = join(THUMBS, `${slug}${big ? "@big" : ""}.png`);
    return route.fulfill({ contentType: "image/png", body: readFileSync(file) });
  });

  const page = await context.newPage();

  // 1) Seed window.__E2E__ BEFORE the mock installs. The mock answers list_recents
  //    from a static value, but recent_thumb/recent_duration must be functions of
  //    the path — so they're inlined here (addInitScript serializes this whole fn,
  //    so the inlined data must be self-contained, no outer references).
  await page.addInitScript(
    (data: {
      recents: unknown;
      conversions: unknown;
      thumbByPath: Record<string, string>;
      durationByPath: Record<string, number>;
    }) => {
      (window as unknown as { __E2E__: unknown }).__E2E__ = {
        mocks: {
          list_recents: data.recents,
          list_conversions: data.conversions,
          // Return the video path itself as the thumb path; the app feeds it back
          // through convertFileSrc() and the route handler maps it to a PNG.
          recent_thumb: (a: { path: string }) =>
            data.thumbByPath[a.path] ? a.path : null,
          recent_duration: (a: { path: string }) => data.durationByPath[a.path] ?? null,
          // Keep the expanded preview on the enlarged thumbnail: never resolve the
          // proxy build, so the <video> swap never happens (still tweaks hide the
          // shimmer). Returns a forever-pending promise.
          ensure_preview: () => new Promise(() => {}),
        },
      };
    },
    {
      recents: demoRecents(),
      conversions: demoConversions(),
      thumbByPath: THUMB_BY_PATH,
      durationByPath: Object.fromEntries(demoRecents().map((r) => [r.path, r.durationSecs ?? 0])),
    },
  );

  // 2) Install the Tauri global before main.ts boots.
  await page.addInitScript(installTauriMock, settings);

  await page.goto(BASE_URL);
  await page.waitForSelector(".panel .seg", { state: "attached" });

  await page.addStyleTag({ content: FRAME_CSS });
  if (opts.still) await page.addStyleTag({ content: STILL_TWEAKS_CSS });

  return { context, page };
}
