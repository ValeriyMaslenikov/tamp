import type { Theme } from "./ipc";

// Tracks the OS dark-mode signal so "system" can resolve and live-update.
const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");

let currentPref: Theme = "system";

/** Resolve a preference to the concrete theme actually painted. */
function effective(pref: Theme): "light" | "dark" {
  if (pref === "light" || pref === "dark") return pref;
  return darkQuery.matches ? "dark" : "light";
}

/**
 * Apply a theme preference to the document. Sets `data-theme` on <html> to the
 * concrete light/dark value; the stylesheet keys its palette off that attribute.
 */
export function applyTheme(pref: Theme): void {
  currentPref = pref;
  document.documentElement.dataset.theme = effective(pref);
}

// When following the system, repaint as the OS theme flips under us.
darkQuery.addEventListener("change", () => {
  if (currentPref === "system") applyTheme("system");
});

// Paint the system theme immediately at import time so first paint matches the
// OS even before stored settings have loaded (avoids a light/dark flash).
applyTheme("system");
