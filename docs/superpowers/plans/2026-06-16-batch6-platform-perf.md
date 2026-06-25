# Batch 6 — Platform & performance

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development / executing-plans. Checkbox (`- [ ]`) steps.

**Goal:** Close the platform-robustness and performance gaps from the audit (`docs/QUALITY-AUDIT-2026-06-16.md`, "Platform & filesystem" + "Performance & resources"): instant panel open (no blocking on full-list thumbnail/duration generation), a shared probe cache, a visible signal for unreachable watched folders, reject-don't-mangle for non-UTF-8 filenames, Windows long-path handling, all-or-nothing context-menu registration, and moving the journal write off the encode worker.

**Already done by prior batches (do NOT redo):** tray live-theme repaint (Batch 1), `reveal` returns `Result` (Batch 4), atomic journal write + one-append-per-split-job (Batch 3).

**Branch:** `converted-tree`. **Mostly Rust backend** → the dev app must be stopped before building (it is). **Conventions:**
- Rust (`cd src-tauri`): `%USERPROFILE%\.cargo\bin` on PATH; `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
- Frontend (repo root): `C:\Program Files\nodejs` FIRST on PATH; `bunx tsc --noEmit`, `bun run test`.
- No servers. One commit per task. The audit has exact file:line evidence — read it + the cited source and adapt.

---

## Task 1: Instant panel open — non-blocking recents, lazy per-row thumbnails & durations

**Audit:** [S2 perf] `commands.rs:44-45` `list_recents()` awaits `ensure_thumbs` then `durations::fill` (sequential, ffmpeg/ffprobe-per-miss @4) before returning, so a cold cache shows an empty panel for many seconds (up to 200 videos). The Converted tab already lazy-loads thumbs per row (`conversion_thumb`). Also [S3 perf] the Videos list eagerly sets every `img.src` with no lazy loading.

**Decision:** instant open with progressively-filling thumbnails (correct for a menu-bar app). Mirror the Converted tab's lazy per-row pattern.

**Files:** `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`, `src/lib/ipc.ts`, `src/views/list.ts`

- [ ] **Step 1 — return rows immediately.** Make `list_recents` return the scanned rows with `thumb_path = None` and `duration_secs = None` (do NOT call `ensure_thumbs`/`durations::fill` inline). Keep the orphan-annotation backfill (journal lookup) which is cheap.
- [ ] **Step 2 — per-row IPC.** Add two commands mirroring `conversion_thumb`: `recent_thumb(path) -> Result<Option<String>, String>` (ensure+return one thumbnail) and `recent_duration(path) -> Result<Option<f64>, String>` (probe+cache one duration via the duration cache). Register both in `lib.rs`. Add `recentThumb`/`recentDuration` to `ipc.ts`.
- [ ] **Step 3 — lazy load in the Videos list.** In `list.ts`, after building rows, lazy-load each row's thumbnail and duration via an `IntersectionObserver` (mirror the Converted tab's approach): when a row scrolls near the viewport, call `recentThumb`/`recentDuration` and fill the `<img>`/duration text when they resolve. Set `loading="lazy"` on the row `<img>`. Rows render instantly with a placeholder; media fills in.
- [ ] **Step 4 — verify.** Rust: `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`. Frontend: `bunx tsc --noEmit && bun run test`. Adapt any list/recents tests to the new None-on-first-return contract. `// manual:` note: cold-cache panel open is instant; thumbnails/durations stream in as you scroll.
- [ ] **Step 5 — commit** `git add src-tauri/src/commands.rs src-tauri/src/lib.rs src/lib/ipc.ts src/views/list.ts && git commit -m "perf: open the panel instantly — return recents without blocking on thumbnails/durations, lazy-load both per row"`

---

## Task 2: Shared probe cache (compute once, reuse for duration/preview/encode)

**Audit:** [S3 perf] No shared probe cache. `durations::fill` (durations.rs:164), `previews::generate` (previews.rs:126), and `run_job` (encoder/mod.rs:362,429) each run `probe::probe()` on the same file; only durations persists a bare `f64`. No-duration WebMs re-trigger a full packet-scan demux each time. Related [S3 robustness, audit ~line 249]: on a probe `Err`, the duration cache currently memoizes the failure — cache only successful probes so a later encode re-probes.

**Files:** `src-tauri/src/probe.rs` (or a new `src-tauri/src/probe_cache.rs`), `src-tauri/src/durations.rs`, `src-tauri/src/previews.rs`, `src-tauri/src/encoder/mod.rs`

- [ ] **Step 1 — ProbeInfo cache.** Introduce a small process-lifetime in-memory cache (e.g. `Mutex<HashMap<CacheKey, ProbeInfo>>`) keyed by the same `path | mtime | size` key the duration cache already uses, storing the full `ProbeInfo` (duration, width, height, fps, has_audio). Provide `probe_cached(path) -> Result<ProbeInfo, _>` that returns the cached value or probes once and stores it. Only cache **successful** probes — never memoize an `Err` (so a transient failure or a since-fixed file re-probes).
- [ ] **Step 2 — consume everywhere.** Route `durations::fill`, `previews::generate`, and the encoder's probe sites (encoder/mod.rs:362,429) through `probe_cached`. Keep the on-disk `durations.json` behavior for the persisted duration; the in-memory cache covers the within-session geometry/audio reuse.
- [ ] **Step 3 — verify.** `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` clean. Add a unit test: a second `probe_cached` for the same key returns without re-invoking the probe fn (inject/stub the probe call, or assert via a call-counter); an `Err` is not cached. `// manual:` note: opening → hovering → converting one file spawns one ffprobe, not three.
- [ ] **Step 4 — commit** `git add -A src-tauri/src && git commit -m "perf: shared in-memory probe cache (duration/geometry/audio) consumed by durations, previews, and the encoder; never cache a failed probe"`

---

## Task 3: Surface unreachable watched folders

**Audit:** [S3 platform] `scanner.rs:74-77` swallows every non-NotFound `read_dir` error (offline UNC, permission-denied) as `log_warn! + continue`, returning a successful possibly-empty Vec; `list_recents` carries no per-folder status, so the UI shows the misleading empty-state ("No videos… record something!") with no hint a folder is unreachable.

**Files:** `src-tauri/src/scanner.rs`, `src-tauri/src/commands.rs`, `src/lib/ipc.ts`, `src/views/list.ts`, `src/styles.css`

- [ ] **Step 1 — collect reachability.** Have the scan distinguish a legitimately-empty/NotFound folder from an unreachable one (permission-denied / offline). Return the list of unreachable folder paths alongside the recents (e.g. a separate command `unreachable_folders() -> Vec<String>`, or widen `list_recents`' return — pick the smaller change and keep the recents Vec shape stable for existing callers).
- [ ] **Step 2 — inline banner.** In the Videos tab, when one or more watched folders are unreachable, show a distinct inline banner/row ("Couldn't read <folder> — it may be offline or you lack permission"), separate from the legitimately-empty state. Style it as a non-alarming notice. Update `ipc.ts`.
- [ ] **Step 3 — verify.** Rust `cargo fmt && clippy -D warnings && cargo test` (add a scanner test: a permission-denied/unreadable dir is reported unreachable, a missing dir is NOT). Frontend `bunx tsc --noEmit && bun run test`. `// manual:` note: add an offline UNC path → the Videos tab shows the unreachable banner, not "no recordings".
- [ ] **Step 4 — commit** `git add -A src-tauri/src/scanner.rs src-tauri/src/commands.rs src/lib/ipc.ts src/views/list.ts src/styles.css && git commit -m "fix(platform): surface unreachable watched folders (offline/permission) as a distinct banner instead of a misleading empty list"`

---

## Task 4: Reject (don't mangle) non-UTF-8 filenames

**Audit:** [S3 platform] `scanner.rs:116` builds `RecentVideo.path` with `to_string_lossy()`, so a non-UTF-8 filename gets U+FFFD replacement chars before the `paths_to_utf8` "reject don't mangle" guard (platform/mod.rs:77) ever runs — the laundered string is valid UTF-8, the check never fires, and probe/copy/reveal then act on a wrong/nonexistent path.

**Decision:** declare non-UTF-8 recording filenames unsupported — **skip + warn** at scan time with a visible reason, rather than silently mangling.

**Files:** `src-tauri/src/scanner.rs`, `src/views/list.ts` (optional surface)

- [ ] **Step 1 — reject at scan.** In the scanner, when a candidate path is not valid UTF-8 (`path.to_str().is_none()`), do NOT emit a lossy `RecentVideo`; skip it and record it as a distinct "unsupported filename" condition (count or a small list), so the lossy string never enters the IPC round-trip.
- [ ] **Step 2 — visible reason (light).** If feasible without much surface, surface a small notice when ≥1 file was skipped for an unsupported (non-UTF-8) filename (e.g. fold into the unreachable/notice area from Task 3, or a count in the empty/secondary state). Keep it minimal — the core fix is not mangling.
- [ ] **Step 3 — fix the doc.** Update the `paths_to_utf8` doc comment (platform/mod.rs) so it no longer claims a guarantee that scan-time rejection now actually upholds (or note rejection happens upstream at scan).
- [ ] **Step 4 — verify.** `cargo fmt && clippy -D warnings && cargo test` (add a scanner test on a synthetic non-UTF-8 OsString path where the platform allows it, or a unit test of the is-valid-UTF-8 gate). Frontend clean. `// manual:` note: a file with a non-UTF-8 name is skipped with a notice, never mangled into a broken row.
- [ ] **Step 5 — commit** `git add -A src-tauri/src/scanner.rs src-tauri/src/platform/mod.rs src/views/list.ts && git commit -m "fix(platform): reject (don't mangle) non-UTF-8 filenames at scan time and note them, instead of laundering them through to_string_lossy"`

---

## Task 5: Windows long-path (>260) handling for outputs and split folders

**Audit:** [S3~ platform] Output paths are built by naive `PathBuf::join`/`format!` (plan.rs:511-552) with no `\\?\` verbatim prefixing. A split adds a folder level + up to 24-char label + a `.part` sibling, so a recording already near MAX_PATH in a deep OneDrive/synced tree pushes the output/part past 260, ffmpeg fails, and the user sees a generic "cannot move finished output into place".

**Decision:** target default-off-LongPaths Windows machines — prefix Windows output/temp/split paths with the `\\?\` verbatim form (prefer the `dunce` crate already in the tree, or `std`'s verbatim handling) before writing, and emit an actionable error when a path is still too long.

**Files:** `src-tauri/src/encoder/plan.rs`, `src-tauri/src/encoder/mod.rs` (and `Cargo.toml` only if `dunce` must be promoted from a transitive to a direct dep)

- [ ] **Step 1 — verbatim-prefix on write.** Before ffmpeg writes (the output path, the `.part` temp, and the split part folder), convert the Windows path to its `\\?\` verbatim/canonical form (use `dunce::canonicalize` where the parent exists, or construct the verbatim prefix for the not-yet-created output). Keep this `#[cfg(windows)]`; no-op on other platforms. Confirm ffmpeg accepts the verbatim path (it does on Windows for absolute paths).
- [ ] **Step 2 — actionable over-length error.** When an intended output/part path would exceed the limit even with verbatim handling unavailable, surface a clear "output path is too long — try a shorter folder" error from the promote/prepare step (mod.rs:638-642/1011) instead of the generic move-failure message.
- [ ] **Step 3 — verify.** `cargo fmt && clippy -D warnings && cargo test` clean. Add a unit test for the path-length detection / verbatim-prefix helper (pure, no ffmpeg). `// manual:` note (Windows): a recording in a deeply-nested path compresses/splits instead of failing; an unavoidably-too-long path gives the actionable error.
- [ ] **Step 4 — commit** `git add -A src-tauri/src/encoder && git commit -m "fix(platform,windows): verbatim (\\\\?\\) long-path handling for outputs/temp/split folders + actionable over-length error"`

---

## Task 6: All-or-nothing context-menu registration + journal write off the encode worker

**Audit:** [S3 robustness, audit ~line 229] `context_menu` register/unregister loops over 6 extensions and a mid-loop failure leaves a partial registration with the UI toggle reverted — not atomic. [S3 perf] `journal.rs` `append()` calls `persist()` (blocking full-file `write`) synchronously on the encode worker.

**Files:** `src-tauri/src/platform/context_menu.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/journal.rs`

- [ ] **Step 1 — atomic context-menu registration.** Make `set_context_menu(true)` all-or-nothing: if registering any of the 6 extension keys fails, roll back the ones already written (delete them) and return an `Err` so the UI toggle reflects the true (off) state rather than a partial registration. Mirror for unregister (best-effort delete all, report if any failed). Keep it `#[cfg(windows)]`.
- [ ] **Step 2 — journal write off-worker.** Move the journal's blocking disk write off the encode worker: in `append()` (now called once per job after Batch 3), do the serialize+write on a blocking task (`tauri::async_runtime::spawn_blocking` or a dedicated thread) so the worker isn't blocked on `fs::write`. Preserve the atomic temp+rename, dedup, cap, and in-memory state semantics exactly (the in-memory Vec update stays synchronous under the lock; only the disk write is offloaded). Keep `load_from_path`/tests working (they call `append` directly and expect persistence to be observable on reload — ensure the test path still flushes, e.g. await/join the write in tests or keep a synchronous fallback when no async runtime is present).
- [ ] **Step 3 — verify.** `cargo fmt && clippy -D warnings && cargo test` clean — including the existing journal tests (append→reload must still see the record). Add a context-menu unit test for the rollback path if the registry calls are stubbable; otherwise a `// manual:` note (toggle on with a simulated mid-loop failure leaves no partial registration).
- [ ] **Step 4 — commit** `git add -A src-tauri/src && git commit -m "fix(platform): atomic all-or-nothing context-menu registration; perf: move the journal disk write off the encode worker"`

---

## Self-review notes
- Task 1 (instant open) is the headline S2 perf fix; Task 2 (probe cache) removes redundant ffprobe spawns; Tasks 3–6 are platform-robustness edge cases the audit graded S3.
- Order: 1 → 2 → 3 → 4 → 5 → 6. Sequential — `commands.rs`, `scanner.rs`, `encoder/mod.rs`, `journal.rs`, `list.ts`, `ipc.ts` recur across tasks.
- Telemetry stays strictly none: unreachable/unsupported-filename notices are purely local UI; no reporting.
- Keep every change behind the `Platform` trait where OS-specific (long-path, context-menu are `#[cfg(windows)]`); macOS/Linux compile to no-ops.
