import { describe, expect, it } from "vitest";
import { stripOutputSuffix } from "./naming";

describe("stripOutputSuffix", () => {
  it("strips the hashed suffix", () => {
    expect(stripOutputSuffix("clip (tamped a3f2).mp4")).toBe("clip");
  });

  it("strips the hashed numbered suffix", () => {
    expect(stripOutputSuffix("clip (tamped a3f2 2).mp4")).toBe("clip");
    expect(stripOutputSuffix("clip (tamped 0f9c 12).mp4")).toBe("clip");
  });

  it("strips the legacy suffix", () => {
    expect(stripOutputSuffix("clip (tamped).mp4")).toBe("clip");
  });

  it("strips the legacy numbered suffix", () => {
    expect(stripOutputSuffix("clip (tamped 2).mp4")).toBe("clip");
  });

  it("treats a 4-digit counter as a hash", () => {
    // Indistinguishable from a hash by pattern alone; both forms are outputs.
    expect(stripOutputSuffix("clip (tamped 1234).mp4")).toBe("clip");
  });

  it("only drops the extension on non-output names", () => {
    expect(stripOutputSuffix("clip.mov")).toBe("clip");
    expect(stripOutputSuffix("Screen Recording at 15.53.01.mov")).toBe(
      "Screen Recording at 15.53.01",
    );
  });

  it("requires the suffix at the end of the stem", () => {
    expect(stripOutputSuffix("x (tamped) y.mp4")).toBe("x (tamped) y");
  });

  it("does not match uppercase hex (hashes are lowercase)", () => {
    expect(stripOutputSuffix("clip (tamped A3F2).mp4")).toBe(
      "clip (tamped A3F2)",
    );
  });

  it("does not match a malformed hash length", () => {
    expect(stripOutputSuffix("clip (tamped a3f).mp4")).toBe("clip (tamped a3f)");
    expect(stripOutputSuffix("clip (tamped a3f2c).mp4")).toBe(
      "clip (tamped a3f2c)",
    );
  });

  it("handles names without an extension", () => {
    expect(stripOutputSuffix("clip (tamped a3f2)")).toBe("clip");
  });

  it("keeps earlier dots in the stem", () => {
    expect(stripOutputSuffix("my.clip (tamped a3f2).mp4")).toBe("my.clip");
  });
});
