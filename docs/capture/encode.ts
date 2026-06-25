// Shared clip encoder. GitHub's README sanitizer strips <video> tags that point
// at committed/raw files (only its own attachment-upload URLs render), so the
// demos are embedded as animated WebP via <img> — which DOES render inline
// (autoplay + loop) and, being full colour, stays crisp with none of the GIF
// palette/dither grain. We also emit an MP4 (H.264) for an optional
// "full quality" link (GitHub's blob view plays committed MP4s on click).
import { execFileSync } from "node:child_process";
import { readdirSync } from "node:fs";
import { join } from "node:path";

/** Find the single *.webm Playwright flushed into a recordVideo dir. */
export function findWebm(dir: string): string {
  const f = readdirSync(dir).find((n) => n.endsWith(".webm"));
  if (!f) throw new Error("no webm recorded in " + dir);
  return join(dir, f);
}

export interface EncodeOpts {
  /** Optional trim: start seconds. */
  ss?: number;
  /** Optional trim: duration seconds. */
  t?: number;
  /** Width of the animated WebP in px (≈1.6× the README embed width for retina). */
  webpWidth: number;
  /** WebP frame rate (default 15). */
  webpFps?: number;
  /** WebP quality 0–100 (default 48; lower = smaller). */
  webpQ?: number;
}

/**
 * Encode `src` to `${outBase}.webp` (animated, for inline <img>) and
 * `${outBase}.mp4` (H.264, for a full-quality link). Source is the 2× recording,
 * so both stay sharp.
 */
export function encodeClip(src: string, outBase: string, opts: EncodeOpts): void {
  const trim: string[] = [];
  if (opts.ss != null) trim.push("-ss", opts.ss.toFixed(3));
  if (opts.t != null) trim.push("-t", opts.t.toFixed(3));

  const fps = opts.webpFps ?? 15;
  const q = opts.webpQ ?? 48;
  const evenScale = "scale=trunc(iw/2)*2:trunc(ih/2)*2";

  // Animated WebP — the inline README image. Full colour, loops forever.
  execFileSync("ffmpeg", [
    "-y", ...trim, "-i", src,
    "-vf", `fps=${fps},scale=${opts.webpWidth}:-1:flags=lanczos`,
    "-c:v", "libwebp", "-loop", "0", "-q:v", String(q), "-compression_level", "6", "-an",
    `${outBase}.webp`,
  ]);

  // MP4 — H.264 high, yuv420p, faststart; kept for a "full quality" link.
  execFileSync("ffmpeg", [
    "-y", ...trim, "-i", src,
    "-vf", evenScale,
    "-c:v", "libx264", "-profile:v", "high", "-pix_fmt", "yuv420p",
    "-crf", "20", "-preset", "slow", "-movflags", "+faststart", "-an",
    `${outBase}.mp4`,
  ]);
}
