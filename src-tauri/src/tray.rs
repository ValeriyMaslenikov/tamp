use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::platform::Platform as _;

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let open_logs = MenuItemBuilder::with_id("open-logs", "Open Logs").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit Tamp").build(app)?;
    let menu = MenuBuilder::new(app).item(&open_logs).item(&quit).build()?;

    TrayIconBuilder::with_id("main")
        .icon(tauri::include_image!("icons/trayicon.png"))
        .icon_as_template(true)
        .tooltip("Tamp")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open-logs" => open_logs_dir(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // The positioner caches the tray rect from these events;
            // it must see them before any TrayBottomCenter move.
            tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_panel(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Opens the app's log directory in the system file manager. The dir is
/// created first so a fresh install (or a failed logger init) still has
/// something to open.
fn open_logs_dir(app: &AppHandle) {
    use tauri_plugin_opener::OpenerExt as _;
    let dir = match app.path().app_log_dir() {
        Ok(dir) => dir,
        Err(e) => {
            crate::log_error!("cannot resolve app log dir: {e}");
            return;
        }
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        crate::log_error!("cannot create log dir {}: {e}", dir.display());
        return;
    }
    if let Err(e) = app.opener().open_path(dir.to_string_lossy(), None::<&str>) {
        crate::log_error!("failed to open log dir {}: {e}", dir.display());
    }
}

fn toggle_panel(app: &AppHandle) {
    let Some(panel) = app.get_webview_window("panel") else {
        crate::log_error!("panel window not found");
        return;
    };
    if panel.is_visible().unwrap_or(false) {
        if let Err(e) = panel.hide() {
            crate::log_warn!("failed to hide panel: {e}");
        }
    } else {
        crate::platform::native().position_panel_at_tray(&panel);
        if let Err(e) = panel.show() {
            crate::log_warn!("failed to show panel: {e}");
        }
        let _ = panel.set_focus();
        if let Err(e) = app.emit("panel:shown", ()) {
            crate::log_warn!("failed to emit panel:shown: {e}");
        }
    }
}

/// Sets the text shown next to the tray icon (macOS-only capability; the
/// macOS platform strategy is the only caller).
#[cfg(target_os = "macos")]
pub fn set_title(app: &AppHandle, text: Option<String>) {
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    if let Err(e) = tray.set_title(text.as_deref()) {
        crate::log_warn!("failed to set tray title: {e}");
    }
}
