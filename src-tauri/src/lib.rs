mod commands;
pub mod encoder;
pub mod platform;
mod scanner;
pub mod settings;
mod thumbs;
mod tray;

use tauri::{Emitter, Manager};
use tauri_plugin_autostart::MacosLauncher;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(panel) = app.get_webview_window("panel") {
                let _ = panel.show();
                let _ = panel.set_focus();
                let _ = app.emit("panel:shown", ());
            }
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
            app.manage(encoder::Encoder::start(app.handle().clone()));
            tray::create(app.handle())?;
            Ok(())
        })
        .on_window_event(|_window, _event| {
            // Hiding on focus loss is release-only: in dev the devtools window
            // steals focus and would close the panel the moment it opens.
            #[cfg(not(debug_assertions))]
            if let tauri::WindowEvent::Focused(false) = _event {
                if _window.label() == "panel" {
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
        _ => {}
    });
}
