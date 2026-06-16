import { describe, expect, it } from "vitest";
import { groupConversions } from "./convgroup";
import type { ConversionRecord } from "./ipc";

/** A single-output record (one delivered file). */
const single = (
  outputPath: string,
  bytes = 1000,
  completedAtMs = 1,
): ConversionRecord => ({
  inputPath: "C:\\v\\Long meeting.mp4", inputBytes: 9000,
  outputs: [{ path: outputPath, bytes }],
  presetHash: "h", presetName: "Slack (25MB)", targetMb: 25, completedAtMs, inputCreatedMs: 0,
});

/** A split record: one record carrying N part outputs. */
const split = (
  outputs: { path: string; bytes: number }[],
  completedAtMs = 1,
): ConversionRecord => ({
  inputPath: "C:\\v\\Long meeting.mp4", inputBytes: 9000, outputs,
  presetHash: "h", presetName: "Slack (25MB)", targetMb: 25, completedAtMs, inputCreatedMs: 0,
});

describe("groupConversions", () => {
  it("keeps a single-output record as a flat node", () => {
    const out = groupConversions([single("C:\\v\\Long meeting (tamped Slack 25MB h).mp4")]);
    expect(out).toHaveLength(1);
    expect(out[0].kind).toBe("single");
    if (out[0].kind === "single") {
      expect(out[0].output.path).toBe("C:\\v\\Long meeting (tamped Slack 25MB h).mp4");
    }
  });

  it("turns a multi-output split record into one group with N parts", () => {
    const out = groupConversions([
      split(
        [
          { path: "C:\\v\\Long meeting (tamped Slack 25MB h)\\Long meeting 1.mp4", bytes: 100 },
          { path: "C:\\v\\Long meeting (tamped Slack 25MB h)\\Long meeting 2.mp4", bytes: 200 },
        ],
        5,
      ),
    ]);
    expect(out).toHaveLength(1);
    expect(out[0].kind).toBe("group");
    if (out[0].kind === "group") {
      expect(out[0].parts).toHaveLength(2);
      expect(out[0].parts[0].path).toBe(
        "C:\\v\\Long meeting (tamped Slack 25MB h)\\Long meeting 1.mp4",
      );
      expect(out[0].totalBytes).toBe(300);
      expect(out[0].completedAtMs).toBe(5);
      // The group's folder is the parent dir of the first part.
      expect(out[0].folder).toBe("C:\\v\\Long meeting (tamped Slack 25MB h)");
    }
  });

  it("orders nodes newest-first by completion", () => {
    const out = groupConversions([
      split([{ path: "C:\\v\\a (tamped X)\\a 1.mp4", bytes: 1 }, { path: "C:\\v\\a (tamped X)\\a 2.mp4", bytes: 1 }], 10),
      single("C:\\v\\b (tamped X).mp4", 1, 20),
    ]);
    expect(out[0].completedAtMs).toBe(20);
  });

  it("re-compressing parts of a split are separate single-output records", () => {
    const out = groupConversions([
      // original 2-part split: ONE record with two outputs.
      split(
        [
          { path: "C:\\v\\Long meeting (tamped Slack 25MB h)\\Long meeting 1.mp4", bytes: 100 },
          { path: "C:\\v\\Long meeting (tamped Slack 25MB h)\\Long meeting 2.mp4", bytes: 200 },
        ],
        5,
      ),
      // re-compress part 1 and part 2: each is its OWN single-output record.
      single(
        "C:\\v\\Long meeting (tamped Slack 25MB h)\\Long meeting 1 (tamped Discord 10MB g).mp4",
        40,
        7,
      ),
      single(
        "C:\\v\\Long meeting (tamped Slack 25MB h)\\Long meeting 2 (tamped Discord 10MB g).mp4",
        50,
        9,
      ),
    ]);
    const groups = out.filter((n) => n.kind === "group");
    const singles = out.filter((n) => n.kind === "single");
    expect(groups).toHaveLength(1); // the original 2-part group, unchanged
    expect(groups[0].kind === "group" && groups[0].parts).toHaveLength(2);
    expect(singles).toHaveLength(2); // the two re-compressions as standalone rows
  });
});
