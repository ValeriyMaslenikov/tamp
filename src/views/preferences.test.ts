import { describe, expect, it } from "vitest";
import { isDuplicatePresetName } from "./preferences";

const preset = (id: string, name: string) => ({ id, name });

describe("isDuplicatePresetName", () => {
  const presets = [
    preset("preset-slack", "Slack"),
    preset("preset-email", "Email"),
  ];

  it("flags a name that differs only by case and surrounding space", () => {
    // Saving a NEW preset (editingId = null) named "  slack " collides with the
    // existing "Slack" — case-insensitive and trimmed.
    expect(isDuplicatePresetName("  slack ", presets, null)).toBe(true);
  });

  it("does not flag the preset being edited against its own name", () => {
    // Re-saving "Slack" while editing preset-slack is not a self-collision.
    expect(isDuplicatePresetName("Slack", presets, "preset-slack")).toBe(false);
    // Even with case/space differences, the edited preset itself is excluded.
    expect(isDuplicatePresetName("  SLACK ", presets, "preset-slack")).toBe(false);
  });

  it("allows a genuinely distinct name", () => {
    expect(isDuplicatePresetName("Discord", presets, null)).toBe(false);
  });

  it("returns false when there are no presets to collide with", () => {
    expect(isDuplicatePresetName("Slack", [], null)).toBe(false);
  });

  it("still flags a collision with a DIFFERENT preset while editing", () => {
    // Editing preset-slack but renaming it to "email" must collide with the
    // other preset, which is excluded only for the one being edited.
    expect(isDuplicatePresetName("email", presets, "preset-slack")).toBe(true);
  });
});
