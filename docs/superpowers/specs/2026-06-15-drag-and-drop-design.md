# Drag & Drop (Quick-Add) — Design

**Status:** approved 2026-06-15
**Branch:** `drag-and-drop` (off `release/0.3.0`)

## Goal

Compress any video that isn't in a watched folder, fast, three ways:

1. **Drag & drop** a video onto the panel.
2. **Add file…** — pick one or more videos from a native dialog.
3. **"Compress with tamp"** — right-click a video in Explorer (Windows, v1).

All three funnel into the existing `enqueue` / `custom_convert` → encoder →
activity-drawer → Converted-tab pipeline. A dropped/added file is *transient*:
it is compressed, the output lands next to the source (today's behavior for
recordings), and `watched_folders` is never touched. The activity drawer and
Converted tab (shipped on `conversion-history`) already show where these
conversions surface, so there is **no separate "staging" list**.

## Prior art this builds on

- **PoC** (`poc-quick-add` branch): `src/lib/dragdrop.ts` (Tauri
  `onDragDropEvent`), `compress_file_args` in `lib.rs` (argv + single-instance
  forwarding), `scripts/windows-context-menu.ps1` (HKCU registry registration).
  The Explorer-menu and CLI paths were proven on-device; the drag *gesture* was
  wired and backend-proven but needs a manual on-device drag to confirm.
- **`docs/mockups/quick-add/`** (5 mockups): drop zone, staging section, picker,
  tray-drop, OS context menu. v1 = drop + picker + context menu (staging dropped
  in favor of the existing drawer).
- **`winreg` dep** already added to `Cargo.toml` (Windows target) for the tray
  theme fix; reused here for context-menu registration.

## Decisions (locked)

| Topic | Decision |
|-------|----------|
| Preset on drop / pick | **Honors the Videos-layout setting.** active-bar → the active preset; quick-pick → a chooser. |
| Multi-file drop in quick-pick | **One** chooser, applied to all dropped files. |
| Active-bar override | **Alt/⌥-drop** opens the chooser as a one-off without changing the active preset. |
| Hide-on-drag | **Both** smart-hide-during-drag **and** a pin toggle. |
| Explorer entry granularity | **Single** "Compress with tamp" using the **default** preset. Preset submenu is a fast-follow. |
| Explorer entry placement | Win11 "Show more options" (classic) menu. Packaged main-menu `IExplorerCommand` is a follow-up. |
| Context-menu control | Preferences toggle, default **on** (Windows only). |
| macOS | drag&drop + picker work via the same Tauri APIs (tested on Windows). The Services/Quick Action equivalent of the context menu is a documented follow-up. |
| Non-video files | Ignored, with a toast naming the rejected file. |
| Output location | Next to the source; a read-only source dir surfaces a clear job failure. |

## Architecture

### Frontend

- **`src/lib/dragdrop.ts`** (productionized from the PoC) — owns the webview
  drag-drop lifecycle:
  - `getCurrentWebview().onDragDropEvent` → enter / over / drop / leave.
  - On enter/over: show the drop overlay (below).
  - On drop: filter paths to `VIDEO_EXTS`; if none are videos, toast and stop.
    Otherwise resolve the preset by layout and route to a shared
    "compress-these-paths" helper. Reads whether Alt is held — tracked via
    `keydown`/`keyup`, since the webview drop event does not reliably carry
    modifier flags — for the active-bar override.
  - On leave / drop: hide the overlay.
- **Drop overlay** — a full-panel element (in `dragdrop.ts`, styled in
  `styles.css`) shown during a drag, with text driven by the active layout:
  - active-bar → `Drop to compress with <active preset name>`
  - quick-pick (or Alt-drop in active-bar) → `Drop to pick a preset`
- **Shared preset-choice helper** — refactor the per-row quick-pick overlay in
  `views/list.ts` (`openQuickPick`, which currently takes a single
  `RecentVideo`) into a reusable function that takes **a set of input paths**
  and a callback, so a multi-file drop and a single row both reuse it. The
  picker's "apply" enqueues each path with the chosen preset.
- **Add-file button** — a `＋ Add file…` control by the filter row
  (`views/list.ts`) opens the dialog plugin's multi-select (video filter);
  selected paths go through the same resolve-preset-by-layout path as a drop.
- **Pin toggle** — a pin button in the panel header (`main.ts` shell +
  `styles.css`) that calls a `set_pin(pinned)` command; reflects pinned state
  visually.

### Backend

- **`compress_file_args(app, &args)`** (from the PoC) — parse argv on first
  launch and the single-instance forwarded args; for each path that is a video,
  widen the asset-scope to its parent dir and `enqueue` with the **default**
  preset. Wired into the single-instance handler and the first-launch path in
  `lib.rs`.
- **Asset-scope widening** — the `enqueue` / `custom_convert` commands widen the
  asset-scope to the path's parent dir (`scope.allow_directory(parent, false)`)
  whenever it lies outside the watched folders, so every external-compress entry
  point (drop, add, arg) gets it for free and the thumbnail/preview /
  `convertFileSrc` can load.
- **Pin** — managed `Pinned(AtomicBool)` (session-only, not persisted) +
  `set_pin` command. The release-only hide-on-blur handler checks it and skips
  hiding when pinned.
- **Smart-hide** — the hide-on-blur handler additionally skips the hide when the
  **primary mouse button is held** (a drag is in flight; the blur fires the
  moment the user mousedowns a file in Explorer, before any dragenter reaches
  us). New Platform-trait method `primary_mouse_button_down() -> bool`
  (Windows: `GetAsyncKeyState(VK_LBUTTON)`; macOS:
  `NSEvent::pressedMouseButtons`; default `false`). Keeps the OS check behind
  the `Platform` boundary.
- **Context-menu registration** (Windows) — `set_context_menu(enabled)` command
  using `winreg`: add/remove
  `HKCU\Software\Classes\SystemFileAssociations\.<ext>\shell\tamp.compress`
  (+ `\command` = `"<current_exe>" "%1"`) for the six video extensions, with a
  menu label and the app icon. Applied at startup from the persisted setting.
  The NSIS uninstaller also removes the keys (the app can't run to clean up
  after uninstall). Registration failure (locked-down registry) surfaces as an
  error on the toggle.
- **Extension validation** — the external-compress entry points reject
  non-video paths with a named error.

### Settings

- `contextMenuEnabled: bool` — Windows only, persisted, default `true`. Drives
  startup registration and the Preferences toggle.
- Pin is **not** a setting (session-only managed state).

## Data flow

```
drop / Add file…                         Explorer "Compress with tamp"
      │                                            │
      ▼                                            ▼
 filter VIDEO_EXTS                        tamp.exe <path>  (single-instance
      │                                     forwards to running app)
      ▼                                            │
 resolve preset by Videos-layout                   ▼
   active-bar → active preset            compress_file_args → default preset
   quick-pick → chooser (one for all)             │
   Alt-drop  → chooser override                   │
      │                                            │
      └──────────────┬─────────────────────────────┘
                     ▼
      widen asset-scope to parent dir
                     ▼
        enqueue / custom_convert  ──►  encoder  ──►  drawer + Converted tab
```

## Error handling

- **Non-video** dropped/picked → toast `"<name> isn't a video"`; videos in the
  same drop still proceed.
- **Read-only source dir** → the encode fails; the existing job-failed path
  surfaces the error in the row/toast.
- **Asset-scope** widened before enqueue so the thumbnail never 404s.
- **Registry write fails** → the `set_context_menu` command returns an error the
  toggle surfaces; the toggle reverts.

## Testing

**Unit (frontend, vitest):**
- Preset resolution by layout: active-bar → active preset id; quick-pick → opens
  the chooser; Alt in active-bar → opens the chooser.
- Extension filtering: mixed drop keeps only videos; all-non-video → toast, no
  enqueue.
- Multi-file drop in quick-pick → a single chooser whose apply enqueues every
  path.

**Unit (backend, cargo):**
- `compress_file_args`: from a mixed arg list, only video paths are enqueued.
- Context-menu command-string construction: the per-ext `command` value is
  `"<exe>" "%1"` for each of the six extensions.
- Smart-hide decision: given `primary_mouse_button_down() == true` (or pinned),
  the hide is skipped.

**Manual on-device (the PoC confirmed synthetic input can't drive these):**
- The actual Explorer→panel drag gesture (overlay text per layout; drop
  compresses).
- Context-menu toggle on → right-click a video → "Show more options" → "Compress
  with tamp" → compresses with the default preset; toggle off removes the entry.
- Pin keeps the panel open across a focus change; smart-hide keeps it open while
  a drag is in flight.

## Out of scope (follow-ups)

- Preset submenu in the Explorer entry (`SubCommands`).
- Win11 main-menu placement (packaged `IExplorerCommand` COM handler).
- macOS Services / Quick Action right-click equivalent.
- Drop onto the tray icon (mockup #4 — uncertain cross-platform feasibility).
