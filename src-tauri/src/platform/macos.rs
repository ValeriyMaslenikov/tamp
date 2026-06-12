use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardWriting, NSWindow, NSWindowCollectionBehavior};
use objc2_foundation::{NSArray, NSString, NSURL};

/// Lets the panel join the Space of a full-screen app; without
/// `FullScreenAuxiliary` macOS refuses to show it over full-screen windows.
/// Must run on the main thread (Tauri's setup hook does).
pub fn configure_panel(window: &tauri::WebviewWindow) -> Result<(), String> {
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
pub fn copy_files_to_clipboard(app: &tauri::AppHandle, paths: &[PathBuf]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("no files to copy".to_string());
    }
    let path_strs = paths
        .iter()
        .map(|p| {
            p.to_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("path is not valid UTF-8: {}", p.display()))
        })
        .collect::<Result<Vec<String>, String>>()?;

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
