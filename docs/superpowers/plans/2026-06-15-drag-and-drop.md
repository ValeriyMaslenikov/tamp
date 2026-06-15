# Drag & Drop (Quick-Add) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users compress any video that isn't in a watched folder, fast — by dropping it on the panel, picking it from a dialog, or right-clicking it in Explorer.

**Architecture:** All three entry points funnel into the existing `enqueue_preset` → encoder → drawer/Converted pipeline. Preset choice honors the Videos-layout setting (active-bar → active preset; quick-pick → a chooser). The panel survives the drag via smart-hide (mouse-button-held) plus a pin. The Explorer entry is per-user HKCU registry keys the app manages via the existing `winreg` dep.

**Tech Stack:** Rust (Tauri 2 backend), TypeScript/Vite (frontend, vitest), `winreg` (Windows registry), NSIS (installer). Bun package manager. ARM64 Windows dev machine.

**Branch:** `drag-and-drop` (already created off `release/0.3.0`).

**Spec:** `docs/superpowers/specs/2026-06-15-drag-and-drop-design.md`

---

## Conventions for this plan

- Rust commands: `cd src-tauri` then `cargo test <filter>`, `cargo fmt`, `cargo clippy --all-targets -- -D warnings`. Put `%USERPROFILE%\.cargo\bin` on PATH.
- Frontend: from repo root, `bunx tsc --noEmit` (typecheck), `bun run test` (vitest). Put `C:\Program Files\nodejs` (ARM64 node) first on PATH.
- The dev app (`bun tauri dev`) is running on this branch; it rebuilds on save, so manual checks can use the live window.
- The PoC to productionize lives on branch `poc-quick-add` (`src/lib/dragdrop.ts`, `scripts/windows-context-menu.ps1`, the `compress_file_args` diff in `lib.rs`).

---

## File Structure

**Backend (`src-tauri/src/`):**
- `commands.rs` — modify: asset-scope widening in `enqueue_preset`; new `set_pin`, `set_context_menu`, `pick_videos` commands.
- `lib.rs` — modify: `compress_file_args` + single-instance/first-launch wiring; `Pinned` state; `should_hide_on_blur` predicate in the hide-on-blur handler; register new commands; apply context-menu setting at startup.
- `platform/mod.rs` — modify: add `primary_mouse_button_down()` to the `Platform` trait.
- `platform/windows.rs`, `platform/macos.rs` — modify: implement `primary_mouse_button_down()`.
- `platform/context_menu.rs` — **create** (Windows-only): pure key/command builders + `register`/`unregister` via `winreg`.
- `settings.rs` — modify: add `context_menu_enabled` field.
- `scanner.rs` — reused as-is (`has_video_ext`, `VIDEO_EXTS`).

**Frontend (`src/`):**
- `lib/dragdrop.ts` — **create** (productionized from PoC): overlay + drop routing + `filterVideos`.
- `lib/ipc.ts` — modify: `setPin`, `setContextMenu`, `pickVideos`; `contextMenuEnabled` on `Settings`.
- `views/list.ts` — modify: `openQuickPickForPaths`, `doEnqueuePath`, expose `compressPaths` + `currentDropHint`; `shouldPickPreset` helper; `＋ Add file…` button.
- `views/preferences.ts` — modify: context-menu toggle (Windows).
- `main.ts` — modify: init drag-drop; pin button in the header.
- `styles.css` — modify: `.drop-overlay`, `.pin-btn` styles.

**Installer:**
- `src-tauri/nsis-hooks.nsh` — **create**: uninstall macro removing the HKCU menu keys.
- `src-tauri/tauri.windows.conf.json` — modify: reference the hook.

**Tests:**
- `src-tauri/src/platform/context_menu.rs` (`#[cfg(test)]`), `lib.rs` (`#[cfg(test)]` for `should_hide_on_blur`), `commands.rs` (`#[cfg(test)]` for the scope-dir helper), `scanner.rs` (existing).
- `src/lib/dragdrop.test.ts`, `src/views/list.test.ts` (existing file — add cases).

---

## Task 1: Asset-scope widening for external files

When a file outside the watched folders is enqueued, its thumbnail/preview (`convertFileSrc`) 404s unless the asset-protocol scope allows its directory. Widen the scope in the single shared enqueue path.

**Files:**
- Modify: `src-tauri/src/commands.rs` (`enqueue_preset`, ~line 199)
- Test: `src-tauri/src/commands.rs` (`#[cfg(test)]`)

- [ ] **Step 1: Write the failing test** — append to the test module at the bottom of `commands.rs` (create the module if absent):

```rust
#[cfg(test)]
mod scope_tests {
    use super::dir_to_allow;
    use std::path::{Path, PathBuf};

    #[test]
    fn allows_parent_of_a_file_outside_watched_folders() {
        let watched = vec!["C:\\Users\\me\\Videos".to_string()];
        let got = dir_to_allow(Path::new("C:\\Downloads\\clip.mp4"), &watched);
        assert_eq!(got, Some(PathBuf::from("C:\\Downloads")));
    }

    #[test]
    fn skips_files_already_inside_a_watched_folder() {
        let watched = vec!["C:\\Users\\me\\Videos".to_string()];
        let got = dir_to_allow(Path::new("C:\\Users\\me\\Videos\\rec.mp4"), &watched);
        assert_eq!(got, None);
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd src-tauri && cargo test scope_tests`
Expected: FAIL — `cannot find function dir_to_allow`.

- [ ] **Step 3: Add the pure helper** above `enqueue_preset` in `commands.rs`:

```rust
/// The directory to grant asset-protocol access to for `path`, or `None` when
/// it already sits inside a watched folder (whose dir is allowed at startup).
/// Lets thumbnails/previews load for files dropped or added from anywhere.
fn dir_to_allow(path: &std::path::Path, watched: &[String]) -> Option<std::path::PathBuf> {
    let parent = path.parent()?;
    let inside = watched
        .iter()
        .any(|w| path.starts_with(std::path::Path::new(w)));
    if inside {
        None
    } else {
        Some(parent.to_path_buf())
    }
}
```

- [ ] **Step 4: Call it from `enqueue_preset`** — insert at the very top of `enqueue_preset` (before reading `post`/`use_hardware`), so every entry point (row click, drop, picker, arg) benefits:

```rust
    {
        let state = app.state::<SettingsState>();
        let watched = lock_settings(&state).watched_folders.clone();
        if let Some(dir) = dir_to_allow(std::path::Path::new(&path), &watched) {
            use tauri::Manager as _;
            if let Err(e) = app.asset_protocol_scope().allow_directory(&dir, false) {
                crate::log_warn!("failed to widen asset scope to {}: {e}", dir.display());
            }
        }
    }
```

- [ ] **Step 5: Run tests + clippy**

Run: `cd src-tauri && cargo test scope_tests && cargo clippy --all-targets -- -D warnings`
Expected: 2 passed; clippy clean.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat: widen asset scope for files enqueued from outside watched folders"
```

---

## Task 2: CLI / Explorer argument handling

Bring over the PoC's `compress_file_args` so `tamp.exe <file>` (and the single-instance forward the Explorer menu triggers) compresses with the default preset.

**Files:**
- Modify: `src-tauri/src/lib.rs` (add `compress_file_args`; single-instance handler ~line 99; first-launch in `setup` after panel config ~line 213)
- Test: `src-tauri/src/lib.rs` (`#[cfg(test)]`)

- [ ] **Step 1: Write the failing test** — add a test module at the bottom of `lib.rs`:

```rust
#[cfg(test)]
mod arg_tests {
    use super::first_video_arg;

    #[test]
    fn picks_the_first_video_arg_skipping_argv0() {
        let args = vec![
            "tamp.exe".to_string(),
            "--flag".to_string(),
            "C:\\a\\clip.MP4".to_string(),
            "C:\\a\\other.mkv".to_string(),
        ];
        assert_eq!(first_video_arg(&args), Some("C:\\a\\clip.MP4".to_string()));
    }

    #[test]
    fn returns_none_without_a_video_arg() {
        let args = vec!["tamp.exe".to_string(), "--toggle".to_string()];
        assert_eq!(first_video_arg(&args), None);
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd src-tauri && cargo test arg_tests`
Expected: FAIL — `cannot find function first_video_arg`.

- [ ] **Step 3: Add `first_video_arg` + `compress_file_args`** to `lib.rs` (after `migrate_legacy_data`, ~line 62). `first_video_arg` is pure (extension only) so it's testable; `compress_file_args` adds the filesystem + enqueue glue:

```rust
/// First argument (after argv[0]) that names a video by extension. Pure — does
/// not touch the filesystem — so it's unit-testable.
fn first_video_arg(args: &[String]) -> Option<String> {
    args.iter()
        .skip(1)
        .find(|a| crate::scanner::has_video_ext(std::path::Path::new(a)))
        .cloned()
}

/// Compresses the first existing video among `args` with the default preset and
/// surfaces the panel. Returns `true` when a file was handled. Powers the
/// Explorer "Compress with tamp" entry (single-instance forwards args) and
/// `tamp <file>` from a shell.
fn compress_file_args(app: &AppHandle, args: &[String]) -> bool {
    let Some(arg) = first_video_arg(args) else {
        return false;
    };
    if !std::path::Path::new(&arg).is_file() {
        return false;
    }
    match crate::commands::enqueue_default(app, arg.clone()) {
        Ok(_) => log_info!("compressing \"{arg}\" (from CLI / context menu)"),
        Err(e) => log_warn!("cannot compress \"{arg}\": {e}"),
    }
    show_panel_fallback(app);
    true
}
```

- [ ] **Step 4: Wire the single-instance handler** — replace the existing `tauri_plugin_single_instance::init` closure (~line 99) with:

```rust
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // A second launch (e.g. the Explorer "Compress with tamp" entry,
            // which runs `tamp.exe "<file>"`) forwards its args here. Compress
            // the file if one was passed; otherwise just surface the panel.
            if !compress_file_args(app, &args) {
                show_panel_fallback(app);
            }
        }))
```

- [ ] **Step 5: Wire the first-launch path** — in `setup`, immediately after the `configure_panel` block (~line 213, before `Ok(())`):

```rust
            // First launch may itself carry a file (the context menu launching
            // tamp for the first time, or `tamp <file>` from a shell).
            let argv: Vec<String> = std::env::args().collect();
            compress_file_args(app.handle(), &argv);
```

- [ ] **Step 6: Run tests + clippy**

Run: `cd src-tauri && cargo test arg_tests && cargo clippy --all-targets -- -D warnings`
Expected: 2 passed; clippy clean.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: compress a video passed as a CLI / single-instance arg with the default preset"
```

---

## Task 3: Windows Explorer "Compress with tamp" registration

A `context_menu_enabled` setting (default on) drives HKCU registry keys (via `winreg`) that add the right-click entry for the six video extensions, invoking the installed exe.

**Files:**
- Create: `src-tauri/src/platform/context_menu.rs`
- Modify: `src-tauri/src/platform/mod.rs` (declare the module, windows-only)
- Modify: `src-tauri/src/settings.rs` (add `context_menu_enabled`)
- Modify: `src-tauri/src/commands.rs` (add `set_context_menu` command)
- Modify: `src-tauri/src/lib.rs` (apply at startup; register command)
- Create: `src-tauri/nsis-hooks.nsh`
- Modify: `src-tauri/tauri.windows.conf.json`

- [ ] **Step 1: Write the failing test** — create `src-tauri/src/platform/context_menu.rs` with only the pure builders + tests first:

```rust
//! Registers (and removes) the per-user "Compress with tamp" Explorer entry by
//! writing HKCU registry keys for each video extension. Per-user (HKCU) needs
//! no admin. On Windows 11 the entry appears under "Show more options".

/// The six video extensions the menu entry is registered for (leading dot).
const EXTS: [&str; 6] = [".mov", ".mp4", ".m4v", ".webm", ".mkv", ".avi"];

/// Registry subkey (under HKCU) carrying the verb for `ext` (e.g. ".mp4").
fn verb_key(ext: &str) -> String {
    format!("Software\\Classes\\SystemFileAssociations\\{ext}\\shell\\tamp.compress")
}

/// The `command` value: the exe invoked with the right-clicked file as `%1`.
fn command_value(exe: &str) -> String {
    format!("\"{exe}\" \"%1\"")
}

#[cfg(test)]
mod tests {
    use super::{command_value, verb_key, EXTS};

    #[test]
    fn verb_key_is_per_extension_under_system_file_associations() {
        assert_eq!(
            verb_key(".mp4"),
            "Software\\Classes\\SystemFileAssociations\\.mp4\\shell\\tamp.compress"
        );
    }

    #[test]
    fn command_quotes_exe_and_passes_percent_one() {
        assert_eq!(
            command_value("C:\\Program Files\\tamp\\tamp.exe"),
            "\"C:\\Program Files\\tamp\\tamp.exe\" \"%1\""
        );
    }

    #[test]
    fn covers_the_six_video_extensions() {
        assert_eq!(EXTS.len(), 6);
        assert!(EXTS.contains(&".mp4") && EXTS.contains(&".mov"));
    }
}
```

- [ ] **Step 2: Declare the module** in `platform/mod.rs` (after the `windows_ring` line ~16):

```rust
#[cfg(target_os = "windows")]
pub mod context_menu;
```

- [ ] **Step 3: Run it to verify it fails**

Run: `cd src-tauri && cargo test context_menu`
Expected: FAIL — module/functions not found until step 1's file compiles (the `register`/`unregister` fns used by later steps don't exist yet, but these three tests should compile and pass once the file is in the module tree; if they pass already, good — proceed).
Expected after fix: 3 passed.

- [ ] **Step 4: Add the registry side-effecting functions** to `context_menu.rs` (below the builders):

```rust
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

/// Adds the "Compress with tamp" entry for every video extension, pointing at
/// `exe`. Overwrites any previous registration (idempotent).
pub fn register(exe: &str) -> std::io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for ext in EXTS {
        let (verb, _) = hkcu.create_subkey(verb_key(ext))?;
        verb.set_value("", &"Compress with tamp")?;
        verb.set_value("Icon", &format!("\"{exe}\""))?;
        let (command, _) = hkcu.create_subkey(format!("{}\\command", verb_key(ext)))?;
        command.set_value("", &command_value(exe))?;
    }
    Ok(())
}

/// Removes the entry for every video extension. Missing keys are not an error.
pub fn unregister() -> std::io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for ext in EXTS {
        match hkcu.delete_subkey_all(verb_key(ext)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// `register`/`unregister` keyed on `enabled`, using the running executable's
/// path. Best-effort logging wrapper for startup + the settings toggle.
pub fn apply(enabled: bool) -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("cannot resolve current exe: {e}"))?
        .to_string_lossy()
        .into_owned();
    let res = if enabled { register(&exe) } else { unregister() };
    res.map_err(|e| format!("context-menu registry update failed: {e}"))
}
```

- [ ] **Step 5: Add the setting** in `settings.rs`. In the `Settings` struct (after the `theme` field):

```rust
    /// Windows: whether the Explorer "Compress with tamp" right-click entry is
    /// registered. Ignored on other platforms.
    #[serde(default = "default_true")]
    pub context_menu_enabled: bool,
```

And in `default_settings`, after `theme: Theme::default(),`:

```rust
        context_menu_enabled: true,
```

- [ ] **Step 6: Add the `set_context_menu` command** in `commands.rs`:

```rust
/// Registers/removes the Windows Explorer "Compress with tamp" entry and
/// persists the choice. No-op (Ok) on non-Windows.
#[tauri::command]
pub fn set_context_menu(app: AppHandle, enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    crate::platform::context_menu::apply(enabled)?;
    {
        let state = app.state::<SettingsState>();
        let mut guard = lock_settings(&state);
        guard.context_menu_enabled = enabled;
        settings::save(&app, &guard);
    }
    Ok(())
}
```

(Verify `settings::save(&app, &guard)` matches the signature used by `save_settings` in this file; adjust the call to match the existing persistence helper.)

- [ ] **Step 7: Apply at startup + register the command** in `lib.rs`. In `setup`, after the shortcuts block (~line 180):

```rust
            #[cfg(target_os = "windows")]
            if let Err(e) = platform::context_menu::apply(loaded.context_menu_enabled) {
                log_warn!("failed to apply context-menu setting at startup: {e}");
            }
```

And add `commands::set_context_menu` to the `generate_handler!` list (after `commands::os_info`).

- [ ] **Step 8: NSIS uninstall cleanup** — create `src-tauri/nsis-hooks.nsh`:

```nsis
!macro NSIS_HOOK_PREUNINSTALL
  ; Remove the per-user "Compress with tamp" Explorer entries the app registered.
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.mov\shell\tamp.compress"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.mp4\shell\tamp.compress"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.m4v\shell\tamp.compress"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.webm\shell\tamp.compress"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.mkv\shell\tamp.compress"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.avi\shell\tamp.compress"
!macroend
```

Reference it in `src-tauri/tauri.windows.conf.json` — change the `nsis` block to:

```json
      "nsis": { "installMode": "currentUser", "installerHooks": "nsis-hooks.nsh" }
```

- [ ] **Step 9: Run tests + fmt + clippy**

Run: `cd src-tauri && cargo test context_menu && cargo fmt && cargo clippy --all-targets -- -D warnings`
Expected: tests pass; fmt clean; clippy clean.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/platform/context_menu.rs src-tauri/src/platform/mod.rs src-tauri/src/settings.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/nsis-hooks.nsh src-tauri/tauri.windows.conf.json
git commit -m "feat: register a Windows 'Compress with tamp' Explorer entry, toggled by a setting"
```

---

## Task 4: Context-menu Preferences toggle (frontend)

Expose `contextMenuEnabled` and a `setContextMenu` IPC call, and add a toggle in Preferences (Windows only).

**Files:**
- Modify: `src/lib/ipc.ts`
- Modify: `src/views/preferences.ts`

- [ ] **Step 1: Add the IPC binding + settings field** in `src/lib/ipc.ts`. In the `Settings` interface add:

```ts
  /** Windows: Explorer "Compress with tamp" right-click entry registered. */
  contextMenuEnabled: boolean;
```

After `saveSettings`, add:

```ts
export const setContextMenu = (enabled: boolean): Promise<void> =>
  invoke<void>("set_context_menu", { enabled });
```

- [ ] **Step 2: Add the toggle to Preferences.** In `src/views/preferences.ts`, find where the platform is checked (the reveal/OS-specific bits use `isMacOS()`/platform from `lib/platform`). Add a Windows-only toggle row bound to `settings.contextMenuEnabled` that calls `setContextMenu(next)` and updates local state. Match the existing toggle markup in this file (reuse the same row/switch builder the other boolean settings use — e.g. `copyToClipboard`). Concretely, add near the other boolean toggles:

```ts
  // Windows-only: register/remove the Explorer right-click entry.
  if (isWindows()) {
    const row = buildToggleRow(
      "Right-click menu",
      "Add “Compress with tamp” to Explorer’s right-click menu",
      settings.contextMenuEnabled,
      async (next) => {
        try {
          await setContextMenu(next);
          settings.contextMenuEnabled = next;
        } catch (e) {
          showToast(String(e));
          return false; // revert the switch
        }
        return true;
      },
    );
    container.appendChild(row);
  }
```

Adapt `buildToggleRow`/`container`/`isWindows` to the actual helpers in this file (read the file first; reuse the existing toggle factory and platform check rather than inventing new ones). Import `setContextMenu` from `../lib/ipc` and `isWindows` from `../lib/platform` (add an `isWindows` export there mirroring the existing `isMacOS`/`revealLabel` if it does not exist).

- [ ] **Step 3: Typecheck**

Run (repo root): `bunx tsc --noEmit`
Expected: no errors.

- [ ] **Step 4: Manual check** on the live dev app: Preferences shows the "Right-click menu" toggle; flipping it off then on logs no error (verify the registry effect in Task 7's manual pass).

- [ ] **Step 5: Commit**

```bash
git add src/lib/ipc.ts src/views/preferences.ts src/lib/platform.ts
git commit -m "feat: Preferences toggle for the Windows Explorer right-click entry"
```

---

## Task 5: Smart-hide during drag + pin

The panel auto-hides on blur (release builds), which fires the instant a file is grabbed in Explorer — before any dragenter reaches us. Keep it open while the mouse button is held, and add a pin.

**Files:**
- Modify: `src-tauri/src/platform/mod.rs` (trait method)
- Modify: `src-tauri/src/platform/windows.rs`, `src-tauri/src/platform/macos.rs` (impls)
- Modify: `src-tauri/src/lib.rs` (`Pinned` state, `should_hide_on_blur` predicate + test, hide handler, register command)
- Modify: `src-tauri/src/commands.rs` (`set_pin` command)
- Modify: `src/lib/ipc.ts`, `src/main.ts`, `src/styles.css` (pin button)
- Modify: `src-tauri/Cargo.toml`, `src-tauri/src/platform/macos.rs` deps for NSEvent

- [ ] **Step 1: Write the failing test** for the hide predicate — add to `lib.rs` test module (the `arg_tests` module from Task 2, or a new one):

```rust
#[cfg(test)]
mod hide_tests {
    use super::should_hide_on_blur;

    #[test]
    fn hides_when_idle() {
        assert!(should_hide_on_blur(false, false, false));
    }

    #[test]
    fn keeps_open_during_a_dialog_pin_or_drag() {
        assert!(!should_hide_on_blur(true, false, false), "dialog open");
        assert!(!should_hide_on_blur(false, true, false), "pinned");
        assert!(!should_hide_on_blur(false, false, true), "mouse button held (drag)");
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd src-tauri && cargo test hide_tests`
Expected: FAIL — `cannot find function should_hide_on_blur`.

- [ ] **Step 3: Add the predicate** to `lib.rs` (near `toggle_panel_fallback`):

```rust
/// Whether the panel should hide when it loses focus. It stays open while a
/// native dialog is up, while pinned, or while the primary mouse button is held
/// (a drag is in flight and may be heading to us).
fn should_hide_on_blur(dialog_open: bool, pinned: bool, mouse_button_down: bool) -> bool {
    !dialog_open && !pinned && !mouse_button_down
}
```

- [ ] **Step 4: Add `Pinned` state** in `lib.rs` (next to `DialogOpen`, ~line 25):

```rust
/// Session-only "keep the panel open" flag, toggled by the pin button.
pub struct Pinned(pub AtomicBool);
```

Manage it in `setup` next to `DialogOpen` (~line 167):

```rust
            app.manage(Pinned(AtomicBool::new(false)));
```

- [ ] **Step 5: Use the predicate in the hide handler.** Replace the body of the `Focused(false)` block (`lib.rs` ~198-212) with:

```rust
            #[cfg(not(debug_assertions))]
            if let tauri::WindowEvent::Focused(false) = _event {
                if _window.label() == "panel" {
                    let app = _window.app_handle();
                    let dialog_open = app
                        .try_state::<DialogOpen>()
                        .is_some_and(|s| s.0.load(std::sync::atomic::Ordering::SeqCst));
                    let pinned = app
                        .try_state::<Pinned>()
                        .is_some_and(|s| s.0.load(std::sync::atomic::Ordering::SeqCst));
                    let mouse_down = platform::native().primary_mouse_button_down();
                    if should_hide_on_blur(dialog_open, pinned, mouse_down) {
                        if let Err(e) = _window.hide() {
                            log_warn!("failed to hide panel on focus loss: {e}");
                        }
                    }
                }
            }
```

(`platform` and `Platform as _` are already imported in `lib.rs`.)

- [ ] **Step 6: Add the trait method** in `platform/mod.rs` (inside `trait Platform`):

```rust
    /// Whether the primary (left) mouse button is currently held — used to keep
    /// the panel open while a file is being dragged in from the OS file manager
    /// (the drag starts, blurring us, before any dragenter reaches the webview).
    fn primary_mouse_button_down(&self) -> bool;
```

- [ ] **Step 7: Implement on Windows** — in `platform/windows.rs`, add to `impl Platform for Windows`:

```rust
    fn primary_mouse_button_down(&self) -> bool {
        #[link(name = "user32")]
        extern "system" {
            fn GetAsyncKeyState(v_key: i32) -> i16;
        }
        const VK_LBUTTON: i32 = 0x01;
        // High-order bit set ⇒ key is currently down.
        (unsafe { GetAsyncKeyState(VK_LBUTTON) } as u16 & 0x8000) != 0
    }
```

- [ ] **Step 8: Implement on macOS** — add the `NSEvent` feature to `objc2-app-kit` in `Cargo.toml`:

```toml
objc2-app-kit = { version = "0.3", features = ["NSPasteboard", "NSPasteboardItem", "NSResponder", "NSWindow", "NSEvent"] }
```

In `platform/macos.rs`, add to `impl Platform for MacOs`:

```rust
    fn primary_mouse_button_down(&self) -> bool {
        // Bit 0 of the pressed-button mask is the left button.
        (unsafe { objc2_app_kit::NSEvent::pressedMouseButtons() } & 1) != 0
    }
```

- [ ] **Step 9: Add the `set_pin` command** in `commands.rs`:

```rust
/// Toggles the session-only "keep the panel open" pin.
#[tauri::command]
pub fn set_pin(app: AppHandle, pinned: bool) {
    if let Some(state) = app.try_state::<crate::Pinned>() {
        state.0.store(pinned, Ordering::SeqCst);
    }
}
```

Register `commands::set_pin` in the `generate_handler!` list in `lib.rs`.

- [ ] **Step 10: Add the IPC binding** in `src/lib/ipc.ts`:

```ts
export const setPin = (pinned: boolean): Promise<void> =>
  invoke<void>("set_pin", { pinned });
```

- [ ] **Step 11: Add the pin button** in `src/main.ts`. In the `.panel-header` markup, add a pin toggle button after the segmented control:

```html
          <button type="button" class="pin-btn" id="pin-btn" aria-pressed="false" title="Keep panel open">📌</button>
```

Wire it after `initToast(...)`:

```ts
  const pinBtn = document.getElementById("pin-btn") as HTMLButtonElement;
  let pinned = false;
  pinBtn.addEventListener("click", () => {
    pinned = !pinned;
    pinBtn.classList.toggle("is-on", pinned);
    pinBtn.setAttribute("aria-pressed", String(pinned));
    void setPin(pinned);
  });
```

Import `setPin` from `./lib/ipc`.

- [ ] **Step 12: Style the pin** — append to `src/styles.css`:

```css
.pin-btn {
  flex: none;
  margin-left: 8px;
  border: 1px solid var(--border);
  background: var(--surface);
  color: var(--text-dim);
  border-radius: 8px;
  width: 30px;
  height: 30px;
  cursor: pointer;
  font-size: 13px;
  line-height: 1;
}
.pin-btn.is-on {
  color: var(--accent);
  border-color: var(--accent);
  background: var(--accent-deep);
}
```

(Ensure `.panel-header` lays the segmented control and the pin out in a row — wrap them with `display:flex; align-items:center;` if needed.)

- [ ] **Step 13: Run tests + checks**

Run: `cd src-tauri && cargo test hide_tests && cargo fmt && cargo clippy --all-targets -- -D warnings`
Run (root): `bunx tsc --noEmit`
Expected: tests pass; clippy/fmt clean; typecheck clean.

- [ ] **Step 14: Commit**

```bash
git add src-tauri/src/platform/mod.rs src-tauri/src/platform/windows.rs src-tauri/src/platform/macos.rs src-tauri/src/lib.rs src-tauri/src/commands.rs src-tauri/Cargo.toml src/lib/ipc.ts src/main.ts src/styles.css
git commit -m "feat: keep the panel open during a drag (mouse-button held) and add a pin"
```

---

## Task 6: Shared preset-choice for arbitrary paths (list-view refactor)

Refactor the per-row quick-pick so a multi-file drop or the picker can choose one preset and apply it to many external paths, honoring the Videos-layout setting.

**Files:**
- Modify: `src/views/list.ts`
- Test: `src/views/list.test.ts` (existing)

- [ ] **Step 1: Write the failing test** — add to `src/views/list.test.ts`:

```ts
import { shouldPickPreset } from "./list";

describe("shouldPickPreset", () => {
  it("opens the chooser in quick-pick mode", () => {
    expect(shouldPickPreset("quick-pick", false)).toBe(true);
  });
  it("uses the active preset in active-bar mode", () => {
    expect(shouldPickPreset("active-bar", false)).toBe(false);
  });
  it("opens the chooser on Alt-drop even in active-bar mode", () => {
    expect(shouldPickPreset("active-bar", true)).toBe(true);
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run (root): `bun run test list`
Expected: FAIL — `shouldPickPreset` is not exported.

- [ ] **Step 3: Add the pure helper** at the top level of `src/views/list.ts` (module scope, exported):

```ts
/** Whether a drop/pick should open the preset chooser (vs. use the active
 *  preset). Quick-pick always chooses; active-bar uses the active preset unless
 *  Alt is held (a one-off override). */
export function shouldPickPreset(
  layout: "quick-pick" | "active-bar",
  altHeld: boolean,
): boolean {
  return layout === "quick-pick" || altHeld;
}
```

- [ ] **Step 4: Add a path-based enqueue** inside `createListView` (next to `doEnqueue`):

```ts
  function basename(p: string): string {
    return p.split(/[\\/]/).pop() ?? p;
  }

  async function doEnqueuePath(path: string, presetId: string): Promise<void> {
    try {
      const id = await enqueue(path, presetId);
      if (!jobs.has(id)) {
        updateJob({
          id, inputPath: path, inputName: basename(path), outputPath: null,
          presetId, presetHash: "", phase: "queued", progress: 0,
          inputBytes: 0, outputBytes: null, reused: false, part: null,
          error: null, postError: null,
        });
      }
    } catch (e) {
      showToast(String(e));
    }
  }
```

- [ ] **Step 5: Generalize the quick-pick overlay** to accept paths. Refactor `openQuickPick(v: RecentVideo)` into `openQuickPickForPaths(paths: string[])`: the overlay markup is unchanged; `applyQuickPick(i)` becomes:

```ts
  function applyQuickPick(i: number): void {
    if (!quickPick) return;
    const p = quickPick.presets[i];
    const paths = quickPick.paths;
    closeQuickPick();
    if (p) for (const path of paths) void doEnqueuePath(path, p.id);
  }
```

Change the `quickPick` state shape from `{ ..., video }` to `{ ..., paths: string[] }`. Update `openQuickPick(v)` callers (the row click path) to call `openQuickPickForPaths([v.path])`. Keep the `C`→Custom shortcut targeting the first path (`openCustom` still needs a `RecentVideo`; for multi-path drops, hide/disable the Custom row when `paths.length > 1` — Custom is single-file only).

- [ ] **Step 6: Add the public `compressPaths` + `currentDropHint`** to the returned `ListView`:

```ts
  function compressPaths(paths: string[], altHeld: boolean): void {
    if (paths.length === 0) return;
    if (shouldPickPreset(layoutMode(), altHeld)) {
      openQuickPickForPaths(paths);
    } else {
      const ap = activePreset();
      if (ap) for (const path of paths) void doEnqueuePath(path, ap.id);
    }
  }

  function currentDropHint(): string {
    if (layoutMode() === "active-bar") {
      const ap = activePreset();
      return ap ? `Drop to compress with ${ap.name}` : "Drop to pick a preset";
    }
    return "Drop to pick a preset";
  }
```

Add both to the `ListView` interface and the returned object: `compressPaths`, `currentDropHint`.

- [ ] **Step 7: Run tests + typecheck**

Run (root): `bun run test list && bunx tsc --noEmit`
Expected: `shouldPickPreset` tests pass; existing list tests still pass; typecheck clean.

- [ ] **Step 8: Commit**

```bash
git add src/views/list.ts src/views/list.test.ts
git commit -m "refactor: list view can compress arbitrary paths via the layout-aware preset choice"
```

---

## Task 7: Drag & drop onto the panel

Productionize the PoC `dragdrop.ts`: an overlay with adaptive text, video filtering, and routing into `compressPaths`.

**Files:**
- Create: `src/lib/dragdrop.ts`
- Create test: `src/lib/dragdrop.test.ts`
- Modify: `src/main.ts` (init), `src/styles.css` (overlay)

- [ ] **Step 1: Write the failing test** — create `src/lib/dragdrop.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { filterVideos } from "./dragdrop";

describe("filterVideos", () => {
  it("keeps only known video extensions, case-insensitively", () => {
    const got = filterVideos([
      "C:\\a\\clip.MP4", "C:\\a\\note.txt", "C:\\a\\rec.mkv", "C:\\a\\img.png",
    ]);
    expect(got).toEqual(["C:\\a\\clip.MP4", "C:\\a\\rec.mkv"]);
  });
  it("returns empty when nothing is a video", () => {
    expect(filterVideos(["a.txt", "b.zip"])).toEqual([]);
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run (root): `bun run test dragdrop`
Expected: FAIL — cannot import `filterVideos`.

- [ ] **Step 3: Create `src/lib/dragdrop.ts`:**

```ts
// Drag video files onto the panel to compress them. Uses Tauri's native webview
// drag-drop (delivers real file paths). Preset choice + the drop action are
// owned by the list view (so they honor the Videos-layout setting); this module
// is the overlay + path filtering + Alt tracking.
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { showToast } from "./toast";

const VIDEO_EXTS = new Set(["mov", "mp4", "m4v", "webm", "mkv", "avi"]);

/** Keeps only paths whose extension is a known video type (case-insensitive). */
export function filterVideos(paths: string[]): string[] {
  return paths.filter((p) => VIDEO_EXTS.has(p.split(".").pop()?.toLowerCase() ?? ""));
}

export interface DragDropDeps {
  /** Compress these (already video) paths, honoring the layout + Alt override. */
  compressPaths(paths: string[], altHeld: boolean): void;
  /** Overlay text for the current layout/active preset. */
  currentDropHint(): string;
}

export function initDragDrop(deps: DragDropDeps): void {
  const overlay = document.createElement("div");
  overlay.className = "drop-overlay";
  overlay.hidden = true;
  overlay.innerHTML =
    `<div class="drop-inner"><div class="drop-arrow">⤓</div>` +
    `<div class="drop-big"></div></div>`;
  document.body.appendChild(overlay);
  const big = overlay.querySelector(".drop-big") as HTMLElement;

  // The webview drop event carries no modifier flags, so track Alt live.
  let altHeld = false;
  window.addEventListener("keydown", (e) => { if (e.key === "Alt") altHeld = true; });
  window.addEventListener("keyup", (e) => { if (e.key === "Alt") altHeld = false; });
  window.addEventListener("blur", () => { altHeld = false; });

  void getCurrentWebview().onDragDropEvent((event) => {
    const p = event.payload;
    if (p.type === "enter" || p.type === "over") {
      big.textContent = deps.currentDropHint();
      overlay.hidden = false;
    } else if (p.type === "leave") {
      overlay.hidden = true;
    } else if (p.type === "drop") {
      overlay.hidden = true;
      const vids = filterVideos(p.paths);
      if (vids.length === 0) {
        showToast("No video files in that drop");
        return;
      }
      deps.compressPaths(vids, altHeld);
    }
  });
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run (root): `bun run test dragdrop`
Expected: 2 passed.

- [ ] **Step 5: Wire it in `src/main.ts`.** After `listView` is created and `setTab`/content are set up, add:

```ts
  initDragDrop({
    compressPaths: (paths, altHeld) => listView.compressPaths(paths, altHeld),
    currentDropHint: () => listView.currentDropHint(),
  });
```

Import `initDragDrop` from `./lib/dragdrop`.

- [ ] **Step 6: Style the overlay** — append to `src/styles.css`:

```css
.drop-overlay {
  position: absolute;
  inset: 0;
  z-index: 100;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--scrim);
  backdrop-filter: blur(2px);
  border: 2px dashed var(--accent);
  border-radius: 16px;
  pointer-events: none;
}
.drop-inner {
  text-align: center;
  color: var(--text);
}
.drop-arrow {
  font-size: 32px;
  color: var(--accent);
  margin-bottom: 8px;
}
.drop-big {
  font-size: 14px;
  font-weight: 600;
}
```

- [ ] **Step 7: Typecheck + manual check**

Run (root): `bunx tsc --noEmit && bun run test`
Expected: clean; all frontend tests pass.
Manual (live dev app, dev build does not auto-hide): drag a video file from Explorer over the panel → overlay shows the layout-appropriate text → drop → it compresses and appears in the drawer/Converted.

- [ ] **Step 8: Commit**

```bash
git add src/lib/dragdrop.ts src/lib/dragdrop.test.ts src/main.ts src/styles.css
git commit -m "feat: drag videos onto the panel to compress them (layout-aware preset choice)"
```

---

## Task 8: "Add file…" picker

A `＋ Add file…` button by the filter opens a native multi-select dialog; chosen videos route through the same `compressPaths`.

**Files:**
- Modify: `src-tauri/src/commands.rs` (`pick_videos` command)
- Modify: `src-tauri/src/lib.rs` (register command)
- Modify: `src/lib/ipc.ts` (`pickVideos`)
- Modify: `src/views/list.ts` (button)

- [ ] **Step 1: Add the `pick_videos` command** in `commands.rs`, mirroring `pick_folder`'s `DialogGuard` pattern but for multiple files with a video filter:

```rust
/// Opens a native multi-select dialog filtered to video files; returns the
/// chosen paths (empty if cancelled). Mirrors `pick_folder`'s dialog guard so
/// the release-only hide-on-blur handler doesn't close the panel.
#[tauri::command]
pub async fn pick_videos(app: AppHandle) -> Vec<String> {
    struct DialogGuard<'a>(&'a crate::DialogOpen);
    impl Drop for DialogGuard<'_> {
        fn drop(&mut self) {
            self.0 .0.store(false, Ordering::SeqCst);
        }
    }
    let dialog_open = app.state::<crate::DialogOpen>();
    dialog_open.0.store(true, Ordering::SeqCst);
    let guard = DialogGuard(dialog_open.inner());

    let dialog_app = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .file()
            .add_filter("Video", &["mov", "mp4", "m4v", "webm", "mkv", "avi"])
            .blocking_pick_files()
    })
    .await;
    drop(guard);

    if let Some(panel) = app.get_webview_window("panel") {
        if !panel.is_visible().unwrap_or(true) {
            let _ = panel.show();
        }
        let _ = panel.set_focus();
    }

    match picked {
        Ok(Some(files)) => files
            .into_iter()
            .filter_map(|f| f.into_path().ok().map(|p| p.to_string_lossy().into_owned()))
            .collect(),
        _ => Vec::new(),
    }
}
```

Register `commands::pick_videos` in the `generate_handler!` list in `lib.rs`.

- [ ] **Step 2: Verify the backend builds**

Run: `cd src-tauri && cargo build && cargo clippy --all-targets -- -D warnings`
Expected: builds; clippy clean. (`add_filter`/`blocking_pick_files` come from `tauri-plugin-dialog`'s `FileDialogBuilder`, already imported where `pick_folder` lives.)

- [ ] **Step 3: Add the IPC binding** in `src/lib/ipc.ts`:

```ts
export const pickVideos = (): Promise<string[]> =>
  invoke<string[]>("pick_videos");
```

- [ ] **Step 4: Add the button** in `src/views/list.ts`. In `createListView`, after building `filterRow`/`filterInput`, add an Add-file button into the filter row:

```ts
  const addBtn = document.createElement("button");
  addBtn.type = "button";
  addBtn.className = "add-file-btn";
  addBtn.textContent = "＋ Add file…";
  addBtn.title = "Compress a video from anywhere";
  addBtn.addEventListener("click", async () => {
    const paths = await pickVideos();
    if (paths.length) compressPaths(paths, false);
  });
  filterRow.appendChild(addBtn);
```

Import `pickVideos` from `../lib/ipc`. (`compressPaths` is defined in this scope from Task 6.)

- [ ] **Step 5: Style the button** — append to `src/styles.css`:

```css
.filter-row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.add-file-btn {
  flex: none;
  border: 1px solid var(--border);
  background: var(--surface);
  color: var(--text-dim);
  border-radius: 8px;
  padding: 0 10px;
  height: 30px;
  font-size: 11.5px;
  cursor: pointer;
  white-space: nowrap;
}
.add-file-btn:hover {
  color: var(--text);
  border-color: var(--accent);
}
```

(Confirm `.filter-row` doesn't already set conflicting layout; merge rather than duplicate the selector.)

- [ ] **Step 6: Typecheck + manual check**

Run (root): `bunx tsc --noEmit && bun run test`
Expected: clean; tests pass.
Manual (live dev app): click `＋ Add file…` → pick one or more videos → they compress (quick-pick shows one chooser; active-bar uses the active preset) and appear in the drawer.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs src/lib/ipc.ts src/views/list.ts src/styles.css
git commit -m "feat: '+ Add file…' picker to compress videos from anywhere"
```

---

## Task 9: Changeset + final verification

- [ ] **Step 1: Add a changeset** — create `.changeset/drag-and-drop.md`:

```markdown
---
"tamp": minor
---

Quick-add: compress videos from outside your watched folders. Drag a video onto
the panel, use the new "＋ Add file…" picker, or right-click a video in Windows
Explorer → "Compress with tamp". The preset follows your Videos-layout setting
(active-bar uses the active preset; quick-pick shows a chooser; Alt-drop forces
the chooser). A pin and smart-hide keep the panel open while you drag a file in.
```

- [ ] **Step 2: Full verification**

Run: `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
Run (root): `bunx tsc --noEmit && bun run test`
Expected: all green.

- [ ] **Step 3: On-device manual pass** (the PoC confirmed synthetic input can't drive these):
  - Drop a video from Explorer onto the panel (both layouts; Alt-drop in active-bar opens the chooser).
  - `＋ Add file…` → multi-select → compresses.
  - Preferences → toggle the right-click menu off and on; right-click a video in Explorer → "Show more options" → "Compress with tamp" compresses with the default preset; toggle off removes the entry.
  - Pin keeps the panel open across a focus change; un-pinned, a drag still keeps it open while the button is held.

- [ ] **Step 4: Commit**

```bash
git add .changeset/drag-and-drop.md
git commit -m "chore: changeset for drag & drop"
```

---

## Self-review notes (addressed)

- **Spec coverage:** drop (Task 7), picker (Task 8), context menu (Tasks 3–4), smart-hide + pin (Task 5), asset-scope (Task 1), arg handling (Task 2), preset-by-layout (Task 6), errors/toasts (Tasks 6–8), NSIS cleanup (Task 3), changeset (Task 9). macOS context menu is explicitly out of scope (spec). 
- **Type consistency:** `compressPaths(paths, altHeld)` and `currentDropHint()` are defined in Task 6 and consumed in Tasks 7–8; `shouldPickPreset(layout, altHeld)` is the single resolver; `dir_to_allow`, `first_video_arg`, `should_hide_on_blur`, `verb_key`, `command_value` are each defined once with their tests.
- **Watch-outs flagged inline:** confirm `settings::save` signature (Task 3.6), reuse the existing Preferences toggle factory + add `isWindows` (Task 4.2), and merge the `.filter-row` rule rather than duplicating it (Task 8.5).
