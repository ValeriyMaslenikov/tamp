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
