import { invoke } from "@tauri-apps/api/core";
import { t } from "../i18n";

type OS = "macos" | "windows" | "linux";

let os: OS = "macos";

/** Resolves the backend OS once at boot; UI strings fall back to macOS. */
export async function initPlatform(): Promise<void> {
  try {
    os = (await invoke<string>("os_info")) as OS;
  } catch {
    /* keep the default */
  }
}

/**
 * The only place the frontend is allowed to branch on the OS: user-visible
 * strings naming OS concepts (file manager, etc.).
 */
export function revealLabel(): string {
  return os === "macos"
    ? t("platform.revealFinder")
    : t("platform.revealExplorer");
}

/** Whether the backend OS is Windows (gates Windows-only Preferences). */
export function isWindows(): boolean {
  return os === "windows";
}
