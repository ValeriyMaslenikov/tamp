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

- Frontend: `bun install`, `bun run test`, `bun run build`.
- Rust (from `src-tauri/`): `cargo fmt`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test` (needs `bun scripts/fetch-ffmpeg.ts` first for the
  integration tests).
- Run the app: `bun tauri dev`.

## Release process

Releases follow a **release-branch** model — features accumulate on a
`release/X.Y.Z` branch, are beta-tested there, then land on `main` where
changesets cuts the tagged release.

**The full process — release branch, accumulation, beta tags, merge to main,
and the automated/manual mechanics — is documented in
[`docs/releasing.md`](docs/releasing.md). Read it before cutting a release.**

Every user-facing change needs a changeset (`bun changeset`). See also
[`CONTRIBUTING.md`](CONTRIBUTING.md).
