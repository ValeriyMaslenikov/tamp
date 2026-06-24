# Installing Tamp

Tamp ships as a small per-user app for **Windows** and **macOS (Apple Silicon)**.
It installs without admin rights and lives entirely in your menu bar / system
tray — there's no Dock icon and no main window.

Grab the latest build from the
**[Releases page](https://github.com/ValeriyMaslenikov/tamp/releases)**.

## Windows

1. Download `Tamp_<version>_x64-setup.exe` (or `_arm64-setup.exe` for Windows on
   ARM) from [Releases](https://github.com/ValeriyMaslenikov/tamp/releases).
2. Run it. Tamp installs per-user — no admin prompt.
3. The Windows build isn't code-signed yet, so SmartScreen will warn you. Click
   **More info → Run anyway**. (See
   [Troubleshooting](FAQ-and-Troubleshooting#smartscreen-warns-me-on-windows).)
4. Look for the compress-arrows icon in the system tray. Windows often tucks new
   icons behind the `^` overflow arrow — drag it onto the taskbar to keep it
   visible.

Out of the box Tamp watches your **Desktop** and the
`Videos\Screen Recordings` (Snipping Tool) and `Videos\Captures` (Xbox Game Bar)
folders. See [Watched Folders](Preferences-and-Shortcuts#watched-folders) to add
your own.

## macOS (Apple Silicon)

1. Download the latest `.dmg` from
   [Releases](https://github.com/ValeriyMaslenikov/tamp/releases).
2. Open it and drag **Tamp** into **Applications**.
3. Open Tamp. The build is **signed with a Developer ID and notarized by Apple**,
   so it launches with no Gatekeeper warning — no right-click-Open needed.
4. Look for the compress-arrows icon in your menu bar. There's no Dock icon —
   Tamp lives entirely in the menu bar.

On first use macOS asks for permission to read your **Desktop** — that's Tamp
finding your screen recordings (where ⌘⇧5 saves them by default).

> **Intel Macs** aren't prebuilt yet, but
> `bun scripts/fetch-ffmpeg.ts x64` + `bun tauri build` produces a working Intel
> binary — see [Building from source](#building-from-source).

## Building from source

```bash
git clone https://github.com/ValeriyMaslenikov/tamp.git && cd tamp
bun install
bun scripts/fetch-ffmpeg.ts      # fetch static FFmpeg/ffprobe sidecars
bun tauri build                  # → src-tauri/target/release/bundle/
```

Requires [Bun](https://bun.sh) and [Rust](https://rustup.rs). For the full
contributor workflow see
[CONTRIBUTING.md](https://github.com/ValeriyMaslenikov/tamp/blob/main/CONTRIBUTING.md).

## Keeping Tamp updated

Tamp can **optionally** check GitHub for newer releases on launch (off by
default; turn it on in
[Preferences → Behavior](Preferences-and-Shortcuts#behavior) or during the
welcome). When a newer version exists you'll see a small notice with a
**Download** button that opens the release page — you install the new build the
same way you installed this one. The check sends nothing about you; see
[How It Works & Privacy](How-It-Works-and-Privacy#privacy--your-data).
