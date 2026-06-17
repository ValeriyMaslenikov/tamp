import { describe, expect, it } from "vitest";
import {
  notificationPrimingHint,
  reopenShortcut,
  trayLocationHint,
} from "./onboarding";

describe("reopenShortcut", () => {
  it("uses Ctrl on Windows", () => {
    expect(reopenShortcut(true)).toBe("Ctrl+Alt+O");
  });
  it("uses Cmd elsewhere", () => {
    expect(reopenShortcut(false)).toBe("Cmd+Alt+O");
  });
});

describe("trayLocationHint", () => {
  it("warns about the ^ overflow on Windows", () => {
    const hint = trayLocationHint(true);
    expect(hint).toContain("system tray");
    expect(hint).toContain("^");
  });

  it("points at the menu bar on macOS", () => {
    const hint = trayLocationHint(false);
    expect(hint).toContain("menu bar");
    expect(hint).not.toContain("^");
  });
});

describe("notificationPrimingHint", () => {
  it("explains that tamp asks to send notifications", () => {
    const hint = notificationPrimingHint();
    expect(hint).toContain("notifications");
  });

  it("says the choice is reversible in Preferences", () => {
    expect(notificationPrimingHint()).toContain("Preferences");
  });
});
