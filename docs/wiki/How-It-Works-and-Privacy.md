# How It Works & Privacy

For the curious: how Tamp hits a size target reliably, how it picks an encoder,
and exactly what does (and doesn't) leave your machine.

## How compression works

Tamp bundles a static [FFmpeg](https://ffmpeg.org) and drives it from a Rust
engine. The flow for a single conversion:

1. **Plan the bitrate from the target.** Roughly:
   `video bitrate = (target size − audio budget) ÷ duration`, minus a ~5%
   container margin. Audio is encoded at a modest fixed rate (or dropped if you
   chose *Strip audio*), and `+faststart` is set so the file plays instantly
   when streamed.
2. **Cap quality only as needed.** If the target would starve the bitrate (think
   a 4K screen recording into 10 MB), Tamp automatically caps the frame rate and
   steps the resolution down *just enough* to keep the result legible — on top
   of any caps you set in the preset.
3. **Encode.** With enough bitrate headroom, the OS **hardware encoder** hits
   the size quickly. When a target is tight enough that quality matters,
   Tamp uses precise **two-pass software** encoding instead.
4. **Verify and converge.** Tamp checks the result; if it ever overshoots, it
   re-encodes with a corrected bitrate. It **never delivers a file over your
   target** — and if a target is genuinely unreachable, it tells you (see
   ["Target too small"](FAQ-and-Troubleshooting#target-too-small)) rather than
   handing you unwatchable output.

### Hardware vs. software encoding

The **Use GPU encoder** setting (on by default) uses your platform's hardware
encoder — **VideoToolbox** on macOS; **NVENC / QSV / AMF / Media Foundation** on
Windows, whichever your machine has. It's fast and great for most targets. For
very tight targets where precision wins, Tamp switches to two-pass software
(x264). Turn the setting off to always prefer software.

### Choosing a format

| Format | Codec | Best for |
|--------|-------|----------|
| **MP4** | H.264 + AAC | The safe default — plays everywhere, hardware-accelerated. |
| **WebM** | VP9 (two-pass) + Opus | The web; often smaller than MP4 at the same quality. |
| **GIF** | palette-optimized, size-targeted | The handful of places that still demand a GIF (no audio). |

### Splitting

When one clip can't look good under your target, Tamp can compress it as several
parts that each fit the full target — automatically (smart) or on your terms
(static). See [Presets & Splitting](Presets-and-Splitting#splitting-into-parts).

## Privacy & your data

Tamp is **local-first by design**:

- **Everything runs on your machine.** Encoding uses the bundled FFmpeg; your
  videos are never uploaded anywhere.
- **No telemetry, no analytics, no account.** Tamp doesn't phone home.
- **One optional outbound request:** if you turn on **Check for updates
  automatically**, Tamp asks the GitHub releases API whether a newer version
  exists. That request carries nothing about you or your files — just a standard
  request for the public release list. It's off by default; see
  [Updates](#updates).

### Logs

Tamp keeps **rotating local logs** (capped at roughly 10 MB total) recording the
FFmpeg command lines and any errors — invaluable when something fails. They stay
on your machine. Open the folder from the **tray menu → Open Logs**, and find
the app version at the bottom of Preferences. When reporting a bug, attaching
the relevant log lines helps a lot — see
[Troubleshooting](FAQ-and-Troubleshooting#reading-the-logs--reporting-a-bug).

## Updates

The opt-in update check (above) surfaces a small notice when a newer release
exists, with **Download** (opens the GitHub release page) and **Later**
(dismisses it — Tamp won't nag again for that same version). You install the new
build yourself, the same way as the first time — see
[Installing Tamp](Installing-Tamp#keeping-tamp-updated).
