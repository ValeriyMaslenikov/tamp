import { describe, expect, it } from "vitest";
import { groupConversions } from "./convgroup";
import type { ConversionRecord } from "./ipc";

const rec = (outputPath: string, bytes = 1000, completedAtMs = 1): ConversionRecord => ({
  inputPath: "C:\\v\\Long meeting.mp4", inputBytes: 9000, outputPath, outputBytes: bytes,
  presetHash: "h", presetName: "Slack (25MB)", targetMb: 25, completedAtMs, inputCreatedMs: 0,
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
});
