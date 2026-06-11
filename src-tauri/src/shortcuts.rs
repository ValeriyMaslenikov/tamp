//! Global shortcuts: compress the newest recording / toggle the tray panel.
//!
//! Registration mirrors the settings: `apply` is called on startup and again
//! from `save_settings` whenever the accelerators change (with rollback to
//! the previous pair when the new ones fail to register).

use crate::settings::Settings;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_notification::{NotificationExt, PermissionState};

/// `compress-latest` scans this many recents to find a non-output video; way
/// past anything a folder of screen recordings realistically interleaves.
const SCAN_LIMIT: usize = 100;

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
        let handler_app = app.clone();
        shortcuts
            .on_shortcut(accel, move |_app, _shortcut, event| {
                if event.state() == ShortcutState::Pressed {
                    compress_latest(&handler_app);
                }
            })
            .map_err(|e| format!("Invalid \"compress latest\" shortcut \"{accel}\": {e}"))?;
        crate::log_info!("registered compress-latest global shortcut \"{accel}\"");
    } else {
        crate::log_info!("compress-latest global shortcut is disabled");
    }
    if let Some(accel) = enabled(&settings.shortcut_toggle_panel) {
        let handler_app = app.clone();
        shortcuts
            .on_shortcut(accel, move |_app, _shortcut, event| {
                if event.state() == ShortcutState::Pressed {
                    crate::toggle_panel_fallback(&handler_app);
                }
            })
            .map_err(|e| format!("Invalid \"toggle panel\" shortcut \"{accel}\": {e}"))?;
        crate::log_info!("registered toggle-panel global shortcut \"{accel}\"");
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
    if let Err(e) = notifications.builder().title("tamp").body(&body).show() {
        crate::log_warn!("failed to show notification: {e}");
    }
}

/// The compress-latest shortcut: find the newest non-output video in the
/// watched folders and enqueue it with the default preset (normal
/// post-actions). Warns via notification when that video looks stale.
fn compress_latest(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let (folders, stale_warn_minutes): (Vec<PathBuf>, u32) = {
            use tauri::Manager;
            let state = app.state::<crate::settings::SettingsState>();
            let guard = state
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            (
                guard.watched_folders.iter().map(PathBuf::from).collect(),
                guard.stale_warn_minutes,
            )
        };
        let scanned = tauri::async_runtime::spawn_blocking(move || {
            crate::scanner::scan(&folders, SCAN_LIMIT)
        })
        .await
        .unwrap_or_else(|e| {
            crate::log_error!("compress-latest scan failed: {e}");
            Vec::new()
        });
        let Some(latest) = scanned.into_iter().find(|v| !v.is_output) else {
            notify(&app, "No recent videos found".to_string());
            return;
        };

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let age_ms = now_ms.saturating_sub(latest.created_ms);
        let age_minutes = age_ms / 60_000;
        if age_ms > u64::from(stale_warn_minutes) * 60_000 {
            notify(
                &app,
                format!(
                    "Compressing {}, recorded {age_minutes}m ago — open tamp to cancel if that wasn't intended",
                    latest.name
                ),
            );
        }

        if let Err(e) = crate::commands::enqueue_default(&app, latest.path) {
            notify(&app, format!("Couldn't compress {}: {e}", latest.name));
        }
    });
}
