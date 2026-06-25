---
"tamp": patch
---

macOS builds are now signed with an Apple Developer ID and notarized by Apple, so
they open with no Gatekeeper warning — the right-click-Open / `xattr` quarantine
workaround is no longer needed. (Windows builds are still unsigned.)
