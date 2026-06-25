import { describe, expect, it } from "vitest";
import type { UpdateInfo } from "./ipc";
import { shouldShowUpdate } from "./updatemodal";

const info = (version: string): UpdateInfo => ({
  version,
  url: `https://github.com/ValeriyMaslenikov/tamp/releases/tag/v${version}`,
  notes: null,
});

describe("shouldShowUpdate", () => {
  it("does not show when there is no update", () => {
    expect(shouldShowUpdate(null, null)).toBe(false);
    expect(shouldShowUpdate(null, "0.3.0")).toBe(false);
  });

  it("shows a newer release that has never been dismissed", () => {
    expect(shouldShowUpdate(info("0.3.0"), null)).toBe(true);
  });

  it("does not reshow the exact version already dismissed", () => {
    expect(shouldShowUpdate(info("0.3.0"), "0.3.0")).toBe(false);
  });

  it("shows again only for a different (strictly newer, per backend) version", () => {
    // The backend only ever returns versions strictly newer than installed, so
    // any version that isn't the dismissed one is a new release to surface.
    expect(shouldShowUpdate(info("0.4.0"), "0.3.0")).toBe(true);
    expect(shouldShowUpdate(info("0.3.0-beta.8"), "0.3.0-beta.7")).toBe(true);
  });
});
