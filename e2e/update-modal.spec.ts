// UPDATE MODAL journey: with the opt-in check on and check_for_update returning
// a newer release, the modal appears on panel open; "Later" persists
// lastDismissedUpdateVersion via save_settings; and a fresh boot that already
// carries that dismissed version does NOT reshow the modal.
import { test, expect } from "./fixtures";

const NEWER = { version: "9.9.9", url: "https://example.com/r", notes: "stuff" };

test.describe("update modal", () => {
  test("a newer version shows the modal on open; 'Later' persists the dismissed version", async ({
    tamp,
    page,
  }) => {
    // The check is gated on updateCheckEnabled; nothing dismissed yet.
    await tamp.seedSettings({
      updateCheckEnabled: true,
      lastDismissedUpdateVersion: null,
    });
    await tamp.setMock("check_for_update", NEWER);
    await tamp.goto();

    // The modal mounts on the panel after the (fire-and-forget) check resolves.
    const modal = page.getByRole("dialog");
    await expect(modal).toBeVisible();
    await expect(modal).toContainText("Tamp 9.9.9 is available");

    // "Later" dismisses and persists lastDismissedUpdateVersion = the version.
    await modal.getByRole("button", { name: "Later" }).click();
    await expect(modal).toHaveCount(0);

    await expect.poll(async () => {
      const calls = await tamp.calls();
      const save = calls.filter((c) => c.cmd === "save_settings").pop();
      return (
        save?.args?.settings as { lastDismissedUpdateVersion?: string } | undefined
      )?.lastDismissedUpdateVersion ?? null;
    }).toBe("9.9.9");
  });

  test("the modal does not reappear when the same version is already dismissed", async ({
    tamp,
    page,
  }) => {
    // Simulate the next launch: the same release comes back, but it's already
    // the dismissed version, so shouldShowUpdate() suppresses the card.
    await tamp.seedSettings({
      updateCheckEnabled: true,
      lastDismissedUpdateVersion: "9.9.9",
    });
    await tamp.setMock("check_for_update", NEWER);
    await tamp.goto();

    // Give the (gated, fire-and-forget) check a beat to run, then confirm no card.
    await expect.poll(async () => {
      const calls = await tamp.calls();
      return calls.some((c) => c.cmd === "check_for_update");
    }).toBe(true);
    await expect(page.getByRole("dialog")).toHaveCount(0);
  });
});
