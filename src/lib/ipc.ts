import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export { convertFileSrc } from "@tauri-apps/api/core";

export interface Preset {
  id: string;
  name: string;
  targetMb: number;
  maxFps: number | null;
  maxWidth: number | null;
  scalePercent: number | null;
  stripAudio: boolean;
}

export interface Settings {
  watchedFolders: string[];
  copyToClipboard: boolean;
  trashOriginal: boolean;
  presets: Preset[];
  defaultPresetId: string;
  launchAtLogin: boolean;
}

export interface RecentVideo {
  path: string;
  name: string;
  sizeBytes: number;
  createdMs: number;
  thumbPath: string | null;
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
  phase: Phase;
  progress: number; // 0..1 overall
  inputBytes: number;
  outputBytes: number | null;
  error: string | null;
}

export const listRecents = (): Promise<RecentVideo[]> =>
  invoke<RecentVideo[]>("list_recents");

export const getSettings = (): Promise<Settings> =>
  invoke<Settings>("get_settings");

export const saveSettings = (settings: Settings): Promise<Settings> =>
  invoke<Settings>("save_settings", { settings });

export const pickFolder = (): Promise<string | null> =>
  invoke<string | null>("pick_folder");

export const enqueue = (path: string, presetId: string): Promise<string> =>
  invoke<string>("enqueue", { path, presetId });

export const cancelJob = (id: string): Promise<void> =>
  invoke<void>("cancel_job", { id });

export const queueState = (): Promise<JobState[]> =>
  invoke<JobState[]>("queue_state");

export const reveal = (path: string): Promise<void> =>
  invoke<void>("reveal", { path });

export const onPanelShown = (cb: () => void): Promise<UnlistenFn> =>
  listen("panel:shown", () => cb());

export const onEncodeState = (cb: (s: JobState) => void): Promise<UnlistenFn> =>
  listen<JobState>("encode:state", (e) => cb(e.payload));

export const onSettingsChanged = (
  cb: (s: Settings) => void,
): Promise<UnlistenFn> => listen<Settings>("settings:changed", (e) => cb(e.payload));
