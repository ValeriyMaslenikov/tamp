# Batch 1 — Critical Correctness Fixes

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development / executing-plans. Checkbox (`- [ ]`) steps.

**Goal:** Fix the three highest-severity correctness defects from the quality audit (`docs/QUALITY-AUDIT-2026-06-16.md`): the drop-on-non-Videos-tab freeze (S1), the cancel-during-Verifying data movement (S2), and the tray icon not tracking live theme changes (S2).

**Branch:** `converted-tree`. **Decisions:** the drop picker should FLOAT over any tab (panel-host mount); a late cancel must be HONORED (discard output, never trash, end Cancelled).

**Conventions:** Frontend (repo root): `C:\Program Files\nodejs` FIRST on PATH; `bunx tsc --noEmit`, `bun run test`. Rust: `%USERPROFILE%\.cargo\bin` on PATH; `cd src-tauri`; `cargo test`, `cargo fmt`, `cargo clippy --all-targets -- -D warnings`. No servers. One commit per task. Do not push. Read the cited code and adapt to the real structure.

---

## Task 1: Drop / picker works on every tab (S1)

**Audit evidence:** `src/lib/dragdrop.ts` is a global webview drop listener → `main.ts` `listView.compressPaths` → in quick-pick mode `list.ts openQuickPickForPaths` does `el.appendChild(overlay)` where `el` is the Videos view root, which `setTab` sets `hidden` on other tabs → the `.quickpick-overlay` is inside a `display:none` subtree (invisible). `quickPick` is then set so `onKeyDown`'s `if (quickPick)` branch swallows ALL keys (it runs before the `el.hidden` guard) and also clashes with the Converted-tab key handler. `setTab` never closes a stale picker.

**Files:** `src/views/list.ts`, `src/main.ts`, `src/views/list.test.ts` (if a pure helper is extracted)

- [ ] **Step 1 — mount the picker on the always-visible panel host.** In `list.ts` `openQuickPickForPaths` (and `openCustom` if it isn't already on `.panel`), append the overlay to the panel host instead of the per-tab `el`. The Custom modal already mounts on `document.querySelector(".panel")` (see `custom.ts` host usage) — mirror that:
  - Replace `el.appendChild(overlay)` with `(document.querySelector(".panel") ?? document.body).appendChild(overlay)`.
  - The `.quickpick-overlay` is `position:absolute; inset:0` (styles.css) — confirm that, relative to `.panel` (which is `position:relative`), it still covers the whole panel. Adjust to cover the panel if needed.

- [ ] **Step 2 — stop the picker keys from clashing with the Converted handler.** In `list.ts onKeyDown`, inside the `if (quickPick) { ... }` branch, after handling each key call `e.stopImmediatePropagation()` (in addition to `e.preventDefault()`), so the Converted view's document `keydown` listener does not also act while the picker is open. (list.ts's listener is registered before converted.ts's, so stopImmediatePropagation on document prevents the later one.) Verify list view is created before converted view in `main.ts` (it is) so ordering holds.

- [ ] **Step 3 — close a stale picker on tab switch.** Expose `closeQuickPick` on the `ListView` interface (return it from `createListView`). In `main.ts` `setTab`, call `listView.closeQuickPick()` at the top (every tab change) so a picker opened by a drop can't linger/resurface.

- [ ] **Step 4 — typecheck + tests.** Run (root): `bunx tsc --noEmit && bun run test` — clean, all pass.

- [ ] **Step 5 — commit** `git add src/views/list.ts src/main.ts && git commit -m "fix: drop/preset-picker works on every tab (mount on panel host, no key clash, close on tab switch)"`

---

## Task 2: Honor a cancel in the post-encode window (S2)

**Audit evidence:** `src-tauri/src/encoder/mod.rs` `run_single` (~527-566) and `run_split_set` (~645-670): after the ffmpeg engine returns `Ok`, the code goes `Phase::Verifying` → `enforce_target` → `run_post_actions` (which trashes the original ~1212-1219 and writes the clipboard ~1190-1210) → `Phase::Done`, with NO `is_cancelled()` recheck. A cancel arriving in this window is only recorded in the cancelled set; `run_job` returns `Ok` so `process_job`'s cancelled-cleanup branch (~307-344) never runs — the job ends Done and the source is trashed.

**Files:** `src-tauri/src/encoder/mod.rs`

- [ ] **Step 1 — read** `run_single`, `run_split_set`, `process_job`, `run_post_actions`, the `is_cancelled` closure, and the cancelled-cleanup branch. Identify the exact point each returns `Ok(output)` from the engine.

- [ ] **Step 2 — re-check cancellation before delivery.** Immediately after the engine returns `Ok` (before setting `Verifying`/calling `enforce_target`/`run_post_actions`) in BOTH `run_single` and `run_split_set`, add an `is_cancelled()` check. If cancelled: remove the just-promoted output file(s) (for a split, the part files + the split folder if now empty), do NOT run post-actions (no trash, no clipboard), and return the same value/path that drives `process_job` into its Cancelled handling (mirror how a cancel detected inside the engine is currently surfaced — return the cancelled outcome, not `Ok(delivered)`). The original must NEVER be trashed once a cancel was observed.

- [ ] **Step 3 — verify the job ends Cancelled.** Trace that the new path makes `process_job` set `Phase::Cancelled` (not Done) and emit it. Add/extend a unit test for the smallest pure piece you can isolate (e.g. a helper `should_deliver(is_cancelled: bool) -> bool` or the cleanup-path computation); the full race is integration-level, so also leave a clear `// manual:` note describing the on-device check (cancel during Verifying → original intact, job Cancelled).

- [ ] **Step 4 — fmt + clippy + tests.** `cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` — clean, all pass, no regressions to existing encoder tests.

- [ ] **Step 5 — commit** `git add src-tauri/src/encoder/mod.rs && git commit -m "fix: honor a cancel in the post-encode window — discard output, never trash the original, end Cancelled"`

---

## Task 3: Tray icon repaints on a live theme change (S2, Windows)

**Audit evidence:** the Windows tray ink (white vs near-black) is chosen from the taskbar theme at icon-update time (`platform/windows.rs` `tray_ink`/`tray_progress`), but nothing repaints the IDLE tray icon when the user flips light/dark while idle, so the glyph can become invisible until the next encode.

**Files:** `src-tauri/src/lib.rs` (and `platform/windows.rs`/`platform/mod.rs` if a trait hook helps)

- [ ] **Step 1 — read** how the app already reacts to theme: `theme.ts` (frontend) and any backend theme handling; the Tauri `WindowEvent::ThemeChanged` / `RunEvent` options; and `tray_progress(app, None)` which repaints the idle tray with the current ink.

- [ ] **Step 2 — repaint the tray on theme change.** Hook the app/window theme-change signal (Tauri's `WindowEvent::ThemeChanged` on the panel window, or the run-event equivalent) and, on it, call `platform::native().tray_progress(app, None)` to repaint the idle icon with the freshly-read taskbar ink. Keep it behind the platform boundary (a no-op consequence on macOS, whose template icon auto-inverts). This is best-effort per the audit's open question — the app-theme event is an acceptable proxy for the taskbar theme; do NOT add a raw WM_SETTINGCHANGE hook.

- [ ] **Step 3 — fmt + clippy + build.** `cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo build` — clean.

- [ ] **Step 4 — commit** `git add -A src-tauri/src && git commit -m "fix: repaint the Windows tray icon on a live light/dark theme change so the idle glyph never goes invisible"`

---

## Self-review notes
- Task 1 moves the picker to the panel host (matches the Custom modal), stops key propagation so it can't fight the Converted handler, and closes stale pickers on tab change — the three sub-causes the audit named.
- Task 2's invariant: once `is_cancelled()` is true, no trash and no clipboard ever run, and the job ends Cancelled; the just-finished output is removed.
- Task 3 is Windows-only behavior behind the Platform boundary; the theme event is the agreed best-effort signal.
