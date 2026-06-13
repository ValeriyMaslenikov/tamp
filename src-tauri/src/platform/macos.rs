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
        if let Err(e) = panel.move_window_constrained(Position::TrayBottomCenter) {
            crate::log_warn!("failed to position panel under tray icon: {e}");
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
