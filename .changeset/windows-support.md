---
"tamp": minor
---

Windows support: tamp now runs in the Windows system tray with the same
size-targeted compression as on macOS — hardware encoding picks from
NVENC/QSV/AMF/Media Foundation with the proven two-pass x264 fallback,
finished files land on the clipboard ready to paste, and releases ship NSIS
installers for x64 and ARM64 alongside the macOS DMG.
