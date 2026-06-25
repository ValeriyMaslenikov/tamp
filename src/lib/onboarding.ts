import { isWindows } from "./platform";
import { t } from "../i18n";

/** The reopen shortcut wording, per OS (mirrors the default toggle-panel
 *  accelerator CmdOrCtrl+Alt+O). An accelerator string, not localized. Pure so
 *  it's unit-testable. */
export function reopenShortcut(windows: boolean): string {
  return windows ? "Ctrl+Alt+O" : "Cmd+Alt+O";
}

/** The compress-latest shortcut wording, per OS (mirrors the default
 *  compress-latest accelerator CmdOrCtrl+Alt+T). An accelerator string, not
 *  localized. Pure so it's unit-testable. */
export function compressLatestShortcut(windows: boolean): string {
  return windows ? "Ctrl+Alt+T" : "Cmd+Alt+T";
}

/** The privacy sub-label under the consent checkbox: it makes the single
 *  outbound request's scope explicit (a pull to GitHub, nothing about the
 *  user). Localized via t(); pure so it's unit-testable. */
export function updateCheckPrivacyHint(): string {
  return t("onboarding.updatePrivacyHint");
}

export interface OnboardingNotice {
  /** The notice element, ready to insert at the top of the panel. */
  el: HTMLElement;
}

/** One guided step: an accent number badge, a bold title, and one calm detail
 *  line. Appended to the notice card in order. */
function buildStep(n: number, title: string, detail: string): HTMLElement {
  const step = document.createElement("div");
  step.className = "onboarding-step";

  const badge = document.createElement("div");
  badge.className = "onboarding-step-badge";
  badge.textContent = String(n);
  badge.setAttribute("aria-hidden", "true");
  step.appendChild(badge);

  const body = document.createElement("div");
  body.className = "onboarding-step-body";

  const stepTitle = document.createElement("div");
  stepTitle.className = "onboarding-step-title";
  stepTitle.textContent = title;
  body.appendChild(stepTitle);

  const stepDetail = document.createElement("div");
  stepDetail.className = "onboarding-step-detail";
  stepDetail.textContent = detail;
  body.appendChild(stepDetail);

  step.appendChild(body);
  return step;
}

/**
 * Builds the one-time first-run notice: a guided, numbered four-step flow that
 * walks the user from finding Tamp in the tray to pasting a shrunk recording,
 * followed by an opt-in update-check consent checkbox. `onDismiss` runs when the
 * user clicks "Got it", receiving the checkbox state so the caller can persist
 * `updateCheckEnabled` (and the seen flag) and remove the element. The checkbox
 * defaults to CHECKED — it's a visible, one-click-to-uncheck choice ("ask on
 * first run" without nagging); privacy-minded users simply uncheck before
 * dismissing. Calm styling reuses the folder-notice card so it informs without
 * alarming. The reopen / compress-latest shortcuts are rendered per OS (Ctrl on
 * Windows, Cmd elsewhere).
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
  title.textContent = t("onboarding.title");
  el.appendChild(title);

  const lead = document.createElement("div");
  lead.className = "onboarding-lead";
  lead.textContent = t("onboarding.lead");
  el.appendChild(lead);

  const steps = document.createElement("div");
  steps.className = "onboarding-steps";

  // 1. Open it — where Tamp lives + the reopen shortcut, per OS.
  steps.appendChild(
    buildStep(
      1,
      t("onboarding.step1Title"),
      t("onboarding.step1Detail", { shortcut: reopenShortcut(windows) }),
    ),
  );
  // 2. Make a preset.
  steps.appendChild(
    buildStep(2, t("onboarding.step2Title"), t("onboarding.step2Detail")),
  );
  // 3. Convert a recording — drag, right-click, or the compress-latest shortcut.
  steps.appendChild(
    buildStep(
      3,
      t("onboarding.step3Title"),
      t("onboarding.step3Detail", {
        shortcut: compressLatestShortcut(windows),
      }),
    ),
  );
  // 4. Use it.
  steps.appendChild(
    buildStep(4, t("onboarding.step4Title"), t("onboarding.step4Detail")),
  );

  el.appendChild(steps);

  // Update-check consent: a default-checked checkbox the user can uncheck in one
  // click. Its state is read at dismiss time so the caller persists
  // updateCheckEnabled alongside the seen flag.
  const consent = document.createElement("label");
  consent.className = "onboarding-consent";
  const consentBox = document.createElement("input");
  consentBox.type = "checkbox";
  consentBox.checked = true; // default on — a visible, one-click-to-uncheck choice
  const consentText = document.createElement("span");
  consentText.className = "onboarding-consent-text";
  const consentTitle = document.createElement("span");
  consentTitle.className = "onboarding-consent-title";
  consentTitle.textContent = t("onboarding.updateConsentTitle");
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
  dismiss.className = "btn-primary onboarding-dismiss";
  dismiss.textContent = t("common.gotIt");
  dismiss.addEventListener("click", () => onDismiss(consentBox.checked));
  actions.appendChild(dismiss);
  el.appendChild(actions);

  return { el };
}
