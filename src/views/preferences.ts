import {
  pickFolder,
  saveSettings,
  type Preset,
  type Settings,
} from "../lib/ipc";
import { showToast } from "../lib/toast";

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

  function render(settings: Settings): void {
    const json = JSON.stringify(settings);
    if (json === lastJson) {
      current = settings;
      return; // identical state; don't blow away an open editor
    }
    current = settings;
    lastJson = json;
    editorOpen = false;
    editingId = null;
    paint();
  }

  async function persist(mutate: (draft: Settings) => void): Promise<void> {
    if (!current) return;
    const draft: Settings = JSON.parse(JSON.stringify(current));
    mutate(draft);
    try {
      const canonical = await saveSettings(draft);
      editorOpen = false;
      editingId = null;
      current = canonical;
      lastJson = JSON.stringify(canonical);
      paint();
      opts.onSettings(canonical);
    } catch (e) {
      showToast(String(e));
      paint(); // snap controls back to the persisted state
    }
  }

  function paint(): void {
    el.innerHTML = "";
    if (!current) return;
    el.append(sectionLabel("Presets"));
    for (const p of current.presets) {
      el.append(
        editorOpen && editingId === p.id ? editorCard(p) : presetCard(p),
      );
    }
    if (editorOpen && editingId === null) {
      el.append(editorCard(null));
    } else {
      const add = button("+ New preset", "btn-primary btn-block");
      add.addEventListener("click", () => {
        editorOpen = true;
        editingId = null;
        paint();
      });
      el.append(add);
    }
    el.append(sectionLabel("Behavior"), behaviorCard());
    el.append(sectionLabel("Watched folders"), foldersCard());
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
    if (p.maxFps != null) parts.push(`${p.maxFps} fps`);
    if (p.maxWidth != null) parts.push(`${p.maxWidth}px`);
    else if (p.scalePercent != null) parts.push(`${p.scalePercent}%`);
    if (p.stripAudio) parts.push("no audio");
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
    edit.addEventListener("click", () => {
      editorOpen = true;
      editingId = p.id;
      paint();
    });

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

  function field(labelText: string, input: HTMLElement): HTMLElement {
    const wrap = document.createElement("div");
    wrap.className = "field";
    const label = document.createElement("span");
    label.className = "field-label";
    label.textContent = labelText;
    wrap.append(label, input);
    return wrap;
  }

  function numberInput(
    value: number | null,
    placeholder: string,
  ): HTMLInputElement {
    const input = document.createElement("input");
    input.type = "number";
    input.className = "input";
    input.placeholder = placeholder;
    if (value != null) input.value = String(value);
    return input;
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

    const audioRow = document.createElement("label");
    audioRow.className = "toggle-row toggle-row-flat";
    audioRow.innerHTML =
      `<span class="toggle-label">Strip audio</span>` +
      `<span class="switch"><input type="checkbox"><span class="track"></span></span>`;
    const audioInput = audioRow.querySelector("input") as HTMLInputElement;
    audioInput.checked = p?.stripAudio ?? false;

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

    const actions = document.createElement("div");
    actions.className = "editor-actions";
    const save = button("Save", "btn-primary");
    const cancel = button("Cancel", "btn-ghost");
    cancel.addEventListener("click", () => {
      editorOpen = false;
      editingId = null;
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
      const preset: Preset = {
        id: p?.id ?? newPresetId(),
        name,
        targetMb: target,
        maxFps: fps,
        maxWidth: width,
        scalePercent: width != null ? null : scale,
        stripAudio: audioInput.checked,
      };
      void persist((d) => {
        const idx = d.presets.findIndex((x) => x.id === preset.id);
        if (idx >= 0) d.presets[idx] = preset;
        else d.presets.push(preset);
      });
    });
    actions.append(save, cancel);

    card.append(grid1, grid2, hint, audioRow, actions);
    return card;
  }

  /** "" -> null; positive integer -> number; anything else -> undefined (invalid). */
  function parseOptionalPositiveInt(raw: string): number | null | undefined {
    const t = raw.trim();
    if (!t) return null;
    const n = Number(t);
    if (!Number.isInteger(n) || n <= 0) return undefined;
    return n;
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
      toggleRow("Launch at login", s.launchAtLogin, (v) =>
        void persist((d) => {
          d.launchAtLogin = v;
        }),
      ),
    );
    return card;
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
        showToast(String(e));
      }
    });
    card.append(add);
    return card;
  }

  return { el, render };
}
