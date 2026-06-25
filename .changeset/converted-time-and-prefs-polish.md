---
"tamp": patch
---

UI polish from beta on-device feedback:

- Converted tab: the relative time is now pinned to the right of each row, so it
  (and its hover affordance) is never truncated away on long or multi-part rows —
  the dotted underline is now consistent across every row.
- Converted tab: fixed the hover tooltip, which showed the original file's
  recording time under a bare "Converted" label. It now reads as two clearly
  labelled rows — **Recorded** (the original) then **Converted** — and the
  label is "Recorded" instead of "Created".
- Preferences: toggle knobs no longer poke out of their track (the knob was
  rendering 2px oversized).
- Preferences: the "Check for updates automatically" privacy hint now sits with
  its toggle as one unit instead of being orphaned past the row divider.
- Preferences: the "Recent videos shown" field is separated from the layout
  picker above it with a hairline.
- Ukrainian: "folder" now uses Windows' standard term «папка» (was «тека»);
  "Default" is «За замовчуванням» (was «Типовий»); the Converted tooltip label
  is «Записано».
