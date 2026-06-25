// Drag-and-drop story GIF for the README, shot against the REAL frontend via the
// Tauri-IPC mock. Output: docs/dragdrop.gif.
//
// Story (~6-7s, looping):
//   1) Panel open (active-bar). The emulated cursor moves to the header PIN
//      button (#pin-btn, "Keep panel open"), hovers, then CLICKS it on — the app
//      sets aria-pressed="true" and highlights it (.is-on). "Pin it so you can
//      drop onto it."
//   2) A fake file card (video glyph + "screen-recording.mov" + "0:38 · 230 MB")
//      appears at the top-right edge and the cursor drags it toward the panel
//      centre.
//   3) As it enters the panel the app's real .drop-overlay is revealed (we flip
//      overlay.hidden=false and set .drop-big to the live drop hint, e.g.
//      "Drop to compress with Discord (10MB)").
//   4) DROP: overlay hides, the card vanishes, and we emit encode:state frames
//      ("queued" → "pass1" 0→1 → "done" outputBytes≈9.4 MB) so the activity
//      drawer shows the "✓ 9.4 MB" payoff for screen-recording.mov.
//   5) Hold ~1s, then loop (the unpin + reset are done off-frame at the very end
//      of the recording window so the loop point reads cleanly).
//
// This file is self-contained and bun-runnable:  bun docs/capture/scenes/dragdrop/dragdrop.ts
// It starts its OWN Vite on port 1432 (NOT 1431, so it never collides with the
// main toolkit's shoot.ts) and tears it down afterwards.
import { chromium, type Browser, type BrowserContext, type Page } from "@playwright/test";
import { spawn } from "node:child_process";
import { mkdirSync, readFileSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as sleep } from "node:timers/promises";
import { installTauriMock } from "../../../../e2e/mock-ipc";
import { defaultSettings } from "../../../../e2e/canned";
import { demoRecents, demoConversions, THUMB_BY_PATH } from "../../demo-data";
import { frameCss, GIF_PAD, viewport } from "../../frame";
import { encodeClip, findWebm } from "../../encode";
import type { JobState, Phase, Settings } from "../../../../src/lib/ipc";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = join(HERE, "..", "..", "..", "..");
const DOCS = join(REPO, "docs");
const THUMBS = join(REPO, "docs", "capture", "thumbs");
const SCRATCH = join(REPO, "node_modules", ".cache", "capture", "dragdrop");
const OUT_BASE = join(DOCS, "dragdrop"); // → dragdrop.mp4 / .webm / .poster.png
const PORT = 1432;

// Record at 2× CSS zoom so the panel text is retina-crisp. Cursor math stays in
// the logical (un-zoomed) coordinate space; zoom scales it uniformly. The only
// zoom-affected input is pin.boundingBox(), divided by SCALE below.
const SCALE = 2;
const BASE_URL = `http://localhost:${PORT}`;

// The dropped file's believable specs.
const DROP_NAME = "screen-recording.mov";
const DROP_PATH = "/Users/demo/Movies/screen-recording.mov";
const DROP_META = "0:38 · 230 MB";
const DROP_INPUT_BYTES = 230 * 1024 * 1024; // 230 MB
const DROP_OUTPUT_BYTES = 9_400_000; // ≈ 9.4 MB payoff

// ---------------------------------------------------------------------------
// Boot a framed page exactly like docs/capture/harness.ts, but against PORT.
// ---------------------------------------------------------------------------
async function bootPanel(
  browser: Browser,
  recordVideoDir: string,
): Promise<{ context: BrowserContext; page: Page }> {
  const settings = {
    ...defaultSettings(),
    videosLayout: "active-bar", // a single active preset ⇒ a named drop hint
    theme: "dark",
    onboardingSeen: true,
  } as unknown as Record<string, unknown> as Settings;

  const pad = GIF_PAD;
  // The video viewport is 2× the logical frame (zoom fills it); cursor math in
  // drive() keeps using the logical viewport(pad).
  const logical = viewport(pad);
  const view = { width: logical.width * SCALE, height: logical.height * SCALE };
  const context = await browser.newContext({
    viewport: view,
    deviceScaleFactor: 1,
    recordVideo: { dir: recordVideoDir, size: view },
  });

  // Serve thumbnails the same way harness.ts does: the app resolves a thumb path
  // through convertFileSrc() → https://asset.localhost/<encoded path>; map back
  // to the slug PNG.
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

  // 1) Seed window.__E2E__ BEFORE the mock installs (mirrors harness.ts).
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
          recent_thumb: (a: { path: string }) =>
            data.thumbByPath[a.path] ? a.path : null,
          recent_duration: (a: { path: string }) => data.durationByPath[a.path] ?? null,
          ensure_preview: () => new Promise(() => {}),
        },
      };
    },
    {
      recents: demoRecents(),
      conversions: demoConversions(),
      thumbByPath: THUMB_BY_PATH,
      durationByPath: Object.fromEntries(
        demoRecents().map((r) => [r.path, r.durationSecs ?? 0]),
      ),
    },
  );

  // 2) Install the Tauri global before main.ts boots.
  await page.addInitScript(installTauriMock, settings as unknown as Record<string, unknown>);

  await page.goto(BASE_URL);
  await page.waitForSelector(".panel .seg", { state: "attached" });
  await page.addStyleTag({ content: frameCss(pad) });
  // Zoom the document so the px-based panel renders at 2× into the doubled
  // viewport — crisp text. Cursor targets stay in logical coords (scaled by zoom).
  await page.addStyleTag({ content: `html { zoom: ${SCALE}; }` });

  return { context, page };
}

// ---------------------------------------------------------------------------
// Scene props injected into the page: an emulated macOS arrow cursor and the
// fake "file being dragged in from outside" card. Both float above the framed
// #app on <body> (pointer-events:none) so they never interfere with the app.
// ---------------------------------------------------------------------------
async function installSceneProps(page: Page): Promise<void> {
  await page.evaluate(() => {
    const cursorSvg = `
      <svg width="22" height="22" viewBox="0 0 22 22" fill="none" xmlns="http://www.w3.org/2000/svg">
        <path d="M4.5 2.2 L4.5 17.4 L8.6 13.6 L11.1 19.3 L13.4 18.3 L10.9 12.7 L16.3 12.5 Z"
              fill="#ffffff" stroke="#1b1b22" stroke-width="1.2" stroke-linejoin="round"/>
      </svg>`;

    const cursor = document.createElement("div");
    cursor.id = "emu-cursor";
    cursor.innerHTML = cursorSvg;
    Object.assign(cursor.style, {
      position: "fixed",
      left: "0px",
      top: "0px",
      width: "22px",
      height: "22px",
      zIndex: "999999",
      pointerEvents: "none",
      filter: "drop-shadow(0 2px 3px rgba(20,18,40,.45))",
      transition: "transform .12s ease",
      transform: "translate(-2px,-1px) scale(1)",
      willChange: "left, top, transform",
    } as CSSStyleDeclaration);
    document.body.appendChild(cursor);

    // The dragged-in file card. Brand-consistent dark chip with a purple video
    // glyph, the filename, and believable meta. Starts hidden.
    const card = document.createElement("div");
    card.id = "emu-file";
    card.innerHTML = `
      <div class="emu-file-glyph">
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
          <rect x="2.5" y="5" width="14" height="14" rx="3" fill="#7C5CFC"/>
          <path d="M17.5 10.2 L21.2 7.6 L21.2 16.4 L17.5 13.8 Z" fill="#7C5CFC"/>
        </svg>
      </div>
      <div class="emu-file-text">
        <div class="emu-file-name">screen-recording.mov</div>
        <div class="emu-file-meta">0:38 · 230 MB</div>
      </div>`;
    Object.assign(card.style, {
      position: "fixed",
      left: "0px",
      top: "0px",
      display: "flex",
      alignItems: "center",
      gap: "9px",
      padding: "9px 13px 9px 10px",
      borderRadius: "12px",
      background: "rgba(20,19,26,.96)",
      border: "1px solid rgba(124,92,252,.55)",
      boxShadow: "0 14px 34px -10px rgba(40,34,80,.55), 0 0 0 1px rgba(20,18,40,.10)",
      color: "#f2f1f7",
      fontFamily: "Montserrat, system-ui, sans-serif",
      zIndex: "999998",
      pointerEvents: "none",
      opacity: "0",
      transform: "translate(-50%,-50%) scale(.92)",
      transition: "opacity .2s ease, transform .2s ease",
      whiteSpace: "nowrap",
      willChange: "left, top, opacity, transform",
    } as CSSStyleDeclaration);
    document.body.appendChild(card);

    const style = document.createElement("style");
    style.textContent = `
      #emu-file .emu-file-glyph{
        width:34px;height:34px;border-radius:9px;flex:none;
        display:flex;align-items:center;justify-content:center;
        background:rgba(124,92,252,.16);
      }
      #emu-file .emu-file-name{font-size:12.5px;font-weight:600;letter-spacing:.1px;}
      #emu-file .emu-file-meta{font-size:11px;font-weight:500;color:#a7a4b8;margin-top:1px;}
    `;
    document.head.appendChild(style);
  });
}

// Move the emulated cursor smoothly to a viewport point over `ms` (eased),
// optionally carrying the file card pinned under the cursor tip.
async function glide(
  page: Page,
  to: { x: number; y: number },
  ms: number,
  carry = false,
): Promise<void> {
  const steps = Math.max(2, Math.round(ms / 28));
  await page.evaluate(
    async ({ to, steps, carry }) => {
      const cursor = document.getElementById("emu-cursor")!;
      const card = document.getElementById("emu-file")!;
      const from = {
        x: parseFloat(cursor.style.left) || 0,
        y: parseFloat(cursor.style.top) || 0,
      };
      const ease = (t: number) => (t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2);
      for (let i = 1; i <= steps; i++) {
        const p = ease(i / steps);
        const x = from.x + (to.x - from.x) * p;
        const y = from.y + (to.y - from.y) * p;
        cursor.style.left = `${x}px`;
        cursor.style.top = `${y}px`;
        if (carry) {
          // Card sits just below-right of the cursor tip, like a real OS drag.
          card.style.left = `${x + 18}px`;
          card.style.top = `${y + 16}px`;
        }
        await new Promise((r) => setTimeout(r, 28));
      }
    },
    { to, steps, carry },
  );
}

async function setCursor(page: Page, x: number, y: number): Promise<void> {
  await page.evaluate(
    ({ x, y }) => {
      const c = document.getElementById("emu-cursor")!;
      c.style.left = `${x}px`;
      c.style.top = `${y}px`;
    },
    { x, y },
  );
}

// Brief press-down feedback on the cursor (scale down then back).
async function press(page: Page): Promise<void> {
  await page.evaluate(() => {
    const c = document.getElementById("emu-cursor")!;
    c.style.transform = "translate(-2px,-1px) scale(.8)";
  });
  await sleep(130);
  await page.evaluate(() => {
    const c = document.getElementById("emu-cursor")!;
    c.style.transform = "translate(-2px,-1px) scale(1)";
  });
  await sleep(60);
}

function job(phase: Phase, progress: number, over: Partial<JobState> = {}): JobState {
  return {
    id: "drop-job",
    inputPath: DROP_PATH,
    inputName: DROP_NAME,
    outputPath: phase === "done" ? DROP_PATH.replace(/\.mov$/, " (tamped).mp4") : null,
    presetId: "discord-10mb",
    presetHash: "h",
    phase,
    progress,
    inputBytes: DROP_INPUT_BYTES,
    outputBytes: phase === "done" ? DROP_OUTPUT_BYTES : null,
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

// Reveal / hide the app's REAL .drop-overlay and set its .drop-big hint to the
// live wording the list view would produce (active-bar ⇒ "Drop to compress with
// Discord (10MB)").
async function setDropOverlay(page: Page, on: boolean): Promise<void> {
  await page.evaluate((show) => {
    const overlay = document.querySelector<HTMLElement>(".drop-overlay");
    if (!overlay) return;
    if (show) {
      const big = overlay.querySelector<HTMLElement>(".drop-big");
      // Mirror list.ts currentDropHint() for active-bar with the Discord preset.
      if (big) big.textContent = "Drop to compress with Discord (10MB)";
    }
    overlay.hidden = !show;
  }, on);
}

// ---------------------------------------------------------------------------
// Drive the whole story.
// ---------------------------------------------------------------------------
async function drive(page: Page): Promise<void> {
  const view = viewport(GIF_PAD);
  const cx = view.width / 2;
  const cy = view.height / 2;

  await installSceneProps(page);

  // Resting cursor start: lower-centre, off the panel chrome.
  await setCursor(page, cx, view.height - 70);
  await sleep(700); // beat: list settles (thumbnails resolve)

  // --- 1) Pin the panel open ------------------------------------------------
  const pin = page.locator("#pin-btn");
  const pinBox = await pin.boundingBox();
  if (!pinBox) throw new Error("pin button not found");
  // boundingBox() reports zoomed coords; convert back to the logical space the
  // cursor (a zoom-scaled fixed element) is positioned in.
  const pinTarget = {
    x: (pinBox.x + pinBox.width / 2) / SCALE,
    y: (pinBox.y + pinBox.height / 2) / SCALE,
  };

  await glide(page, pinTarget, 620);
  await sleep(280); // hover dwell
  await press(page);
  await pin.click();
  // Verify it actually pinned (aria-pressed + .is-on highlight).
  const pressed = await pin.getAttribute("aria-pressed");
  if (pressed !== "true") throw new Error(`pin did not toggle on (aria-pressed=${pressed})`);
  await sleep(520); // let the highlight read

  // --- 2) File dragged in from outside (top-right edge) --------------------
  const start = { x: view.width - 46, y: 70 };
  await glide(page, start, 520);
  await page.evaluate(
    ({ x, y }) => {
      const card = document.getElementById("emu-file")!;
      card.style.left = `${x + 18}px`;
      card.style.top = `${y + 16}px`;
      card.style.opacity = "1";
      card.style.transform = "translate(-50%,-50%) scale(1)";
    },
    start,
  );
  // The drop overlay appears IMMEDIATELY, the instant the file enters the window
  // (like a real OS drag-enter) — not after the drag travels inward.
  await setDropOverlay(page, true);
  await sleep(300); // card + overlay register together

  // --- 3) Drag toward the panel centre (overlay already showing) ----------
  // Settle a little above centre so the dragged card clears the overlay's
  // ".drop-big" hint underneath it (both stay fully legible).
  await glide(page, { x: cx, y: cy - 64 }, 620, true);
  await sleep(520); // hold on the "Drop to compress with Discord (10MB)" hint

  // --- 4) DROP ------------------------------------------------------------
  await press(page); // release
  await setDropOverlay(page, false);
  await page.evaluate(() => {
    const card = document.getElementById("emu-file")!;
    card.style.opacity = "0";
    card.style.transform = "translate(-50%,-50%) scale(.82)";
  });
  await sleep(220);

  // Conversion drives the activity drawer for screen-recording.mov.
  await emit(page, job("queued", 0));
  await sleep(220);
  for (let p = 0; p <= 100; p += 5) {
    await emit(page, job("pass1", p / 100, { progress: p / 100 }));
    await sleep(70);
  }
  // Done — the "✓ 9.4 MB" payoff in the drawer.
  await emit(page, job("done", 1));

  // Drift the cursor gently off the drawer so the held end frame is clean.
  await glide(page, { x: cx, y: view.height - 70 }, 360);
  await sleep(1100); // hold the payoff, then loop
}

async function waitForServer(url: string, tries = 80): Promise<void> {
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
  rmSync(SCRATCH, { recursive: true, force: true });
  mkdirSync(SCRATCH, { recursive: true });

  console.log(`• starting vite on :${PORT}`);
  const vite = spawn("bun", ["run", "vite", "--port", String(PORT), "--strictPort"], {
    cwd: REPO,
    stdio: "ignore",
  });

  try {
    await waitForServer(BASE_URL);
    const browser = await chromium.launch();
    try {
      console.log("• shooting the drag-and-drop gif");
      const { context, page } = await bootPanel(browser, SCRATCH);
      await drive(page);
      await context.close(); // flush the webm
    } finally {
      await browser.close();
    }
  } finally {
    vite.kill("SIGTERM");
  }

  // Transcode to MP4 + WebM + poster. Poster ~4.4s: the drop overlay + hint.
  encodeClip(findWebm(SCRATCH), OUT_BASE, { webpWidth: 600, webpFps: 12, webpQ: 28 });
  console.log(`✓ ${OUT_BASE}.{webp,mp4}`);
}

await main();
