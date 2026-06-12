import { describe, expect, it } from "vitest";
import { defaultSplitConfig, parseSplitInputs } from "./forms";

describe("defaultSplitConfig", () => {
  it("mirrors the backend defaults (off, by parts, 2 parts, 30s)", () => {
    expect(defaultSplitConfig()).toEqual({
      mode: "off",
      by: "parts",
      parts: 2,
      seconds: 30,
    });
  });
});

describe("parseSplitInputs", () => {
  it("accepts static by-parts within 2..20", () => {
    expect(parseSplitInputs("static", "parts", "2", "30")).toEqual({
      config: { mode: "static", by: "parts", parts: 2, seconds: 30 },
    });
    expect(parseSplitInputs("static", "parts", "20", "30")).toEqual({
      config: { mode: "static", by: "parts", parts: 20, seconds: 30 },
    });
  });

  it("rejects static by-parts outside 2..20 or non-integers", () => {
    for (const raw of ["1", "21", "0", "-3", "2.5", "", "abc"]) {
      expect(parseSplitInputs("static", "parts", raw, "30")).toHaveProperty(
        "error",
      );
    }
  });

  it("accepts static by-duration of at least 10 seconds", () => {
    expect(parseSplitInputs("static", "seconds", "2", "10")).toEqual({
      config: { mode: "static", by: "seconds", parts: 2, seconds: 10 },
    });
    expect(parseSplitInputs("static", "seconds", "2", "300")).toEqual({
      config: { mode: "static", by: "seconds", parts: 2, seconds: 300 },
    });
  });

  it("rejects static by-duration under 10 seconds or non-integers", () => {
    for (const raw of ["9", "0", "-1", "12.5", "", "abc"]) {
      expect(parseSplitInputs("static", "seconds", "2", raw)).toHaveProperty(
        "error",
      );
    }
  });

  it("only validates the active static sub-field", () => {
    // Invalid seconds is ignored while splitting by parts (and vice versa);
    // the inactive value falls back to its default.
    expect(parseSplitInputs("static", "parts", "3", "junk")).toEqual({
      config: { mode: "static", by: "parts", parts: 3, seconds: 30 },
    });
    expect(parseSplitInputs("static", "seconds", "junk", "45")).toEqual({
      config: { mode: "static", by: "seconds", parts: 2, seconds: 45 },
    });
  });

  it("never errors for off/smart modes and keeps valid sub-values", () => {
    expect(parseSplitInputs("off", "parts", "junk", "junk")).toEqual({
      config: { mode: "off", by: "parts", parts: 2, seconds: 30 },
    });
    expect(parseSplitInputs("smart", "seconds", "4", "60")).toEqual({
      config: { mode: "smart", by: "seconds", parts: 4, seconds: 60 },
    });
  });

  it("trims whitespace around numbers", () => {
    expect(parseSplitInputs("static", "parts", " 4 ", "30")).toEqual({
      config: { mode: "static", by: "parts", parts: 4, seconds: 30 },
    });
  });
});
