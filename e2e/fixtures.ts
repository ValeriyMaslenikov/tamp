// Playwright fixture that boots the REAL frontend against the mocked Tauri IPC.
//
// Usage in a spec:
//   import { test, expect } from "./fixtures";
//   test("...", async ({ tamp, page }) => {
//     await tamp.seedSettings({ locale: "uk" });   // BEFORE goto
//     await tamp.setMock("list_recents", [...]);    // BEFORE goto
//     await tamp.goto();                             // installs mock, then boots app
//     await tamp.emit("encode:state", {...});        // AFTER goto
//   });
//
// The mock is installed via page.addInitScript so it runs before main.ts. Any
// override a test sets BEFORE goto() is pre-seeded onto window.__E2E__ (also via
// addInitScript) so installTauriMock() picks it up; setMock/emit AFTER goto()
// reach into the live window.__E2E__.
import { test as base, expect } from "@playwright/test";
import type { Page } from "@playwright/test";
import { installTauriMock, type MockResponse } from "./mock-ipc";
import { defaultSettings } from "./canned";
import type { Settings } from "../src/lib/ipc";

export interface TampHarness {
  /**
   * Pre-seed a per-command override (a value or a function of the invoke args).
   * Must be called BEFORE goto(). Functions are serialized into the page.
   */
  setMock(cmd: string, response: MockResponse): Promise<void>;
  /** Override the canned settings (shallow-merged onto the default). BEFORE goto. */
  seedSettings(partial: Partial<Settings>): Promise<void>;
  /** Install the mock + navigate + wait for the panel to render. */
  goto(): Promise<void>;
  /** Emit a Tauri event (encode:state, panel:shown, settings:changed) post-boot. */
  emit(event: string, payload: unknown): Promise<void>;
  /** The command names the app invoked, in order. */
  calls(): Promise<{ cmd: string; args: Record<string, unknown> }[]>;
  /** Set a command override AFTER boot (e.g. before clicking something). */
  setMockLive(cmd: string, response: MockResponse): Promise<void>;
}

// Overrides accumulated before goto(): { cmd -> serialized response } and a
// settings patch. addInitScript-ed in goto() so installTauriMock sees them.
interface PendingOverrides {
  // Functions can't cross the addInitScript boundary as live closures, so a
  // function override is captured as its source string and rebuilt in-page.
  mocks: { cmd: string; value: unknown; isFn: boolean }[];
  settingsPatch: Record<string, unknown> | null;
}

function serialize(response: MockResponse): { value: unknown; isFn: boolean } {
  return typeof response === "function"
    ? { value: response.toString(), isFn: true }
    : { value: response, isFn: false };
}

export const test = base.extend<{ tamp: TampHarness }>({
  tamp: async ({ page }, use) => {
    const pending: PendingOverrides = { mocks: [], settingsPatch: null };
    let booted = false;

    const harness: TampHarness = {
      async setMock(cmd, response) {
        if (booted) return harness.setMockLive(cmd, response);
        pending.mocks.push({ cmd, ...serialize(response) });
      },

      async seedSettings(partial) {
        pending.settingsPatch = {
          ...(pending.settingsPatch || {}),
          ...partial,
        };
      },

      async goto() {
        const base = defaultSettings();
        const settings = (
          pending.settingsPatch ? { ...base, ...pending.settingsPatch } : base
        ) as unknown as Record<string, unknown>;

        // 1) Pre-seed window.__E2E__ with the per-test overrides BEFORE the mock
        //    init runs. Function overrides are rebuilt from their source here.
        await page.addInitScript(
          (data: {
            mocks: { cmd: string; value: unknown; isFn: boolean }[];
            settings: Record<string, unknown>;
          }) => {
            const mocks: Record<string, unknown> = {};
            for (const m of data.mocks) {
              // eslint-disable-next-line @typescript-eslint/no-implied-eval
              mocks[m.cmd] = m.isFn ? new Function(`return (${m.value})`)() : m.value;
            }
            (window as unknown as { __E2E__: unknown }).__E2E__ = {
              mocks,
              settings: data.settings,
            };
          },
          { mocks: pending.mocks, settings },
        );

        // 2) Install the Tauri global BEFORE main.ts boots. installTauriMock is
        //    serialized to source by addInitScript, so it must be self-contained.
        await page.addInitScript(installTauriMock, settings);

        await page.goto("/");
        // The app builds .panel synchronously after get_settings resolves.
        await page.waitForSelector(".panel .seg", { state: "attached" });
        booted = true;
      },

      async emit(event, payload) {
        await page.evaluate(
          ([ev, pl]) => {
            (window as unknown as { __E2E__: { emit: (e: string, p: unknown) => void } })
              .__E2E__.emit(ev as string, pl);
          },
          [event, payload] as const,
        );
      },

      async setMockLive(cmd, response) {
        const { value, isFn } = serialize(response);
        await page.evaluate(
          ([c, v, fn]) => {
            const bridge = (window as unknown as { __E2E__: { mocks: Record<string, unknown> } }).__E2E__;
            // eslint-disable-next-line @typescript-eslint/no-implied-eval
            bridge.mocks[c as string] = fn ? new Function(`return (${v})`)() : v;
          },
          [cmd, value, isFn] as const,
        );
      },

      async calls() {
        return page.evaluate(
          () =>
            (window as unknown as { __E2E__: { calls: { cmd: string; args: Record<string, unknown> }[] } })
              .__E2E__.calls,
        );
      },
    };

    await use(harness);
  },
});

export { expect };
export type { Page };
