#[cfg(target_os = "macos")]
mod macos;

/// Puts the file (as a file reference, not its contents) on the system
/// clipboard so it can be pasted into Finder, Slack, Discord, etc.
pub fn copy_file_to_clipboard(
    app: &tauri::AppHandle,
    path: &std::path::Path,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::copy_file_to_clipboard(app, path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, path);
        Err("unsupported".to_string())
    }
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
