import { describe, expect, it } from "vitest";
import { nextNavIndex } from "./converted";

describe("nextNavIndex", () => {
  it("moves down within range", () => { expect(nextNavIndex(0, 1, 3)).toBe(1); });
  it("clamps at the bottom", () => { expect(nextNavIndex(2, 1, 3)).toBe(2); });
  it("clamps at the top", () => { expect(nextNavIndex(0, -1, 3)).toBe(0); });
  it("selects the first row from no selection moving down", () => { expect(nextNavIndex(-1, 1, 3)).toBe(0); });
  it("selects the last row from no selection moving up", () => { expect(nextNavIndex(-1, -1, 3)).toBe(2); });
  it("returns -1 when there are no rows", () => { expect(nextNavIndex(0, 1, 0)).toBe(-1); });
});
