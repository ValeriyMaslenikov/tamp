import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  cancelJob,
  convertFileSrc,
  copyFile,
  enqueue,
  ensurePreview,
  listRecents,
  pickVideos,
  queueState,
  recentDuration,
  recentThumb,
  reveal,
  unreachableFolders,
  type JobState,
  type Phase,
  type Preset,
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
import { revealLabel } from "../lib/platform";
import { showToast } from "../lib/toast";
import { friendlyError } from "../lib/errors";
import { t } from "../i18n";
import { openCustomModal, type CustomModal } from "./custom";

const RUNNING = new Set<Phase>(["pass1", "pass2", "verifying"]);
const TERMINAL = new Set<Phase>(["done", "failed", "cancelled"]);

/** Brief pressed/active flash on a Reveal button so a slow or backgrounded
 *  launch is acknowledged even as the panel hides on blur. Applied synchronously
 *  on activate (inline styles, no stylesheet dependency) and reverted after a
 *  beat; success stays silent (a toast on every reveal would be too noisy). */
function pressFeedback(btn: HTMLElement): void {
  btn.classList.add("is-pressed");
  const prevColor = btn.style.color;
  const prevTransform = btn.style.transform;
  btn.style.color = "var(--accent, rgba(124, 92, 252, 1))";
  btn.style.transform = "scale(0.85)";
  window.setTimeout(() => {
    btn.classList.remove("is-pressed");
    btn.style.color = prevColor;
    btn.style.transform = prevTransform;
  }, 220);
}

function phaseLabel(phase: Phase): string {
  switch (phase) {
    case "pass1":
      return t("videos.status.pass1");
    case "pass2":
      return t("videos.status.pass2");
    case "verifying":
      return t("videos.status.verifying");
    default:
      return "";
  }
}

function isBusy(j: JobState | undefined): boolean {
  return !!j && (j.phase === "queued" || RUNNING.has(j.phase));
}

export function isTerminal(phase: Phase): boolean {
  return TERMINAL.has(phase);
}

/** Whether a drop/pick should open the preset chooser (vs. use the active
 *  preset). Quick-pick always chooses; active-bar uses the active preset unless
 *  Alt is held (a one-off override). */
export function shouldPickPreset(
  layout: "quick-pick" | "active-bar",
  altHeld: boolean,
): boolean {
  return layout === "quick-pick" || altHeld;
}

/** Zero-based preset index a 1–9 key selects, or null if out of range / not a
 *  digit. Used for the "press a number to convert with that preset" shortcut. */
export function presetIndexForDigit(key: string, count: number): number | null {
  const n = Number(key) - 1;
  return Number.isInteger(n) && n >= 0 && n < count ? n : null;
}

/** Clicking a failed row's retry: re-run the SAME preset the failed job used
 *  when it still exists (the row is the obvious retry target), or fall back to
 *  the preset picker when that preset has since been deleted. Returns the
 *  decision so the caller just dispatches it. `presets` is the current set;
 *  `failedPresetId` is the id the failed job ran with. */
export function failedRetryDecision(
  failedPresetId: string,
  presets: ReadonlyArray<Pick<Preset, "id">>,
): { kind: "retry"; presetId: string } | { kind: "pick" } {
  return presets.some((p) => p.id === failedPresetId)
    ? { kind: "retry", presetId: failedPresetId }
    : { kind: "pick" };
}

/** Identity of the visible video list; job statuses are painted separately.
 *  `unreachable` (the offline/permission-denied watched folders) folds in so the
 *  Videos tab re-renders when a share goes offline or comes back even if the
 *  video rows themselves are unchanged. */
export function videoListSignature(
  videos: RecentVideo[],
  unreachable: string[] = [],
): string {
  return JSON.stringify({
    videos: videos.map((v) => [
      v.path,
      v.sizeBytes,
      v.createdMs,
      v.thumbPath,
      v.isOutput,
      v.conversion,
      v.durationSecs,
    ]),
    unreachable,
  });
}

export interface ListView {
  el: HTMLElement;
  refresh(): Promise<void>;
  updateJob(state: JobState): void;
  onSettingsChanged(): void;
  /** Put the cursor in the filter input (panel shown / tab switched). */
  focusFilter(): void;
  /** Compress arbitrary paths, honoring the layout + Alt override. */
  compressPaths(paths: string[], altHeld: boolean): void;
  /** Close an open preset picker (e.g. a drop-opened one) on tab switch. */
  closeQuickPick(): void;
  /** Overlay text for the current layout / active preset. */
  currentDropHint(): string;
  /** Footer hint reflecting the hotkeys available in the current layout mode. */
  footerHint(): string;
}

export function createListView(getSettings: () => Settings | null): ListView {
  const el = document.createElement("div");
  el.className = "view view-videos";

  const filterRow = document.createElement("div");
  filterRow.className = "filter-row";
  const filterInput = document.createElement("input");
  filterInput.type = "text";
  filterInput.className = "input filter-input";
  filterInput.placeholder = t("videos.filterPlaceholder");
  filterRow.appendChild(filterInput);

  const addBtn = document.createElement("button");
  addBtn.type = "button";
  addBtn.className = "add-file-btn";
  addBtn.textContent = t("videos.addFile");
  addBtn.title = t("videos.addFileTitle");
  addBtn.addEventListener("click", async () => {
    const paths = await pickVideos();
    if (paths.length) compressPaths(paths, false);
  });
  filterRow.appendChild(addBtn);

  // Active-preset bar (videos-layout "active-bar" mode). Hidden in quick-pick
  // mode; rebuilt by updateActiveBar().
  const activeBar = document.createElement("div");
  activeBar.className = "active-bar";
  activeBar.hidden = true;

  const listScroll = document.createElement("div");
  listScroll.className = "list-scroll";
  el.append(activeBar, filterRow, listScroll);

  let videos: RecentVideo[] = [];
  // Watched folders that exist but can't be read right now (offline share /
  // permission-denied). Drives the inline banner, distinct from the empty state.
  let unreachable: string[] = [];
  const jobs = new Map<string, JobState>(); // job id -> latest state
  const jobByPath = new Map<string, string>(); // input path -> job id shown on the row
  const dismissed = new Set<string>(); // job ids no longer shown on rows
  const dismissTimers = new Map<string, number>();
  const rowByPath = new Map<string, HTMLElement>();
  const rowCleanups = new Map<string, () => void>(); // path -> preview teardown
  const postErrorToasted = new Set<string>(); // job ids whose postError was toasted
  const previewPaths = new Map<string, string>(); // video path -> resolved proxy path
  const thumbPaths = new Map<string, string>(); // video path -> resolved thumbnail path
  const durationByPath = new Map<string, number>(); // video path -> resolved duration (secs)
  const lazyLoaded = new Set<string>(); // paths whose thumb+duration load has started
  let previewSeq = 0; // staleness token for in-flight proxy resolutions
  let lastSignature: string | null = null;
  let noMatch: HTMLElement | null = null; // "no filter matches" note
  let selectedPath: string | null = null; // keyboard selection (violet ring)
  let expandedPath: string | null = null; // at most one row is expanded
  let modal: CustomModal | null = null; // open custom-conversion page
  // "active-bar" mode: the preset every click applies. Session state, reset to
  // the default preset whenever the panel opens (focusFilter).
  let activePresetId: string | null = null;
  // "quick-pick" mode: the open picker overlay (null when closed).
  let quickPick: { el: HTMLElement; paths: string[]; presets: Preset[]; index: number } | null =
    null;

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
    showToast(state.postError, "error");
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
      // A probe of an offline UNC share can be slow; tolerate its failure so a
      // single unreachable folder never blocks listing the reachable ones.
      const [vids, queue, unreach] = await Promise.all([
        listRecents(),
        queueState(),
        unreachableFolders().catch(() => [] as string[]),
      ]);
      unreachable = unreach;
      // Carry forward thumbnails/durations resolved lazily this session: the
      // backend always returns these as null (filled per row on the frontend),
      // so re-merging keeps the in-memory model — and the list signature —
      // stable across routine refreshes instead of resetting to null.
      for (const v of vids) {
        const known = durationByPath.get(v.path);
        if (known != null) v.durationSecs = known;
        const thumb = thumbPaths.get(v.path);
        if (thumb && v.thumbPath == null) v.thumbPath = thumb;
      }
      videos = vids;
      applySnapshot(queue);
      const sig = videoListSignature(vids, unreachable);
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
      showToast(friendlyError(e), "error");
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

  function focusFilterOnly(): void {
    filterInput.focus();
    filterInput.select();
  }

  /** Public entry (panel shown / tab switched to Videos): the active-bar
   *  preset resets to the default, then the filter takes focus. */
  function focusFilter(): void {
    const s = getSettings();
    if (s) {
      activePresetId = s.defaultPresetId;
      updateActiveBar();
    }
    focusFilterOnly();
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
      focusFilterOnly();
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
      focusFilterOnly();
      return;
    }
    getCurrentWindow()
      .hide()
      .catch((e) => showToast(friendlyError(e), "error"));
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
    if (quickPick) {
      // The picker captures keys: numbers apply, Enter takes the default,
      // arrows move the highlight, C goes to Custom, Esc cancels. Each handled
      // key also calls stopImmediatePropagation() so the Converted view's
      // document keydown listener (registered after this one) doesn't act on the
      // same key while the picker floats over its tab.
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopImmediatePropagation();
        closeQuickPick();
      } else if (e.key === "Enter") {
        e.preventDefault();
        e.stopImmediatePropagation();
        applyQuickPick(quickPick.index);
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        e.stopImmediatePropagation();
        moveQuick(1);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        e.stopImmediatePropagation();
        moveQuick(-1);
      } else if (e.key === "c" || e.key === "C") {
        // Custom is single-file only; ignore the shortcut for multi-path drops.
        if (quickPick.paths.length !== 1) return;
        const v = videos.find((x) => x.path === quickPick?.paths[0]);
        if (!v) return;
        e.preventDefault();
        e.stopImmediatePropagation();
        closeQuickPick();
        openCustom(v);
      } else if (e.key >= "1" && e.key <= "9") {
        const n = Number(e.key) - 1;
        if (n < quickPick.presets.length) {
          e.preventDefault();
          e.stopImmediatePropagation();
          applyQuickPick(n);
        }
      }
      return; // swallow everything else while the picker is open
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

    // Active-bar mode: [ ] and ← → cycle the active preset.
    if (layoutMode() === "active-bar") {
      if (e.key === "[" || e.key === "ArrowLeft") {
        e.preventDefault();
        cycleActive(-1);
        return;
      }
      if (e.key === "]" || e.key === "ArrowRight") {
        e.preventDefault();
        cycleActive(1);
        return;
      }
    }

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
      if (e.key >= "1" && e.key <= "9") {
        const idx = presetIndexForDigit(e.key, orderedPresets().length);
        if (idx !== null) {
          e.preventDefault();
          collapseExpanded();
          void doEnqueue(selected, orderedPresets()[idx].id);
          return;
        }
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

  // ----- lazy per-row thumbnails & durations ------------------------------

  // list_recents returns rows immediately with thumb_path/duration_secs = None
  // (a cold cache would otherwise block the panel for many seconds). Each row's
  // thumbnail and duration are fetched lazily as it scrolls into view, so the
  // panel opens instantly and media streams in (mirrors the Converted tab).
  // manual: open the panel on a cold cache — rows appear at once with a
  // placeholder thumbnail and "—" length; thumbnails and durations fill in as
  // you scroll.
  const lazyObserver =
    typeof IntersectionObserver !== "undefined"
      ? new IntersectionObserver(
          (entries, obs) => {
            for (const entry of entries) {
              if (!entry.isIntersecting) continue;
              const row = entry.target as HTMLElement;
              obs.unobserve(row);
              const path = row.dataset.path;
              if (path) void lazyLoadRow(path, row);
            }
          },
          // Start loading a little before a row reaches the viewport so media
          // is usually ready by the time it's actually visible.
          { root: listScroll, rootMargin: "200px" },
        )
      : null;

  /** Fetch and fill one row's thumbnail and duration, once. */
  async function lazyLoadRow(path: string, row: HTMLElement): Promise<void> {
    if (lazyLoaded.has(path)) return;
    lazyLoaded.add(path);

    const img = row.querySelector<HTMLImageElement>(".thumb");
    const cachedThumb = thumbPaths.get(path);
    if (img) {
      if (cachedThumb) {
        img.src = convertFileSrc(cachedThumb);
        img.classList.remove("thumb-placeholder");
      } else {
        recentThumb(path)
          .then((thumb) => {
            if (!thumb) return;
            thumbPaths.set(path, thumb);
            // The row may have been rebuilt; refetch the live img for this path.
            const live = rowByPath.get(path)?.querySelector<HTMLImageElement>(".thumb");
            if (live) {
              live.src = convertFileSrc(thumb);
              live.classList.remove("thumb-placeholder");
            }
          })
          .catch(() => {});
      }
    }

    const fillLength = (secs: number): void => {
      durationByPath.set(path, secs);
      // Cache on the video too so expand/re-render and the signature reflect it.
      const cur = videos.find((x) => x.path === path);
      if (cur) cur.durationSecs = secs;
      const lengthEl = rowByPath.get(path)?.querySelector<HTMLElement>(".meta-length");
      if (lengthEl) lengthEl.textContent = formatDuration(secs);
    };
    const cachedDuration = durationByPath.get(path);
    const v = videos.find((x) => x.path === path);
    if (cachedDuration != null) {
      fillLength(cachedDuration);
    } else if (v && v.durationSecs == null) {
      recentDuration(path)
        .then((secs) => {
          if (secs != null) fillLength(secs);
        })
        .catch(() => {});
    }
  }

  // ----- rendering --------------------------------------------------------

  /** A non-alarming inline notice for any watched folders that exist but can't
   *  be read right now (offline share / permission-denied). Returns null when
   *  every folder is reachable. Separate from the empty state so an offline
   *  share doesn't masquerade as "no recordings".
   *  manual: add an offline UNC path as a watched folder → this banner shows
   *  ("Couldn't read \\nas\… — it may be offline or you lack permission"),
   *  not the empty-state message. */
  function buildUnreachableBanner(): HTMLElement | null {
    if (unreachable.length === 0) return null;
    const banner = document.createElement("div");
    banner.className = "folder-notice";
    banner.setAttribute("role", "status");
    const title = document.createElement("div");
    title.className = "folder-notice-title";
    title.textContent =
      unreachable.length === 1
        ? t("videos.unreachable.titleOne")
        : t("videos.unreachable.titleMany", { count: unreachable.length });
    banner.appendChild(title);
    for (const folder of unreachable) {
      const line = document.createElement("div");
      line.className = "folder-notice-path";
      line.textContent = folder;
      line.title = folder;
      banner.appendChild(line);
    }
    const hint = document.createElement("div");
    hint.className = "folder-notice-hint";
    hint.textContent = t("videos.unreachable.hint");
    banner.appendChild(hint);
    return banner;
  }

  function render(): void {
    // Tear down old rows first: stop preview videos before their elements
    // are dropped, so decoders don't leak.
    for (const cleanup of rowCleanups.values()) cleanup();
    rowCleanups.clear();
    rowByPath.clear();
    lazyObserver?.disconnect();
    lazyLoaded.clear();
    expandedPath = null;
    noMatch = null;
    listScroll.innerHTML = "";
    // Unreachable watched folders get a distinct, non-alarming notice ABOVE the
    // list (and above the empty state), so an offline share reads as "couldn't
    // read that folder" rather than the misleading "no recordings".
    const banner = buildUnreachableBanner();
    if (banner) listScroll.appendChild(banner);
    if (videos.length === 0) {
      const empty = document.createElement("div");
      empty.className = "empty";
      empty.textContent = t("videos.empty");
      listScroll.appendChild(empty);
      setSelected(null);
      return;
    }
    // Every row here carries a path that round-trips back to a real file:
    // recordings with a non-UTF-8 filename are declared unsupported and skipped
    // at scan time (scanner::scan), never laundered through to_string_lossy into
    // a U+FFFD path that copy/reveal/probe would then mis-target. So a file with
    // an exotic name simply doesn't appear, rather than showing as a broken row.
    // manual: a recording with a non-UTF-8 filename is absent from the list (and
    // logged as skipped), not present as a row that fails when you act on it.
    for (const v of videos) {
      const row = buildRow(v);
      listScroll.appendChild(row);
      // Observe for lazy thumbnail/duration loading. Without an observer
      // (non-browser env / tests) load every row's media eagerly instead.
      if (lazyObserver) lazyObserver.observe(row);
      else void lazyLoadRow(v.path, row);
    }
    noMatch = document.createElement("div");
    noMatch.className = "empty";
    noMatch.textContent = t("videos.noMatch");
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
      showToast(friendlyError(e), "error");
    }
  }

  function basename(p: string): string {
    return p.split(/[\\/]/).pop() ?? p;
  }

  async function doEnqueuePath(path: string, presetId: string): Promise<void> {
    try {
      const id = await enqueue(path, presetId);
      if (!jobs.has(id)) {
        updateJob({
          id,
          inputPath: path,
          inputName: basename(path),
          outputPath: null,
          presetId,
          presetHash: "",
          phase: "queued",
          progress: 0,
          inputBytes: 0,
          outputBytes: null,
          reused: false,
          part: null,
          error: null,
          postError: null,
        });
      }
    } catch (e) {
      showToast(friendlyError(e), "error");
    }
  }

  // ----- preset selection (videos-layout modes) --------------------------

  function layoutMode(): "quick-pick" | "active-bar" {
    return getSettings()?.videosLayout ?? "quick-pick";
  }

  /** Presets with the default first, so it's option 1 / the highlighted one. */
  function orderedPresets(): Preset[] {
    const s = getSettings();
    if (!s) return [];
    const def = s.presets.find((p) => p.id === s.defaultPresetId);
    const rest = s.presets.filter((p) => p.id !== s.defaultPresetId);
    return def ? [def, ...rest] : [...s.presets];
  }

  /** The active-bar preset, falling back to default / first if unset. */
  function activePreset(): Preset | null {
    const s = getSettings();
    if (!s) return null;
    return (
      s.presets.find((p) => p.id === activePresetId) ??
      s.presets.find((p) => p.id === s.defaultPresetId) ??
      s.presets[0] ??
      null
    );
  }

  function cycleActive(dir: 1 | -1): void {
    const s = getSettings();
    if (!s || s.presets.length === 0) return;
    const cur = activePreset();
    const idx = cur ? s.presets.findIndex((p) => p.id === cur.id) : -1;
    const n = s.presets.length;
    activePresetId = s.presets[((idx < 0 ? 0 : idx) + dir + n) % n].id;
    updateActiveBar();
  }

  /** Renders the active-preset bar; shown only in "active-bar" mode. */
  function updateActiveBar(): void {
    const s = getSettings();
    const show = !!s && layoutMode() === "active-bar";
    activeBar.hidden = !show;
    activeBar.innerHTML = "";
    if (!show || !s) return;
    const ap = activePreset();

    const label = document.createElement("span");
    label.className = "active-bar-label";
    label.textContent = t("videos.activeBar.label");

    const control = document.createElement("div");
    control.className = "active-bar-control";
    const prev = document.createElement("button");
    prev.type = "button";
    prev.className = "active-bar-arrow";
    prev.textContent = "‹";
    prev.title = t("videos.activeBar.prevTitle");
    prev.setAttribute("aria-label", t("videos.activeBar.prevAria"));
    prev.tabIndex = -1;
    prev.disabled = s.presets.length < 2;
    prev.addEventListener("click", () => cycleActive(-1));
    const name = document.createElement("span");
    name.className = "active-bar-name";
    name.textContent = ap ? ap.name : t("videos.activeBar.none");
    const next = document.createElement("button");
    next.type = "button";
    next.className = "active-bar-arrow";
    next.textContent = "›";
    next.title = t("videos.activeBar.nextTitle");
    next.setAttribute("aria-label", t("videos.activeBar.nextAria"));
    next.tabIndex = -1;
    next.disabled = s.presets.length < 2;
    next.addEventListener("click", () => cycleActive(1));
    control.append(prev, name, next);

    activeBar.append(label, control);
  }

  function closeQuickPick(): void {
    if (!quickPick) return;
    quickPick.el.remove();
    quickPick = null;
    // Restore focus to the trigger context: the videos filter is the list's
    // natural focus home, regardless of whether the picker was opened by a row
    // click, the Add-file button, or a drop. Only steal focus if it's loose
    // (still on the now-removed overlay / on <body>), so we don't yank it from
    // another tab when onSettingsChanged closes a stale picker.
    if (!el.hidden) {
      const active = document.activeElement;
      if (!active || active === document.body) focusFilterOnly();
    }
  }

  function highlightQuick(): void {
    if (!quickPick) return;
    const items = quickPick.el.querySelectorAll<HTMLElement>(
      ".quickpick-item:not(.quickpick-custom)",
    );
    items.forEach((it, i) => it.classList.toggle("is-highlight", i === quickPick?.index));
  }

  function moveQuick(dir: 1 | -1): void {
    if (!quickPick) return;
    const n = quickPick.presets.length;
    quickPick.index = (quickPick.index + dir + n) % n;
    highlightQuick();
  }

  function applyQuickPick(i: number): void {
    if (!quickPick) return;
    const p = quickPick.presets[i];
    const paths = quickPick.paths;
    closeQuickPick();
    if (p) for (const path of paths) void doEnqueuePath(path, p.id);
  }

  /** Opens the preset picker (quick-pick mode) for one or more paths. The chosen
   *  preset is applied to every path. Custom (single-file only) is shown just for
   *  a single-path pick. */
  function openQuickPickForPaths(paths: string[]): void {
    closeQuickPick();
    if (paths.length === 0) return;
    const presets = orderedPresets();
    if (presets.length === 0) return;

    const overlay = document.createElement("div");
    overlay.className = "quickpick-overlay";
    overlay.addEventListener("click", (e) => {
      if (e.target === overlay) closeQuickPick();
    });
    // Tab focus trap: keep Tab/Shift+Tab cycling within the picker so it can't
    // walk behind to the covered tabs/rows. Arrows/numbers/Enter/Esc are still
    // handled by the document keydown handler (which calls
    // stopImmediatePropagation), so this only governs Tab.
    overlay.addEventListener("keydown", (e) => {
      if (e.key !== "Tab" || !quickPick) return;
      const focusable = overlay.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [tabindex]:not([tabindex="-1"])',
      );
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      const active = document.activeElement;
      if (e.shiftKey) {
        if (active === first || !overlay.contains(active)) {
          e.preventDefault();
          last.focus();
        }
      } else if (active === last || !overlay.contains(active)) {
        e.preventDefault();
        first.focus();
      }
    });

    const card = document.createElement("div");
    card.className = "quickpick";
    // Modal dialog semantics on the card: SR announces a labelled dialog on
    // open. manual: opening the picker announces "Choose a preset, dialog";
    // Tab stays inside; closing restores focus to the filter.
    card.setAttribute("role", "dialog");
    card.setAttribute("aria-modal", "true");
    card.setAttribute("aria-label", t("videos.quickPick.dialogAria"));
    const title = document.createElement("div");
    title.className = "quickpick-title";
    title.textContent =
      paths.length > 1
        ? t("videos.quickPick.titleMany", { count: paths.length })
        : t("videos.quickPick.title");
    card.appendChild(title);

    presets.forEach((p, i) => {
      const item = document.createElement("button");
      item.type = "button";
      item.className = "quickpick-item";
      item.innerHTML =
        `<span class="quickpick-num">${i < 9 ? i + 1 : ""}</span>` +
        `<span class="quickpick-name"></span>`;
      (item.querySelector(".quickpick-name") as HTMLElement).textContent = p.name;
      item.addEventListener("click", () => applyQuickPick(i));
      item.addEventListener("mousemove", () => {
        if (quickPick && quickPick.index !== i) {
          quickPick.index = i;
          highlightQuick();
        }
      });
      card.appendChild(item);
    });

    // Custom is a single-file one-off; offer it only for a single-path pick.
    const customVideo =
      paths.length === 1 ? videos.find((x) => x.path === paths[0]) : undefined;
    if (customVideo) {
      const custom = document.createElement("button");
      custom.type = "button";
      custom.className = "quickpick-item quickpick-custom";
      custom.innerHTML =
        `<span class="quickpick-num">C</span>` +
        `<span class="quickpick-name"></span>`;
      (custom.querySelector(".quickpick-name") as HTMLElement).textContent =
        t("videos.quickPick.custom");
      custom.addEventListener("click", () => {
        closeQuickPick();
        openCustom(customVideo);
      });
      card.appendChild(custom);
    }

    const hint = document.createElement("div");
    hint.className = "quickpick-hint";
    hint.textContent = t("videos.quickPick.hint");
    card.appendChild(hint);

    overlay.appendChild(card);
    // Mount on the always-visible panel host (like the Custom modal) rather than
    // the per-tab view root, which setTab hides on other tabs — otherwise a drop
    // opened from another tab would place the picker inside a display:none
    // subtree and never show. The overlay is position:absolute; inset:0, so it
    // covers the whole panel (.panel is position:relative).
    (document.querySelector(".panel") ?? document.body).appendChild(overlay);
    quickPick = { el: overlay, paths, presets, index: 0 };
    highlightQuick();
    // Move DOM focus into the dialog so SR users land inside it (the document
    // key handler still drives arrows/numbers/Enter). Focusing the first item
    // keeps Tab trapped within the picker from the first keystroke.
    overlay.querySelector<HTMLElement>(".quickpick-item")?.focus();
  }

  function onRowClick(v: RecentVideo): void {
    if (v.isOutput) {
      // Orphaned output: the original is gone, so a click just copies the file.
      copyFile(v.path)
        .then(() => showToast(t("videos.copiedToClipboard"), "success"))
        .catch((e) => showToast(friendlyError(e), "error"));
      return;
    }
    const j = jobForPath(v.path);
    if (j) {
      if (isBusy(j)) return;
      if (j.phase === "failed") {
        // Retry: clear the red error AND re-run the same conversion with the
        // SAME preset the failed job used — the row is the obvious retry target.
        // Only if that preset no longer exists do we fall through to the picker.
        // manual: force a failure, click the row → it re-runs the same preset
        // (queued status reappears), not a silent dismiss; delete that preset
        // first → clicking the failed row opens the picker instead.
        const retryId = j.presetId;
        dismiss(j);
        const s = getSettings();
        if (s && failedRetryDecision(retryId, s.presets).kind === "retry") {
          void doEnqueue(v, retryId);
          return;
        }
        // else: preset was deleted — fall through to the normal enqueue path.
      } else {
        dismiss(j);
      }
    }
    const settings = getSettings();
    if (!settings) return;
    if (layoutMode() === "active-bar") {
      const ap = activePreset();
      if (ap) void doEnqueue(v, ap.id);
      return;
    }
    openQuickPickForPaths([v.path]);
  }

  /** Compress arbitrary paths (drop / picker), honoring the Videos-layout
   *  setting: active-bar uses the active preset (unless Alt forces the chooser),
   *  quick-pick always opens the chooser. */
  function compressPaths(paths: string[], altHeld: boolean): void {
    if (paths.length === 0) return;
    if (shouldPickPreset(layoutMode(), altHeld)) {
      openQuickPickForPaths(paths);
    } else {
      const ap = activePreset();
      if (ap) for (const path of paths) void doEnqueuePath(path, ap.id);
    }
  }

  function currentDropHint(): string {
    if (layoutMode() === "active-bar") {
      const ap = activePreset();
      return ap
        ? t("videos.drop.withName", { name: ap.name })
        : t("videos.drop.pickPreset");
    }
    return t("videos.drop.pickPreset");
  }

  function footerHint(): string {
    return layoutMode() === "active-bar"
      ? t("videos.footer.activeBar")
      : t("videos.footer.quickPick");
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
      btn.title = blocked ? t("videos.trashGuardHint") : btn.dataset.baseTitle ?? "";
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
      // thumb_path is no longer filled at list time (lazy per row), so prefer
      // the thumbnail resolved for this row this session.
      const knownThumb = thumbPaths.get(v.path) ?? v.thumbPath;
      if (knownThumb) {
        const img = document.createElement("img");
        img.className = "preview-thumb";
        img.src = convertFileSrc(knownThumb);
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
      loading.textContent = t("videos.preparingPreview");
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
          loading.textContent = t("videos.previewUnavailable");
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
          chip.dataset.baseTitle = t("videos.compressWithName", { name: p.name });
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
      custom.textContent = t("videos.customChip");
      custom.dataset.guardPreset = "custom";
      custom.dataset.baseTitle = t("videos.customChipTitle");
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

  /** Small magnifier button that reveals the file in the file manager. */
  function buildRevealButton(path: string): HTMLButtonElement {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "row-reveal";
    btn.title = revealLabel();
    // Named + operable: in the Tab order with an accessible name, so keyboard /
    // SR users can reach and trigger it (arrow keys still drive row selection).
    btn.setAttribute("aria-label", revealLabel());
    btn.tabIndex = 0;
    // manual: Tab reaches the reveal button; SR reads it (e.g. "Show in
    // File Explorer, button"); Enter/Space reveals the file.
    btn.innerHTML =
      `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" ` +
      `stroke-width="1.5" stroke-linecap="round" aria-hidden="true">` +
      `<circle cx="7" cy="7" r="4.25"/><path d="M10.3 10.3 13.5 13.5"/></svg>`;
    btn.addEventListener("click", (e) => {
      e.stopPropagation();
      pressFeedback(btn);
      // manual: revealing a moved/deleted output now toasts "File no longer
      // exists at …" (reveal returns a Result); pressing it flashes a pressed
      // state even as the panel hides on blur.
      reveal(path).catch((err) => showToast(friendlyError(err), "error"));
    });
    return btn;
  }

  function buildRow(v: RecentVideo): HTMLElement {
    const row = document.createElement("div");
    row.className = "row";
    row.dataset.path = v.path; // the lazy observer reads this to load media
    rowByPath.set(v.path, row);

    const main = document.createElement("div");
    main.className = "row-main";

    // Always an <img> (lazily filled): the thumbnail is fetched per row as the
    // row scrolls into view (see lazyLoadRow), so the panel opens instantly.
    // A known thumb (cached this session, or rarely a pre-filled one) shows at
    // once; otherwise the placeholder styling holds until the lazy load lands.
    const img = document.createElement("img");
    img.className = "thumb thumb-placeholder";
    img.alt = "";
    img.draggable = false;
    img.loading = "lazy";
    const knownThumb = thumbPaths.get(v.path) ?? v.thumbPath;
    if (knownThumb) {
      img.src = convertFileSrc(knownThumb);
      img.classList.remove("thumb-placeholder");
    }
    main.appendChild(img);

    const info = document.createElement("div");
    info.className = "row-info";

    const name = document.createElement("div");
    name.className = "row-name";
    name.title = v.name;
    if (v.isOutput) {
      // Orphaned output: show the original's stem with a "compressed" badge.
      // The text span ellipsizes; the badge + reveal button are flex:none
      // siblings outside it so they're never clipped (styles.css .row-name).
      const text = document.createElement("span");
      text.className = "row-name-text";
      text.textContent = truncateMiddle(stripOutputSuffix(v.name), 28);
      const badge = document.createElement("span");
      badge.className = "badge-compressed";
      badge.textContent = t("videos.compressedBadge");
      if (v.conversion?.presetName) badge.title = v.conversion.presetName;
      name.append(text, badge);
    } else {
      const text = document.createElement("span");
      text.className = "row-name-text";
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
      `<div class="meta"><span class="meta-label">${t("videos.metaSize")}</span>` +
      `<span class="meta-value">${sizeText}</span></div>` +
      `<div class="meta"><span class="meta-label">${t("videos.metaLength")}</span>` +
      `<span class="meta-value meta-length">${lengthText}</span></div>` +
      `<div class="meta"><span class="meta-label">${t("videos.metaRecorded")}</span>` +
      `<span class="meta-value">${formatRelativeTime(v.createdMs)}</span></div>`;

    const status = document.createElement("div");
    status.className = "row-status";

    info.append(name, meta, status);

    const cancelBtn = document.createElement("button");
    cancelBtn.type = "button";
    cancelBtn.className = "row-cancel";
    cancelBtn.title = t("videos.cancelTitle");
    cancelBtn.setAttribute("aria-label", t("videos.cancelAria"));
    cancelBtn.textContent = "✕";
    cancelBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      const j = jobForPath(v.path);
      if (j && isBusy(j)) cancelJob(j.id).catch((err) => showToast(friendlyError(err), "error"));
    });

    const chevron = document.createElement("button");
    chevron.type = "button";
    chevron.className = "row-chevron";
    chevron.title = t("videos.detailsTitle");
    chevron.setAttribute("aria-label", t("videos.detailsAria"));
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
        status.innerHTML = `<span class="status-dim">${t("videos.status.queued")}</span>`;
        bar.style.width = "0%";
        break;
      case "pass1":
      case "pass2":
      case "verifying": {
        const pct = Math.min(100, Math.max(0, Math.round(j.progress * 100)));
        const partLabel = j.part
          ? t("videos.status.part", { current: j.part[0], total: j.part[1] })
          : "";
        status.innerHTML =
          `<span class="status-pct">${pct}%</span>` +
          `<span class="status-dim">${phaseLabel(j.phase)}${partLabel}</span>`;
        bar.style.width = `${pct}%`;
        break;
      }
      case "done": {
        const outB = j.outputBytes ?? 0;
        // Split jobs produced n part files; outputBytes is their summed size.
        const parts = j.part && j.part[1] > 1 ? j.part[1] : null;
        const outText = parts
          ? t("videos.donePartsSize", {
              parts: t("units.parts", { count: parts }),
              size: formatBytes(outB),
            })
          : formatBytes(outB);
        let html = `<span class="done-stat">${formatBytes(j.inputBytes)} → ${outText}</span>`;
        if (j.reused) {
          html += `<span class="done-smaller">${t("videos.status.reused")}</span>`;
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
        // A "Retry" affordance makes the click behavior (re-run the same preset,
        // see onRowClick) discoverable rather than a silent error-dismiss. The
        // pill is self-styled inline (no styles.css change for this task) and
        // sits below the clamped error text via flex-basis:100%.
        status.innerHTML =
          `<span class="row-error"></span>` +
          `<span class="row-retry" title="${t("videos.status.retryTitle")}" ` +
          `style="flex-basis:100%;display:inline-flex;align-items:center;gap:4px;` +
          `margin-top:2px;font-size:11px;font-weight:600;color:var(--danger);` +
          `opacity:0.85;cursor:pointer">${t("videos.status.retry")}</span>`;
        const errEl = status.querySelector<HTMLElement>(".row-error");
        if (errEl) {
          // The clamp hides anything past three lines; hover shows it all.
          const msg = j.error ?? t("videos.status.encodingFailed");
          errEl.textContent = msg;
          errEl.title = msg;
        }
        bar.style.width = "0%";
        break;
      }
      case "cancelled":
        status.innerHTML = `<span class="status-dim">${t("videos.status.cancelled")}</span>`;
        bar.style.width = "0%";
        break;
    }
  }

  function onSettingsChanged(): void {
    // Presets drive the chips; collapse previews and repaint statuses.
    closeQuickPick();
    collapseExpanded();
    // Keep the active preset valid (the chosen preset may have been deleted)
    // and reflect any layout-mode / preset changes in the bar.
    const s = getSettings();
    if (s && !s.presets.some((p) => p.id === activePresetId)) {
      activePresetId = s.defaultPresetId;
    }
    updateActiveBar();
    for (const [path, row] of rowByPath) {
      const expand = row.querySelector<HTMLElement>(".row-expand");
      if (expand) collapseRow(row, expand);
      const v = videos.find((x) => x.path === path);
      if (v) renderStatusIn(row, v);
    }
  }

  return {
    el,
    refresh,
    updateJob,
    onSettingsChanged,
    focusFilter,
    compressPaths,
    closeQuickPick,
    currentDropHint,
    footerHint,
  };
}
