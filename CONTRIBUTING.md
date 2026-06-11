# Contributing to tamp

Thanks for your interest in improving tamp!

## Prerequisites

- macOS 12+ (Apple Silicon or Intel)
- [Rust](https://rustup.rs/) (stable)
- [Bun](https://bun.sh/)
- Xcode Command Line Tools (`xcode-select --install`)

## Getting started

```bash
git clone https://github.com/ValeriyMaslenikov/tamp.git
cd tamp
bun install
./scripts/fetch-ffmpeg.sh   # downloads the static FFmpeg/ffprobe sidecars
bun tauri dev
```

`bun tauri dev` starts the Vite dev server and launches the app — look for the
compress-arrows icon in your menu bar (there is intentionally no Dock icon).

## Project layout

```
src/                  # frontend: vanilla TypeScript + Vite
  lib/                # typed IPC wrappers, pure formatting helpers
  views/              # list (videos) and preferences views
src-tauri/src/        # Rust backend
  encoder/            # probe, bitrate planning, two-pass ffmpeg, progress
  platform/           # macOS-specific shims (clipboard file copy)
  scanner.rs          # watched-folder scanning
  thumbs.rs           # thumbnail generation/cache
  tray.rs             # tray icon, panel toggle, title progress
scripts/              # sidecar fetcher, icon generation
```

## Tests

```bash
bun run test                 # frontend unit tests (vitest)
cd src-tauri && cargo test   # Rust unit + integration tests (needs sidecars fetched)
```

The integration test (`src-tauri/tests/`) generates a synthetic clip with the
bundled FFmpeg and runs the real two-pass encode against it, so
`./scripts/fetch-ffmpeg.sh` must have been run first.

Before pushing, also run:

```bash
cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings
```

## Submitting changes

1. Fork and create a feature branch.
2. Make your change, with tests where it makes sense.
3. Add a changeset describing the user-facing change:
   ```bash
   bun changeset
   ```
   (Skip this for changes that shouldn't appear in the changelog, e.g. CI tweaks.)
4. Open a pull request. CI must pass (frontend tests/build, rustfmt, clippy, cargo tests).

## Release process (maintainers)

Releases are automated with [changesets](https://github.com/changesets/changesets):

1. Merged PRs accumulate changeset files in `.changeset/`.
2. The release workflow keeps a **"chore: release"** PR up to date, bumping
   `package.json` (the single source of version truth — `tauri.conf.json`
   reads it) and writing `CHANGELOG.md`.
3. Merging that PR tags `vX.Y.Z`, creates the GitHub Release, and a macOS
   (Apple Silicon) job builds the DMG and attaches it to the release.
