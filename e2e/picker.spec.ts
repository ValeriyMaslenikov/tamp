// PRESET PICKER journey: in quick-pick layout (the canned default), clicking a
// recent-video row opens the preset chooser; picking a preset calls enqueue
// with that video's path + the chosen preset id.
import { test, expect } from "./fixtures";
import { sampleRecents } from "./canned";

test.describe("preset picker", () => {
  test("opening the quick-pick shows presets; choosing one calls enqueue", async ({
    tamp,
    page,
  }) => {
    await tamp.setMock("list_recents", sampleRecents());
    await tamp.goto();

    // Click the first video row to open the quick-pick chooser.
    const firstRow = page.locator(".view-videos .row").first();
    await expect(firstRow).toBeVisible();
    await firstRow.locator(".row-main").click();

    // The picker is a labelled modal dialog (aria-label "Choose a preset")
    // titled "Compress with…", with one item per preset.
    const dialog = page.getByRole("dialog", { name: "Choose a preset" });
    await expect(dialog).toBeVisible();
    await expect(dialog.locator(".quickpick-title")).toHaveText("Compress with…");
    const items = dialog.locator(".quickpick-item:not(.quickpick-custom)");
    // The canned settings define two presets (Discord / Slack).
    await expect(items).toHaveCount(2);
    await expect(dialog.locator(".quickpick-name").first()).toHaveText("Discord (10MB)");

    // Choosing "Slack (25MB)" enqueues that path with the slack preset id.
    await dialog.getByText("Slack (25MB)").click();

    await expect.poll(async () => {
      const calls = await tamp.calls();
      return calls.filter((c) => c.cmd === "enqueue").length;
    }).toBe(1);

    const calls = await tamp.calls();
    const enq = calls.find((c) => c.cmd === "enqueue");
    expect(enq?.args).toMatchObject({
      path: "/Users/e2e/Movies/screen-recording-1.mov",
      presetId: "slack-25mb",
    });

    // The picker closes after a choice.
    await expect(dialog).toHaveCount(0);
  });
});
