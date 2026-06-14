---
"tamp": patch
---

Global shortcuts now fire on the key that **types** the configured character
in your keyboard layout, not the physical QWERTY position. On macOS (whose
hotkey API is positional), `Cmd+Alt+T` on a Dvorak layout now triggers on the
key that types "t" rather than the QWERTY-T position. Windows hotkeys were
already layout-aware and are unchanged.
