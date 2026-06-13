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

fn has_video_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| VIDEO_EXTS.iter().any(|v| ext.eq_ignore_ascii_case(v)))
        .unwrap_or(false)
}

/// If `stem` ends with a tamped-output suffix, returns the derived original
/// stem (the stem with the whole suffix removed); `None` otherwise.
///
/// The suffix grammar (must agree with output naming in `encoder::plan`):
/// `" (tamped"` + optional `" "` + 4 lowercase hex chars + optional `" "` +
/// (collision-counter digits OR a `p<digits>of<digits>` part token — never
/// both) + `")"`. That covers hashed names like `clip (tamped a3f2)` /
/// `clip (tamped a3f2 2)`, split parts like `clip (tamped a3f2 p2of5)`, and
/// the legacy `clip (tamped)` / `clip (tamped 2)` ones.
pub fn tamped_original_stem(stem: &str) -> Option<&str> {
    let inner = stem.strip_suffix(')')?;
    let idx = inner.rfind(" (tamped")?;
    let rest = &inner[idx + " (tamped".len()..];
    suffix_args_match(rest).then(|| &inner[..idx])
}

/// Matches what may sit between `" (tamped"` and the closing `")"`:
/// nothing, `" {hash}"`, `" {digits}"`, `" {part}"`, `" {hash} {digits}"`,
/// or `" {hash} {part}"`.
fn suffix_args_match(rest: &str) -> bool {
    if rest.is_empty() {
        return true;
    }
    let Some(rest) = rest.strip_prefix(' ') else {
        return false;
    };
    match rest.split_once(' ') {
        None => is_preset_hash(rest) || is_digits(rest) || is_part_token(rest),
        Some((hash, arg)) => is_preset_hash(hash) && (is_digits(arg) || is_part_token(arg)),
    }
}

/// Exactly 4 lowercase hex chars, the shape `encoder::plan::preset_hash` emits.
fn is_preset_hash(s: &str) -> bool {
    s.len() == 4 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// `p<digits>of<digits>` — the split-part token (`encoder::plan` emits
/// `p{i}of{n}`). Purely shape-based: no range checks, so e.g. `p0of0` is
/// recognised; the planner only ever writes 1-based `i <= n` names.
fn is_part_token(s: &str) -> bool {
    let Some(rest) = s.strip_prefix('p') else {
        return false;
    };
    match rest.split_once("of") {
        Some((i, n)) => is_digits(i) && is_digits(n),
        None => false,
    }
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

pub fn scan(folders: &[PathBuf], limit: usize) -> Vec<RecentVideo> {
    let mut videos: Vec<RecentVideo> = Vec::new();
    for folder in folders {
        let entries = match std::fs::read_dir(folder) {
            Ok(entries) => entries,
            // A default watched folder (e.g. Windows' Videos\Screen
            // Recordings) may not exist until the user first records there;
            // that's expected, not worth a warning on every scan.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                crate::log_debug!("watched folder {} does not exist yet", folder.display());
                continue;
            }
            Err(e) => {
                crate::log_warn!("cannot read watched folder {}: {e}", folder.display());
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
