import "./styles.css";
import {
  getSettings,
  onEncodeState,
  onPanelShown,
  onSettingsChanged,
  setPin,
  type Settings,
} from "./lib/ipc";
import { createDrawer } from "./lib/drawer";
import { initDragDrop } from "./lib/dragdrop";
import { initPlatform } from "./lib/platform";
import { applyTheme } from "./lib/theme";
import { initToast, showToast } from "./lib/toast";
import { createConvertedView } from "./views/converted";
import { createListView } from "./views/list";
import { createPreferencesView } from "./views/preferences";

type Tab = "videos" | "converted" | "prefs";

function main(): void {
  const app = document.getElementById("app");
  if (!app) return;

  app.innerHTML = `
    <div class="panel">
      <header class="panel-header">
        <div class="seg" role="tablist">
          <button type="button" class="seg-btn is-active" data-tab="videos" role="tab" id="tab-videos" aria-controls="panel-videos" aria-selected="true" tabindex="0">Videos</button>
          <button type="button" class="seg-btn" data-tab="converted" role="tab" id="tab-converted" aria-controls="panel-converted" aria-selected="false" tabindex="-1">Converted</button>
          <button type="button" class="seg-btn" data-tab="prefs" role="tab" id="tab-prefs" aria-controls="panel-prefs" aria-selected="false" tabindex="-1">Preferences</button>
        </div>
        <button type="button" class="pin-btn" id="pin-btn" aria-pressed="false" title="Keep panel open">📌</button>
      </header>
      <main class="content" id="content"></main>
      <footer class="panel-footer">↑↓ select · ⏎/d default · e expand · esc back</footer>
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

  let settings: Settings | null = null;

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

  const drawer = createDrawer(app.querySelector(".panel") as HTMLElement);

  const footer = app.querySelector(".panel-footer") as HTMLElement;

  const segButtons = Array.from(
    app.querySelectorAll<HTMLButtonElement>(".seg-btn"),
  );

  function setTab(tab: Tab): void {
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
      footer.textContent = "↑↓ select · ⏎ play · →/e expand · c copy · r reveal · esc back";
      void convertedView.refresh();
    } else {
      footer.textContent = "esc back";
    }
  }

  // Move to the tab at `index` (wrapping): focus it AND activate it, per the
  // ARIA tabs "automatic activation" pattern.
  function activateTabAt(index: number): void {
    const count = segButtons.length;
    const b = segButtons[((index % count) + count) % count];
    setTab(b.dataset.tab as Tab);
    b.focus();
  }

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
    // Platform resolves before the first render so per-OS labels (reveal
    // button) never flash the wrong OS's wording.
    await initPlatform();
    setTab("videos");
    try {
      settings = await getSettings();
      applyTheme(settings.theme);
      prefsView.render(settings);
      listView.onSettingsChanged();
      // setTab ran before settings loaded (defaulting the hint to quick-pick);
      // refresh it now that the real videos-layout is known.
      if (!listView.el.hidden) footer.textContent = listView.footerHint();
    } catch (e) {
      showToast(String(e), "error");
    }
    await listView.refresh();
    listView.focusFilter();
  })();
}

window.addEventListener("DOMContentLoaded", main);

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
