# tamp

## 0.2.0

### Minor Changes

- [`8546de9`](https://github.com/ValeriyMaslenikov/tamp/commit/8546de991509b74e0310b34a49f9cc5023df302e) Thanks [@ValeriyMaslenikov](https://github.com/ValeriyMaslenikov)! - Split videos into parts, each compressed to the full target size. Off by
  default; turn it on per preset (or in a custom conversion) in two modes:
  **Smart** picks the fewest parts that keep every part at good quality —
  a 2-minute 4K recording at 10 MB becomes five crisp ~25s parts instead of
  one heavily downscaled file — and **Static** splits by a fixed number of
  parts or by duration (equal-length parts, no stub at the end). One paste
  attaches all parts; re-clicking reuses the whole set; the never-over-target
  guarantee applies to every part.

## 0.1.0

Initial release.

- **Size-first compression**: pick a target ("fit under 10 MB") and tamp
  computes the bitrate from the video's duration to land just under it —
  guaranteed: it never delivers a file over the target
- **Three output formats**: MP4 (H.264, GPU-accelerated via VideoToolbox
  with automatic software fallback), WebM (two-pass VP9 + Opus), GIF
  (palette-optimized with iterative size targeting)
- **Automatic quality planning**: when a target would starve the bitrate
  (think 4K screen recording into 10 MB), tamp caps the frame rate at 30
  and steps the resolution down just enough to stay legible
- **Menu-bar panel**: recent recordings from watched folders with
  thumbnails, length, and live encode progress in the menu bar
- **Conversion reuse**: outputs carry a 4-character config fingerprint in
  the name; re-clicking reuses the existing file instantly
- **Keyboard-first**: filename filter with autofocus, arrow-key selection,
  Enter/d for default preset, e to expand, Esc to back out; global
  shortcuts to compress the latest recording (⌘⌥T) and toggle the panel
  (⌘⌥O), with a staleness warning notification
- **Previews**: expand a row for a generated mini-montage preview, pick a
  preset, or run a one-off custom conversion (size/fps/scale/format)
- **Clipboard-ready output**, optional move-original-to-Trash (with
  conversion history for deleted originals), reveal in Finder
- **Rotating logs** (10 MB cap) with full ffmpeg command lines — menu bar
  right-click → Open Logs
