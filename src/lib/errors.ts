// Maps raw backend error strings (from Tauri command rejections) to short,
// sentence-case, actionable toast copy. The raw developer-facing fragments
// ("clipboard task failed: JoinError…", "couldn't open {path}: {e}", "ffmpeg
// -encoders exited with {status}", "failed to clear global shortcuts: {e}") leak
// OS/ffmpeg internals and offer no next step, so we never put them in a toast.
//
// Telemetry stays STRICTLY none: friendly copy is a purely local presentation
// layer. The raw text is logged to the console (local only) for diagnosis and is
// never transmitted anywhere.

import { t } from "../i18n";

/** Friendly fallback key when no known fragment matches. */
const GENERIC_KEY = "errors.generic";

/** Ordered rules: the first whose `match` substring (lower-cased) is found in the
 *  lower-cased raw string wins. Order matters — more specific fragments (e.g.
 *  "file no longer exists") must precede broader ones ("open"/"reveal"). The
 *  match strings are refined against the real backend strings in commands.rs,
 *  encoder/hw.rs, encoder/mod.rs and shortcuts.rs. The matching logic stays
 *  English (it inspects the raw backend fragment); only the OUTPUT copy is
 *  localized via t() so a switched locale translates these toasts live. */
const RULES: ReadonlyArray<{ match: readonly string[]; key: string }> = [
  // commands.rs: `recents scan failed: {e}`
  { match: ["recents scan", "scan failed"], key: "errors.recentsScan" },
  // commands.rs: `File no longer exists at {path}` — must precede reveal/open.
  { match: ["no longer exists"], key: "errors.noLongerExists" },
  // commands.rs / encoder/mod.rs: `clipboard task failed: {e}`, `Couldn't copy to clipboard (…)`.
  {
    match: ["clipboard", "couldn't copy", "copy to clipboard", "copy failed"],
    key: "errors.clipboard",
  },
  // commands.rs: `couldn't reveal {path}: {e}` — before the broader "open" rule.
  { match: ["couldn't reveal", "reveal failed"], key: "errors.reveal" },
  // commands.rs: `couldn't open {path}: {e}`.
  { match: ["couldn't open", "open failed"], key: "errors.open" },
  // encoder/hw.rs + encoder/mod.rs: `ffmpeg … failed`, `ffmpeg -encoders exited with {status}`,
  // `cannot probe ffmpeg encoders`, `failed to run ffmpeg -encoders`.
  { match: ["ffmpeg", "encoder", "exited with"], key: "errors.ffmpeg" },
  // shortcuts.rs: `failed to clear global shortcuts: {e}`, `Invalid "…" shortcut "…": {e}`.
  { match: ["shortcut"], key: "errors.shortcut" },
];

/** Maps an arbitrary thrown/rejected value to friendly, actionable toast copy.
 *  Matching is case-insensitive substring on the raw fragment. The raw value is
 *  logged locally (never the friendly copy) so developers keep full diagnostics;
 *  nothing is transmitted. Always returns a non-empty sentence-case string in
 *  the active locale. */
export function friendlyError(raw: unknown): string {
  // Local-only diagnostics — raw stays in the console, never in the toast.
  console.error(raw);
  const text = String(raw).toLowerCase();
  for (const rule of RULES) {
    if (rule.match.some((m) => text.includes(m))) return t(rule.key);
  }
  return t(GENERIC_KEY);
}

// manual: trigger a failure (e.g. reveal a deleted Converted output) and confirm
// the toast reads as a friendly sentence-case message ("That file is no longer
// there — …"), not the raw lowercase backend fragment, while the raw text still
// appears in the devtools console (local only — nothing is transmitted).
