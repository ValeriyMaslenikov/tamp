// Download-manager style activity drawer: shows running + just-finished
// conversions over the bottom of the panel, from any source (watched folders
// or dropped externals), then dismisses itself once everything is done.
import { copyFile, reveal, type JobState, type Phase } from "./ipc";
import { formatBytes } from "./format";
import { showToast } from "./toast";

const RUNNING: ReadonlySet<Phase> = new Set(["queued", "pass1", "pass2", "verifying"]);
/** How long a finished row lingers before it (and an idle drawer) clears. */
const KEEP_DONE_MS = 8000;

export interface Drawer {
  updateJob(state: JobState): void;
}

export function createDrawer(panel: HTMLElement): Drawer {
  const el = document.createElement("div");
  el.className = "drawer";
  el.hidden = true;
  const head = document.createElement("div");
  head.className = "drawer-head";
  const title = document.createElement("span");
  title.className = "drawer-title";
  const close = document.createElement("button");
  close.type = "button";
  close.className = "drawer-close";
  close.setAttribute("aria-label", "Dismiss");
  close.textContent = "✕";
  head.append(title, close);
  const rowsEl = document.createElement("div");
  rowsEl.className = "drawer-rows";
  el.append(head, rowsEl);
  panel.appendChild(el);

  const jobs = new Map<string, JobState>();
  const expiry = new Map<string, number>(); // job id -> clearTimeout handle

  close.addEventListener("click", () => {
    for (const t of expiry.values()) window.clearTimeout(t);
    expiry.clear();
    jobs.clear();
    render();
  });

  function scheduleExpiry(id: string): void {
    if (expiry.has(id)) return;
    const t = window.setTimeout(() => {
      expiry.delete(id);
      jobs.delete(id);
      render();
    }, KEEP_DONE_MS);
    expiry.set(id, t);
  }

  function actionBtn(svg: string, title: string, on: () => void): HTMLButtonElement {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "drawer-act";
    b.title = title;
    b.innerHTML = svg;
    b.addEventListener("click", on);
    return b;
  }

  const COPY =
    `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><rect x="5.5" y="5.5" width="8" height="8" rx="1.5"/><path d="M10.5 5.5V4A1.5 1.5 0 0 0 9 2.5H4A1.5 1.5 0 0 0 2.5 4v5A1.5 1.5 0 0 0 4 10.5h1.5"/></svg>`;
  const REVEAL =
    `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M2 5.5 2.6 4A1.5 1.5 0 0 1 4 3h2.2l1 1.5H12A1.5 1.5 0 0 1 13.5 6v5A1.5 1.5 0 0 1 12 12.5H3.5A1.5 1.5 0 0 1 2 11Z"/></svg>`;

  function rowFor(j: JobState): HTMLElement {
    const r = document.createElement("div");
    r.className = "drawer-row";
    const meta = document.createElement("div");
    meta.className = "drawer-meta";
    const name = document.createElement("div");
    name.className = "drawer-name";
    name.textContent = j.inputName;
    name.title = j.inputPath;
    const tag = document.createElement("span");
    name.append(" ");
    name.append(tag);
    meta.append(name);

    if (RUNNING.has(j.phase)) {
      const pct = Math.round((j.progress ?? 0) * 100);
      tag.className = "drawer-tag run";
      tag.textContent = j.phase === "queued" ? "queued" : `${pct}%`;
      const bar = document.createElement("div");
      bar.className = "drawer-bar";
      const fill = document.createElement("i");
      fill.style.width = `${pct}%`;
      bar.append(fill);
      meta.append(bar);
      r.append(meta);
    } else if (j.phase === "done") {
      tag.className = "drawer-tag ok";
      tag.textContent = j.outputBytes != null ? `✓ ${formatBytes(j.outputBytes)}` : "✓ done";
      r.append(meta);
      if (j.outputPath) {
        const out = j.outputPath;
        r.append(
          actionBtn(COPY, "Copy compressed file", () =>
            copyFile(out)
              .then(() => showToast("Copied to clipboard"))
              .catch((e) => showToast(String(e))),
          ),
          actionBtn(REVEAL, "Show in file manager", () =>
            reveal(out).catch((e) => showToast(String(e))),
          ),
        );
      }
    } else {
      tag.className = "drawer-tag fail";
      tag.textContent = "✕ failed";
      r.append(meta);
    }
    return r;
  }

  function render(): void {
    const list = [...jobs.values()];
    if (list.length === 0) {
      el.hidden = true;
      rowsEl.innerHTML = "";
      return;
    }
    el.hidden = false;
    const running = list.filter((j) => RUNNING.has(j.phase)).length;
    const done = list.filter((j) => j.phase === "done").length;
    title.textContent =
      running > 0
        ? `Compressing… ${running} running${done ? `, ${done} done` : ""}`
        : `Conversions · ${done} done`;
    // Running items first, then the rest (most-recently-updated order otherwise).
    list.sort((a, b) => Number(RUNNING.has(b.phase)) - Number(RUNNING.has(a.phase)));
    rowsEl.innerHTML = "";
    for (const j of list) rowsEl.append(rowFor(j));
  }

  return {
    updateJob(state: JobState): void {
      if (state.phase === "cancelled") {
        // Cancelled jobs don't belong in an activity feed.
        const t = expiry.get(state.id);
        if (t !== undefined) {
          window.clearTimeout(t);
          expiry.delete(state.id);
        }
        jobs.delete(state.id);
        render();
        return;
      }
      jobs.set(state.id, state);
      if (state.phase === "done" || state.phase === "failed") {
        scheduleExpiry(state.id);
      }
      render();
    },
  };
}
