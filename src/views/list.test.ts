import { describe, expect, it } from "vitest";
import { isTerminal, videoListSignature } from "./list";
import type { RecentVideo } from "../lib/ipc";

const video = (overrides: Partial<RecentVideo> = {}): RecentVideo => ({
  path: "/tmp/clip.mov",
  name: "clip.mov",
  sizeBytes: 1000,
  createdMs: 1700000000000,
  thumbPath: null,
  isOutput: false,
  conversion: null,
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
