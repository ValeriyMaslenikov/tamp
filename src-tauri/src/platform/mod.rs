//! OS-specific strategies behind one interface. The ONLY place in the
//! codebase allowed to know which operating system it runs on is this
//! module's cfg-selected implementation; everything else calls [`native()`]
//! and stays platform-neutral. Adding an OS = one new module implementing
//! [`Platform`] plus one selection line below.

use std::path::PathBuf;

#[cfg(target_os = "windows")]
pub mod context_menu;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "macos", test))]
mod macos_keyboard;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(any(target_os = "windows", test))]
mod windows_ring;

#[cfg(target_os = "macos")]
static NATIVE: macos::MacOs = macos::MacOs;
#[cfg(target_os = "windows")]
static NATIVE: windows::Windows = windows::Windows;

/// The running OS's [`Platform`] strategy.
pub fn native() -> &'static impl Platform {
    &NATIVE
}

/// Live encode progress for the tray: how it's surfaced is per-OS (macOS can
/// render text next to the icon; Windows cannot).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrayProgress {
    /// Overall progress of the running job, 0..=1.
    pub fraction: f64,
    /// Jobs waiting behind it.
    pub queued: usize,
}

impl TrayProgress {
    /// Whole percent (0..=100) every strategy displays.
    pub fn percent(&self) -> u32 {
        (self.fraction.clamp(0.0, 1.0) * 100.0).round() as u32
    }
}

/// Which horizontal screen edge a fallback panel hugs — the side this OS's
/// tray lives on. Each OS uses exactly one variant, so the other reads as
/// dead code on any single target.
#[allow(dead_code)]
pub(crate) enum VAnchor {
    Top,
    Bottom,
}

/// Physical top-left to place `panel` in the right-`vanchor` corner of
/// `monitor`'s WORK AREA (excludes the taskbar/menu bar), inset from the
/// edges. `None` if the panel size can't be read.
pub(crate) fn work_area_corner(
    panel: &tauri::WebviewWindow,
    monitor: &tauri::Monitor,
    vanchor: VAnchor,
) -> Option<tauri::PhysicalPosition<i32>> {
    const INSET: i32 = 8;
    let area = monitor.work_area();
    let win = panel.outer_size().ok()?;
    let x = area.position.x + area.size.width as i32 - win.width as i32 - INSET;
    let y = match vanchor {
        VAnchor::Top => area.position.y + INSET,
        VAnchor::Bottom => area.position.y + area.size.height as i32 - win.height as i32 - INSET,
    };
    Some(tauri::PhysicalPosition::new(x, y))
}

/// Clipboard implementations need their paths as UTF-8; reject (rather than
/// mangle) the exotic ones so the error names the offending file.
fn paths_to_utf8(paths: &[PathBuf]) -> Result<Vec<String>, String> {
    if paths.is_empty() {
        return Err("no files to copy".to_string());
    }
    paths
        .iter()
        .map(|p| {
            p.to_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("path is not valid UTF-8: {}", p.display()))
        })
        .collect()
}

/// One hardware H.264 encoder this OS may offer; the encoder probes the
/// bundled ffmpeg for availability before use.
pub struct HwCandidate {
    /// ffmpeg encoder name as listed by `ffmpeg -encoders`
    /// (e.g. "h264_videotoolbox").
    pub name: &'static str,
    /// Extra codec args appended after `-c:v <name>` (rate args are shared).
    pub extra_args: &'static [&'static str],
}

pub trait Platform {
    /// App-wide startup tweaks (e.g. macOS's Accessory activation policy,
    /// which keeps tamp out of the Dock). Runs first in Tauri's setup hook.
    fn configure_app(&self, app: &mut tauri::App);

    /// Puts files (as file references, not contents) on the system clipboard
    /// in one write, so a multi-file paste lands all of them.
    fn copy_files_to_clipboard(
        &self,
        app: &tauri::AppHandle,
        paths: &[PathBuf],
    ) -> Result<(), String>;

    /// Per-OS window tweaks for the tray panel (e.g. on macOS, letting it
    /// appear over full-screen apps and follow the user across Spaces).
    fn configure_panel(&self, window: &tauri::WebviewWindow) -> Result<(), String>;

    /// Positions `panel` against the clicked tray icon (the positioner has
    /// cached the click rect). The anchor differs by where this OS puts its
    /// tray: macOS's menu bar is at the top so the panel drops below it;
    /// Windows' taskbar is at the bottom so the panel rises above it.
    fn position_panel_at_tray(&self, panel: &tauri::WebviewWindow);

    /// Positions `panel` in this OS's tray corner of `monitor`'s work area
    /// when there is no click rect to anchor to (shortcut toggle, relaunch).
    fn position_panel_fallback(&self, panel: &tauri::WebviewWindow, monitor: &tauri::Monitor);

    /// Folders watched out of the box — wherever this OS's default screen
    /// recorders save.
    fn default_watched_folders(&self, app: &tauri::AppHandle) -> Vec<PathBuf>;

    /// Pre-spawn tweaks for background helper processes (ffmpeg/ffprobe); on
    /// Windows this suppresses the console window that would otherwise flash
    /// up for every spawn.
    fn prepare_background_command(&self, cmd: &mut tokio::process::Command);

    /// Surfaces encode progress on the tray; `None` clears it back to idle.
    fn tray_progress(&self, app: &tauri::AppHandle, progress: Option<TrayProgress>);

    /// Hardware H.264 encoders this OS can offer, in preference order.
    /// Empty means hardware encoding is never attempted.
    fn hw_candidates(&self) -> &'static [HwCandidate];

    /// Rewrites a global-shortcut accelerator so it fires on the key that
    /// PRODUCES the configured character in the user's current keyboard
    /// layout, rather than the physical QWERTY position. Only macOS needs
    /// this (its hotkey API is positional); other platforms return the
    /// accelerator unchanged. See [`rewrite_accelerator_key`].
    fn resolve_accelerator(&self, accelerator: &str) -> String;
}

/// Replaces the trailing key token of an accelerator ("CmdOrCtrl+Alt+T" ->
/// key "T") using `map`, keeping the modifiers intact. `map` receives the
/// lowercased single-character key and returns the replacement character, or
/// `None` to leave the accelerator untouched (non-character keys, unmapped
/// layouts). Shared so a platform can focus only on the character→character
/// translation. Only macOS rewrites accelerators; gated so it isn't dead code
/// elsewhere.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn rewrite_accelerator_key(
    accelerator: &str,
    map: impl FnOnce(char) -> Option<char>,
) -> String {
    let Some(plus) = accelerator.rfind('+') else {
        return accelerator.to_string(); // a bare key, no modifiers
    };
    let (mods, key) = accelerator.split_at(plus + 1);
    let mut chars = key.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return accelerator.to_string(); // not a single-character key
    };
    match map(c.to_ascii_lowercase()) {
        Some(replacement) => format!("{mods}{}", replacement.to_ascii_uppercase()),
        None => accelerator.to_string(),
    }
}

/// Single-file convenience over [`Platform::copy_files_to_clipboard`].
pub fn copy_file_to_clipboard(
    app: &tauri::AppHandle,
    path: &std::path::Path,
) -> Result<(), String> {
    native().copy_files_to_clipboard(app, &[path.to_path_buf()])
}

/// A `tokio::process::Command` pre-configured for background helpers.
pub fn background_command(program: impl AsRef<std::ffi::OsStr>) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(program);
    native().prepare_background_command(&mut cmd);
    cmd
}
