import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export { convertFileSrc } from "@tauri-apps/api/core";

export type OutputFormat = "mp4" | "webm" | "gif";

export type SplitMode = "off" | "smart" | "static";

export type StaticSplitBy = "parts" | "seconds";

/**
 * Split-into-parts settings: each part is compressed independently to the
 * preset's full target size. `parts`/`seconds` only apply in static mode.
 */
export interface SplitConfig {
  mode: SplitMode;
  by: StaticSplitBy;
  parts: number;
  seconds: number;
}

export interface Preset {
  id: string;
  name: string;
  targetMb: number;
  maxFps: number | null;
  maxWidth: number | null;
  scalePercent: number | null;
  stripAudio: boolean;
  format: OutputFormat;
  split: SplitConfig;
}

export interface Settings {
  watchedFolders: string[];
  copyToClipboard: boolean;
  trashOriginal: boolean;
  presets: Preset[];
  defaultPresetId: string;
  launchAtLogin: boolean;
  useHardwareEncoder: boolean;
  /** Global shortcut accelerators; null/empty disables the shortcut. */
  shortcutCompressLatest: string | null;
  shortcutTogglePanel: string | null;
  /** Notify when the compress-latest shortcut picks a video older than this. */
  staleWarnMinutes: number;
  /** Reveal finished outputs in the file manager after converting. */
  openAfterConvert: OpenAfterConvert;
  /** Which preset-selection UI the Videos screen shows. */
  videosLayout: VideosLayout;
  /** Color theme for the panel. */
  theme: Theme;
  /** Windows: Explorer "Compress with tamp" right-click entry registered. */
  contextMenuEnabled: boolean;
  /** How many recent videos the Videos tab lists (1–200). */
  recentsLimit: number;
  /** Whether the one-time first-run notice has been shown and dismissed. */
  onboardingSeen: boolean;
  /** Whether the opt-in GitHub update check runs on launch (off by default). */
  updateCheckEnabled: boolean;
  /** Newest version already dismissed in the update modal; never re-nags for it. */
  lastDismissedUpdateVersion: string | null;
}

/**
 * Notification permission as a lowercase string. "unsupported" means the state
 * couldn't be read (no API / plugin error). On desktop the plugin always
 * reports "granted" (the OS owns the toggle), so "denied" only ever appears
 * where the platform surfaces a real one.
 */
export type NotificationPermission =
  | "granted"
  | "denied"
  | "prompt"
  | "prompt-with-rationale"
  | "unsupported";

/** off = never; multipart = open the folder after a split; all = also reveal single outputs. */
export type OpenAfterConvert = "off" | "multipart" | "all";

/** quick-pick = picker per video (default); active-bar = one active preset. */
export type VideosLayout = "quick-pick" | "active-bar";

/** system = follow OS light/dark (tracks live changes); light/dark pin it. */
export type Theme = "system" | "light" | "dark";

/** One-off conversion settings for custom_convert. */
export interface CustomConfig {
  targetMb: number;
  maxFps: number | null;
  maxWidth: number | null;
  scalePercent: number | null;
  stripAudio: boolean;
  format: OutputFormat;
  split: SplitConfig;
}

/** Conversion details for an orphaned output row (original gone). */
export interface ConversionMeta {
  originalBytes: number | null;
  outputBytes: number;
  presetName: string | null;
}

export interface RecentVideo {
  path: string;
  name: string;
  sizeBytes: number;
  createdMs: number;
  thumbPath: string | null;
  /** True for "(tamped …)" outputs whose original no longer exists. */
  isOutput: boolean;
  conversion: ConversionMeta | null;
  /** Probed video duration; null until probed (or when probing failed). */
  durationSecs: number | null;
}

export type Phase =
  | "queued"
  | "pass1"
  | "pass2"
  | "verifying"
  | "done"
  | "failed"
  | "cancelled";

export interface JobState {
  id: string;
  inputPath: string;
  inputName: string;
  outputPath: string | null;
  presetId: string;
  /** Config hash of the job's preset; equal hashes mean identical output. */
  presetHash: string;
  phase: Phase;
  progress: number; // 0..1 overall
  inputBytes: number;
  outputBytes: number | null;
  /** Done without encoding: an identical earlier output already existed. */
  reused: boolean;
  /** Split jobs only: [current part, total parts]; null for single outputs. */
  part: [number, number] | null;
  error: string | null;
  /** Set when a post-action (clipboard copy / trash original) failed after a successful encode. */
  postError: string | null;
}

export const listRecents = (): Promise<RecentVideo[]> =>
  invoke<RecentVideo[]>("list_recents");

/** Watched folders that exist but can't be read now (offline share /
 *  permission-denied), as display strings — drives the Videos-tab "couldn't
 *  read <folder>" banner. Empty when all folders are reachable or just missing. */
export const unreachableFolders = (): Promise<string[]> =>
  invoke<string[]>("unreachable_folders");

/** One delivered output of a conversion job: a single has one, a split has N. */
export interface ConversionOutput {
  path: string;
  bytes: number;
}

/** One past conversion, from the persistent journal. */
export interface ConversionRecord {
  inputPath: string;
  inputBytes: number;
  /** The job's delivered outputs: one for a single, N for a split set. */
  outputs: ConversionOutput[];
  presetHash: string;
  presetName: string;
  targetMb: number;
  completedAtMs: number;
  inputCreatedMs: number;
}

export const listConversions = (): Promise<ConversionRecord[]> =>
  invoke<ConversionRecord[]>("list_conversions");

export const getSettings = (): Promise<Settings> =>
  invoke<Settings>("get_settings");

export const saveSettings = (settings: Settings): Promise<Settings> =>
  invoke<Settings>("save_settings", { settings });

export const setContextMenu = (enabled: boolean): Promise<void> =>
  invoke<void>("set_context_menu", { enabled });

export const setPin = (pinned: boolean): Promise<void> =>
  invoke<void>("set_pin", { pinned });

/** The current notification permission state (see {@link NotificationPermission}). */
export const notificationPermission = (): Promise<NotificationPermission> =>
  invoke<NotificationPermission>("notification_permission");

/** Re-requests the notification permission; resolves to the resulting state. */
export const requestNotificationPermission =
  (): Promise<NotificationPermission> =>
    invoke<NotificationPermission>("request_notification_permission");

/** Deep-links to the OS notifications settings (no-op where unsupported). */
export const openNotificationSettings = (): Promise<void> =>
  invoke<void>("open_notification_settings");

export const pickFolder = (): Promise<string | null> =>
  invoke<string | null>("pick_folder");

export const pickVideos = (): Promise<string[]> =>
  invoke<string[]>("pick_videos");

export const enqueue = (path: string, presetId: string): Promise<string> =>
  invoke<string>("enqueue", { path, presetId });

/** One-off conversion with ad-hoc settings; returns the job id. */
export const customConvert = (
  path: string,
  config: CustomConfig,
): Promise<string> => invoke<string>("custom_convert", { path, config });

export const cancelJob = (id: string): Promise<void> =>
  invoke<void>("cancel_job", { id });

export const queueState = (): Promise<JobState[]> =>
  invoke<JobState[]>("queue_state");

export const reveal = (path: string): Promise<void> =>
  invoke<void>("reveal", { path });

/** Resolve (generating on miss) a lightweight montage proxy for hover previews. */
export const ensurePreview = (path: string): Promise<string> =>
  invoke<string>("ensure_preview", { path });

export const copyFile = (path: string): Promise<void> =>
  invoke<void>("copy_file", { path });

/** Copy multiple files to the clipboard in one write (a single CF_HDROP list on
 *  Windows) so every part of a split lands — not just the last. */
export const copyFiles = (paths: string[]): Promise<void> =>
  invoke<void>("copy_files", { paths });

export const openFile = (path: string): Promise<void> =>
  invoke<void>("open_file", { path });

export const conversionThumb = (path: string): Promise<string | null> =>
  invoke<string | null>("conversion_thumb", { path });

/** Ensure (generating on miss) one recent video's thumbnail; lazy per row. */
export const recentThumb = (path: string): Promise<string | null> =>
  invoke<string | null>("recent_thumb", { path });

/** Resolve (probing + caching on miss) one recent video's duration; lazy per row. */
export const recentDuration = (path: string): Promise<number | null> =>
  invoke<number | null>("recent_duration", { path });

export const onPanelShown = (cb: () => void): Promise<UnlistenFn> =>
  listen("panel:shown", () => cb());

export const onEncodeState = (cb: (s: JobState) => void): Promise<UnlistenFn> =>
  listen<JobState>("encode:state", (e) => cb(e.payload));

export const onSettingsChanged = (
  cb: (s: Settings) => void,
): Promise<UnlistenFn> => listen<Settings>("settings:changed", (e) => cb(e.payload));
