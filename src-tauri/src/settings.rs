use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "settings.json";
const STORE_KEY: &str = "settings";

/// Container/codec family a preset encodes to. Mp4 is the historical default;
/// presets stored before the field existed deserialize as Mp4.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Mp4,
    Webm,
    Gif,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub target_mb: f64,
    pub max_fps: Option<u32>,
    pub max_width: Option<u32>,
    pub scale_percent: Option<u32>,
    pub strip_audio: bool,
    #[serde(default)]
    pub format: OutputFormat,
}

// Field-level defaults keep previously stored settings readable when new
// fields are added later (missing keys no longer fail deserialization).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub watched_folders: Vec<String>,
    #[serde(default = "default_true")]
    pub copy_to_clipboard: bool,
    #[serde(default)]
    pub trash_original: bool,
    #[serde(default = "default_true")]
    pub use_hardware_encoder: bool,
    #[serde(default)]
    pub presets: Vec<Preset>,
    #[serde(default)]
    pub default_preset_id: String,
    #[serde(default)]
    pub launch_at_login: bool,
    /// Global shortcut that compresses the newest recording with the default
    /// preset. `None` or an empty string disables it.
    #[serde(default = "default_shortcut_compress_latest")]
    pub shortcut_compress_latest: Option<String>,
    /// Global shortcut that toggles the tray panel. `None`/empty disables it.
    #[serde(default = "default_shortcut_toggle_panel")]
    pub shortcut_toggle_panel: Option<String>,
    /// The compress-latest shortcut warns (via notification) when the newest
    /// video is older than this many minutes — it probably isn't the clip the
    /// user thinks they're compressing.
    #[serde(default = "default_stale_warn_minutes")]
    pub stale_warn_minutes: u32,
}

fn default_true() -> bool {
    true
}

fn default_shortcut_compress_latest() -> Option<String> {
    Some("CmdOrCtrl+Alt+T".into())
}

fn default_shortcut_toggle_panel() -> Option<String> {
    Some("CmdOrCtrl+Alt+O".into())
}

fn default_stale_warn_minutes() -> u32 {
    10
}

pub struct SettingsState(pub Mutex<Settings>);

/// Default settings. Needs the app handle because the default watched folder
/// is `~/Desktop` and home resolution goes through the Tauri path resolver.
pub fn default_settings(app: &AppHandle) -> Settings {
    let watched_folders = app
        .path()
        .home_dir()
        .map(|home| vec![home.join("Desktop").to_string_lossy().into_owned()])
        .unwrap_or_else(|e| {
            crate::log_warn!("cannot resolve home dir for default watched folder: {e}");
            Vec::new()
        });
    Settings {
        watched_folders,
        copy_to_clipboard: true,
        trash_original: false,
        use_hardware_encoder: true,
        presets: vec![Preset {
            id: "discord-10mb".into(),
            name: "Discord (10MB)".into(),
            target_mb: 10.0,
            max_fps: None,
            max_width: None,
            scale_percent: None,
            strip_audio: false,
            format: OutputFormat::default(),
        }],
        default_preset_id: "discord-10mb".into(),
        launch_at_login: false,
        shortcut_compress_latest: default_shortcut_compress_latest(),
        shortcut_toggle_panel: default_shortcut_toggle_panel(),
        stale_warn_minutes: default_stale_warn_minutes(),
    }
}

pub fn load(app: &AppHandle) -> Settings {
    let store = match app.store(STORE_FILE) {
        Ok(store) => store,
        Err(e) => {
            crate::log_error!("failed to open settings store, using defaults: {e}");
            return default_settings(app);
        }
    };
    match store.get(STORE_KEY) {
        Some(value) => match serde_json::from_value::<Settings>(value) {
            Ok(mut settings) => {
                // Field-level serde defaults can leave presets empty for old
                // stores; re-seed so the app always has a usable preset.
                if settings.presets.is_empty() {
                    crate::log_info!("stored settings carry no presets; re-seeding the defaults");
                    let defaults = default_settings(app);
                    settings.presets = defaults.presets;
                    settings.default_preset_id = defaults.default_preset_id;
                }
                crate::log_info!(
                    "settings loaded: {} watched folder(s), {} preset(s), default preset \"{}\"",
                    settings.watched_folders.len(),
                    settings.presets.len(),
                    settings.default_preset_id
                );
                settings
            }
            Err(e) => {
                crate::log_error!("stored settings are unreadable, falling back to defaults: {e}");
                backup_store_file(app);
                default_settings(app)
            }
        },
        None => {
            crate::log_info!("no stored settings; using defaults");
            default_settings(app)
        }
    }
}

/// Best-effort backup of an unreadable settings file so the next save does
/// not destroy the user's old data.
fn backup_store_file(app: &AppHandle) {
    let path = match tauri_plugin_store::resolve_store_path(app, STORE_FILE) {
        Ok(path) => path,
        Err(e) => {
            crate::log_warn!("cannot resolve settings store path for backup: {e}");
            return;
        }
    };
    let backup = path.with_extension("json.bak");
    if let Err(e) = std::fs::copy(&path, &backup) {
        crate::log_warn!(
            "failed to back up unreadable settings to {}: {e}",
            backup.display()
        );
    }
}

pub fn save(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let store = app
        .store(STORE_FILE)
        .map_err(|e| format!("failed to open settings store: {e}"))?;
    let value =
        serde_json::to_value(settings).map_err(|e| format!("failed to serialize settings: {e}"))?;
    store.set(STORE_KEY, value);
    store
        .save()
        .map_err(|e| format!("failed to persist settings: {e}"))?;
    crate::log_info!(
        "settings saved: {} watched folder(s), {} preset(s), default preset \"{}\"",
        settings.watched_folders.len(),
        settings.presets.len(),
        settings.default_preset_id
    );
    Ok(())
}

pub fn validate(settings: &Settings) -> Result<(), String> {
    if settings.presets.is_empty() {
        return Err("at least one preset is required".into());
    }
    if !settings
        .presets
        .iter()
        .any(|p| p.id == settings.default_preset_id)
    {
        return Err(format!(
            "default preset \"{}\" does not exist",
            settings.default_preset_id
        ));
    }
    for preset in &settings.presets {
        if preset.name.trim().is_empty() {
            return Err("preset names cannot be empty".into());
        }
        // also rejects NaN, which would otherwise pass a `<= 0.0` check
        if !preset.target_mb.is_finite() || preset.target_mb <= 0.0 {
            return Err(format!(
                "preset \"{}\" must have a target size greater than 0",
                preset.name
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // The exact JSON shape a pre-round-3 build persisted (no `format` on
    // presets, no shortcut/staleness fields). It must keep deserializing.
    const LEGACY_SETTINGS_JSON: &str = r#"{
        "watchedFolders": ["/Users/me/Desktop"],
        "copyToClipboard": true,
        "trashOriginal": false,
        "useHardwareEncoder": true,
        "presets": [{
            "id": "discord-10mb",
            "name": "Discord (10MB)",
            "targetMb": 10.0,
            "maxFps": null,
            "maxWidth": null,
            "scalePercent": null,
            "stripAudio": false
        }],
        "defaultPresetId": "discord-10mb",
        "launchAtLogin": false
    }"#;

    #[test]
    fn old_stored_settings_without_new_fields_still_load() {
        let settings: Settings = serde_json::from_str(LEGACY_SETTINGS_JSON).unwrap();
        assert_eq!(settings.presets[0].format, OutputFormat::Mp4);
        assert_eq!(
            settings.shortcut_compress_latest.as_deref(),
            Some("CmdOrCtrl+Alt+T")
        );
        assert_eq!(
            settings.shortcut_toggle_panel.as_deref(),
            Some("CmdOrCtrl+Alt+O")
        );
        assert_eq!(settings.stale_warn_minutes, 10);
    }

    #[test]
    fn output_format_serializes_lowercase() {
        assert_eq!(
            serde_json::to_value(OutputFormat::Mp4).unwrap(),
            serde_json::json!("mp4")
        );
        assert_eq!(
            serde_json::to_value(OutputFormat::Webm).unwrap(),
            serde_json::json!("webm")
        );
        assert_eq!(
            serde_json::to_value(OutputFormat::Gif).unwrap(),
            serde_json::json!("gif")
        );
        assert_eq!(OutputFormat::default(), OutputFormat::Mp4);
    }

    #[test]
    fn preset_format_round_trips() {
        let mut settings: Settings = serde_json::from_str(LEGACY_SETTINGS_JSON).unwrap();
        settings.presets[0].format = OutputFormat::Webm;
        let json = serde_json::to_value(&settings).unwrap();
        assert_eq!(json["presets"][0]["format"], serde_json::json!("webm"));
        let back: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(back.presets[0].format, OutputFormat::Webm);
    }

    #[test]
    fn explicitly_disabled_shortcuts_stay_disabled() {
        // `null` stored on purpose (user cleared the field) must not be
        // resurrected by the field-level default.
        let json = r#"{ "shortcutCompressLatest": null, "shortcutTogglePanel": "", "staleWarnMinutes": 3, "presets": [], "defaultPresetId": "", "watchedFolders": [] }"#;
        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.shortcut_compress_latest, None);
        assert_eq!(settings.shortcut_toggle_panel.as_deref(), Some(""));
        assert_eq!(settings.stale_warn_minutes, 3);
    }
}
