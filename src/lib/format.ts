// Pure formatting helpers — unit tested in format.test.ts.

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

const MONTHS = [
  "Jan", "Feb", "Mar", "Apr", "May", "Jun",
  "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
] as const;

function trimmed(v: number): string {
  // >= 100 -> integer ("229"), otherwise one decimal with trailing .0 stripped ("9.4", "1")
  return v >= 100 ? v.toFixed(0) : (Math.round(v * 10) / 10).toString();
}

/** Decimal units: 1 MB = 1,000,000 bytes. e.g. "1.2 MB", "229 MB". */
export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "0 B";
  let v = n;
  let i = 0;
  while (v >= 1000 && i < BYTE_UNITS.length - 1) {
    v /= 1000;
    i++;
  }
  let text = trimmed(v);
  // Display rounding can hit 1000 (e.g. 999,999 B -> "1000 KB"); bump the unit.
  if (Number(text) >= 1000 && i < BYTE_UNITS.length - 1) {
    v /= 1000;
    i++;
    text = trimmed(v);
  }
  return `${text} ${BYTE_UNITS[i]}`;
}

/** Local wall-clock "HH:MM" (24h) for a unix-millis timestamp. */
export function formatClock(ms: number): string {
  const d = new Date(ms);
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  return `${hh}:${mm}`;
}

/** "Just now", "2m ago", "3h ago", "Yesterday 14:03", "May 2 09:30", "Dec 31, 2025". */
export function formatRelativeTime(ms: number, now: number = Date.now()): string {
  const diff = now - ms;
  if (diff < 60_000) return "Just now";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;

  const d = new Date(ms);
  const n = new Date(now);
  const startOfDay = (x: Date) =>
    new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const dayDiff = Math.round((startOfDay(n) - startOfDay(d)) / 86_400_000);

  if (dayDiff <= 0) return `${Math.floor(diff / 3_600_000)}h ago`;
  if (dayDiff === 1) return `Yesterday ${formatClock(ms)}`;

  const datePart = `${MONTHS[d.getMonth()]} ${d.getDate()}`;
  if (d.getFullYear() === n.getFullYear()) return `${datePart} ${formatClock(ms)}`;
  return `${datePart}, ${d.getFullYear()}`;
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

/** "93.9% smaller" — clamped at 0% when output is not smaller. */
export function formatPercentSmaller(inB: number, outB: number): string {
  if (!(inB > 0)) return "0% smaller";
  const pct = Math.max(0, (1 - outB / inB) * 100);
  return `${Math.round(pct * 10) / 10}% smaller`;
}

/** Middle-ellipsis truncation, keeps the extension visible. */
export function truncateMiddle(s: string, max = 36): string {
  if (s.length <= max) return s;
  const head = Math.ceil((max - 1) / 2);
  const tail = Math.floor((max - 1) / 2);
  return `${s.slice(0, head)}…${s.slice(s.length - tail)}`;
}
