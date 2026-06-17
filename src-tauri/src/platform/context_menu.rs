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

/// Writes the verb + command keys for a single extension. Split out so
/// `register` can attempt one extension, then roll back on a later failure.
fn write_ext(hkcu: &RegKey, ext: &str, exe: &str) -> std::io::Result<()> {
    let (verb, _) = hkcu.create_subkey(verb_key(ext))?;
    verb.set_value("", &"Compress with tamp")?;
    verb.set_value("Icon", &format!("\"{exe}\""))?;
    let (command, _) = hkcu.create_subkey(format!("{}\\command", verb_key(ext)))?;
    command.set_value("", &command_value(exe))?;
    Ok(())
}

/// Best-effort delete of one extension's verb key tree. A missing key is not an
/// error (already absent is the desired end state); any other error is returned.
fn delete_ext(hkcu: &RegKey, ext: &str) -> std::io::Result<()> {
    match hkcu.delete_subkey_all(verb_key(ext)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// All-or-nothing registration orchestration, parameterized over the per-key
/// `write`/`delete` ops so the rollback path is unit-testable without touching
/// the real registry. Writes each extension in turn; on the first failure it
/// rolls back (deletes) the extensions already written this call and returns the
/// original error. A failed rollback delete is logged, not surfaced — the write
/// error is the actionable one.
fn register_with<W, D>(mut write: W, mut delete: D) -> std::io::Result<()>
where
    W: FnMut(&str) -> std::io::Result<()>,
    D: FnMut(&str) -> std::io::Result<()>,
{
    let mut written: Vec<&str> = Vec::with_capacity(EXTS.len());
    for ext in EXTS {
        if let Err(e) = write(ext) {
            for done in written {
                if let Err(re) = delete(done) {
                    crate::log_warn!("context-menu rollback failed for {done}: {re}");
                }
            }
            return Err(e);
        }
        written.push(ext);
    }
    Ok(())
}

/// Best-effort unregistration orchestration, parameterized over the per-key
/// `delete` op. Every extension is attempted (a failure on one does not abort
/// the rest) so no stale key is left behind; the first error, if any, is
/// returned after all six were attempted.
fn unregister_with<D>(mut delete: D) -> std::io::Result<()>
where
    D: FnMut(&str) -> std::io::Result<()>,
{
    let mut first_err: Option<std::io::Error> = None;
    for ext in EXTS {
        if let Err(e) = delete(ext) {
            if first_err.is_none() {
                first_err = Some(e);
            }
        }
    }
    match first_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Adds the "Compress with tamp" entry for every video extension, pointing at
/// `exe`. All-or-nothing: if any extension fails to register, the ones already
/// written this call are rolled back (deleted) and the original error is
/// returned, so the on-disk state matches the (reverted) UI toggle rather than
/// a half-registered set. Overwrites any previous registration (idempotent).
pub fn register(exe: &str) -> std::io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    register_with(
        |ext| write_ext(&hkcu, ext, exe),
        |ext| delete_ext(&hkcu, ext),
    )
}

/// Removes the entry for every video extension. Best-effort: every extension is
/// attempted so no stale key is left behind; missing keys are not an error. If
/// any delete failed, the first such error is returned after all six attempts.
pub fn unregister() -> std::io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    unregister_with(|ext| delete_ext(&hkcu, ext))
}

// manual: the all-or-nothing rollback is unit-tested at the orchestration layer
// (register_with/unregister_with with stubbed write/delete ops below); the real
// HKCU writes can't be driven from tests without mutating the dev machine's
// registry. On-device: with a policy-locked or contended key partway through,
// toggling the context menu ON rejects (the UI toggle stays OFF) and leaves NO
// stale "Compress with tamp" entry on any extension; toggling OFF removes the
// entry from every extension even if one delete transiently fails.

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
    use super::{command_value, register_with, unregister_with, verb_key, EXTS};
    use std::cell::RefCell;
    use std::io;

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

    fn not_found() -> io::Error {
        io::Error::new(io::ErrorKind::NotFound, "absent")
    }

    fn denied() -> io::Error {
        io::Error::new(io::ErrorKind::PermissionDenied, "locked key")
    }

    // A mid-loop register failure rolls back exactly the extensions written
    // before it, leaving NO partial registration — so the rejected toggle's
    // on-disk state matches its reverted (off) UI state.
    #[test]
    fn register_rolls_back_writes_when_a_later_extension_fails() {
        // Fail on the 3rd extension (the audit's example): exts 1-2 are written
        // then must be rolled back; 4-6 are never attempted.
        let fail_on = EXTS[2];
        let written = RefCell::new(Vec::<String>::new());
        let deleted = RefCell::new(Vec::<String>::new());

        let res = register_with(
            |ext| {
                if ext == fail_on {
                    return Err(denied());
                }
                written.borrow_mut().push(ext.to_string());
                Ok(())
            },
            |ext| {
                deleted.borrow_mut().push(ext.to_string());
                Ok(())
            },
        );

        assert!(res.is_err(), "a mid-loop write failure must surface as Err");
        // Only the two extensions before the failing one were written...
        assert_eq!(
            *written.borrow(),
            vec![EXTS[0].to_string(), EXTS[1].to_string()]
        );
        // ...and exactly those two were rolled back (deleted), nothing else.
        assert_eq!(
            *deleted.borrow(),
            vec![EXTS[0].to_string(), EXTS[1].to_string()]
        );
    }

    // A failure on the very first extension writes nothing, so there is nothing
    // to roll back.
    #[test]
    fn register_first_extension_failure_deletes_nothing() {
        let deleted = RefCell::new(Vec::<String>::new());
        let res = register_with(
            |_ext| Err(denied()),
            |ext| {
                deleted.borrow_mut().push(ext.to_string());
                Ok(())
            },
        );
        assert!(res.is_err());
        assert!(
            deleted.borrow().is_empty(),
            "nothing was written, so nothing is rolled back"
        );
    }

    // A clean run writes all six and rolls back nothing.
    #[test]
    fn register_all_succeed_writes_six_rolls_back_none() {
        let written = RefCell::new(Vec::<String>::new());
        let deleted = RefCell::new(Vec::<String>::new());
        let res = register_with(
            |ext| {
                written.borrow_mut().push(ext.to_string());
                Ok(())
            },
            |ext| {
                deleted.borrow_mut().push(ext.to_string());
                Ok(())
            },
        );
        assert!(res.is_ok());
        assert_eq!(written.borrow().len(), EXTS.len());
        assert!(deleted.borrow().is_empty());
    }

    // unregister is best-effort: every extension is attempted even when one
    // delete fails, and the failure is reported (so the UI knows it wasn't
    // fully clean) rather than aborting on the first error.
    #[test]
    fn unregister_attempts_all_and_reports_a_failure() {
        let fail_on = EXTS[1];
        let attempted = RefCell::new(Vec::<String>::new());
        let res = unregister_with(|ext| {
            attempted.borrow_mut().push(ext.to_string());
            if ext == fail_on {
                Err(denied())
            } else {
                Ok(())
            }
        });
        assert!(res.is_err(), "a delete failure must be reported");
        assert_eq!(
            attempted.borrow().len(),
            EXTS.len(),
            "every extension is attempted despite the mid-loop failure"
        );
    }

    // A NotFound from the delete op is already handled inside delete_ext (maps
    // to Ok); at the orchestration layer an explicit Ok for an absent key must
    // not be treated as a failure.
    #[test]
    fn unregister_all_clean_is_ok() {
        let res = unregister_with(|_ext| Ok(()));
        assert!(res.is_ok());
    }

    // delete_ext maps NotFound -> Ok; mirror that contract at the helper layer
    // so a missing key during best-effort unregister is not a reported failure.
    #[test]
    fn unregister_treats_handled_absent_keys_as_ok() {
        // Simulate the post-delete_ext contract: absent keys arrive as Ok.
        let res = unregister_with(|ext| {
            if ext == EXTS[0] {
                // delete_ext already swallowed the NotFound -> Ok
                let _ = not_found();
                Ok(())
            } else {
                Ok(())
            }
        });
        assert!(res.is_ok());
    }
}
