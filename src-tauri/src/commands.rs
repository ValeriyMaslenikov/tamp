use crate::encoder::{Encoder, JobState, PostActions};
use crate::scanner::{self, RecentVideo};
use crate::settings::{self, Settings, SettingsState};
use std::path::PathBuf;
use std::sync::{MutexGuard, PoisonError};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::DialogExt;

const RECENTS_LIMIT: usize = 8;

fn lock_settings(state: &SettingsState) -> MutexGuard<'_, Settings> {
    // A poisoned lock only means another command panicked mid-write;
    // the settings value itself is still usable.
    state.0.lock().unwrap_or_else(PoisonError::into_inner)
}

#[tauri::command]
pub async fn list_recents(app: AppHandle) -> Result<Vec<RecentVideo>, String> {
    let folders: Vec<PathBuf> = {
        let state = app.state::<SettingsState>();
        let guard = lock_settings(&state);
        guard.watched_folders.iter().map(PathBuf::from).collect()
    };
    let mut videos =
        tauri::async_runtime::spawn_blocking(move || scanner::scan(&folders, RECENTS_LIMIT))
            .await
            .map_err(|e| format!("recents scan failed: {e}"))?;
    crate::thumbs::ensure_thumbs(&app, &mut videos).await;
    Ok(videos)
}

#[tauri::command]
pub fn get_settings(app: AppHandle, state: State<'_, SettingsState>) -> Settings {
    let mut settings = lock_settings(&state).clone();
    if let Ok(enabled) = app.autolaunch().is_enabled() {
        settings.launch_at_login = enabled;
    }
    settings
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, SettingsState>,
    settings: Settings,
) -> Result<Settings, String> {
    settings::validate(&settings)?;

    let previous_folders: Vec<String> = lock_settings(&state).watched_folders.clone();

    // Autostart sync is best-effort: a sandbox/profile quirk must not block saving.
    let autolaunch = app.autolaunch();
    let currently_enabled = autolaunch.is_enabled().unwrap_or(false);
    if settings.launch_at_login != currently_enabled {
        let result = if settings.launch_at_login {
            autolaunch.enable()
        } else {
            autolaunch.disable()
        };
        if let Err(e) = result {
            eprintln!("tamp: failed to update launch-at-login: {e}");
        }
    }

    let mut canonical = settings;
    canonical.launch_at_login = autolaunch.is_enabled().unwrap_or(canonical.launch_at_login);

    settings::save(&app, &canonical)?;

    for folder in &canonical.watched_folders {
        if !previous_folders.contains(folder) {
            if let Err(e) = app.asset_protocol_scope().allow_directory(folder, false) {
                eprintln!("tamp: failed to extend asset scope for {folder}: {e}");
            }
        }
    }

    *lock_settings(&state) = canonical.clone();

    if let Err(e) = app.emit("settings:changed", &canonical) {
        eprintln!("tamp: failed to emit settings:changed: {e}");
    }
    Ok(canonical)
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Option<String> {
    // blocking_pick_folder parks the calling thread while the native dialog
    // runs on the main thread, so it must not run on the async runtime itself.
    let picked =
        tauri::async_runtime::spawn_blocking(move || app.dialog().file().blocking_pick_folder())
            .await;
    match picked {
        Ok(Some(file_path)) => match file_path.into_path() {
            Ok(path) => Some(path.to_string_lossy().into_owned()),
            Err(e) => {
                eprintln!("tamp: folder picker returned a non-path location: {e}");
                None
            }
        },
        Ok(None) => None,
        Err(e) => {
            eprintln!("tamp: folder picker task failed: {e}");
            None
        }
    }
}

#[tauri::command]
pub fn enqueue(
    app: AppHandle,
    state: State<'_, SettingsState>,
    path: String,
    preset_id: String,
) -> Result<String, String> {
    let (preset, post) = {
        let guard = lock_settings(&state);
        let preset = guard
            .presets
            .iter()
            .find(|p| p.id == preset_id)
            .cloned()
            .ok_or_else(|| format!("unknown preset: {preset_id}"))?;
        let post = PostActions {
            copy_to_clipboard: guard.copy_to_clipboard,
            trash_original: guard.trash_original,
        };
        (preset, post)
    };
    app.state::<Encoder>()
        .enqueue(PathBuf::from(path), preset, post)
}

#[tauri::command]
pub fn cancel_job(app: AppHandle, id: String) {
    app.state::<Encoder>().cancel(&id);
}

#[tauri::command]
pub fn queue_state(app: AppHandle) -> Vec<JobState> {
    app.state::<Encoder>().snapshot()
}

#[tauri::command]
pub fn reveal(path: String) {
    #[cfg(target_os = "macos")]
    {
        if let Err(e) = std::process::Command::new("/usr/bin/open")
            .args(["-R", &path])
            .spawn()
        {
            eprintln!("tamp: failed to reveal {path}: {e}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
    }
}
