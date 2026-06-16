import {
  conversionThumb,
  convertFileSrc,
  copyFile,
  listConversions,
  openFile,
  reveal,
  type ConversionRecord,
} from "../lib/ipc";
import { formatAbsolute, formatBytes, formatRelativeTime } from "../lib/format";
import { groupConversions, type ConvNode } from "../lib/convgroup";
import { showToast } from "../lib/toast";

export interface ConvertedView {
  el: HTMLElement;
  refresh(): Promise<void>;
}

function basename(p: string): string {
  return p.split(/[\\/]/).pop() ?? p;
}

const COPY_SVG =
  `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" ` +
  `stroke-linecap="round" stroke-linejoin="round"><rect x="5.5" y="5.5" width="8" height="8" rx="1.5"/>` +
  `<path d="M10.5 5.5V4A1.5 1.5 0 0 0 9 2.5H4A1.5 1.5 0 0 0 2.5 4v5A1.5 1.5 0 0 0 4 10.5h1.5"/></svg>`;
const REVEAL_SVG =
  `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" ` +
  `stroke-linecap="round" stroke-linejoin="round"><path d="M2 5.5 2.6 4A1.5 1.5 0 0 1 4 3h2.2l1 1.5H12A1.5 1.5 0 0 1 13.5 6v5A1.5 1.5 0 0 1 12 12.5H3.5A1.5 1.5 0 0 1 2 11Z"/></svg>`;
const PLAY_SVG =
  `<svg viewBox="0 0 16 16" fill="currentColor"><path d="M5 3.5v9l7-4.5z"/></svg>`;
const CHEVRON_SVG =
  `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" ` +
  `stroke-linecap="round" stroke-linejoin="round"><path d="M6 4l4 4-4 4"/></svg>`;

/** The Converted tab: a durable history of every conversion (journal-backed),
 *  including videos compressed from outside the watched folders. Multi-part
 *  (split) conversions collapse into one expandable folder-style row. */
export function createConvertedView(): ConvertedView {
  const el = document.createElement("div");
  el.className = "view view-converted";
  const scroll = document.createElement("div");
  scroll.className = "list-scroll";
  el.append(scroll);

  function actionButton(
    label: string,
    title: string,
    svg: string,
    extraClass = "",
  ): HTMLButtonElement {
    const b = document.createElement("button");
    b.type = "button";
    b.className = extraClass ? `conv-act ${extraClass}` : "conv-act";
    b.title = title;
    b.setAttribute("aria-label", label);
    b.innerHTML = svg;
    return b;
  }

  /** A lazy thumbnail for one output video; falls back to an empty surface. */
  function thumb(outputPath: string): HTMLElement {
    const img = document.createElement("img");
    img.className = "conv-thumb";
    img.alt = "";
    img.draggable = false;
    conversionThumb(outputPath)
      .then((path) => {
        if (path) img.src = convertFileSrc(path);
      })
      .catch(() => {});
    return img;
  }

  function playButton(outputPath: string): HTMLButtonElement {
    const play = actionButton("Play", "Play the converted video", PLAY_SVG, "conv-play");
    play.addEventListener("click", (e) => {
      e.stopPropagation();
      openFile(outputPath).catch((err) => showToast(String(err)));
    });
    return play;
  }

  function copyButton(outputPath: string): HTMLButtonElement {
    const copy = actionButton("Copy file", "Copy compressed file", COPY_SVG);
    copy.addEventListener("click", (e) => {
      e.stopPropagation();
      copyFile(outputPath)
        .then(() => showToast("Copied to clipboard"))
        .catch((err) => showToast(String(err)));
    });
    return copy;
  }

  function revealButton(path: string, title: string): HTMLButtonElement {
    const rev = actionButton("Reveal", title, REVEAL_SVG);
    rev.addEventListener("click", (e) => {
      e.stopPropagation();
      reveal(path).catch((err) => showToast(String(err)));
    });
    return rev;
  }

  /** The time element, carrying a Recorded-vs-Converted tooltip on hover. */
  function timeEl(recordedMs: number, convertedMs: number): HTMLElement {
    const wrap = document.createElement("span");
    wrap.className = "conv-time";
    wrap.textContent = formatRelativeTime(convertedMs);
    const tip = document.createElement("span");
    tip.className = "conv-tip";
    tip.innerHTML =
      `<span class="conv-tip-row"><b class="rec">Recorded</b><span></span></span>` +
      `<span class="conv-tip-row"><b class="conv">Converted</b><span></span></span>`;
    const vals = tip.querySelectorAll("span span");
    (vals[0] as HTMLElement).textContent = formatAbsolute(recordedMs);
    (vals[1] as HTMLElement).textContent = formatAbsolute(convertedMs);
    wrap.appendChild(tip);
    return wrap;
  }

  function singleRow(rec: ConversionRecord): HTMLElement {
    const r = document.createElement("div");
    r.className = "conv-row";

    r.append(thumb(rec.outputPath));

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
      `<span class="conv-where"></span> · `;
    (sub.querySelector(".conv-where") as HTMLElement).textContent = rec.presetName;
    sub.append(timeEl(rec.inputCreatedMs, rec.completedAtMs));
    meta.append(name, sub);

    r.append(meta, playButton(rec.outputPath), copyButton(rec.outputPath), revealButton(rec.outputPath, "Show in file manager"));
    return r;
  }

  /** One part of a split conversion: a compact text row inside the group block
   *  (no card, no thumbnail) with a thin tree connector — like the mockup. */
  function partRow(part: ConversionRecord, index: number): HTMLElement {
    const r = document.createElement("div");
    r.className = "conv-part";

    const line = document.createElement("span");
    line.className = "conv-tline";

    const no = document.createElement("span");
    no.className = "conv-partno";
    no.textContent = String(index + 1);

    const name = document.createElement("span");
    name.className = "conv-cname";
    name.textContent = basename(part.outputPath);
    name.title = part.outputPath;

    const size = document.createElement("span");
    size.className = "conv-csize";
    size.textContent = formatBytes(part.outputBytes);

    r.append(
      line,
      no,
      name,
      size,
      playButton(part.outputPath),
      copyButton(part.outputPath),
      revealButton(part.outputPath, "Show in file manager"),
    );
    return r;
  }

  function groupNode(
    node: Extract<ConvNode, { kind: "group" }>,
  ): HTMLElement {
    const wrap = document.createElement("div");
    wrap.className = "conv-tree";

    const parent = document.createElement("div");
    parent.className = "conv-tree-parent";

    const chevron = document.createElement("span");
    chevron.className = "conv-chevron";
    chevron.innerHTML = CHEVRON_SVG;

    parent.append(chevron, thumb(node.parts[0].outputPath));

    const meta = document.createElement("div");
    meta.className = "conv-meta";
    const name = document.createElement("div");
    name.className = "conv-name";
    name.textContent = basename(node.inputPath);
    name.title = node.inputPath;
    const sub = document.createElement("div");
    sub.className = "conv-sub";
    const before = node.inputBytes ? formatBytes(node.inputBytes) : "—";
    const after = formatBytes(node.totalBytes);
    sub.innerHTML =
      `${before} → <b class="conv-after">${after}</b> · ` +
      `<span class="badge-parts"></span> · ` +
      `<span class="conv-where"></span> · `;
    (sub.querySelector(".badge-parts") as HTMLElement).textContent =
      `${node.parts.length} parts`;
    (sub.querySelector(".conv-where") as HTMLElement).textContent = node.presetName;
    sub.append(timeEl(node.inputCreatedMs, node.completedAtMs));
    meta.append(name, sub);

    const copyAll = actionButton("Copy all", "Copy all parts", COPY_SVG);
    copyAll.addEventListener("click", (e) => {
      e.stopPropagation();
      Promise.all(node.parts.map((p) => copyFile(p.outputPath)))
        .then(() => showToast("Copied to clipboard"))
        .catch((err) => showToast(String(err)));
    });

    parent.append(meta, copyAll, revealButton(node.folder, "Open output folder"));

    const children = document.createElement("div");
    children.className = "conv-children";
    children.hidden = true;
    node.parts.forEach((part, i) => children.append(partRow(part, i)));

    parent.addEventListener("click", () => {
      const open = wrap.classList.toggle("is-open");
      children.hidden = !open;
    });

    wrap.append(parent, children);
    return wrap;
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
    for (const node of groupConversions(records)) {
      scroll.append(node.kind === "single" ? singleRow(node.rec) : groupNode(node));
    }
  }

  return { el, refresh };
}
