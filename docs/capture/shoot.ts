// One command to regenerate every README/wiki asset. Starts the Vite dev server
// on the capture port, regenerates thumbnails, shoots all stills + the GIF into
// docs/, then tears the server down.
import { chromium } from "@playwright/test";
import { spawn, execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as sleep } from "node:timers/promises";
import { genThumbs } from "./gen-thumbs";
import { shootStills } from "./stills";
import { shootClip } from "./clip";
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
      console.log("• shooting the one-click demo clip");
      await shootClip(browser, join(DOCS, "demo"), join(SCRATCH, "gif"));
    } finally {
      await browser.close();
    }
  } finally {
    vite.kill("SIGTERM");
  }

  // The two story clips are self-contained recorders (multipart needs no server;
  // drag-drop spawns its own Vite on port 1432), so run them as subprocesses.
  console.log("• shooting the multi-part clip");
  execFileSync("bun", ["docs/capture/scenes/multipart/shoot-multipart.ts"], {
    cwd: REPO,
    stdio: "inherit",
  });
  console.log("• shooting the drag-and-drop clip");
  execFileSync("bun", ["docs/capture/scenes/dragdrop/dragdrop.ts"], {
    cwd: REPO,
    stdio: "inherit",
  });

  console.log("✓ assets regenerated in docs/");
}

await main();
