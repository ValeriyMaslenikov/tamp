# Test coverage — unit gap-fill + E2E (Playwright + tauri-driver)

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development / executing-plans. Checkbox (`- [ ]`) steps.

**Goal:** Every feature built this session is covered by unit tests AND end-to-end tests. Fill unit-test gaps; add a **Playwright** UI-E2E suite (real frontend, Tauri IPC mocked) for the UI flows; expand **Rust integration** tests for backend flows; and add a small **tauri-driver** smoke suite that exercises the real built app end-to-end. Wire all of it into CI.

**Decisions (user):** "Both" — hybrid (Playwright UI E2E + Rust integration) AND a tauri-driver smoke suite. Scope E2E to **critical user journeys**, not every micro-interaction (avoid hundreds of brittle tests).

**Context:** vitest runs in the default node env (existing tests are pure-function); `@tauri-apps/api` is a dep (so `@tauri-apps/api/mocks` `mockIPC` is available); the app reads `import.meta.env.DEV` for a dev autotest hook; the frontend serves on port 1420 (`vite`), built to `dist/`. The Windows ARM64 dev VM has the known node/rollup-arch quirk — E2E (esp. tauri-driver) is **primarily validated in CI**; locally, ensure the arm64 rollup binary is present (`bun install`).

**Branch:** `converted-tree`. **Conventions:** Frontend (repo root) `C:\Program Files\nodejs` FIRST on PATH; `bunx tsc --noEmit`, `bun run test`. Rust (`cd src-tauri`) `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`. No pushing inside tasks. One commit per task.

**Execution:** Phase 1 (unit gap-fill + Rust integration) → Phase 2 (Playwright UI E2E) → Phase 3 (tauri-driver smoke), verifying between. The Playwright/tauri-driver *harness* tasks must PROVE themselves with one passing test before the flow tasks pile on.

---

## PHASE 1 — Unit gap-fill + Rust integration expansion

### Task 1.1: Frontend unit gap-fill (extract + test the untested logic)
**Files:** `src/views/preferences.ts`, `src/views/list.ts`, `src/views/converted.ts`, `src/lib/*.ts` as needed + matching `*.test.ts`
- [ ] Extract the still-inline logic of recent features into pure, testable helpers and cover them (keep the node-env pure-function pattern; only add `happy-dom` as a vitest devDep + a per-file `// @vitest-environment happy-dom` if a behavior genuinely needs DOM):
  - **Duplicate preset name:** extract `isDuplicatePresetName(name, presets, editingId)` (case-insensitive, trimmed, excludes the edited id) from preferences.ts; use it at the call site; test (dup w/ different case+space → true; the edited preset itself → false; distinct → false).
  - **Failed-row retry:** extract the decision (retry the same preset vs fall back to the picker when the preset id no longer exists) into a pure helper in list.ts; test both branches.
  - **Converted refresh selection/expansion:** extract the snapshot→restore key logic (rowKey derivation + "which row to re-select after rebuild, fall back to first only when there was no prior selection") into a pure helper; test preserve / vanished-row / first-load.
  - Audit the other session features for any untested pure logic and add a test (copy-all path list, update `shouldShowUpdate` already tested, drawer `planDrawer` already tested, i18n parity already tested).
- [ ] Verify: `bunx tsc --noEmit && bun run test` clean. Commit: `test(unit): cover duplicate-name, failed-row retry, and converted selection-restore logic`.

### Task 1.2: Rust integration tests (command/flow layer)
**Files:** `src-tauri/tests/*.rs` (new integration files; the lib is `tamp_lib`)
- [ ] Add integration tests exercising flows end-to-end at the module/command layer (beyond the per-fn unit tests):
  - **Journal migration**: write a real legacy per-part `conversions.json` to a temp dir → `Journal::load_from_path` → assert merged/deduped records + one-time rewrite + idempotent reload (mirrors the on-device check I ran manually).
  - **Settings validate flows**: round-trip a Settings through save/load + `validate()` for the new invariants (locale, duplicate names, recents bounds).
  - **Probe cache**: a second `probe_cached` for the same key doesn't re-probe (if reachable without ffmpeg; else skip with a note).
  - Keep `encode_integration.rs` as-is; add the new files alongside.
- [ ] Verify: `cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` clean. Commit: `test(integration): journal migration, settings validate flows, probe cache (Rust)`.

---

## PHASE 2 — Playwright UI E2E (real frontend, mocked IPC)

### Task 2.1: Harness (PROVE it with one smoke test)
**Files:** `package.json`, `playwright.config.ts` (new), `e2e/` (new: a mock-IPC fixture + first spec), CI
- [ ] `bun add -D @playwright/test`; `bunx playwright install chromium`.
- [ ] Build the harness: serve the app (`vite` dev server on 1420, or `vite preview` of `dist/`) and, BEFORE the app boots, install a Tauri IPC mock so `invoke`/`listen` resolve. Use `@tauri-apps/api/mocks` `mockIPC` (and `mockWindows`/event shims) — inject via a Playwright fixture that `page.addInitScript`s a small mock-setup bundle, or a dedicated E2E entry/Vite mode. The fixture provides canned responses for the commands the UI calls (`get_settings`, `list_recents`, `list_conversions`, `queue_state`, `recent_thumb`/`recent_duration`, `check_for_update`, `save_settings`, `unreachable_folders`, …) so the real frontend renders against deterministic data.
  - This is the crux — make the mock overridable per-test (a test can set `check_for_update` to return a newer version, `get_settings` to return `locale:"uk"`, etc.).
- [ ] First spec proves the harness: the panel renders the three tabs and the Videos empty/list state. `playwright.config.ts` sets the webServer (vite) + baseURL.
- [ ] CI: add a Playwright job (ubuntu-latest is fine — it's a mocked-IPC browser test) running `bunx playwright test`.
- [ ] Verify: `bunx playwright test` green locally (or document the env caveat + that CI is authoritative). Commit: `test(e2e): Playwright harness with mocked Tauri IPC + a panel-render smoke test + CI job`.

### Task 2.2: Core UI flows
**Files:** `e2e/*.spec.ts`
- [ ] Specs for the critical journeys (mock the relevant commands per test):
  - **Language switch**: set `get_settings` locale en → assert English UI; switch the Preferences Language to Українська → assert `save_settings` called with `locale:"uk"` and (after a reload with `locale:"uk"`) Ukrainian text renders.
  - **Preset picker**: a drop/quick-pick opens the picker; selecting a preset calls `enqueue`.
  - **Converted tab**: with a mocked journal (a split = one multi-output record), the tab renders one expandable group + singles; expand shows parts; Copy all calls `copy_files` with all paths.
  - **Preferences**: toggles persist via `save_settings`; **duplicate preset name** shows the error toast and does NOT save; recents-limit validation.
  - **Update modal**: `check_for_update` returns a newer version → opening the panel shows the modal; "Later"/dismiss persists `lastDismissedUpdateVersion` and it doesn't reappear for that version.
  - **Onboarding**: first-run (`onboardingSeen:false`) shows the welcome notice; "Got it" persists.
  - **Drawer**: feed `encode:state` events → running/queued-summary/done/cancelled states render.
- [ ] Verify green (or CI-authoritative). Commit: `test(e2e): core UI journeys — language, picker, Converted, preferences/duplicate-name, update modal, onboarding, drawer`.

---

## PHASE 3 — tauri-driver smoke (real built app, end-to-end)

### Task 3.1: tauri-driver harness + smoke + CI (CI-authoritative)
**Files:** `src-tauri/src/lib.rs` (a test-mode env to show+pin the panel), `e2e-native/` (new: WDIO config + smoke spec), `.github/workflows/ci.yml`
- [ ] Add a guarded test mode: when `TAMP_E2E=1`, on startup show + pin the panel (so WebDriver can attach to and drive the WebView2 window despite smart-hide). Keep it behind the env check; no effect in normal runs.
- [ ] Harness: WebdriverIO + `tauri-driver` (Windows: msedgedriver matching the WebView2 runtime). Config launches the built app under `TAMP_E2E=1`.
- [ ] Smoke specs (1–3, happy-path only): the app launches and the panel shows; the three tabs are present; (optionally) the autotest hook converts a tiny fixture and a Converted/▴ state updates. Keep assertions minimal and robust.
- [ ] CI: a Windows-only job that builds the app, installs `tauri-driver` + msedgedriver, and runs the smoke suite. Mark it `continue-on-error` initially if flakiness is a concern, with a note to harden.
- [ ] Verify in CI (document that local ARM64 runs are best-effort). Commit: `test(e2e-native): tauri-driver smoke suite on the built app + TAMP_E2E panel-show mode + Windows CI job`.

---

## Self-review notes
- Unit (pure logic) + Rust integration cover the backend + extractable logic; Playwright covers the real UI flows reliably (mocked IPC); tauri-driver proves the whole app boots and the panel works end-to-end. Layered, each at the right altitude.
- The two new harnesses (Playwright mockIPC, tauri-driver panel-show mode) are the technical risks — each is proven by a single smoke test before flow tests are added.
- E2E is scoped to critical journeys, not every micro-feature; CI is the authoritative runner (the ARM64 dev VM's node/rollup quirk makes local E2E best-effort).
- No telemetry, no behavior change to the app except the guarded `TAMP_E2E` panel-show mode.
