use crate::encoder::{Encoder, JobState, Phase, PostActions};
use crate::scanner::{self, RecentVideo};
use crate::settings::{self, OutputFormat, Preset, Settings, SettingsState};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{MutexGuard, PoisonError};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::DialogExt;

const RECENTS_LIMIT: usize = 8;

const TRASH_MULTI_PRESET_ERR: &str = "'Move original to Trash' is on, so the original disappears after the first conversion — only one preset per video. Turn the toggle off in Preferences to export several formats.";

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
    // Orphaned outputs only know their on-disk size; the journal remembers
    // what they were compressed from and with which preset.
    if let Some(journal) = app.try_state::<crate::journal::Journal>() {
        for video in videos.iter_mut().filter(|v| v.is_output) {
            if let Some(record) = journal.find_by_output(&video.path) {
                if let Some(meta) = video.conversion.as_mut() {
                    meta.original_bytes = Some(record.input_bytes);
                    meta.preset_name = Some(record.preset_name);
                }
            }
        }
    }
    crate::thumbs::ensure_thumbs(&app, &mut videos).await;
    crate::durations::fill(&app, &mut videos).await;
    Ok(videos)
}

/// Describe drag-and-dropped files as `RecentVideo`s so the panel can stage
/// them as rows. Files may live anywhere (outside the watched folders); the
/// encoder accepts arbitrary paths, so a staged row behaves like any other.
/// Non-video / missing paths are skipped.
#[tauri::command]
pub async fn describe_dropped(app: AppHandle, paths: Vec<String>) -> Vec<RecentVideo> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    scanner::describe_paths(&app, &paths).await
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

    // Validate the global shortcuts by actually registering them; on failure
    // the previous pair is re-registered and the save is rejected.
    {
        let previous = lock_settings(&state).clone();
        if crate::shortcuts::changed(&previous, &settings) {
            if let Err(err) = crate::shortcuts::apply(&app, &settings) {
                if let Err(rollback) = crate::shortcuts::apply(&app, &previous) {
                    crate::log_error!("failed to restore previous global shortcuts: {rollback}");
                }
                return Err(err);
            }
        }
    }

    let previous_folders: Vec<String> = lock_settings(&state).watched_folders.clone();

    // An autostart sync failure must not block saving the rest: persist with
    // the actual autostart state and surface the failure to the caller after.
    let autolaunch = app.autolaunch();
    let currently_enabled = autolaunch.is_enabled().unwrap_or(false);
    let mut autostart_error: Option<String> = None;
    if settings.launch_at_login != currently_enabled {
        let result = if settings.launch_at_login {
            autolaunch.enable()
        } else {
            autolaunch.disable()
        };
        if let Err(e) = result {
            crate::log_error!("failed to update launch-at-login: {e}");
            autostart_error = Some(e.to_string());
        }
    }

    let mut canonical = settings;
    // If both the sync and the re-read fail, the pre-sync reading is the best
    // guess at the actual state; the requested value is known not applied.
    canonical.launch_at_login = autolaunch
        .is_enabled()
        .unwrap_or(if autostart_error.is_some() {
            currently_enabled
        } else {
            canonical.launch_at_login
        });

    settings::save(&app, &canonical)?;

    for folder in &canonical.watched_folders {
        if !previous_folders.contains(folder) {
            if let Err(e) = app.asset_protocol_scope().allow_directory(folder, false) {
                crate::log_warn!("failed to extend asset scope for {folder}: {e}");
            }
        }
    }

    *lock_settings(&state) = canonical.clone();

    if let Err(e) = app.emit("settings:changed", &canonical) {
        crate::log_warn!("failed to emit settings:changed: {e}");
    }
    if let Some(reason) = autostart_error {
        return Err(format!("Couldn't update Launch at login: {reason}"));
    }
    Ok(canonical)
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Option<String> {
    // Flag the dialog as open so the release-only hide-on-blur handler does
    // not close the panel when the native picker takes focus; the guard
    // resets the flag on every exit path (including unwinds).
    struct DialogGuard<'a>(&'a crate::DialogOpen);
    impl Drop for DialogGuard<'_> {
        fn drop(&mut self) {
            self.0 .0.store(false, Ordering::SeqCst);
        }
    }
    let dialog_open = app.state::<crate::DialogOpen>();
    dialog_open.0.store(true, Ordering::SeqCst);
    let guard = DialogGuard(dialog_open.inner());

    // blocking_pick_folder parks the calling thread while the native dialog
    // runs on the main thread, so it must not run on the async runtime itself.
    let dialog_app = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog_app.dialog().file().blocking_pick_folder()
    })
    .await;
    drop(guard);

    // Defensive: if the panel still lost focus and hid, bring it back.
    if let Some(panel) = app.get_webview_window("panel") {
        if !panel.is_visible().unwrap_or(true) {
            let _ = panel.show();
        }
        let _ = panel.set_focus();
    }

    match picked {
        Ok(Some(file_path)) => match file_path.into_path() {
            Ok(path) => Some(path.to_string_lossy().into_owned()),
            Err(e) => {
                crate::log_warn!("folder picker returned a non-path location: {e}");
                None
            }
        },
        Ok(None) => None,
        Err(e) => {
            crate::log_error!("folder picker task failed: {e}");
            None
        }
    }
}

/// True when "Move original to Trash" would make `input_path` disappear
/// before a second, differently-configured conversion could run: some job for
/// the same input with a different preset config hash is neither failed nor
/// cancelled. Same-hash jobs are fine — a re-click just reuses the output.
/// Jobs are (input_path, phase, preset_hash).
fn trash_conflicts<'a>(
    jobs: impl IntoIterator<Item = (&'a str, Phase, &'a str)>,
    input_path: &str,
    requested_hash: &str,
) -> bool {
    jobs.into_iter().any(|(path, phase, hash)| {
        path == input_path
            && !matches!(phase, Phase::Failed | Phase::Cancelled)
            && hash != requested_hash
    })
}

/// The single enqueue path shared by `enqueue`, `custom_convert` and the
/// compress-latest global shortcut: applies the trash-original multi-preset
/// guard and the user's post-action/encoder settings.
fn enqueue_preset(app: &AppHandle, path: String, preset: Preset) -> Result<String, String> {
    let (post, use_hardware) = {
        let state = app.state::<SettingsState>();
        let guard = lock_settings(&state);
        let post = PostActions {
            copy_to_clipboard: guard.copy_to_clipboard,
            trash_original: guard.trash_original,
        };
        (post, guard.use_hardware_encoder)
    };
    let encoder = app.state::<Encoder>();
    if post.trash_original {
        let requested_hash = crate::encoder::plan::preset_hash(&preset);
        let snapshot = encoder.snapshot();
        let jobs = snapshot
            .iter()
            .map(|j| (j.input_path.as_str(), j.phase, j.preset_hash.as_str()));
        if trash_conflicts(jobs, &path, &requested_hash) {
            return Err(TRASH_MULTI_PRESET_ERR.to_string());
        }
    }
    encoder.enqueue(PathBuf::from(path), preset, post, use_hardware)
}

/// Enqueues `path` with the default preset; the compress-latest global
/// shortcut's entry point (`shortcuts.rs`).
pub(crate) fn enqueue_default(app: &AppHandle, path: String) -> Result<String, String> {
    let preset = {
        let state = app.state::<SettingsState>();
        let guard = lock_settings(&state);
        guard
            .presets
            .iter()
            .find(|p| p.id == guard.default_preset_id)
            .cloned()
            .ok_or_else(|| format!("unknown preset: {}", guard.default_preset_id))?
    };
    enqueue_preset(app, path, preset)
}

#[tauri::command]
pub fn enqueue(
    app: AppHandle,
    state: State<'_, SettingsState>,
    path: String,
    preset_id: String,
) -> Result<String, String> {
    let preset = {
        let guard = lock_settings(&state);
        guard
            .presets
            .iter()
            .find(|p| p.id == preset_id)
            .cloned()
            .ok_or_else(|| format!("unknown preset: {preset_id}"))?
    };
    enqueue_preset(&app, path, preset)
}

/// One-off conversion settings from the panel's "Custom…" page; mirrors the
/// preset's encode-affecting fields.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomConfig {
    pub target_mb: f64,
    pub max_fps: Option<u32>,
    pub max_width: Option<u32>,
    pub scale_percent: Option<u32>,
    pub strip_audio: bool,
    pub format: OutputFormat,
    // Older frontends omit the field; default keeps splitting off.
    #[serde(default)]
    pub split: crate::settings::SplitConfig,
}

#[tauri::command]
pub fn custom_convert(
    app: AppHandle,
    path: String,
    config: CustomConfig,
) -> Result<String, String> {
    // also rejects NaN, which would otherwise pass a `<= 0.0` check
    if !config.target_mb.is_finite() || config.target_mb <= 0.0 {
        return Err("target size must be greater than 0".to_string());
    }
    settings::validate_split(&config.split)?;
    let preset = Preset {
        id: "custom".into(),
        name: "Custom".into(),
        target_mb: config.target_mb,
        max_fps: config.max_fps,
        max_width: config.max_width,
        scale_percent: config.scale_percent,
        strip_audio: config.strip_audio,
        format: config.format,
        split: config.split,
    };
    enqueue_preset(&app, path, preset)
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
pub async fn ensure_preview(app: AppHandle, path: String) -> Result<String, String> {
    crate::previews::ensure_preview(&app, &path).await
}

#[tauri::command]
pub async fn copy_file(app: AppHandle, path: String) -> Result<(), String> {
    // The clipboard write round-trips through the main thread and blocks on
    // the reply, so keep it off the async runtime's worker threads.
    tauri::async_runtime::spawn_blocking(move || {
        crate::platform::copy_file_to_clipboard(&app, Path::new(&path))
    })
    .await
    .map_err(|e| format!("clipboard task failed: {e}"))?
}

#[tauri::command]
pub fn reveal(path: String) {
    #[cfg(target_os = "macos")]
    {
        if let Err(e) = std::process::Command::new("/usr/bin/open")
            .args(["-R", &path])
            .spawn()
        {
            crate::log_error!("failed to reveal {path}: {e}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INPUT: &str = "/rec/clip.mov";

    #[test]
    fn no_jobs_never_conflicts() {
        assert!(!trash_conflicts([], INPUT, "823f"));
    }

    #[test]
    fn different_hash_for_same_input_conflicts() {
        let jobs = [(INPUT, Phase::Done, "823f")];
        assert!(trash_conflicts(jobs, INPUT, "d6e4"));
    }

    #[test]
    fn same_hash_reclick_stays_allowed() {
        let jobs = [(INPUT, Phase::Done, "823f")];
        assert!(!trash_conflicts(jobs, INPUT, "823f"));
    }

    #[test]
    fn other_inputs_do_not_conflict() {
        let jobs = [("/rec/other.mov", Phase::Pass2, "823f")];
        assert!(!trash_conflicts(jobs, INPUT, "d6e4"));
    }

    #[test]
    fn failed_and_cancelled_jobs_do_not_conflict() {
        let jobs = [
            (INPUT, Phase::Failed, "823f"),
            (INPUT, Phase::Cancelled, "eb3d"),
        ];
        assert!(!trash_conflicts(jobs, INPUT, "d6e4"));
    }

    #[test]
    fn custom_config_without_split_defaults_to_off() {
        // The payload an older frontend (pre-split) sends must keep working.
        let json = r#"{
            "targetMb": 10.0,
            "maxFps": null,
            "maxWidth": null,
            "scalePercent": null,
            "stripAudio": false,
            "format": "mp4"
        }"#;
        let config: CustomConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.split, crate::settings::SplitConfig::default());
        assert_eq!(config.split.mode, crate::settings::SplitMode::Off);
    }

    #[test]
    fn custom_config_split_deserializes_camel_case() {
        let json = r#"{
            "targetMb": 10.0,
            "maxFps": null,
            "maxWidth": null,
            "scalePercent": null,
            "stripAudio": false,
            "format": "mp4",
            "split": { "mode": "static", "by": "seconds", "parts": 2, "seconds": 45 }
        }"#;
        let config: CustomConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.split.mode, crate::settings::SplitMode::Static);
        assert_eq!(config.split.by, crate::settings::StaticSplitBy::Seconds);
        assert_eq!(config.split.seconds, 45);
    }

    #[test]
    fn active_phases_all_conflict() {
        for phase in [
            Phase::Queued,
            Phase::Pass1,
            Phase::Pass2,
            Phase::Verifying,
            Phase::Done,
        ] {
            assert!(
                trash_conflicts([(INPUT, phase, "823f")], INPUT, "d6e4"),
                "phase must conflict"
            );
        }
    }
}
