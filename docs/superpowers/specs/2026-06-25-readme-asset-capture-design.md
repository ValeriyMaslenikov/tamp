# README/wiki asset capture harness — design

**Date:** 2026-06-25
**Status:** Approved

## Problem

The README's demo GIF (`docs/convert.gif`) was a raw screen-grab of the running
**dev** app: stray terminal text bleeds in at the bottom (`tamp dev (active-bar
fixed stop…`), the window edges are rough, and the recording shows a garish SMPTE
color-bar thumbnail. It reads as a screencast, not a product shot.

The stills are inconsistent: `docs/panel.png` is clean and curated, but it was
captured by some other ad-hoc means and can't be reliably regenerated when the UI
changes.

We want every README/wiki asset to be **clean, consistent, professional, and
reproducible from one command** — no more hand-recording.

## Key insight

The repo already has the right tool: the Playwright E2E harness (`e2e/`). It boots
the **real** frontend in a plain Chromium at a fixed viewport with **mocked Tauri
IPC** (`e2e/mock-ipc.ts`) and curated fixtures — no OS window chrome, no terminal,
no real video files. It can also drive a **full conversion animation
deterministically** by emitting `encode:state` events (`queued → pass1 0%→100% →
done`, landing on a "X% smaller" result).

So the fix is to script the captures through this harness rather than recording the
live app.

## Decisions (confirmed with user)

- **Scope:** redo the messy GIF **and** refresh all stills for consistency.
- **Format:** an optimized, looping palette **GIF** for the demo (reliable
  autoplay+loop on GitHub).
- **Framing:** the panel floats on a **subtle backdrop with a soft drop shadow**
  (understated, more polished than bare).
- **Thumbnails:** bundle a small set of **tasteful demo thumbnail images** so rows
  show colored tiles, not gray `.thumb-placeholder` boxes. (With a bare mock,
  `recent_thumb` returns `null` and rows render plain gray boxes — a visual
  downgrade from today's panel. The demo tiles are the one bit of staged content,
  and they keep the shots reading as professional.)

## Architecture

A new self-contained `docs/capture/` directory. It **reuses** `e2e/mock-ipc.ts` and
the canned-data shapes, and **does not touch product code**. Captures run via
Playwright (already a dev dependency, driven with `bunx playwright`).

### Components

1. **`docs/capture/demo-data.ts`** — one source of truth for the demo cast:
   curated `RecentVideo[]`, `ConversionRecord[]`, and `Settings` overrides. Reuses
   the fun names already in the README panel (`cat-knocks-over-everything.mov`,
   `ranked-match-final-boss.mov`, `duck-debugging-session.mov`, …) with realistic
   sizes/durations/timestamps. Each recent points at a bundled demo thumbnail.
   Types imported from `src/lib/ipc.ts`.

2. **`docs/capture/thumbs/`** — a handful of small, tasteful demo thumbnail PNGs
   (abstract/gradient cards, ~64×40 source at 2x). Referenced by `demo-data.ts`.

3. **`docs/capture/frame.ts`** — the framing CSS injected into the page at capture
   time (NOT into the app bundle): a gentle neutral/gradient body backdrop, the
   `#app`/`.panel` centered with `border-radius`, a soft drop shadow, and padding.
   Capture runs at `deviceScaleFactor: 2`. The panel itself renders exactly as in
   production; only the surrounding frame is added.

4. **`docs/capture/capture.spec.ts`** (or a Playwright test file under a dedicated
   config) — the capture script. It imports the existing harness pieces
   (`installTauriMock`, the bridge `emit`) and the demo data. It must serve the
   bundled thumbnails so `convertFileSrc`-resolved `<img>`s load (route fulfillment
   or copying thumbs where the asset URL resolves).

   Because the existing `e2e/fixtures.ts` hard-codes `defaultSettings()` and the
   default Playwright config uses the `Desktop Chrome` viewport, the capture uses
   its **own** setup (own viewport size, own seed data, own video dir) rather than
   bending the test fixtures. It may import `installTauriMock` directly and seed
   `window.__E2E__` the same way `fixtures.ts` does.

5. **`docs/capture/playwright.capture.config.ts`** — a dedicated config: viewport
   sized to the framed panel, `deviceScaleFactor: 2`, `video: 'on'` with a fixed
   `recordVideo` size for the GIF, output to a scratch dir. Kept separate so it
   never runs as part of `bunx playwright test` (the CI E2E suite).

6. **`docs/capture/shoot.ts`** + a `package.json` script (e.g. `assets:shoot`) —
   the one entry point. It runs the capture, then post-processes the recorded webm
   into the final GIF.

### Stills produced (all refreshed)

| Output | Content |
| --- | --- |
| `docs/panel.png` | Videos list (hero still) |
| `docs/expanded.png` | a row expanded |
| `docs/preferences.png` | Preferences tab |
| `docs/preset-quickpick.png` | quick-pick preset menu |
| `docs/preset-active-bar.png` | active-preset bar layout |

Each is a deterministic viewport/element screenshot at 2x.

### The conversion GIF (`docs/convert.gif`)

Story, scripted deterministically:

1. Panel open on the Videos list (curated data).
2. Cursor moves to a recording and clicks it.
3. Drive `encode:state` events `queued → pass1` ramping `progress` 0→1 over ~2–3s;
   the real progress UI animates.
4. Land on the `done` state showing the "… → … · N% smaller" result; hold ~1s.
5. Loop.

Recorded via Playwright's `recordVideo` (clean webm at the exact viewport), then
encoded to an optimized **looping palette GIF** with ffmpeg (two-pass
`palettegen` / `paletteuse`, `fps` ~15, scaled to the README's 360px display
width). Target a small file (well under today's 150 KB if feasible, but prioritize
smoothness).

### Encoding command (reference)

```
ffmpeg -y -i capture.webm -vf "fps=15,scale=720:-1:flags=lanczos,palettegen=stats_mode=diff" palette.png
ffmpeg -y -i capture.webm -i palette.png -lavfi "fps=15,scale=720:-1:flags=lanczos,paletteuse=dither=bayer:bayer_scale=3" -loop 0 docs/convert.gif
```

(720px source → crisp when displayed at 360px. Exact fps/scale/dither tuned during
implementation.)

## Reproducibility

`docs/capture/README.md`: a short runbook — `bun run assets:shoot` regenerates
every asset after a UI change. Requires `ffmpeg` on PATH (already present locally;
documented as a prerequisite).

## Non-goals

- Not wiring asset capture into CI (manual, on-demand only).
- Not changing any product UI to make captures easier.
- Not producing video (MP4/WebM) deliverables — GIF only, per the format decision.

## Testing / verification

- Each still opens and visually matches the framed, curated look (no OS chrome, no
  terminal, colored thumbnails, soft shadow).
- The GIF loops cleanly, shows the 0→100% progress and the "N% smaller" payoff, and
  embeds + autoplays in the rendered README.
- `bunx playwright test` (the CI E2E suite) is unaffected — the capture config is
  separate and not picked up by the default test run.
