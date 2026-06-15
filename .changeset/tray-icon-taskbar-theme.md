---
"tamp": patch
---

Fix the Windows tray icon vanishing on a light taskbar. The progress ring and
the idle icon were drawn in a fixed color — white for the ring, black for the
idle glyph — so each disappeared on the taskbar theme that matched it (the ring
on a light taskbar, the idle icon on a dark one). Both now read the taskbar
light/dark setting and render in a contrasting ink.
