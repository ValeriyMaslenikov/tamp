// Drag video files onto the panel to compress them. Uses Tauri's native webview
// drag-drop (delivers real file paths). Preset choice + the drop action are
// owned by the list view (so they honor the Videos-layout setting); this module
// is the overlay + path filtering + Alt tracking.
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { showToast } from "./toast";
import { t } from "../i18n";

const VIDEO_EXTS = new Set(["mov", "mp4", "m4v", "webm", "mkv", "avi"]);

/** Keeps only paths whose extension is a known video type (case-insensitive). */
export function filterVideos(paths: string[]): string[] {
  return paths.filter((p) => VIDEO_EXTS.has(p.split(".").pop()?.toLowerCase() ?? ""));
}

export interface DragDropDeps {
  /** Compress these (already video) paths, honoring the layout + Alt override. */
  compressPaths(paths: string[], altHeld: boolean): void;
  /** Overlay text for the current layout/active preset. */
  currentDropHint(): string;
}

export function initDragDrop(deps: DragDropDeps): void {
  const overlay = document.createElement("div");
  overlay.className = "drop-overlay";
  overlay.hidden = true;
  overlay.innerHTML =
    `<div class="drop-inner"><div class="drop-arrow">⤓</div>` +
    `<div class="drop-big"></div></div>`;
  document.body.appendChild(overlay);
  const big = overlay.querySelector(".drop-big") as HTMLElement;

  // The webview drop event carries no modifier flags, so track Alt live.
  let altHeld = false;
  window.addEventListener("keydown", (e) => { if (e.key === "Alt") altHeld = true; });
  window.addEventListener("keyup", (e) => { if (e.key === "Alt") altHeld = false; });
  window.addEventListener("blur", () => { altHeld = false; });

  void getCurrentWebview().onDragDropEvent((event) => {
    const p = event.payload;
    if (p.type === "enter" || p.type === "over") {
      big.textContent = deps.currentDropHint();
      overlay.hidden = false;
    } else if (p.type === "leave") {
      overlay.hidden = true;
    } else if (p.type === "drop") {
      overlay.hidden = true;
      const vids = filterVideos(p.paths);
      if (vids.length === 0) {
        showToast(t("videos.noVideoFilesInDrop"));
        return;
      }
      deps.compressPaths(vids, altHeld);
    }
  });
}
