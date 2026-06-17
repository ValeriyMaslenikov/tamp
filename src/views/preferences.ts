import { getVersion } from "@tauri-apps/api/app";
import {
  getSettings,
  notificationPermission,
  openNotificationSettings,
  pickFolder,
  requestNotificationPermission,
  saveSettings,
  setContextMenu,
  type LocaleSetting,
  type NotificationPermission,
  type OpenAfterConvert,
  type OutputFormat,
  type Preset,
  type Settings,
  type Theme,
  type VideosLayout,
} from "../lib/ipc";
import { isWindows } from "../lib/platform";
import {
  field,
  formatSelect,
  numberInput,
  parseOptionalPositiveInt,
  splitControl,
  switchRow,
} from "../lib/forms";
import { splitSummaryLabel } from "../lib/format";
import { showToast } from "../lib/toast";
import { friendlyError } from "../lib/errors";

export interface PreferencesView {
  el: HTMLElement;
  render(settings: Settings): void;
}

const newPresetId = (): string =>
  `preset-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;

export function createPreferencesView(opts: {
  onSettings: (s: Settings) => void;
}): PreferencesView {
  const el = document.createElement("div");
  el.className = "view view-prefs";

  let current: Settings | null = null;
  let lastJson = "";
  let editorOpen = false;
  let editingId: string | null = null; // null while editorOpen => creating a new preset
  let editorEl: HTMLElement | null = null; // live editor node, reused across repaints

  function render(settings: Settings): void {
    const json = JSON.stringify(settings);
    if (json === lastJson) {
      current = settings;
      return; // identical state; don't blow away an open editor
    }
    current = settings;
    lastJson = json;
    // The editor only closes from its own Save/Cancel; repaint around it.
    paint();
  }

  function closeEditor(): void {
    editorOpen = false;
    editingId = null;
    editorEl = null;
  }

  function openEditor(id: string | null): void {
    editorOpen = true;
    editingId = id;
    editorEl = null;
    paint();
  }

  function adopt(canonical: Settings): void {
    current = canonical;
    lastJson = JSON.stringify(canonical);
    paint();
    opts.onSettings(canonical);
  }

  async function persist(mutate: (draft: Settings) => void): Promise<boolean> {
    if (!current) return false;
    const draft: Settings = JSON.parse(JSON.stringify(current));
    mutate(draft);
    try {
      adopt(await saveSettings(draft));
      return true;
    } catch (e) {
      showToast(friendlyError(e), "error");
      // Part of the change may have been persisted (e.g. everything except
      // launch-at-login); re-fetch and repaint from the canonical state.
      try {
        adopt(await getSettings());
      } catch {
        paint(); // backend unreachable; snap controls back to last known state
      }
      return false;
    }
  }

  function editor(p: Preset | null): HTMLElement {
    if (!editorEl) editorEl = editorCard(p);
    return editorEl; // detached nodes keep input state across repaints
  }

  function paint(): void {
    el.innerHTML = "";
    if (!current) return;
    el.append(sectionLabel("Presets"));
    let editorPlaced = false;
    for (const p of current.presets) {
      if (editorOpen && editingId === p.id) {
        el.append(editor(p));
        editorPlaced = true;
      } else {
        el.append(presetCard(p));
      }
    }
    if (editorOpen && !editorPlaced) {
      // New preset, or the edited preset vanished in a remote update;
      // either way keep whatever the user has typed.
      el.append(editor(null));
    } else {
      const add = button("+ New preset", "btn-primary btn-block");
      add.addEventListener("click", () => openEditor(null));
      el.append(add);
    }
    el.append(sectionLabel("Behavior"), behaviorCard());
    el.append(sectionLabel("Videos screen"), videosLayoutCard());
    el.append(sectionLabel("Shortcuts"), shortcutsCard());
    el.append(sectionLabel("Watched folders"), foldersCard());
    el.append(sectionLabel("Appearance"), appearanceCard());
    el.append(versionLine());
  }

  function versionLine(): HTMLElement {
    const line = document.createElement("div");
    line.className = "version-line";
    line.textContent = "tamp";
    void getVersion().then((v) => {
      line.textContent = `tamp v${v}`;
    });
    return line;
  }

  function sectionLabel(text: string): HTMLElement {
    const label = document.createElement("div");
    label.className = "section-label";
    label.textContent = text;
    return label;
  }

  function button(text: string, className: string): HTMLButtonElement {
    const b = document.createElement("button");
    b.type = "button";
    b.className = className;
    b.textContent = text;
    return b;
  }

  function presetSummary(p: Preset): string {
    const parts = [`≤ ${p.targetMb} MB`];
    if (p.format && p.format !== "mp4") parts.push(p.format);
    if (p.maxFps != null) parts.push(`${p.maxFps} fps`);
    if (p.maxWidth != null) parts.push(`${p.maxWidth}px`);
    else if (p.scalePercent != null) parts.push(`${p.scalePercent}%`);
    if (p.stripAudio) parts.push("no audio");
    const split = splitSummaryLabel(p.split);
    if (split) parts.push(split);
    return parts.join(" · ");
  }

  function presetCard(p: Preset): HTMLElement {
    const s = current as Settings;
    const card = document.createElement("div");
    card.className = "card preset-card";

    const info = document.createElement("div");
    info.className = "preset-info";
    const name = document.createElement("div");
    name.className = "preset-name";
    name.textContent = p.name;
    const summary = document.createElement("div");
    summary.className = "preset-summary";
    summary.textContent = presetSummary(p);
    info.append(name, summary);

    const actions = document.createElement("div");
    actions.className = "preset-actions";

    const radioLabel = document.createElement("label");
    radioLabel.className = "radio";
    const radio = document.createElement("input");
    radio.type = "radio";
    radio.name = "default-preset";
    radio.checked = p.id === s.defaultPresetId;
    radio.addEventListener("change", () => {
      if (radio.checked) {
        void persist((d) => {
          d.defaultPresetId = p.id;
        });
      }
    });
    const radioText = document.createElement("span");
    radioText.textContent = "Default";
    radioLabel.append(radio, radioText);

    const edit = button("Edit", "btn-ghost");
    edit.addEventListener("click", () => openEditor(p.id));

    const del = button("Delete", "btn-ghost btn-danger");
    del.disabled = s.presets.length <= 1;
    del.addEventListener("click", () => {
      void persist((d) => {
        d.presets = d.presets.filter((x) => x.id !== p.id);
        if (d.defaultPresetId === p.id && d.presets.length > 0) {
          d.defaultPresetId = d.presets[0].id;
        }
      });
    });

    actions.append(radioLabel, edit, del);
    card.append(info, actions);
    return card;
  }

  function editorCard(p: Preset | null): HTMLElement {
    const card = document.createElement("div");
    card.className = "card preset-editor";

    const nameInput = document.createElement("input");
    nameInput.type = "text";
    nameInput.className = "input";
    nameInput.placeholder = "e.g. Slack (25MB)";
    nameInput.value = p?.name ?? "";

    const targetInput = numberInput(p?.targetMb ?? null, "10");
    targetInput.min = "0.1";
    targetInput.step = "0.5";

    const fpsInput = numberInput(p?.maxFps ?? null, "auto");
    const widthInput = numberInput(p?.maxWidth ?? null, "auto");
    const scaleInput = numberInput(p?.scalePercent ?? null, "auto");

    // Max width and scale % are mutually exclusive: typing one clears the other.
    widthInput.addEventListener("input", () => {
      if (widthInput.value.trim() !== "") scaleInput.value = "";
    });
    scaleInput.addEventListener("input", () => {
      if (scaleInput.value.trim() !== "") widthInput.value = "";
    });

    const formatInput = formatSelect(p?.format ?? "mp4");

    const split = splitControl(p?.split);

    const audio = switchRow("Strip audio", p?.stripAudio ?? false);

    const grid1 = document.createElement("div");
    grid1.className = "field-grid";
    grid1.append(field("Name", nameInput), field("Target MB", targetInput));

    const grid2 = document.createElement("div");
    grid2.className = "field-grid field-grid-3";
    grid2.append(
      field("Max FPS", fpsInput),
      field("Max width", widthInput),
      field("Scale %", scaleInput),
    );

    const hint = document.createElement("div");
    hint.className = "field-hint";
    hint.textContent = "Max width and scale % are mutually exclusive.";

    const grid3 = document.createElement("div");
    grid3.className = "field-grid";
    grid3.append(field("Format", formatInput));

    const actions = document.createElement("div");
    actions.className = "editor-actions";
    const save = button("Save", "btn-primary");
    const cancel = button("Cancel", "btn-ghost");
    cancel.addEventListener("click", () => {
      closeEditor();
      paint();
    });
    save.addEventListener("click", () => {
      const name = nameInput.value.trim();
      if (!name) {
        showToast("Preset name is required");
        return;
      }
      const target = Number(targetInput.value);
      if (!(target > 0)) {
        showToast("Target size must be greater than 0 MB");
        return;
      }
      const fps = parseOptionalPositiveInt(fpsInput.value);
      const width = parseOptionalPositiveInt(widthInput.value);
      const scale = parseOptionalPositiveInt(scaleInput.value);
      if (fps === undefined || width === undefined || scale === undefined) {
        showToast("FPS, width and scale must be positive whole numbers");
        return;
      }
      const splitRead = split.read();
      if ("error" in splitRead) {
        showToast(splitRead.error);
        return;
      }
      const preset: Preset = {
        id: p?.id ?? newPresetId(),
        name,
        targetMb: target,
        maxFps: fps,
        maxWidth: width,
        scalePercent: width != null ? null : scale,
        stripAudio: audio.input.checked,
        format: formatInput.value as OutputFormat,
        split: splitRead.config,
      };
      void persist((d) => {
        const idx = d.presets.findIndex((x) => x.id === preset.id);
        if (idx >= 0) d.presets[idx] = preset;
        else d.presets.push(preset);
      }).then((ok) => {
        if (ok) {
          closeEditor();
          paint();
        }
      });
    });
    actions.append(save, cancel);

    card.append(grid1, grid2, hint, grid3, split.el, audio.row, actions);
    return card;
  }

  function toggleRow(
    labelText: string,
    value: boolean,
    onChange: (v: boolean) => void,
  ): HTMLElement {
    const row = document.createElement("label");
    row.className = "toggle-row";
    row.innerHTML =
      `<span class="toggle-label"></span>` +
      `<span class="switch"><input type="checkbox"><span class="track"></span></span>`;
    const label = row.querySelector(".toggle-label") as HTMLElement;
    label.textContent = labelText;
    const input = row.querySelector("input") as HTMLInputElement;
    input.checked = value;
    input.addEventListener("change", () => onChange(input.checked));
    return row;
  }

  /** A labelled row of mutually-exclusive radio options. */
  function radioRow<T extends string>(
    labelText: string,
    value: T,
    options: ReadonlyArray<{ value: T; label: string }>,
    onChange: (v: T) => void,
  ): HTMLElement {
    const row = document.createElement("div");
    row.className = "radio-row";
    const name = `radio-${labelText.replace(/\s+/g, "-").toLowerCase()}`;
    const label = document.createElement("span");
    label.className = "toggle-label";
    label.id = `${name}-label`;
    label.textContent = labelText;
    const group = document.createElement("div");
    group.className = "radio-group";
    // Name the option set so AT announces it as a group (Theme / Open after…).
    group.setAttribute("role", "radiogroup");
    group.setAttribute("aria-labelledby", label.id);
    for (const opt of options) {
      const item = document.createElement("label");
      item.className = "radio-item";
      const input = document.createElement("input");
      input.type = "radio";
      input.name = name;
      input.checked = opt.value === value;
      input.addEventListener("change", () => {
        if (input.checked) onChange(opt.value);
      });
      const text = document.createElement("span");
      text.textContent = opt.label;
      item.append(input, text);
      group.append(item);
    }
    row.append(label, group);
    return row;
  }

  /** Radio cards choosing how the Videos screen offers presets. */
  function videosLayoutCard(): HTMLElement {
    const s = current as Settings;
    const card = document.createElement("div");
    card.className = "card";
    const options: { value: VideosLayout; title: string; desc: string }[] = [
      {
        value: "quick-pick",
        title: "Pick a preset each time",
        desc: "Clicking a video opens a quick menu — your default is preselected, or press 1–9 to choose another.",
      },
      {
        value: "active-bar",
        title: "Keep one preset active",
        desc: "A bar holds one active preset; clicking a video applies it instantly. Switch it with ‹ › or [ ].",
      },
    ];
    // Group the layout radios so AT announces them as one named set.
    const layoutGroup = document.createElement("div");
    layoutGroup.setAttribute("role", "radiogroup");
    layoutGroup.setAttribute("aria-label", "Videos screen layout");
    for (const opt of options) {
      const row = document.createElement("label");
      row.className = "option-row";
      const input = document.createElement("input");
      input.type = "radio";
      input.name = "videos-layout";
      input.className = "option-radio";
      input.checked = s.videosLayout === opt.value;
      input.addEventListener("change", () => {
        if (input.checked) {
          void persist((d) => {
            d.videosLayout = opt.value;
          });
        }
      });
      const text = document.createElement("div");
      text.className = "option-text";
      const title = document.createElement("div");
      title.className = "option-title";
      title.textContent = opt.title;
      const desc = document.createElement("div");
      desc.className = "option-desc";
      desc.textContent = opt.desc;
      text.append(title, desc);
      row.append(input, text);
      layoutGroup.appendChild(row);
    }
    // manual: SR announces "Videos screen layout, radio group" entering the set.
    card.appendChild(layoutGroup);

    const limitInput = numberInput(s.recentsLimit, "50");
    limitInput.min = "1";
    limitInput.max = "200";
    limitInput.step = "1";
    limitInput.addEventListener("change", () => {
      const n = Math.round(Number(limitInput.value));
      if (!Number.isFinite(n) || n < 1 || n > 200) {
        showToast("Recent videos shown must be a whole number from 1 to 200");
        limitInput.value = String((current as Settings).recentsLimit);
        return;
      }
      void persist((d) => {
        d.recentsLimit = n;
      });
    });
    card.append(field("Recent videos shown", limitInput));

    return card;
  }

  /** Theme + language pickers. */
  function appearanceCard(): HTMLElement {
    const s = current as Settings;
    const card = document.createElement("div");
    card.className = "card";
    card.append(
      radioRow<Theme>(
        "Theme",
        s.theme,
        [
          { value: "system", label: "System" },
          { value: "light", label: "Light" },
          { value: "dark", label: "Dark" },
        ],
        (v) =>
          void persist((d) => {
            d.theme = v;
          }),
      ),
      // The autonyms ("English"/"Українська") stay in their own language in
      // every locale so a user can always recognize their language.
      radioRow<LocaleSetting>(
        "Language",
        s.locale,
        [
          { value: "system", label: "System" },
          { value: "en", label: "English" },
          { value: "uk", label: "Українська" },
        ],
        (v) => {
          // Persist, then hard-reload so the whole webview re-reads settings
          // and re-renders fully localized — the simplest reliable apply. A
          // failed save shows a toast (via persist) and skips the reload, so
          // the controls snap back to the persisted language.
          // manual: switching language reloads the panel into the new locale.
          void persist((d) => {
            d.locale = v;
          }).then((ok) => {
            if (ok) location.reload();
          });
        },
      ),
    );
    return card;
  }

  /** The privacy sub-label under the "Check for updates automatically" switch:
   *  the single outbound request is a pull to GitHub and sends nothing about the
   *  user. Mirrors the first-run consent wording so the choice reads the same in
   *  both places. */
  function updateCheckHint(): HTMLElement {
    const hint = document.createElement("div");
    hint.className = "field-hint toggle-hint";
    hint.textContent =
      "Only asks GitHub for the latest version — nothing about you is sent.";
    return hint;
  }

  function behaviorCard(): HTMLElement {
    const s = current as Settings;
    const card = document.createElement("div");
    card.className = "card";
    card.append(
      toggleRow("Copy result to clipboard", s.copyToClipboard, (v) =>
        void persist((d) => {
          d.copyToClipboard = v;
        }),
      ),
      toggleRow("Move original to Trash", s.trashOriginal, (v) =>
        void persist((d) => {
          d.trashOriginal = v;
        }),
      ),
      toggleRow(
        "Use GPU encoder (faster, slightly lower quality)",
        s.useHardwareEncoder,
        (v) =>
          void persist((d) => {
            d.useHardwareEncoder = v;
          }),
      ),
      toggleRow("Launch at login", s.launchAtLogin, (v) =>
        void persist((d) => {
          d.launchAtLogin = v;
        }),
      ),
      toggleRow(
        "Check for updates automatically",
        s.updateCheckEnabled,
        (v) =>
          void persist((d) => {
            d.updateCheckEnabled = v;
          }),
      ),
      updateCheckHint(),
      radioRow<OpenAfterConvert>(
        "Open in file manager after converting",
        s.openAfterConvert,
        [
          { value: "off", label: "Off" },
          { value: "multipart", label: "Multi-part splits only" },
          { value: "all", label: "All conversions" },
        ],
        (v) =>
          void persist((d) => {
            d.openAfterConvert = v;
          }),
      ),
    );
    // Windows-only: register/remove the Explorer "Compress with tamp" entry.
    // The backend command both updates the registry and persists the setting,
    // so we reflect the choice locally rather than via persist()/saveSettings.
    if (isWindows()) {
      card.append(
        toggleRow(
          "Add “Compress with tamp” to Explorer’s right-click menu",
          s.contextMenuEnabled,
          (v) => {
            void setContextMenu(v)
              .then(() => {
                if (current) current.contextMenuEnabled = v;
                lastJson = JSON.stringify(current);
              })
              .catch((e) => {
                showToast(friendlyError(e), "error");
                paint(); // revert the switch to the persisted state
              });
          },
        ),
      );
    }
    return card;
  }

  /** Accelerator text input; empty disables. Persists on change (blur/Enter). */
  function shortcutField(
    labelText: string,
    value: string | null,
    placeholder: string,
    apply: (d: Settings, v: string | null) => void,
  ): HTMLElement {
    const input = document.createElement("input");
    input.type = "text";
    input.className = "input";
    input.placeholder = placeholder;
    input.value = value ?? "";
    input.addEventListener("change", () => {
      const v = input.value.trim();
      // The backend validates by attempting registration and rejects bad
      // accelerators; persist() then snaps back to the canonical settings.
      void persist((d) => apply(d, v === "" ? null : v));
    });
    return field(labelText, input);
  }

  function shortcutsCard(): HTMLElement {
    const s = current as Settings;
    const card = document.createElement("div");
    card.className = "card";

    const stack = document.createElement("div");
    stack.className = "field-stack";
    stack.append(
      shortcutField(
        "Compress latest recording",
        s.shortcutCompressLatest,
        "CmdOrCtrl+Alt+T",
        (d, v) => {
          d.shortcutCompressLatest = v;
        },
      ),
      shortcutField(
        "Show / hide panel",
        s.shortcutTogglePanel,
        "CmdOrCtrl+Alt+O",
        (d, v) => {
          d.shortcutTogglePanel = v;
        },
      ),
    );

    const staleInput = numberInput(s.staleWarnMinutes, "10");
    staleInput.min = "0";
    staleInput.step = "1";
    staleInput.addEventListener("change", () => {
      const n = Number(staleInput.value);
      if (!Number.isInteger(n) || n < 0) {
        showToast("Minutes must be a whole number (0 or more)");
        staleInput.value = String((current as Settings).staleWarnMinutes);
        return;
      }
      void persist((d) => {
        d.staleWarnMinutes = n;
      });
    });
    stack.append(
      field("Warn when the latest video is older than N minutes", staleInput),
    );

    const hint = document.createElement("div");
    hint.className = "field-hint";
    hint.textContent = "Leave a shortcut empty to disable it.";

    // The stale-recording warning rides on a notification, so a denied
    // notification permission silently kills it. A recovery row appears here
    // when permission is missing; it stays hidden when granted (the desktop
    // default) or unreadable.
    const notif = notificationStatusRow();

    card.append(stack, hint, notif);
    return card;
  }

  /** A recovery row shown when notifications are off: it warns the
   *  stale-recording notification won't fire and offers a re-request. The row
   *  starts hidden and reveals itself only once the async permission probe
   *  reports a non-granted, recoverable state — so on desktop (always
   *  "granted") and where the API is absent ("unsupported") it never appears. */
  function notificationStatusRow(): HTMLElement {
    const row = document.createElement("div");
    row.className = "folder-notice notif-notice";
    row.setAttribute("role", "status");
    row.hidden = true;

    const title = document.createElement("div");
    title.className = "folder-notice-title";
    title.textContent = "Notifications are off";
    const body = document.createElement("div");
    body.className = "folder-notice-hint";
    body.textContent =
      "The stale-recording warning (when the latest video is older than the limit above) won't show. Re-request below, or open System Settings if it's been turned off there.";
    const actions = document.createElement("div");
    actions.className = "onboarding-actions";
    const enable = button("Enable notifications", "btn-ghost");
    // The open-settings deep link is the real recovery for a hard OS-level deny
    // (where re-requesting is a no-op, e.g. macOS); the command no-ops where no
    // deep link exists, so the button is safe to offer unconditionally.
    const openSettings = button("Open System Settings", "btn-ghost");
    openSettings.addEventListener("click", () => {
      void openNotificationSettings().catch((e) =>
        showToast(friendlyError(e), "error"),
      );
    });
    actions.append(enable, openSettings);
    row.append(title, body, actions);

    const reveal = (state: NotificationPermission): void => {
      // Only a real, recoverable "off" state shows the row. "granted" hides it;
      // "unsupported" (no API) degrades to hidden rather than nagging.
      row.hidden = state === "granted" || state === "unsupported";
    };

    enable.addEventListener("click", () => {
      enable.disabled = true;
      void requestNotificationPermission()
        .then((state) => {
          reveal(state);
          if (state === "granted") {
            showToast("Notifications enabled", "success");
          }
        })
        .catch((e) => showToast(friendlyError(e), "error"))
        .finally(() => {
          enable.disabled = false;
        });
    });

    void notificationPermission()
      .then(reveal)
      .catch(() => {
        /* leave hidden on probe failure */
      });

    return row;
  }

  function foldersCard(): HTMLElement {
    const s = current as Settings;
    const card = document.createElement("div");
    card.className = "card";

    for (const folder of s.watchedFolders) {
      const row = document.createElement("div");
      row.className = "folder-row";
      const path = document.createElement("span");
      path.className = "folder-path";
      path.textContent = folder;
      path.title = folder;
      const remove = button("✕", "folder-remove");
      remove.title = "Remove folder";
      remove.setAttribute("aria-label", "Remove folder");
      remove.disabled = s.watchedFolders.length <= 1;
      remove.addEventListener("click", () => {
        void persist((d) => {
          d.watchedFolders = d.watchedFolders.filter((f) => f !== folder);
        });
      });
      row.append(path, remove);
      card.append(row);
    }

    const add = button("+ Add folder", "btn-primary btn-block");
    add.addEventListener("click", async () => {
      try {
        const picked = await pickFolder();
        if (!picked || !current) return;
        if (current.watchedFolders.includes(picked)) return;
        await persist((d) => {
          d.watchedFolders.push(picked);
        });
      } catch (e) {
        showToast(friendlyError(e), "error");
      }
    });
    card.append(add);
    return card;
  }

  return { el, render };
}
