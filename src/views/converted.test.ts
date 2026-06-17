import { describe, expect, it } from "vitest";
import { nextNavIndex, restoreSelection } from "./converted";

describe("nextNavIndex", () => {
  it("moves down within range", () => { expect(nextNavIndex(0, 1, 3)).toBe(1); });
  it("clamps at the bottom", () => { expect(nextNavIndex(2, 1, 3)).toBe(2); });
  it("clamps at the top", () => { expect(nextNavIndex(0, -1, 3)).toBe(0); });
  it("selects the first row from no selection moving down", () => { expect(nextNavIndex(-1, 1, 3)).toBe(0); });
  it("selects the last row from no selection moving up", () => { expect(nextNavIndex(-1, -1, 3)).toBe(2); });
  it("returns -1 when there are no rows", () => { expect(nextNavIndex(0, 1, 0)).toBe(-1); });
});

describe("restoreSelection", () => {
  it("preserves the selected row when its key survives the rebuild", () => {
    expect(
      restoreSelection({ hadSelection: true, selectedKey: "single:/out/a.mp4" }, true),
    ).toEqual({ kind: "preserve", key: "single:/out/a.mp4" });
  });

  it("selects the first row on a true first load (no prior selection)", () => {
    expect(restoreSelection({ hadSelection: false, selectedKey: null }, false)).toEqual({
      kind: "first",
    });
  });

  it("selects nothing when the previously-selected row vanished", () => {
    // A background refresh that drops the selected row must NOT yank the cursor
    // to the top — it leaves the selection cleared instead.
    expect(
      restoreSelection({ hadSelection: true, selectedKey: "single:/out/gone.mp4" }, false),
    ).toEqual({ kind: "none" });
  });

  it("does not preserve a stale key even if one was recorded but is absent", () => {
    expect(
      restoreSelection({ hadSelection: true, selectedKey: "group:/out/dir" }, false),
    ).toEqual({ kind: "none" });
  });
});
