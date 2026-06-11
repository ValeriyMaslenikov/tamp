use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentVideo {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub created_ms: u64,
    pub thumb_path: Option<String>,
}

const VIDEO_EXTS: [&str; 6] = ["mov", "mp4", "m4v", "webm", "mkv", "avi"];

fn has_video_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| VIDEO_EXTS.iter().any(|v| ext.eq_ignore_ascii_case(v)))
        .unwrap_or(false)
}

/// True when a file stem ends with " (tamped)" or " (tamped N)" where N is digits —
/// i.e. it's one of our own outputs and must not be offered for re-compression.
fn is_tamped_output(stem: &str) -> bool {
    if stem.ends_with(" (tamped)") {
        return true;
    }
    if let Some(idx) = stem.rfind(" (tamped ") {
        let rest = &stem[idx + " (tamped ".len()..];
        if let Some(num) = rest.strip_suffix(')') {
            return !num.is_empty() && num.bytes().all(|b| b.is_ascii_digit());
        }
    }
    false
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
            Err(e) => {
                eprintln!("tamp: cannot read watched folder {}: {e}", folder.display());
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !has_video_ext(&path) {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if is_tamped_output(stem) {
                continue;
            }
            let meta = match entry.metadata() {
                Ok(meta) if meta.is_file() => meta,
                _ => continue,
            };
            videos.push(RecentVideo {
                path: path.to_string_lossy().into_owned(),
                name: entry.file_name().to_string_lossy().into_owned(),
                size_bytes: meta.len(),
                created_ms: created_unix_ms(&meta),
                thumb_path: None,
            });
        }
    }
    videos.sort_by(|a, b| b.created_ms.cmp(&a.created_ms));
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
    fn excludes_tamped_outputs() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "clip.mov",
            "clip (tamped).mp4",
            "clip (tamped 2).mp4",
            "clip (tamped 12).mov",
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
    }

    #[test]
    fn tamped_matcher() {
        assert!(is_tamped_output("clip (tamped)"));
        assert!(is_tamped_output("clip (tamped 2)"));
        assert!(is_tamped_output("clip (tamped 42)"));
        assert!(!is_tamped_output("clip (tamped )"));
        assert!(!is_tamped_output("clip (tamped x)"));
        assert!(!is_tamped_output("clip (tamped) more"));
        assert!(!is_tamped_output("clip"));
        assert!(!is_tamped_output("retamped"));
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
}
