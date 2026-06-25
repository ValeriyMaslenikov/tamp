# Preferences & Shortcuts

Everything you can configure, by Preferences section, followed by a complete
keyboard-shortcut reference. Changes save immediately.

## Presets

Create, edit, delete, and choose a default preset, and set splitting. This has
its own page → [Presets & Splitting](Presets-and-Splitting).

## Behavior

| Setting | Default | What it does |
|---------|---------|--------------|
| **Copy result to clipboard** | On | Put the finished file on the clipboard as a *file*, ready to paste into a chat. See [clipboard output](Converted-History-and-Output#clipboard-ready-output). |
| **Move original to Trash** | Off | Send the source to the Trash after a successful compress. Enables the [one-preset-per-video](Converted-History-and-Output#reclaiming-disk-space-move-original-to-trash) guard. |
| **Use GPU encoder** | On | Encode on the OS hardware encoder (faster, slightly lower quality). Tamp falls back to precise software encoding when a target is too tight for hardware to hit cleanly. |
| **Launch at login** | Off | Start Tamp automatically when you log in. |
| **Check for updates automatically** | Off | On launch, ask GitHub whether a newer Tamp exists. The only outbound request Tamp makes, and it sends nothing about you — see [Privacy](How-It-Works-and-Privacy#privacy--your-data). |
| **Open in file manager after converting** | Off | `Off` = never; `Multi-part splits only` = open the folder after a split; `All conversions` = also reveal single outputs. |
| **Compress with Tamp** (Windows only) | On | Add a *Compress with Tamp* entry to Explorer's right-click menu for video files. Per-user registry entry; no admin needed. |

## Videos screen

- **Layout** — choose **quick-pick menu** or **active-preset bar** for how
  clicking a video picks a preset. Full explanation in
  [Compressing Videos](Compressing-Videos#two-ways-to-pick-a-preset).
- **Recent videos shown** — how many recordings the Videos tab lists (1–200,
  default 50).

## Shortcuts

Two **global** hotkeys work from anywhere, even with the panel closed:

| Shortcut | Default (macOS / Windows) | Action |
|----------|---------------------------|--------|
| **Compress latest recording** | <kbd>⌘⌥T</kbd> / <kbd>Ctrl+Alt+T</kbd> | Compress your newest recording with the default preset and copy it to the clipboard — without even opening the panel. |
| **Show / hide panel** | <kbd>⌘⌥O</kbd> / <kbd>Ctrl+Alt+O</kbd> | Toggle the panel. |

Edit either field to rebind it (standard accelerator strings like
`CmdOrCtrl+Alt+T`); **leave a field empty to disable** that shortcut. An invalid
combination is rejected and the field snaps back.

### Stale-recording warning

"Compress latest" grabs the *newest* file — handy, but easy to misfire if you
forgot to record. Tamp can warn you when the latest recording is **older than N
minutes** (set the threshold here; 0 disables the warning). The warning rides on
a desktop notification, so if notifications are off you'll see a small recovery
card offering **Enable notifications** / **Open System Settings**.

## Watched folders

Tamp lists recordings from the folders you watch — it never copies or imports
them.

- **Defaults**
  - **macOS:** Desktop (where ⌘⇧5 saves).
  - **Windows:** Desktop, `Videos\Screen Recordings` (Snipping Tool), and
    `Videos\Captures` (Xbox Game Bar).
- **+ Add folder** picks any folder to watch; the **✕** on a row removes it (you
  must keep at least one).
- If a watched folder exists but can't be read right now (an offline network
  share, a permissions issue), the Videos tab shows a calm **"couldn't read a
  folder"** notice instead of pretending you have no recordings. It clears once
  the folder is reachable again.

## Appearance

- **Theme** — `System` (follows your OS light/dark setting, live), `Light`, or
  `Dark`.
- **Language** — `System` (follows your OS/browser language), `English`, or
  `Українська`. Switching reloads the panel into the new language.

The app name and version sit at the bottom of Preferences; the tray menu's
**Open Logs** entry opens the [log folder](How-It-Works-and-Privacy#logs).

## Keyboard shortcuts

**Global (anywhere):** see [Shortcuts](#shortcuts) above.

**Videos tab**

| Key | Action |
|-----|--------|
| <kbd>↑</kbd> / <kbd>↓</kbd> | Move the selection |
| <kbd>⏎</kbd> / <kbd>d</kbd> | Compress with the current preset |
| <kbd>1</kbd>–<kbd>9</kbd> | Compress with that preset |
| <kbd>e</kbd> | Expand / collapse a row |
| (type) | Filter the list |
| <kbd>Esc</kbd> | Back out / hide the panel |

**Quick-pick menu**

| Key | Action |
|-----|--------|
| <kbd>1</kbd>–<kbd>9</kbd> | Apply that preset |
| <kbd>⏎</kbd> | Apply the default |
| <kbd>↑</kbd> / <kbd>↓</kbd> | Move |
| <kbd>Esc</kbd> | Cancel |

**Active-bar mode**

| Key | Action |
|-----|--------|
| <kbd>[</kbd> <kbd>]</kbd> or <kbd>‹</kbd> <kbd>›</kbd> | Previous / next active preset |

**Converted tab:** see
[Converted History & Output Files](Converted-History-and-Output#keyboard-shortcuts-converted-tab).
