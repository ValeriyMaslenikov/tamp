// Records the "multi-part" story to MP4 + poster. The LEFT panel is the REAL app
// (the Converted tab, booted against the Tauri-IPC mock with the curated split
// conversion) — its actual "Copy all" button and real success toast. The RIGHT
// panel is a Discord-flavored chat mock (an external app is fair to fake). An
// emulated cursor clicks Copy all, then pastes both parts into the chat.
//
//   Story (~6s, click-to-play): Converted tab shows duck-debugging-session.mov
//   split into 2 parts (9.8 + 9.6 MB) → cursor clicks the real "Copy all" →
//   real "Copied to clipboard" toast → cursor moves to the chat → both parts
//   slide in as attachments, each "under 10 MB".
//
// Run:  bun docs/capture/scenes/multipart/shoot-multipart.ts
import { chromium, type Browser, type BrowserContext, type Page } from "@playwright/test";
import { spawn } from "node:child_process";
import { mkdirSync, readFileSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as sleep } from "node:timers/promises";
import { installTauriMock } from "../../../../e2e/mock-ipc";
import { defaultSettings } from "../../../../e2e/canned";
import { demoRecents, demoConversions, THUMB_BY_PATH } from "../../demo-data";
import { encodeClip, findWebm } from "../../encode";
import type { Settings } from "../../../../src/lib/ipc";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = join(HERE, "..", "..", "..", "..");
const DOCS = join(REPO, "docs");
const THUMBS = join(REPO, "docs", "capture", "thumbs");
const OUT_BASE = join(DOCS, "multipart"); // → multipart.mp4 / .poster.png
const SCRATCH = join(REPO, "node_modules", ".cache", "capture", "multipart");
const PORT = 1433; // its own port (1431 = stills, 1432 = dragdrop)
const BASE_URL = `http://localhost:${PORT}`;

// Record at 2× CSS zoom for retina-crisp text; cursor targets stay in logical
// coords (boundingBox()/SCALE), zoom scales them uniformly.
const SCALE = 2;
const APP_W = 348;
const CHAT_W = 372;
const PANEL_H = 558;
const GAP = 20;
const PAD_X = 26;
const PAD_Y = 28;
const LOGICAL = { width: PAD_X * 2 + APP_W + GAP + CHAT_W, height: PAD_Y * 2 + PANEL_H };

// ── Composite framing + Discord-mock styling (light backdrop, matches frame.ts) ─
const COMPOSITE_CSS = `
  html, body { height: 100%; margin: 0; }
  body {
    background: radial-gradient(125% 125% at 50% 0%, #f3f2f8 0%, #e6e5ee 52%, #d5d4e0 100%);
    display: grid; place-items: center; overflow: hidden;
    font-family: Montserrat, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  }
  .mp-stage { display: flex; gap: ${GAP}px; align-items: center; }
  #app {
    width: ${APP_W}px; height: ${PANEL_H}px; flex: none;
    border-radius: 16px; overflow: hidden;
    box-shadow: 0 24px 55px -14px rgba(40,34,80,.40), 0 8px 22px -10px rgba(40,34,80,.30), 0 0 0 1px rgba(20,18,40,.06);
  }
  #mp-chat {
    width: ${CHAT_W}px; height: ${PANEL_H}px; flex: none;
    border-radius: 16px; overflow: hidden;
    display: flex; flex-direction: column; background: #313338;
    box-shadow: 0 24px 55px -14px rgba(40,34,80,.40), 0 8px 22px -10px rgba(40,34,80,.30), 0 0 0 1px rgba(20,18,40,.06);
    color: #dbdee1;
  }
  .mp-head { height: 44px; flex: none; display: flex; align-items: center; gap: 7px;
    padding: 0 14px; border-bottom: 1px solid rgba(0,0,0,.25); }
  .mp-hash { color: #80848e; font-size: 19px; font-weight: 500; line-height: 1; }
  .mp-chan { color: #f2f3f5; font-size: 13.5px; font-weight: 700; }
  .mp-body { flex: 1; display: flex; flex-direction: column; justify-content: flex-end; padding: 14px; gap: 10px; }
  .mp-msg { display: flex; gap: 9px; opacity: .92; }
  .mp-avatar { width: 30px; height: 30px; border-radius: 50%; flex: none;
    background: linear-gradient(135deg, #5865f2, #4752c4); }
  .mp-msg-name { font-size: 12px; font-weight: 700; color: #c9a0ff; }
  .mp-msg-name .when { color: #72767d; font-weight: 500; font-size: 10px; margin-left: 6px; }
  .mp-msg-text { font-size: 12px; color: #dbdee1; margin-top: 1px; }
  .mp-composer { flex: none; margin: 0 14px 14px; }
  .mp-attachments { display: flex; flex-direction: column; gap: 7px; margin-bottom: 8px; }
  .mp-attach {
    background: #2b2d31; border: 1px solid rgba(255,255,255,.06); border-radius: 9px;
    padding: 9px 11px; display: flex; align-items: center; gap: 10px;
    opacity: 0; transform: translateY(16px) scale(.98);
    transition: opacity .34s cubic-bezier(.2,.9,.25,1), transform .34s cubic-bezier(.2,.9,.25,1);
  }
  #mp-chat.is-pasted .mp-attach.a1 { opacity: 1; transform: none; }
  #mp-chat.is-pasted .mp-attach.a2 { opacity: 1; transform: none; transition-delay: .12s; }
  .mp-vic { width: 34px; height: 34px; flex: none; border-radius: 7px;
    background: linear-gradient(135deg, #3a3460, #2a2542); display: grid; place-items: center;
    border: 1px solid rgba(124,92,252,.3); }
  .mp-vic svg { width: 17px; height: 17px; }
  .mp-ainfo { min-width: 0; flex: 1; }
  .mp-aname { font-size: 11px; font-weight: 600; color: #f2f3f5;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .mp-asub { font-size: 9.5px; color: #b5bac1; margin-top: 2px; display: flex; align-items: center; gap: 5px; }
  .mp-ok { display: inline-flex; align-items: center; gap: 4px; color: #2dd4a7; font-weight: 700; font-size: 9.5px; }
  .mp-ok svg { width: 11px; height: 11px; }
  .mp-ring { width: 20px; height: 20px; flex: none; border-radius: 50%;
    background: rgba(45,212,167,.14); display: grid; place-items: center;
    transform: scale(0); transition: transform .3s cubic-bezier(.3,1.5,.5,1); }
  .mp-ring svg { width: 12px; height: 12px; }
  #mp-chat.is-pasted .mp-attach.a1 .mp-ring { transform: scale(1); transition-delay: .34s; }
  #mp-chat.is-pasted .mp-attach.a2 .mp-ring { transform: scale(1); transition-delay: .5s; }
  .mp-input { height: 42px; border-radius: 9px; background: #383a40; display: flex; align-items: center;
    padding: 0 12px; gap: 9px; position: relative; transition: box-shadow .25s ease; }
  #mp-chat.is-focus .mp-input { box-shadow: inset 0 0 0 1.5px rgba(124,92,252,.55); }
  .mp-plus { width: 22px; height: 22px; flex: none; border-radius: 50%; background: #b5bac1; color: #383a40;
    display: grid; place-items: center; font-size: 17px; line-height: 1; }
  .mp-ph { color: #6d7178; font-size: 12px; flex: 1; }
  .mp-paste { position: absolute; right: 10px; display: flex; gap: 4px; opacity: 0; transition: opacity .2s ease; }
  #mp-chat.is-focus .mp-paste { opacity: 1; }
  .mp-key { font-size: 9.5px; font-weight: 700; color: #dbdee1; background: rgba(255,255,255,.08);
    border: 1px solid rgba(255,255,255,.12); border-bottom-width: 2px; border-radius: 5px; padding: 2px 6px; line-height: 1; }
`;

const VIDEO_GLYPH = `<svg viewBox="0 0 24 24" fill="none"><rect x="3" y="5" width="18" height="14" rx="2.5" stroke="#a78bff" stroke-width="2"/><path d="M10 9.5l4 2.5-4 2.5z" fill="#a78bff"/></svg>`;
const CHECK = `<svg viewBox="0 0 24 24" fill="none"><path d="M5 12.5l4 4 10-11" stroke="#2dd4a7" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/></svg>`;

function attachHtml(cls: string, name: string, size: string): string {
  return `
    <div class="mp-attach ${cls}">
      <span class="mp-vic">${VIDEO_GLYPH}</span>
      <span class="mp-ainfo">
        <div class="mp-aname">${name}</div>
        <div class="mp-asub">${size}<span class="mp-ok">${CHECK}under 10 MB</span></div>
      </span>
      <span class="mp-ring">${CHECK}</span>
    </div>`;
}

const CHAT_HTML = `
  <section id="mp-chat">
    <div class="mp-head"><span class="mp-hash">#</span><span class="mp-chan">screenshares</span></div>
    <div class="mp-body">
      <div class="mp-msg">
        <span class="mp-avatar"></span>
        <span>
          <div class="mp-msg-name">teammate <span class="when">2:14 PM</span></div>
          <div class="mp-msg-text">did you record that duck bug? 🦆</div>
        </span>
      </div>
      <div class="mp-composer">
        <div class="mp-attachments">
          ${attachHtml("a1", "duck-debugging-session (tamped 1of2).mp4", "9.8 MB")}
          ${attachHtml("a2", "duck-debugging-session (tamped 2of2).mp4", "9.6 MB")}
        </div>
        <div class="mp-input">
          <span class="mp-plus">+</span>
          <span class="mp-ph">Message #screenshares</span>
          <span class="mp-paste"><span class="mp-key">⌘</span><span class="mp-key">V</span></span>
        </div>
      </div>
    </div>
  </section>`;

// ── Boot the real app (mirrors harness.ts) against PORT, at 2× ─────────────────
async function bootApp(browser: Browser, dir: string): Promise<{ context: BrowserContext; page: Page }> {
  const settings = {
    ...defaultSettings(),
    theme: "dark",
    onboardingSeen: true,
  } as unknown as Record<string, unknown> as Settings;

  const view = { width: LOGICAL.width * SCALE, height: LOGICAL.height * SCALE };
  const context = await browser.newContext({
    viewport: view,
    deviceScaleFactor: 1,
    recordVideo: { dir, size: view },
  });

  await context.route("https://asset.localhost/**", async (route) => {
    const url = new URL(route.request().url());
    const original = decodeURIComponent(url.pathname.replace(/^\//, ""));
    const big = original.endsWith("@big");
    const videoPath = original.replace(/@big$/, "");
    const slug = THUMB_BY_PATH[videoPath];
    if (!slug) return route.fulfill({ status: 404, body: "" });
    return route.fulfill({
      contentType: "image/png",
      body: readFileSync(join(THUMBS, `${slug}${big ? "@big" : ""}.png`)),
    });
  });

  const page = await context.newPage();
  await page.addInitScript(
    (data: { recents: unknown; conversions: unknown; thumbByPath: Record<string, string> }) => {
      (window as unknown as { __E2E__: unknown }).__E2E__ = {
        mocks: {
          list_recents: data.recents,
          list_conversions: data.conversions,
          recent_thumb: (a: { path: string }) => (data.thumbByPath[a.path] ? a.path : null),
          ensure_preview: () => new Promise(() => {}),
        },
      };
    },
    { recents: demoRecents(), conversions: demoConversions(), thumbByPath: THUMB_BY_PATH },
  );
  await page.addInitScript(installTauriMock, settings as unknown as Record<string, unknown>);

  await page.goto(BASE_URL);
  await page.waitForSelector(".panel .seg", { state: "attached" });
  return { context, page };
}

// ── Emulated macOS cursor (same approach as the drag-drop scene) ───────────────
async function installCursor(page: Page): Promise<void> {
  await page.evaluate(() => {
    const c = document.createElement("div");
    c.id = "emu-cursor";
    c.innerHTML = `<svg width="22" height="22" viewBox="0 0 22 22" fill="none" xmlns="http://www.w3.org/2000/svg">
      <path d="M4.5 2.2 L4.5 17.4 L8.6 13.6 L11.1 19.3 L13.4 18.3 L10.9 12.7 L16.3 12.5 Z" fill="#fff" stroke="#1b1b22" stroke-width="1.2" stroke-linejoin="round"/></svg>`;
    Object.assign(c.style, {
      position: "fixed", left: "0px", top: "0px", width: "22px", height: "22px",
      zIndex: "999999", pointerEvents: "none", filter: "drop-shadow(0 2px 3px rgba(20,18,40,.45))",
      transition: "transform .12s ease", transform: "translate(-2px,-1px) scale(1)", willChange: "left, top, transform",
    } as CSSStyleDeclaration);
    document.body.appendChild(c);
  });
}

async function setCursor(page: Page, x: number, y: number): Promise<void> {
  await page.evaluate(({ x, y }) => {
    const c = document.getElementById("emu-cursor")!;
    c.style.left = `${x}px`;
    c.style.top = `${y}px`;
  }, { x, y });
}

async function glide(page: Page, to: { x: number; y: number }, ms: number): Promise<void> {
  const steps = Math.max(2, Math.round(ms / 28));
  await page.evaluate(async ({ to, steps }) => {
    const c = document.getElementById("emu-cursor")!;
    const from = { x: parseFloat(c.style.left) || 0, y: parseFloat(c.style.top) || 0 };
    const ease = (t: number) => (t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2);
    for (let i = 1; i <= steps; i++) {
      const p = ease(i / steps);
      c.style.left = `${from.x + (to.x - from.x) * p}px`;
      c.style.top = `${from.y + (to.y - from.y) * p}px`;
      await new Promise((r) => setTimeout(r, 28));
    }
  }, { to, steps });
}

async function press(page: Page): Promise<void> {
  await page.evaluate(() => {
    document.getElementById("emu-cursor")!.style.transform = "translate(-2px,-1px) scale(.8)";
  });
  await sleep(130);
  await page.evaluate(() => {
    document.getElementById("emu-cursor")!.style.transform = "translate(-2px,-1px) scale(1)";
  });
  await sleep(60);
}

/** Centre of an element in logical (un-zoomed) coords. */
async function centerOf(page: Page, selector: string): Promise<{ x: number; y: number }> {
  const box = await page.locator(selector).first().boundingBox();
  if (!box) throw new Error("not found: " + selector);
  return { x: (box.x + box.width / 2) / SCALE, y: (box.y + box.height / 2) / SCALE };
}

async function drive(page: Page): Promise<void> {
  // Switch to the Converted tab and expand the split group so its parts + the
  // "Copy all" button are visible.
  await page.locator("#tab-converted").click();
  await page.waitForSelector(".view-converted .conv-tree", { state: "visible" });
  const group = page.locator(".view-converted .conv-tree").first();
  await group.locator(".conv-tree-parent").click();
  await page.waitForSelector(".view-converted .conv-tree.is-open .conv-children", { state: "visible" });

  // Lay the real app + the chat side by side; add the cursor.
  await page.addStyleTag({ content: COMPOSITE_CSS });
  await page.evaluate((chatHtml) => {
    const stage = document.createElement("div");
    stage.className = "mp-stage";
    document.body.appendChild(stage);
    stage.appendChild(document.getElementById("app")!); // move the real app in
    stage.insertAdjacentHTML("beforeend", chatHtml);
  }, CHAT_HTML);
  await page.addStyleTag({ content: `html { zoom: ${SCALE}; }` });
  await installCursor(page);
  await page.waitForTimeout(150);

  const copyBtn = group.getByRole("button", { name: "Copy all" });
  const copyAt = await centerOf(page, "#app .view-converted .conv-tree .conv-tree-parent");
  const copyBox = await copyBtn.boundingBox();
  const copyTarget = copyBox
    ? { x: (copyBox.x + copyBox.width / 2) / SCALE, y: (copyBox.y + copyBox.height / 2) / SCALE }
    : copyAt;
  const inputAt = await centerOf(page, "#mp-chat .mp-input");

  // Rest cursor lower-left, settle.
  await setCursor(page, copyTarget.x - 30, copyTarget.y + 80);
  await sleep(650);

  // 1) Click the REAL "Copy all" → real "Copied to clipboard" toast.
  await glide(page, copyTarget, 600);
  await sleep(220);
  await press(page);
  await copyBtn.click();
  await sleep(900); // hold on the real success toast

  // 2) Move to the chat, focus, paste both parts.
  await glide(page, { x: inputAt.x, y: inputAt.y }, 720);
  await page.evaluate(() => document.getElementById("mp-chat")!.classList.add("is-focus"));
  await press(page);
  await sleep(220);
  await page.evaluate(() => document.getElementById("mp-chat")!.classList.add("is-pasted"));
  await sleep(1600); // hold on the two attachments + checks
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
      const { context, page } = await bootApp(browser, SCRATCH);
      await drive(page);
      await context.close(); // flush the webm
    } finally {
      await browser.close();
    }
  } finally {
    vite.kill("SIGTERM");
  }

  // ss 0.75 trims the boot + the brief 1×→2× zoom settle so the clip opens on
  // the clean composite (real Converted tab beside the chat, cursor resting).
  encodeClip(findWebm(SCRATCH), OUT_BASE, { ss: 0.75, webpWidth: 900, webpFps: 14, webpQ: 40 });
  console.log(`✓ ${OUT_BASE}.{webp,mp4}`);
}

await main();
