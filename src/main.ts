import "./styles.css";
import {
  getSettings,
  onEncodeState,
  onPanelShown,
  onSettingsChanged,
  type Settings,
} from "./lib/ipc";
import { initPlatform } from "./lib/platform";
import { applyTheme } from "./lib/theme";
import { initToast, showToast } from "./lib/toast";
import { createListView } from "./views/list";
import { createPreferencesView } from "./views/preferences";

type Tab = "videos" | "prefs";

function main(): void {
  const app = document.getElementById("app");
  if (!app) return;

  app.innerHTML = `
    <div class="panel">
      <header class="panel-header">
        <div class="seg" role="tablist">
          <button type="button" class="seg-btn is-active" data-tab="videos" role="tab">Videos</button>
          <button type="button" class="seg-btn" data-tab="prefs" role="tab">Preferences</button>
        </div>
      </header>
      <main class="content" id="content"></main>
      <footer class="panel-footer">↑↓ select · ⏎/d default · e expand · esc back</footer>
      <div class="toast" id="toast" hidden></div>
    </div>`;

  initToast(document.getElementById("toast") as HTMLElement);

  let settings: Settings | null = null;

  const listView = createListView(() => settings);
  const prefsView = createPreferencesView({
    onSettings: (s) => {
      settings = s;
      applyTheme(s.theme);
      listView.onSettingsChanged();
    },
  });

  const content = document.getElementById("content") as HTMLElement;
  content.append(listView.el, prefsView.el);

  const segButtons = Array.from(
    app.querySelectorAll<HTMLButtonElement>(".seg-btn"),
  );

  function setTab(tab: Tab): void {
    listView.el.hidden = tab !== "videos";
    prefsView.el.hidden = tab !== "prefs";
    for (const b of segButtons) {
      b.classList.toggle("is-active", b.dataset.tab === tab);
    }
    if (tab === "videos") {
      void listView.refresh();
      listView.focusFilter();
    }
  }

  for (const b of segButtons) {
    b.addEventListener("click", () => setTab(b.dataset.tab as Tab));
  }

  void onPanelShown(() => {
    void listView.refresh();
    listView.focusFilter();
  });
  void onEncodeState((state) => {
    listView.updateJob(state);
  });
  void onSettingsChanged((s) => {
    settings = s;
    applyTheme(s.theme);
    prefsView.render(s);
    listView.onSettingsChanged();
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
    } catch (e) {
      showToast(String(e));
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
