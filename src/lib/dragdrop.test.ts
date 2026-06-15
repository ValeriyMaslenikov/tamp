import { describe, expect, it } from "vitest";
import { filterVideos } from "./dragdrop";

describe("filterVideos", () => {
  it("keeps only known video extensions, case-insensitively", () => {
    const got = filterVideos([
      "C:\\a\\clip.MP4", "C:\\a\\note.txt", "C:\\a\\rec.mkv", "C:\\a\\img.png",
    ]);
    expect(got).toEqual(["C:\\a\\clip.MP4", "C:\\a\\rec.mkv"]);
  });
  it("returns empty when nothing is a video", () => {
    expect(filterVideos(["a.txt", "b.zip"])).toEqual([]);
  });
});
