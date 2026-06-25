import { beforeAll, describe, expect, it } from "vitest";
import {
  formatAbsolute,
  formatBytes,
  formatClock,
  formatDuration,
  formatPercentSmaller,
  formatRelativeTime,
  pluralParts,
  splitSummaryLabel,
  truncateMiddle,
} from "./format";
import { setLocale } from "../i18n";
import type { SplitConfig } from "./ipc";

// Intl output is locale-dependent and vitest runs under the host locale, so
// every locale-sensitive assertion pins an explicit locale ("en-US" for the
// app's reference convention, a comma-decimal locale for number separators).
// We never assert a bare default-locale Intl string.
//
// The textual LABELS (pluralParts, splitSummaryLabel, formatPercentSmaller now
// route through the i18n t() runtime, so pin the active dictionary to "en" —
// these assertions check the English source strings.
beforeAll(() => setLocale("en"));

const split = (overrides: Partial<SplitConfig> = {}): SplitConfig => ({
  mode: "off",
  by: "parts",
  parts: 2,
  seconds: 30,
  ...overrides,
});

describe("splitSummaryLabel", () => {
  it("is null when splitting is off or missing", () => {
    expect(splitSummaryLabel(split())).toBeNull();
    expect(splitSummaryLabel(undefined)).toBeNull();
  });

  it("labels smart mode", () => {
    expect(splitSummaryLabel(split({ mode: "smart" }))).toBe("smart split");
  });

  it("labels static by-parts with the count", () => {
    expect(splitSummaryLabel(split({ mode: "static", parts: 4 }))).toBe(
      "4 parts",
    );
  });

  it("uses the singular for a one-part split", () => {
    expect(splitSummaryLabel(split({ mode: "static", parts: 1 }))).toBe(
      "1 part",
    );
  });

  it("labels static by-duration with the seconds", () => {
    expect(
      splitSummaryLabel(split({ mode: "static", by: "seconds", seconds: 30 })),
    ).toBe("split ≈30s");
  });

  it("rolls a by-duration interval up into minutes above 60s", () => {
    expect(
      splitSummaryLabel(split({ mode: "static", by: "seconds", seconds: 600 })),
    ).toBe("split ≈10m");
  });

  it("rolls a by-duration interval up into hours above 3600s", () => {
    expect(
      splitSummaryLabel(split({ mode: "static", by: "seconds", seconds: 3600 })),
    ).toBe("split ≈1h");
  });
});

describe("pluralParts", () => {
  it("uses the singular for one", () => {
    expect(pluralParts(1)).toBe("1 part");
  });

  it("uses the plural otherwise", () => {
    expect(pluralParts(0)).toBe("0 parts");
    expect(pluralParts(2)).toBe("2 parts");
    expect(pluralParts(12)).toBe("12 parts");
  });
});

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
    expect(formatBytes(512, "en-US")).toBe("512 B");
  });

  it("formats exactly 1 MB (decimal)", () => {
    expect(formatBytes(1_000_000, "en-US")).toBe("1 MB");
  });

  it("keeps one decimal under 100", () => {
    expect(formatBytes(1_200_000, "en-US")).toBe("1.2 MB");
    expect(formatBytes(9_400_000, "en-US")).toBe("9.4 MB");
  });

  it("drops decimals at >= 100", () => {
    expect(formatBytes(229_000_000, "en-US")).toBe("229 MB");
  });

  it("formats > 1 GB", () => {
    expect(formatBytes(1_234_000_000, "en-US")).toBe("1.2 GB");
  });

  it("bumps unit when rounding hits 1000", () => {
    expect(formatBytes(999_999, "en-US")).toBe("1 MB");
  });

  it("formats TB", () => {
    expect(formatBytes(2_500_000_000_000, "en-US")).toBe("2.5 TB");
  });

  it("uses the locale's decimal separator (comma-decimal locale)", () => {
    // de-DE uses a comma for the decimal separator.
    expect(formatBytes(1_200_000, "de-DE")).toBe("1,2 MB");
  });
});

// Relative time is now fully relative for ANY age via dayjs (no flip to an
// absolute date). dayjs output is locale-dependent, so every assertion pins an
// explicit dayjs locale: "en" (dayjs's default) for the reference strings, plus
// one "uk" case to prove Ukrainian relative time loads.
describe("formatRelativeTime", () => {
  const now = new Date(2026, 5, 11, 18, 0).getTime(); // Jun 11 2026, 18:00 local
  const DAY = 86_400_000;

  it("says 'a few seconds ago' under a minute", () => {
    expect(formatRelativeTime(now - 30_000, now, "en")).toBe(
      "a few seconds ago",
    );
  });

  it("counts minutes within the hour", () => {
    expect(formatRelativeTime(now - 5 * 60_000, now, "en")).toBe(
      "5 minutes ago",
    );
  });

  it("counts hours within the day", () => {
    expect(formatRelativeTime(now - 3 * 3_600_000, now, "en")).toBe(
      "3 hours ago",
    );
  });

  it("stays relative for days — no flip to an absolute date", () => {
    expect(formatRelativeTime(now - 5 * DAY, now, "en")).toBe("5 days ago");
  });

  it("stays relative for months and years", () => {
    expect(formatRelativeTime(now - 35 * DAY, now, "en")).toBe("a month ago");
    expect(formatRelativeTime(now - 2 * 365 * DAY, now, "en")).toBe(
      "2 years ago",
    );
  });

  it("localizes to Ukrainian when the dayjs locale is uk", () => {
    // Cyrillic relative string: "5 хвилин тому".
    const out = formatRelativeTime(now - 5 * 60_000, now, "uk");
    expect(out).toBe("5 хвилин тому");
    expect(out).toMatch(/[Ѐ-ӿ]/); // contains Cyrillic
  });
});

describe("formatPercentSmaller", () => {
  it("formats one decimal", () => {
    expect(formatPercentSmaller(1000, 61, "en-US")).toBe("93.9% smaller");
  });

  it("rounds to one decimal", () => {
    // 1 - 9.4/229 = 95.8951...% -> 95.9%
    expect(formatPercentSmaller(229_000_000, 9_400_000, "en-US")).toBe(
      "95.9% smaller",
    );
  });

  it("strips trailing .0", () => {
    expect(formatPercentSmaller(1000, 40, "en-US")).toBe("96% smaller");
    expect(formatPercentSmaller(1000, 500, "en-US")).toBe("50% smaller");
  });

  it("clamps when output is not smaller", () => {
    expect(formatPercentSmaller(100, 150, "en-US")).toBe("0% smaller");
  });

  it("handles zero input bytes", () => {
    expect(formatPercentSmaller(0, 100, "en-US")).toBe("0% smaller");
  });

  it("uses the locale's decimal separator (comma-decimal locale)", () => {
    expect(formatPercentSmaller(1000, 61, "de-DE")).toBe("93,9% smaller");
  });
});

describe("formatDuration", () => {
  it("formats zero", () => {
    expect(formatDuration(0)).toBe("0:00");
  });

  it("treats negative and non-finite as zero", () => {
    expect(formatDuration(-5)).toBe("0:00");
    expect(formatDuration(NaN)).toBe("0:00");
    expect(formatDuration(Infinity)).toBe("0:00");
  });

  it("formats sub-minute durations", () => {
    expect(formatDuration(42)).toBe("0:42");
  });

  it("pads seconds under ten", () => {
    expect(formatDuration(65)).toBe("1:05");
    expect(formatDuration(725)).toBe("12:05");
  });

  it("rounds fractional seconds", () => {
    expect(formatDuration(41.6)).toBe("0:42");
    expect(formatDuration(59.7)).toBe("1:00");
  });

  it("formats hour-long durations with padded minutes", () => {
    expect(formatDuration(3600)).toBe("1:00:00");
    expect(formatDuration(3903)).toBe("1:05:03");
    expect(formatDuration(10 * 3600 + 2)).toBe("10:00:02");
  });
});

describe("formatClock", () => {
  it("formats afternoon time in a 24h locale", () => {
    expect(formatClock(new Date(2026, 5, 11, 14, 3).getTime(), "en-GB")).toBe(
      "14:03",
    );
  });

  it("pads hours and minutes in a 24h locale", () => {
    expect(formatClock(new Date(2026, 0, 1, 9, 5).getTime(), "en-GB")).toBe(
      "09:05",
    );
  });

  it("formats midnight in a 24h locale", () => {
    expect(formatClock(new Date(2026, 0, 1, 0, 0).getTime(), "en-GB")).toBe(
      "00:00",
    );
  });

  it("follows the locale's 12/24h convention", () => {
    // en-US is a 12h clock with AM/PM markers; en-GB is 24h.
    const ms = new Date(2026, 5, 11, 14, 3).getTime();
    expect(formatClock(ms, "en-US")).toContain("PM");
    expect(formatClock(ms, "en-GB")).not.toContain("PM");
  });
});

describe("formatAbsolute", () => {
  it("formatAbsolute shows a date and time", () => {
    const s = formatAbsolute(1_700_000_000_000, "en-US");
    expect(s).toMatch(/\d{4}/); // a year
    expect(s).toContain("·");
  });

  it("formatAbsolute returns em dash for unknown (0)", () => {
    expect(formatAbsolute(0)).toBe("—");
  });
});

describe("truncateMiddle", () => {
  it("leaves short strings alone", () => {
    expect(truncateMiddle("clip.mp4", 36)).toBe("clip.mp4");
  });

  it("truncates in the middle keeping the extension", () => {
    const name = "Screen Recording 2026-06-11 at 15.53.01 extra long.mov";
    const out = truncateMiddle(name, 36);
    expect([...out].length).toBe(36);
    expect(out).toContain("…");
    expect(out.endsWith("long.mov")).toBe(true);
    expect(out.startsWith("Screen Recording")).toBe(true);
  });

  it("counts code points, leaving an emoji-heavy short name intact", () => {
    // 10 astral emoji = 20 UTF-16 units but only 10 code points, so a max of 36
    // code points must leave it untouched (the old s.length check truncated it).
    const name = "🎬".repeat(10) + ".mov";
    expect(truncateMiddle(name, 36)).toBe(name);
  });

  it("never splits an astral character into a lone surrogate", () => {
    // A long run of emoji forces a cut; the result must contain no unpaired
    // surrogate — a high surrogate (0xD800–0xDBFF) always followed by a low one
    // (0xDC00–0xDFFF) — which is exactly what UTF-16 code-unit slicing produced.
    const name = "🎬".repeat(40) + ".mov";
    const out = truncateMiddle(name, 20);
    const hasLoneSurrogate = (s: string): boolean => {
      for (let i = 0; i < s.length; i++) {
        const c = s.charCodeAt(i);
        if (c >= 0xd800 && c <= 0xdbff) {
          const next = s.charCodeAt(i + 1);
          if (!(next >= 0xdc00 && next <= 0xdfff)) return true; // lone high
          i++; // skip the matched low surrogate
        } else if (c >= 0xdc00 && c <= 0xdfff) {
          return true; // lone low surrogate
        }
      }
      return false;
    };
    expect(hasLoneSurrogate(out)).toBe(false);
    // And the visible length is the requested code-point budget.
    expect([...out].length).toBe(20);
    expect(out).toContain("…");
  });
});
