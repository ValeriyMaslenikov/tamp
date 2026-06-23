import { describe, expect, it } from "vitest";
import { isTerminal, mergeDropped, videoListSignature } from "./list";
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

describe("mergeDropped", () => {
  it("puts dropped videos first, then scanned", () => {
    const dropped = [video({ path: "/dl/d.mov", name: "d.mov" })];
    const scanned = [video({ path: "/desk/s.mov", name: "s.mov" })];
    expect(mergeDropped(dropped, scanned).map((v) => v.path)).toEqual([
      "/dl/d.mov",
      "/desk/s.mov",
    ]);
  });

  it("dedups by path, keeping the dropped copy at the top", () => {
    const shared = "/desk/clip.mov";
    const dropped = [video({ path: shared, sizeBytes: 1 })];
    const scanned = [
      video({ path: shared, sizeBytes: 2 }),
      video({ path: "/desk/other.mov" }),
    ];
    const merged = mergeDropped(dropped, scanned);
    expect(merged.map((v) => v.path)).toEqual([shared, "/desk/other.mov"]);
    // The dropped entry (its own metadata) wins, not the scanned duplicate.
    expect(merged[0].sizeBytes).toBe(1);
  });

  it("returns scanned unchanged when nothing is dropped", () => {
    const scanned = [video({ path: "/desk/s.mov" })];
    expect(mergeDropped([], scanned)).toEqual(scanned);
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
