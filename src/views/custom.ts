// Full-panel "Custom conversion" page: ad-hoc one-off settings for a single
// video, submitted through the custom_convert ipc command.

import {
  customConvert,
  type CustomConfig,
  type OutputFormat,
  type RecentVideo,
} from "../lib/ipc";
import {
  field,
  formatSelect,
  numberInput,
  parseOptionalPositiveInt,
  splitControl,
  switchRow,
} from "../lib/forms";
import { truncateMiddle } from "../lib/format";
import { showToast } from "../lib/toast";
import { friendlyError } from "../lib/errors";
import { t } from "../i18n";

export interface CustomModal {
  el: HTMLElement;
  /** Remove the page from the DOM; idempotent. Fires onClose once. */
  close(): void;
}

export function openCustomModal(opts: {
  host: HTMLElement;
  video: RecentVideo;
  onStarted: (jobId: string, config: CustomConfig) => void;
  onClose: () => void;
}): CustomModal {
  const { video } = opts;
  const el = document.createElement("div");
  el.className = "modal-page";
  // Modal dialog semantics: SR announces a labelled dialog on open, and the Tab
  // trap below keeps focus inside so Tab can't walk behind to the covered tabs.
  // manual: opening Custom announces "Custom conversion, dialog"; Tab cycles
  // within the page and Shift+Tab from the first control wraps to the last.
  el.setAttribute("role", "dialog");
  el.setAttribute("aria-modal", "true");
  el.setAttribute("aria-label", t("custom.title"));

  // The trap is registered on open and torn down on close. Capturing Tab inside
  // the dialog cycles focus across its focusable controls (Shift+Tab reverses).
  function onTrapKeyDown(e: KeyboardEvent): void {
    if (e.key !== "Tab") return;
    const focusable = el.querySelectorAll<HTMLElement>(
      'a[href], button:not([disabled]), input:not([disabled]), ' +
        'select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
    );
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    const active = document.activeElement;
    if (e.shiftKey) {
      if (active === first || !el.contains(active)) {
        e.preventDefault();
        last.focus();
      }
    } else if (active === last || !el.contains(active)) {
      e.preventDefault();
      first.focus();
    }
  }

  let open = true;
  function close(): void {
    if (!open) return;
    open = false;
    el.removeEventListener("keydown", onTrapKeyDown);
    el.remove();
    opts.onClose();
  }

  const header = document.createElement("div");
  header.className = "modal-header";
  const back = document.createElement("button");
  back.type = "button";
  back.className = "modal-back";
  back.title = t("custom.backTitle");
  back.setAttribute("aria-label", t("custom.back"));
  back.textContent = "‹";
  back.addEventListener("click", close);
  const titles = document.createElement("div");
  titles.className = "modal-titles";
  const title = document.createElement("div");
  title.className = "modal-title";
  title.textContent = t("custom.title");
  const subtitle = document.createElement("div");
  subtitle.className = "modal-subtitle";
  subtitle.textContent = truncateMiddle(video.name, 44);
  subtitle.title = video.name;
  titles.append(title, subtitle);
  header.append(back, titles);

  const body = document.createElement("div");
  body.className = "modal-body";

  const targetInput = numberInput(null, "10");
  targetInput.min = "0.1";
  targetInput.step = "0.5";

  const formatInput = formatSelect("mp4");

  const fpsInput = numberInput(null, "auto");
  const widthInput = numberInput(null, "auto");
  const scaleInput = numberInput(null, "auto");

  // Max width and scale % are mutually exclusive: typing one clears the other.
  widthInput.addEventListener("input", () => {
    if (widthInput.value.trim() !== "") scaleInput.value = "";
  });
  scaleInput.addEventListener("input", () => {
    if (scaleInput.value.trim() !== "") widthInput.value = "";
  });

  const split = splitControl();

  const audio = switchRow(t("custom.stripAudio"), false);

  const grid1 = document.createElement("div");
  grid1.className = "field-grid";
  grid1.append(
    field(t("custom.targetMb"), targetInput),
    field(t("custom.format"), formatInput),
  );

  const grid2 = document.createElement("div");
  grid2.className = "field-grid field-grid-3";
  grid2.append(
    field(t("custom.maxFps"), fpsInput),
    field(t("custom.maxWidth"), widthInput),
    field(t("custom.scalePercent"), scaleInput),
  );

  const hint = document.createElement("div");
  hint.className = "field-hint";
  hint.textContent = t("custom.widthScaleHint");

  const convert = document.createElement("button");
  convert.type = "button";
  convert.className = "btn-primary btn-block modal-convert";
  convert.textContent = t("custom.convert");
  convert.addEventListener("click", () => {
    const target = Number(targetInput.value);
    if (!(target > 0)) {
      showToast(t("custom.errTargetSize"));
      return;
    }
    const fps = parseOptionalPositiveInt(fpsInput.value);
    const width = parseOptionalPositiveInt(widthInput.value);
    const scale = parseOptionalPositiveInt(scaleInput.value);
    if (fps === undefined || width === undefined || scale === undefined) {
      showToast(t("custom.errFpsWidthScale"));
      return;
    }
    const splitRead = split.read();
    if ("error" in splitRead) {
      showToast(splitRead.error);
      return;
    }
    const config: CustomConfig = {
      targetMb: target,
      maxFps: fps,
      maxWidth: width,
      scalePercent: width != null ? null : scale,
      stripAudio: audio.input.checked,
      format: formatInput.value as OutputFormat,
      split: splitRead.config,
    };
    convert.disabled = true;
    customConvert(video.path, config)
      .then((id) => {
        opts.onStarted(id, config);
        close();
      })
      .catch((e) => {
        convert.disabled = false;
        showToast(friendlyError(e), "error");
      });
  });

  body.append(grid1, grid2, hint, split.el, audio.row, convert);
  el.append(header, body);
  opts.host.appendChild(el);
  el.addEventListener("keydown", onTrapKeyDown);
  // Move DOM focus into the dialog so SR users land inside; onClose restores
  // focus to the trigger via the caller's focusFilter().
  targetInput.focus();

  return { el, close };
}
