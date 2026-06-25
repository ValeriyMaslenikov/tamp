//! Integration coverage for the settings validate flow through
//! `tamp_lib::settings`' public API: construct a `Settings` by round-tripping
//! it through serde (the exact path `settings::load`/`save` use to persist the
//! camelCase store), then assert `validate` accepts a valid one and rejects the
//! new invariants — bad locale, duplicate preset names, out-of-range recents.
//!
//! The per-fn unit tests in `settings.rs` cover these branches in isolation;
//! this file proves them over a fully round-tripped `Settings` value (the shape
//! that actually reaches the store), exercising the public surface a caller has.

use tamp_lib::settings::{validate, Preset, Settings, SplitConfig};

/// A complete, valid settings JSON in the camelCase store shape. One preset,
/// a matching default id, in-range recents, a supported locale.
const VALID_SETTINGS_JSON: &str = r#"{
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
    "launchAtLogin": false,
    "recentsLimit": 50,
    "locale": "en"
}"#;

/// Deserialize the valid settings, then serialize+deserialize once more so the
/// value under test has truly survived a full round-trip through the store's
/// camelCase shape (the path `save` then `load` take).
fn round_tripped_settings() -> Settings {
    let parsed: Settings = serde_json::from_str(VALID_SETTINGS_JSON).unwrap();
    let json = serde_json::to_value(&parsed).unwrap();
    serde_json::from_value(json).unwrap()
}

#[test]
fn validate_accepts_a_round_tripped_valid_settings() {
    let settings = round_tripped_settings();
    assert!(
        validate(&settings).is_ok(),
        "a fully round-tripped valid settings must validate"
    );
}

#[test]
fn validate_rejects_an_unsupported_locale() {
    let mut settings = round_tripped_settings();
    settings.locale = "fr".into();
    let err = validate(&settings).unwrap_err();
    assert!(err.contains("unsupported locale"), "{err}");

    // The supported set ("system" / "en" / "uk") all validate, even after a
    // round-trip; an exact-case match is required ("EN" is rejected).
    for ok in ["system", "en", "uk"] {
        settings.locale = ok.into();
        assert!(validate(&settings).is_ok(), "{ok}");
    }
    settings.locale = "EN".into();
    assert!(validate(&settings)
        .unwrap_err()
        .contains("unsupported locale"));
}

#[test]
fn validate_rejects_duplicate_preset_names_case_insensitively() {
    let mut settings = round_tripped_settings();
    let mut dup: Preset = settings.presets[0].clone();
    dup.id = "discord-10mb-dup".into();
    // Same name, different case + surrounding whitespace — still a duplicate.
    dup.name = format!("  {}  ", settings.presets[0].name.to_uppercase());
    dup.split = SplitConfig::default();
    settings.presets.push(dup);

    let err = validate(&settings).unwrap_err();
    assert!(err.contains("unique name"), "{err}");
}

#[test]
fn validate_accepts_distinct_preset_names() {
    let mut settings = round_tripped_settings();
    let mut other: Preset = settings.presets[0].clone();
    other.id = "other-id".into();
    other.name = "A genuinely different preset".into();
    settings.presets.push(other);
    assert!(validate(&settings).is_ok());
}

#[test]
fn validate_bounds_recents_limit_to_1_through_200() {
    let mut settings = round_tripped_settings();

    for bad in [0usize, 201, 1000] {
        settings.recents_limit = bad;
        let err = validate(&settings).unwrap_err();
        assert!(err.contains("recent videos"), "limit {bad}: {err}");
    }
    for ok in [1usize, 50, 200] {
        settings.recents_limit = ok;
        assert!(validate(&settings).is_ok(), "limit {ok} must be accepted");
    }
}

#[test]
fn validate_round_trips_after_a_rejection_is_corrected() {
    // A flow check: an invalid value is rejected, the value is corrected, the
    // settings serialize back to the store shape and validate clean — the
    // save-then-reload contract.
    let mut settings = round_tripped_settings();
    settings.recents_limit = 500;
    settings.locale = "de".into();
    assert!(validate(&settings).is_err());

    settings.recents_limit = 25;
    settings.locale = "uk".into();
    assert!(validate(&settings).is_ok());

    // The corrected value survives a serialize/deserialize round-trip and is
    // still valid (camelCase keys are preserved).
    let json = serde_json::to_value(&settings).unwrap();
    assert_eq!(json["recentsLimit"], serde_json::json!(25));
    assert_eq!(json["locale"], serde_json::json!("uk"));
    let back: Settings = serde_json::from_value(json).unwrap();
    assert!(validate(&back).is_ok());
    assert_eq!(back.recents_limit, 25);
    assert_eq!(back.locale, "uk");
}
