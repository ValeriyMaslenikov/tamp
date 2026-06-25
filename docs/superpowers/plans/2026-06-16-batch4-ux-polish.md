# Batch 4 — UX polish

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development / executing-plans. Checkbox (`- [ ]`) steps.

**Goal:** Fix the UX-rough edges from the audit (`docs/QUALITY-AUDIT-2026-06-16.md`): the Converted tab losing your place on every refresh (and leaking a tooltip), a failed queue row swallowing the retry click, the toast landing on top of the activity drawer, Play/Reveal giving no feedback (and reveal never surfacing real failures), and the drawer growing unbounded with no cancel feedback.

**Decisions (user, prior):** failed-row click **retries the same preset**; refresh **preserves selection + expansion** (auto-refresh stays, just non-disruptive); toasts **stack above** the drawer; Play/Reveal get a **pressed/confirmation** signal and reveal **surfaces real failures**.

**Branch:** `converted-tree`. Mostly frontend; **Task 4 touches Rust** (`reveal` return type) → the dev app must be stopped before building (it is).

**Conventions:**
- Frontend (repo root): `C:\Program Files\nodejs` FIRST on PATH; `bunx tsc --noEmit`, `bun run test`.
- Rust (`cd src-tauri`): `%USERPROFILE%\.cargo\bin` on PATH; `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
- No servers. One commit per task. Read the cited code (the audit gives exact file:line evidence) and adapt.

---

## Task 1: Converted tab keeps your place across refresh (+ no leaked tooltip, + first-load state)

**Audit:** [S3] `converted.ts` `refresh()` (~351-373) does `scroll.innerHTML = ""` then unconditionally `selEl = null; setSelected(navRows()[0])` — discarding the keyboard selection and every expanded split group on each refresh, which fires on every tab switch (`main.ts:87`) and every finished encode while the tab is open (`main.ts:106-113`). Also [S3] the body-attached time tooltip (`timeEl()` ~131-153) leaks: `innerHTML=""` removes the row without firing `mouseleave`, orphaning the fixed tip permanently.

**Files:** `src/views/converted.ts`, `src/main.ts`

- [ ] **Step 1 — capture state before rebuild.** In `refresh()`, before `scroll.innerHTML=""`, snapshot: the selected row's identity (the output path of the selected single, or the folder of the selected group — whatever key uniquely identifies a `navRow`) and the set of expanded group folders.
- [ ] **Step 2 — restore after rebuild.** After rebuilding rows, re-expand the groups whose folder was in the saved set, then re-select the row matching the saved identity (and `scrollIntoView` it). Only fall back to `setSelected(navRows()[0])` when there was **no prior selection** (first load / previously empty). Do not yank to top when a background conversion completes.
- [ ] **Step 3 — kill the tooltip leak.** Track the active body-attached tip in a module/closure variable; remove it at the top of `refresh()` (and on view teardown) so a refresh while hovering a time value can't orphan it. (Or re-parent the tip as a child positioned via CSS — but the tracked-and-removed approach is smallest.)
- [ ] **Step 4 — first-load affordance (minor).** When `refresh()` is the first load (no rows yet) and `listConversions()` is still pending, show a lightweight loading/skeleton or "Loading…" placeholder instead of a blank scroll; clear it when results arrive. Keep prior content during subsequent refreshes (already the case).
- [ ] **Step 5 — verify.** `bunx tsc --noEmit && bun run test` clean. `// manual:` note: scroll down, select a row, expand a group; trigger a refresh (tab away/back or finish a conversion) → selection and expansion are preserved, no stray popover.
- [ ] **Step 6 — commit** `git add src/views/converted.ts src/main.ts && git commit -m "fix(converted): preserve selection + expanded groups across refresh, stop the leaked time tooltip, add a first-load state"`

---

## Task 2: A failed queue row retries the same preset on click

**Audit:** [S3] `list.ts` `onRowClick` (~746-759): for a failed job it calls `dismiss(j); return` (dismiss ~174-183 only clears the error + re-renders), so the primary click on a failed row is a no-op that just erases the red error — never re-running. Non-failed rows dismiss then re-enqueue.

**Files:** `src/views/list.ts`

- [ ] **Step 1 — retry on click.** For a failed job, instead of `dismiss(j); return`, dismiss the error state AND re-run the same conversion with the **same preset** the job used (re-enqueue `j.inputPath` with `j.presetId`). Reuse the existing enqueue path the non-failed branch uses; do not open the picker (the decision is "retry the same preset"). Guard against a missing/now-invalid preset id (fall back to opening the picker only if the preset no longer exists).
- [ ] **Step 2 — make it discoverable.** Give the failed row a clear affordance that a click retries — e.g. a "Retry" label/cursor affordance or tooltip on the failed row (keep it lightweight; the click behavior is the substance).
- [ ] **Step 3 — verify.** `bunx tsc --noEmit && bun run test` clean. `// manual:` note: force a failure, click the row → it re-runs with the same preset (not a silent dismiss).
- [ ] **Step 4 — commit** `git add src/views/list.ts && git commit -m "fix(videos): clicking a failed row retries the same preset instead of silently dismissing"`

---

## Task 3: Toast stacks above the activity drawer

**Audit:** [S3] toast (`styles.css` ~1555-1560) and drawer (~1416-1421) both `position:absolute` bottom, full width, `z-index:50`, with no coordination — so any toast (incl. the drawer's own "Copied") lands on top of the drawer while a conversion runs.

**Files:** `src/lib/drawer.ts`, `src/lib/toast.ts`, `src/styles.css`

- [ ] **Step 1 — drawer publishes its height.** In `drawer.ts` `render()`, when the drawer is visible set a CSS custom property on the panel (e.g. `panel.style.setProperty("--drawer-h", el.offsetHeight + "px")`); when hidden set it to `0px`. (Measure after the rows are in the DOM.)
- [ ] **Step 2 — toast offsets above it.** In `styles.css`, change the toast's `bottom` to `calc(var(--drawer-h, 0px) + <existing gap>)` (keep the existing 12px as the gap, plus a small extra so it clears the drawer). The toast now floats just above the drawer when one is visible and sits at the normal spot otherwise. Confirm `--drawer-h` is read on `.panel` (set the var on the same element the toast is positioned relative to / a shared ancestor).
- [ ] **Step 3 — verify.** `bunx tsc --noEmit && bun run test` clean. `// manual:` note: start a conversion (drawer visible) then trigger a toast (copy/drop-error) → the toast sits above the drawer, neither occludes the other.
- [ ] **Step 4 — commit** `git add src/lib/drawer.ts src/lib/toast.ts src/styles.css && git commit -m "fix(ux): stack the toast above the activity drawer instead of overlapping it"`

---

## Task 4: Play/Reveal feedback + reveal surfaces real failures

**Audit:** [S3] Play/Reveal are silent on success (`converted.ts` play/openFile ~67/103-107, reveal ~70/121-128; `list.ts` reveal ~990; `drawer.ts` reveal ~110), so on a slow/backgrounded launch the user re-clicks and spawns duplicates. [S3 §345/349] worse, `commands::reveal` returns `()` and only logs server-side, so the frontend `.catch` can only fire on an IPC transport error — revealing a moved/deleted output does nothing and shows nothing.

**Files:** `src-tauri/src/commands.rs`, `src/lib/ipc.ts`, `src/views/converted.ts`, `src/views/list.ts`, `src/lib/drawer.ts`

- [ ] **Step 1 — reveal returns a Result.** Change `commands::reveal` to return `Result<(), String>` (mirror `open_file`/`copy_file`): on a reveal/`reveal_item_in_dir` error (or a pre-check that the path no longer exists), return `Err("File no longer exists at <path>")`-style message instead of swallowing. Update the command registration if the signature type matters. Update `ipc.ts` `reveal` to `Promise<void>` that rejects (it already returns `invoke<void>`, which rejects on `Err`) — no TS change needed beyond confirming callers `.catch`.
- [ ] **Step 2 — success/acknowledge feedback.** Give Play and Reveal an immediate acknowledgement that survives the panel hiding on blur: a brief pressed/active visual on the button (CSS `:active`-style class toggled for ~150-300ms on activate) at every call site (`converted.ts`, `list.ts`, `drawer.ts`). Keep it subtle — do NOT add a toast on every reveal/play (the audit notes that's too noisy given the panel usually hides). Errors still toast (now including real reveal failures from Step 1).
- [ ] **Step 3 — verify.** Frontend `bunx tsc --noEmit && bun run test` clean; Rust `cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` clean. `// manual:` note: reveal a deleted output → a "no longer exists" toast now appears; pressing Play/Reveal shows a pressed state even as the panel hides.
- [ ] **Step 4 — commit** `git add src-tauri/src/commands.rs src/lib/ipc.ts src/views/converted.ts src/views/list.ts src/lib/drawer.ts && git commit -m "fix(ux): reveal surfaces real failures (Result) and Play/Reveal show a pressed-state acknowledgement"`

---

## Task 5: Drawer batch summary — collapse queued, cap rows, cancel feedback

**Audit:** [S4] `drawer.ts` `render()` lists every job, so dropping 30 files makes a 30-row scroll covering the list; and cancelling the only running job makes the drawer vanish instantly (`updateJob` cancelled branch ~144-154 → empty → `el.hidden=true`) with no confirmation.

**Files:** `src/lib/drawer.ts`, `src/styles.css`

- [ ] **Step 1 — collapse queued into a summary.** In `render()`, when there are many queued jobs, render a single "N queued" summary row instead of one row each (optionally expand on demand). Keep running (pass1/pass2/verifying) and done/failed rows individual.
- [ ] **Step 2 — cap visible rows.** Cap the number of individual rows shown (e.g. running + most-recent done) and add a "+N more" line rather than an unbounded scroll. Pick a sensible cap (e.g. 6) and keep the title summary (`running`/`done` counts) accurate over the full set.
- [ ] **Step 3 — cancel feedback.** When cancelling the last job empties the drawer, show a brief "Cancelled" flash (a transient row/state for ~1.2s) before it clears, instead of an instant disappearance, so the user sees it was their cancel (not a crash). (Use a short timeout that then hides; ensure it doesn't resurrect on further updates.)
- [ ] **Step 4 — verify.** `bunx tsc --noEmit && bun run test` clean. `// manual:` note: drop many files → "N queued" summary + capped rows; cancel the only job → brief "Cancelled" then clear.
- [ ] **Step 5 — commit** `git add src/lib/drawer.ts src/styles.css && git commit -m "feat(drawer): collapse queued into a summary, cap visible rows, and flash 'Cancelled' before clearing"`

---

## Self-review notes
- Task 1 is the headline fix (don't lose the user's place); Tasks 2–5 remove the smaller papercuts the audit graded S3/S4.
- Order: 1 (converted/main) → 2 (list) → 3 (drawer/toast/styles) → 4 (commands/ipc/views/drawer) → 5 (drawer/styles). Sequential — `main.ts`, `drawer.ts`, `styles.css`, `list.ts`, `converted.ts` overlap across tasks.
- Telemetry stays strictly none: failures surface only via the existing local toast/log, never reported anywhere.
- No new dependencies; all changes are within existing modules and the existing IPC surface (only `reveal`'s return type widens to `Result`).
