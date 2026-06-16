import { describe, expect, it } from "vitest";
import { groupConversions, isPartPath } from "./convgroup";
import type { ConversionRecord } from "./ipc";

const rec = (outputPath: string, bytes = 1000, completedAtMs = 1): ConversionRecord => ({
  inputPath: "C:\\v\\Long meeting.mp4", inputBytes: 9000, outputPath, outputBytes: bytes,
  presetHash: "h", presetName: "Slack (25MB)", targetMb: 25, completedAtMs, inputCreatedMs: 0,
});

describe("isPartPath", () => {
  it("a bare part file inside a tamped folder is a part", () => {
    expect(isPartPath("C:\\v\\X (tamped Slack 25MB h)\\X 1.mp4")).toBe(true);
  });
  it("a single output (tamped name, normal folder) is not a part", () => {
    expect(isPartPath("C:\\v\\X (tamped Slack 25MB h).mp4")).toBe(false);
  });
  it("a re-compressed part (tamped name *inside* a tamped folder) is not a part", () => {
    expect(
      isPartPath("C:\\v\\X (tamped Slack 25MB h)\\X 1 (tamped Discord 10MB g).mp4"),
    ).toBe(false);
  });
});

describe("groupConversions", () => {
  it("keeps a single output as a flat node", () => {
    const out = groupConversions([rec("C:\\v\\Long meeting (tamped Slack 25MB h).mp4")]);
    expect(out).toHaveLength(1);
    expect(out[0].kind).toBe("single");
  });
  it("groups parts that share a (tamped …) folder", () => {
    const out = groupConversions([
      rec("C:\\v\\Long meeting (tamped Slack 25MB h)\\Long meeting 1.mp4", 100, 3),
      rec("C:\\v\\Long meeting (tamped Slack 25MB h)\\Long meeting 2.mp4", 200, 5),
    ]);
    expect(out).toHaveLength(1);
    expect(out[0].kind).toBe("group");
    if (out[0].kind === "group") {
      expect(out[0].parts).toHaveLength(2);
      expect(out[0].totalBytes).toBe(300);
      expect(out[0].completedAtMs).toBe(5); // newest part
    }
  });
  it("orders nodes newest-first by completion", () => {
    const out = groupConversions([
      rec("C:\\v\\a (tamped X)\\a 1.mp4", 1, 10),
      rec("C:\\v\\b (tamped X).mp4", 1, 20),
    ]);
    expect(out[0].completedAtMs).toBe(20);
  });
  it("re-compressing parts of a split makes separate records, not extra parts", () => {
    const out = groupConversions([
      // original 2-part split, in the (tamped …) folder
      rec("C:\\v\\Long meeting (tamped Slack 25MB h)\\Long meeting 1.mp4", 100, 3),
      rec("C:\\v\\Long meeting (tamped Slack 25MB h)\\Long meeting 2.mp4", 200, 5),
      // re-compress part 1 and part 2: their outputs are tamped FILES inside the
      // same folder — each is its own standalone conversion (a single row).
      rec(
        "C:\\v\\Long meeting (tamped Slack 25MB h)\\Long meeting 1 (tamped Discord 10MB g).mp4",
        40,
        7,
      ),
      rec(
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
