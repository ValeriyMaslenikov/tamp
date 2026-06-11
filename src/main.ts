import "./styles.css";
import {
  getSettings,
  onEncodeState,
  onPanelShown,
  onSettingsChanged,
  type Settings,
} from "./lib/ipc";
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
      <div class="toast" id="toast" hidden></div>
    </div>`;

  initToast(document.getElementById("toast") as HTMLElement);

  let settings: Settings | null = null;

  const listView = createListView(() => settings);
  const prefsView = createPreferencesView({
    onSettings: (s) => {
      settings = s;
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
    if (tab === "videos") void listView.refresh();
  }

  for (const b of segButtons) {
    b.addEventListener("click", () => setTab(b.dataset.tab as Tab));
  }
  setTab("videos");

  void onPanelShown(() => {
    void listView.refresh();
  });
  void onEncodeState((state) => {
    listView.updateJob(state);
  });
  void onSettingsChanged((s) => {
    settings = s;
    prefsView.render(s);
    listView.onSettingsChanged();
  });

  void (async () => {
    try {
      settings = await getSettings();
      prefsView.render(settings);
      listView.onSettingsChanged();
    } catch (e) {
      showToast(String(e));
    }
    await listView.refresh();
  })();
}

window.addEventListener("DOMContentLoaded", main);

// Dev-only test hook: drop an autotest.json in public/ ({"path", "presetId"})
// to trigger an encode through the real IPC path without UI interaction.
if (import.meta.env.DEV) {
  window.addEventListener("DOMContentLoaded", async () => {
    try {
      const res = await fetch("/autotest.json");
      if (!res.ok) return;
      const { path, presetId } = await res.json();
      const { enqueue } = await import("./lib/ipc");
      console.log("[autotest] enqueue", path, presetId);
      await enqueue(path, presetId);
    } catch {
      /* no autotest file — normal run */
    }
  });
}
