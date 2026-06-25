# Capture toolkit

Regenerates the README/wiki screenshots and the demo GIF from the **real**
frontend, driven by the Tauri-IPC mock (`e2e/mock-ipc.ts`) with curated demo
data. No live app, no screen recording — reproducible from one command.

## Prerequisites

- `ffmpeg` on your `PATH` (`brew install ffmpeg`).
- Repo deps installed (`bun install`) and the Playwright browser
  (`bunx playwright install chromium`).

## Regenerate everything

```bash
bun run assets:shoot
```

This regenerates the emoji thumbnails, starts a throwaway Vite server on port
1431, shoots all stills + the GIF into `docs/`, and stops the server.

## What it produces

- `docs/panel.png` — Videos list (README hero)
- `docs/preset-quickpick.png`, `docs/preset-active-bar.png` — preset layouts
- `docs/preferences.png` — Preferences tab
- `docs/expanded.png` — an expanded row with preview + preset chips
- `docs/demo.{webp,mp4}` — the one-click conversion demo (README hero)
- `docs/multipart.{webp,mp4}` — split a long recording → paste every part into a
  chat; LEFT is the **real app** (Converted tab) beside a Discord mock
  (`scenes/multipart/`, spawns its own Vite on port 1433)
- `docs/dragdrop.{webp,mp4}` — pin the panel, drag a recording in, drop to
  compress (real app via `scenes/dragdrop/`, spawns its own Vite on port 1432)

The README embeds the **animated WebP** via `<img>` (the `.webp`), because
GitHub's sanitizer strips `<video>` tags pointing at committed/raw files — only
its own attachment-upload URLs render. `<img>` renders inline (autoplay + loop),
and WebP is full-colour so text stays crisp with none of the GIF palette dither.
The `.mp4` (H.264, 2× retina) is kept for full-quality playback / linking;
GitHub's blob view plays committed MP4s. Clips are recorded at 2× CSS zoom
because Playwright records at CSS-pixel resolution regardless of
`deviceScaleFactor`.

## Editing the demo cast

Edit `docs/capture/emoji.ts` (names, emoji, sizes, durations). Both the
thumbnails and the fixtures derive from it. Re-run `assets:shoot`.

## How it works

`harness.ts` boots the panel framed on a soft-shadow backdrop (`frame.ts`),
seeds `window.__E2E__` with curated recents/conversions (`demo-data.ts`), and
serves the generated emoji cards (`thumbs/`) by intercepting the
`asset.localhost` URLs the app resolves via `convertFileSrc()`. `gif.ts` drives
the real progress UI by emitting `encode:state` events, records a webm, and
encodes a looping palette GIF with ffmpeg.

The capture toolkit is standalone (run with `bun`, on port 1431) and is **not**
part of the CI E2E suite (`bunx playwright test`, port 1430).
