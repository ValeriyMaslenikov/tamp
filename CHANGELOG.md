# tamp

## 0.3.0

### Minor Changes

- 9f3d215: Converted tab + activity drawer. A new **Converted** tab keeps a durable history
  of every conversion — including videos compressed from outside your watched
  folders — showing before→after sizes, where the output landed, and copy/reveal
  actions. A download-manager style **drawer** at the bottom of the panel surfaces
  live and just-finished conversions from any source (watched recordings, dropped
  files, the Add-file picker, or the Explorer right-click entry), then dismisses
  itself once everything is done. Together they keep one-off external files out of
  the watched-recordings list while still showing their progress and result.
- 426d904: Keyboard navigation on the Converted tab. ↑/↓ move a cursor over the history;
  Enter/Space play the selected output (→/e expand a multi-part group, ← collapse);
  c copies, r reveals (copy-all / open-folder on a group header); Esc collapses or
  closes. The footer hint lists the keys.
- c12735c: The Converted tab now groups a multi-part (split) conversion into one
  expandable row — a folder-style parent that drills into its parts — instead of
  listing each part separately. Every conversion gets a thumbnail (timestamp-named
  recordings are easy to tell apart), a ▶ play button that opens the output in the
  default player, and a hover tooltip on the time showing when the source was
  recorded vs when it was converted. The Videos tab now lists 50 recent videos by
  default and the count is configurable in Preferences.
- d30cd44: Quick-add: compress videos from outside your watched folders. Drag a video onto
  the panel, use the new "＋ Add file…" picker, or right-click a video in Windows
  Explorer → "Compress with tamp". The preset follows your Videos-layout setting
  (active-bar uses the active preset; quick-pick shows a chooser; Alt-drop forces
  the chooser). A pin and smart-hide keep the panel open while you drag a file in.
- e1a66c3: The interface is now fully localized, with a **Ukrainian** translation alongside
  English and a Language picker in Preferences (System / English / Українська).
  Dates, file sizes, and relative times follow the active locale — relative times
  ("5 minutes ago" / «5 хвилин тому») are rendered with dayjs.
- e1a66c3: A consistent keyboard model across the panel. The arrow keys now mean one thing
  everywhere: ↑/↓ move the selection, and →/← expand/collapse the selected row on
  both the Videos and Converted tabs. Active-bar preset cycling moves to `[` and
  `]` (and the on-screen ‹ › buttons); the top tabs switch with
  Ctrl+Tab / Ctrl+Shift+Tab; and Esc now also works on the Preferences tab —
  backing out of an open preset editor, otherwise hiding the panel. The Videos
  filter input's focus ring is no longer clipped at the top.
- e1a66c3: A first-run welcome that explains Tamp in four steps — Open it, Make a preset,
  Convert a recording, Use it — so new users get the mental model at a glance.
  Shown once and dismissible.
- 94e87c6: New preference "Open in file manager after converting" (Off / Multi-part
  splits only / All conversions): when enabled, a finished multi-part split
  opens its folder in Finder/Explorer, and — on the "All" setting — a single
  output is revealed in its folder. Off by default.
- e5608da: Output filenames now include the preset name (e.g. `clip (tamped Discord
10MB 823f).mp4`) so compressed files are easy to tell apart at a glance, and
  multi-part splits land in a single named folder (`clip (tamped Discord
10MB 823f)/` containing `clip 1.mp4`, `clip 2.mp4`) instead of scattering
  `pNofM` files next to the original. The short config hash is kept alongside
  the name so re-clicking still reuses the existing output. Existing outputs
  (named the old way) keep working.
- 7c12862: Light and dark themes for the panel. A new Appearance setting (Preferences →
  Appearance) chooses System (default), Light, or Dark. System follows the OS
  light/dark setting and tracks live changes; Light and Dark pin it. The light
  theme is a crisp-white palette with elevated white modals.
- e1a66c3: Optional update check. Tamp can ask GitHub on launch whether a newer release is
  out and show a dismissible "update available" card linking to the release notes.
  It's **off by default**, opt-in from the welcome screen or Preferences, and sends
  nothing about you — just a request for the public release list.
- 04dc071: Keyboard-first preset switching on the Videos tab. In "Keep one preset active"
  mode, ← / → cycle the active profile (alongside [ / ]). In both modes, pressing
  1–9 instantly converts the selected video with that preset — no menu. The footer
  hint reflects the keys available in the current mode.
- 0cf585d: Choose how the Videos screen offers presets (Preferences → Videos screen):
  "Pick a preset each time" (default) opens a quick menu on click — your
  default is preselected, press 1–9 to pick another — and "Keep one preset
  active" shows a bar with one active preset (switch with ‹ › or [ ]) that
  every click applies instantly.
- b9354c7: Windows support: tamp now runs in the Windows system tray with the same
  size-targeted compression as on macOS — hardware encoding picks from
  NVENC/QSV/AMF/Media Foundation with the proven two-pass x264 fallback,
  finished files land on the clipboard ready to paste, and releases ship NSIS
  installers for x64 and ARM64 alongside the macOS DMG.

### Patch Changes

- e1a66c3: The app is now branded **Tamp** at the OS level: the macOS app, the Windows
  installer / Start-menu / Add-Remove entry, and the installer filenames read
  "Tamp" instead of "tamp" (internal identifiers — bundle id, crate, the
  "(tamped …)" output grammar, log files — stay lowercase). Existing
  "Launch at login" autostart entries migrate automatically across the rename.
- 822e729: UI polish from beta on-device feedback:

  - Converted tab: the relative time is now pinned to the right of each row, so it
    (and its hover affordance) is never truncated away on long or multi-part rows —
    the dotted underline is now consistent across every row.
  - Converted tab: fixed the hover tooltip, which showed the original file's
    recording time under a bare "Converted" label. It now reads as two clearly
    labelled rows — **Recorded** (the original) then **Converted** — and the
    label is "Recorded" instead of "Created".
  - Preferences: toggle knobs no longer poke out of their track (the knob was
    rendering 2px oversized).
  - Preferences: the "Check for updates automatically" privacy hint now sits with
    its toggle as one unit instead of being orphaned past the row divider.
  - Preferences: the "Recent videos shown" field is separated from the layout
    picker above it with a hairline.
  - Ukrainian: "folder" now uses Windows' standard term «папка» (was «тека»);
    "Default" is «За замовчуванням» (was «Типовий»); the Converted tooltip label
    is «Записано».

- 4415f36: Global shortcuts now fire on the key that **types** the configured character
  in your keyboard layout, not the physical QWERTY position. On macOS (whose
  hotkey API is positional), `Cmd+Alt+T` on a Dvorak layout now triggers on the
  key that types "t" rather than the QWERTY-T position. Windows hotkeys were
  already layout-aware and are unchanged.
- e1a66c3: macOS builds are now signed with an Apple Developer ID and notarized by Apple, so
  they open with no Gatekeeper warning — the right-click-Open / `xattr` quarantine
  workaround is no longer needed. (Windows builds are still unsigned.)
- d55e249: Fix the Windows tray icon vanishing on a light taskbar. The progress ring and
  the idle icon were drawn in a fixed color — white for the ring, black for the
  idle glyph — so each disappeared on the taskbar theme that matched it (the ring
  on a light taskbar, the idle icon on a dark one). Both now read the taskbar
  light/dark setting and render in a contrasting ink.

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
