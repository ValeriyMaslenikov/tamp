// Records the one-click conversion story and encodes it to MP4 + WebM + poster.
// The story (active-bar layout, so a single row click converts): panel open →
// click a recording → progress ramps 0→100% via encode:state → "N% smaller"
// result holds. Recorded at 2× (videoScale) so the text is retina-crisp.
import type { Browser, Page } from "@playwright/test";
import { rmSync } from "node:fs";
import { bootPanel } from "./harness";
import { demoRecents } from "./demo-data";
import { GIF_PAD } from "./frame";
import { encodeClip, findWebm } from "./encode";
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

/** Record the one-click demo and write `${outBase}.mp4|.webm|.poster.png`. */
export async function shootClip(browser: Browser, outBase: string, scratchDir: string): Promise<void> {
  rmSync(scratchDir, { recursive: true, force: true });
  const { context, page } = await bootPanel(browser, {
    layout: "active-bar",
    recordVideoDir: scratchDir,
    pad: GIF_PAD,
    videoScale: 2,
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

  // Beat 4: done — the "N% smaller" payoff. Hold a beat.
  await emit(page, job("done", 1));
  await page.waitForTimeout(1700);

  // Close the context to flush the webm, then transcode to animated WebP + MP4.
  await context.close();
  // ss 0.15 trims the one blank frame before the panel paints (white loop flash).
  encodeClip(findWebm(scratchDir), outBase, { ss: 0.15, webpWidth: 640, webpFps: 13, webpQ: 30 });
}
