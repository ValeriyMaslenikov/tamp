use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSString, NSURL};

pub fn copy_file_to_clipboard(app: &tauri::AppHandle, path: &Path) -> Result<(), String> {
    let path_str = path
        .to_str()
        .ok_or_else(|| "path is not valid UTF-8".to_string())?
        .to_string();

    // NSPasteboard must be used from the main thread; ship the result back
    // over a channel since run_on_main_thread takes a fire-and-forget closure.
    let (tx, rx) = mpsc::channel::<bool>();
    app.run_on_main_thread(move || {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path_str));
        let object: Retained<ProtocolObject<dyn NSPasteboardWriting>> =
            ProtocolObject::from_retained(url);
        let objects = NSArray::from_retained_slice(&[object]);
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
