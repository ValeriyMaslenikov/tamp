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
}

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

/** One past conversion, from the persistent journal. */
export interface ConversionRecord {
  inputPath: string;
  inputBytes: number;
  outputPath: string;
  outputBytes: number;
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

export const onPanelShown = (cb: () => void): Promise<UnlistenFn> =>
  listen("panel:shown", () => cb());

export const onEncodeState = (cb: (s: JobState) => void): Promise<UnlistenFn> =>
  listen<JobState>("encode:state", (e) => cb(e.payload));

export const onSettingsChanged = (
  cb: (s: Settings) => void,
): Promise<UnlistenFn> => listen<Settings>("settings:changed", (e) => cb(e.payload));
