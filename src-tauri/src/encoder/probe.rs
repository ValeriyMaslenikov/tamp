use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug)]
pub struct ProbeInfo {
    pub duration_secs: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub has_audio: bool,
}

/// Process-lifetime in-memory probe cache: the same `path|mtime|size` key the
/// thumbnail/duration caches use (`thumbs::cache_key`) mapped to the full
/// `ProbeInfo`. Lets a single open → hover → convert of one file spawn one
/// ffprobe instead of three. Only **successful** probes are stored — a failure
/// is never memoized, so a transient error or a since-fixed file re-probes.
fn cache() -> &'static Mutex<HashMap<String, ProbeInfo>> {
    static CACHE: OnceLock<Mutex<HashMap<String, ProbeInfo>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The cache key for `path`, derived from its current path/mtime/size so a
/// re-recorded file (same path, new content) misses and re-probes.
fn key_for(path: &Path) -> String {
    let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    crate::thumbs::cache_key(&path.to_string_lossy(), size_bytes)
}

/// Returns the cached `ProbeInfo` for `path`, or probes once with ffprobe and
/// stores the result. Used by `durations::ensure_one`, `previews::generate`,
/// and the encoder so the same file is probed at most once per content version.
pub async fn probe_cached(path: &Path) -> Result<ProbeInfo, String> {
    probe_cached_with(path, || probe(path)).await
}

/// Cache core, generic over the probe function so tests can inject a
/// call-counting stub. Returns a cache hit without invoking `probe_fn`; on a
/// miss it runs `probe_fn`, storing only a successful (`Ok`) result.
async fn probe_cached_with<F, Fut>(path: &Path, probe_fn: F) -> Result<ProbeInfo, String>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<ProbeInfo, String>>,
{
    let key = key_for(path);
    if let Some(info) = cache().lock().unwrap().get(&key).cloned() {
        return Ok(info);
    }
    let info = probe_fn().await?;
    cache().lock().unwrap().insert(key, info.clone());
    Ok(info)
}

pub async fn probe(path: &Path) -> Result<ProbeInfo, String> {
    let output = crate::platform::background_command(super::bin::ffprobe_path())
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-show_entries",
            "stream=codec_type,width,height,avg_frame_rate,duration",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .await
        .map_err(|e| format!("failed to run ffprobe: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "ffprobe failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("invalid ffprobe output: {e}"))?;

    let empty = Vec::new();
    let streams = json["streams"].as_array().unwrap_or(&empty);
    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video")
        .ok_or_else(|| "no video stream found".to_string())?;

    // MediaRecorder WebMs often carry no container duration; fall back to the
    // video stream's duration, then to scanning packet timestamps.
    let duration_secs =
        match parse_secs(&json["format"]["duration"]).or_else(|| parse_secs(&video["duration"])) {
            Some(d) => d,
            None => duration_from_packets(path)
                .await
                .ok_or_else(|| "ffprobe reported no duration".to_string())?,
        };

    Ok(ProbeInfo {
        duration_secs,
        width: video["width"].as_u64().unwrap_or(0) as u32,
        height: video["height"].as_u64().unwrap_or(0) as u32,
        fps: parse_frame_rate(video["avg_frame_rate"].as_str().unwrap_or("")),
        has_audio: streams.iter().any(|s| s["codec_type"] == "audio"),
    })
}

fn parse_secs(value: &serde_json::Value) -> Option<f64> {
    value
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|d| d.is_finite() && *d > 0.0)
}

/// Last-resort duration: demux the video stream and take the largest packet
/// timestamp. Demux-only (no decode), so it stays fast even for big files.
async fn duration_from_packets(path: &Path) -> Option<f64> {
    let output = crate::platform::background_command(super::bin::ffprobe_path())
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "packet=pts_time,dts_time",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .flat_map(|line| line.split(',').filter_map(|v| v.parse::<f64>().ok()))
        .reduce(f64::max)
        .filter(|d| d.is_finite() && *d > 0.0)
}

fn parse_frame_rate(rate: &str) -> f64 {
    match rate.split_once('/') {
        Some((num, den)) => {
            let num: f64 = num.parse().unwrap_or(0.0);
            let den: f64 = den.parse().unwrap_or(0.0);
            if den > 0.0 {
                num / den
            } else {
                0.0
            }
        }
        None => rate.parse().unwrap_or(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn parses_rational_frame_rates() {
        assert!((parse_frame_rate("30000/1001") - 29.97).abs() < 0.01);
        assert_eq!(parse_frame_rate("30/1"), 30.0);
        assert_eq!(parse_frame_rate("0/0"), 0.0);
        assert_eq!(parse_frame_rate("60"), 60.0);
        assert_eq!(parse_frame_rate(""), 0.0);
    }

    fn sample_info(duration: f64) -> ProbeInfo {
        ProbeInfo {
            duration_secs: duration,
            width: 1920,
            height: 1080,
            fps: 30.0,
            has_audio: true,
        }
    }

    #[test]
    fn second_lookup_hits_the_cache_without_reprobing() {
        // A unique on-disk file so the path|mtime|size key can't collide with
        // another test sharing the process-global cache.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.mp4");
        std::fs::write(&path, b"sample").unwrap();

        let calls = AtomicUsize::new(0);
        let runtime = tokio::runtime::Runtime::new().unwrap();

        let first = runtime.block_on(probe_cached_with(&path, || {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(sample_info(12.0)) }
        }));
        assert_eq!(first.unwrap().duration_secs, 12.0);
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // Second lookup for the same key must return the cached value and
        // NOT invoke the probe fn again.
        let second = runtime.block_on(probe_cached_with(&path, || {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(sample_info(99.0)) }
        }));
        assert_eq!(second.unwrap().duration_secs, 12.0);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "probe fn re-invoked on a hit"
        );
    }

    #[test]
    fn a_failed_probe_is_not_cached() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.mp4");
        std::fs::write(&path, b"sample").unwrap();

        let calls = AtomicUsize::new(0);
        let runtime = tokio::runtime::Runtime::new().unwrap();

        // First probe fails — the failure must not be memoized.
        let first = runtime.block_on(probe_cached_with(&path, || {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err("transient ffprobe failure".to_string()) }
        }));
        assert!(first.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // A later probe for the same key re-invokes the probe fn (the Err was
        // not cached) and this time succeeds.
        let second = runtime.block_on(probe_cached_with(&path, || {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(sample_info(7.0)) }
        }));
        assert_eq!(second.unwrap().duration_secs, 7.0);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "Err was cached — fn not re-invoked"
        );

        // The successful probe IS now cached: a third lookup is a hit.
        let third = runtime.block_on(probe_cached_with(&path, || {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(sample_info(123.0)) }
        }));
        assert_eq!(third.unwrap().duration_secs, 7.0);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
