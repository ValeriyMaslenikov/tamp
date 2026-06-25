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
mod update_check;

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

/// Session-only "keep the panel open" flag, toggled by the pin button.
pub struct Pinned(pub AtomicBool);

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

/// True only when the `TAMP_E2E` env var is exactly `"1"`. Pure (takes the
/// already-read value) so the guard is unit-testable without touching the
/// process environment; any other value — including unset (`None`), empty, or
/// `"0"` — keeps the test mode off.
fn e2e_mode_enabled(var: Option<&str>) -> bool {
    var == Some("1")
}

/// E2E test mode: when `TAMP_E2E=1` is set at startup, surface the panel and
/// set the session pin so it stays open. The release-only hide-on-blur handler
/// closes the panel the instant WebDriver's automation window takes focus
/// (smart-hide); pinning it (the same flag the pin button toggles via
/// `set_pin`) keeps it attachable so `tauri-driver` can drive the WebView2
/// window. Strictly guarded by the env var: a complete no-op on every normal
/// run (the var is never set outside the WDIO harness), so there is zero effect
/// on shipped behavior.
fn apply_e2e_mode(app: &AppHandle) {
    if !e2e_mode_enabled(std::env::var("TAMP_E2E").ok().as_deref()) {
        return;
    }
    log_info!("TAMP_E2E=1: showing and pinning the panel for WebDriver");
    if let Some(state) = app.try_state::<Pinned>() {
        state.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    show_panel_fallback(app);
}

/// Whether the panel should hide when it loses focus. It stays open while a
/// native dialog is up, while pinned, or while the primary mouse button is held
/// (a drag is in flight and may be heading to us). Only the release-only
/// hide-on-blur handler (and tests) call it, so it's dead code in a debug lib
/// build where that handler is compiled out.
#[cfg_attr(debug_assertions, allow(dead_code))]
fn should_hide_on_blur(dialog_open: bool, pinned: bool, mouse_button_down: bool) -> bool {
    !dialog_open && !pinned && !mouse_button_down
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // A second launch (e.g. the Explorer "Compress with tamp" entry,
            // which runs `tamp.exe "<file>"`) forwards its args here. Compress
            // the file if one was passed; otherwise just surface the panel.
            if !compress_file_args(app, &args) {
                show_panel_fallback(app);
            }
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

            // Drop any stale launch-at-login agent a pre-rebrand build left
            // behind (a macOS LaunchAgent named by the legacy bundle id), then
            // re-enable under the current identity. The modern agent is named by
            // the rebrand-stable product name, so re-enabling rewrites it in
            // place; this only sweeps the orphan the rename would miss. No-op on
            // Windows.
            platform::native().cleanup_legacy_autostart();
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
            app.manage(Pinned(AtomicBool::new(false)));
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

            #[cfg(target_os = "windows")]
            if let Err(e) = platform::context_menu::apply(loaded.context_menu_enabled) {
                log_warn!("failed to apply context-menu setting at startup: {e}");
            }

            if let Some(panel) = app.get_webview_window("panel") {
                if let Err(e) = platform::native().configure_panel(&panel) {
                    log_warn!("failed to configure panel: {e}");
                }
            }

            // First launch may itself carry a file (the context menu launching
            // tamp for the first time, or `tamp <file>` from a shell).
            let argv: Vec<String> = std::env::args().collect();
            compress_file_args(app.handle(), &argv);

            // Guarded test mode (no-op unless TAMP_E2E=1): show + pin the panel
            // so the tauri-driver smoke suite can attach to the WebView2 window.
            apply_e2e_mode(app.handle());
            Ok(())
        })
        .on_window_event(|_window, _event| {
            // A live system light/dark flip while idle must repaint the tray:
            // platforms that recolor the glyph per taskbar theme (Windows) pick
            // their ink at icon-update time, so without this the idle glyph can
            // become invisible until the next encode. tray_progress(None) re-reads
            // the current ink and repaints the idle icon; it's a no-op beyond a
            // title clear on macOS, whose template icon auto-inverts. The
            // app-theme event is the agreed best-effort proxy for the taskbar
            // theme (the panel window pins no theme, so it's delivered).
            if let tauri::WindowEvent::ThemeChanged(_) = _event {
                platform::native().tray_progress(_window.app_handle(), None);
            }

            // Hiding on focus loss is release-only: in dev the devtools window
            // steals focus and would close the panel the moment it opens.
            #[cfg(not(debug_assertions))]
            if let tauri::WindowEvent::Focused(false) = _event {
                if _window.label() == "panel" {
                    let app = _window.app_handle();
                    // A native dialog (folder picker) taking focus, the pin, or
                    // an in-flight drag (mouse button held) must each keep the
                    // panel open out from under them.
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
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_recents,
            commands::unreachable_folders,
            commands::get_settings,
            commands::save_settings,
            commands::pick_folder,
            commands::enqueue,
            commands::custom_convert,
            commands::cancel_job,
            commands::queue_state,
            commands::notification_permission,
            commands::request_notification_permission,
            commands::open_notification_settings,
            commands::ensure_preview,
            commands::copy_file,
            commands::copy_files,
            commands::reveal,
            commands::os_info,
            commands::list_conversions,
            commands::set_context_menu,
            commands::set_pin,
            commands::pick_videos,
            commands::open_file,
            commands::open_url,
            commands::conversion_thumb,
            commands::recent_thumb,
            commands::recent_duration,
            update_check::check_for_update
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

#[cfg(test)]
mod e2e_mode_tests {
    use super::e2e_mode_enabled;

    #[test]
    fn enabled_only_for_exactly_one() {
        assert!(e2e_mode_enabled(Some("1")));
    }

    #[test]
    fn disabled_when_unset_or_other_value() {
        assert!(!e2e_mode_enabled(None), "unset");
        assert!(!e2e_mode_enabled(Some("")), "empty");
        assert!(!e2e_mode_enabled(Some("0")), "zero");
        assert!(!e2e_mode_enabled(Some("true")), "true");
    }
}

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
        assert!(
            !should_hide_on_blur(false, false, true),
            "mouse button held (drag)"
        );
    }
}
