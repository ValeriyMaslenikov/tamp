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
  switchRow,
} from "../lib/forms";
import { truncateMiddle } from "../lib/format";
import { showToast } from "../lib/toast";

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

  let open = true;
  function close(): void {
    if (!open) return;
    open = false;
    el.remove();
    opts.onClose();
  }

  const header = document.createElement("div");
  header.className = "modal-header";
  const back = document.createElement("button");
  back.type = "button";
  back.className = "modal-back";
  back.title = "Back (Esc)";
  back.textContent = "‹";
  back.addEventListener("click", close);
  const titles = document.createElement("div");
  titles.className = "modal-titles";
  const title = document.createElement("div");
  title.className = "modal-title";
  title.textContent = "Custom conversion";
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

  const audio = switchRow("Strip audio", false);

  const grid1 = document.createElement("div");
  grid1.className = "field-grid";
  grid1.append(field("Target MB", targetInput), field("Format", formatInput));

  const grid2 = document.createElement("div");
  grid2.className = "field-grid field-grid-3";
  grid2.append(
    field("Max FPS", fpsInput),
    field("Max width", widthInput),
    field("Scale %", scaleInput),
  );

  const hint = document.createElement("div");
  hint.className = "field-hint";
  hint.textContent = "Max width and scale % are mutually exclusive.";

  const convert = document.createElement("button");
  convert.type = "button";
  convert.className = "btn-primary btn-block modal-convert";
  convert.textContent = "Convert";
  convert.addEventListener("click", () => {
    const target = Number(targetInput.value);
    if (!(target > 0)) {
      showToast("Target size must be greater than 0 MB");
      return;
    }
    const fps = parseOptionalPositiveInt(fpsInput.value);
    const width = parseOptionalPositiveInt(widthInput.value);
    const scale = parseOptionalPositiveInt(scaleInput.value);
    if (fps === undefined || width === undefined || scale === undefined) {
      showToast("FPS, width and scale must be positive whole numbers");
      return;
    }
    const config: CustomConfig = {
      targetMb: target,
      maxFps: fps,
      maxWidth: width,
      scalePercent: width != null ? null : scale,
      stripAudio: audio.input.checked,
      format: formatInput.value as OutputFormat,
    };
    convert.disabled = true;
    customConvert(video.path, config)
      .then((id) => {
        opts.onStarted(id, config);
        close();
      })
      .catch((e) => {
        convert.disabled = false;
        showToast(String(e));
      });
  });

  body.append(grid1, grid2, hint, audio.row, convert);
  el.append(header, body);
  opts.host.appendChild(el);
  targetInput.focus();

  return { el, close };
}
