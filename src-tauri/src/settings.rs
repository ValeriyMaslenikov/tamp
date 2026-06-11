use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "settings.json";
const STORE_KEY: &str = "settings";

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub watched_folders: Vec<String>,
    pub copy_to_clipboard: bool,
    pub trash_original: bool,
    pub presets: Vec<Preset>,
    pub default_preset_id: String,
    pub launch_at_login: bool,
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
            eprintln!("tamp: cannot resolve home dir for default watched folder: {e}");
            Vec::new()
        });
    Settings {
        watched_folders,
        copy_to_clipboard: true,
        trash_original: false,
        presets: vec![Preset {
            id: "discord-10mb".into(),
            name: "Discord (10MB)".into(),
            target_mb: 10.0,
            max_fps: None,
            max_width: None,
            scale_percent: None,
            strip_audio: false,
        }],
        default_preset_id: "discord-10mb".into(),
        launch_at_login: false,
    }
}

pub fn load(app: &AppHandle) -> Settings {
    let store = match app.store(STORE_FILE) {
        Ok(store) => store,
        Err(e) => {
            eprintln!("tamp: failed to open settings store, using defaults: {e}");
            return default_settings(app);
        }
    };
    match store.get(STORE_KEY) {
        Some(value) => match serde_json::from_value::<Settings>(value) {
            Ok(settings) => settings,
            Err(e) => {
                eprintln!("tamp: stored settings are unreadable, falling back to defaults: {e}");
                default_settings(app)
            }
        },
        None => default_settings(app),
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
        .map_err(|e| format!("failed to persist settings: {e}"))
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
