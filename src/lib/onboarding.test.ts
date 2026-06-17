import { beforeAll, describe, expect, it } from "vitest";
import {
  notificationPrimingHint,
  reopenShortcut,
  trayLocationHint,
  updateCheckConsentLine,
  updateCheckPrivacyHint,
} from "./onboarding";
import { setLocale } from "../i18n";

// These pure copy helpers now route through t(); pin the locale to English so
// the assertions check the source-of-truth en.json strings deterministically.
beforeAll(() => setLocale("en"));

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

describe("update-check consent copy", () => {
  it("frames the check as something tamp can do on start", () => {
    const line = updateCheckConsentLine();
    expect(line).toContain("tamp");
    expect(line.toLowerCase()).toContain("version");
  });

  it("makes the privacy scope explicit (GitHub, nothing about you)", () => {
    const hint = updateCheckPrivacyHint();
    expect(hint).toContain("GitHub");
    expect(hint.toLowerCase()).toContain("nothing about you");
  });
});

describe("onboarding copy is localized through t()", () => {
  it("returns Ukrainian copy when the locale is uk", () => {
    setLocale("uk");
    try {
      // Cyrillic content proves the helpers resolve through the active dict.
      expect(trayLocationHint(false)).toContain("рядку меню");
      expect(notificationPrimingHint()).toContain("сповіщення");
      expect(updateCheckConsentLine()).toContain("tamp");
      expect(updateCheckPrivacyHint()).toContain("GitHub");
    } finally {
      setLocale("en");
    }
  });
});
