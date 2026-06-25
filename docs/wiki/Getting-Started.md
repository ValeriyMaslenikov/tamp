# Getting Started

This page walks you from a fresh install to a pasted, compressed file. If you
haven't installed yet, start with [Installing Tamp](Installing-Tamp).

## Opening the panel

Tamp has no Dock icon and no main window — it lives in your **menu bar** (macOS,
top of the screen) or **system tray** (Windows, near the clock; possibly behind
the `^` overflow arrow).

- **Click the icon** to open the panel; click it again, click away, or press
  <kbd>Esc</kbd> to hide it.
- Or use the global **toggle shortcut** — <kbd>⌘⌥O</kbd> (macOS) /
  <kbd>Ctrl+Alt+O</kbd> (Windows) — from anywhere.
- The panel hides itself when it loses focus so it stays out of your way. Use
  the **pin** button in the header to keep it open while you work.

## The four-step flow

The first time you open Tamp, a short welcome lays out the whole idea:

1. **Open it** — Tamp sits in your tray; press the toggle shortcut to show or
   hide the panel anytime.
2. **Make a preset** — in Preferences, create one for wherever you share — say
   *Discord*, capped at 10 MB. (A **Discord (10 MB)** preset comes built in.)
   See [Presets & Splitting](Presets-and-Splitting).
3. **Convert a recording** — drop a video onto the panel, right-click one in
   Explorer → *Compress with Tamp* (Windows), or press the compress-latest
   shortcut to shrink your most recent recording.
4. **Use it** — the smaller file is copied to your clipboard; just paste it
   wherever you're sharing.

## The three tabs

The panel header switches between three tabs:

- **Videos** — your most recent screen recordings. Click one to compress it.
  This is where you'll spend most of your time → [Compressing Videos](Compressing-Videos).
- **Converted** — a durable history of everything you've compressed, with
  Play / Copy / Reveal for each → [Converted History & Output Files](Converted-History-and-Output).
- **Preferences** — presets, behavior, shortcuts, watched folders, and
  appearance → [Preferences & Shortcuts](Preferences-and-Shortcuts).

## The activity drawer

While a conversion runs, a small drawer slides up from the bottom showing
what's **running**, **queued**, and recently **done**, with a live progress bar.
You can queue several videos while one is encoding; finished rows include quick
Copy / Reveal actions and clear themselves after a few seconds. On Windows and
macOS a live percentage also appears next to the tray icon.

## Where your recordings come from

Tamp doesn't import anything — it **watches folders** and lists what's there.
The defaults match where each OS saves screen recordings (Desktop on macOS;
Desktop plus the Snipping Tool and Xbox Game Bar folders on Windows). Point Tamp
at wherever your recorder saves under
[Preferences → Watched folders](Preferences-and-Shortcuts#watched-folders).
