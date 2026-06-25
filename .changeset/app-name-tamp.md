---
"tamp": patch
---

The app is now branded **Tamp** at the OS level: the macOS app, the Windows
installer / Start-menu / Add-Remove entry, and the installer filenames read
"Tamp" instead of "tamp" (internal identifiers — bundle id, crate, the
"(tamped …)" output grammar, log files — stay lowercase). Existing
"Launch at login" autostart entries migrate automatically across the rename.
