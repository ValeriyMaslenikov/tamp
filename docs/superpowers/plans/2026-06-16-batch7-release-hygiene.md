# Batch 7 — Release hygiene (credential-free distribution polish)

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development / executing-plans. Checkbox (`- [ ]`) steps.

**Goal:** Close the credential-free distribution/releasability gaps from the audit (`docs/QUALITY-AUDIT-2026-06-16.md`, "Releasability & distribution" + the uninstall/onboarding items): a clean uninstall, a tag-driven beta flow with synced versions, first-run onboarding + permission priming, and a beta workflow that fails loudly instead of masking errors.

**Explicitly DEFERRED (do NOT touch — handled in a future session with real certs):** Windows code signing, macOS notarization + hardenedRuntime/entitlements, the Tauri auto-updater, and the macOS Finder Quick Action. This batch must NOT add signing config, an updater plugin/keypair, or entitlements.

**Branch:** `converted-tree`. Mixed Rust + CI/config + frontend. **Verifiability note:** CI workflow (`.github/workflows/*.yml`) and NSIS (`.nsh`) changes are NOT built locally — they're verified by review + syntax + careful reading; they're truly exercised on the next real release. Rust/frontend changes get the usual full verification. The dev app must be stopped before Rust builds (it is).

**Conventions:**
- Rust (`cd src-tauri`): `%USERPROFILE%\.cargo\bin` on PATH; `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
- Frontend (repo root): `C:\Program Files\nodejs` FIRST on PATH; `bunx tsc --noEmit`, `bun run test`.
- No servers, no `gh`/CI triggering, no pushing. One commit per task. Read the cited code/config and adapt.

---

## Task 1: Clean uninstall — drop autostart + caches, keep user data; remove the legacy macOS LaunchAgent

**Audit:** [S3] `nsis-hooks.nsh` (only uninstall logic) deletes ONLY the six context-menu HKCU keys. It never removes the autostart Run value (the autostart plugin writes `HKCU\…\CurrentVersion\Run\tamp` + a `StartupApproved\Run` value, windows.rs:6,8,40), so uninstall-with-launch-at-login leaves a dead Run entry pointing at the removed exe. Caches/logs/journal are also orphaned. [S3] on macOS, startup re-enables autostart under the new id but never removes a pre-rebrand legacy LaunchAgent.

**Decision:** uninstall removes autostart entries + transient caches/logs/thumbnails/previews, but **keeps** `settings.json` + `conversions.json` (reinstall continuity). Best-effort, no prompt.

**Files:** `src-tauri/nsis-hooks.nsh`, `src-tauri/src/lib.rs` (and `src-tauri/src/platform/macos.rs` if the legacy-LaunchAgent cleanup lives behind the platform boundary)

- [ ] **Step 1 — NSIS uninstall: drop the autostart Run entries.** In `NSIS_HOOK_PREUNINSTALL` (or POSTUNINSTALL), in addition to the six context-menu keys, `DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "tamp"` and the corresponding `StartupApproved\Run` value, so an uninstall with Launch-at-login enabled leaves no dead Run entry. Match the exact value name the autostart plugin uses (`tamp`, the product name — verify against `auto-launch`/windows.rs).
- [ ] **Step 2 — NSIS uninstall: remove transient app data.** Delete the cache/log/thumbnail/preview dirs under the app cache + log dirs (the `RMDir /r` on `$LOCALAPPDATA\…\<identifier>` cache + log locations), but DO NOT delete the app-data dir holding `settings.json`/`conversions.json` (keep user settings + history for reinstall continuity). Use the correct per-identifier paths (`io.github.valeriymaslenikov.tamp`). Keep it best-effort (guard each delete).
- [ ] **Step 3 — macOS legacy LaunchAgent cleanup (Rust).** On startup, best-effort remove a stale/legacy LaunchAgent plist before re-enabling autostart under the current id (mirror any existing `migrate_legacy_data` cleanup). Behind `#[cfg(target_os = "macos")]` / the platform boundary; no-op elsewhere. (Per the audit's verifier the current plist name is stable, so this is mainly the legacy-identifier case — keep it defensive and best-effort.)
- [ ] **Step 4 — verify.** Rust: `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` clean. The `.nsh` is not built locally — leave a `// manual:`/comment note describing the on-uninstall check (no Run entry, caches gone, settings/history preserved). Re-read the `.nsh` for correct NSIS syntax (`DeleteRegValue`, `RMDir /r`, `${If}` guards).
- [ ] **Step 5 — commit** `git add src-tauri/nsis-hooks.nsh src-tauri/src/lib.rs src-tauri/src/platform/macos.rs && git commit -m "fix(uninstall): remove the autostart Run entry + transient caches/logs on uninstall (keep settings + history); clean up the legacy macOS LaunchAgent"`

---

## Task 2: Tag-driven beta versioning + Cargo.toml↔package.json sync + CI -beta guard

**Audit:** [S4] `Cargo.toml`/`Cargo.lock` are pinned at `0.1.0` while `package.json` is `0.2.0` — the crate version drifts every release (only package.json is bumped). [S3/S4] the beta flow requires a manual `package.json` bump + a manual revert, and a forgotten revert poisons the next changesets version; `prerelease.yml` triggers on the tag but never sets the version, so the manual edit is load-bearing.

**Decision:** derive the beta version from the tag at build time (no manual bump/revert), sync Cargo.toml from package.json, and add a CI guard so a `-beta` version can't reach `main`.

**Files:** `.github/workflows/prerelease.yml`, `.github/workflows/release.yml`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `package.json`, `scripts/sync-version.ts` (new), `docs/releasing.md`

- [ ] **Step 1 — fix the current drift.** Set `src-tauri/Cargo.toml` `version` (and the `tamp` entry in `Cargo.lock`) to match `package.json` (`0.2.0`). Run `cargo build` so the lockfile is consistent.
- [ ] **Step 2 — version-sync script.** Add `scripts/sync-version.ts` (bun) that reads `package.json` `version` and writes it into `src-tauri/Cargo.toml` (and updates the `tamp` package entry in `Cargo.lock`). Add a `package.json` script (e.g. `"sync-version": "bun scripts/sync-version.ts"`). Keep it dependency-free (bun's built-in file APIs + a targeted regex/`Bun.file`).
- [ ] **Step 3 — tag-driven beta.** In `prerelease.yml`, BEFORE `bun tauri build`, derive `VERSION="${GITHUB_REF_NAME#v}"` and patch `package.json` to that version (e.g. `npm pkg set version="$VERSION"` or a `node`/`bun` one-liner) and run the sync-version script, so the built installers carry the tag's version with **no committed bump and nothing to revert** (the checkout is ephemeral). Update the release-create step accordingly.
- [ ] **Step 4 — CI -beta guard.** In `release.yml` (which runs on `main`/PRs), add a fast step that fails if `package.json`'s `version` contains `-beta` (and optionally if `Cargo.toml` ≠ `package.json`), so a forgotten beta version can never poison the changesets bump on `main`.
- [ ] **Step 5 — docs.** Update `docs/releasing.md` "Beta releases" to the new tag-only flow: "push `vX.Y.Z-beta.N`; CI derives the version from the tag — no package.json edit/revert." Note `bun run sync-version` for the stable flow.
- [ ] **Step 6 — verify.** Rust `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` clean (Cargo.toml version change must not break the build). `bun run sync-version` runs cleanly and leaves Cargo.toml == package.json. Frontend `bunx tsc --noEmit` clean. YAML re-read for syntax. `// manual:` note: the next `-beta` tag builds the right version with no manual edit.
- [ ] **Step 7 — commit** `git add .github/workflows/prerelease.yml .github/workflows/release.yml src-tauri/Cargo.toml src-tauri/Cargo.lock package.json scripts/sync-version.ts docs/releasing.md && git commit -m "build: tag-driven beta versioning (no manual bump/revert), Cargo.toml<->package.json sync, and a CI guard against -beta on main"`

---

## Task 3: First-run onboarding + notification permission priming

**Audit:** [S3] No first-run flow: the tray icon is easy to miss (README warns Windows hides it behind the `^` overflow), the macOS Desktop/notification prompts appear out of context, and a user who reflexively denies notifications permanently loses the stale-recording safety warning with no in-app recovery (`shortcuts.rs:79-95` requests permission lazily and only logs if denied).

**Decision:** a lightweight one-time first-run notice (tray-location hint + permission priming with context), and surface notification-denied in Preferences with a re-request affordance. Keep it minimal — no full welcome wizard.

**Files:** `src-tauri/src/lib.rs`, `src-tauri/src/shortcuts.rs`, `src-tauri/src/commands.rs`, `src/views/preferences.ts`, `src/main.ts`, `src/styles.css`

- [ ] **Step 1 — first-run flag.** Add a persisted first-run marker (a bool in settings, or a sentinel file in the app-data dir). Determine "first run" at startup/first panel open.
- [ ] **Step 2 — one-time notice.** On the first panel open, show a lightweight, dismissible notice in the panel: where the tray/menu-bar icon lives (and on Windows, that it may be under the `^` overflow), and how to reopen (`Ctrl/Cmd+Alt+O`). Dismiss clears the first-run flag so it never shows again. Frontend, styled calmly (reuse the notice styling from Batch 6's folder banner if present).
- [ ] **Step 3 — notification permission priming + recovery.** Prime the notification permission with context (a one-line explanation) before/at first request rather than silently on the first shortcut fire. Add a command to query the current notification permission state, and surface a "Notifications are off — the stale-recording warning won't show. Enable in System Settings" row in Preferences with a re-request / open-settings affordance when denied. Keep platform specifics behind `#[cfg]` where needed; degrade gracefully where a permission API isn't available.
- [ ] **Step 4 — verify.** Rust `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` clean. Frontend `bunx tsc --noEmit && bun run test` clean. `// manual:` note: a fresh profile shows the one-time tray hint once; denying notifications shows the recoverable Preferences row.
- [ ] **Step 5 — commit** `git add -A src-tauri/src src/views/preferences.ts src/main.ts src/styles.css && git commit -m "feat(onboarding): one-time first-run tray hint + notification permission priming with an in-app recovery path in Preferences"`

---

## Task 4: Beta workflow fails loudly + real release notes

**Audit:** [S4] `prerelease.yml:21-25` runs `gh release create … || echo "release already exists"`, masking ALL failures (auth, API, malformed notes, tag mismatch) as a green step; the downstream `gh release upload … --clobber` then errors confusingly or clobbers a stale same-named release. Notes are a hardcoded generic string with no changelog.

**Files:** `.github/workflows/prerelease.yml`

- [ ] **Step 1 — create-if-absent, fail on real errors.** Replace the blanket `|| echo` with an explicit check: query `gh release view "$GITHUB_REF_NAME"` first and only `gh release create` when it's absent; let any other failure (auth/API) surface as a red step. (Equivalent: guard specifically on the already-exists case and re-raise otherwise.)
- [ ] **Step 2 — real notes.** Pull the release notes from `CHANGELOG.md` (the entry for this version) instead of the hardcoded string, keeping the unsigned-bypass note as a footer. If no changelog entry exists for a beta, fall back to the generic note (but keep the bypass line).
- [ ] **Step 3 — verify.** YAML re-read for syntax; trace the create/upload sequence so a genuine create failure is no longer green and an existing release is reused without clobbering unexpectedly. (Not runtime-testable here.) `# note:` comment in the workflow documenting the intended behavior.
- [ ] **Step 4 — commit** `git add .github/workflows/prerelease.yml && git commit -m "ci(beta): create-if-absent with loud failures instead of masking all errors; pull notes from the changelog"`

---

## Self-review notes
- All four tasks are credential-free: no signing, no entitlements, no updater, no Apple/Windows certs — those are a separate future session.
- Order: 1 (uninstall) → 2 (versioning/CI) → 3 (onboarding) → 4 (beta CI). `lib.rs` recurs (Tasks 1, 3); `prerelease.yml` recurs (Tasks 2, 4) — sequential.
- Telemetry stays strictly none: onboarding/permission UI is local; no reporting, no network beyond what already exists.
- CI/NSIS/macOS pieces are review-and-syntax verified here and validated on the next real release; Rust/frontend pieces get full local verification.
