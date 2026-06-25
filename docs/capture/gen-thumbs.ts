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
    .card{width:100vw;height:100vh;display:flex;align-items:center;justify-content:center;
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
