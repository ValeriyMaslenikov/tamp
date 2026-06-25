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
import { frameCss, STILL_TWEAKS_CSS, STILL_PAD, viewport } from "./frame";

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
  /** Backdrop padding around the panel; defaults to the roomy still framing. */
  pad?: { x: number; y: number };
  /** When recording video, render at this CSS zoom for retina-crisp output
   *  (Playwright records at CSS-pixel resolution, ignoring deviceScaleFactor).
   *  e.g. 2 → a 2× video. Default 1. */
  videoScale?: number;
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

  const pad = opts.pad ?? STILL_PAD;
  const base = viewport(pad);
  // Scale the viewport up only when recording video (so the webm is rendered at
  // CSS-pixel 2×); stills already get crispness from deviceScaleFactor:2.
  const vs = opts.recordVideoDir ? opts.videoScale ?? 1 : 1;
  const view = { width: base.width * vs, height: base.height * vs };
  const context = await browser.newContext({
    viewport: view,
    // dsf only helps screenshots; for a video context keep it 1 to avoid a 4×
    // backing store on the already-doubled viewport.
    deviceScaleFactor: opts.recordVideoDir ? 1 : 2,
    ...(opts.recordVideoDir
      ? { recordVideo: { dir: opts.recordVideoDir, size: view } }
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

  await page.addStyleTag({ content: frameCss(pad) });
  if (opts.still) await page.addStyleTag({ content: STILL_TWEAKS_CSS });
  // Zoom the whole document so the (px-based) panel renders at 2× into the
  // doubled video viewport — crisp text instead of a 1× upscale.
  if (vs !== 1) await page.addStyleTag({ content: `html { zoom: ${vs}; }` });

  return { context, page };
}
