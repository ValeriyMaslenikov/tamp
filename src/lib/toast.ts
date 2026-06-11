let host: HTMLElement | null = null;
let timer: number | undefined;

export function initToast(el: HTMLElement): void {
  host = el;
}

/** Bottom-of-panel toast, auto-hides after 4s. */
export function showToast(message: string): void {
  if (!host) return;
  host.textContent = message;
  host.hidden = false;
  window.clearTimeout(timer);
  timer = window.setTimeout(() => {
    if (host) host.hidden = true;
  }, 4000);
}
