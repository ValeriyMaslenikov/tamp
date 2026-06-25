// Records the one-click conversion story to webm, then encodes a looping palette
// GIF with ffmpeg. The story (active-bar layout, so a single row click converts):
// panel open → click a recording → progress ramps 0→100% via encode:state →
// "N% smaller" result holds → loop.
import type { Browser, Page } from "@playwright/test";
import { execFileSync, spawnSync } from "node:child_process";
import { readdirSync, rmSync, statSync } from "node:fs";
import { join } from "node:path";
import { bootPanel } from "./harness";
import { demoRecents } from "./demo-data";
import { GIF_PAD, viewport } from "./frame";
import type { JobState, Phase } from "../../src/lib/ipc";

const STAR = demoRecents().find((r) => r.name.startsWith("ranked-match"))!; // 260 MB / 0:42 → 9.4 MB

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
    outputBytes: phase === "done" ? 9_400_000 : null,
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

export async function shootGif(
  browser: Browser,
  outPath: string,
  scratchDir: string,
): Promise<void> {
  rmSync(scratchDir, { recursive: true, force: true });
  // active-bar layout: one click on a row converts with the visible preset.
  // Tight framing so the panel fills the frame and its text is legible when the
  // GIF is embedded near its native width.
  const { context, page } = await bootPanel(browser, {
    layout: "active-bar",
    recordVideoDir: scratchDir,
    pad: GIF_PAD,
  });

  // Beat 1: hold on the list (thumbnails settled).
  await page.waitForTimeout(1100);

  // Beat 2: hover + click the star recording — one click starts the conversion.
  const row = page.locator(".view-videos .row", { hasText: STAR.name });
  await row.scrollIntoViewIfNeeded();
  await row.hover();
  await page.waitForTimeout(350);
  await row.click();

  // Beat 3: drive progress. The app's enqueue is mocked; we animate the real
  // progress UI by emitting encode:state frames.
  await emit(page, job("queued", 0));
  await page.waitForTimeout(250);
  for (let p = 0; p <= 100; p += 5) {
    await emit(page, job("pass1", p / 100, { progress: p / 100 }));
    await page.waitForTimeout(95);
  }

  // Beat 4: done — the "N% smaller" payoff. Hold, then loop.
  await emit(page, job("done", 1));
  await page.waitForTimeout(1700);

  // Close the context to flush the webm to disk.
  await context.close();

  const webm = readdirSync(scratchDir).find((f) => f.endsWith(".webm"));
  if (!webm) throw new Error("no webm recorded in " + scratchDir);
  const src = join(scratchDir, webm);

  // Two-pass palette GIF: generate an optimized 128-colour palette, then apply
  // it. Encoded at the tight frame's native width so the panel text stays crisp
  // and legible when embedded ~1:1. 12fps keeps motion smooth without bloating.
  const gifW = viewport(GIF_PAD).width; // 464
  const palette = join(scratchDir, "palette.png");
  const vf = `fps=12,scale=${gifW}:-1:flags=lanczos`;
  execFileSync("ffmpeg", [
    "-y", "-i", src,
    "-vf", `${vf},palettegen=stats_mode=diff:max_colors=128`,
    palette,
  ]);
  execFileSync("ffmpeg", [
    "-y", "-i", src, "-i", palette,
    "-lavfi", `${vf} [x]; [x][1:v] paletteuse=dither=bayer:bayer_scale=4`,
    "-loop", "0", outPath,
  ]);

  // Optionally shrink further with gifsicle (lossy LZW). ~4× smaller with no
  // visible quality loss; skipped (with a note) if gifsicle isn't installed.
  const hasGifsicle = spawnSync("gifsicle", ["--version"], { stdio: "ignore" }).status === 0;
  if (hasGifsicle) {
    execFileSync("gifsicle", ["-O3", "--lossy=60", "-b", outPath]);
  } else {
    console.log("  (install gifsicle for a smaller GIF: brew install gifsicle)");
  }
  console.log(`  convert.gif: ${(statSync(outPath).size / 1024).toFixed(0)} KB`);
}
