//! Global shortcuts: compress the newest recording / toggle the tray panel.
//!
//! Registration mirrors the settings: `apply` is called on startup and again
//! from `save_settings` whenever the accelerators change (with rollback to
//! the previous pair when the new ones fail to register).

use crate::platform::Platform as _;
use crate::settings::Settings;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_notification::{NotificationExt, PermissionState};

/// `compress-latest` scans this many recents to find a non-output video; way
/// past anything a folder of screen recordings realistically interleaves.
const SCAN_LIMIT: usize = 100;

/// The locale a native notification renders in. The global shortcut fires
/// without the panel, so the frontend's `t()` can't reach these strings — they
/// are localized here from the persisted `settings.locale`. Only the two
/// languages the app ships (English + Ukrainian) exist; everything resolves to
/// one of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotifLang {
    En,
    Uk,
}

/// Map the persisted `settings.locale` (`"system" | "en" | "uk"`) to a concrete
/// notification language, applying the SAME uk-prefix rule the frontend uses
/// (`src/i18n/index.ts::resolveLocale`): `"en"`/`"uk"` pass through; `"system"`
/// resolves to `Uk` only when the system language starts with `uk` (case-
/// insensitive, so `uk`/`uk-UA`/`UK`), otherwise `En`. Pure so it's unit-tested.
fn resolve_notif_lang(setting: &str, sys_lang: &str) -> NotifLang {
    match setting {
        "uk" => NotifLang::Uk,
        "en" => NotifLang::En,
        // "system" (and any unexpected value) follows the OS UI language.
        _ => {
            if sys_lang.to_lowercase().starts_with("uk") {
                NotifLang::Uk
            } else {
                NotifLang::En
            }
        }
    }
}

/// Best-effort read of the OS UI language for the `"system"` locale setting.
/// The webview's `navigator.language` is unreachable from Rust (the shortcut
/// fires without it), so we read the standard POSIX locale environment
/// variables (`LC_ALL`/`LC_MESSAGES`/`LANG`); when none is set (e.g. a default
/// Windows session) we return an empty string, which `resolve_notif_lang`
/// treats as English — the privacy-safe, minimal fallback. A user who wants
/// Ukrainian notifications sets the locale explicitly in Preferences.
fn system_language() -> String {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(val) = std::env::var(var) {
            if !val.is_empty() {
                return val;
            }
        }
    }
    String::new()
}

/// "No recent videos found" — localized.
fn msg_no_recent_videos(lang: NotifLang) -> &'static str {
    match lang {
        NotifLang::En => "No recent videos found",
        NotifLang::Uk => "Нещодавніх відео не знайдено",
    }
}

/// The stale-recording warning: the latest video is older than the configured
/// limit, so it probably isn't the clip the user means to compress. Localized.
fn msg_stale_warning(lang: NotifLang, name: &str, age_minutes: u64) -> String {
    match lang {
        NotifLang::En => format!(
            "Compressing {name}, recorded {age_minutes}m ago — open Tamp to cancel if that wasn't intended"
        ),
        NotifLang::Uk => format!(
            "Стискаємо {name}, записано {age_minutes} хв тому — відкрийте Tamp, щоб скасувати, якщо це не те, що ви хотіли"
        ),
    }
}

/// "Couldn't compress {name}: {e}" — localized (the raw error stays English;
/// it's a developer-facing fragment, like the frontend's friendlyError input).
fn msg_compress_failed(lang: NotifLang, name: &str, err: &str) -> String {
    match lang {
        NotifLang::En => format!("Couldn't compress {name}: {err}"),
        NotifLang::Uk => format!("Не вдалося стиснути {name}: {err}"),
    }
}

/// An accelerator that is `None` or blank means "shortcut disabled".
fn enabled(accel: &Option<String>) -> Option<&str> {
    accel.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// (Re-)registers both global shortcuts to match `settings`, replacing
/// whatever was registered before. On failure the previous registrations are
/// gone — callers that need atomicity roll back by re-applying the old
/// settings (see `save_settings`).
pub fn apply(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let shortcuts = app.global_shortcut();
    shortcuts
        .unregister_all()
        .map_err(|e| format!("failed to clear global shortcuts: {e}"))?;

    if let Some(accel) = enabled(&settings.shortcut_compress_latest) {
        // Register against the key that TYPES the configured character in the
        // user's layout (a no-op outside macOS / on QWERTY).
        let registered = crate::platform::native().resolve_accelerator(accel);
        let handler_app = app.clone();
        shortcuts
            .on_shortcut(registered.as_str(), move |_app, _shortcut, event| {
                if event.state() == ShortcutState::Pressed {
                    compress_latest(&handler_app);
                }
            })
            .map_err(|e| format!("Invalid \"compress latest\" shortcut \"{accel}\": {e}"))?;
        crate::log_info!(
            "registered compress-latest global shortcut \"{accel}\" (as \"{registered}\")"
        );
    } else {
        crate::log_info!("compress-latest global shortcut is disabled");
    }
    if let Some(accel) = enabled(&settings.shortcut_toggle_panel) {
        let registered = crate::platform::native().resolve_accelerator(accel);
        let handler_app = app.clone();
        shortcuts
            .on_shortcut(registered.as_str(), move |_app, _shortcut, event| {
                if event.state() == ShortcutState::Pressed {
                    crate::toggle_panel_fallback(&handler_app);
                }
            })
            .map_err(|e| format!("Invalid \"toggle panel\" shortcut \"{accel}\": {e}"))?;
        crate::log_info!(
            "registered toggle-panel global shortcut \"{accel}\" (as \"{registered}\")"
        );
    } else {
        crate::log_info!("toggle-panel global shortcut is disabled");
    }
    Ok(())
}

/// True when re-applying registrations is needed at all: only the two
/// accelerator fields affect them.
pub fn changed(old: &Settings, new: &Settings) -> bool {
    old.shortcut_compress_latest != new.shortcut_compress_latest
        || old.shortcut_toggle_panel != new.shortcut_toggle_panel
}

/// The current notification permission as a lowercase string the frontend can
/// branch on: "granted" | "denied" | "prompt" | "prompt-with-rationale", or
/// "unsupported" when the permission can't be read (no API / plugin error).
///
/// On desktop this plugin reports `Granted` unconditionally (the OS, not tamp,
/// owns the real notification toggle), so the Preferences "notifications are
/// off" row simply never appears there — the right graceful degradation. The
/// command still reports a real denied/prompt state on platforms that surface
/// one, so the recovery UI lights up wherever it's meaningful.
pub fn permission_state_str(app: &AppHandle) -> &'static str {
    match app.notification().permission_state() {
        Ok(PermissionState::Granted) => "granted",
        Ok(PermissionState::Denied) => "denied",
        Ok(PermissionState::Prompt) => "prompt",
        Ok(PermissionState::PromptWithRationale) => "prompt-with-rationale",
        Err(e) => {
            crate::log_warn!("cannot read notification permission: {e}");
            "unsupported"
        }
    }
}

/// Requests the notification permission and reports the resulting state as the
/// same lowercase string [`permission_state_str`] returns. Backs the
/// Preferences "Enable" affordance: a `prompt` state re-shows the OS prompt; a
/// hard `denied` is unchanged by this (the user must flip it in System
/// Settings) but the call is harmless and surfaces the still-denied state.
pub fn request_permission_str(app: &AppHandle) -> &'static str {
    match app.notification().request_permission() {
        Ok(PermissionState::Granted) => "granted",
        Ok(PermissionState::Denied) => "denied",
        Ok(PermissionState::Prompt) => "prompt",
        Ok(PermissionState::PromptWithRationale) => "prompt-with-rationale",
        Err(e) => {
            crate::log_warn!("notification permission request failed: {e}");
            "unsupported"
        }
    }
}

/// Sends a user notification, requesting permission lazily on first use.
/// Best-effort: failures only log (shortcuts must never crash the app).
pub fn notify(app: &AppHandle, body: String) {
    let notifications = app.notification();
    let granted = match notifications.permission_state() {
        Ok(PermissionState::Granted) => true,
        Ok(_) => matches!(
            notifications.request_permission(),
            Ok(PermissionState::Granted)
        ),
        Err(e) => {
            crate::log_warn!("cannot read notification permission: {e}");
            false
        }
    };
    if !granted {
        crate::log_warn!("notifications not permitted; wanted to say: {body}");
        return;
    }
    if let Err(e) = notifications.builder().title("Tamp").body(&body).show() {
        crate::log_warn!("failed to show notification: {e}");
    }
}

/// The compress-latest shortcut: find the newest non-output video in the
/// watched folders and enqueue it with the default preset (normal
/// post-actions). Warns via notification when that video looks stale.
fn compress_latest(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let (folders, stale_warn_minutes, locale): (Vec<PathBuf>, u32, String) = {
            use tauri::Manager;
            let state = app.state::<crate::settings::SettingsState>();
            let guard = state
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            (
                guard.watched_folders.iter().map(PathBuf::from).collect(),
                guard.stale_warn_minutes,
                guard.locale.clone(),
            )
        };
        // Native notifications fire without the panel, so localize them here
        // from the persisted locale (the same uk-prefix rule as the frontend).
        let lang = resolve_notif_lang(&locale, &system_language());
        let scanned = tauri::async_runtime::spawn_blocking(move || {
            crate::scanner::scan(&folders, SCAN_LIMIT)
        })
        .await
        .unwrap_or_else(|e| {
            crate::log_error!("compress-latest scan failed: {e}");
            Vec::new()
        });
        let Some(latest) = scanned.into_iter().find(|v| !v.is_output) else {
            notify(&app, msg_no_recent_videos(lang).to_string());
            return;
        };

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let age_ms = now_ms.saturating_sub(latest.created_ms);
        let age_minutes = age_ms / 60_000;
        if age_ms > u64::from(stale_warn_minutes) * 60_000 {
            notify(&app, msg_stale_warning(lang, &latest.name, age_minutes));
        }

        if let Err(e) = crate::commands::enqueue_default(&app, latest.path) {
            notify(&app, msg_compress_failed(lang, &latest.name, &e));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_locale_overrides_the_system_language() {
        // "en"/"uk" pass straight through regardless of the OS language.
        assert_eq!(resolve_notif_lang("en", "uk-UA"), NotifLang::En);
        assert_eq!(resolve_notif_lang("uk", "en-US"), NotifLang::Uk);
        assert_eq!(resolve_notif_lang("en", ""), NotifLang::En);
        assert_eq!(resolve_notif_lang("uk", ""), NotifLang::Uk);
    }

    #[test]
    fn system_follows_the_os_language_with_the_uk_prefix_rule() {
        // "system" mirrors the frontend resolveLocale: uk-prefixed → Uk, else En.
        for sys in ["uk", "uk-UA", "UK", "uk_UA.UTF-8"] {
            assert_eq!(resolve_notif_lang("system", sys), NotifLang::Uk, "{sys}");
        }
        for sys in ["", "en", "en-US", "fr", "ru", "ukulele-is-not-uk"] {
            // Only a genuine `uk` language prefix counts; everything else is En.
            let want = if sys.to_lowercase().starts_with("uk") {
                NotifLang::Uk
            } else {
                NotifLang::En
            };
            assert_eq!(resolve_notif_lang("system", sys), want, "{sys}");
        }
        // An unexpected stored value degrades to the system rule, not a panic.
        assert_eq!(resolve_notif_lang("fr", "uk-UA"), NotifLang::Uk);
        assert_eq!(resolve_notif_lang("fr", "en-US"), NotifLang::En);
    }

    #[test]
    fn notification_strings_are_localized_per_language() {
        // No-recent-videos: distinct copy per language.
        assert_eq!(
            msg_no_recent_videos(NotifLang::En),
            "No recent videos found"
        );
        assert_eq!(
            msg_no_recent_videos(NotifLang::Uk),
            "Нещодавніх відео не знайдено"
        );

        // Stale warning: interpolates name + age, localized framing.
        let en = msg_stale_warning(NotifLang::En, "clip.mov", 42);
        assert!(en.contains("Compressing clip.mov"), "{en}");
        assert!(en.contains("42m ago"), "{en}");
        let uk = msg_stale_warning(NotifLang::Uk, "clip.mov", 42);
        assert!(uk.contains("Стискаємо clip.mov"), "{uk}");
        assert!(uk.contains("42 хв тому"), "{uk}");

        // Compress-failed: interpolates name + the (English) raw error fragment.
        let en = msg_compress_failed(NotifLang::En, "clip.mov", "disk full");
        assert!(en.starts_with("Couldn't compress clip.mov:"), "{en}");
        assert!(en.contains("disk full"), "{en}");
        let uk = msg_compress_failed(NotifLang::Uk, "clip.mov", "disk full");
        assert!(uk.starts_with("Не вдалося стиснути clip.mov:"), "{uk}");
        assert!(uk.contains("disk full"), "{uk}");
    }
}
