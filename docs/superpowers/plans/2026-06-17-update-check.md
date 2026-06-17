# Update check — automatic, opt-in, unannoying

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development / executing-plans. Checkbox (`- [ ]`) steps.

**Goal:** When enabled, tamp quietly asks GitHub for the latest version on launch and, the next time the panel opens, shows a **gentle, dismissible modal** if a newer version exists — no OS notifications, never nagging (dismiss remembers the version). Opt-in on first run, togglable in Preferences. The only outbound request; sends no data about the user.

**Decisions (user):** automatic startup check; **ask on first run** (consent in the welcome notice); a Preferences toggle to turn it off for privacy; **modal on panel open** (not an OS notification); maximally unannoying — one quiet card per new version, dismiss = remembered. While tamp is pre-1.0, "latest" **includes prereleases** so beta testers hear about newer betas.

**Architecture:** the network call lives in **Rust** (a single GET via a minimal client) so no CSP widening is needed and the call is gated server-side. The frontend only calls it when the setting is on.

**Branch:** `converted-tree`. Builds on Batch 7's onboarding (`src/lib/onboarding.ts`) + `onboarding_seen` setting. **Touches Rust + frontend** → dev stopped (it is). **Conventions:**
- Rust (`cd src-tauri`): `%USERPROFILE%\.cargo\bin` on PATH; `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
- Frontend (repo root): `C:\Program Files\nodejs` FIRST on PATH; `bunx tsc --noEmit`, `bun run test`.
- No servers, no pushing. One commit per task. **Telemetry stays strictly none** — the check is a pull to a public API; it sends nothing about the user or their usage.

---

## Task 1: Backend — settings fields + the GitHub version check

**Files:** `src-tauri/Cargo.toml`, `src-tauri/src/settings.rs`, `src-tauri/src/commands.rs` (or a new `src-tauri/src/update_check.rs`), `src-tauri/src/lib.rs`

- [ ] **Step 1 — settings fields.** Add to `Settings` (camelCase serde, both with `#[serde(default)]` so old stores load):
  - `update_check_enabled: bool` (default **false** — off until the first-run consent or the Preferences toggle turns it on).
  - `last_dismissed_update_version: Option<String>` (default `None`) — the newest version the user has already dismissed, so the modal never re-nags for it.
  Mirror both in the TS `Settings` interface (`src/lib/ipc.ts`) — `updateCheckEnabled: boolean`, `lastDismissedUpdateVersion: string | null`.
- [ ] **Step 2 — deps.** Add a minimal HTTP client + semver to `Cargo.toml`: prefer `ureq` (blocking, `rustls-tls`, no OpenSSL) and `semver`. Keep features minimal; ensure it builds for `aarch64-pc-windows-msvc` (rustls is pure-Rust). (If `ureq`+rustls is awkward, `reqwest` with `default-features=false, features=["rustls-tls","json"]` is an acceptable fallback — but avoid pulling OpenSSL.)
- [ ] **Step 3 — the check command.** Add `check_for_update(app) -> Result<Option<UpdateInfo>, String>` where `UpdateInfo { version: String, url: String, notes: Option<String> }`:
  - GET `https://api.github.com/repos/ValeriyMaslenikov/tamp/releases?per_page=20` (the **list**, so prereleases are included). GitHub requires a `User-Agent` header — set e.g. `tamp/<version> update-check`. Run the blocking request in `tauri::async_runtime::spawn_blocking`.
  - Parse the releases; ignore drafts; among the rest pick the highest `semver::Version` from each release's `tag_name` (strip a leading `v`). Compare to the **installed** version (`app.package_info().version`). Return `Some(UpdateInfo{ version, html_url, body })` when the latest is strictly greater, else `None`.
  - On any network/parse error, return `Err(short message)` (the frontend will just silently ignore it — a failed check must never surface an error toast).
  - Register the command in `lib.rs`. The command does NOT itself read the enabled flag — the frontend only calls it when enabled (keeps it simple/testable).
- [ ] **Step 4 — tests.** Unit-test the **pure** pieces (no network): tag→version parsing (`v0.3.0-beta.7` → `0.3.0-beta.7`), the "pick the highest semver across a list including prereleases" selection, and "newer-than-installed" comparison (incl. prerelease ordering: `0.3.0-beta.7` > `0.2.0`, `0.3.0` > `0.3.0-beta.7`). Factor the selection/compare into a pure fn that takes a `Vec<tag strings>` + installed version so it's testable without HTTP.
- [ ] **Step 5 — verify.** `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` clean (the new deps must compile). `// manual:` note: with the feature on, a real launch finds the latest release.
- [ ] **Step 6 — commit** `git add -A src-tauri Cargo.toml src/lib/ipc.ts && git commit -m "feat(update-check): backend GitHub version check (opt-in setting + dismissed-version memory), minimal rustls HTTP client"`

---

## Task 2: Frontend — first-run consent, Preferences toggle, and the update-available modal

**Files:** `src/lib/onboarding.ts`, `src/lib/onboarding.test.ts`, `src/views/preferences.ts`, `src/main.ts`, `src/lib/ipc.ts`, `src/styles.css` (and a small `src/lib/updatemodal.ts` if a dedicated module reads cleaner)

- [ ] **Step 1 — ipc binding.** Add `checkForUpdate(): Promise<UpdateInfo | null>` (`UpdateInfo = { version: string; url: string; notes: string | null }`) to `ipc.ts`, invoking `check_for_update`.
- [ ] **Step 2 — first-run consent.** In `onboarding.ts` `buildOnboardingNotice`, add one calm line + a checkbox **"Check for new versions automatically"** (default **checked** — it's a visible, one-click-to-uncheck choice, matching "ask on first run" without nagging; privacy-minded users uncheck). Sub-label: "Only asks GitHub for the latest version — nothing about you is sent." Pass the checkbox state to `onDismiss`/a callback so the caller persists `updateCheckEnabled` when the user clicks "Got it". Keep the existing tray/reopen/notification copy. Update `onboarding.test.ts`.
- [ ] **Step 3 — Preferences toggle.** Add a **"Check for updates automatically"** switch to Preferences → Behavior (near the other toggles), bound to `updateCheckEnabled`, with a one-line privacy sub-label. Saving persists it like the other toggles.
- [ ] **Step 4 — the modal (gentle, once-per-version).** On panel shown / startup, IF `settings.updateCheckEnabled`: call `checkForUpdate()` (swallow errors silently). If it returns an `UpdateInfo` whose `version !== settings.lastDismissedUpdateVersion`, show a dismissible modal over the panel: title "tamp {version} is available", a short line, and actions **"What's new"** (open `url`/release notes via the opener), **"Download"** (open `url`), **"Later"** (dismiss). Any dismiss path persists `lastDismissedUpdateVersion = version` (via `saveSettings`) so it never reshows for that version — only a strictly newer one reappears. Reuse the dialog a11y semantics + panel-host mount from the Custom/quick-pick modals (role="dialog", aria-modal, focus-in, Esc/Tab handling). Keep it calm and non-blocking — it must be trivially dismissible and never auto-act.
- [ ] **Step 5 — wire it.** In `main.ts`, run the check once per launch (e.g. on the first `panel:shown` after settings load, or right after the onboarding notice path), gated on the setting. Do not block panel rendering on the network call (fire-and-forget; show the modal when/if it resolves).
- [ ] **Step 6 — verify.** `bunx tsc --noEmit && bun run test` clean; `cd src-tauri && cargo test` still clean (settings round-trip). Add a small pure test for the "show modal?" decision (newer version AND not the dismissed one). `// manual:` note: fresh profile → consent checkbox; with a newer release present, opening the panel shows one dismissible card; dismiss → never reshows for that version; toggle off in Preferences → no check.
- [ ] **Step 7 — commit** `git add -A src/lib src/views src/main.ts src/styles.css && git commit -m "feat(update-check): first-run consent + Preferences toggle + a gentle once-per-version 'update available' modal (no OS notifications)"`

---

## Self-review notes
- Privacy: off by default; opt-in on first run; togglable; the single outbound request sends nothing about the user (a GET to the public releases API). Matches the README "no uploads" promise — people who want zero passive network just leave it off.
- Unannoying: no OS notification; a quiet modal only on panel open; `lastDismissedUpdateVersion` guarantees at most one card per new release; everything is one click to dismiss and never auto-acts.
- Pre-1.0 channel: "latest" includes prereleases so beta testers are served; revisit channel splitting once a stable 1.0 line exists.
- Order: Task 1 (backend + settings + deps) → Task 2 (frontend consent/toggle/modal). Sequential — Task 2 depends on the command + settings fields.
