//! Lightweight hover-preview proxies: a short montage mp4 sampled from the
//! source video, cached under `{app_cache_dir}/previews`.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;
use tauri::{AppHandle, Manager};

/// Cache keeps only this many montage files (newest by mtime).
const MAX_CACHED: usize = 40;
/// At most this many ffmpeg montage generations run at once.
const MAX_CONCURRENT: usize = 2;

/// Dedupes concurrent requests for the same preview (per-key locks) and caps
/// how many generations run in parallel. Managed as Tauri state in `lib.rs`.
pub struct Previews {
    permits: tokio::sync::Semaphore,
    in_flight: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl Default for Previews {
    fn default() -> Previews {
        Previews {
            permits: tokio::sync::Semaphore::new(MAX_CONCURRENT),
            in_flight: Mutex::new(HashMap::new()),
        }
    }
}

/// Same key scheme as `thumbs.rs`: DefaultHasher over path|mtime|size. Cache
/// keys may change across releases — stale entries just age out of the cache.
fn preview_key(path: &str) -> String {
    let (mtime_ms, size): (u128, u64) = std::fs::metadata(path)
        .map(|m| {
            let mtime_ms = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis())
                .unwrap_or(0);
            (mtime_ms, m.len())
        })
        .unwrap_or((0, 0));
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    mtime_ms.hash(&mut hasher);
    size.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn is_valid(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.len() > 0)
        .unwrap_or(false)
}

/// Returns the absolute path of the cached montage for `path`, generating it
/// on a cache miss.
pub async fn ensure_preview(app: &AppHandle, path: &str) -> Result<String, String> {
    let input = PathBuf::from(path);
    if !input.is_file() {
        return Err(format!("not a file: {path}"));
    }
    let previews_dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("cannot resolve cache dir for previews: {e}"))?
        .join("previews");
    std::fs::create_dir_all(&previews_dir)
        .map_err(|e| format!("cannot create preview cache dir: {e}"))?;

    let key = preview_key(path);
    let cached = previews_dir.join(format!("{key}.mp4"));
    if is_valid(&cached) {
        return Ok(cached.to_string_lossy().into_owned());
    }

    let state = app.state::<Previews>();
    // Per-key lock: concurrent requests for the same video wait here and then
    // hit the cache instead of generating the montage twice.
    let key_lock = {
        let mut in_flight = state.in_flight.lock().unwrap();
        in_flight.entry(key.clone()).or_default().clone()
    };
    let result = {
        let _key_guard = key_lock.lock().await;
        if is_valid(&cached) {
            Ok(())
        } else {
            let _permit = state
                .permits
                .acquire()
                .await
                .map_err(|e| format!("preview scheduler unavailable: {e}"))?;
            generate(&input, &cached).await
        }
    };
    state.in_flight.lock().unwrap().remove(&key);
    if let Err(e) = result {
        crate::log_warn!("preview generation failed for {path}: {e}");
        return Err(e);
    }

    prune(&previews_dir);
    Ok(cached.to_string_lossy().into_owned())
}

/// Sample points: one segment for short clips, four spread across the
/// timeline otherwise. Returns (start_secs, length_secs) pairs.
fn segments_for(duration_secs: f64) -> Vec<(f64, f64)> {
    if duration_secs <= 8.0 {
        vec![(0.0, duration_secs.min(6.0))]
    } else {
        [0.05, 0.30, 0.55, 0.80]
            .iter()
            .map(|p| (duration_secs * p, 1.5))
            .collect()
    }
}

async fn generate(input: &Path, cached: &Path) -> Result<(), String> {
    let info = crate::encoder::probe::probe_cached(input).await?;
    let segments = segments_for(info.duration_secs);

    let ffmpeg = crate::encoder::bin::ffmpeg_path();
    let tmp = tempfile::tempdir().map_err(|e| format!("cannot create preview temp dir: {e}"))?;

    let mut concat_list = String::new();
    for (i, (start, len)) in segments.iter().enumerate() {
        let seg_name = format!("seg{i}.mp4");
        let seg = tmp.path().join(&seg_name);
        // Input-side -ss for fast seeking; ultrafast/CRF 30 because this is a
        // throwaway proxy, not a deliverable. `-v error` keeps the captured
        // stderr to actual error lines.
        let out = crate::platform::background_command(&ffmpeg)
            .args([
                "-y",
                "-v",
                "error",
                "-ss",
                &format!("{start:.3}"),
                "-t",
                &format!("{len:.3}"),
                "-i",
            ])
            .arg(input)
            .args([
                "-vf",
                "fps=10,scale=400:-2,format=yuv420p",
                "-an",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-crf",
                "30",
                "-movflags",
                "+faststart",
            ])
            .arg(&seg)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| format!("failed to run ffmpeg for preview segment: {e}"))?;
        if !out.status.success() || !is_valid(&seg) {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(format!(
                "preview segment {i} failed ({}): {}",
                out.status,
                stderr.trim()
            ));
        }
        concat_list.push_str(&format!("file '{seg_name}'\n"));
    }

    // Stitch the segments with the concat demuxer; relative names resolve
    // against the list file's directory (the tempdir).
    let list_path = tmp.path().join("list.txt");
    std::fs::write(&list_path, concat_list)
        .map_err(|e| format!("cannot write concat list: {e}"))?;
    let joined = tmp.path().join("preview.mp4");
    let out = crate::platform::background_command(&ffmpeg)
        .args(["-y", "-v", "error", "-f", "concat", "-safe", "0", "-i"])
        .arg(&list_path)
        .args(["-c", "copy", "-movflags", "+faststart"])
        .arg(&joined)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("failed to run ffmpeg concat for preview: {e}"))?;
    if !out.status.success() || !is_valid(&joined) {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "preview concat failed ({}): {}",
            out.status,
            stderr.trim()
        ));
    }

    // Land in the cache atomically: copy into a sibling .part first (the
    // tempdir may be on another volume), then rename within the cache dir.
    let part = cached.with_extension("mp4.part");
    std::fs::copy(&joined, &part).map_err(|e| format!("cannot store preview: {e}"))?;
    std::fs::rename(&part, cached).map_err(|e| format!("cannot finalize preview: {e}"))?;
    Ok(())
}

/// Best-effort cap on cache size: keep the `MAX_CACHED` newest files by mtime.
fn prune(dir: &Path) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            crate::log_warn!("cannot read preview cache dir for pruning: {e}");
            return;
        }
    };
    let mut files: Vec<(PathBuf, std::time::SystemTime)> = entries
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            Some((entry.path(), meta.modified().unwrap_or(UNIX_EPOCH)))
        })
        .collect();
    if files.len() <= MAX_CACHED {
        return;
    }
    files.sort_by_key(|(_, mtime)| std::cmp::Reverse(*mtime));
    for (path, _) in files.drain(MAX_CACHED..) {
        if let Err(e) = std::fs::remove_file(&path) {
            crate::log_warn!("failed to prune preview {}: {e}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::segments_for;

    fn assert_close(actual: &[(f64, f64)], expected: &[(f64, f64)]) {
        assert_eq!(actual.len(), expected.len());
        for ((a_start, a_len), (e_start, e_len)) in actual.iter().zip(expected) {
            assert!(
                (a_start - e_start).abs() < 1e-9,
                "{actual:?} vs {expected:?}"
            );
            assert!((a_len - e_len).abs() < 1e-9, "{actual:?} vs {expected:?}");
        }
    }

    #[test]
    fn short_clips_get_one_segment_from_the_start() {
        assert_close(&segments_for(8.0), &[(0.0, 6.0)]);
        assert_close(&segments_for(6.0), &[(0.0, 6.0)]);
        assert_close(&segments_for(3.5), &[(0.0, 3.5)]);
    }

    #[test]
    fn long_clips_get_four_spread_segments() {
        assert_close(
            &segments_for(100.0),
            &[(5.0, 1.5), (30.0, 1.5), (55.0, 1.5), (80.0, 1.5)],
        );
        // The last segment always fits: 0.8d + 1.5 <= d for every d > 8.
        let (start, len) = *segments_for(8.1).last().unwrap();
        assert!(start + len <= 8.1);
    }
}
