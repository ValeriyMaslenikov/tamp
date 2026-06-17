use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::AppHandle;
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

/// How a video gets split into independently compressed parts. Off keeps the
/// historical single-output behavior; presets stored before the field existed
/// deserialize as Off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SplitMode {
    #[default]
    Off,
    Smart,
    Static,
}

/// What a static split is measured in: a fixed number of parts, or a target
/// part length in seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StaticSplitBy {
    #[default]
    Parts,
    Seconds,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitConfig {
    #[serde(default)]
    pub mode: SplitMode,
    #[serde(default)]
    pub by: StaticSplitBy,
    #[serde(default = "default_split_parts")]
    pub parts: u32,
    #[serde(default = "default_split_seconds")]
    pub seconds: u32,
}

fn default_split_parts() -> u32 {
    2
}

fn default_split_seconds() -> u32 {
    30
}

impl Default for SplitConfig {
    fn default() -> Self {
        SplitConfig {
            mode: SplitMode::Off,
            by: StaticSplitBy::Parts,
            parts: default_split_parts(),
            seconds: default_split_seconds(),
        }
    }
}

/// Validates a split config; shared by preset validation and the one-off
/// custom-convert path (`commands.rs`). Only the dimension a static split
/// actually uses is checked — the other field is inert until selected, and
/// the frontend form keeps it in range before it can become active.
pub fn validate_split(split: &SplitConfig) -> Result<(), String> {
    if split.mode != SplitMode::Static {
        return Ok(());
    }
    match split.by {
        StaticSplitBy::Parts if !(2..=20).contains(&split.parts) => {
            Err("split part count must be between 2 and 20".into())
        }
        StaticSplitBy::Seconds if split.seconds < 10 => {
            Err("split part duration must be at least 10 seconds".into())
        }
        _ => Ok(()),
    }
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
    #[serde(default)]
    pub split: SplitConfig,
}

/// When to reveal finished outputs in the system file manager after a
/// conversion completes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpenAfterConvert {
    /// Never open anything (the default).
    #[default]
    Off,
    /// Open the part folder when a multi-part split finishes; do nothing for
    /// single outputs.
    Multipart,
    /// Open the part folder for splits, and reveal the file for single
    /// outputs.
    All,
}

/// Which color theme the panel renders in. `System` follows the OS
/// light/dark setting (and tracks live changes); `Light`/`Dark` pin it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// How the Videos screen offers presets when compressing a recording.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VideosLayout {
    /// Clicking a video opens a preset picker (default highlighted) — best
    /// when you choose a preset per clip.
    #[default]
    QuickPick,
    /// A persistent bar holds one active preset; clicking a video applies it
    /// immediately — best when you mostly use one preset.
    ActiveBar,
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
    /// Whether to open finished outputs in the file manager when a conversion
    /// completes (off / multi-part splits only / all conversions).
    #[serde(default)]
    pub open_after_convert: OpenAfterConvert,
    /// Which preset-selection UI the Videos screen shows.
    #[serde(default)]
    pub videos_layout: VideosLayout,
    /// Color theme for the panel (system / light / dark).
    #[serde(default)]
    pub theme: Theme,
    /// Windows: whether the Explorer "Compress with tamp" right-click entry is
    /// registered. Ignored on other platforms.
    #[serde(default = "default_true")]
    pub context_menu_enabled: bool,
    /// How many recent videos the Videos tab lists. 1..=200.
    #[serde(default = "default_recents_limit")]
    pub recents_limit: usize,
    /// Whether the one-time first-run notice (tray-location hint + how to
    /// reopen) has been shown and dismissed. Defaults to `false` so a fresh
    /// profile sees it once; the frontend flips it to `true` on dismiss.
    #[serde(default)]
    pub onboarding_seen: bool,
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

fn default_recents_limit() -> usize {
    50
}

pub struct SettingsState(pub Mutex<Settings>);

/// Default settings. Needs the app handle because the default watched
/// folders come from the platform strategy (wherever this OS's screen
/// recorders save) via the Tauri path resolver.
pub fn default_settings(app: &AppHandle) -> Settings {
    use crate::platform::Platform as _;
    let watched_folders = crate::platform::native()
        .default_watched_folders(app)
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
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
            split: SplitConfig::default(),
        }],
        default_preset_id: "discord-10mb".into(),
        launch_at_login: false,
        shortcut_compress_latest: default_shortcut_compress_latest(),
        shortcut_toggle_panel: default_shortcut_toggle_panel(),
        stale_warn_minutes: default_stale_warn_minutes(),
        open_after_convert: OpenAfterConvert::default(),
        videos_layout: VideosLayout::default(),
        theme: Theme::default(),
        context_menu_enabled: true,
        recents_limit: 50,
        onboarding_seen: false,
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
        if let Err(e) = validate_split(&preset.split) {
            return Err(format!("preset \"{}\": {e}", preset.name));
        }
    }
    if !(1..=200).contains(&settings.recents_limit) {
        return Err("recent videos shown must be between 1 and 200".into());
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
        assert_eq!(settings.presets[0].split, SplitConfig::default());
        assert_eq!(settings.presets[0].split.mode, SplitMode::Off);
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
    fn theme_serializes_lowercase_and_defaults_to_system() {
        assert_eq!(Theme::default(), Theme::System);
        assert_eq!(
            serde_json::to_value(Theme::System).unwrap(),
            serde_json::json!("system")
        );
        assert_eq!(
            serde_json::to_value(Theme::Light).unwrap(),
            serde_json::json!("light")
        );
        assert_eq!(
            serde_json::to_value(Theme::Dark).unwrap(),
            serde_json::json!("dark")
        );
        // Stores written before the field existed default to System.
        let settings: Settings = serde_json::from_str(LEGACY_SETTINGS_JSON).unwrap();
        assert_eq!(settings.theme, Theme::System);
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
    fn split_defaults_match_contract() {
        let split = SplitConfig::default();
        assert_eq!(split.mode, SplitMode::Off);
        assert_eq!(split.by, StaticSplitBy::Parts);
        assert_eq!(split.parts, 2);
        assert_eq!(split.seconds, 30);
    }

    #[test]
    fn split_enums_serialize_lowercase() {
        assert_eq!(
            serde_json::to_value(SplitMode::Off).unwrap(),
            serde_json::json!("off")
        );
        assert_eq!(
            serde_json::to_value(SplitMode::Smart).unwrap(),
            serde_json::json!("smart")
        );
        assert_eq!(
            serde_json::to_value(SplitMode::Static).unwrap(),
            serde_json::json!("static")
        );
        assert_eq!(
            serde_json::to_value(StaticSplitBy::Parts).unwrap(),
            serde_json::json!("parts")
        );
        assert_eq!(
            serde_json::to_value(StaticSplitBy::Seconds).unwrap(),
            serde_json::json!("seconds")
        );
    }

    #[test]
    fn partial_split_json_fills_field_defaults() {
        // A store written by a build that only knew `mode` (or a hand-edited
        // file) must still load, with the missing fields defaulted.
        let split: SplitConfig = serde_json::from_str(r#"{ "mode": "smart" }"#).unwrap();
        assert_eq!(split.mode, SplitMode::Smart);
        assert_eq!(split.by, StaticSplitBy::Parts);
        assert_eq!(split.parts, 2);
        assert_eq!(split.seconds, 30);

        let split: SplitConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(split, SplitConfig::default());
    }

    #[test]
    fn preset_split_round_trips_camel_case() {
        let mut settings: Settings = serde_json::from_str(LEGACY_SETTINGS_JSON).unwrap();
        settings.presets[0].split = SplitConfig {
            mode: SplitMode::Static,
            by: StaticSplitBy::Seconds,
            parts: 4,
            seconds: 45,
        };
        let json = serde_json::to_value(&settings).unwrap();
        assert_eq!(
            json["presets"][0]["split"],
            serde_json::json!({
                "mode": "static",
                "by": "seconds",
                "parts": 4,
                "seconds": 45
            })
        );
        let back: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(back.presets[0].split, settings.presets[0].split);
    }

    fn settings_with_split(split: SplitConfig) -> Settings {
        let mut settings: Settings = serde_json::from_str(LEGACY_SETTINGS_JSON).unwrap();
        settings.presets[0].split = split;
        settings
    }

    #[test]
    fn validate_accepts_off_and_smart_regardless_of_numbers() {
        for mode in [SplitMode::Off, SplitMode::Smart] {
            let split = SplitConfig {
                mode,
                by: StaticSplitBy::Parts,
                parts: 0,
                seconds: 0,
            };
            assert!(validate(&settings_with_split(split)).is_ok(), "{mode:?}");
        }
    }

    #[test]
    fn validate_bounds_static_parts() {
        let by_parts = |parts| SplitConfig {
            mode: SplitMode::Static,
            by: StaticSplitBy::Parts,
            parts,
            seconds: 30,
        };
        for parts in [2, 7, 20] {
            assert!(validate(&settings_with_split(by_parts(parts))).is_ok());
        }
        for parts in [0, 1, 21] {
            let err = validate(&settings_with_split(by_parts(parts))).unwrap_err();
            assert!(err.contains("between 2 and 20"), "{err}");
        }
    }

    #[test]
    fn validate_bounds_static_seconds() {
        let by_seconds = |seconds| SplitConfig {
            mode: SplitMode::Static,
            by: StaticSplitBy::Seconds,
            parts: 2,
            seconds,
        };
        for seconds in [10, 30, 3600] {
            assert!(validate(&settings_with_split(by_seconds(seconds))).is_ok());
        }
        for seconds in [0, 9] {
            let err = validate(&settings_with_split(by_seconds(seconds))).unwrap_err();
            assert!(err.contains("at least 10 seconds"), "{err}");
        }
    }

    #[test]
    fn validate_static_only_checks_the_active_dimension() {
        // Splitting by parts: a stale seconds value is inert, and vice versa.
        let split = SplitConfig {
            mode: SplitMode::Static,
            by: StaticSplitBy::Parts,
            parts: 3,
            seconds: 1,
        };
        assert!(validate(&settings_with_split(split)).is_ok());
        let split = SplitConfig {
            mode: SplitMode::Static,
            by: StaticSplitBy::Seconds,
            parts: 99,
            seconds: 15,
        };
        assert!(validate(&settings_with_split(split)).is_ok());
    }

    #[test]
    fn recents_limit_defaults_to_50_and_is_bounded() {
        let s: crate::settings::Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.recents_limit, 50);
    }

    #[test]
    fn onboarding_seen_defaults_to_false_for_old_stores() {
        // A store written before the field existed must read as "not yet
        // seen" so the one-time notice still appears once.
        let settings: Settings = serde_json::from_str(LEGACY_SETTINGS_JSON).unwrap();
        assert!(!settings.onboarding_seen);
        let empty: Settings = serde_json::from_str("{}").unwrap();
        assert!(!empty.onboarding_seen);
    }

    #[test]
    fn validate_bounds_recents_limit() {
        let mut settings: Settings = serde_json::from_str(LEGACY_SETTINGS_JSON).unwrap();
        settings.recents_limit = 0;
        let err = validate(&settings).unwrap_err();
        assert!(err.contains("recent videos"), "{err}");
        settings.recents_limit = 201;
        let err = validate(&settings).unwrap_err();
        assert!(err.contains("recent videos"), "{err}");
        for limit in [1, 50, 200] {
            settings.recents_limit = limit;
            assert!(validate(&settings).is_ok(), "{limit}");
        }
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
