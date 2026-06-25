# README/wiki Asset Capture Harness — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the hand-recorded, untidy README demo GIF and refresh all stills with clean, consistent, professional assets produced from one reproducible command.

**Architecture:** A self-contained `docs/capture/` toolkit drives the **real** frontend in headless Chromium against the existing Tauri-IPC mock (`e2e/mock-ipc.ts`), with curated demo fixtures, generated emoji-card thumbnails served via request interception, a soft-shadow backdrop injected at capture time, and a scripted conversion animation driven by `encode:state` events. Stills are screenshots; the demo is a Playwright-recorded webm encoded to an optimized looping palette GIF with ffmpeg. Nothing in product code changes.

**Tech Stack:** TypeScript run with `bun`; `@playwright/test` (re-exports `chromium`); ffmpeg (system binary); the project's existing `e2e/mock-ipc.ts` and `src/lib/ipc.ts` types.

## Global Constraints

- **No product-code changes.** Only files under `docs/capture/` (and `docs/*.png`/`docs/convert.gif` outputs, `package.json` scripts, README) may be created/modified. Never edit `src/**` or `e2e/**`.
- **Reuse the existing mock.** Import `installTauriMock` from `e2e/mock-ipc.ts`; do not fork or duplicate it.
- **Types come from `src/lib/ipc.ts`** (`RecentVideo`, `ConversionRecord`, `Settings`, `JobState`, `Preset`). Mirror the camelCase shapes in `e2e/canned.ts`.
- **Run everything with `bun`** (the repo's runner; Playwright is driven via `bunx playwright` in CI). Scripts are `.ts`, executed `bun docs/capture/<file>.ts`.
- **ffmpeg is a prerequisite** on `PATH` (present locally at `/opt/homebrew/bin/ffmpeg`). Document it; do not bundle it.
- **Do not touch the CI E2E suite.** The capture toolkit must NOT be picked up by `bunx playwright test` (it lives outside `testDir: "./e2e"` and is a standalone script, not a `*.spec.ts` under `e2e/`).
- **Capture viewport:** panel box `420×640`, backdrop padding `56px` horizontal / `60px` vertical → viewport `532×760`; stills at `deviceScaleFactor: 2`. These are the single source of truth — reference them, don't re-derive.
- **Output paths (overwrite in place):** `docs/panel.png`, `docs/expanded.png`, `docs/preferences.png`, `docs/preset-quickpick.png`, `docs/preset-active-bar.png`, `docs/convert.gif`.

---

## File Structure

| File | Responsibility |
| --- | --- |
| `docs/capture/emoji.ts` | The demo cast: each fake recording's filename, emoji, gradient, size, duration, timestamp. One source of truth consumed by both the thumbnail generator and the fixtures. |
| `docs/capture/gen-thumbs.ts` | Renders each cast member's emoji-on-gradient card to `docs/capture/thumbs/<slug>.png` (small) and `<slug>@big.png` (expanded preview) via Chromium. |
| `docs/capture/demo-data.ts` | Builds `RecentVideo[]`, `ConversionRecord[]`, and `Settings` overrides from `emoji.ts`, plus the path→thumb lookup used by the IPC mock overrides and the route handler. |
| `docs/capture/frame.ts` | The backdrop/shadow framing CSS string + still-only tweaks, injected into the page at capture time. |
| `docs/capture/harness.ts` | `bootPanel(browser, opts)`: new context (viewport/dsf/optional video), install mock + seed `__E2E__`, route `asset.localhost` thumbnails, navigate, wait for `.panel`, apply framing CSS. Returns `{ context, page }`. |
| `docs/capture/stills.ts` | One function per still (`shootPanel`, `shootExpanded`, `shootPreferences`, `shootQuickpick`, `shootActiveBar`) using `harness.ts`. |
| `docs/capture/gif.ts` | Records the conversion animation to webm, then encodes `docs/convert.gif` with ffmpeg. |
| `docs/capture/shoot.ts` | Orchestrator: regenerate thumbs, run all stills, build the GIF. The `bun run assets:shoot` entry point. |
| `docs/capture/README.md` | Runbook: prerequisites + how to regenerate. |
| `docs/capture/thumbs/*.png` | Generated thumbnail assets (committed). |
| `package.json` | Add `"assets:shoot"` script. |

---

## Task 1: Demo cast + thumbnail generator

**Files:**
- Create: `docs/capture/emoji.ts`
- Create: `docs/capture/gen-thumbs.ts`
- Create (generated, committed): `docs/capture/thumbs/*.png`

**Interfaces:**
- Produces: `export interface CastMember { slug: string; file: string; emoji: string; grad: [string, string]; sizeBytes: number; durationSecs: number; recordedAgoMs: number; }` and `export const CAST: CastMember[]` from `emoji.ts`.
- Produces: `docs/capture/thumbs/<slug>.png` (64×40 @2x = 128×80) and `docs/capture/thumbs/<slug>@big.png` (404×232) — written by `gen-thumbs.ts`’s `export async function genThumbs(): Promise<void>`.

- [ ] **Step 1: Write the cast list**

Create `docs/capture/emoji.ts`:

```ts
// The demo cast for README/wiki captures — one source of truth for both the
// generated thumbnails (gen-thumbs.ts) and the fixture data (demo-data.ts).
// Names are intentionally playful and reuse the originals from the old panel.
export interface CastMember {
  slug: string;
  file: string;
  emoji: string;
  grad: [string, string]; // CSS gradient stops for the thumbnail card
  sizeBytes: number;
  durationSecs: number;
  recordedAgoMs: number; // how long before "now" it was recorded
}

const MIN = 60_000;
const HOUR = 3_600_000;

export const CAST: CastMember[] = [
  { slug: "cat",   file: "cat-knocks-over-everything.mov", emoji: "🐈", grad: ["#f0823c", "#e35d6a"], sizeBytes: 242 * 1024 * 1024, durationSecs: 42,  recordedAgoMs: 3 * MIN },
  { slug: "boss",  file: "ranked-match-final-boss.mov",    emoji: "🎮", grad: ["#6d5cf0", "#3a2f88"], sizeBytes: 895 * 1024 * 1024, durationSecs: 187, recordedAgoMs: 21 * MIN },
  { slug: "duck",  file: "duck-debugging-session.mov",     emoji: "🦆", grad: ["#1aa6a6", "#147a7a"], sizeBytes: 1_181_116_006,       durationSecs: 365, recordedAgoMs: 2 * HOUR },
  { slug: "deploy",file: "deploy-friday-5pm.mov",          emoji: "🚀", grad: ["#7c5cfc", "#4a36a8"], sizeBytes: 158 * 1024 * 1024, durationSecs: 96,  recordedAgoMs: 19 * HOUR },
  { slug: "demo",  file: "demo-went-fine-honestly.mov",    emoji: "⭐", grad: ["#f0a93c", "#e3743c"], sizeBytes: 86_700_000,         durationSecs: 64,  recordedAgoMs: 22 * HOUR },
  { slug: "pizza", file: "pizza-tracker-speedrun.mov",     emoji: "🍕", grad: ["#e3563c", "#a8362f"], sizeBytes: 132 * 1024 * 1024, durationSecs: 51,  recordedAgoMs: 26 * HOUR },
];
```

- [ ] **Step 2: Write the thumbnail generator**

Create `docs/capture/gen-thumbs.ts`:

```ts
// Renders each cast member's emoji-on-gradient card to a PNG via Chromium, at
// two sizes: the list thumbnail (128×80) and the expanded preview (404×232).
import { chromium } from "@playwright/test";
import { mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { CAST } from "./emoji";

const HERE = dirname(fileURLToPath(import.meta.url));
const OUT = join(HERE, "thumbs");

function cardHtml(emoji: string, grad: [string, string], emojiPx: number): string {
  return `<!doctype html><html><head><meta charset="utf-8"><style>
    html,body{margin:0;padding:0;}
    .card{width:100%;height:100%;display:flex;align-items:center;justify-content:center;
      background:linear-gradient(135deg, ${grad[0]} 0%, ${grad[1]} 100%);}
    .e{font-size:${emojiPx}px;line-height:1;filter:drop-shadow(0 2px 6px rgba(0,0,0,.25));}
  </style></head><body><div class="card"><span class="e">${emoji}</span></div></body></html>`;
}

export async function genThumbs(): Promise<void> {
  mkdirSync(OUT, { recursive: true });
  const browser = await chromium.launch();
  try {
    for (const m of CAST) {
      for (const [suffix, w, h, emojiPx] of [
        ["", 128, 80, 44],
        ["@big", 404, 232, 120],
      ] as const) {
        const ctx = await browser.newContext({ viewport: { width: w, height: h } });
        const page = await ctx.newPage();
        await page.setContent(cardHtml(m.emoji, m.grad, emojiPx));
        await page.screenshot({ path: join(OUT, `${m.slug}${suffix}.png`) });
        await ctx.close();
      }
    }
  } finally {
    await browser.close();
  }
}

// Allow `bun docs/capture/gen-thumbs.ts` to run it standalone.
if (import.meta.main) {
  await genThumbs();
  console.log("thumbs written to", OUT);
}
```

- [ ] **Step 3: Generate the thumbnails**

Run: `bun docs/capture/gen-thumbs.ts`
Expected: `thumbs written to .../docs/capture/thumbs` and 12 PNGs created.

- [ ] **Step 4: Verify the thumbnails exist and are non-trivial**

Run: `ls -la docs/capture/thumbs/ && file docs/capture/thumbs/cat.png`
Expected: 12 files; `cat.png` reported as `PNG image data, 128 x 80`. Open `docs/capture/thumbs/cat@big.png` and confirm it's a clean emoji-on-gradient card.

- [ ] **Step 5: Commit**

```bash
git add docs/capture/emoji.ts docs/capture/gen-thumbs.ts docs/capture/thumbs/
git commit -m "feat(capture): demo cast + emoji thumbnail generator"
```

---

## Task 2: Demo fixtures + thumb lookup

**Files:**
- Create: `docs/capture/demo-data.ts`

**Interfaces:**
- Consumes: `CAST`, `CastMember` from `./emoji`.
- Produces:
  - `export function demoRecents(): RecentVideo[]`
  - `export function demoConversions(): ConversionRecord[]`
  - `export function demoSettings(layout: "quick-pick" | "active-bar"): Partial<Settings>`
  - `export const THUMB_BY_PATH: Record<string, string>` — video path → thumb **slug** (e.g. `"/Users/demo/Movies/cat-knocks-over-everything.mov" → "cat"`). Used by the route handler to pick the PNG and by the mock to answer `recent_thumb`.

- [ ] **Step 1: Write the fixtures module**

Create `docs/capture/demo-data.ts`:

```ts
// Curated demo fixtures for captures, derived from the cast in emoji.ts. The IPC
// mock answers list_recents/recent_thumb/recent_duration from these so the panel
// renders the same coherent set across every shot.
import type { RecentVideo, ConversionRecord, Settings } from "../../src/lib/ipc";
import { CAST } from "./emoji";

const DIR = "/Users/demo/Movies";
// A fixed "now" so timestamps are deterministic across runs (avoids drift
// between the recents' "Xm ago" and assertions). Chosen arbitrarily.
const NOW = 1_750_000_000_000;

function pathFor(file: string): string {
  return `${DIR}/${file}`;
}

export const THUMB_BY_PATH: Record<string, string> = Object.fromEntries(
  CAST.map((m) => [pathFor(m.file), m.slug]),
);

export function demoRecents(): RecentVideo[] {
  return CAST.map((m) => ({
    path: pathFor(m.file),
    name: m.file,
    sizeBytes: m.sizeBytes,
    createdMs: NOW - m.recordedAgoMs,
    // thumbPath stays null: the list resolves thumbs lazily via recent_thumb,
    // which the mock overrides (see harness.ts) to return a slug path the route
    // handler serves. Setting it here would skip the lazy path we rely on.
    thumbPath: null,
    isOutput: false,
    conversion: null,
    durationSecs: m.durationSecs,
  }));
}

export function demoConversions(): ConversionRecord[] {
  // One split (group) + one single, mirroring e2e/canned.ts's shape so the
  // Converted tab renders a realistic tree if we ever shoot it.
  const cat = CAST[0];
  const duck = CAST[2];
  return [
    {
      inputPath: pathFor(duck.file),
      inputBytes: duck.sizeBytes,
      outputs: [
        { path: `${DIR}/duck-debugging-session (tamped 1of2).mp4`, bytes: 9_800_000 },
        { path: `${DIR}/duck-debugging-session (tamped 2of2).mp4`, bytes: 9_600_000 },
      ],
      presetHash: "hash-split",
      presetName: "Discord (10MB)",
      targetMb: 10,
      completedAtMs: NOW - 120_000,
      inputCreatedMs: NOW - 600_000,
    },
    {
      inputPath: pathFor(cat.file),
      inputBytes: cat.sizeBytes,
      outputs: [{ path: `${DIR}/cat-knocks-over-everything (tamped).mp4`, bytes: 8_200_000 }],
      presetHash: "hash-single",
      presetName: "Slack (25MB)",
      targetMb: 25,
      completedAtMs: NOW - 240_000,
      inputCreatedMs: NOW - 900_000,
    },
  ];
}

export function demoSettings(layout: "quick-pick" | "active-bar"): Partial<Settings> {
  return { videosLayout: layout, theme: "dark", locale: "en", onboardingSeen: true };
}
```

- [ ] **Step 2: Verify it type-checks and produces data**

Run:
```bash
bun -e 'import("./docs/capture/demo-data.ts").then(m=>{const r=m.demoRecents();console.log(r.length, r[0].name, Object.keys(m.THUMB_BY_PATH).length)})'
```
Expected: `6 cat-knocks-over-everything.mov 6`

- [ ] **Step 3: Commit**

```bash
git add docs/capture/demo-data.ts
git commit -m "feat(capture): curated demo fixtures + thumb lookup"
```

---

## Task 3: Framing CSS

**Files:**
- Create: `docs/capture/frame.ts`

**Interfaces:**
- Produces: `export const FRAME_CSS: string` (backdrop + shadow + sizing) and `export const STILL_TWEAKS_CSS: string` (hide preview shimmer for stills). Both are injected via `page.addStyleTag({ content })`.

- [ ] **Step 1: Write the framing module**

Create `docs/capture/frame.ts`:

```ts
// CSS injected into the page at capture time (NOT into the app bundle). It turns
// the full-bleed panel into a framed product shot: the panel floats on a subtle
// backdrop with a soft drop shadow, sized to the capture viewport box.
//
// Sizing is the single source of truth shared with harness.ts:
//   panel box 420×640, backdrop padding 56px horizontal / 60px vertical
//   → viewport 532×760.
export const PANEL_W = 420;
export const PANEL_H = 640;
export const PAD_X = 56;
export const PAD_Y = 60;
export const VIEW_W = PANEL_W + PAD_X * 2; // 532
export const VIEW_H = PANEL_H + PAD_Y * 2; // 760

export const FRAME_CSS = `
  html, body { height: 100%; }
  body {
    background: radial-gradient(120% 120% at 50% 0%, #2a2342 0%, #15131f 55%, #0d0c12 100%);
    display: flex; align-items: center; justify-content: center;
    overflow: hidden;
  }
  #app {
    width: ${PANEL_W}px; height: ${PANEL_H}px; flex: none;
    border-radius: 18px; overflow: hidden;
    box-shadow:
      0 24px 60px -12px rgba(0, 0, 0, 0.65),
      0 8px 24px -8px rgba(0, 0, 0, 0.5),
      0 0 0 1px rgba(255, 255, 255, 0.04);
  }
`;

// Stills only: the expanded preview shows an enlarged thumbnail while it "prepares
// a preview"; hide the shimmer/label so the still reads as a finished frame.
export const STILL_TWEAKS_CSS = `
  .preview-loading { display: none !important; }
`;
```

- [ ] **Step 2: Verify it imports**

Run: `bun -e 'import("./docs/capture/frame.ts").then(m=>console.log(m.VIEW_W, m.VIEW_H, m.FRAME_CSS.length>0))'`
Expected: `532 760 true`

- [ ] **Step 3: Commit**

```bash
git add docs/capture/frame.ts
git commit -m "feat(capture): backdrop + soft-shadow framing CSS"
```

---

## Task 4: Capture harness (boot a framed panel)

**Files:**
- Create: `docs/capture/harness.ts`

**Interfaces:**
- Consumes: `installTauriMock` from `../../e2e/mock-ipc`; `defaultSettings` from `../../e2e/canned`; `demoRecents`, `demoConversions`, `THUMB_BY_PATH` from `./demo-data`; `FRAME_CSS`, `STILL_TWEAKS_CSS`, `VIEW_W`, `VIEW_H` from `./frame`.
- Produces:
  - `export interface BootOpts { layout?: "quick-pick" | "active-bar"; settings?: Partial<Settings>; recordVideoDir?: string; still?: boolean; }`
  - `export async function bootPanel(browser: Browser, opts?: BootOpts): Promise<{ context: BrowserContext; page: Page }>`
  - `export const BASE_URL: string` (the Vite dev server URL, `http://localhost:1431`).

- [ ] **Step 1: Write the harness**

Create `docs/capture/harness.ts`:

```ts
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
  // video path to its slug, and fulfill with the generated PNG.
  await context.route("https://asset.localhost/**", async (route) => {
    const url = new URL(route.request().url());
    const original = decodeURIComponent(url.pathname.replace(/^\//, ""));
    // original is a thumb path we minted in the mock: "<videoPath>#thumb" or "@big".
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
          // Return a sentinel thumb path the route handler recognizes; the app
          // feeds it back through convertFileSrc().
          recent_thumb: (a: { path: string }) =>
            data.thumbByPath[a.path] ? `${a.path}` : null,
          recent_duration: (a: { path: string }) => data.durationByPath[a.path] ?? null,
          // Keep the expanded preview on the enlarged thumbnail: never resolve the
          // proxy build, so the <video> swap never happens (still tweaks hide the
          // shimmer). Returns a forever-pending promise.
          ensure_preview: (a: { path: string }) =>
            data.thumbByPath[a.path]
              ? new Promise(() => {})
              : new Promise(() => {}),
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
```

> **Note on `recent_thumb`:** it returns the *video* path as the thumb path. `convertFileSrc()` then produces `https://asset.localhost/<encoded videoPath>`, which the route handler maps to the slug PNG. For the expanded `@big` preview, the enlarged image uses the same `recent_thumb` value, so it resolves to the small card; the route handler only serves `@big` when the requested path ends with `@big`. If the expanded shot needs the big card, the `preview-thumb` `<img>` uses the lazily-resolved thumb path (small) — acceptable, but to force the big card, override the resolved thumb to `${path}@big` for that one shot (handled in Task 5's `shootExpanded`).

- [ ] **Step 2: Smoke-test the harness boots (no assert yet — Vite must be running)**

This is verified end-to-end in Task 5 (the first still). No standalone test here; the harness has no behavior without a running dev server.

- [ ] **Step 3: Commit**

```bash
git add docs/capture/harness.ts
git commit -m "feat(capture): framed-panel boot harness with thumbnail serving"
```

---

## Task 5: Stills

**Files:**
- Create: `docs/capture/stills.ts`

**Interfaces:**
- Consumes: `bootPanel`, `BootOpts` from `./harness`.
- Produces: `export async function shootStills(browser: Browser, outDir: string): Promise<void>` which writes `panel.png`, `expanded.png`, `preferences.png`, `preset-quickpick.png`, `preset-active-bar.png` into `outDir`.

- [ ] **Step 1: Write the stills module**

Create `docs/capture/stills.ts`:

```ts
// One screenshot per README/wiki still, all framed identically via bootPanel.
import type { Browser, Page } from "@playwright/test";
import { join } from "node:path";
import { bootPanel } from "./harness";

// Let lazy thumbnails resolve + paint before shooting.
async function settle(page: Page): Promise<void> {
  await page.waitForTimeout(500);
}

export async function shootStills(browser: Browser, outDir: string): Promise<void> {
  // 1) Videos list (hero) — quick-pick layout.
  {
    const { context, page } = await bootPanel(browser, { layout: "quick-pick", still: true });
    await settle(page);
    await page.screenshot({ path: join(outDir, "panel.png") });
    await context.close();
  }

  // 2) Quick-pick preset menu open. Click the first row's preset affordance to
  //    reveal the quick-pick overlay (1–9). The row click in quick-pick mode opens
  //    the preset chooser rather than converting immediately.
  {
    const { context, page } = await bootPanel(browser, { layout: "quick-pick", still: true });
    await settle(page);
    // Open the quick-pick overlay for the first row (keyboard: focus list, press
    // the row's chevron). Fall back to clicking the row name to expand controls.
    await page.locator(".view-videos .row").first().click();
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

  // 5) Expanded row showing the enlarged preview + preset chips. Force the big
  //    card by re-pointing the resolved thumb to the @big asset before expanding.
  {
    const { context, page } = await bootPanel(browser, { layout: "quick-pick", still: true });
    await settle(page);
    // Expand the first row. The expand affordance is the row's chevron/expand
    // button; the keyboard 'e' also expands the focused row.
    const firstRow = page.locator(".view-videos .row").first();
    await firstRow.click();
    await page.keyboard.press("e");
    await page.waitForSelector(".view-videos .row.is-expanded .preview-thumb", { state: "visible" });
    // Swap the preview image to the high-res @big card for a crisp hero.
    await page.evaluate(() => {
      const img = document.querySelector<HTMLImageElement>(
        ".row.is-expanded .preview-thumb",
      );
      if (img && !img.src.endsWith("%40big")) img.src = img.src + "%40big";
    });
    await page.waitForTimeout(400);
    await page.screenshot({ path: join(outDir, "expanded.png") });
    await context.close();
  }
}
```

> **Interaction caveat:** the exact affordance to open the quick-pick overlay / expand a row depends on the current list DOM. During implementation, run the panel still first, then use the screenshots to confirm selectors (`.view-videos .row`, `.row.is-expanded`, `.preview-thumb`, the quick-pick overlay class). Adjust the click targets to match the real DOM rather than guessing — verify each shot visually before moving on. If a selector differs, fix it here; do not change product code.

- [ ] **Step 2: Add a temporary runner to shoot stills against a running dev server**

These need the Vite dev server on port 1431. Start it in one shell and run a throwaway driver:

```bash
# Shell A: dev server on the capture port
bun run vite --port 1431 --strictPort
```

```bash
# Shell B: shoot into a scratch dir
bun -e '
  import { chromium } from "@playwright/test";
  import { shootStills } from "./docs/capture/stills.ts";
  const b = await chromium.launch();
  await shootStills(b, "/tmp/cap");
  await b.close();
  console.log("done");
'
```
Expected: `done`, and `/tmp/cap` contains all five PNGs.

- [ ] **Step 3: Verify each still visually**

Open each of `/tmp/cap/panel.png`, `preset-quickpick.png`, `preset-active-bar.png`, `preferences.png`, `expanded.png`. Confirm: framed on the backdrop with a soft shadow, colored emoji thumbnails (not gray), no OS chrome, no terminal, the intended state visible. Fix selectors/timing in `stills.ts` until all five are clean.

- [ ] **Step 4: Commit**

```bash
git add docs/capture/stills.ts
git commit -m "feat(capture): framed stills for panel, presets, prefs, expanded"
```

---

## Task 6: Conversion GIF

**Files:**
- Create: `docs/capture/gif.ts`

**Interfaces:**
- Consumes: `bootPanel`, `BASE_URL` from `./harness`; `demoRecents` from `./demo-data`.
- Produces: `export async function shootGif(browser: Browser, outPath: string, scratchDir: string): Promise<void>` — records the animation and writes the optimized GIF to `outPath`.

- [ ] **Step 1: Write the GIF module**

Create `docs/capture/gif.ts`:

```ts
// Records the one-click conversion story to webm, then encodes a looping palette
// GIF with ffmpeg. The story: panel open → click a recording → progress ramps
// 0→100% via encode:state → "N% smaller" result holds → loop.
import type { Browser, Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { readdirSync, renameSync, rmSync } from "node:fs";
import { join } from "node:path";
import { bootPanel } from "./harness";
import { demoRecents } from "./demo-data";
import type { JobState, Phase } from "../../src/lib/ipc";

const STAR = demoRecents().find((r) => r.name.startsWith("ranked-match"))!; // 895 MB → big drop

function job(phase: Phase, progress: number, over: Partial<JobState> = {}): JobState {
  return {
    id: "demo-job",
    inputPath: STAR.path,
    inputName: STAR.name,
    outputPath: phase === "done" ? STAR.path.replace(/\.mov$/, " (tamped).mp4") : null,
    presetId: "discord-10mb",
    presetHash: "h",
    phase,
    progress,
    inputBytes: STAR.sizeBytes,
    outputBytes: phase === "done" ? 9_600_000 : null,
    reused: false,
    part: null,
    error: null,
    postError: null,
    ...over,
  };
}

async function emit(page: Page, state: JobState): Promise<void> {
  await page.evaluate((s) => {
    (window as unknown as { __E2E__: { emit: (e: string, p: unknown) => void } }).__E2E__.emit(
      "encode:state",
      s,
    );
  }, state as unknown as Record<string, unknown>);
}

export async function shootGif(browser: Browser, outPath: string, scratchDir: string): Promise<void> {
  rmSync(scratchDir, { recursive: true, force: true });
  const { context, page } = await bootPanel(browser, {
    layout: "quick-pick",
    recordVideoDir: scratchDir,
  });

  // Beat 1: hold on the list.
  await page.waitForTimeout(900);

  // Beat 2: click the star recording to convert it (quick-pick default preset).
  const row = page.locator(".view-videos .row", { hasText: STAR.name });
  await row.scrollIntoViewIfNeeded();
  await row.hover();
  await page.waitForTimeout(250);
  await row.click();

  // Beat 3: drive progress. The app's enqueue mock returns a job id; we animate
  // the real progress UI by emitting encode:state frames.
  await emit(page, job("queued", 0));
  await page.waitForTimeout(250);
  for (let p = 0; p <= 100; p += 5) {
    await emit(page, job("pass1", p / 100, { progress: p / 100 }));
    await page.waitForTimeout(90);
  }
  // Beat 4: done — the "N% smaller" payoff. Hold a beat, then loop.
  await emit(page, job("done", 1));
  await page.waitForTimeout(1400);

  // Close the context to flush the webm to disk.
  await context.close();

  const webm = readdirSync(scratchDir).find((f) => f.endsWith(".webm"));
  if (!webm) throw new Error("no webm recorded in " + scratchDir);
  const src = join(scratchDir, webm);

  // Two-pass palette GIF: generate an optimized palette, then apply it. Source is
  // 532px wide; displayed at 360 in the README → downscaled, crisp. fps 15 keeps
  // it smooth without bloating the file.
  const palette = join(scratchDir, "palette.png");
  const vf = "fps=15,scale=532:-1:flags=lanczos";
  execFileSync("ffmpeg", ["-y", "-i", src, "-vf", `${vf},palettegen=stats_mode=diff`, palette]);
  execFileSync("ffmpeg", [
    "-y", "-i", src, "-i", palette,
    "-lavfi", `${vf} [x]; [x][1:v] paletteuse=dither=bayer:bayer_scale=3`,
    "-loop", "0", outPath,
  ]);
}
```

- [ ] **Step 2: Shoot the GIF against the running dev server**

With the dev server still on port 1431 (Task 5 Shell A):

```bash
bun -e '
  import { chromium } from "@playwright/test";
  import { shootGif } from "./docs/capture/gif.ts";
  const b = await chromium.launch();
  await shootGif(b, "/tmp/cap/convert.gif", "/tmp/cap/gif");
  await b.close();
  console.log("done");
'
```
Expected: `done`; `/tmp/cap/convert.gif` exists.

- [ ] **Step 3: Verify the GIF**

Run: `file /tmp/cap/convert.gif && ls -la /tmp/cap/convert.gif`
Open it: confirm it loops, shows the click → progress 0→100% → "N% smaller" result, framed on the backdrop with the soft shadow, colored thumbnails, no terminal/OS chrome. Tune `fps`/`scale`/hold timings in `gif.ts` if the file is too large or the motion stutters.

- [ ] **Step 4: Commit**

```bash
git add docs/capture/gif.ts
git commit -m "feat(capture): scripted conversion GIF via encode:state + ffmpeg"
```

---

## Task 7: Orchestrator + runbook + wire-up

**Files:**
- Create: `docs/capture/shoot.ts`
- Create: `docs/capture/README.md`
- Modify: `package.json` (add `assets:shoot` script)
- Modify: `docs/panel.png`, `docs/expanded.png`, `docs/preferences.png`, `docs/preset-quickpick.png`, `docs/preset-active-bar.png`, `docs/convert.gif` (regenerated outputs)

**Interfaces:**
- Consumes: `shootStills` from `./stills`; `shootGif` from `./gif`.
- Produces: a `bun docs/capture/shoot.ts` entry point that starts Vite on 1431, regenerates thumbs, shoots all stills + the GIF into `docs/`, then stops Vite.

- [ ] **Step 1: Write the orchestrator**

Create `docs/capture/shoot.ts`:

```ts
// One command to regenerate every README/wiki asset. Starts the Vite dev server
// on the capture port, regenerates thumbnails, shoots all stills + the GIF into
// docs/, then tears the server down.
import { chromium } from "@playwright/test";
import { spawn } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as sleep } from "node:timers/promises";
import { genThumbs } from "./gen-thumbs";
import { shootStills } from "./stills";
import { shootGif } from "./gif";
import { BASE_URL } from "./harness";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = join(HERE, "..", "..");
const DOCS = join(REPO, "docs");
const SCRATCH = join(REPO, "node_modules", ".cache", "capture");

async function waitForServer(url: string, tries = 60): Promise<void> {
  for (let i = 0; i < tries; i++) {
    try {
      const r = await fetch(url);
      if (r.ok) return;
    } catch {
      /* not up yet */
    }
    await sleep(500);
  }
  throw new Error("dev server never came up at " + url);
}

async function main(): Promise<void> {
  console.log("• regenerating thumbnails");
  await genThumbs();

  console.log("• starting vite on the capture port");
  const vite = spawn("bun", ["run", "vite", "--port", "1431", "--strictPort"], {
    cwd: REPO,
    stdio: "ignore",
  });

  try {
    await waitForServer(BASE_URL);
    const browser = await chromium.launch();
    try {
      console.log("• shooting stills");
      await shootStills(browser, DOCS);
      console.log("• shooting the conversion gif");
      await shootGif(browser, join(DOCS, "convert.gif"), join(SCRATCH, "gif"));
    } finally {
      await browser.close();
    }
  } finally {
    vite.kill("SIGTERM");
  }
  console.log("✓ assets regenerated in docs/");
}

await main();
```

- [ ] **Step 2: Add the package.json script**

In `package.json`, under `"scripts"`, add:

```json
"assets:shoot": "bun docs/capture/shoot.ts",
```

- [ ] **Step 3: Write the runbook**

Create `docs/capture/README.md`:

```markdown
# Capture toolkit

Regenerates the README/wiki screenshots and the demo GIF from the **real**
frontend, driven by the Tauri-IPC mock (`e2e/mock-ipc.ts`) with curated demo
data. No live app, no screen recording — reproducible from one command.

## Prerequisites

- `ffmpeg` on your `PATH` (`brew install ffmpeg`).
- Repo deps installed (`bun install`).

## Regenerate everything

```bash
bun run assets:shoot
```

This regenerates the emoji thumbnails, starts a throwaway Vite server on port
1431, shoots all stills + the GIF into `docs/`, and stops the server.

## What it produces

- `docs/panel.png` — Videos list (README hero)
- `docs/preset-quickpick.png`, `docs/preset-active-bar.png` — preset layouts
- `docs/preferences.png` — Preferences tab
- `docs/expanded.png` — an expanded row with preview + preset chips
- `docs/convert.gif` — the one-click conversion demo

## Editing the demo cast

Edit `docs/capture/emoji.ts` (names, emoji, sizes, durations). Both the
thumbnails and the fixtures derive from it. Re-run `assets:shoot`.

## How it works

`harness.ts` boots the panel framed on a soft-shadow backdrop (`frame.ts`),
seeds `window.__E2E__` with curated recents/conversions (`demo-data.ts`), and
serves the generated emoji cards (`thumbs/`) by intercepting the
`asset.localhost` URLs the app resolves via `convertFileSrc()`. `gif.ts` drives
the real progress UI by emitting `encode:state` events, records a webm, and
encodes a looping palette GIF with ffmpeg.
```

- [ ] **Step 4: Regenerate the real assets**

Run: `bun run assets:shoot`
Expected: `✓ assets regenerated in docs/`. Confirm `git status` shows the six `docs/*.png|gif` files modified.

- [ ] **Step 5: Verify the README renders**

Open `README.md` in a Markdown preview (or push a branch and view on GitHub). Confirm `docs/panel.png` and `docs/convert.gif` look clean, framed, and professional, and the GIF loops. Spot-check the wiki stills.

- [ ] **Step 6: Commit**

```bash
git add docs/capture/shoot.ts docs/capture/README.md package.json \
  docs/panel.png docs/expanded.png docs/preferences.png \
  docs/preset-quickpick.png docs/preset-active-bar.png docs/convert.gif
git commit -m "feat(capture): one-command asset regeneration + refreshed README/wiki assets"
```

---

## Self-Review notes

- **Spec coverage:** capture harness (Tasks 4), reuse of `e2e/mock-ipc.ts` (Task 4), curated single-source fixtures (Tasks 1–2), demo thumbnails (Task 1), soft-shadow framing (Task 3), all five stills refreshed (Task 5), scripted `encode:state` GIF + ffmpeg palette encode (Task 6), one-command reproducibility + runbook (Task 7), GIF-only output / no CI wiring / no product changes (Global Constraints) — all covered.
- **CI isolation:** the toolkit is standalone scripts outside `e2e/`, never matched by `bunx playwright test` (`testDir: "./e2e"`), and uses port 1431 (vs 1420 dev, 1430 E2E).
- **Type consistency:** `JobState`/`Phase`/`RecentVideo`/`ConversionRecord`/`Settings` imported from `src/lib/ipc.ts`; `CastMember`/`CAST` defined in Task 1 and consumed unchanged in Task 2; `bootPanel`/`BootOpts`/`BASE_URL` defined in Task 4 and consumed in Tasks 5–6.
- **Known verification-sensitive spots:** exact list/quick-pick/expand DOM selectors in Task 5 and the GIF beat timings in Task 6 are confirmed visually during implementation (each task ends with an eyeball check), since screenshot fidelity can't be asserted purely in code.
```

