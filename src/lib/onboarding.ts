import { isWindows } from "./platform";

/** The reopen shortcut wording, per OS (mirrors the default toggle-panel
 *  accelerator CmdOrCtrl+Alt+O). Pure so it's unit-testable. */
export function reopenShortcut(windows: boolean): string {
  return windows ? "Ctrl+Alt+O" : "Cmd+Alt+O";
}

/** Where this OS hides the tray/menu-bar icon — the load-bearing hint, since a
 *  resident menu-bar app is invisible until the user finds its icon. Windows
 *  buries it under the `^` overflow; macOS keeps it in the menu bar. Pure. */
export function trayLocationHint(windows: boolean): string {
  return windows
    ? "tamp now lives in your system tray, near the clock. Windows often hides it under the ^ overflow arrow — drag it out to keep it handy."
    : "tamp now lives in your menu bar, near the clock at the top-right of your screen.";
}

/** The one-line notification rationale shown in the first-run notice, so the
 *  permission prompt that may follow has in-app context: why tamp wants to
 *  notify, and that it's reversible. Pure so it's unit-testable. This is the
 *  "permission priming with context" the onboarding step requires. */
export function notificationPrimingHint(): string {
  return "tamp may ask to send notifications so it can warn you when your latest recording goes stale — you can change this any time in Preferences.";
}

export interface OnboardingNotice {
  /** The notice element, ready to insert at the top of the panel. */
  el: HTMLElement;
}

/**
 * Builds the one-time first-run notice: where the tray/menu-bar icon lives, how
 * to reopen the panel, and a one-line notification rationale so the permission
 * priming that follows has in-app context. `onDismiss` runs when the user
 * dismisses it (the caller persists the seen flag and removes the element). Calm
 * styling reuses the Batch 6 folder-notice card so it informs without alarming.
 */
export function buildOnboardingNotice(onDismiss: () => void): OnboardingNotice {
  const windows = isWindows();
  const el = document.createElement("div");
  el.className = "folder-notice onboarding-notice";
  el.setAttribute("role", "status");

  const title = document.createElement("div");
  title.className = "folder-notice-title";
  title.textContent = "Welcome to tamp";
  el.appendChild(title);

  const where = document.createElement("div");
  where.className = "folder-notice-hint";
  where.textContent = trayLocationHint(windows);
  el.appendChild(where);

  const reopen = document.createElement("div");
  reopen.className = "folder-notice-hint";
  reopen.textContent = `Press ${reopenShortcut(windows)} any time to show or hide this panel.`;
  el.appendChild(reopen);

  // The notification rationale: this is the in-context explanation that must be
  // visible at the moment the caller primes the permission, so the OS prompt (on
  // platforms that surface one) isn't unexplained.
  const notify = document.createElement("div");
  notify.className = "folder-notice-hint";
  notify.textContent = notificationPrimingHint();
  el.appendChild(notify);

  const actions = document.createElement("div");
  actions.className = "onboarding-actions";
  const dismiss = document.createElement("button");
  dismiss.type = "button";
  dismiss.className = "btn-ghost";
  dismiss.textContent = "Got it";
  dismiss.addEventListener("click", onDismiss);
  actions.appendChild(dismiss);
  el.appendChild(actions);

  return { el };
}
