---
"tamp": patch
---

Fixed high-resolution recordings failing (or producing unwatchable mush)
with small targets: when the target would starve the bitrate, tamp now
automatically caps the frame rate at 30 fps and steps the resolution down
just enough to stay legible — the GPU encoder then hits the target reliably
on the first attempt. Hardware overshoots no longer poison the software
retry's bitrate.
