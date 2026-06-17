// The gentle, once-per-version "update available" modal. When the opt-in update
// check finds a newer release, this shows one calm, dismissible card over the
// panel. It never auto-acts: every action is an explicit click, and any dismiss
// path remembers the version so the same release never re-nags — only a strictly
// newer one reappears.

import { openUrl, type UpdateInfo } from "./ipc";

/**
 * Pure decision: should the update modal be shown for `info`? True only when an
 * update exists AND its version differs from the one already dismissed. The
 * backend already guarantees `info.version` is strictly newer than the installed
 * build, so a version that isn't the dismissed one is, by construction, a newer
 * release the user hasn't seen a card for yet.
 */
export function shouldShowUpdate(
  info: UpdateInfo | null,
  lastDismissedVersion: string | null,
): info is UpdateInfo {
  return info != null && info.version !== lastDismissedVersion;
}

export interface UpdateModal {
  el: HTMLElement;
  /** Remove the modal from the DOM; idempotent. Does not re-persist. */
  close(): void;
}

/**
 * Shows the update-available modal on `host` (the always-visible panel element,
 * like the Custom / quick-pick modals — never a per-tab root that setTab hides).
 * `onDismiss` runs once on any dismiss path (Later, Esc, backdrop, or after an
 * action) so the caller can persist `lastDismissedUpdateVersion = info.version`.
 * Reuses the dialog a11y semantics from those modals: role="dialog", aria-modal,
 * focus moved inside, Esc closes, Tab is trapped.
 *
 * manual: with a newer release present and the feature on, opening the panel
 * shows one card titled "tamp <version> is available"; "What's new"/"Download"
 * open the release page; "Later"/Esc/backdrop dismiss; the card never reshows
 * for that version (only a strictly newer one does).
 */
export function openUpdateModal(opts: {
  host: HTMLElement;
  info: UpdateInfo;
  onDismiss: (version: string) => void;
}): UpdateModal {
  const { info } = opts;

  const overlay = document.createElement("div");
  overlay.className = "quickpick-overlay update-overlay";
  // A backdrop click (on the overlay itself, not the card) dismisses — calm and
  // trivially escapable, matching the quick-pick picker.
  overlay.addEventListener("click", (e) => {
    if (e.target === overlay) dismiss();
  });

  const card = document.createElement("div");
  card.className = "quickpick update-card";
  // Modal dialog semantics: SR announces a labelled dialog on open; the Tab trap
  // below keeps focus inside so it can't walk behind to the covered tabs.
  card.setAttribute("role", "dialog");
  card.setAttribute("aria-modal", "true");

  const title = document.createElement("div");
  title.className = "quickpick-title update-title";
  title.id = "update-modal-title";
  title.textContent = `tamp ${info.version} is available`;
  card.setAttribute("aria-labelledby", title.id);
  card.appendChild(title);

  const line = document.createElement("div");
  line.className = "update-line";
  line.textContent =
    "A newer version of tamp is ready. Update whenever it suits you — this won't change anything until you choose to.";
  card.appendChild(line);

  const actions = document.createElement("div");
  actions.className = "update-actions";

  // "What's new" and "Download" both open the release page (its notes live
  // there); kept as two affordances so either intent has an obvious button.
  const whatsNew = document.createElement("button");
  whatsNew.type = "button";
  whatsNew.className = "btn-ghost";
  whatsNew.textContent = "What's new";
  whatsNew.addEventListener("click", () => {
    void openUrl(info.url).catch(() => {});
    dismiss();
  });

  const download = document.createElement("button");
  download.type = "button";
  download.className = "btn-primary";
  download.textContent = "Download";
  download.addEventListener("click", () => {
    void openUrl(info.url).catch(() => {});
    dismiss();
  });

  const later = document.createElement("button");
  later.type = "button";
  later.className = "btn-ghost";
  later.textContent = "Later";
  later.addEventListener("click", () => dismiss());

  actions.append(whatsNew, download, later);
  card.appendChild(actions);

  // Tab focus trap: keep Tab/Shift+Tab cycling within the card so it can't walk
  // behind to the covered tabs/rows. Esc dismisses.
  function onKeyDown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      dismiss();
      return;
    }
    if (e.key !== "Tab") return;
    const focusable = card.querySelectorAll<HTMLElement>(
      'button:not([disabled]), [tabindex]:not([tabindex="-1"])',
    );
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    const active = document.activeElement;
    if (e.shiftKey) {
      if (active === first || !card.contains(active)) {
        e.preventDefault();
        last.focus();
      }
    } else if (active === last || !card.contains(active)) {
      e.preventDefault();
      first.focus();
    }
  }
  overlay.addEventListener("keydown", onKeyDown);

  let open = true;
  function close(): void {
    if (!open) return;
    open = false;
    overlay.removeEventListener("keydown", onKeyDown);
    overlay.remove();
  }

  // Any dismiss path persists the version (via the caller) so the card never
  // reshows for this release, then closes. Idempotent through `close`/`open`.
  function dismiss(): void {
    if (!open) return;
    opts.onDismiss(info.version);
    close();
  }

  overlay.appendChild(card);
  opts.host.appendChild(overlay);
  // Move DOM focus into the dialog so SR users land inside; "Later" first keeps
  // the calm, non-committal action under the initial focus.
  later.focus();

  return { el: overlay, close };
}
