// CONVERTED TAB journey: a journal with one split (multi-output) record + one
// single renders as one expandable group row + one single row; expanding the
// group reveals its parts; "Copy all" copies EVERY part path (the split-copy
// bug we fixed — not just the last part).
import { test, expect } from "./fixtures";
import { sampleConversions } from "./canned";

test.describe("converted tab", () => {
  test("renders a group + a single; expand shows parts; Copy all copies every part", async ({
    tamp,
    page,
  }) => {
    await tamp.setMock("list_conversions", sampleConversions());
    await tamp.goto();

    // Switch to the Converted tab and let it refresh.
    await page.locator("#tab-converted").click();
    const view = page.locator(".view-converted");
    await expect(view).toBeVisible();

    // The split record collapses into one expandable group; the single is a row.
    const group = view.locator(".conv-tree");
    const single = view.locator(".conv-row");
    await expect(group).toHaveCount(1);
    await expect(single).toHaveCount(1);
    await expect(group.locator(".conv-name")).toHaveText("long-recording.mov");
    await expect(single.locator(".conv-name")).toHaveText("short-clip.mov");

    // Parts start hidden (the children block is collapsed).
    const children = group.locator(".conv-children");
    await expect(children).toBeHidden();

    // Expand the group by clicking its parent row; the two parts appear.
    await group.locator(".conv-tree-parent").click();
    await expect(group).toHaveClass(/is-open/);
    await expect(children).toBeVisible();
    const parts = children.locator(".conv-part");
    await expect(parts).toHaveCount(2);
    await expect(parts.nth(0).locator(".conv-cname")).toHaveText(
      "long-recording (tamped 1of2).mp4",
    );
    await expect(parts.nth(1).locator(".conv-cname")).toHaveText(
      "long-recording (tamped 2of2).mp4",
    );

    // "Copy all" on the group must copy_files with ALL part paths — the bug we
    // fixed was that only the last part landed on the clipboard.
    await group.getByRole("button", { name: "Copy all" }).click();

    await expect.poll(async () => {
      const calls = await tamp.calls();
      return calls.some((c) => c.cmd === "copy_files");
    }).toBe(true);

    const calls = await tamp.calls();
    const copy = calls.find((c) => c.cmd === "copy_files");
    expect(copy?.args?.paths).toEqual([
      "/Users/e2e/Movies/long-recording (tamped 1of2).mp4",
      "/Users/e2e/Movies/long-recording (tamped 2of2).mp4",
    ]);
    // It must NOT fall back to the single-file copy command for a split.
    expect(calls.some((c) => c.cmd === "copy_file")).toBe(false);
  });
});
