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

/** The calm one-liner above the update-check consent checkbox: it frames the
 *  choice as opt-in and reassuring, not a demand. Pure so it's unit-testable. */
export function updateCheckConsentLine(): string {
  return "tamp can quietly check for new versions when it starts.";
}

/** The privacy sub-label under the consent checkbox: it makes the single
 *  outbound request's scope explicit (a pull to GitHub, nothing about the
 *  user). Pure so it's unit-testable. */
export function updateCheckPrivacyHint(): string {
  return "Only asks GitHub for the latest version — nothing about you is sent.";
}

export interface OnboardingNotice {
  /** The notice element, ready to insert at the top of the panel. */
  el: HTMLElement;
}

/**
 * Builds the one-time first-run notice: where the tray/menu-bar icon lives, how
 * to reopen the panel, a one-line notification rationale so the permission
 * priming that follows has in-app context, and an opt-in update-check consent
 * checkbox. `onDismiss` runs when the user clicks "Got it", receiving the
 * checkbox state so the caller can persist `updateCheckEnabled` (and the seen
 * flag) and remove the element. The checkbox defaults to CHECKED — it's a
 * visible, one-click-to-uncheck choice ("ask on first run" without nagging);
 * privacy-minded users simply uncheck before dismissing. Calm styling reuses the
 * Batch 6 folder-notice card so it informs without alarming.
 */
export function buildOnboardingNotice(
  onDismiss: (updateCheckEnabled: boolean) => void,
): OnboardingNotice {
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

  // Update-check consent: a calm line + a default-checked checkbox the user can
  // uncheck in one click. Its state is read at dismiss time so the caller
  // persists updateCheckEnabled alongside the seen flag.
  const consentLine = document.createElement("div");
  consentLine.className = "folder-notice-hint";
  consentLine.textContent = updateCheckConsentLine();
  el.appendChild(consentLine);

  const consent = document.createElement("label");
  consent.className = "onboarding-consent";
  const consentBox = document.createElement("input");
  consentBox.type = "checkbox";
  consentBox.checked = true; // default on — a visible, one-click-to-uncheck choice
  const consentText = document.createElement("span");
  consentText.className = "onboarding-consent-text";
  const consentTitle = document.createElement("span");
  consentTitle.className = "onboarding-consent-title";
  consentTitle.textContent = "Check for new versions automatically";
  const consentSub = document.createElement("span");
  consentSub.className = "onboarding-consent-sub";
  consentSub.textContent = updateCheckPrivacyHint();
  consentText.append(consentTitle, consentSub);
  consent.append(consentBox, consentText);
  el.appendChild(consent);

  const actions = document.createElement("div");
  actions.className = "onboarding-actions";
  const dismiss = document.createElement("button");
  dismiss.type = "button";
  dismiss.className = "btn-ghost";
  dismiss.textContent = "Got it";
  dismiss.addEventListener("click", () => onDismiss(consentBox.checked));
  actions.appendChild(dismiss);
  el.appendChild(actions);

  return { el };
}
