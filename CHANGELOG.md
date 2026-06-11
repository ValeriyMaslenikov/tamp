# tamp

## 0.3.1

### Patch Changes

- [`39eac9a`](https://github.com/ValeriyMaslenikov/tamp/commit/39eac9a271b2f5337f535623a7e2fe484112ed99) Thanks [@ValeriyMaslenikov](https://github.com/ValeriyMaslenikov)! - Fixed high-resolution recordings failing (or producing unwatchable mush)
  with small targets: when the target would starve the bitrate, tamp now
  automatically caps the frame rate at 30 fps and steps the resolution down
  just enough to stay legible — the GPU encoder then hits the target reliably
  on the first attempt. Hardware overshoots no longer poison the software
  retry's bitrate.

## 0.3.0

### Minor Changes

- [`ac01a4c`](https://github.com/ValeriyMaslenikov/tamp/commit/ac01a4cea09fc222075d253a66dbb601d598ebd7) Thanks [@ValeriyMaslenikov](https://github.com/ValeriyMaslenikov)! - Debuggability and quality-of-life: tamp now writes rotating logs
  (~/Library/Logs, 10MB cap) with full ffmpeg command lines and errors —
  right-click the menu bar icon → "Open Logs"; encode failures show the real
  error instead of metadata noise; and every row gains a reveal-in-Finder
  button next to the file name.

## 0.2.1

### Patch Changes

- [`10836f7`](https://github.com/ValeriyMaslenikov/tamp/commit/10836f787eae0c7d6caf8a050920595c91ae5bc0) Thanks [@ValeriyMaslenikov](https://github.com/ValeriyMaslenikov)! - Outputs are now guaranteed to land at or under the preset's target size —
  Discord rejects files even one byte over, so tamp converges with corrected
  re-encodes (switching from the GPU encoder to precise two-pass software on
  overshoot) and fails with a clear message rather than ever delivering an
  oversized file. Also: crash-proof atomic outputs, reuse only serves verified
  under-target files with matching provenance, stale oversized outputs from
  the old behavior are cleaned up, and the README preview screenshot no longer
  looks like a broken TV.

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
