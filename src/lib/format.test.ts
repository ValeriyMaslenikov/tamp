import { describe, expect, it } from "vitest";
import {
  formatBytes,
  formatClock,
  formatPercentSmaller,
  formatRelativeTime,
  truncateMiddle,
} from "./format";

describe("formatBytes", () => {
  it("formats 0 bytes", () => {
    expect(formatBytes(0)).toBe("0 B");
  });

  it("treats negative and non-finite as 0", () => {
    expect(formatBytes(-5)).toBe("0 B");
    expect(formatBytes(NaN)).toBe("0 B");
    expect(formatBytes(Infinity)).toBe("0 B");
  });

  it("formats plain bytes", () => {
    expect(formatBytes(512)).toBe("512 B");
  });

  it("formats exactly 1 MB (decimal)", () => {
    expect(formatBytes(1_000_000)).toBe("1 MB");
  });

  it("keeps one decimal under 100", () => {
    expect(formatBytes(1_200_000)).toBe("1.2 MB");
    expect(formatBytes(9_400_000)).toBe("9.4 MB");
  });

  it("drops decimals at >= 100", () => {
    expect(formatBytes(229_000_000)).toBe("229 MB");
  });

  it("formats > 1 GB", () => {
    expect(formatBytes(1_234_000_000)).toBe("1.2 GB");
  });

  it("bumps unit when rounding hits 1000", () => {
    expect(formatBytes(999_999)).toBe("1 MB");
  });

  it("formats TB", () => {
    expect(formatBytes(2_500_000_000_000)).toBe("2.5 TB");
  });
});

describe("formatRelativeTime", () => {
  const now = new Date(2026, 5, 11, 18, 0).getTime(); // Jun 11 2026, 18:00 local

  it("returns Just now under a minute", () => {
    expect(formatRelativeTime(now - 30_000, now)).toBe("Just now");
  });

  it("returns minutes ago under an hour", () => {
    expect(formatRelativeTime(now - 2 * 60_000, now)).toBe("2m ago");
    expect(formatRelativeTime(now - 59 * 60_000, now)).toBe("59m ago");
  });

  it("returns hours ago for same calendar day", () => {
    const ms = new Date(2026, 5, 11, 14, 3).getTime();
    expect(formatRelativeTime(ms, now)).toBe("3h ago");
  });

  it("returns Yesterday with clock time", () => {
    const ms = new Date(2026, 5, 10, 14, 3).getTime();
    expect(formatRelativeTime(ms, now)).toBe("Yesterday 14:03");
  });

  it("returns month/day with clock time within the year", () => {
    const ms = new Date(2026, 4, 2, 9, 30).getTime();
    expect(formatRelativeTime(ms, now)).toBe("May 2 09:30");
  });

  it("returns month/day with year for older timestamps", () => {
    const ms = new Date(2025, 11, 31, 9, 30).getTime();
    expect(formatRelativeTime(ms, now)).toBe("Dec 31, 2025");
  });
});

describe("formatPercentSmaller", () => {
  it("formats one decimal", () => {
    expect(formatPercentSmaller(1000, 61)).toBe("93.9% smaller");
  });

  it("rounds to one decimal", () => {
    // 1 - 9.4/229 = 95.8951...% -> 95.9%
    expect(formatPercentSmaller(229_000_000, 9_400_000)).toBe("95.9% smaller");
  });

  it("strips trailing .0", () => {
    expect(formatPercentSmaller(1000, 40)).toBe("96% smaller");
    expect(formatPercentSmaller(1000, 500)).toBe("50% smaller");
  });

  it("clamps when output is not smaller", () => {
    expect(formatPercentSmaller(100, 150)).toBe("0% smaller");
  });

  it("handles zero input bytes", () => {
    expect(formatPercentSmaller(0, 100)).toBe("0% smaller");
  });
});

describe("formatClock", () => {
  it("formats afternoon time", () => {
    expect(formatClock(new Date(2026, 5, 11, 14, 3).getTime())).toBe("14:03");
  });

  it("pads hours and minutes", () => {
    expect(formatClock(new Date(2026, 0, 1, 9, 5).getTime())).toBe("09:05");
  });

  it("formats midnight", () => {
    expect(formatClock(new Date(2026, 0, 1, 0, 0).getTime())).toBe("00:00");
  });
});

describe("truncateMiddle", () => {
  it("leaves short strings alone", () => {
    expect(truncateMiddle("clip.mp4", 36)).toBe("clip.mp4");
  });

  it("truncates in the middle keeping the extension", () => {
    const name = "Screen Recording 2026-06-11 at 15.53.01 extra long.mov";
    const out = truncateMiddle(name, 36);
    expect(out.length).toBe(36);
    expect(out).toContain("…");
    expect(out.endsWith("long.mov")).toBe(true);
    expect(out.startsWith("Screen Recording")).toBe(true);
  });
});
