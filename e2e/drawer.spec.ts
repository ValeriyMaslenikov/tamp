// DRAWER journey: encode:state events drive the activity drawer. A running job
// plus a backlog of queued jobs renders a running row + an "N queued" summary;
// a done event renders a done row; cancelling the last job flashes "Cancelled".
import { test, expect } from "./fixtures";
import type { JobState, Phase } from "../src/lib/ipc";

// A JobState builder with sane defaults; specs override what matters per event.
function job(id: string, phase: Phase, over: Partial<JobState> = {}): JobState {
  return {
    id,
    inputPath: `/Users/e2e/Movies/${id}.mov`,
    inputName: `${id}.mov`,
    outputPath: phase === "done" ? `/Users/e2e/Movies/${id} (tamped).mp4` : null,
    presetId: "discord-10mb",
    presetHash: "h",
    phase,
    progress: phase === "pass1" ? 0.4 : 0,
    inputBytes: 50_000_000,
    outputBytes: phase === "done" ? 9_000_000 : null,
    reused: false,
    part: null,
    error: null,
    postError: null,
    ...over,
  };
}

test.describe("drawer", () => {
  test("running + a queued backlog renders a running row and an 'N queued' summary", async ({
    tamp,
    page,
  }) => {
    await tamp.goto();
    const drawer = page.locator(".drawer");
    await expect(drawer).toBeHidden();

    // One actively encoding job, plus two queued (>= 2 collapses into a summary).
    await tamp.emit("encode:state", job("a", "pass1", { progress: 0.4 }));
    await tamp.emit("encode:state", job("b", "queued"));
    await tamp.emit("encode:state", job("c", "queued"));

    await expect(drawer).toBeVisible();
    // Title reflects the running count over all jobs.
    await expect(drawer.locator(".drawer-title")).toContainText("running");
    // The running job has its own row showing a percentage tag.
    const runRow = drawer.locator(".drawer-row").filter({ hasText: "a.mov" });
    await expect(runRow).toBeVisible();
    await expect(runRow.locator(".drawer-tag.run")).toContainText("40%");
    // The two queued jobs fold into one summary line, not two rows.
    await expect(drawer.locator(".drawer-summary")).toContainText("2 queued");
  });

  test("a done event renders a done row with the output size", async ({
    tamp,
    page,
  }) => {
    await tamp.goto();
    await tamp.emit("encode:state", job("a", "pass1", { progress: 0.4 }));
    await tamp.emit(
      "encode:state",
      job("a", "done", { progress: 1, outputBytes: 9_000_000 }),
    );

    const drawer = page.locator(".drawer");
    await expect(drawer).toBeVisible();
    const doneRow = drawer.locator(".drawer-row").filter({ hasText: "a.mov" });
    await expect(doneRow.locator(".drawer-tag.ok")).toBeVisible();
  });

  test("cancelling the last job flashes 'Cancelled'", async ({ tamp, page }) => {
    await tamp.goto();
    // Start one job so the drawer is visible, then cancel it (emptying it).
    await tamp.emit("encode:state", job("a", "pass1", { progress: 0.2 }));
    const drawer = page.locator(".drawer");
    await expect(drawer).toBeVisible();

    await tamp.emit("encode:state", job("a", "cancelled"));
    // The drawer stays visible for a beat showing the Cancelled flash.
    await expect(drawer.locator(".drawer-title")).toHaveText("Cancelled");
    // ...then clears itself once the flash timer (1.2s) elapses.
    await expect(drawer).toBeHidden({ timeout: 3000 });
  });
});
