# AGENTS.md

Guidance for AI agents (and humans) working in this repo.

**tamp** — a Tauri 2 menu-bar/tray app that shrinks screen recordings to a
target size. Frontend is vanilla TypeScript + Vite (Bun); the backend (folder
scanning, bitrate planning, ffmpeg orchestration, clipboard/tray) is Rust.

## Where things are

- `src/` — frontend (TypeScript). `lib/` typed IPC + helpers, `views/` panel UI.
- `src-tauri/src/` — Rust backend. `encoder/` (probe, plan, two-pass, naming),
  `platform/` (per-OS strategies behind one `Platform` trait — clipboard, tray,
  watched folders, hardware encoders, shortcuts), `scanner.rs`, `tray.rs`.
- `scripts/fetch-ffmpeg.ts` — fetches the bundled ffmpeg/ffprobe sidecars.
- `docs/` — project docs. `docs/mockups/` holds design explorations.

## Architecture rules

- **OS-specific behavior lives behind the `Platform` trait** in
  `src-tauri/src/platform/` (one impl per OS, selected at a single `cfg` site).
  Do not scatter `#[cfg(target_os = …)]` through feature code; add a trait
  method and implement it per platform. Linux is added the same way.
- Keep the three "(tamped …)" suffix parsers in lockstep — the scanner
  delegates to `encoder::plan`, and `src/lib/naming.ts` mirrors it.

## Build & verify

- Frontend: `bun install`, `bun run test` (unit), `bunx playwright test` (UI
  E2E), `bun run build`. See **Testing** below for all levels.
- Rust (from `src-tauri/`): `cargo fmt`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test` (needs `bun scripts/fetch-ffmpeg.ts` first for the
  integration tests).
- Run the app: `bun tauri dev`.

## Testing

**Every change ships with tests. This is not optional.** When you add or change a
feature, add or update tests at the appropriate level(s) below — as much as the
change reasonably allows.

**The litmus test for "enough":** if you can revert your behavior change and **no
test turns red**, the suite has a hole — fill it. A test must be *sensitive to the
behavior it protects*: a quick way to confirm this is to (mentally or literally)
break your new code and check that a specific, named test fails. A test that
passes no matter what the app does is worse than no test — it gives false
confidence. We routinely mutation-check tests this way; you should too.

Pick the level that actually exercises what you changed (prefer the cheapest that
catches the regression; add higher levels for user-facing flows):

- **Frontend unit** — `vitest`, files `src/**/*.test.ts`, default node env (pure
  functions). Extract logic from a view into a pure exported helper and test
  that, rather than leaving it untestable inline. Run: `bun run test`.
- **Frontend UI E2E** — Playwright, `e2e/*.spec.ts`. Drives the **real frontend**
  in a browser with the Tauri IPC mocked (`e2e/mock-ipc.ts`, injected pre-boot).
  Use it for user journeys (rendering, clicks, persistence calls). A **new IPC
  command must get a mock entry** so `e2e/mock-ipc.ts` stays in sync with
  `src/lib/ipc.ts` — that mock is hand-maintained and will silently drift
  otherwise. Run: `bunx playwright test` (first time: `bunx playwright install
  chromium`).
- **Rust unit** — `#[cfg(test)]` modules next to the code in `src-tauri/src/`.
  Run: `cargo test`.
- **Rust integration** — `src-tauri/tests/*.rs`, exercising flows end-to-end at
  the command/module layer (journal migration, settings/validate, probe cache,
  encode). Run: `cargo test` (`encode_integration` needs the ffmpeg sidecars —
  `bun scripts/fetch-ffmpeg.ts` first).
- **Native E2E smoke** — `tauri-driver` + WebdriverIO in `e2e-native/`, driving
  the **built app** (set `TAMP_E2E=1` so the panel shows/pins for WebDriver).
  Happy-path boot smoke only; Windows CI job (authoritative — not runnable on
  every dev machine).

CI runs `bun run test`, `bunx playwright test`, `cargo test`, and the native
smoke job; keep all of them green. The Playwright mock layer means the
frontend↔backend IPC contract is **not** auto-verified end-to-end except by the
native smoke — when you change a command's shape, update both sides and the mock.

## Release process

Releases follow a **release-branch** model — features accumulate on a
`release/X.Y.Z` branch, are beta-tested there, then land on `main` where
changesets cuts the tagged release.

**The full process — release branch, accumulation, beta tags, merge to main,
and the automated/manual mechanics — is documented in
[`docs/releasing.md`](docs/releasing.md). Read it before cutting a release.**

Every user-facing change needs a changeset (`bun changeset`). See also
[`CONTRIBUTING.md`](CONTRIBUTING.md).
