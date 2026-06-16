import { describe, expect, it } from "vitest";
import {
  isTerminal,
  presetIndexForDigit,
  shouldPickPreset,
  videoListSignature,
} from "./list";
import type { RecentVideo } from "../lib/ipc";

const video = (overrides: Partial<RecentVideo> = {}): RecentVideo => ({
  path: "/tmp/clip.mov",
  name: "clip.mov",
  sizeBytes: 1000,
  createdMs: 1700000000000,
  thumbPath: null,
  isOutput: false,
  conversion: null,
  durationSecs: null,
  ...overrides,
});

describe("isTerminal", () => {
  it("treats done, failed and cancelled as terminal", () => {
    expect(isTerminal("done")).toBe(true);
    expect(isTerminal("failed")).toBe(true);
    expect(isTerminal("cancelled")).toBe(true);
  });

  it("treats queued and running phases as non-terminal", () => {
    expect(isTerminal("queued")).toBe(false);
    expect(isTerminal("pass1")).toBe(false);
    expect(isTerminal("pass2")).toBe(false);
    expect(isTerminal("verifying")).toBe(false);
  });
});

describe("presetIndexForDigit", () => {
  it("maps '1' to the first preset", () => {
    expect(presetIndexForDigit("1", 3)).toBe(0);
  });
  it("maps '3' to index 2 when in range", () => {
    expect(presetIndexForDigit("3", 3)).toBe(2);
  });
  it("returns null when the digit is past the preset count", () => {
    expect(presetIndexForDigit("4", 3)).toBeNull();
  });
  it("returns null for a non-digit", () => {
    expect(presetIndexForDigit("x", 3)).toBeNull();
  });
});

describe("shouldPickPreset", () => {
  it("opens the chooser in quick-pick mode", () => {
    expect(shouldPickPreset("quick-pick", false)).toBe(true);
  });
  it("uses the active preset in active-bar mode", () => {
    expect(shouldPickPreset("active-bar", false)).toBe(false);
  });
  it("opens the chooser on Alt-drop even in active-bar mode", () => {
    expect(shouldPickPreset("active-bar", true)).toBe(true);
  });
});

describe("videoListSignature", () => {
  it("is stable for equivalent lists", () => {
    expect(videoListSignature([video()])).toBe(videoListSignature([video()]));
  });

  it("changes when a video is added or removed", () => {
    const one = videoListSignature([video()]);
    const two = videoListSignature([
      video(),
      video({ path: "/tmp/other.mov" }),
    ]);
    expect(two).not.toBe(one);
    expect(videoListSignature([])).not.toBe(one);
  });

  it("changes when size, created time or thumb changes", () => {
    const base = videoListSignature([video()]);
    expect(videoListSignature([video({ sizeBytes: 999 })])).not.toBe(base);
    expect(videoListSignature([video({ createdMs: 1 })])).not.toBe(base);
    expect(videoListSignature([video({ thumbPath: "/tmp/t.jpg" })])).not.toBe(
      base,
    );
  });

  it("changes when a probed duration lands", () => {
    const base = videoListSignature([video()]);
    expect(videoListSignature([video({ durationSecs: 42.5 })])).not.toBe(base);
  });

  it("changes when output state or conversion meta changes", () => {
    const base = videoListSignature([video()]);
    const orphan = videoListSignature([video({ isOutput: true })]);
    expect(orphan).not.toBe(base);
    expect(
      videoListSignature([
        video({
          isOutput: true,
          conversion: {
            originalBytes: 5000,
            outputBytes: 1000,
            presetName: "Slack",
          },
        }),
      ]),
    ).not.toBe(orphan);
  });

  it("changes when a watched folder becomes unreachable", () => {
    const base = videoListSignature([video()]);
    const offline = videoListSignature([video()], ["\\\\nas\\recordings"]);
    expect(offline).not.toBe(base);
    // and is stable for the same unreachable set
    expect(offline).toBe(
      videoListSignature([video()], ["\\\\nas\\recordings"]),
    );
  });

  it("defaults to no unreachable folders (back-compat)", () => {
    expect(videoListSignature([video()])).toBe(
      videoListSignature([video()], []),
    );
  });

  it("is order-sensitive", () => {
    const a = video({ path: "/tmp/a.mov" });
    const b = video({ path: "/tmp/b.mov" });
    expect(videoListSignature([a, b])).not.toBe(videoListSignature([b, a]));
  });

  it("does not collide on tricky path contents", () => {
    const a = videoListSignature([video({ path: "/tmp/a 1000.mov" })]);
    const b = videoListSignature([
      video({ path: "/tmp/a", sizeBytes: 1000 }),
    ]);
    expect(a).not.toBe(b);
  });
});
