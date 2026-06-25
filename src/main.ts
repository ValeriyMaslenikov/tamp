import "./fonts.css";
import "./styles.css";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  checkForUpdate,
  getSettings,
  onEncodeState,
  onPanelShown,
  onSettingsChanged,
  requestNotificationPermission,
  saveSettings,
  setPin,
  type Settings,
} from "./lib/ipc";
import { createDrawer } from "./lib/drawer";
import { initDragDrop } from "./lib/dragdrop";
import { buildOnboardingNotice } from "./lib/onboarding";
import { openUpdateModal, shouldShowUpdate } from "./lib/updatemodal";
import { initPlatform } from "./lib/platform";
import { applyTheme } from "./lib/theme";
import { initToast, showToast } from "./lib/toast";
import { friendlyError } from "./lib/errors";
import { createConvertedView } from "./views/converted";
import { createListView } from "./views/list";
import { createPreferencesView } from "./views/preferences";
import { resolveLocale, setLocale, t } from "./i18n";

type Tab = "videos" | "converted" | "prefs";

async function main(): Promise<void> {
  const app = document.getElementById("app");
  if (!app) return;

  // Resolve the platform and the persisted settings BEFORE building any view,
  // so the very first render is in the right locale (and per-OS labels don't
  // flash). `setLocale` swaps the active dictionary the views read through
  // t(); a failure here leaves the "en" default + null settings, and the
  // views render from defaults until the retry below succeeds.
  let settings: Settings | null = null;
  await initPlatform();
  try {
    settings = await getSettings();
    setLocale(resolveLocale(settings.locale, navigator.language));
  } catch {
    // Leave settings null and the locale at its "en" default; the IIFE below
    // re-fetches and surfaces the error via a toast once the panel is built.
  }

  app.innerHTML = `
    <div class="panel">
      <header class="panel-header">
        <div class="seg" role="tablist">
          <button type="button" class="seg-btn is-active" data-tab="videos" role="tab" id="tab-videos" aria-controls="panel-videos" aria-selected="true" tabindex="0">${t("main.tabVideos")}</button>
          <button type="button" class="seg-btn" data-tab="converted" role="tab" id="tab-converted" aria-controls="panel-converted" aria-selected="false" tabindex="-1">${t("main.tabConverted")}</button>
          <button type="button" class="seg-btn" data-tab="prefs" role="tab" id="tab-prefs" aria-controls="panel-prefs" aria-selected="false" tabindex="-1">${t("main.tabPrefs")}</button>
        </div>
        <button type="button" class="pin-btn" id="pin-btn" aria-pressed="false" title="${t("main.pinTitle")}">📌</button>
        <button type="button" class="close-btn" id="close-btn" title="${t("main.closeTitle")}" aria-label="${t("main.closeTitle")}">✕</button>
      </header>
      <main class="content" id="content"></main>
      <footer class="panel-footer">${t("main.footerVideos")}</footer>
      <div class="toast" id="toast" role="status" aria-live="polite"></div>
    </div>`;

  initToast(document.getElementById("toast") as HTMLElement);

  const pinBtn = document.getElementById("pin-btn") as HTMLButtonElement;
  let pinned = false;
  pinBtn.addEventListener("click", () => {
    pinned = !pinned;
    pinBtn.classList.toggle("is-on", pinned);
    pinBtn.setAttribute("aria-pressed", String(pinned));
    void setPin(pinned);
  });

  // An explicit dismiss: hides the panel (same as Esc / clicking the tray icon).
  // Reliable on macOS, where click-outside-to-hide is unreliable (the panel's
  // blur can land mid-click). The app stays in the tray — quit from its menu.
  const closeBtn = document.getElementById("close-btn") as HTMLButtonElement;
  closeBtn.addEventListener("click", () => {
    void getCurrentWindow().hide();
  });

  const listView = createListView(() => settings);
  const convertedView = createConvertedView();
  const prefsView = createPreferencesView({
    onSettings: (s) => {
      settings = s;
      applyTheme(s.theme);
      listView.onSettingsChanged();
    },
  });

  const content = document.getElementById("content") as HTMLElement;
  content.append(listView.el, convertedView.el, prefsView.el);

  // Tie each tabpanel to its controlling tab so AT announces the relationship.
  listView.el.id = "panel-videos";
  listView.el.setAttribute("role", "tabpanel");
  listView.el.setAttribute("aria-labelledby", "tab-videos");
  convertedView.el.id = "panel-converted";
  convertedView.el.setAttribute("role", "tabpanel");
  convertedView.el.setAttribute("aria-labelledby", "tab-converted");
  prefsView.el.id = "panel-prefs";
  prefsView.el.setAttribute("role", "tabpanel");
  prefsView.el.setAttribute("aria-labelledby", "tab-prefs");

  const panel = app.querySelector(".panel") as HTMLElement;
  const drawer = createDrawer(panel);

  const footer = app.querySelector(".panel-footer") as HTMLElement;

  // The one-time first-run notice mounts here (above the content), so it shows
  // over whichever tab is active until dismissed. Tracked so we never stack two.
  let onboardingEl: HTMLElement | null = null;

  // Prime the notification permission with context the first time the panel
  // opens, rather than letting the OS prompt appear silently on the first
  // shortcut fire. Best-effort: a granted/denied/unsupported result needs no
  // action here — Preferences surfaces a denied state with a recovery path.
  let primedNotifications = false;
  function primeNotifications(): void {
    if (primedNotifications) return;
    primedNotifications = true;
    // No blocking dialog: we simply trigger the request in context, right after
    // the welcome notice mounts, so on a platform that surfaces an OS prompt it
    // appears while the user is reading the first-run flow rather than silently
    // on the first shortcut fire. Failures are swallowed — notifications are an
    // optional convenience.
    void requestNotificationPermission().catch(() => {});
  }

  function maybeShowOnboarding(s: Settings): void {
    if (s.onboardingSeen || onboardingEl) return;
    const notice = buildOnboardingNotice((updateCheckEnabled) => {
      // Dismiss: drop the notice immediately, then persist the seen flag plus the
      // update-check consent the user left set. Spread the LATEST settings (not
      // the show-time snapshot) so a preference changed meanwhile isn't reverted.
      // A persist failure only means the notice may show once more.
      onboardingEl?.remove();
      onboardingEl = null;
      const base = settings ?? s;
      const next: Settings = {
        ...base,
        onboardingSeen: true,
        updateCheckEnabled,
      };
      settings = next;
      void saveSettings(next).catch((e) => showToast(friendlyError(e), "error"));
    });
    onboardingEl = notice.el;
    panel.insertBefore(notice.el, content);
    // First run is also the moment to prime notifications, in context: the
    // welcome notice is on screen, so any OS permission prompt that follows
    // arrives while the user is actively onboarding rather than out of the blue.
    primeNotifications();
  }

  // manual: on a fresh profile (no settings.json), the one-time four-step
  // welcome notice shows once and never again after "Got it". On a
  // platform that can deny notifications, denying then opening Preferences shows
  // the recoverable "Notifications are off" row with an Enable button; on
  // desktop the plugin reports "granted", so that row stays hidden by design.

  // The update modal mounts here too (on the always-visible panel host, never a
  // per-tab root). Tracked so a stale one is replaced rather than stacked.
  let updateModalEl: HTMLElement | null = null;

  // Run the opt-in update check at most once per launch. Gated on the setting
  // and fire-and-forget: a failed/slow check never blocks the panel and never
  // toasts (it's a passive convenience). If a newer-than-dismissed release comes
  // back, one gentle, dismissible card shows over the panel; dismissing persists
  // lastDismissedUpdateVersion so it never re-nags for that version.
  let updateCheckRan = false;
  function maybeRunUpdateCheck(): void {
    if (updateCheckRan) return;
    const s = settings;
    if (!s || !s.updateCheckEnabled) return;
    updateCheckRan = true;
    void checkForUpdate()
      .then((info) => {
        // Re-read the live settings: the user may have dismissed this version in
        // a prior session, or toggled the feature off while the request was in
        // flight. shouldShowUpdate also narrows `info` to non-null.
        const cur = settings;
        if (!cur || !cur.updateCheckEnabled) return;
        if (!shouldShowUpdate(info, cur.lastDismissedUpdateVersion)) return;
        if (updateModalEl) return; // never stack two
        const modal = openUpdateModal({
          host: panel,
          info,
          onDismiss: (version) => {
            updateModalEl = null;
            // Persist the dismissed version off the LATEST settings so a
            // meanwhile-changed preference isn't reverted. A persist failure
            // only means the card may show once more next launch.
            const base = settings ?? cur;
            const next: Settings = {
              ...base,
              lastDismissedUpdateVersion: version,
            };
            settings = next;
            void saveSettings(next).catch(() => {});
          },
        });
        updateModalEl = modal.el;
      })
      .catch(() => {
        // Swallow silently: a failed update check must never surface a toast.
      });
  }

  const segButtons = Array.from(
    app.querySelectorAll<HTMLButtonElement>(".seg-btn"),
  );

  let activeTab: Tab = "videos";

  function setTab(tab: Tab, focusContent = true): void {
    activeTab = tab;
    // A drop can float the preset picker over any tab; close a stale one on
    // every tab change so it can't linger or resurface on the wrong tab.
    listView.closeQuickPick();
    listView.el.hidden = tab !== "videos";
    convertedView.el.hidden = tab !== "converted";
    prefsView.el.hidden = tab !== "prefs";
    for (const b of segButtons) {
      const selected = b.dataset.tab === tab;
      b.classList.toggle("is-active", selected);
      b.setAttribute("aria-selected", String(selected));
      // Roving tabindex: only the active tab is in the Tab order; arrow keys
      // move between the rest.
      b.tabIndex = selected ? 0 : -1;
    }
    if (tab === "videos") {
      footer.textContent = listView.footerHint();
      void listView.refresh();
      listView.focusFilter();
    } else if (tab === "converted") {
      footer.textContent = t("main.footerConverted");
      void convertedView.refresh();
    } else {
      footer.textContent = t("main.footerPrefs");
    }
    // Drop focus from the tab strip into the content so the freed ← → arrows
    // drive expand/collapse in the list instead of the ARIA tablist. Skipped
    // for keyboard tab-nav (focusContent=false), which keeps focus on the chip
    // for continued arrowing; Videos already hands focus to its filter above.
    if (focusContent && tab !== "videos") {
      const a = document.activeElement;
      if (a instanceof HTMLElement && a.classList.contains("seg-btn")) a.blur();
    }
  }

  // Move to the tab at `index` (wrapping): focus it AND activate it, per the
  // ARIA tabs "automatic activation" pattern.
  function activateTabAt(index: number): void {
    const count = segButtons.length;
    const b = segButtons[((index % count) + count) % count];
    setTab(b.dataset.tab as Tab, false); // keep focus on the chip for continued arrowing
    b.focus();
  }

  // Ctrl+Tab / Ctrl+Shift+Tab cycle the top tabs from anywhere. (Cmd+Tab is the
  // OS app switcher on macOS, so this stays on Ctrl.) setTab moves focus into the
  // content, so the now-freed ← → arrows act on the list, not the tab strip.
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Tab" || !e.ctrlKey) return;
    e.preventDefault();
    const order: Tab[] = ["videos", "converted", "prefs"];
    const dir = e.shiftKey ? -1 : 1;
    const next = (order.indexOf(activeTab) + dir + order.length) % order.length;
    setTab(order[next]);
  });

  for (const b of segButtons) {
    b.addEventListener("click", () => setTab(b.dataset.tab as Tab));
    // The keydown handler lives on the buttons (not document) so it can't fight
    // the views' own document-level key handlers.
    b.addEventListener("keydown", (e) => {
      const i = segButtons.indexOf(b);
      // Stop the keys we consume from bubbling to the views' document-level
      // keydown handlers (list.ts / converted.ts). activateTabAt() runs
      // setTab() synchronously, so without this the same event keeps bubbling
      // and, once the target tab is visible, would trigger that view's own
      // arrow handling (e.g. active-bar cycleActive, group expand).
      switch (e.key) {
        case "ArrowRight":
          e.preventDefault();
          e.stopPropagation();
          activateTabAt(i + 1);
          break;
        case "ArrowLeft":
          e.preventDefault();
          e.stopPropagation();
          activateTabAt(i - 1);
          break;
        case "Home":
          e.preventDefault();
          e.stopPropagation();
          activateTabAt(0);
          break;
        case "End":
          e.preventDefault();
          e.stopPropagation();
          activateTabAt(segButtons.length - 1);
          break;
      }
    });
  }

  // manual: with a screen reader, the active tab announces "selected"; Left/Right
  // (and Home/End) move between tabs, both moving focus and switching panels.

  initDragDrop({
    compressPaths: (paths, altHeld) => listView.compressPaths(paths, altHeld),
    currentDropHint: () => listView.currentDropHint(),
  });

  void onPanelShown(() => {
    void listView.refresh();
    listView.focusFilter();
    // First panel:shown after settings load is the moment to run the once-per-
    // launch update check (no-op until settings exist; gated on the setting).
    maybeRunUpdateCheck();
  });
  void onEncodeState((state) => {
    listView.updateJob(state);
    drawer.updateJob(state);
    // A finished conversion is a new history entry; refresh if that tab is open.
    if (state.phase === "done" && !convertedView.el.hidden) {
      void convertedView.refresh();
    }
  });
  void onSettingsChanged((s) => {
    settings = s;
    applyTheme(s.theme);
    prefsView.render(s);
    listView.onSettingsChanged();
    if (!listView.el.hidden) footer.textContent = listView.footerHint();
  });

  void (async () => {
    // Platform + the locale-deciding settings were resolved before the views
    // were built (so the first render is in the right locale). Reuse that
    // fetch when it succeeded; only re-fetch if the early load failed.
    setTab("videos");
    try {
      if (!settings) settings = await getSettings();
      applyTheme(settings.theme);
      prefsView.render(settings);
      listView.onSettingsChanged();
      // setTab ran before settings loaded (defaulting the hint to quick-pick);
      // refresh it now that the real videos-layout is known.
      if (!listView.el.hidden) footer.textContent = listView.footerHint();
      // First-run notice (the guided four-step welcome flow) + notification
      // permission priming, once the real seen-flag is known.
      maybeShowOnboarding(settings);
      // The panel is already shown at launch, so a panel:shown event may have
      // fired (and no-op'd) before settings loaded. Run the gated check now that
      // settings are known; the once-guard makes a later panel:shown harmless.
      maybeRunUpdateCheck();
    } catch (e) {
      showToast(String(e), "error");
    }
    await listView.refresh();
    listView.focusFilter();
  })();
}

window.addEventListener("DOMContentLoaded", () => void main());

// Dev-only test hook: drop an autotest.json in public/ to trigger an encode
// through the real IPC path without UI interaction. Either
// {"path", "presetId"} (enqueue) or {"path", "custom": CustomConfig}
// (custom_convert, e.g. for headless split testing).
if (import.meta.env.DEV) {
  window.addEventListener("DOMContentLoaded", async () => {
    try {
      const res = await fetch("/autotest.json");
      if (!res.ok) return;
      const { path, presetId, custom } = await res.json();
      const { customConvert, enqueue } = await import("./lib/ipc");
      if (custom) {
        console.log("[autotest] customConvert", path, custom);
        await customConvert(path, custom);
      } else {
        console.log("[autotest] enqueue", path, presetId);
        await enqueue(path, presetId);
      }
    } catch {
      /* no autotest file — normal run */
    }
  });
}
