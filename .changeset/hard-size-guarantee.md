---
"tamp": patch
---

Outputs are now guaranteed to land at or under the preset's target size —
Discord rejects files even one byte over, so tamp converges with corrected
re-encodes (switching from the GPU encoder to precise two-pass software on
overshoot) and fails with a clear message rather than ever delivering an
oversized file. Also: crash-proof atomic outputs, reuse only serves verified
under-target files with matching provenance, stale oversized outputs from
the old behavior are cleaned up, and the README preview screenshot no longer
looks like a broken TV.
