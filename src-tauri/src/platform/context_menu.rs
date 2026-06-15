//! Registers (and removes) the per-user "Compress with tamp" Explorer entry by
//! writing HKCU registry keys for each video extension. Per-user (HKCU) needs
//! no admin. On Windows 11 the entry appears under "Show more options".

/// The six video extensions the menu entry is registered for (leading dot).
const EXTS: [&str; 6] = [".mov", ".mp4", ".m4v", ".webm", ".mkv", ".avi"];

/// Registry subkey (under HKCU) carrying the verb for `ext` (e.g. ".mp4").
fn verb_key(ext: &str) -> String {
    format!("Software\\Classes\\SystemFileAssociations\\{ext}\\shell\\tamp.compress")
}

/// The `command` value: the exe invoked with the right-clicked file as `%1`.
fn command_value(exe: &str) -> String {
    format!("\"{exe}\" \"%1\"")
}

use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

/// Adds the "Compress with tamp" entry for every video extension, pointing at
/// `exe`. Overwrites any previous registration (idempotent).
pub fn register(exe: &str) -> std::io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for ext in EXTS {
        let (verb, _) = hkcu.create_subkey(verb_key(ext))?;
        verb.set_value("", &"Compress with tamp")?;
        verb.set_value("Icon", &format!("\"{exe}\""))?;
        let (command, _) = hkcu.create_subkey(format!("{}\\command", verb_key(ext)))?;
        command.set_value("", &command_value(exe))?;
    }
    Ok(())
}

/// Removes the entry for every video extension. Missing keys are not an error.
pub fn unregister() -> std::io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for ext in EXTS {
        match hkcu.delete_subkey_all(verb_key(ext)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// `register`/`unregister` keyed on `enabled`, using the running executable's
/// path. Best-effort logging wrapper for startup + the settings toggle.
pub fn apply(enabled: bool) -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("cannot resolve current exe: {e}"))?
        .to_string_lossy()
        .into_owned();
    let res = if enabled {
        register(&exe)
    } else {
        unregister()
    };
    res.map_err(|e| format!("context-menu registry update failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::{command_value, verb_key, EXTS};

    #[test]
    fn verb_key_is_per_extension_under_system_file_associations() {
        assert_eq!(
            verb_key(".mp4"),
            "Software\\Classes\\SystemFileAssociations\\.mp4\\shell\\tamp.compress"
        );
    }

    #[test]
    fn command_quotes_exe_and_passes_percent_one() {
        assert_eq!(
            command_value("C:\\Program Files\\tamp\\tamp.exe"),
            "\"C:\\Program Files\\tamp\\tamp.exe\" \"%1\""
        );
    }

    #[test]
    fn covers_the_six_video_extensions() {
        assert_eq!(EXTS.len(), 6);
        assert!(EXTS.contains(&".mp4") && EXTS.contains(&".mov"));
    }
}
