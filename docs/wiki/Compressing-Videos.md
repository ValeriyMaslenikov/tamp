# Compressing Videos

The **Videos** tab is the heart of Tamp: your recent recordings, one click from
a shareable file. This page covers the list itself, the two ways to pick a
preset, dragging files in, and one-off custom conversions.

## The Videos tab

The list shows your most recent recordings from your
[watched folders](Preferences-and-Shortcuts#watched-folders), newest first. Each
row shows a thumbnail, the file name, its size and duration, and how long ago it
was **Recorded**.

- **Filter** — just start typing to filter the list by file name.
- **Add file…** — the button at the top opens a file picker so you can compress
  a video from *anywhere*, not only a watched folder.
- **Drag & drop** — drag one or more video files from Finder/Explorer onto the
  panel to compress them (non-video files in the drop are ignored).
- **Expand a row** — click the chevron or press <kbd>e</kbd> to reveal a
  generated mini-montage preview (quick even for gigabyte files), the preset
  choices, and the **Custom…** one-off option.
- **Already compressed** — re-run the same video with the same preset and Tamp
  *reuses* the existing output instantly instead of re-encoding (it shows
  "Already compressed — reused"). See
  [reuse](Converted-History-and-Output#reuse-re-clicking-is-instant).

> If the list is empty or shows a "couldn't read a folder" notice, see
> [Watched Folders](Preferences-and-Shortcuts#watched-folders) and
> [Troubleshooting](FAQ-and-Troubleshooting#nothing-shows-up-in-the-videos-list).

## Two ways to pick a preset

Tamp offers two layouts for *how* clicking a video chooses a preset. Pick the
one that fits how you work in
[Preferences → Videos screen](Preferences-and-Shortcuts#videos-screen).

### Quick-pick menu (default)

Clicking a video opens a small menu of your presets with your **default
preselected**. Press <kbd>1</kbd>–<kbd>9</kbd> to apply a preset by position,
<kbd>⏎</kbd> to take the default, or pick **Custom…** for a one-off. This is
best when you switch targets often (Discord one minute, email the next).

### Active-preset bar

A bar above the list holds **one active preset**; clicking any video applies it
instantly — no menu. Switch the active preset with <kbd>‹</kbd> <kbd>›</kbd> or
<kbd>[</kbd> <kbd>]</kbd>. This is best when you mostly compress for one place.

## Keyboard shortcuts (Videos tab)

| Key | Action |
|-----|--------|
| <kbd>↑</kbd> / <kbd>↓</kbd> | Move the selection |
| <kbd>⏎</kbd> or <kbd>d</kbd> | Compress the selected video with the current preset |
| <kbd>1</kbd>–<kbd>9</kbd> | Compress with that preset (quick profile) |
| <kbd>e</kbd> | Expand / collapse the selected row |
| (type) | Jump to the filter and start filtering |
| <kbd>Esc</kbd> | Back out / hide the panel |

The full list — including the global hotkeys — lives in
[Preferences & Shortcuts](Preferences-and-Shortcuts#keyboard-shortcuts).

## Custom (one-off) conversion

Need different settings just this once, without making a preset? Expand a row
and choose **Custom…**. The custom page has the same controls as a preset —
target MB, format, max FPS, max width / scale %, strip audio, and
[splitting](Presets-and-Splitting#splitting-into-parts) — but applies only to
that single file and isn't saved. (Custom conversion is single-file only.)
