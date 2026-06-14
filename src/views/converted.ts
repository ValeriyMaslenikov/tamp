import {
  copyFile,
  listConversions,
  reveal,
  type ConversionRecord,
} from "../lib/ipc";
import { formatBytes, formatRelativeTime } from "../lib/format";
import { showToast } from "../lib/toast";

export interface ConvertedView {
  el: HTMLElement;
  refresh(): Promise<void>;
}

function basename(p: string): string {
  return p.split(/[\\/]/).pop() ?? p;
}

function dirname(p: string): string {
  const parts = p.split(/[\\/]/);
  parts.pop();
  return parts.join("\\") || p;
}

/** The Converted tab: a durable history of every conversion (journal-backed),
 *  including videos compressed from outside the watched folders. */
export function createConvertedView(): ConvertedView {
  const el = document.createElement("div");
  el.className = "view view-converted";
  const scroll = document.createElement("div");
  scroll.className = "list-scroll";
  el.append(scroll);

  function actionButton(label: string, title: string, svg: string): HTMLButtonElement {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "conv-act";
    b.title = title;
    b.setAttribute("aria-label", label);
    b.innerHTML = svg;
    return b;
  }

  const COPY_SVG =
    `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" ` +
    `stroke-linecap="round" stroke-linejoin="round"><rect x="5.5" y="5.5" width="8" height="8" rx="1.5"/>` +
    `<path d="M10.5 5.5V4A1.5 1.5 0 0 0 9 2.5H4A1.5 1.5 0 0 0 2.5 4v5A1.5 1.5 0 0 0 4 10.5h1.5"/></svg>`;
  const REVEAL_SVG =
    `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" ` +
    `stroke-linecap="round" stroke-linejoin="round"><path d="M2 5.5 2.6 4A1.5 1.5 0 0 1 4 3h2.2l1 1.5H12A1.5 1.5 0 0 1 13.5 6v5A1.5 1.5 0 0 1 12 12.5H3.5A1.5 1.5 0 0 1 2 11Z"/></svg>`;

  function row(rec: ConversionRecord): HTMLElement {
    const r = document.createElement("div");
    r.className = "conv-row";

    const meta = document.createElement("div");
    meta.className = "conv-meta";
    const name = document.createElement("div");
    name.className = "conv-name";
    name.textContent = basename(rec.inputPath);
    name.title = rec.inputPath;
    const sub = document.createElement("div");
    sub.className = "conv-sub";
    const before = rec.inputBytes ? formatBytes(rec.inputBytes) : "—";
    const after = formatBytes(rec.outputBytes);
    sub.innerHTML =
      `${before} → <b class="conv-after">${after}</b> · ` +
      `<span class="conv-where"></span> · ${formatRelativeTime(rec.completedAtMs)}`;
    (sub.querySelector(".conv-where") as HTMLElement).textContent = dirname(rec.outputPath);
    meta.append(name, sub);

    const copy = actionButton("Copy file", "Copy compressed file", COPY_SVG);
    copy.addEventListener("click", () => {
      copyFile(rec.outputPath)
        .then(() => showToast("Copied to clipboard"))
        .catch((e) => showToast(String(e)));
    });
    const rev = actionButton("Reveal", "Show in file manager", REVEAL_SVG);
    rev.addEventListener("click", () => {
      reveal(rec.outputPath).catch((e) => showToast(String(e)));
    });

    r.append(meta, copy, rev);
    return r;
  }

  async function refresh(): Promise<void> {
    let records: ConversionRecord[];
    try {
      records = await listConversions();
    } catch (e) {
      showToast(String(e));
      return;
    }
    scroll.innerHTML = "";
    if (records.length === 0) {
      const empty = document.createElement("div");
      empty.className = "empty";
      empty.textContent =
        "No conversions yet.\nCompress a video and it'll show up here — including files from outside your watched folders.";
      scroll.append(empty);
      return;
    }
    for (const rec of records) scroll.append(row(rec));
  }

  return { el, refresh };
}
