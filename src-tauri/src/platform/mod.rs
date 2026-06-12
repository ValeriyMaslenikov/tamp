//! OS-specific strategies behind one interface. The ONLY place in the
//! codebase allowed to know which operating system it runs on is this
//! module's cfg-selected implementation; everything else calls [`native()`]
//! and stays platform-neutral. Adding an OS = one new module implementing
//! [`Platform`] plus one selection line below.

use std::path::PathBuf;

#[cfg(target_os = "macos")]
mod macos;
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
