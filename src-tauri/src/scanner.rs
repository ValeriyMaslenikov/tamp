use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// What we know about how an orphaned output was produced. `original_bytes`
/// and `preset_name` come from the conversion journal when a record exists;
/// `output_bytes` is always the file's current size on disk.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionMeta {
    pub original_bytes: Option<u64>,
    pub output_bytes: u64,
    pub preset_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentVideo {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub created_ms: u64,
    pub thumb_path: Option<String>,
    /// Video duration in seconds; filled by the duration cache in
    /// `commands::list_recents` (not by `scan`), `None` until probed.
    pub duration_secs: Option<f64>,
    pub is_output: bool,
    pub conversion: Option<ConversionMeta>,
}

const VIDEO_EXTS: [&str; 6] = ["mov", "mp4", "m4v", "webm", "mkv", "avi"];

pub(crate) fn has_video_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| VIDEO_EXTS.iter().any(|v| ext.eq_ignore_ascii_case(v)))
        .unwrap_or(false)
}

/// Whether `path` can be represented as UTF-8 without loss. Non-UTF-8 recording
/// filenames are declared unsupported and skipped at scan time (see `scan`):
/// `RecentVideo.path` is a `String` that round-trips through IPC and back into
/// probe/copy/reveal, so a `to_string_lossy()` replacement (U+FFFD) would point
/// those at a wrong or nonexistent file. Rejecting here means the IPC layer only
/// ever sees a path that maps back to the real file. The clipboard's
/// `paths_to_utf8` guard (platform/mod.rs) is the same predicate, kept as a
/// backstop for paths that arrive by other routes (drops, the picker).
pub(crate) fn is_supported_filename(path: &Path) -> bool {
    path.to_str().is_some()
}

/// If `stem` ends with a tamped-output suffix, returns the derived original
/// stem (the stem with the whole suffix removed); `None` otherwise.
///
/// The recognizer lives in `encoder::plan` (the planner that emits these
/// names); the scanner delegates so the two can never drift. Covers hashed
/// names like `clip (tamped a3f2)` / `clip (tamped a3f2 2)`, named ones like
/// `clip (tamped Discord a3f2)`, split parts, and the legacy forms.
pub fn tamped_original_stem(stem: &str) -> Option<&str> {
    crate::encoder::plan::output_original_stem(stem)
}

fn created_unix_ms(meta: &std::fs::Metadata) -> u64 {
    // birthtime where the platform supports it (macOS does), mtime otherwise
    meta.created()
        .or_else(|_| meta.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Whether a `read_dir` error means the folder is *unreachable* (offline UNC
/// share, permission-denied, …) as opposed to legitimately missing. A
/// not-yet-created watched folder (e.g. Windows' `Videos\Screen Recordings`
/// before the first recording) returns `NotFound` and is NOT unreachable — it
/// is just empty. Everything else (PermissionDenied, network errors, …) is.
///
/// `scan` and `unreachable` both route their `read_dir` error through this so
/// the "empty vs. unreachable" classification can never drift between the list
/// the panel renders and the banner it shows.
fn is_unreachable_err(err: &std::io::Error) -> bool {
    err.kind() != std::io::ErrorKind::NotFound
}

/// The watched folders that exist but can't be read right now (offline network
/// drive, permission-denied), as display strings. A legitimately-missing
/// (`NotFound`) folder is omitted: it's an empty source, not an error worth
/// surfacing. Used by the `unreachable_folders` command to drive the Videos
/// tab's "couldn't read <folder>" banner, distinct from the empty state.
pub fn unreachable(folders: &[PathBuf]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for folder in folders {
        if let Err(e) = std::fs::read_dir(folder) {
            if is_unreachable_err(&e) {
                crate::log_warn!("watched folder {} is unreachable: {e}", folder.display());
                out.push(folder.to_string_lossy().into_owned());
            }
        }
    }
    out
}

pub fn scan(folders: &[PathBuf], limit: usize) -> Vec<RecentVideo> {
    let mut videos: Vec<RecentVideo> = Vec::new();
    for folder in folders {
        let entries = match std::fs::read_dir(folder) {
            Ok(entries) => entries,
            // An unreachable folder (offline UNC, permission-denied) is
            // surfaced separately by `unreachable()` and the Videos-tab
            // banner; a not-yet-created default folder (NotFound) is expected
            // and not worth a warning on every scan. Either way, skip it here.
            Err(e) => {
                if is_unreachable_err(&e) {
                    crate::log_warn!("cannot read watched folder {}: {e}", folder.display());
                } else {
                    crate::log_debug!("watched folder {} does not exist yet", folder.display());
                }
                continue;
            }
        };
        // Orphan detection needs the folder's full stem set before any output
        // can be classified, so collect first and classify second.
        let mut files: Vec<(PathBuf, String, std::fs::Metadata)> = Vec::new();
        let mut stems: HashSet<String> = HashSet::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !has_video_ext(&path) {
                continue;
            }
            // Non-UTF-8 filenames are unsupported: `RecentVideo.path` is a
            // `String`, and emitting a `to_string_lossy()` version (U+FFFD)
            // would mangle the path so probe/copy/reveal act on a wrong or
            // nonexistent file. Skip it here so the lossy string never enters
            // IPC, and warn so the skip is a distinct, visible condition (not a
            // silent drop) — `path.display()` itself is lossy for logging only.
            if !is_supported_filename(&path) {
                crate::log_warn!(
                    "skipping recording with a non-UTF-8 filename: {}",
                    path.display()
                );
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let meta = match entry.metadata() {
                Ok(meta) if meta.is_file() => meta,
                _ => continue,
            };
            stems.insert(stem.to_string());
            let name = entry.file_name().to_string_lossy().into_owned();
            files.push((path, name, meta));
        }
        for (path, name, meta) in files {
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let conversion = match tamped_original_stem(stem) {
                // Output whose original is still around: the source row covers it.
                Some(original) if stems.contains(original) => continue,
                // Orphaned output: surface it; the journal lookup (commands.rs)
                // fills original_bytes/preset_name when a record exists.
                Some(_) => Some(ConversionMeta {
                    original_bytes: None,
                    output_bytes: meta.len(),
                    preset_name: None,
                }),
                None => None,
            };
            videos.push(RecentVideo {
                path: path.to_string_lossy().into_owned(),
                name,
                size_bytes: meta.len(),
                created_ms: created_unix_ms(&meta),
                thumb_path: None,
                duration_secs: None,
                is_output: conversion.is_some(),
                conversion,
            });
        }
    }
    videos.sort_by_key(|v| std::cmp::Reverse(v.created_ms));
    videos.truncate(limit);
    videos
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    fn touch(dir: &Path, name: &str) {
        let mut file = File::create(dir.join(name)).unwrap();
        file.write_all(b"video bytes").unwrap();
    }

    /// A filename `<stem>.<ext>` whose stem is NOT valid UTF-8 (the extension
    /// stays ASCII so it still passes the video-extension gate). On Unix a raw
    /// continuation byte 0x80 is never valid UTF-8 on its own; on Windows an
    /// unpaired high surrogate (0xD800) has no UTF-8 mapping. Either way
    /// `OsString::to_str()` returns `None`.
    fn non_utf8_name(stem: &str, ext: &str) -> std::ffi::OsString {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let mut bytes = stem.as_bytes().to_vec();
            bytes.push(0x80); // lone continuation byte: invalid UTF-8
            bytes.push(b'.');
            bytes.extend_from_slice(ext.as_bytes());
            std::ffi::OsString::from_vec(bytes)
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            let mut units: Vec<u16> = stem.encode_utf16().collect();
            units.push(0xD800); // unpaired high surrogate: no UTF-8 mapping
            units.extend(format!(".{ext}").encode_utf16());
            std::ffi::OsString::from_wide(&units)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (stem, ext);
            std::ffi::OsString::from("placeholder.mp4")
        }
    }

    /// Best-effort create an empty file with an arbitrary (possibly non-UTF-8)
    /// `OsString` name; `None` if the filesystem refuses the byte sequence.
    fn touch_os(dir: &Path, name: &std::ffi::OsStr) -> Option<()> {
        let mut file = File::create(dir.join(name)).ok()?;
        file.write_all(b"video bytes").ok()?;
        Some(())
    }

    fn names(videos: &[RecentVideo]) -> Vec<&str> {
        videos.iter().map(|v| v.name.as_str()).collect()
    }

    #[test]
    fn filters_by_extension_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "a.mov",
            "b.MP4",
            "c.M4v",
            "d.webm",
            "e.MKV",
            "f.avi",
            "g.txt",
            "h.jpg",
            "noext",
            "i.mov.part",
        ] {
            touch(dir.path(), name);
        }
        let found = scan(&[dir.path().to_path_buf()], 100);
        let mut got = names(&found);
        got.sort();
        assert_eq!(
            got,
            vec!["a.mov", "b.MP4", "c.M4v", "d.webm", "e.MKV", "f.avi"]
        );
    }

    #[test]
    fn excludes_outputs_whose_original_still_exists() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "clip.mov",
            "clip (tamped).mp4",
            "clip (tamped 2).mp4",
            "clip (tamped 12).mov",
            "clip (tamped a3f2).mp4",
            "clip (tamped a3f2 2).mp4",
            "clip (tamped x).mov",
            "retamped.mov",
            "clip (tamped) take two.mov",
        ] {
            touch(dir.path(), name);
        }
        let found = scan(&[dir.path().to_path_buf()], 100);
        let mut got = names(&found);
        got.sort();
        assert_eq!(
            got,
            vec![
                "clip (tamped x).mov",
                "clip (tamped) take two.mov",
                "clip.mov",
                "retamped.mov",
            ]
        );
        assert!(found.iter().all(|v| !v.is_output && v.conversion.is_none()));
    }

    #[test]
    fn includes_orphaned_outputs_with_metadata() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "gone (tamped).mp4"); // legacy orphan
        touch(dir.path(), "lost (tamped a3f2).mp4"); // hashed orphan
        touch(dir.path(), "lost (tamped a3f2 2).mp4"); // hashed + counter orphan
        touch(dir.path(), "kept.mov");
        touch(dir.path(), "kept (tamped 2).mp4"); // original still there

        let found = scan(&[dir.path().to_path_buf()], 100);
        let mut got = names(&found);
        got.sort();
        assert_eq!(
            got,
            vec![
                "gone (tamped).mp4",
                "kept.mov",
                "lost (tamped a3f2 2).mp4",
                "lost (tamped a3f2).mp4",
            ]
        );

        let kept = found.iter().find(|v| v.name == "kept.mov").unwrap();
        assert!(!kept.is_output);
        assert!(kept.conversion.is_none());

        for orphan in found.iter().filter(|v| v.name != "kept.mov") {
            assert!(orphan.is_output, "{} must be an output", orphan.name);
            let meta = orphan.conversion.as_ref().expect("orphan conversion meta");
            assert_eq!(meta.output_bytes, orphan.size_bytes);
            assert_eq!(meta.original_bytes, None);
            assert_eq!(meta.preset_name, None);
        }
    }

    #[test]
    fn part_outputs_excluded_while_original_exists() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "clip.mov",
            "clip (tamped 823f p1of3).mp4",
            "clip (tamped 823f p2of3).mp4",
            "clip (tamped 823f p3of3).mp4",
            // grammar admits a part token without a hash too
            "clip (tamped p2of5).mp4",
        ] {
            touch(dir.path(), name);
        }
        let found = scan(&[dir.path().to_path_buf()], 100);
        assert_eq!(names(&found), vec!["clip.mov"]);
        assert!(!found[0].is_output);
    }

    #[test]
    fn part_outputs_shown_as_orphans_without_original() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "gone (tamped 823f p1of2).mp4");
        touch(dir.path(), "gone (tamped 823f p2of2).mp4");
        let found = scan(&[dir.path().to_path_buf()], 100);
        let mut got = names(&found);
        got.sort();
        assert_eq!(
            got,
            vec![
                "gone (tamped 823f p1of2).mp4",
                "gone (tamped 823f p2of2).mp4",
            ]
        );
        for orphan in &found {
            assert!(orphan.is_output, "{} must be an output", orphan.name);
            let meta = orphan.conversion.as_ref().expect("orphan conversion meta");
            assert_eq!(meta.output_bytes, orphan.size_bytes);
            assert_eq!(meta.original_bytes, None);
            assert_eq!(meta.preset_name, None);
        }
    }

    #[test]
    fn part_outputs_mix_with_legacy_hash_and_counter_forms() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "clip.mov",
            "clip (tamped).mp4",            // legacy
            "clip (tamped 2).mp4",          // legacy + counter
            "clip (tamped a3f2).mp4",       // hash
            "clip (tamped a3f2 2).mp4",     // hash + counter
            "clip (tamped a3f2 p1of2).mp4", // hash + part
            "clip (tamped a3f2 p2of2).mp4",
            // malformed part tokens are not outputs, so they stay listed
            "clip (tamped a3f2 pof3).mp4",
            "clip (tamped a3f2 2 p1of2).mp4", // counter AND part: never both
        ] {
            touch(dir.path(), name);
        }
        let found = scan(&[dir.path().to_path_buf()], 100);
        let mut got = names(&found);
        got.sort();
        assert_eq!(
            got,
            vec![
                "clip (tamped a3f2 2 p1of2).mp4",
                "clip (tamped a3f2 pof3).mp4",
                "clip.mov",
            ]
        );
    }

    #[test]
    fn original_with_different_extension_still_suppresses_output() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "demo.webm");
        touch(dir.path(), "demo (tamped a3f2).mp4");
        let found = scan(&[dir.path().to_path_buf()], 100);
        assert_eq!(names(&found), vec!["demo.webm"]);
    }

    #[test]
    fn derives_original_stem() {
        // legacy names (no hash)
        assert_eq!(tamped_original_stem("clip (tamped)"), Some("clip"));
        assert_eq!(tamped_original_stem("clip (tamped 2)"), Some("clip"));
        assert_eq!(tamped_original_stem("clip (tamped 42)"), Some("clip"));
        // hashed names
        assert_eq!(tamped_original_stem("clip (tamped a3f2)"), Some("clip"));
        assert_eq!(tamped_original_stem("clip (tamped 0042)"), Some("clip"));
        assert_eq!(tamped_original_stem("clip (tamped a3f2 2)"), Some("clip"));
        assert_eq!(tamped_original_stem("clip (tamped a3f2 12)"), Some("clip"));
        // originals whose own name contains parentheses
        assert_eq!(
            tamped_original_stem("demo (1) (tamped a3f2)"),
            Some("demo (1)")
        );
        assert_eq!(tamped_original_stem("demo (1) (tamped)"), Some("demo (1)"));
        // chained outputs strip only the trailing suffix
        assert_eq!(
            tamped_original_stem("demo (tamped a3f2) (tamped 9bd0)"),
            Some("demo (tamped a3f2)")
        );
    }

    #[test]
    fn derives_original_stem_for_part_tokens() {
        assert_eq!(
            tamped_original_stem("clip (tamped 823f p1of3)"),
            Some("clip")
        );
        assert_eq!(
            tamped_original_stem("clip (tamped 823f p2of5)"),
            Some("clip")
        );
        assert_eq!(
            tamped_original_stem("clip (tamped 823f p12of20)"),
            Some("clip")
        );
        // part token without a hash is within the grammar
        assert_eq!(tamped_original_stem("clip (tamped p2of5)"), Some("clip"));
        // the contract grammar is p<digits>of<digits> with no further
        // validation, so out-of-range digits are still recognised; must stay
        // consistent with encoder::plan::output_original_stem
        assert_eq!(
            tamped_original_stem("clip (tamped 823f p0of0)"),
            Some("clip")
        );
        // originals whose own name contains parentheses
        assert_eq!(
            tamped_original_stem("demo (1) (tamped 823f p1of2)"),
            Some("demo (1)")
        );
    }

    #[test]
    fn rejects_malformed_part_tokens() {
        assert_eq!(tamped_original_stem("clip (tamped 823f pof3)"), None);
        assert_eq!(tamped_original_stem("clip (tamped 823f p2of)"), None);
        assert_eq!(tamped_original_stem("clip (tamped 823f pof)"), None);
        assert_eq!(tamped_original_stem("clip (tamped 823f P2of5)"), None);
        assert_eq!(tamped_original_stem("clip (tamped 823f p2OF5)"), None);
        assert_eq!(tamped_original_stem("clip (tamped 823f p2of5x)"), None);
        assert_eq!(tamped_original_stem("clip (tamped 823f px2of5)"), None);
        assert_eq!(tamped_original_stem("clip (tamped 823f 2of5)"), None);
        // counter and part token never appear together, in either order
        assert_eq!(tamped_original_stem("clip (tamped 823f 2 p1of2)"), None);
        assert_eq!(tamped_original_stem("clip (tamped 823f p1of2 2)"), None);
        // part token must follow a valid hash when two tokens are present
        assert_eq!(tamped_original_stem("clip (tamped A3F2 p1of2)"), None);
        assert_eq!(tamped_original_stem("clip (tamped 2 p1of2)"), None);
    }

    #[test]
    fn rejects_non_output_stems() {
        assert_eq!(tamped_original_stem("clip"), None);
        assert_eq!(tamped_original_stem("retamped"), None);
        assert_eq!(tamped_original_stem("clip (tamped )"), None);
        assert_eq!(tamped_original_stem("clip (tamped x)"), None);
        assert_eq!(tamped_original_stem("clip (tamped) more"), None);
        // hash must be exactly 4 lowercase hex chars
        assert_eq!(tamped_original_stem("clip (tamped A3F2)"), None);
        assert_eq!(tamped_original_stem("clip (tamped a3f)"), None);
        assert_eq!(tamped_original_stem("clip (tamped a3f21)"), None);
        // counter must be digits and follow a valid hash
        assert_eq!(tamped_original_stem("clip (tamped a3f2 x)"), None);
        assert_eq!(tamped_original_stem("clip (tamped a3f2 2 3)"), None);
        assert_eq!(tamped_original_stem("clip (tamped  2)"), None);
    }

    #[test]
    fn sorts_newest_first_and_applies_limit() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["old.mov", "mid.mov", "new.mov"] {
            touch(dir.path(), name);
            // birthtime has sub-millisecond resolution on APFS; the sleep keeps
            // created_ms strictly increasing between files
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let all = scan(&[dir.path().to_path_buf()], 100);
        assert_eq!(names(&all), vec!["new.mov", "mid.mov", "old.mov"]);

        let limited = scan(&[dir.path().to_path_buf()], 2);
        assert_eq!(names(&limited), vec!["new.mov", "mid.mov"]);
    }

    #[test]
    fn orphans_compete_with_sources_for_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["old.mov", "orphan (tamped a3f2).mp4", "new.mov"] {
            touch(dir.path(), name);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let limited = scan(&[dir.path().to_path_buf()], 2);
        assert_eq!(names(&limited), vec!["new.mov", "orphan (tamped a3f2).mp4"]);
    }

    #[test]
    fn scans_multiple_folders_and_survives_missing_ones() {
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        touch(dir_a.path(), "a.mov");
        touch(dir_b.path(), "b.mp4");
        let folders = vec![
            dir_a.path().to_path_buf(),
            PathBuf::from("/nonexistent/tamp-test"),
            dir_b.path().to_path_buf(),
        ];
        let found = scan(&folders, 100);
        let mut got = names(&found);
        got.sort();
        assert_eq!(got, vec!["a.mov", "b.mp4"]);
    }

    #[test]
    fn unreachable_reports_unreadable_but_not_missing_folders() {
        // A folder that exists and is readable is reachable.
        let ok = tempfile::tempdir().unwrap();
        // A path that exists but can't be enumerated as a directory (here, a
        // regular file) stands in for an offline/permission-denied folder:
        // read_dir returns an error whose kind is NOT NotFound — exactly the
        // class `is_unreachable_err` flags. (Cross-platform; no ACL fiddling.)
        let blocked_dir = tempfile::tempdir().unwrap();
        let blocked = blocked_dir.path().join("not-a-dir");
        touch(blocked_dir.path(), "not-a-dir");
        assert!(std::fs::read_dir(&blocked).unwrap_err().kind() != std::io::ErrorKind::NotFound);
        // A simply-missing folder is NOT unreachable: it's an empty source.
        let missing = PathBuf::from("/nonexistent/tamp-unreachable-test");
        assert_eq!(
            std::fs::read_dir(&missing).unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );

        let got = unreachable(&[ok.path().to_path_buf(), blocked.clone(), missing]);
        assert_eq!(got, vec![blocked.to_string_lossy().into_owned()]);
        // manual: add an offline UNC path as a watched folder → the Videos tab
        // shows the "couldn't read <folder>" banner, not "no recordings".
    }

    #[test]
    fn is_supported_filename_gates_on_lossless_utf8() {
        // ASCII / valid UTF-8 paths are supported.
        assert!(is_supported_filename(Path::new("/rec/clip.mov")));
        assert!(is_supported_filename(Path::new(
            "/rec/café (tamped a3f2).mp4"
        )));

        // A path whose filename is NOT valid UTF-8 is rejected, so the scanner
        // never laundered it through `to_string_lossy` into a `RecentVideo`.
        let bad = non_utf8_name("clip", "mp4");
        assert!(
            bad.to_str().is_none(),
            "test fixture must be a genuinely non-UTF-8 name"
        );
        assert!(!is_supported_filename(&PathBuf::from("/rec").join(&bad)));
    }

    #[test]
    fn scan_skips_non_utf8_filenames_instead_of_mangling_them() {
        let dir = tempfile::tempdir().unwrap();
        // A normal recording alongside one with a non-UTF-8 filename and a valid
        // `.mp4` extension (the extension is ASCII; only the stem is invalid —
        // exactly the case `to_string_lossy` would have mangled into a U+FFFD
        // path that probe/copy/reveal then mis-target).
        touch(dir.path(), "good.mp4");
        if touch_os(dir.path(), &non_utf8_name("bad", "mp4")).is_none() {
            // Some filesystems reject the byte sequence outright; the UTF-8 gate
            // itself is still covered by `is_supported_filename_gates_*`.
            return;
        }
        let folders = vec![dir.path().to_path_buf()];

        // The non-UTF-8 row is skipped, never emitted as a lossy RecentVideo…
        let found = scan(&folders, 100);
        assert_eq!(names(&found), vec!["good.mp4"]);
        // …so no row carries a laundered path: `to_string_lossy` would have
        // injected the U+FFFD replacement char, and that mangled string would
        // not round-trip back to the real file for copy/reveal/probe.
        assert!(
            found
                .iter()
                .all(|v| !v.path.contains('\u{FFFD}') && !v.name.contains('\u{FFFD}')),
            "no row may carry a U+FFFD-mangled path or name"
        );
        // manual: a file with a non-UTF-8 name is skipped (with a warn log),
        // never mangled into a broken row that probe/copy/reveal mis-targets.
    }

    #[test]
    fn orphan_check_is_per_folder() {
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        // The original lives in another folder, so the output is an orphan in its own.
        touch(dir_a.path(), "clip.mov");
        touch(dir_b.path(), "clip (tamped a3f2).mp4");
        let folders = vec![dir_a.path().to_path_buf(), dir_b.path().to_path_buf()];
        let found = scan(&folders, 100);
        let mut got = names(&found);
        got.sort();
        assert_eq!(got, vec!["clip (tamped a3f2).mp4", "clip.mov"]);
        assert!(
            found
                .iter()
                .find(|v| v.name == "clip (tamped a3f2).mp4")
                .unwrap()
                .is_output
        );
    }
}
