use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_positioner::{Position, WindowExt};

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let quit = MenuItemBuilder::with_id("quit", "Quit tamp").build(app)?;
    let menu = MenuBuilder::new(app).item(&quit).build()?;

    TrayIconBuilder::with_id("main")
        .icon(tauri::include_image!("icons/trayicon.png"))
        .icon_as_template(true)
        .tooltip("tamp")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "quit" {
                app.exit(0);
            }
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

fn toggle_panel(app: &AppHandle) {
    let Some(panel) = app.get_webview_window("panel") else {
        eprintln!("tamp: panel window not found");
        return;
    };
    if panel.is_visible().unwrap_or(false) {
        if let Err(e) = panel.hide() {
            eprintln!("tamp: failed to hide panel: {e}");
        }
    } else {
        if let Err(e) = panel.move_window(Position::TrayBottomCenter) {
            eprintln!("tamp: failed to position panel under tray icon: {e}");
        }
        if let Err(e) = panel.show() {
            eprintln!("tamp: failed to show panel: {e}");
        }
        let _ = panel.set_focus();
        if let Err(e) = app.emit("panel:shown", ()) {
            eprintln!("tamp: failed to emit panel:shown: {e}");
        }
    }
}

pub fn set_progress(app: &AppHandle, text: Option<String>) {
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    if let Err(e) = tray.set_title(text.as_deref()) {
        eprintln!("tamp: failed to set tray title: {e}");
    }
}
