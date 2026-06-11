# tamp

## 0.1.0

### Minor Changes

- Initial release: menu-bar app that compresses screen recordings to a target
  file size (two-pass H.264 with bundled FFmpeg).
  - Size-first presets ("fit under N MB"), with optional FPS cap, downscaling,
    and audio stripping; ships with a Discord (10 MB) preset
  - Recent recordings from watched folders (Desktop by default), one click to
    compress with the default preset
  - Hover preview (2× muted playback) with per-video preset choice
  - Live progress in the menu bar; queueing and cancellation
  - Output saved next to the original as "name (tamped).mp4", copied to the
    clipboard as a file, original optionally moved to Trash
  - Apple Silicon DMG, ad-hoc signed
