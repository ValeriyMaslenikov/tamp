// ONBOARDING journey: a first run (onboardingSeen:false) shows the welcome
// notice over the panel; "Got it" persists onboardingSeen:true via save_settings
// and removes the notice.
import { test, expect } from "./fixtures";

test.describe("onboarding", () => {
  test("first run shows the welcome notice; 'Got it' persists onboardingSeen:true", async ({
    tamp,
    page,
  }) => {
    await tamp.seedSettings({ onboardingSeen: false });
    await tamp.goto();

    // The one-time notice mounts above the content: the guided 4-step
    // "Welcome to Tamp" flow (Open → Make a preset → Convert → Use it).
    const notice = page.locator(".onboarding-notice");
    await expect(notice).toBeVisible();
    await expect(notice).toContainText("Welcome to Tamp");
    await expect(notice).toContainText("Open it");
    await expect(notice).toContainText("Make a preset");

    // "Got it" dismisses and persists the seen flag.
    await notice.getByRole("button", { name: "Got it" }).click();
    await expect(notice).toHaveCount(0);

    await expect.poll(async () => {
      const calls = await tamp.calls();
      const save = calls.filter((c) => c.cmd === "save_settings").pop();
      return (save?.args?.settings as { onboardingSeen?: boolean } | undefined)
        ?.onboardingSeen;
    }).toBe(true);
  });

  test("a returning user (onboardingSeen:true) never sees the notice", async ({
    tamp,
    page,
  }) => {
    // The canned default is onboardingSeen:true.
    await tamp.goto();
    await expect(page.locator(".onboarding-notice")).toHaveCount(0);
  });
});
