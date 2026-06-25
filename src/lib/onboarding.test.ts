import { beforeAll, describe, expect, it } from "vitest";
import {
  compressLatestShortcut,
  reopenShortcut,
  updateCheckPrivacyHint,
} from "./onboarding";
import { setLocale, t } from "../i18n";

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

describe("compressLatestShortcut", () => {
  it("uses Ctrl on Windows", () => {
    expect(compressLatestShortcut(true)).toBe("Ctrl+Alt+T");
  });
  it("uses Cmd elsewhere", () => {
    expect(compressLatestShortcut(false)).toBe("Cmd+Alt+T");
  });
});

describe("update-check privacy copy", () => {
  it("makes the privacy scope explicit (GitHub, nothing about you)", () => {
    const hint = updateCheckPrivacyHint();
    expect(hint).toContain("GitHub");
    expect(hint.toLowerCase()).toContain("nothing about you");
  });
});

describe("the four-step welcome copy (en)", () => {
  it("welcomes to Tamp with the four-step lead", () => {
    expect(t("onboarding.title")).toBe("Welcome to Tamp 👋");
    expect(t("onboarding.lead")).toContain("four steps");
  });

  it("step 1 names the tray, the ^ overflow, and the reopen shortcut", () => {
    expect(t("onboarding.step1Title")).toBe("Open it");
    const detail = t("onboarding.step1Detail", {
      shortcut: reopenShortcut(true),
    });
    expect(detail).toContain("tray");
    expect(detail).toContain("^");
    expect(detail).toContain("Ctrl+Alt+O");
  });

  it("step 2 points at a preset and the built-in Discord preset", () => {
    expect(t("onboarding.step2Title")).toBe("Make a preset");
    const detail = t("onboarding.step2Detail");
    expect(detail).toContain("Discord");
    expect(detail).toContain("10 MB");
    // The four-step flow mentions only Discord — never Slack.
    expect(detail).not.toContain("Slack");
  });

  it("step 3 covers drop, right-click, and the compress-latest shortcut", () => {
    expect(t("onboarding.step3Title")).toBe("Convert a recording");
    const detail = t("onboarding.step3Detail", {
      shortcut: compressLatestShortcut(true),
    });
    expect(detail).toContain("Compress with Tamp");
    expect(detail).toContain("Ctrl+Alt+T");
  });

  it("step 4 explains the clipboard hand-off", () => {
    expect(t("onboarding.step4Title")).toBe("Use it");
    expect(t("onboarding.step4Detail")).toContain("clipboard");
  });
});

describe("onboarding copy is localized through t()", () => {
  it("returns Ukrainian copy when the locale is uk", () => {
    setLocale("uk");
    try {
      // Cyrillic content proves the keys resolve through the active dict.
      expect(t("onboarding.title")).toContain("Ласкаво просимо");
      expect(t("onboarding.step1Detail", { shortcut: "Cmd+Alt+O" })).toContain(
        "лотку",
      );
      expect(t("onboarding.step2Detail")).toContain("Discord");
      expect(updateCheckPrivacyHint()).toContain("GitHub");
    } finally {
      setLocale("en");
    }
  });
});
