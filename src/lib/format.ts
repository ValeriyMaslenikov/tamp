// Pure formatting helpers — unit tested in format.test.ts.
//
// i18n: dates/times/numbers follow the OS locale by routing through Intl on the
// *default* locale (locale param left undefined). Each formatter takes an
// optional `locale` so tests can pin a deterministic locale (Intl output varies
// by host locale, so tests must never assert a bare default-locale string).

import type { SplitConfig } from "./ipc";
import { t } from "../i18n";

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

function trimmed(v: number): string {
  // >= 100 -> integer (100), otherwise one decimal with trailing .0 stripped.
  return v >= 100 ? String(Math.round(v)) : String(Math.round(v * 10) / 10);
}

/** Locale-formatted magnitude (max 1 fraction digit). Comma-decimal locales
 *  render "1,2" instead of "1.2"; the unit token is kept separate by callers. */
function formatMagnitude(v: number, locale?: string): string {
  return new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(v);
}

/** Decimal units: 1 MB = 1,000,000 bytes. e.g. "1.2 MB", "229 MB". The numeric
 *  magnitude follows the OS locale (comma-decimal → "1,2 MB"); the unit-bump and
 *  clamp logic are locale-independent. */
export function formatBytes(n: number, locale?: string): string {
  if (!Number.isFinite(n) || n <= 0) return "0 B";
  let v = n;
  let i = 0;
  while (v >= 1000 && i < BYTE_UNITS.length - 1) {
    v /= 1000;
    i++;
  }
  // Display rounding can hit 1000 (e.g. 999,999 B -> "1000 KB"); bump the unit.
  // Decide on the locale-independent `trimmed` value so the threshold is exact.
  if (Number(trimmed(v)) >= 1000 && i < BYTE_UNITS.length - 1) {
    v /= 1000;
    i++;
  }
  const mag = v >= 100 ? Math.round(v) : Math.round(v * 10) / 10;
  return `${formatMagnitude(mag, locale)} ${BYTE_UNITS[i]}`;
}

/** Local wall-clock time for a unix-millis timestamp, in the OS locale's
 *  12/24h convention (e.g. "14:03" or "2:03 PM"). */
export function formatClock(ms: number, locale?: string): string {
  return new Intl.DateTimeFormat(locale, {
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(ms));
}

/** "Just now", "2m ago", "3h ago", "Yesterday 14:03", "May 2 09:30",
 *  "Dec 31, 2025" — month names and 12/24h follow the OS locale. Recent
 *  timestamps stay relative; older ones become absolute (compact style). */
export function formatRelativeTime(
  ms: number,
  now: number = Date.now(),
  locale?: string,
): string {
  const diff = now - ms;
  if (diff < 60_000) return t("converted.justNow");
  if (diff < 3_600_000) {
    return t("converted.minutesAgo", { count: Math.floor(diff / 60_000) });
  }

  const d = new Date(ms);
  const n = new Date(now);
  const startOfDay = (x: Date) =>
    new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const dayDiff = Math.round((startOfDay(n) - startOfDay(d)) / 86_400_000);

  if (dayDiff <= 0) {
    return t("converted.hoursAgo", { count: Math.floor(diff / 3_600_000) });
  }
  if (dayDiff === 1) {
    return t("converted.yesterday", { time: formatClock(ms, locale) });
  }

  const monthDay = new Intl.DateTimeFormat(locale, {
    month: "short",
    day: "numeric",
  }).format(d);
  if (d.getFullYear() === n.getFullYear()) {
    return `${monthDay} ${formatClock(ms, locale)}`;
  }
  return new Intl.DateTimeFormat(locale, {
    month: "short",
    day: "numeric",
    year: "numeric",
  }).format(d);
}

/** "Jun 12, 2026 · 23:41" for a tooltip — month names and 12/24h follow the OS
 *  locale; em dash when the time is unknown (0). */
export function formatAbsolute(ms: number, locale?: string): string {
  if (!ms) return "—";
  const d = new Date(ms);
  const date = new Intl.DateTimeFormat(locale, {
    month: "short",
    day: "numeric",
    year: "numeric",
  }).format(d);
  const time = formatClock(ms, locale);
  return `${date} · ${time}`;
}

/** Video length: "0:42", "12:05", "1:05:03". Invalid/negative -> "0:00". */
export function formatDuration(secs: number): string {
  const total = Number.isFinite(secs) && secs > 0 ? Math.round(secs) : 0;
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const ss = String(total % 60).padStart(2, "0");
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${ss}`;
  return `${m}:${ss}`;
}

/** "93.9% smaller" — clamped at 0% when output is not smaller. The numeric
 *  magnitude follows the OS locale (comma-decimal → "93,9 % smaller"). */
export function formatPercentSmaller(
  inB: number,
  outB: number,
  locale?: string,
): string {
  if (!(inB > 0)) return t("converted.percentSmaller", { percent: "0" });
  const pct = Math.max(0, (1 - outB / inB) * 100);
  const magnitude = formatMagnitude(Math.round(pct * 10) / 10, locale);
  return t("converted.percentSmaller", { percent: magnitude });
}

/** Grammatical part count: "1 part" / "4 parts" (locale-correct plural). */
export function pluralParts(n: number): string {
  return t("units.parts", { count: n });
}

/** Compact "≈Ns / ≈Nm / ≈Nh" for a by-seconds split interval, rolling up into
 *  minutes/hours above 60/3600 (mirrors formatDuration's tiers) so a 600s
 *  interval reads "≈10m" rather than "≈600s". The unit token (s/m/h) is
 *  localized; the ≈ glyph and the number are locale-independent. */
function approxInterval(seconds: number): string {
  const unit = (key: string, n: number) =>
    `≈${n}${t(`converted.intervalUnit.${key}`)}`;
  if (seconds >= 3600 && seconds % 3600 === 0) return unit("h", seconds / 3600);
  if (seconds >= 60 && seconds % 60 === 0) return unit("m", seconds / 60);
  if (seconds >= 3600) return unit("h", Math.round(seconds / 3600));
  if (seconds >= 60) return unit("m", Math.round(seconds / 60));
  return unit("s", seconds);
}

/**
 * Preset-summary fragment for a split config: "smart split" / "4 parts" /
 * "split ≈30s". Null when splitting is off (or the preset predates splits).
 */
export function splitSummaryLabel(
  split: SplitConfig | undefined,
): string | null {
  if (!split || split.mode === "off") return null;
  if (split.mode === "smart") return t("converted.smartSplit");
  return split.by === "parts"
    ? pluralParts(split.parts)
    : t("converted.splitInterval", { interval: approxInterval(split.seconds) });
}

/** Middle-ellipsis truncation, keeps the extension visible. Counts/cuts over
 *  Unicode code points (not UTF-16 units) so an astral char (emoji) is never
 *  split into a lone surrogate. */
export function truncateMiddle(s: string, max = 36): string {
  const cp = [...s]; // code points, so surrogate pairs stay intact
  if (cp.length <= max) return s;
  const head = Math.ceil((max - 1) / 2);
  const tail = Math.floor((max - 1) / 2);
  return `${cp.slice(0, head).join("")}…${cp.slice(cp.length - tail).join("")}`;
}
