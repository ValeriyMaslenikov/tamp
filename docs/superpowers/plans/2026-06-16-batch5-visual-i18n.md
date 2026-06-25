# Batch 5 — Visual polish + content/i18n

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development / executing-plans. Checkbox (`- [ ]`) steps.

**Goal:** Close the visual-polish and internationalization gaps from the audit (`docs/QUALITY-AUDIT-2026-06-16.md`, "Visual polish & theming" + "Internationalization & content"): light-theme toggles that read as off vs on, graceful name truncation, a scrollable preset picker with themed scrollbars, OS-locale dates/times/numbers, correct plurals, emoji-safe truncation, and friendly error copy.

**Decisions (user, prior):** **follow the OS locale** for dates/numbers (and 12/24h); **scrollable picker** (no hard cap); **friendly mapped error copy**. Distinct success/error toasts and the focus-visible ring already shipped (Batch 2/4) — do not redo them.

**Branch:** `converted-tree`. **Frontend only** (dev can stay stopped). **Conventions:** repo root, `C:\Program Files\nodejs` FIRST on PATH; `bunx tsc --noEmit`, `bun run test`. No servers. One commit per task. Read the cited code (the audit has exact file:line evidence) and adapt.

> Test note for Task 2: `Intl` output is locale-dependent, and vitest runs under the machine/CI locale. Keep production code on the **default** locale (OS), but make the new/updated `format.test.ts` assertions deterministic by passing an **explicit** locale (e.g. `"en-US"`) into the formatter in the test (add an optional `locale` param that defaults to `undefined`), or by asserting locale-independent structure. Never assert a bare `Intl` string that varies by host locale.

---

## Task 1: Visual/theming polish (CSS)

**Audit (Visual):** [S3] light-theme toggle OFF is a #fff knob on a #e6e6ea track — nearly invisible (styles.css:1056/1069). [S3] `.row-name` hard-clips with no `text-overflow:ellipsis` (styles.css:350), unlike `.preset-name`/`.conv-name`/`.drawer-name`. [S3] `.active-bar-name` has no truncation, so a long preset name wraps and grows the bar (styles.css:1609). [S4] `.quickpick` has no `max-height`/overflow — many presets clip top & bottom, unscrollable (styles.css:1654, overlay centers at :1649). [S3] `.drawer-rows` uses the default chunky scrollbar (the slim 6px themed scrollbar at :232-252 only targets `.list-scroll`/`.view-prefs`/`.modal-body`).

**Files:** `src/styles.css` (and `src/views/list.ts` only if the reveal button/badge must move outside the ellipsizing span)

- [ ] **Step 1 — light-theme toggle contrast.** Give the OFF switch state clear separation in light mode: darken the OFF track (e.g. a `--switch-track`/`--switch-knob` semantic var pair, or use `--border-strong`/`--surface-2`) and/or add a subtle border/shadow to the `.track::after` knob so the #fff thumb separates from the track. Verify the ON (accent) state still reads clearly and dark mode is unchanged.
- [ ] **Step 2 — name ellipsis.** Add `text-overflow:ellipsis` to `.row-name` (it already has `white-space:nowrap; overflow:hidden`). Ensure the inline reveal button + "compressed" badge sit outside the ellipsizing text (so they aren't clipped) — adjust the markup in `list.ts` only if needed. Add `white-space:nowrap; overflow:hidden; text-overflow:ellipsis` + a `max-width`/`flex:1; min-width:0` to `.active-bar-name` so long preset names truncate with `…` and the ‹ › arrows stay put.
- [ ] **Step 3 — scrollable picker.** Give `.quickpick` a `max-height` (e.g. `calc(100% - 36px)`) + `overflow-y:auto` (or switch the overlay to `align-items:flex-start` with top padding so it only spills downward) so arbitrarily many presets stay reachable by scroll. Keep it centered/clean for small lists.
- [ ] **Step 4 — themed scrollbars.** Add `.drawer-rows` (and `.quickpick`) to the shared slim 6px scrollbar selector group (styles.css:232-252) — or extract a `.scroll-slim` utility and apply it to all scroll containers. Both themes.
- [ ] **Step 5 — verify.** `bunx tsc --noEmit && bun run test` clean. `// manual:` note: in light mode an OFF toggle is clearly off; long names ellipsize; a 16+ preset picker scrolls with the slim scrollbar; the drawer scrollbar matches the app.
- [ ] **Step 6 — commit** `git add src/styles.css src/views/list.ts && git commit -m "fix(visual): light-mode toggle contrast, name ellipsis, scrollable preset picker, themed drawer/picker scrollbars"`

---

## Task 2: Locale-aware dates/times/numbers, plurals, and surrogate-safe truncation

**Audit (i18n):** [S3] `truncateMiddle` slices UTF-16 code units → splits emoji/surrogate pairs into U+FFFD (format.ts:113-118). [S3] dates hardcode English `MONTHS` + 24h and the Converted tooltip mixes English label (`formatRelativeTime`) with locale-aware body (`formatAbsolute`) (format.ts:7-62, converted.ts label vs Created/Converted rows). [S3] group badge renders ungrammatical "1 parts" (converted.ts ~261). [S4] bytes/percent always use '.' decimal, ignoring comma-decimal locales (format.ts formatBytes/formatPercentSmaller). [S4] split "≈600s" has no minute rollover (format.ts splitSummaryLabel).

**Files:** `src/lib/format.ts`, `src/lib/format.test.ts`, `src/views/converted.ts`

- [ ] **Step 1 — surrogate-safe truncateMiddle.** Truncate over code points (`[...s]` array) instead of `s.length`/`s.slice` so an astral character is never split into a lone surrogate. Keep the middle-ellipsis + extension-preserving behavior and the `max` default.
- [ ] **Step 2 — OS-locale dates/times.** Replace the hardcoded English `MONTHS` and forced 24h: route `formatClock`/`formatRelativeTime`/`formatAbsolute` through `Intl.DateTimeFormat` (and `Intl.RelativeTimeFormat` where it fits) on the **default** locale so month names and 12/24h follow the OS. Make the Converted tooltip consistent — the visible label and the Created/Converted rows must use the same locale convention (no English "Yesterday/May" over locale-aware body). Keep the app's compact style (relative for recent, absolute for old). Add an optional `locale` param (default `undefined`) so tests can pin a locale.
- [ ] **Step 3 — OS-locale numbers.** Format the numeric magnitude in `formatBytes` and `formatPercentSmaller` with `Intl.NumberFormat` (default locale, `maximumFractionDigits:1`), keeping the unit token (`MB`, `% smaller`) separate, so comma-decimal locales render "1,2 MB" / "93,9 % smaller". Preserve the existing unit-bump and clamp logic.
- [ ] **Step 4 — plural + duration rollover.** Add a tiny plural helper (or inline `n === 1 ? "1 part" : `${n} parts``) and use it in `converted.ts` where the group part count renders (and anywhere else a count is shown). In `splitSummaryLabel`, roll by-seconds up into minutes/hours above 60/3600 (mirror `formatDuration` tiers): "≈10m", "≈1h" instead of "≈600s".
- [ ] **Step 5 — tests.** Update `format.test.ts` for the new shapes, using an explicit locale where output is locale-sensitive (Step "Test note"). Cover: truncateMiddle no longer yields a lone surrogate for an emoji-containing name; bytes/percent with an explicit comma-decimal locale render a comma; "1 part" singular; "≈10m" rollover. Keep `formatDuration`/`formatBytes` magnitude logic green.
- [ ] **Step 6 — verify.** `bunx tsc --noEmit && bun run test` clean.
- [ ] **Step 7 — commit** `git add src/lib/format.ts src/lib/format.test.ts src/views/converted.ts && git commit -m "fix(i18n): OS-locale dates/times/numbers, '1 part' plural + minute rollover, surrogate-safe truncation"`

---

## Task 3: Friendly error copy at the IPC boundary

**Audit (i18n):** [S3] ~15 sites do `showToast(String(e), "error")` surfacing raw lowercase developer-facing backend fragments ("clipboard task failed: JoinError…", "couldn't open {path}: {e}", "ffmpeg -encoders exited with {status}", "failed to clear global shortcuts: {e}") — no next step, leaking OS/ffmpeg internals (list.ts:251,527,557,739-740,990,1069; converted.ts:67,69-70,105,116,125,272,290,356; preferences.ts:86,532,649). Telemetry stays **strictly none** — friendly copy is purely local; raw text stays in the log/console, never sent anywhere.

**Files:** `src/lib/errors.ts` (new), `src/views/list.ts`, `src/views/converted.ts`, `src/views/preferences.ts`, `src/lib/drawer.ts`

- [ ] **Step 1 — mapping module.** Create `src/lib/errors.ts` exporting `friendlyError(raw: unknown): string` that maps known backend error kinds to short, sentence-case, actionable copy and falls back to a generic message. Starter catalog (match case-insensitively on the raw fragment; refine against the real backend strings in `commands.rs`/`hw.rs`/`shortcuts.rs`):
  - recents/scan failed → "Couldn't refresh your recordings. Try again."
  - clipboard / copy failed → "Couldn't copy the file. Try again, or reveal it in your file manager."
  - couldn't open / open failed → "Couldn't open the file — it may have been moved or deleted."
  - couldn't reveal / reveal failed → "Couldn't show the file in your file manager."
  - "file no longer exists" (Batch 4 reveal) → "That file is no longer there — it may have been moved or deleted."
  - ffmpeg / encoder / "exited with" → "The video tool hit an error. Check the logs (tray → Open Logs) for details."
  - shortcut / global shortcut → "Couldn't update the keyboard shortcut."
  - default → "Something went wrong. Check the logs (tray → Open Logs) for details."
  Keep the raw string out of the toast but available for diagnosis (e.g. `console.error(raw)`), per the strictly-no-telemetry rule (local only).
- [ ] **Step 2 — apply at call sites.** Replace `showToast(String(e), "error")` (and `String(err)`) at the ~15 cited sites with `showToast(friendlyError(e), "error")`. Leave already-friendly, intentional messages (e.g. the trash-multi-preset guidance, "Copied to clipboard") untouched. Do not change success toasts.
- [ ] **Step 3 — tests.** Add `src/lib/errors.test.ts`: each known fragment maps to its friendly copy; an unknown string falls back to the generic message; mapping is case-insensitive.
- [ ] **Step 4 — verify.** `bunx tsc --noEmit && bun run test` clean. `// manual:` note: trigger a failure (e.g. reveal a deleted output) → a friendly sentence-case toast, not a raw lowercase fragment.
- [ ] **Step 5 — commit** `git add src/lib/errors.ts src/lib/errors.test.ts src/views/list.ts src/views/converted.ts src/views/preferences.ts src/lib/drawer.ts && git commit -m "fix(content): map raw backend errors to friendly, actionable toast copy (local only, no telemetry)"`

---

## Self-review notes
- Already-shipped visual items (focus-visible ring, toast success/error variants, panel-host picker) are intentionally excluded — Batch 2/4 covered them.
- Order: 1 (CSS) → 2 (format.ts/converted.ts) → 3 (errors.ts + view call sites). Sequential; `converted.ts`/`list.ts` recur across tasks.
- i18n decision is "follow the OS locale" — code uses the default locale; tests pin an explicit locale for determinism.
- Telemetry stays strictly none: friendly copy is a local presentation layer; raw error text stays in the local log/console and is never transmitted.
