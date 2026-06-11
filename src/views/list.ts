import {
  cancelJob,
  convertFileSrc,
  enqueue,
  listRecents,
  queueState,
  type JobState,
  type Phase,
  type RecentVideo,
  type Settings,
} from "../lib/ipc";
import {
  formatBytes,
  formatPercentSmaller,
  formatRelativeTime,
  truncateMiddle,
} from "../lib/format";
import { showToast } from "../lib/toast";

const RUNNING = new Set<Phase>(["pass1", "pass2", "verifying"]);
const TERMINAL = new Set<Phase>(["done", "failed", "cancelled"]);

const PHASE_LABELS: Partial<Record<Phase, string>> = {
  pass1: "Pass 1",
  pass2: "Pass 2",
  verifying: "Verifying",
};

function isBusy(j: JobState | undefined): boolean {
  return !!j && (j.phase === "queued" || RUNNING.has(j.phase));
}

export function isTerminal(phase: Phase): boolean {
  return TERMINAL.has(phase);
}

/** Identity of the visible video list; job statuses are painted separately. */
export function videoListSignature(videos: RecentVideo[]): string {
  return JSON.stringify(
    videos.map((v) => [v.path, v.sizeBytes, v.createdMs, v.thumbPath]),
  );
}

export interface ListView {
  el: HTMLElement;
  refresh(): Promise<void>;
  updateJob(state: JobState): void;
  onSettingsChanged(): void;
}

export function createListView(getSettings: () => Settings | null): ListView {
  const el = document.createElement("div");
  el.className = "view view-videos";

  let videos: RecentVideo[] = [];
  const jobs = new Map<string, JobState>(); // job id -> latest state
  const jobByPath = new Map<string, string>(); // input path -> job id shown on the row
  const dismissed = new Set<string>(); // job ids no longer shown on rows
  const dismissTimers = new Map<string, number>();
  const rowByPath = new Map<string, HTMLElement>();
  const rowCleanups = new Map<string, () => void>(); // path -> hover/preview teardown
  const postErrorToasted = new Set<string>(); // job ids whose postError was toasted
  let lastSignature: string | null = null;

  function jobForPath(path: string): JobState | undefined {
    const id = jobByPath.get(path);
    return id ? jobs.get(id) : undefined;
  }

  function dismiss(job: JobState): void {
    dismissed.add(job.id);
    const t = dismissTimers.get(job.id);
    if (t !== undefined) {
      window.clearTimeout(t);
      dismissTimers.delete(job.id);
    }
    if (jobByPath.get(job.inputPath) === job.id) jobByPath.delete(job.inputPath);
    renderStatus(job.inputPath);
  }

  function scheduleDismiss(job: JobState, delay: number): void {
    if (dismissTimers.has(job.id) || dismissed.has(job.id)) return;
    const t = window.setTimeout(() => {
      dismissTimers.delete(job.id);
      dismiss(job);
    }, delay);
    dismissTimers.set(job.id, t);
  }

  function noticePostError(state: JobState): void {
    if (state.phase !== "done" || !state.postError) return;
    if (postErrorToasted.has(state.id)) return;
    postErrorToasted.add(state.id);
    showToast(state.postError);
  }

  function updateJob(state: JobState): void {
    jobs.set(state.id, state);
    noticePostError(state);
    if (dismissed.has(state.id)) return;
    jobByPath.set(state.inputPath, state.id);
    if (state.phase === "done") scheduleDismiss(state, 6000);
    else if (state.phase === "cancelled") scheduleDismiss(state, 1200);
    renderStatus(state.inputPath);
  }

  function applySnapshot(states: JobState[]): void {
    for (const s of states) {
      const prev = jobs.get(s.id);
      // Monotonic per job id: a stale snapshot must never roll a terminal
      // state (done/failed/cancelled) back to a non-terminal one.
      if (prev && isTerminal(prev.phase) && !isTerminal(s.phase)) continue;
      jobs.set(s.id, s);
      noticePostError(s);
      if (dismissed.has(s.id)) continue;
      if (s.phase === "cancelled") {
        dismissed.add(s.id);
        continue;
      }
      const mappedId = jobByPath.get(s.inputPath);
      const mapped = mappedId ? jobs.get(mappedId) : undefined;
      // Never let a finished job replace a still-busy one on the same row.
      if (mapped && mapped.id !== s.id && isBusy(mapped) && !isBusy(s)) continue;
      jobByPath.set(s.inputPath, s.id);
      if (s.phase === "done") scheduleDismiss(s, 6000);
    }
  }

  async function refresh(): Promise<void> {
    try {
      const [vids, queue] = await Promise.all([listRecents(), queueState()]);
      videos = vids;
      applySnapshot(queue);
      const sig = videoListSignature(vids);
      if (sig === lastSignature) {
        // Same videos: repaint statuses in place so an open hover preview
        // (and its playing <video>) survives routine panel:shown refreshes.
        for (const v of videos) {
          const row = rowByPath.get(v.path);
          if (row) renderStatusIn(row, v);
        }
        return;
      }
      lastSignature = sig;
      render();
    } catch (e) {
      showToast(String(e));
    }
  }

  function render(): void {
    // Tear down old rows first: stop preview videos and pending hover timers
    // before their elements are dropped, so decoders don't leak.
    for (const cleanup of rowCleanups.values()) cleanup();
    rowCleanups.clear();
    rowByPath.clear();
    el.innerHTML = "";
    if (videos.length === 0) {
      const empty = document.createElement("div");
      empty.className = "empty";
      empty.textContent =
        "No videos in your watched folders yet — record something!";
      el.appendChild(empty);
      return;
    }
    for (const v of videos) el.appendChild(buildRow(v));
  }

  async function doEnqueue(v: RecentVideo, presetId: string): Promise<void> {
    try {
      const id = await enqueue(v.path, presetId);
      // Optimistic queued state in case the backend's first event hasn't landed yet.
      if (!jobs.has(id)) {
        updateJob({
          id,
          inputPath: v.path,
          inputName: v.name,
          outputPath: null,
          presetId,
          phase: "queued",
          progress: 0,
          inputBytes: v.sizeBytes,
          outputBytes: null,
          error: null,
          postError: null,
        });
      }
    } catch (e) {
      showToast(String(e));
    }
  }

  function onRowClick(v: RecentVideo): void {
    const j = jobForPath(v.path);
    if (j) {
      if (isBusy(j)) return;
      if (j.phase === "failed") {
        dismiss(j);
        return;
      }
      dismiss(j);
    }
    const settings = getSettings();
    if (!settings) return;
    void doEnqueue(v, settings.defaultPresetId);
  }

  function expandRow(row: HTMLElement, expand: HTMLElement, v: RecentVideo): void {
    if (row.classList.contains("is-expanded")) return;
    const settings = getSettings();
    if (!settings) return;

    expand.innerHTML = "";

    const video = document.createElement("video");
    video.muted = true;
    video.autoplay = true;
    video.loop = true;
    video.playsInline = true;
    video.setAttribute("playsinline", "");
    video.addEventListener("loadedmetadata", () => {
      video.playbackRate = 2;
    });
    video.src = convertFileSrc(v.path);

    const chips = document.createElement("div");
    chips.className = "chips";
    for (const p of settings.presets) {
      const chip = document.createElement("button");
      chip.type = "button";
      chip.className =
        "chip" + (p.id === settings.defaultPresetId ? " chip-default" : "");
      chip.textContent = p.name;
      chip.title = `Compress with ${p.name}`;
      chip.addEventListener("click", (e) => {
        e.stopPropagation();
        collapseRow(row, expand);
        void doEnqueue(v, p.id);
      });
      chips.appendChild(chip);
    }

    expand.append(video, chips);
    row.classList.add("is-expanded");
  }

  function collapseRow(row: HTMLElement, expand: HTMLElement): void {
    if (!row.classList.contains("is-expanded")) return;
    const video = expand.querySelector("video");
    if (video) {
      video.pause();
      video.removeAttribute("src");
      video.load();
    }
    expand.innerHTML = "";
    row.classList.remove("is-expanded");
  }

  function buildRow(v: RecentVideo): HTMLElement {
    const row = document.createElement("div");
    row.className = "row";
    rowByPath.set(v.path, row);

    const main = document.createElement("div");
    main.className = "row-main";

    if (v.thumbPath) {
      const img = document.createElement("img");
      img.className = "thumb";
      img.src = convertFileSrc(v.thumbPath);
      img.alt = "";
      img.draggable = false;
      main.appendChild(img);
    } else {
      const ph = document.createElement("div");
      ph.className = "thumb thumb-placeholder";
      main.appendChild(ph);
    }

    const info = document.createElement("div");
    info.className = "row-info";

    const name = document.createElement("div");
    name.className = "row-name";
    name.textContent = truncateMiddle(v.name, 36);
    name.title = v.name;

    const meta = document.createElement("div");
    meta.className = "row-meta";
    meta.innerHTML =
      `<div class="meta"><span class="meta-label">Size</span>` +
      `<span class="meta-value">${formatBytes(v.sizeBytes)}</span></div>` +
      `<div class="meta"><span class="meta-label">Recorded</span>` +
      `<span class="meta-value">${formatRelativeTime(v.createdMs)}</span></div>`;

    const status = document.createElement("div");
    status.className = "row-status";

    info.append(name, meta, status);

    const cancelBtn = document.createElement("button");
    cancelBtn.type = "button";
    cancelBtn.className = "row-cancel";
    cancelBtn.title = "Cancel";
    cancelBtn.textContent = "✕";
    cancelBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      const j = jobForPath(v.path);
      if (j && isBusy(j)) cancelJob(j.id).catch((err) => showToast(String(err)));
    });

    main.append(info, cancelBtn);
    main.addEventListener("click", () => onRowClick(v));

    const progress = document.createElement("div");
    progress.className = "row-progress";
    progress.innerHTML = `<div class="bar"></div>`;

    const expand = document.createElement("div");
    expand.className = "row-expand";

    row.append(main, progress, expand);

    let hoverTimer: number | undefined;
    row.addEventListener("mouseenter", () => {
      const settings = getSettings();
      if (!settings || settings.presets.length < 2) return;
      if (isBusy(jobForPath(v.path))) return;
      hoverTimer = window.setTimeout(() => expandRow(row, expand, v), 250);
    });
    row.addEventListener("mouseleave", () => {
      window.clearTimeout(hoverTimer);
      collapseRow(row, expand);
    });
    rowCleanups.set(v.path, () => {
      window.clearTimeout(hoverTimer);
      collapseRow(row, expand);
    });

    renderStatusIn(row, v);
    return row;
  }

  function renderStatus(path: string): void {
    const row = rowByPath.get(path);
    if (!row) return;
    const v = videos.find((x) => x.path === path);
    if (v) renderStatusIn(row, v);
  }

  function renderStatusIn(row: HTMLElement, v: RecentVideo): void {
    const status = row.querySelector<HTMLElement>(".row-status");
    const bar = row.querySelector<HTMLElement>(".row-progress .bar");
    if (!status || !bar) return;
    const j = jobForPath(v.path);

    row.classList.toggle("is-queued", j?.phase === "queued");
    row.classList.toggle("is-active", !!j && RUNNING.has(j.phase));
    row.classList.toggle("is-done", j?.phase === "done");
    row.classList.toggle("is-failed", j?.phase === "failed");

    if (!j) {
      status.innerHTML = "";
      bar.style.width = "0%";
      return;
    }

    switch (j.phase) {
      case "queued":
        status.innerHTML = `<span class="status-dim">Queued</span>`;
        bar.style.width = "0%";
        break;
      case "pass1":
      case "pass2":
      case "verifying": {
        const pct = Math.min(100, Math.max(0, Math.round(j.progress * 100)));
        status.innerHTML =
          `<span class="status-pct">${pct}%</span>` +
          `<span class="status-dim">${PHASE_LABELS[j.phase] ?? ""}</span>`;
        bar.style.width = `${pct}%`;
        break;
      }
      case "done": {
        const outB = j.outputBytes ?? 0;
        let html =
          `<span class="done-stat">${formatBytes(j.inputBytes)} → ${formatBytes(outB)}</span>` +
          `<span class="done-smaller">${formatPercentSmaller(j.inputBytes, outB)}</span>`;
        const preset = getSettings()?.presets.find((p) => p.id === j.presetId);
        if (preset && outB > preset.targetMb * 1_000_000) {
          html += `<span class="above-target">above target</span>`;
        }
        if (j.postError) html += `<span class="post-warn"></span>`;
        status.innerHTML = html;
        if (j.postError) {
          const warnEl = status.querySelector<HTMLElement>(".post-warn");
          if (warnEl) warnEl.textContent = j.postError;
        }
        bar.style.width = "0%";
        break;
      }
      case "failed": {
        status.innerHTML = `<span class="row-error"></span>`;
        const errEl = status.querySelector<HTMLElement>(".row-error");
        if (errEl) errEl.textContent = j.error ?? "Encoding failed";
        bar.style.width = "0%";
        break;
      }
      case "cancelled":
        status.innerHTML = `<span class="status-dim">Cancelled</span>`;
        bar.style.width = "0%";
        break;
    }
  }

  function onSettingsChanged(): void {
    // Presets drive chips + above-target notes; collapse previews and repaint statuses.
    for (const [path, row] of rowByPath) {
      const expand = row.querySelector<HTMLElement>(".row-expand");
      if (expand) collapseRow(row, expand);
      const v = videos.find((x) => x.path === path);
      if (v) renderStatusIn(row, v);
    }
  }

  return { el, refresh, updateJob, onSettingsChanged };
}
