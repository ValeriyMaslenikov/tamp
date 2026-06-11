mod commands;
pub mod encoder;
pub mod platform;
mod scanner;
pub mod settings;
mod thumbs;
mod tray;

use std::sync::atomic::AtomicBool;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::MacosLauncher;

/// True while a native dialog (folder picker) is open; the release-only
/// hide-on-blur handler must not close the panel when the dialog takes focus.
pub struct DialogOpen(pub AtomicBool);

/// Shows the panel when there is no tray-click rect to position against
/// (app relaunch, Dock/Finder reopen).
fn show_panel_fallback(app: &AppHandle) {
    if let Some(panel) = app.get_webview_window("panel") {
        // No tray click happened, so the positioner has no tray rect;
        // top-right of the primary monitor approximates the tray area.
        // (Current-monitor positioning can land on a sleeping display.)
        if let Ok(Some(monitor)) = panel.primary_monitor() {
            let scale = monitor.scale_factor();
            let size = monitor.size().to_logical::<f64>(scale);
            let pos = monitor.position().to_logical::<f64>(scale);
            let width = panel
                .outer_size()
                .map(|s| s.width as f64 / scale)
                .unwrap_or(420.0);
            let _ = panel.set_position(tauri::LogicalPosition::new(
                pos.x + size.width - width - 8.0,
                pos.y + 32.0,
            ));
        }
        let _ = panel.show();
        let _ = panel.set_focus();
        let _ = app.emit("panel:shown", ());
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_panel_fallback(app);
        }))
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let loaded = settings::load(app.handle());

            let scope = app.asset_protocol_scope();
            for folder in &loaded.watched_folders {
                if let Err(e) = scope.allow_directory(folder, false) {
                    eprintln!("tamp: failed to allow asset access to {folder}: {e}");
                }
            }
            match app.path().app_cache_dir() {
                Ok(cache_dir) => {
                    let thumbs_dir = cache_dir.join("thumbs");
                    if let Err(e) = std::fs::create_dir_all(&thumbs_dir) {
                        eprintln!("tamp: failed to create thumbnail cache dir: {e}");
                    }
                    if let Err(e) = scope.allow_directory(&thumbs_dir, false) {
                        eprintln!("tamp: failed to allow asset access to thumbnail cache: {e}");
                    }
                }
                Err(e) => eprintln!("tamp: cannot resolve app cache dir: {e}"),
            }

            app.manage(settings::SettingsState(std::sync::Mutex::new(loaded)));
            app.manage(DialogOpen(AtomicBool::new(false)));
            app.manage(encoder::Encoder::start(app.handle().clone()));
            tray::create(app.handle())?;

            // Tray panels must follow the user across Spaces/displays; without
            // this the panel opens on the Space the app launched on.
            if let Some(panel) = app.get_webview_window("panel") {
                if let Err(e) = panel.set_visible_on_all_workspaces(true) {
                    eprintln!("tamp: failed to set panel visible on all workspaces: {e}");
                }
                if let Err(e) = platform::configure_panel(&panel) {
                    eprintln!("tamp: failed to configure panel for full-screen overlay: {e}");
                }
            }
            Ok(())
        })
        .on_window_event(|_window, _event| {
            // Hiding on focus loss is release-only: in dev the devtools window
            // steals focus and would close the panel the moment it opens.
            #[cfg(not(debug_assertions))]
            if let tauri::WindowEvent::Focused(false) = _event {
                if _window.label() == "panel" {
                    // A native dialog (folder picker) taking focus must not
                    // close the panel out from under it.
                    let dialog_open = _window
                        .app_handle()
                        .try_state::<DialogOpen>()
                        .is_some_and(|s| s.0.load(std::sync::atomic::Ordering::SeqCst));
                    if dialog_open {
                        return;
                    }
                    if let Err(e) = _window.hide() {
                        eprintln!("tamp: failed to hide panel on focus loss: {e}");
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_recents,
            commands::get_settings,
            commands::save_settings,
            commands::pick_folder,
            commands::enqueue,
            commands::cancel_job,
            commands::queue_state,
            commands::reveal
        ])
        .build(tauri::generate_context!())
        .expect("error while building tamp");

    app.run(|app_handle, event| match event {
        tauri::RunEvent::ExitRequested {
            code: None, api, ..
        } => {
            // Keep running when the last window hides; we live in the tray.
            api.prevent_exit();
        }
        tauri::RunEvent::Exit => {
            if let Some(encoder) = app_handle.try_state::<encoder::Encoder>() {
                encoder.shutdown();
            }
        }
        // Relaunching from Finder/Dock should bring the panel back.
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen { .. } => {
            show_panel_fallback(app_handle);
        }
        _ => {}
    });
}
