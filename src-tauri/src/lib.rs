mod commands;
mod durations;
pub mod encoder;
pub mod journal;
pub mod logging;
pub mod platform;
mod previews;
mod scanner;
pub mod settings;
mod shortcuts;
mod thumbs;
mod tray;

use platform::Platform as _;
use std::sync::atomic::AtomicBool;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as _};

/// The bundle identifier before the io.github rebrand; referenced only to
/// migrate user data left behind under the old app-data directory.
const LEGACY_IDENTIFIER: &str = "com.joystudios.tamp";

/// True while a native dialog (folder picker) is open; the release-only
/// hide-on-blur handler must not close the panel when the dialog takes focus.
pub struct DialogOpen(pub AtomicBool);

/// One-time, best-effort migration of settings and the conversion journal
/// from the pre-rebrand app-data dir. Must run before `settings::load`: a
/// rebranded install starts with an empty app-data dir and would otherwise
/// fall back to defaults even though the user's data sits right next door.
fn migrate_legacy_data(app: &AppHandle) {
    let data_dir = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(e) => {
            log_error!("cannot resolve app data dir for legacy migration: {e}");
            return;
        }
    };
    if data_dir.join("settings.json").exists() {
        return; // already migrated, or an install that has saved settings
    }
    let Some(legacy_dir) = data_dir.parent().map(|p| p.join(LEGACY_IDENTIFIER)) else {
        return;
    };
    if !legacy_dir.join("settings.json").exists() {
        return;
    }
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        log_error!("cannot create app data dir for legacy migration: {e}");
        return;
    }
    for file in ["settings.json", "conversions.json"] {
        let src = legacy_dir.join(file);
        if !src.exists() {
            continue;
        }
        match std::fs::copy(&src, data_dir.join(file)) {
            Ok(_) => log_info!("migrated {file} from {}", legacy_dir.display()),
            Err(e) => log_error!("failed to migrate legacy {file}: {e}"),
        }
    }
}

/// Shows the panel when there is no tray-click rect to position against
/// (app relaunch, Dock/Finder reopen).
fn show_panel_fallback(app: &AppHandle) {
    if let Some(panel) = app.get_webview_window("panel") {
        // No tray click happened, so the positioner has no tray rect; the
        // platform strategy drops the panel in its tray corner of the primary
        // monitor's work area. (Current-monitor positioning can land on a
        // sleeping display, so anchor to the primary one.)
        if let Ok(Some(monitor)) = panel.primary_monitor() {
            platform::native().position_panel_fallback(&panel, &monitor);
        }
        let _ = panel.show();
        let _ = panel.set_focus();
        let _ = app.emit("panel:shown", ());
    }
}

/// Keyboard-driven panel toggle (global shortcut): hide when visible,
/// otherwise show via the tray-less fallback positioning — a shortcut press
/// has no tray rect for the positioner to use.
pub(crate) fn toggle_panel_fallback(app: &AppHandle) {
    if let Some(panel) = app.get_webview_window("panel") {
        if panel.is_visible().unwrap_or(false) {
            if let Err(e) = panel.hide() {
                log_warn!("failed to hide panel: {e}");
            }
            return;
        }
    }
    show_panel_fallback(app);
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
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            platform::native().configure_app(app);

            // File logging first so everything below (including migration and
            // settings load) is captured. Failure is non-fatal: debug builds
            // still mirror to stderr.
            match app.path().app_log_dir() {
                Ok(dir) => {
                    if let Err(e) = logging::init(dir) {
                        eprintln!("tamp: cannot initialize file logging: {e}");
                    }
                }
                Err(e) => eprintln!("tamp: cannot resolve app log dir: {e}"),
            }
            log_info!(
                "tamp {} ({}) starting",
                app.package_info().version,
                app.config().identifier
            );

            migrate_legacy_data(app.handle());
            let loaded = settings::load(app.handle());

            // The pre-rebrand LaunchAgent pointed at the old bundle
            // identifier; re-enabling rewrites it against the current app.
            if loaded.launch_at_login {
                if let Err(e) = app.autolaunch().enable() {
                    log_warn!("failed to refresh launch-at-login agent: {e}");
                }
            }

            let scope = app.asset_protocol_scope();
            for folder in &loaded.watched_folders {
                if let Err(e) = scope.allow_directory(folder, false) {
                    log_warn!("failed to allow asset access to {folder}: {e}");
                }
            }
            match app.path().app_cache_dir() {
                Ok(cache_dir) => {
                    for (subdir, what) in [("thumbs", "thumbnail"), ("previews", "preview")] {
                        let dir = cache_dir.join(subdir);
                        if let Err(e) = std::fs::create_dir_all(&dir) {
                            log_warn!("failed to create {what} cache dir: {e}");
                        }
                        if let Err(e) = scope.allow_directory(&dir, false) {
                            log_warn!("failed to allow asset access to {what} cache: {e}");
                        }
                    }
                }
                Err(e) => log_warn!("cannot resolve app cache dir: {e}"),
            }

            app.manage(settings::SettingsState(std::sync::Mutex::new(
                loaded.clone(),
            )));
            app.manage(DialogOpen(AtomicBool::new(false)));
            app.manage(journal::Journal::load(app.handle()));
            app.manage(durations::Durations::load(app.handle()));
            app.manage(previews::Previews::default());
            app.manage(encoder::Encoder::start(app.handle().clone()));
            tray::create(app.handle())?;
            // Repaint the tray to its idle state so platforms that recolor the
            // icon per system theme (Windows, where the bundled template glyph
            // isn't auto-inverted) show a contrasting icon from launch, not
            // just once an encode starts. No-op beyond a title clear on macOS.
            platform::native().tray_progress(app.handle(), None);

            // After the managed state the handlers rely on. Startup
            // registration is best-effort: a stored accelerator another app
            // grabbed since must not prevent launching (the next
            // save_settings surfaces the error to the user).
            if let Err(e) = shortcuts::apply(app.handle(), &loaded) {
                log_warn!("failed to register global shortcuts: {e}");
            }

            if let Some(panel) = app.get_webview_window("panel") {
                if let Err(e) = platform::native().configure_panel(&panel) {
                    log_warn!("failed to configure panel: {e}");
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
                        log_warn!("failed to hide panel on focus loss: {e}");
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
            commands::custom_convert,
            commands::cancel_job,
            commands::queue_state,
            commands::ensure_preview,
            commands::copy_file,
            commands::reveal,
            commands::os_info
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
