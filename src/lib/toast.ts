let host: HTMLElement | null = null;
let timer: number | undefined;

/** Toast severity. Drives both styling and the live-region politeness:
 *  errors interrupt (assertive/alert), everything else waits (polite/status). */
export type ToastKind = "info" | "success" | "error";

export function initToast(el: HTMLElement): void {
  host = el;
}

/** Bottom-of-panel toast, auto-hides after 4s. `kind` is optional and defaults
 *  to "info" so existing callers stay source-compatible. */
export function showToast(message: string, kind: ToastKind = "info"): void {
  if (!host) return;
  // The container stays in the DOM (a persistent live region); we toggle a
  // visual `is-shown` class rather than `hidden`/display:none so the textContent
  // change is reliably announced. Errors get an assertive alert role so they
  // interrupt; info/success stay polite.
  const assertive = kind === "error";
  host.setAttribute("role", assertive ? "alert" : "status");
  host.setAttribute("aria-live", assertive ? "assertive" : "polite");
  host.classList.remove("toast-info", "toast-success", "toast-error");
  host.classList.add(`toast-${kind}`);
  host.textContent = message;
  host.classList.add("is-shown");
  window.clearTimeout(timer);
  timer = window.setTimeout(() => {
    if (host) host.classList.remove("is-shown");
  }, 4000);
}

// manual: with a screen reader, "Copied to clipboard" announces politely and an
// error message interrupts; success looks green/check-distinct from red errors.
