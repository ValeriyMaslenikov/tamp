use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardWriting, NSWindow, NSWindowCollectionBehavior};
use objc2_foundation::{NSArray, NSString, NSURL};

use tauri_plugin_positioner::{Position, WindowExt as _};

use super::{HwCandidate, Platform, TrayProgress};

pub struct MacOs;

impl Platform for MacOs {
    fn configure_app(&self, app: &mut tauri::App) {
        // No Dock icon — tamp lives entirely in the menu bar.
        app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    }

    fn copy_files_to_clipboard(
        &self,
        app: &tauri::AppHandle,
        paths: &[PathBuf],
    ) -> Result<(), String> {
        copy_files_to_clipboard(app, paths)
    }

    fn configure_panel(&self, window: &tauri::WebviewWindow) -> Result<(), String> {
        // Tray panels must follow the user across Spaces/displays; without
        // this the panel opens on the Space the app launched on.
        window
            .set_visible_on_all_workspaces(true)
            .map_err(|e| format!("failed to set panel visible on all workspaces: {e}"))?;
        configure_panel(window)
    }

    fn position_panel_at_tray(&self, panel: &tauri::WebviewWindow) {
        // Menu bar is at the top, so the panel hangs below the icon.
        if panel
            .move_window_constrained(Position::TrayBottomCenter)
            .is_ok()
        {
            return;
        }
        // No cached tray rect to anchor against: fall back to the work
        // area's top corner so the panel is never positioned off-screen.
        crate::log_warn!("no tray rect to anchor the panel; using the work-area corner");
        if let Ok(Some(monitor)) = panel.primary_monitor() {
            self.position_panel_fallback(panel, &monitor);
        }
    }

    fn position_panel_fallback(&self, panel: &tauri::WebviewWindow, monitor: &tauri::Monitor) {
        if let Some(pos) = super::work_area_corner(panel, monitor, super::VAnchor::Top) {
            let _ = panel.set_position(pos);
        }
    }

    fn default_watched_folders(&self, app: &tauri::AppHandle) -> Vec<PathBuf> {
        // ⌘⇧5 saves to the Desktop by default.
        match tauri::Manager::path(app).desktop_dir() {
            Ok(desktop) => vec![desktop],
            Err(e) => {
                crate::log_warn!("cannot resolve desktop dir for default watched folder: {e}");
                Vec::new()
            }
        }
    }

    fn prepare_background_command(&self, _cmd: &mut tokio::process::Command) {}

    fn tray_progress(&self, app: &tauri::AppHandle, progress: Option<TrayProgress>) {
        // Tray title text next to the icon is a macOS-only capability.
        let text = progress.map(|p| {
            let pct = p.percent();
            if p.queued > 0 {
                format!("{pct}% (+{})", p.queued)
            } else {
                format!("{pct}%")
            }
        });
        crate::tray::set_title(app, text);
    }

    fn hw_candidates(&self) -> &'static [HwCandidate] {
        // `-allow_sw 1` lets VideoToolbox use Apple's software encoder when
        // no hardware session is available.
        &[HwCandidate {
            name: "h264_videotoolbox",
            extra_args: &["-allow_sw", "1"],
        }]
    }

    fn primary_mouse_button_down(&self) -> bool {
        // Bit 0 of the pressed-button mask is the left button. The pinned
        // objc2-app-kit marks this call safe, so no `unsafe` block is needed
        // (an unnecessary one is a hard error under `-D warnings`).
        (objc2_app_kit::NSEvent::pressedMouseButtons() & 1) != 0
    }

    fn resolve_accelerator(&self, accelerator: &str) -> String {
        // macOS hotkeys are positional; rewrite the key token to the position
        // that types the configured character in the active layout.
        super::rewrite_accelerator_key(accelerator, super::macos_keyboard::layout_token)
    }

    fn cleanup_legacy_autostart(&self) {
        remove_legacy_launch_agent();
    }
}

/// Best-effort removal of a pre-rebrand LaunchAgent plist named by the legacy
/// bundle identifier. The autostart plugin names its plist by the product name
/// "tamp" (`~/Library/LaunchAgents/tamp.plist`), which is rebrand-stable and so
/// gets overwritten on re-enable — but an earlier build that registered the
/// agent under the old bundle id leaves `com.joystudios.tamp.plist` orphaned,
/// pointing at a binary that no longer exists. Mirrors the data-side
/// `migrate_legacy_data` cleanup: keyed off `LEGACY_IDENTIFIER`, runs once at
/// startup before the current agent is re-enabled, and swallows every error.
fn remove_legacy_launch_agent() {
    // ~/Library/LaunchAgents is where per-user agents live; $HOME is always set
    // for a logged-in macOS session (the autostart plugin resolves it the same
    // way via dirs::home_dir).
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let plist = PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{}.plist", crate::LEGACY_IDENTIFIER));
    if !plist.exists() {
        return;
    }
    match std::fs::remove_file(&plist) {
        Ok(()) => crate::log_info!("removed legacy LaunchAgent {}", plist.display()),
        Err(e) => crate::log_warn!(
            "failed to remove legacy LaunchAgent {}: {e}",
            plist.display()
        ),
    }
}

/// Lets the panel join the Space of a full-screen app; without
/// `FullScreenAuxiliary` macOS refuses to show it over full-screen windows.
/// Must run on the main thread (Tauri's setup hook does).
fn configure_panel(window: &tauri::WebviewWindow) -> Result<(), String> {
    let ns_window = window
        .ns_window()
        .map_err(|e| format!("failed to get NSWindow handle: {e}"))?
        as *mut NSWindow;
    // SAFETY: ns_window() returns a live NSWindow owned by this window, and
    // we only touch it from the main thread.
    let ns_window = unsafe { &*ns_window };
    ns_window.setCollectionBehavior(
        ns_window.collectionBehavior() | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    Ok(())
}

/// Writes ALL `paths` as file URLs onto the general pasteboard in a single
/// `writeObjects` call — one clearContents, one NSArray — so pasting into
/// Finder/Discord drops the whole set at once.
fn copy_files_to_clipboard(app: &tauri::AppHandle, paths: &[PathBuf]) -> Result<(), String> {
    let path_strs = super::paths_to_utf8(paths)?;

    // NSPasteboard must be used from the main thread; ship the result back
    // over a channel since run_on_main_thread takes a fire-and-forget closure.
    let (tx, rx) = mpsc::channel::<bool>();
    app.run_on_main_thread(move || {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        let objects: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = path_strs
            .iter()
            .map(|path_str| {
                let url = NSURL::fileURLWithPath(&NSString::from_str(path_str));
                ProtocolObject::from_retained(url)
            })
            .collect();
        let objects = NSArray::from_retained_slice(&objects);
        let ok = pasteboard.writeObjects(&objects);
        let _ = tx.send(ok);
    })
    .map_err(|e| format!("failed to dispatch to main thread: {e}"))?;

    match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(true) => Ok(()),
        Ok(false) => Err("NSPasteboard writeObjects returned false".to_string()),
        Err(_) => Err("timed out waiting for main thread clipboard write".to_string()),
    }
}
