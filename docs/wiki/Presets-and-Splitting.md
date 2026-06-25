# Presets & Splitting

A **preset** captures *where you're sharing* — a target size and how to reach it
— so compressing is one click. This page covers presets end to end, then how to
split one long clip into several parts that each fit your target.

## The size-first idea

Most encoders make you guess a bitrate and hope. Tamp flips it: you say **"fit
under N MB"** and it computes the bitrate from the video's duration to land just
under that, with a small safety margin. It **never exceeds your target**, and
never grows a file past the source's own bitrate. Everything else (FPS,
resolution, audio) is an optional cap on top of the size goal. The math is in
[How It Works](How-It-Works-and-Privacy#how-compression-works).

## Preset fields

| Field | What it does |
|-------|--------------|
| **Target MB** | The size to land under. The only required field. |
| **Format** | `MP4` (H.264 — broad compatibility, hardware-accelerated), `WebM` (two-pass VP9 + Opus — great for the web), or `GIF` (palette-optimized, size-targeted). See [format trade-offs](How-It-Works-and-Privacy#choosing-a-format). |
| **Max FPS** | Optional cap on frame rate (e.g. 30). Lower FPS leaves more bitrate for image quality. |
| **Max width** | Optional pixel-width cap; the height scales to match. |
| **Scale %** | Optional uniform downscale (e.g. 50%). *Mutually exclusive with Max width.* |
| **Strip audio** | Drop the audio track entirely — smaller files, more room for video. |
| **Split** | Break a long clip into parts that each fit Target MB — see [below](#splitting-into-parts). |

The card under each preset summarizes it at a glance, e.g.
`≤ 10 MB · 30 fps · 1280px · smart split`.

## The built-in Discord (10 MB) preset

Tamp ships with a **Discord (10 MB)** preset so a typical recording is shareable
immediately. Edit it, delete it, or add your own (Slack, email, a bug
tracker — whatever caps you hit).

## Creating, editing, and choosing a default

In **Preferences → Presets**:

- **+ New preset** opens an inline editor; fill in the fields and **Save**.
- **Edit** / **Delete** sit on each preset card (Delete is disabled when only
  one preset remains).
- The **radio button** on a card marks it the **default** — the preset a plain
  click (or <kbd>⏎</kbd>/<kbd>d</kbd>) uses, and the one preselected in the
  quick-pick menu.
- **Names must be unique** (case-insensitive); Tamp blocks a duplicate name so
  the pickers can always tell presets apart.

## Splitting into parts

Sometimes one clip simply can't look good under your target — a 20-minute
recording squeezed into 10 MB would be mush. **Splitting** compresses the video
as **several parts that each fit the full target**, so a long demo becomes, say,
four crisp 10 MB files you can post in sequence.

Set it per preset (or in a custom conversion) under **Split**:

- **Off** — one output (the default).
- **Smart** — Tamp decides how many parts are needed so each one looks good,
  splitting more for longer/busier video, up to a sensible cap. Best when you
  just want "good enough, automatically."
- **Static** — you decide:
  - **By parts** — a fixed number of equal pieces (2–20).
  - **By duration** — a new part every N seconds.

Very short clips are never split (there's nothing to gain).

### Split output in the Converted tab

A split conversion's parts are saved together (each numbered) and collapse into
**one expandable group row** in the [Converted tab](Converted-History-and-Output).
Expand it to see the parts, use **Copy all** to put every part on the clipboard
in one go, or **Open output folder** to reveal them. See
[Converted History & Output Files](Converted-History-and-Output#multi-part-group-rows).
