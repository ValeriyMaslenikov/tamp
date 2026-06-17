// Download-manager style activity drawer: shows running + just-finished
// conversions over the bottom of the panel, from any source (watched folders
// or dropped externals), then dismisses itself once everything is done.
import { copyFile, reveal, type JobState, type Phase } from "./ipc";
import { formatBytes } from "./format";
import { showToast } from "./toast";
import { friendlyError } from "./errors";
import { t } from "../i18n";

const RUNNING: ReadonlySet<Phase> = new Set(["queued", "pass1", "pass2", "verifying"]);
/** Phases that are actively encoding (a queued job hasn't started yet). */
const ACTIVE: ReadonlySet<Phase> = new Set(["pass1", "pass2", "verifying"]);
/** How long a finished row lingers before it (and an idle drawer) clears. */
const KEEP_DONE_MS = 8000;
/** A cancelled-into-empty drawer flashes "Cancelled" this long before clearing. */
const CANCELLED_FLASH_MS = 1200;
/** Cap on individually-rendered rows; the rest fold into a "+N more" line. */
const MAX_ROWS = 6;

export interface Drawer {
  updateJob(state: JobState): void;
}

/** What `render()` should draw — derived purely from the job set (unit-tested). */
export interface DrawerPlan {
  title: string;
  /** Jobs to render as their own row, in display order (running first). */
  rows: JobState[];
  /** Number of queued jobs folded into a single "N queued" summary (0 = none). */
  queuedSummary: number;
  /** Individual rows hidden by the cap, surfaced as a "+N more" line (0 = none). */
  overflow: number;
}

/**
 * Decide the drawer's contents from the full job set: keep the title's
 * running/done counts accurate over EVERY job, collapse a backlog of queued
 * jobs into one "N queued" summary, render running and done/failed rows
 * individually, and cap the individual rows (folding the rest into "+N more")
 * so a big batch drop can't grow an unbounded scroll over the list.
 */
export function planDrawer(jobs: Iterable<JobState>): DrawerPlan {
  const list = [...jobs];
  const running = list.filter((j) => RUNNING.has(j.phase)).length;
  const done = list.filter((j) => j.phase === "done").length;
  const title =
    running > 0
      ? done
        ? t("drawer.titleRunningWithDone", { running, done })
        : t("drawer.titleRunning", { running })
      : t("drawer.titleIdle", { done });

  const active = list.filter((j) => ACTIVE.has(j.phase));
  const queued = list.filter((j) => j.phase === "queued");
  const finished = list.filter((j) => !RUNNING.has(j.phase));

  // Collapse a backlog of queued jobs into one summary; a lone queued job is
  // cheap enough to show as its own row.
  const queuedSummary = queued.length >= 2 ? queued.length : 0;
  const queuedRows = queuedSummary > 0 ? [] : queued;

  // Active encodes first, then any lone queued job, then the finished rows.
  const ordered = [...active, ...queuedRows, ...finished];
  const rows = ordered.slice(0, MAX_ROWS);
  const overflow = ordered.length - rows.length;

  return { title, rows, queuedSummary, overflow };
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
  close.setAttribute("aria-label", t("drawer.dismiss"));
  close.textContent = "✕";
  head.append(title, close);
  const rowsEl = document.createElement("div");
  rowsEl.className = "drawer-rows";
  el.append(head, rowsEl);
  panel.appendChild(el);

  const jobs = new Map<string, JobState>();
  const expiry = new Map<string, number>(); // job id -> clearTimeout handle
  // While a "Cancelled" flash is showing, the drawer is empty but stays visible
  // for a beat; later updates must not resurrect a real row underneath it, and
  // the flash's own timeout must be cancellable if work arrives.
  let flashTimer: number | null = null;

  function clearFlash(): void {
    if (flashTimer !== null) {
      window.clearTimeout(flashTimer);
      flashTimer = null;
    }
  }

  close.addEventListener("click", () => {
    clearFlash();
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
    // Icon-only: name it for AT (the SVG is decorative / aria-hidden).
    b.setAttribute("aria-label", title);
    b.innerHTML = svg;
    b.addEventListener("click", on);
    return b;
  }

  // Brief pressed/active flash on Reveal so a slow or backgrounded launch is
  // acknowledged even as the panel hides on blur. Applied synchronously on
  // activate (inline styles, no stylesheet dependency) and reverted after a beat;
  // success stays silent (a toast on every reveal would be too noisy).
  function pressFeedback(btn: HTMLElement): void {
    btn.classList.add("is-pressed");
    const prevColor = btn.style.color;
    const prevBorder = btn.style.borderColor;
    const prevTransform = btn.style.transform;
    btn.style.color = "var(--accent, rgba(124, 92, 252, 1))";
    btn.style.borderColor = "rgba(124, 92, 252, 0.7)";
    btn.style.transform = "scale(0.9)";
    window.setTimeout(() => {
      btn.classList.remove("is-pressed");
      btn.style.color = prevColor;
      btn.style.borderColor = prevBorder;
      btn.style.transform = prevTransform;
    }, 220);
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
      tag.textContent = j.phase === "queued" ? t("drawer.queued") : `${pct}%`;
      const bar = document.createElement("div");
      bar.className = "drawer-bar";
      const fill = document.createElement("i");
      fill.style.width = `${pct}%`;
      bar.append(fill);
      meta.append(bar);
      r.append(meta);
    } else if (j.phase === "done") {
      tag.className = "drawer-tag ok";
      tag.textContent =
        j.outputBytes != null
          ? t("drawer.doneSize", { size: formatBytes(j.outputBytes) })
          : t("drawer.done");
      r.append(meta);
      if (j.outputPath) {
        const out = j.outputPath;
        const revealBtn = actionBtn(REVEAL, t("drawer.reveal"), () => {
          pressFeedback(revealBtn);
          reveal(out).catch((e) => showToast(friendlyError(e), "error"));
        });
        r.append(
          actionBtn(COPY, t("drawer.copyFile"), () =>
            copyFile(out)
              .then(() => showToast(t("drawer.copiedToClipboard"), "success"))
              .catch((e) => showToast(friendlyError(e), "error")),
          ),
          revealBtn,
        );
      }
    } else {
      tag.className = "drawer-tag fail";
      tag.textContent = t("drawer.failed");
      r.append(meta);
    }
    return r;
  }

  /** A folded "N queued" / "+N more" line (not tied to a single job). */
  function summaryRow(text: string): HTMLElement {
    const r = document.createElement("div");
    r.className = "drawer-row drawer-summary";
    const meta = document.createElement("div");
    meta.className = "drawer-meta";
    const name = document.createElement("div");
    name.className = "drawer-name";
    name.textContent = text;
    meta.append(name);
    r.append(meta);
    return r;
  }

  function publishHeight(): void {
    // Publish the drawer's height (now that the rows are in the DOM) so the toast
    // can offset itself above the drawer instead of overlapping it (styles.css
    // reads --drawer-h on .panel via calc()).
    // manual: start a conversion (drawer visible) then trigger a toast (the
    // drawer's own Copy button, or a drop-error) → the toast sits just above the
    // drawer and neither occludes the other; with no drawer it sits at bottom:12px.
    panel.style.setProperty("--drawer-h", `${el.offsetHeight}px`);
  }

  function render(): void {
    // A "Cancelled" flash owns the drawer until its timer clears it; ignore
    // re-renders that would otherwise wipe the flash while it's showing.
    if (flashTimer !== null) return;

    if (jobs.size === 0) {
      el.hidden = true;
      rowsEl.innerHTML = "";
      // No drawer occupying the bottom slot: the toast sits at its normal spot.
      panel.style.setProperty("--drawer-h", "0px");
      return;
    }
    el.hidden = false;
    // manual: drop many files → a single "N queued" summary row plus the
    // capped individual rows and a "+N more" line, instead of an unbounded
    // scroll covering the list. (planDrawer's logic is covered in drawer.test.ts.)
    const plan = planDrawer(jobs.values());
    title.textContent = plan.title;
    rowsEl.innerHTML = "";
    for (const j of plan.rows) rowsEl.append(rowFor(j));
    if (plan.queuedSummary > 0) {
      rowsEl.append(
        summaryRow(t("drawer.queuedSummary", { count: plan.queuedSummary })),
      );
    }
    if (plan.overflow > 0) {
      rowsEl.append(summaryRow(t("drawer.more", { count: plan.overflow })));
    }
    publishHeight();
  }

  // manual: cancel the only running job → the drawer shows "Cancelled" for ~1.2s
  // then clears (not an instant disappearance); enqueueing during the flash
  // replaces it immediately with the new row, and it never resurrects an old one.
  /** Show a transient "Cancelled" state, then clear the (now empty) drawer. */
  function flashCancelled(): void {
    clearFlash();
    el.hidden = false;
    title.textContent = t("drawer.cancelled");
    rowsEl.innerHTML = "";
    publishHeight();
    flashTimer = window.setTimeout(() => {
      flashTimer = null;
      render();
    }, CANCELLED_FLASH_MS);
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
        // If that emptied the drawer, flash "Cancelled" before it vanishes so
        // the user sees their cancel landed (not a crash); otherwise re-render.
        if (jobs.size === 0 && !el.hidden) {
          flashCancelled();
        } else {
          render();
        }
        return;
      }
      // Real work arrived — a lingering flash must yield to it immediately.
      clearFlash();
      jobs.set(state.id, state);
      if (state.phase === "done" || state.phase === "failed") {
        scheduleExpiry(state.id);
      }
      render();
    },
  };
}
