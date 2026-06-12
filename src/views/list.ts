import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  cancelJob,
  convertFileSrc,
  copyFile,
  enqueue,
  ensurePreview,
  listRecents,
  queueState,
  reveal,
  type JobState,
  type Phase,
  type RecentVideo,
  type Settings,
} from "../lib/ipc";
import {
  formatBytes,
  formatDuration,
  formatPercentSmaller,
  formatRelativeTime,
  truncateMiddle,
} from "../lib/format";
import { stripOutputSuffix } from "../lib/naming";
import { showToast } from "../lib/toast";
import { openCustomModal, type CustomModal } from "./custom";

const RUNNING = new Set<Phase>(["pass1", "pass2", "verifying"]);
const TERMINAL = new Set<Phase>(["done", "failed", "cancelled"]);

const PHASE_LABELS: Partial<Record<Phase, string>> = {
  pass1: "Pass 1",
  pass2: "Pass 2",
  verifying: "Verifying",
};

const TRASH_GUARD_HINT =
  "'Move original to Trash' is on, so the original disappears after the " +
  "first conversion — only one preset per video. Turn the toggle off in " +
  "Preferences to export several formats.";

function isBusy(j: JobState | undefined): boolean {
  return !!j && (j.phase === "queued" || RUNNING.has(j.phase));
}

export function isTerminal(phase: Phase): boolean {
  return TERMINAL.has(phase);
}

/** Identity of the visible video list; job statuses are painted separately. */
export function videoListSignature(videos: RecentVideo[]): string {
  return JSON.stringify(
    videos.map((v) => [
      v.path,
      v.sizeBytes,
      v.createdMs,
      v.thumbPath,
      v.isOutput,
      v.conversion,
      v.durationSecs,
    ]),
  );
}

export interface ListView {
  el: HTMLElement;
  refresh(): Promise<void>;
  updateJob(state: JobState): void;
  onSettingsChanged(): void;
  /** Put the cursor in the filter input (panel shown / tab switched). */
  focusFilter(): void;
}

export function createListView(getSettings: () => Settings | null): ListView {
  const el = document.createElement("div");
  el.className = "view view-videos";

  const filterRow = document.createElement("div");
  filterRow.className = "filter-row";
  const filterInput = document.createElement("input");
  filterInput.type = "text";
  filterInput.className = "input filter-input";
  filterInput.placeholder = "Filter recordings…";
  filterRow.appendChild(filterInput);

  const listScroll = document.createElement("div");
  listScroll.className = "list-scroll";
  el.append(filterRow, listScroll);

  let videos: RecentVideo[] = [];
  const jobs = new Map<string, JobState>(); // job id -> latest state
  const jobByPath = new Map<string, string>(); // input path -> job id shown on the row
  const dismissed = new Set<string>(); // job ids no longer shown on rows
  const dismissTimers = new Map<string, number>();
  const rowByPath = new Map<string, HTMLElement>();
  const rowCleanups = new Map<string, () => void>(); // path -> preview teardown
  const postErrorToasted = new Set<string>(); // job ids whose postError was toasted
  const previewPaths = new Map<string, string>(); // video path -> resolved proxy path
  let previewSeq = 0; // staleness token for in-flight proxy resolutions
  let lastSignature: string | null = null;
  let noMatch: HTMLElement | null = null; // "no filter matches" note
  let selectedPath: string | null = null; // keyboard selection (violet ring)
  let expandedPath: string | null = null; // at most one row is expanded
  let modal: CustomModal | null = null; // open custom-conversion page

  function jobForPath(path: string): JobState | undefined {
    const id = jobByPath.get(path);
    return id ? jobs.get(id) : undefined;
  }

  /** Jobs that already produced (or will produce) an output for this input. */
  function survivingJobs(path: string): JobState[] {
    const out: JobState[] = [];
    for (const j of jobs.values()) {
      if (
        j.inputPath === path &&
        j.phase !== "failed" &&
        j.phase !== "cancelled"
      ) {
        out.push(j);
      }
    }
    return out;
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
        // Same videos: repaint statuses in place so an open preview (and its
        // playing <video>) survives routine panel:shown refreshes.
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

  // ----- filtering & selection ------------------------------------------

  function matchesFilter(v: RecentVideo): boolean {
    const q = filterInput.value.trim().toLowerCase();
    return !q || v.name.toLowerCase().includes(q);
  }

  function visibleVideos(): RecentVideo[] {
    return videos.filter(matchesFilter);
  }

  function setSelected(path: string | null): void {
    if (selectedPath && selectedPath !== path) {
      rowByPath.get(selectedPath)?.classList.remove("is-selected");
    }
    selectedPath = path;
    if (!path) return;
    const row = rowByPath.get(path);
    if (!row) {
      selectedPath = null;
      return;
    }
    row.classList.add("is-selected");
    row.scrollIntoView({ block: "nearest" });
  }

  function applyFilter(resetSelection: boolean): void {
    if (resetSelection) setSelected(null);
    let any = false;
    for (const v of videos) {
      const row = rowByPath.get(v.path);
      if (!row) continue;
      const show = matchesFilter(v);
      row.hidden = !show;
      if (show) any = true;
      if (!show && expandedPath === v.path) collapseExpanded();
    }
    if (noMatch) noMatch.hidden = any || videos.length === 0;
    if (selectedPath) setSelected(selectedPath); // re-ring after a rebuild
  }

  function focusFilter(): void {
    filterInput.focus();
    filterInput.select();
  }

  filterInput.addEventListener("input", () => applyFilter(true));

  function moveSelection(dir: 1 | -1, fromFilter: boolean): void {
    const vis = visibleVideos();
    if (vis.length === 0) return;
    if (fromFilter) {
      if (dir !== 1) return;
      filterInput.blur(); // moving into the list takes focus off the filter
      setSelected(vis[0].path);
      return;
    }
    const idx = selectedPath ? vis.findIndex((v) => v.path === selectedPath) : -1;
    if (idx < 0) {
      if (dir === 1) setSelected(vis[0].path);
      return;
    }
    if (dir === -1 && idx === 0) {
      // Off the top of the list: hand focus back to the filter.
      setSelected(null);
      focusFilter();
      return;
    }
    const next = Math.min(vis.length - 1, Math.max(0, idx + dir));
    setSelected(vis[next].path);
  }

  function onEscape(): void {
    if (modal) {
      modal.close();
      return;
    }
    if (expandedPath) {
      collapseExpanded();
      return;
    }
    if (filterInput.value !== "") {
      filterInput.value = "";
      applyFilter(true);
      focusFilter();
      return;
    }
    getCurrentWindow()
      .hide()
      .catch((e) => showToast(String(e)));
  }

  function isEditable(t: EventTarget | null): boolean {
    return (
      t instanceof HTMLElement &&
      (t.tagName === "INPUT" || t.tagName === "SELECT" || t.tagName === "TEXTAREA")
    );
  }

  function onKeyDown(e: KeyboardEvent): void {
    if (modal) {
      // The custom page handles its own form; only Esc-to-go-back is global.
      if (e.key === "Escape") {
        e.preventDefault();
        modal.close();
      }
      return;
    }
    if (el.hidden) return; // preferences tab
    const inFilter = e.target === filterInput;
    if (!inFilter && isEditable(e.target)) return;

    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        moveSelection(1, inFilter);
        return;
      case "ArrowUp":
        e.preventDefault();
        moveSelection(-1, inFilter);
        return;
      case "Escape":
        e.preventDefault();
        onEscape();
        return;
    }
    if (inFilter) return; // everything else types into the filter natively

    const selected = selectedPath
      ? videos.find((v) => v.path === selectedPath)
      : undefined;
    if (selected) {
      if (e.key === "Enter" || e.key === "d") {
        e.preventDefault();
        onRowClick(selected);
        return;
      }
      if (e.key === "e") {
        e.preventDefault();
        toggleExpand(selected);
        return;
      }
    }
    // Any other printable character goes to the filter.
    if (e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault();
      filterInput.focus();
      filterInput.value += e.key;
      applyFilter(true);
    }
  }

  document.addEventListener("keydown", onKeyDown);

  // ----- rendering --------------------------------------------------------

  function render(): void {
    // Tear down old rows first: stop preview videos before their elements
    // are dropped, so decoders don't leak.
    for (const cleanup of rowCleanups.values()) cleanup();
    rowCleanups.clear();
    rowByPath.clear();
    expandedPath = null;
    noMatch = null;
    listScroll.innerHTML = "";
    if (videos.length === 0) {
      const empty = document.createElement("div");
      empty.className = "empty";
      empty.textContent =
        "No videos in your watched folders yet — record something!";
      listScroll.appendChild(empty);
      setSelected(null);
      return;
    }
    for (const v of videos) listScroll.appendChild(buildRow(v));
    noMatch = document.createElement("div");
    noMatch.className = "empty";
    noMatch.textContent = "No recordings match your filter.";
    noMatch.hidden = true;
    listScroll.appendChild(noMatch);
    applyFilter(false);
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
          presetHash: "",
          phase: "queued",
          progress: 0,
          inputBytes: v.sizeBytes,
          outputBytes: null,
          reused: false,
          part: null,
          error: null,
          postError: null,
        });
      }
    } catch (e) {
      showToast(String(e));
    }
  }

  function onRowClick(v: RecentVideo): void {
    if (v.isOutput) {
      // Orphaned output: the original is gone, so a click just copies the file.
      copyFile(v.path)
        .then(() => showToast("Copied to clipboard"))
        .catch((e) => showToast(String(e)));
      return;
    }
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

  function openCustom(v: RecentVideo): void {
    if (modal) return;
    const host = document.querySelector<HTMLElement>(".panel") ?? document.body;
    modal = openCustomModal({
      host,
      video: v,
      onStarted: (id) => {
        if (!jobs.has(id)) {
          updateJob({
            id,
            inputPath: v.path,
            inputName: v.name,
            outputPath: null,
            presetId: "custom",
            presetHash: "",
            phase: "queued",
            progress: 0,
            inputBytes: v.sizeBytes,
            outputBytes: null,
            reused: false,
            part: null,
            error: null,
            postError: null,
          });
        }
      },
      onClose: () => {
        modal = null;
        focusFilter();
      },
    });
  }

  function proxyVideo(proxyPath: string): HTMLVideoElement {
    const video = document.createElement("video");
    video.muted = true;
    video.autoplay = true;
    video.loop = true;
    video.playsInline = true;
    video.setAttribute("playsinline", "");
    // The montage proxy is already condensed; play it at normal speed.
    video.src = convertFileSrc(proxyPath);
    return video;
  }

  /** Disable preset chips / Custom per the trash-original one-preset guard. */
  function refreshGuards(row: HTMLElement, v: RecentVideo): void {
    const buttons = row.querySelectorAll<HTMLButtonElement>(
      ".row-expand [data-guard-preset]",
    );
    if (buttons.length === 0) return;
    const settings = getSettings();
    const guardJobs = settings?.trashOriginal ? survivingJobs(v.path) : [];
    const allowed = new Set(guardJobs.map((j) => j.presetId));
    for (const btn of buttons) {
      const blocked =
        guardJobs.length > 0 && !allowed.has(btn.dataset.guardPreset ?? "");
      btn.disabled = blocked;
      btn.title = blocked ? TRASH_GUARD_HINT : btn.dataset.baseTitle ?? "";
    }
  }

  function collapseExpanded(): void {
    if (!expandedPath) return;
    const row = rowByPath.get(expandedPath);
    expandedPath = null;
    if (!row) return;
    const expand = row.querySelector<HTMLElement>(".row-expand");
    if (expand) collapseRow(row, expand);
  }

  function toggleExpand(v: RecentVideo): void {
    const row = rowByPath.get(v.path);
    const expand = row?.querySelector<HTMLElement>(".row-expand");
    if (!row || !expand) return;
    if (expandedPath === v.path) {
      collapseExpanded();
      return;
    }
    collapseExpanded();
    expandRow(row, expand, v);
    if (row.classList.contains("is-expanded")) expandedPath = v.path;
  }

  function expandRow(row: HTMLElement, expand: HTMLElement, v: RecentVideo): void {
    if (row.classList.contains("is-expanded")) return;
    const settings = getSettings();
    if (!settings) return;

    expand.innerHTML = "";
    const token = String(++previewSeq);
    expand.dataset.previewToken = token;

    const stage = document.createElement("div");
    stage.className = "preview-stage";

    const cached = previewPaths.get(v.path);
    if (cached) {
      stage.appendChild(proxyVideo(cached));
    } else {
      // Show the enlarged thumbnail with a shimmer while the proxy is built.
      if (v.thumbPath) {
        const img = document.createElement("img");
        img.className = "preview-thumb";
        img.src = convertFileSrc(v.thumbPath);
        img.alt = "";
        img.draggable = false;
        stage.appendChild(img);
      } else {
        const ph = document.createElement("div");
        ph.className = "preview-thumb preview-thumb-placeholder";
        stage.appendChild(ph);
      }
      const loading = document.createElement("div");
      loading.className = "preview-loading";
      loading.textContent = "Preparing preview…";
      stage.appendChild(loading);

      ensurePreview(v.path)
        .then((proxyPath) => {
          previewPaths.set(v.path, proxyPath);
          // Guard staleness: the row may have collapsed or re-expanded since.
          if (expand.dataset.previewToken !== token) return;
          if (!row.classList.contains("is-expanded")) return;
          stage.replaceChildren(proxyVideo(proxyPath));
        })
        .catch(() => {
          if (expand.dataset.previewToken !== token) return;
          loading.classList.add("is-failed");
          loading.textContent = "Preview unavailable";
        });
    }

    expand.appendChild(stage);

    if (!v.isOutput) {
      const chips = document.createElement("div");
      chips.className = "chips";
      if (settings.presets.length >= 2) {
        for (const p of settings.presets) {
          const chip = document.createElement("button");
          chip.type = "button";
          chip.className =
            "chip" + (p.id === settings.defaultPresetId ? " chip-default" : "");
          chip.textContent = p.name;
          chip.dataset.guardPreset = p.id;
          chip.dataset.baseTitle = `Compress with ${p.name}`;
          chip.title = chip.dataset.baseTitle;
          chip.addEventListener("click", (e) => {
            e.stopPropagation();
            collapseExpanded();
            void doEnqueue(v, p.id);
          });
          chips.appendChild(chip);
        }
      }
      const custom = document.createElement("button");
      custom.type = "button";
      custom.className = "chip chip-custom";
      custom.textContent = "Custom…";
      custom.dataset.guardPreset = "custom";
      custom.dataset.baseTitle = "One-off conversion with custom settings";
      custom.title = custom.dataset.baseTitle;
      custom.addEventListener("click", (e) => {
        e.stopPropagation();
        openCustom(v);
      });
      chips.appendChild(custom);
      expand.appendChild(chips);
    }

    row.classList.add("is-expanded");
    refreshGuards(row, v);
  }

  function collapseRow(row: HTMLElement, expand: HTMLElement): void {
    if (!row.classList.contains("is-expanded")) return;
    delete expand.dataset.previewToken;
    const video = expand.querySelector("video");
    if (video) {
      video.pause();
      video.removeAttribute("src");
      video.load();
    }
    expand.innerHTML = "";
    row.classList.remove("is-expanded");
  }

  /** Small magnifier button that reveals the file in Finder. */
  function buildRevealButton(path: string): HTMLButtonElement {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "row-reveal";
    btn.title = "Reveal in Finder";
    btn.tabIndex = -1; // arrow-key selection owns keyboard focus
    btn.innerHTML =
      `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" ` +
      `stroke-width="1.5" stroke-linecap="round" aria-hidden="true">` +
      `<circle cx="7" cy="7" r="4.25"/><path d="M10.3 10.3 13.5 13.5"/></svg>`;
    btn.addEventListener("click", (e) => {
      e.stopPropagation();
      reveal(path).catch((err) => showToast(String(err)));
    });
    return btn;
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
    name.title = v.name;
    if (v.isOutput) {
      // Orphaned output: show the original's stem with a "compressed" badge.
      const text = document.createElement("span");
      text.textContent = truncateMiddle(stripOutputSuffix(v.name), 28);
      const badge = document.createElement("span");
      badge.className = "badge-compressed";
      badge.textContent = "compressed";
      if (v.conversion?.presetName) badge.title = v.conversion.presetName;
      name.append(text, badge);
    } else {
      const text = document.createElement("span");
      text.textContent = truncateMiddle(v.name, 36);
      name.appendChild(text);
    }
    name.appendChild(buildRevealButton(v.path));

    const sizeText =
      v.isOutput && v.conversion?.originalBytes != null
        ? `${formatBytes(v.conversion.originalBytes)} → ${formatBytes(v.conversion.outputBytes)}`
        : formatBytes(v.sizeBytes);

    const lengthText =
      v.durationSecs != null ? formatDuration(v.durationSecs) : "—";

    const meta = document.createElement("div");
    meta.className = "row-meta";
    meta.innerHTML =
      `<div class="meta"><span class="meta-label">Size</span>` +
      `<span class="meta-value">${sizeText}</span></div>` +
      `<div class="meta"><span class="meta-label">Length</span>` +
      `<span class="meta-value">${lengthText}</span></div>` +
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

    const chevron = document.createElement("button");
    chevron.type = "button";
    chevron.className = "row-chevron";
    chevron.title = "Details (e)";
    chevron.textContent = "›";
    chevron.addEventListener("click", (e) => {
      e.stopPropagation();
      toggleExpand(v);
    });

    main.append(info, cancelBtn, chevron);
    main.addEventListener("click", () => onRowClick(v));

    const progress = document.createElement("div");
    progress.className = "row-progress";
    progress.innerHTML = `<div class="bar"></div>`;

    const expand = document.createElement("div");
    expand.className = "row-expand";

    row.append(main, progress, expand);

    rowCleanups.set(v.path, () => {
      if (expandedPath === v.path) expandedPath = null;
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

    // Job phases shift the trash-original guard for an expanded row's chips.
    refreshGuards(row, v);

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
        const partLabel = j.part ? ` · Part ${j.part[0]}/${j.part[1]}` : "";
        status.innerHTML =
          `<span class="status-pct">${pct}%</span>` +
          `<span class="status-dim">${PHASE_LABELS[j.phase] ?? ""}${partLabel}</span>`;
        bar.style.width = `${pct}%`;
        break;
      }
      case "done": {
        const outB = j.outputBytes ?? 0;
        // Split jobs produced n part files; outputBytes is their summed size.
        const parts = j.part && j.part[1] > 1 ? j.part[1] : null;
        const outText = parts
          ? `${parts} parts · ${formatBytes(outB)}`
          : formatBytes(outB);
        let html = `<span class="done-stat">${formatBytes(j.inputBytes)} → ${outText}</span>`;
        if (j.reused) {
          html += `<span class="done-smaller">Already compressed — reused</span>`;
        } else {
          // No above-target note: the backend guarantees a Done job's output
          // is at or under its preset's byte target.
          html += `<span class="done-smaller">${formatPercentSmaller(j.inputBytes, outB)}</span>`;
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
        if (errEl) {
          // The clamp hides anything past three lines; hover shows it all.
          const msg = j.error ?? "Encoding failed";
          errEl.textContent = msg;
          errEl.title = msg;
        }
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
    // Presets drive the chips; collapse previews and repaint statuses.
    collapseExpanded();
    for (const [path, row] of rowByPath) {
      const expand = row.querySelector<HTMLElement>(".row-expand");
      if (expand) collapseRow(row, expand);
      const v = videos.find((x) => x.path === path);
      if (v) renderStatusIn(row, v);
    }
  }

  return { el, refresh, updateJob, onSettingsChanged, focusFilter };
}
