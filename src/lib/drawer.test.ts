import { describe, expect, it } from "vitest";
import { planDrawer } from "./drawer";
import type { JobState, Phase } from "./ipc";

/** A minimal JobState in a given phase; id derives from the index. */
const job = (id: string, phase: Phase): JobState => ({
  id,
  inputPath: `C:\\v\\${id}.mp4`,
  inputName: `${id}.mp4`,
  outputPath: phase === "done" ? `C:\\v\\${id} (tamped).mp4` : null,
  presetId: "p",
  presetHash: "h",
  phase,
  progress: phase === "done" ? 1 : 0.5,
  inputBytes: 9000,
  outputBytes: phase === "done" ? 1000 : null,
  reused: false,
  part: null,
  error: phase === "failed" ? "boom" : null,
  postError: null,
});

const queuedN = (n: number): JobState[] =>
  Array.from({ length: n }, (_, i) => job(`q${i}`, "queued"));

describe("planDrawer", () => {
  it("titles by running/done counts over the FULL set, not the visible rows", () => {
    const plan = planDrawer([
      job("a", "pass1"),
      ...queuedN(10), // queued count toward "running" in the title
      job("d1", "done"),
      job("d2", "done"),
    ]);
    // 1 active + 10 queued = 11 running; 2 done.
    expect(plan.title).toBe("Compressing… 11 running, 2 done");
  });

  it("collapses 2+ queued jobs into a single summary instead of one row each", () => {
    const plan = planDrawer([job("a", "pass1"), ...queuedN(8)]);
    expect(plan.queuedSummary).toBe(8);
    // The active encode is a row; the 8 queued are folded, not listed.
    expect(plan.rows.map((j) => j.id)).toEqual(["a"]);
  });

  it("keeps a lone queued job as its own row (no summary)", () => {
    const plan = planDrawer([job("a", "pass1"), job("q0", "queued")]);
    expect(plan.queuedSummary).toBe(0);
    expect(plan.rows.map((j) => j.id)).toEqual(["a", "q0"]);
  });

  it("renders running and done/failed rows individually", () => {
    const plan = planDrawer([
      job("a", "pass2"),
      job("v", "verifying"),
      job("d", "done"),
      job("f", "failed"),
    ]);
    expect(plan.queuedSummary).toBe(0);
    expect(plan.overflow).toBe(0);
    expect(new Set(plan.rows.map((j) => j.id))).toEqual(
      new Set(["a", "v", "d", "f"]),
    );
  });

  it("orders active encodes before finished rows", () => {
    const plan = planDrawer([job("d", "done"), job("a", "pass1")]);
    expect(plan.rows.map((j) => j.id)).toEqual(["a", "d"]);
  });

  it("caps individual rows and folds the rest into '+N more'", () => {
    // 8 done rows, cap is 6 → 6 shown, 2 overflow.
    const dones = Array.from({ length: 8 }, (_, i) => job(`d${i}`, "done"));
    const plan = planDrawer(dones);
    expect(plan.rows).toHaveLength(6);
    expect(plan.overflow).toBe(2);
  });

  it("the queued summary does not count against the row cap or overflow", () => {
    // 5 done rows + a queued backlog: all 5 fit, queued folds to a summary.
    const dones = Array.from({ length: 5 }, (_, i) => job(`d${i}`, "done"));
    const plan = planDrawer([...dones, ...queuedN(20)]);
    expect(plan.rows).toHaveLength(5);
    expect(plan.queuedSummary).toBe(20);
    expect(plan.overflow).toBe(0);
  });

  it("is empty-safe", () => {
    const plan = planDrawer([]);
    expect(plan.rows).toEqual([]);
    expect(plan.queuedSummary).toBe(0);
    expect(plan.overflow).toBe(0);
    expect(plan.title).toBe("Conversions · 0 done");
  });
});
