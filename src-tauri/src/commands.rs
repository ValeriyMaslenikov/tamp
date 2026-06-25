use crate::encoder::{Encoder, JobState, Phase, PostActions};
use crate::scanner::{self, RecentVideo};
use crate::settings::{self, OutputFormat, Preset, Settings, SettingsState};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{MutexGuard, PoisonError};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::DialogExt;

const TRASH_MULTI_PRESET_ERR: &str = "'Move original to Trash' is on, so the original disappears after the first conversion — only one preset per video. Turn the toggle off in Preferences to export several formats.";

fn lock_settings(state: &SettingsState) -> MutexGuard<'_, Settings> {
    // A poisoned lock only means another command panicked mid-write;
    // the settings value itself is still usable.
    state.0.lock().unwrap_or_else(PoisonError::into_inner)
}

#[tauri::command]
pub async fn list_recents(app: AppHandle) -> Result<Vec<RecentVideo>, String> {
    let (folders, limit): (Vec<PathBuf>, usize) = {
        let state = app.state::<SettingsState>();
        let guard = lock_settings(&state);
        (
            guard.watched_folders.iter().map(PathBuf::from).collect(),
            guard.recents_limit,
        )
    };
    let mut videos = tauri::async_runtime::spawn_blocking(move || scanner::scan(&folders, limit))
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
                    // The record can hold several outputs (a split set); use the
                    // size recorded for the specific part this row is.
                    if let Some(out) = record.outputs.iter().find(|o| o.path == video.path) {
                        meta.output_bytes = out.bytes;
                    }
                }
            }
        }
    }
    // Thumbnails and durations are NOT filled here: a cold cache would block the
    // panel for many seconds (ffmpeg/ffprobe per miss, up to 200 videos). Rows
    // return immediately with thumb_path/duration_secs = None; the frontend
    // lazy-loads each row's thumbnail (`recent_thumb`) and duration
    // (`recent_duration`) as it scrolls into view (mirrors the Converted tab).
    Ok(videos)
}

/// The watched folders that exist but can't be read right now (offline network
/// drive, permission-denied), as display strings. Empty when every folder is
/// reachable or merely not-yet-created. The Videos tab uses this to show a
/// distinct "couldn't read <folder>" banner instead of the misleading
/// "no recordings" empty state when a share is offline. `list_recents`' own
/// return shape stays unchanged, so existing callers are unaffected.
#[tauri::command]
pub async fn unreachable_folders(app: AppHandle) -> Result<Vec<String>, String> {
    let folders: Vec<PathBuf> = {
        let state = app.state::<SettingsState>();
        let guard = lock_settings(&state);
        guard.watched_folders.iter().map(PathBuf::from).collect()
    };
    // read_dir on an offline UNC share can block, so probe off the async runtime.
    tauri::async_runtime::spawn_blocking(move || scanner::unreachable(&folders))
        .await
        .map_err(|e| format!("unreachable-folder probe failed: {e}"))
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

/// The directory to grant asset-protocol access to for `path`, or `None` when
/// it already sits inside a watched folder (whose dir is allowed at startup).
/// Lets thumbnails/previews load for files dropped or added from anywhere.
fn dir_to_allow(path: &std::path::Path, watched: &[String]) -> Option<std::path::PathBuf> {
    let parent = path.parent()?;
    let inside = watched
        .iter()
        .any(|w| path.starts_with(std::path::Path::new(w)));
    if inside {
        None
    } else {
        Some(parent.to_path_buf())
    }
}

/// The single enqueue path shared by `enqueue`, `custom_convert` and the
/// compress-latest global shortcut: applies the trash-original multi-preset
/// guard and the user's post-action/encoder settings.
fn enqueue_preset(app: &AppHandle, path: String, preset: Preset) -> Result<String, String> {
    {
        let state = app.state::<SettingsState>();
        let watched = lock_settings(&state).watched_folders.clone();
        if let Some(dir) = dir_to_allow(std::path::Path::new(&path), &watched) {
            if let Err(e) = app.asset_protocol_scope().allow_directory(&dir, false) {
                crate::log_warn!("failed to widen asset scope to {}: {e}", dir.display());
            }
        }
    }
    let (post, use_hardware) = {
        let state = app.state::<SettingsState>();
        let guard = lock_settings(&state);
        let post = PostActions {
            copy_to_clipboard: guard.copy_to_clipboard,
            trash_original: guard.trash_original,
            open_after: guard.open_after_convert,
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

/// Copies ALL `paths` to the clipboard in a single write (one CF_HDROP file
/// list on Windows), so a multi-part group lands every part — unlike calling
/// `copy_file` per path, where each write replaces the clipboard and only the
/// last survives.
#[tauri::command]
pub async fn copy_files(app: AppHandle, paths: Vec<String>) -> Result<(), String> {
    use crate::platform::Platform as _;
    tauri::async_runtime::spawn_blocking(move || {
        let pbs: Vec<std::path::PathBuf> = paths.iter().map(std::path::PathBuf::from).collect();
        crate::platform::native().copy_files_to_clipboard(&app, &pbs)
    })
    .await
    .map_err(|e| format!("clipboard task failed: {e}"))?
}

/// Reveals `path` in the OS file manager (Finder/Explorer/…). Returns an error
/// the frontend can toast: a moved/deleted output is caught by the existence
/// pre-check (the opener would otherwise fail or silently no-op), mirroring
/// `open_file`/`copy_file` so a real failure surfaces instead of only logging.
#[tauri::command]
pub fn reveal(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt as _;
    if !Path::new(&path).exists() {
        return Err(format!("File no longer exists at {path}"));
    }
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| format!("couldn't reveal {path}: {e}"))
}

/// The backend OS ("macos" | "windows" | "linux") for per-platform UI labels.
#[tauri::command]
pub fn os_info() -> &'static str {
    std::env::consts::OS
}

/// The current notification permission state as a lowercase string
/// ("granted" | "denied" | "prompt" | "prompt-with-rationale" | "unsupported").
/// Drives the Preferences "notifications are off" recovery row: only a real
/// "denied" surfaces it. On desktop the notification plugin always reports
/// "granted" (the OS owns the toggle), so the row stays hidden there.
#[tauri::command]
pub fn notification_permission(app: AppHandle) -> &'static str {
    crate::shortcuts::permission_state_str(&app)
}

/// Re-requests the notification permission (the Preferences "Enable" button when
/// notifications are off) and returns the resulting state in the same vocabulary
/// as `notification_permission`. A hard OS-level deny is unaffected — the user
/// must flip it in System Settings — but the call is safe and reports back.
#[tauri::command]
pub fn request_notification_permission(app: AppHandle) -> &'static str {
    crate::shortcuts::request_permission_str(&app)
}

/// Deep-links to the OS notifications settings so a user with a hard "denied"
/// state (where re-requesting is a no-op, e.g. macOS) has a one-click recovery
/// path instead of hunting through System Settings by hand. The per-OS URL is
/// `#[cfg]`-gated; on platforms without a known deep link this is an Ok no-op so
/// the frontend can offer the button unconditionally without erroring.
#[tauri::command]
pub fn open_notification_settings(app: AppHandle) -> Result<(), String> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        use tauri_plugin_opener::OpenerExt as _;
        #[cfg(target_os = "macos")]
        let url = "x-apple.systempreferences:com.apple.preference.notifications";
        #[cfg(target_os = "windows")]
        let url = "ms-settings:notifications";
        app.opener()
            .open_url(url, None::<&str>)
            .map_err(|e| format!("couldn't open notification settings: {e}"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = app;
        Ok(())
    }
}

/// The conversion history (newest first) for the Converted tab. The source's
/// creation time is frozen in the record: it's captured at encode time and
/// backfilled once during the journal's one-time migration, never re-read live
/// here (a moved/edited source must not change the recorded "Created" time).
#[tauri::command]
pub fn list_conversions(app: AppHandle) -> Vec<crate::journal::ConversionRecord> {
    app.try_state::<crate::journal::Journal>()
        .map(|j| j.records())
        .unwrap_or_default()
}

/// Registers/removes the Windows Explorer "Compress with tamp" entry and
/// persists the choice. No-op (Ok) on non-Windows.
#[tauri::command]
pub fn set_context_menu(app: AppHandle, enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    crate::platform::context_menu::apply(enabled)?;
    {
        let state = app.state::<SettingsState>();
        let mut guard = lock_settings(&state);
        guard.context_menu_enabled = enabled;
        settings::save(&app, &guard)?;
    }
    Ok(())
}

/// Opens a native multi-select dialog filtered to video files; returns the
/// chosen paths (empty if cancelled). Mirrors `pick_folder`'s dialog guard so
/// the release-only hide-on-blur handler doesn't close the panel.
#[tauri::command]
pub async fn pick_videos(app: AppHandle) -> Vec<String> {
    struct DialogGuard<'a>(&'a crate::DialogOpen);
    impl Drop for DialogGuard<'_> {
        fn drop(&mut self) {
            self.0 .0.store(false, Ordering::SeqCst);
        }
    }
    let dialog_open = app.state::<crate::DialogOpen>();
    dialog_open.0.store(true, Ordering::SeqCst);
    let guard = DialogGuard(dialog_open.inner());

    let dialog_app = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .file()
            .add_filter("Video", &["mov", "mp4", "m4v", "webm", "mkv", "avi"])
            .blocking_pick_files()
    })
    .await;
    drop(guard);

    if let Some(panel) = app.get_webview_window("panel") {
        if !panel.is_visible().unwrap_or(true) {
            let _ = panel.show();
        }
        let _ = panel.set_focus();
    }

    match picked {
        Ok(Some(files)) => files
            .into_iter()
            .filter_map(|f| f.into_path().ok().map(|p| p.to_string_lossy().into_owned()))
            .collect(),
        _ => Vec::new(),
    }
}

/// Toggles the session-only "keep the panel open" pin.
#[tauri::command]
pub fn set_pin(app: AppHandle, pinned: bool) {
    if let Some(state) = app.try_state::<crate::Pinned>() {
        state.0.store(pinned, Ordering::SeqCst);
    }
}

/// Opens `path` in the OS default application (used by the Converted tab's
/// ▶ play button to preview an output).
#[tauri::command]
pub fn open_file(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt as _;
    app.opener()
        .open_path(&path, None::<&str>)
        .map_err(|e| format!("couldn't open {path}: {e}"))
}

/// Opens an external `url` in the default browser. Backs the update-available
/// modal's "What's new" / "Download" actions: opening through Rust keeps the
/// panel window's CSP and capabilities unchanged (no opener permission for the
/// frontend). Mirrors `open_notification_settings`' opener use.
#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt as _;
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| format!("couldn't open {url}: {e}"))
}

/// Ensures (generating on miss) a thumbnail for a single video and returns its
/// cached path, or None on failure. Backs the Converted tab's per-row preview.
#[tauri::command]
pub async fn conversion_thumb(app: AppHandle, path: String) -> Option<String> {
    // Reuse the same single-frame thumbnail generation ensure_thumbs uses.
    crate::thumbs::ensure_one(&app, Path::new(&path)).await
}

/// Ensures (generating on miss) a thumbnail for one recent video, returning its
/// cached path or None. The Videos tab calls this per row as it scrolls into
/// view so the panel opens instantly instead of blocking on the whole list.
#[tauri::command]
pub async fn recent_thumb(app: AppHandle, path: String) -> Result<Option<String>, String> {
    Ok(crate::thumbs::ensure_one(&app, Path::new(&path)).await)
}

/// Resolves the duration (seconds) for one recent video: a cache hit returns
/// immediately, a miss probes once with ffprobe and caches the result. The
/// Videos tab calls this per row (lazy, like `recent_thumb`). None when the
/// probe fails (retried on the next listing).
#[tauri::command]
pub async fn recent_duration(app: AppHandle, path: String) -> Result<Option<f64>, String> {
    Ok(crate::durations::ensure_one(&app, Path::new(&path)).await)
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

#[cfg(test)]
mod scope_tests {
    use super::dir_to_allow;
    use std::path::{Path, PathBuf};

    // `dir_to_allow`'s containment/parent logic is the same on every OS, but the
    // literals must use the test platform's own separators and root — a Windows
    // `C:\…` string is a single (parent-less) component on Unix, so reuse one set
    // of paths per OS.
    #[cfg(windows)]
    mod paths {
        pub const WATCHED: &str = "C:\\Users\\me\\Videos";
        pub const OUTSIDE_FILE: &str = "C:\\Downloads\\clip.mp4";
        pub const OUTSIDE_DIR: &str = "C:\\Downloads";
        pub const INSIDE_FILE: &str = "C:\\Users\\me\\Videos\\rec.mp4";
    }
    #[cfg(not(windows))]
    mod paths {
        pub const WATCHED: &str = "/home/me/Videos";
        pub const OUTSIDE_FILE: &str = "/home/me/Downloads/clip.mp4";
        pub const OUTSIDE_DIR: &str = "/home/me/Downloads";
        pub const INSIDE_FILE: &str = "/home/me/Videos/rec.mp4";
    }

    #[test]
    fn allows_parent_of_a_file_outside_watched_folders() {
        let watched = vec![paths::WATCHED.to_string()];
        let got = dir_to_allow(Path::new(paths::OUTSIDE_FILE), &watched);
        assert_eq!(got, Some(PathBuf::from(paths::OUTSIDE_DIR)));
    }

    #[test]
    fn skips_files_already_inside_a_watched_folder() {
        let watched = vec![paths::WATCHED.to_string()];
        let got = dir_to_allow(Path::new(paths::INSIDE_FILE), &watched);
        assert_eq!(got, None);
    }
}
