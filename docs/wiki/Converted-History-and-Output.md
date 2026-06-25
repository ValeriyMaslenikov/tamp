# Converted History & Output Files

The **Converted** tab is a durable record of everything you've compressed — even
files you dragged in from outside your watched folders, and even after you've
deleted the originals. This page covers that history and how Tamp names, places,
reuses, and hands off the files it produces.

## The history journal

Every finished conversion is written to a persistent journal, so the Converted
tab survives restarts and shows the full before/after story of each file. On
first open it shows a brief "Loading…"; later visits refresh in the background
without yanking your place in the list.

### Single rows

A normal conversion is one card row: a thumbnail, the **before → after** sizes,
the preset used, and how long ago it was converted. Three actions sit on the
right:

- **▶ Play** — open the compressed video in your default player.
- **⧉ Copy** — copy the file to the clipboard (as a *file*, ready to paste).
- **▣ Reveal** — show it in Finder / Explorer.

### Multi-part group rows

A [split](Presets-and-Splitting#splitting-into-parts) conversion collapses into
one expandable **group** row showing the total size and part count. Click it (or
press <kbd>→</kbd> / <kbd>e</kbd>) to expand the numbered parts, each with its
own Play / Copy / Reveal. The group header offers:

- **Copy all** — every part on the clipboard in a single write (so all parts
  paste, not just the last).
- **Open output folder** — reveal the folder containing the parts.

### The Recorded → Converted tooltip

Each row shows a relative time ("2 days ago"). Hover it for the exact
timestamps, as two labelled lines:

- **Recorded** — when the original video was made.
- **Converted** — when Tamp produced this output.

### Keyboard shortcuts (Converted tab)

| Key | Action |
|-----|--------|
| <kbd>↑</kbd> / <kbd>↓</kbd> | Move the selection |
| <kbd>⏎</kbd> / <kbd>Space</kbd> | Play (single) or expand/collapse (group) |
| <kbd>→</kbd> / <kbd>e</kbd> | Expand a group |
| <kbd>←</kbd> | Collapse a group |
| <kbd>c</kbd> | Copy file(s) |
| <kbd>r</kbd> | Reveal in file manager |
| <kbd>Esc</kbd> | Collapse, or hide the panel |

## Where files are saved

Outputs are written **next to the original**, in the same folder. Split parts
are saved together (each numbered) so they stay grouped.

## Output naming

Tamp marks its outputs with a `(tamped …)` suffix so they're easy to recognize
and never get re-listed as if they were new recordings:

```
clip.mp4   →   clip (tamped Discord 10MB a3f2).mp4
```

The words are the (cosmetic) preset name; the 4-character code is a
**fingerprint** of the exact settings used. That fingerprint is what powers
reuse:

## Reuse: re-clicking is instant

Because the fingerprint encodes the settings, clicking the same video with the
same preset again finds the existing output and **reuses it instantly** instead
of re-encoding — you'll see "Already compressed — reused". Change any setting
and the fingerprint changes, so you get a fresh output rather than a silent
overwrite. (Tamp's own outputs never clutter the Videos list while their
original still exists.)

## Clipboard-ready output

When **Copy result to clipboard** is on
([Preferences → Behavior](Preferences-and-Shortcuts#behavior)), a finished file
lands on your clipboard as an actual **file** — so <kbd>⌘V</kbd>/<kbd>Ctrl+V</kbd>
attaches it directly in Discord, Slack, or an email, no "save then attach" dance.

## Reclaiming disk space (Move original to Trash)

Turn on **Move original to Trash** to send the bulky source to the Trash
(recoverable) after a successful compress. The compressed copy stays in your
history with its before/after sizes even if the original is gone.

> **One preset per video with Trash on.** Because the original disappears after
> the first conversion, Tamp blocks a *second* preset on the same video (rather
> than failing halfway). Want several formats from one recording? Turn the
> toggle off. See [Behavior](Preferences-and-Shortcuts#behavior).
