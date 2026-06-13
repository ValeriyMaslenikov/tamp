---
"tamp": minor
---

Output filenames now include the preset name (e.g. `clip (tamped Discord
10MB 823f).mp4`) so compressed files are easy to tell apart at a glance, and
multi-part splits land in a single named folder (`clip (tamped Discord
10MB 823f)/` containing `clip 1.mp4`, `clip 2.mp4`) instead of scattering
`pNofM` files next to the original. The short config hash is kept alongside
the name so re-clicking still reuses the existing output. Existing outputs
(named the old way) keep working.
