# Contributing to tamp

Thanks for your interest in improving tamp!

## Prerequisites

- [Rust](https://rustup.rs/) (stable) and [Bun](https://bun.sh/)
- **macOS 12+:** Xcode Command Line Tools (`xcode-select --install`)
- **Windows 10/11:** Visual Studio Build Tools 2022 with the "Desktop development
  with C++" workload (add the ARM64 component on ARM machines). WebView2 is
  preinstalled on Windows 11.

## Getting started

```bash
git clone https://github.com/ValeriyMaslenikov/tamp.git
cd tamp
bun install
bun scripts/fetch-ffmpeg.ts # downloads the static FFmpeg/ffprobe sidecars
bun tauri dev
```

`bun tauri dev` starts the Vite dev server and launches the app — look for the
compress-arrows icon in your menu bar / system tray (there is intentionally no
Dock or taskbar icon).

## Project layout

```
src/                  # frontend: vanilla TypeScript + Vite
  lib/                # typed IPC wrappers, pure formatting helpers
  views/              # list (videos) and preferences views
src-tauri/src/        # Rust backend
  encoder/            # probe, bitrate planning, two-pass ffmpeg, progress
  platform/           # per-OS strategies behind one trait (clipboard, tray
                      # progress, watched folders, hw encoder candidates)
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

See [docs/releasing.md](docs/releasing.md) — stable releases are automated with
changesets (macOS DMG + Windows NSIS x64/arm64); beta prereleases are cut by
pushing a `vX.Y.Z-beta.N` tag from any branch.
