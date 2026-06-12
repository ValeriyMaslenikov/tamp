#[cfg(target_os = "macos")]
mod macos;

/// Puts the files (as file references, not their contents) on the system
/// clipboard in one write, so a multi-file paste lands all of them into
/// Finder, Slack, Discord, etc. — split jobs copy every part this way.
pub fn copy_files_to_clipboard(
    app: &tauri::AppHandle,
    paths: &[std::path::PathBuf],
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::copy_files_to_clipboard(app, paths)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, paths);
        Err("unsupported".to_string())
    }
}

/// Single-file convenience over [`copy_files_to_clipboard`].
pub fn copy_file_to_clipboard(
    app: &tauri::AppHandle,
    path: &std::path::Path,
) -> Result<(), String> {
    copy_files_to_clipboard(app, &[path.to_path_buf()])
}

/// Platform-specific window tweaks for the tray panel; on macOS this lets it
/// appear over full-screen apps.
pub fn configure_panel(window: &tauri::WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::configure_panel(window)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window;
        Ok(())
    }
}
