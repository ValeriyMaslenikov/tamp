# tamp

## 0.2.0

### Minor Changes

- [`36c3758`](https://github.com/ValeriyMaslenikov/tamp/commit/36c375835947784a49ef1a1aa49cd442c96260f2) Thanks [@ValeriyMaslenikov](https://github.com/ValeriyMaslenikov)! - Formats, reuse, GPU encoding, and a keyboard-first panel:

  - **Output formats per preset**: MP4 (H.264, hardware-accelerated via
    VideoToolbox with automatic software fallback), WebM (two-pass VP9 + Opus),
    and GIF (palette-optimized with iterative size targeting)
  - **Conversion reuse**: outputs are named with a 4-char preset-config
    fingerprint; re-clicking reuses the existing file instantly, and changing
    the preset re-encodes
  - **Custom conversion**: a one-off convert page (size/fps/scale/audio/format)
    without saving a preset
  - **Keyboard-first UX**: filename filter with autofocus, arrow-key selection,
    Enter/d for the default preset, e to expand, Esc cascade; global shortcuts
    to compress the latest recording (⌘⌥T) and toggle the panel (⌘⌥O), with a
    staleness warning notification
  - **Better previews**: expand a row for a generated mini-montage preview —
    instant even for gigabyte files; video length shown in the list
  - Conversions journal keeps the last 200 results so compressed copies of
    deleted originals still show their before/after sizes
  - Outputs never exceed the source's own bitrate (small files no longer grow)
  - Bundle identifier is now io.github.valeriymaslenikov.tamp (settings and
    history migrate automatically; macOS will re-ask for Desktop access once)

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
