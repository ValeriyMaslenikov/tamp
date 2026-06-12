// Small form-building helpers shared by the preset editor and the custom
// conversion page.

import type {
  OutputFormat,
  SplitConfig,
  SplitMode,
  StaticSplitBy,
} from "./ipc";

export function field(labelText: string, input: HTMLElement): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "field";
  const label = document.createElement("span");
  label.className = "field-label";
  label.textContent = labelText;
  wrap.append(label, input);
  return wrap;
}

export function numberInput(
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

const FORMAT_OPTIONS: ReadonlyArray<[OutputFormat, string]> = [
  ["mp4", "MP4"],
  ["webm", "WebM"],
  ["gif", "GIF"],
];

export function formatSelect(value: OutputFormat): HTMLSelectElement {
  const select = document.createElement("select");
  select.className = "input";
  for (const [v, label] of FORMAT_OPTIONS) {
    const opt = document.createElement("option");
    opt.value = v;
    opt.textContent = label;
    select.appendChild(opt);
  }
  select.value = value;
  return select;
}

/** Labeled "Strip audio"-style switch row; returns the row and its checkbox. */
export function switchRow(
  labelText: string,
  checked: boolean,
): { row: HTMLElement; input: HTMLInputElement } {
  const row = document.createElement("label");
  row.className = "toggle-row toggle-row-flat";
  row.innerHTML =
    `<span class="toggle-label"></span>` +
    `<span class="switch"><input type="checkbox"><span class="track"></span></span>`;
  (row.querySelector(".toggle-label") as HTMLElement).textContent = labelText;
  const input = row.querySelector("input") as HTMLInputElement;
  input.checked = checked;
  return { row, input };
}

/** "" -> null; positive integer -> number; anything else -> undefined (invalid). */
export function parseOptionalPositiveInt(
  raw: string,
): number | null | undefined {
  const t = raw.trim();
  if (!t) return null;
  const n = Number(t);
  if (!Number.isInteger(n) || n <= 0) return undefined;
  return n;
}

// ---------- Split-into-parts control ----------

/** Mirrors the backend's SplitConfig::default(). */
export const defaultSplitConfig = (): SplitConfig => ({
  mode: "off",
  by: "parts",
  parts: 2,
  seconds: 30,
});

export type SplitReadResult = { config: SplitConfig } | { error: string };

/**
 * Validate the split form inputs into a SplitConfig. Only the active static
 * sub-field is validated (parts 2-20, seconds >= 10); the inactive one falls
 * back to its default so stray text never blocks saving.
 */
export function parseSplitInputs(
  mode: SplitMode,
  by: StaticSplitBy,
  partsRaw: string,
  secondsRaw: string,
): SplitReadResult {
  const parts = Number(partsRaw.trim());
  const seconds = Number(secondsRaw.trim());
  const partsOk = Number.isInteger(parts) && parts >= 2 && parts <= 20;
  const secondsOk = Number.isInteger(seconds) && seconds >= 10;
  if (mode === "static" && by === "parts" && !partsOk) {
    return { error: "Split parts must be a whole number from 2 to 20" };
  }
  if (mode === "static" && by === "seconds" && !secondsOk) {
    return { error: "Split duration must be a whole number of seconds (10 or more)" };
  }
  return {
    config: {
      mode,
      by,
      parts: partsOk ? parts : 2,
      seconds: secondsOk ? seconds : 30,
    },
  };
}

function radioRow<T extends string>(
  name: string,
  options: ReadonlyArray<[T, string]>,
  value: T,
  onChange: (v: T) => void,
): HTMLElement {
  const row = document.createElement("div");
  row.className = "radio-row";
  for (const [v, labelText] of options) {
    const label = document.createElement("label");
    label.className = "radio";
    const input = document.createElement("input");
    input.type = "radio";
    input.name = name;
    input.value = v;
    input.checked = v === value;
    input.addEventListener("change", () => {
      if (input.checked) onChange(v);
    });
    const text = document.createElement("span");
    text.textContent = labelText;
    label.append(input, text);
    row.appendChild(label);
  }
  return row;
}

export interface SplitControl {
  el: HTMLElement;
  read(): SplitReadResult;
}

let splitControlSeq = 0; // unique radio-group names per live control

/**
 * "Split into parts" form section: Off / Smart / Static radios with the
 * static by-parts / by-duration sub-choice. Shared by the preset editor and
 * the custom conversion page.
 */
export function splitControl(initial?: SplitConfig): SplitControl {
  const start = initial ?? defaultSplitConfig();
  const seq = ++splitControlSeq;
  let mode: SplitMode = start.mode;
  let by: StaticSplitBy = start.by;

  const partsInput = numberInput(start.parts, "2");
  partsInput.min = "2";
  partsInput.max = "20";
  partsInput.step = "1";
  const secondsInput = numberInput(start.seconds, "30");
  secondsInput.min = "10";
  secondsInput.step = "1";

  const partsField = field("Parts (2–20)", partsInput);
  const secondsField = field("≈ every N seconds", secondsInput);

  const sub = document.createElement("div");
  sub.className = "split-sub";

  function sync(): void {
    sub.hidden = mode !== "static";
    partsField.hidden = by !== "parts";
    secondsField.hidden = by !== "seconds";
  }

  const modeRow = radioRow<SplitMode>(
    `split-mode-${seq}`,
    [
      ["off", "Off"],
      ["smart", "Smart"],
      ["static", "Static"],
    ],
    mode,
    (v) => {
      mode = v;
      sync();
    },
  );
  const byRow = radioRow<StaticSplitBy>(
    `split-by-${seq}`,
    [
      ["parts", "By parts"],
      ["seconds", "By duration"],
    ],
    by,
    (v) => {
      by = v;
      sync();
    },
  );

  sub.append(byRow, partsField, secondsField);
  sync();

  const el = field("Split into parts", modeRow);
  el.classList.add("split-field");
  el.appendChild(sub);

  return {
    el,
    read: () => parseSplitInputs(mode, by, partsInput.value, secondsInput.value),
  };
}
