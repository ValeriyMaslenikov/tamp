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
import { groupConversions, type ConvNode, type ConvPart } from "../lib/convgroup";
import { showToast } from "../lib/toast";
import { getCurrentWindow } from "@tauri-apps/api/window";

export interface ConvertedView {
  el: HTMLElement;
  refresh(): Promise<void>;
}

/** Next cursor index when moving by `dir`, clamped to [0, len-1]. From "no
 *  selection" (-1), down → first row, up → last row. -1 when there are no rows. */
export function nextNavIndex(cur: number, dir: number, len: number): number {
  if (len === 0) return -1;
  if (cur < 0) return dir > 0 ? 0 : len - 1;
  return Math.min(len - 1, Math.max(0, cur + dir));
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

  type RowActions = {
    kind: "single" | "group" | "part";
    play?: () => void;
    copy: () => void;
    reveal: () => void;
    expand?: () => void;
    collapse?: () => void;
    isOpen?: () => boolean;
  };
  const rowActions = new WeakMap<HTMLElement, RowActions>();
  let selEl: HTMLElement | null = null;
  // True until the first listConversions() resolves, so refresh() can show a
  // loading affordance only on the very first load (not on later refreshes,
  // where the prior content stays put until results arrive).
  let firstLoad = true;
  // The Created/Converted tooltip is attached to <body> on hover (see timeEl);
  // tracking the live one lets refresh()/teardown remove it so a rebuild while
  // hovering can't orphan it permanently (the row vanishes without a mouseleave).
  let activeTip: HTMLElement | null = null;
  function clearActiveTip(): void {
    if (activeTip) {
      activeTip.remove();
      activeTip = null;
    }
  }

  // A nav row's stable identity across rebuilds: a single's output path or a
  // group's folder. Stored on the element so refresh() can re-select by value
  // rather than by a now-stale DOM node.
  function rowKey(el: HTMLElement): string | null {
    return el.dataset.navKey ?? null;
  }
  /** The nav row carrying `key`, or null. (Parts of a collapsed group are kept
   *  out of the keyboard order by navRows() but still exist in the DOM, so a
   *  group is re-expanded before this runs to surface a selected part.) */
  function findRowByKey(key: string): HTMLElement | null {
    for (const el of scroll.querySelectorAll<HTMLElement>(".conv-nav")) {
      if (rowKey(el) === key) return el;
    }
    return null;
  }

  const doPlay = (p: string) => openFile(p).catch((e) => showToast(String(e), "error"));
  const doCopy = (p: string) =>
    copyFile(p)
      .then(() => showToast("Copied to clipboard", "success"))
      .catch((e) => showToast(String(e), "error"));
  const doReveal = (p: string) => reveal(p).catch((e) => showToast(String(e), "error"));

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
      openFile(outputPath).catch((err) => showToast(String(err), "error"));
    });
    return play;
  }

  function copyButton(outputPath: string): HTMLButtonElement {
    const copy = actionButton("Copy file", "Copy compressed file", COPY_SVG);
    copy.addEventListener("click", (e) => {
      e.stopPropagation();
      copyFile(outputPath)
        .then(() => showToast("Copied to clipboard", "success"))
        .catch((err) => showToast(String(err), "error"));
    });
    return copy;
  }

  function revealButton(path: string, title: string): HTMLButtonElement {
    const rev = actionButton("Reveal", title, REVEAL_SVG);
    rev.addEventListener("click", (e) => {
      e.stopPropagation();
      reveal(path).catch((err) => showToast(String(err), "error"));
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
      `<span class="conv-tip-row"><b class="rec">Created</b><span></span></span>` +
      `<span class="conv-tip-row"><b class="conv">Converted</b><span></span></span>`;
    const vals = tip.querySelectorAll("span span");
    (vals[0] as HTMLElement).textContent = formatAbsolute(recordedMs);
    (vals[1] as HTMLElement).textContent = formatAbsolute(convertedMs);
    // The sub-line and scroll container clip overflow, so the popover is
    // attached to <body> and positioned over the viewport on hover instead of
    // nesting it (where it would be clipped and never show).
    wrap.addEventListener("mouseenter", () => {
      const r = wrap.getBoundingClientRect();
      tip.style.right = `${Math.max(8, window.innerWidth - r.right)}px`;
      tip.style.bottom = `${window.innerHeight - r.top + 6}px`;
      // Drop any previously-tracked tip (e.g. a rebuild orphaned it) before
      // showing — and ours becomes the tracked one for refresh()/teardown.
      clearActiveTip();
      document.body.appendChild(tip);
      activeTip = tip;
    });
    wrap.addEventListener("mouseleave", () => {
      tip.remove();
      if (activeTip === tip) activeTip = null;
    });
    return wrap;
  }

  function singleRow(node: Extract<ConvNode, { kind: "single" }>): HTMLElement {
    const outputPath = node.output.path;
    const r = document.createElement("div");
    r.className = "conv-row";
    r.classList.add("conv-nav");
    r.dataset.navKey = `single:${outputPath}`;
    rowActions.set(r, {
      kind: "single",
      play: () => doPlay(outputPath),
      copy: () => doCopy(outputPath),
      reveal: () => doReveal(outputPath),
    });

    // Empty toggle-gutter so the thumbnail lines up with multi-part rows, whose
    // chevron would otherwise push their thumbnail further right (jagged edge).
    const gutter = document.createElement("span");
    gutter.className = "conv-gutter";

    r.append(gutter, thumb(outputPath));

    const meta = document.createElement("div");
    meta.className = "conv-meta";
    const name = document.createElement("div");
    name.className = "conv-name";
    name.textContent = basename(node.inputPath);
    name.title = node.inputPath;
    const sub = document.createElement("div");
    sub.className = "conv-sub";
    const before = node.inputBytes ? formatBytes(node.inputBytes) : "—";
    const after = formatBytes(node.output.bytes);
    sub.innerHTML =
      `${before} → <b class="conv-after">${after}</b> · ` +
      `<span class="conv-where"></span> · `;
    (sub.querySelector(".conv-where") as HTMLElement).textContent = node.presetName;
    sub.append(timeEl(node.inputCreatedMs, node.completedAtMs));
    meta.append(name, sub);

    r.append(meta, playButton(outputPath), copyButton(outputPath), revealButton(outputPath, "Show in file manager"));
    return r;
  }

  /** One part of a split conversion: a compact text row inside the group block
   *  (no card, no thumbnail) with a thin tree connector — like the mockup. */
  function partRow(part: ConvPart, index: number): HTMLElement {
    const r = document.createElement("div");
    r.className = "conv-part";
    r.classList.add("conv-nav");
    r.dataset.navKey = `part:${part.path}`;
    rowActions.set(r, {
      kind: "part",
      play: () => doPlay(part.path),
      copy: () => doCopy(part.path),
      reveal: () => doReveal(part.path),
    });

    const no = document.createElement("span");
    no.className = "conv-partno";
    no.textContent = String(index + 1);

    const name = document.createElement("span");
    name.className = "conv-cname";
    name.textContent = basename(part.path);
    name.title = part.path;

    const size = document.createElement("span");
    size.className = "conv-csize";
    size.textContent = formatBytes(part.bytes);

    r.append(
      no,
      name,
      size,
      playButton(part.path),
      copyButton(part.path),
      revealButton(part.path, "Show in file manager"),
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

    parent.append(chevron, thumb(node.parts[0].path));

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
      Promise.all(node.parts.map((p) => copyFile(p.path)))
        .then(() => showToast("Copied to clipboard", "success"))
        .catch((err) => showToast(String(err), "error"));
    });

    parent.append(meta, copyAll, revealButton(node.folder, "Open output folder"));

    const children = document.createElement("div");
    children.className = "conv-children";
    children.hidden = true;
    node.parts.forEach((part, i) => children.append(partRow(part, i)));

    const expand = () => { wrap.classList.add("is-open"); children.hidden = false; };
    const collapse = () => { wrap.classList.remove("is-open"); children.hidden = true; };
    const isOpen = () => wrap.classList.contains("is-open");
    parent.addEventListener("click", () => (isOpen() ? collapse() : expand()));
    parent.classList.add("conv-nav");
    parent.dataset.navKey = `group:${node.folder}`;
    rowActions.set(parent, {
      kind: "group",
      copy: () => Promise.all(node.parts.map((p) => copyFile(p.path)))
        .then(() => showToast("Copied to clipboard", "success")).catch((e) => showToast(String(e), "error")),
      reveal: () => doReveal(node.folder),
      expand, collapse, isOpen,
    });

    wrap.append(parent, children);
    return wrap;
  }

  function navRows(): HTMLElement[] {
    return Array.from(scroll.querySelectorAll<HTMLElement>(".conv-nav")).filter((el) => {
      const kids = el.closest(".conv-children") as HTMLElement | null;
      return !kids || !kids.hidden; // skip parts of a collapsed group
    });
  }
  function setSelected(node: HTMLElement | null): void {
    if (selEl) selEl.classList.remove("is-sel");
    selEl = node;
    if (node) {
      node.classList.add("is-sel");
      node.scrollIntoView({ block: "nearest" });
    }
  }
  function moveSel(dir: number): void {
    const rows = navRows();
    const cur = selEl ? rows.indexOf(selEl) : -1;
    const i = nextNavIndex(cur, dir, rows.length);
    setSelected(i >= 0 ? rows[i] : null);
  }
  function onKeyDown(e: KeyboardEvent): void {
    if (el.hidden) return; // only when the Converted tab is active
    if (e.key === "ArrowDown") { e.preventDefault(); moveSel(1); return; }
    if (e.key === "ArrowUp") { e.preventDefault(); moveSel(-1); return; }
    if (!selEl) return;
    const a = rowActions.get(selEl);
    if (!a) return;
    switch (e.key) {
      case "Enter":
      case " ":
        e.preventDefault();
        if (a.kind === "group") (a.isOpen?.() ? a.collapse : a.expand)?.();
        else a.play?.();
        return;
      case "ArrowRight":
      case "e":
        if (a.kind === "group" && !a.isOpen?.()) { e.preventDefault(); a.expand?.(); }
        return;
      case "ArrowLeft":
        if (a.kind === "group" && a.isOpen?.()) { e.preventDefault(); a.collapse?.(); }
        return;
      case "c": e.preventDefault(); a.copy(); return;
      case "r": e.preventDefault(); a.reveal(); return;
      case "Escape":
        if (a.kind === "group" && a.isOpen?.()) { e.preventDefault(); a.collapse?.(); return; }
        e.preventDefault();
        void getCurrentWindow().hide().catch(() => {});
        return;
    }
  }
  document.addEventListener("keydown", onKeyDown);
  // manual: the view lives for the app's lifetime (main.ts never tears it down),
  // so the only place a body-attached tip can be orphaned is a rebuild — handled
  // at the top of refresh() via clearActiveTip(). If a destroy() path is ever
  // added, call clearActiveTip() and removeEventListener there too.

  /** The folders of every currently-expanded group (DOM state, pre-rebuild). */
  function expandedFolders(): Set<string> {
    const open = new Set<string>();
    for (const parent of scroll.querySelectorAll<HTMLElement>(".conv-tree.is-open > .conv-tree-parent")) {
      const key = rowKey(parent);
      if (key) open.add(key);
    }
    return open;
  }
  /** Re-expand the groups whose folder key was open before the rebuild. */
  function restoreExpanded(open: Set<string>): void {
    if (open.size === 0) return;
    for (const parent of scroll.querySelectorAll<HTMLElement>(".conv-tree > .conv-tree-parent")) {
      const key = rowKey(parent);
      if (key && open.has(key)) rowActions.get(parent)?.expand?.();
    }
  }

  async function refresh(): Promise<void> {
    // A rebuild removes the hovered row without firing mouseleave, so drop any
    // body-attached tooltip first — otherwise it floats over the panel forever.
    clearActiveTip();

    // On the very first load, show a placeholder instead of a blank scroll while
    // listConversions() is in flight. Later refreshes keep the prior content.
    if (firstLoad && scroll.childElementCount === 0) {
      const loading = document.createElement("div");
      loading.className = "empty";
      loading.textContent = "Loading…";
      scroll.append(loading);
    }

    let records: ConversionRecord[];
    try {
      records = await listConversions();
    } catch (e) {
      // Drop the first-load placeholder so a failed first load doesn't stick on
      // "Loading…" forever; later refreshes leave the prior content untouched.
      if (firstLoad) scroll.innerHTML = "";
      firstLoad = false;
      showToast(String(e), "error");
      return;
    }
    firstLoad = false;

    // Remember the user's place before wiping the DOM: which row was selected
    // (by stable key) and which groups were expanded, so a background refresh
    // (tab switch / a finished encode) doesn't yank them to the top.
    const hadSelection = selEl !== null;
    const selectedKey = selEl ? rowKey(selEl) : null;
    const open = expandedFolders();

    scroll.innerHTML = "";
    selEl = null;
    if (records.length === 0) {
      const empty = document.createElement("div");
      empty.className = "empty";
      empty.textContent =
        "No conversions yet.\nCompress a video and it'll show up here — including files from outside your watched folders.";
      scroll.append(empty);
      return;
    }
    for (const node of groupConversions(records)) {
      scroll.append(node.kind === "single" ? singleRow(node) : groupNode(node));
    }

    restoreExpanded(open);

    // Re-select the previously-selected row if it still exists; otherwise only
    // fall back to the first row on a true first load (no prior selection). Match
    // by stored key rather than a CSS attribute selector to sidestep escaping the
    // path characters (\, /, etc.) the keys contain.
    const restored = selectedKey ? findRowByKey(selectedKey) : null;
    if (restored) setSelected(restored);
    else if (!hadSelection) setSelected(navRows()[0] ?? null);
    // manual: scroll down, select a row, expand a split group, then trigger a
    // refresh (tab away/back, or let a queued conversion finish) — the selection
    // and the expanded group are preserved, and no stray time tooltip is left
    // floating over the panel.
  }

  return { el, refresh };
}
