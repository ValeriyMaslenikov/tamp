// Small form-building helpers shared by the preset editor and the custom
// conversion page.

import type { OutputFormat } from "./ipc";

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
