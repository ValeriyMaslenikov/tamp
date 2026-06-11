use crate::scanner::RecentVideo;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tauri::{AppHandle, Manager};
use tokio::task::JoinSet;

const MAX_CONCURRENT: usize = 4;

/// Cache key over path|mtime|size, shared by the thumbnail and duration
/// caches (`durations.rs`). DefaultHasher output may change across releases —
/// stale entries just age out of the caches.
pub(crate) fn cache_key(path: &str, size_bytes: u64) -> String {
    let mtime_ms: u128 = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    mtime_ms.hash(&mut hasher);
    size_bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn is_valid_thumb(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.len() > 0)
        .unwrap_or(false)
}

async fn generate(ffmpeg: &Path, input: &Path, output: &Path) -> bool {
    // Try grabbing a frame at 1s for a representative image; very short clips
    // produce nothing there, so retry from the start.
    for seek in [Some("1"), None] {
        let mut cmd = tokio::process::Command::new(ffmpeg);
        cmd.arg("-y");
        if let Some(seek) = seek {
            cmd.args(["-ss", seek]);
        }
        cmd.arg("-i")
            .arg(input)
            .args(["-frames:v", "1", "-vf", "scale=160:-2", "-q:v", "6"])
            .arg(output)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        match cmd.status().await {
            Ok(status) if status.success() && is_valid_thumb(output) => return true,
            Ok(_) => continue,
            Err(e) => {
                eprintln!("tamp: failed to spawn ffmpeg for thumbnail: {e}");
                return false;
            }
        }
    }
    // Don't leave a 0-byte file behind: the cache check would treat it as done.
    let _ = std::fs::remove_file(output);
    false
}

fn apply(videos: &mut [RecentVideo], idx: usize, thumb: Option<PathBuf>) {
    if let (Some(video), Some(path)) = (videos.get_mut(idx), thumb) {
        video.thumb_path = Some(path.to_string_lossy().into_owned());
    }
}

pub async fn ensure_thumbs(app: &AppHandle, videos: &mut [RecentVideo]) {
    let thumbs_dir = match app.path().app_cache_dir() {
        Ok(dir) => dir.join("thumbs"),
        Err(e) => {
            eprintln!("tamp: cannot resolve cache dir for thumbnails: {e}");
            return;
        }
    };
    if let Err(e) = std::fs::create_dir_all(&thumbs_dir) {
        eprintln!("tamp: cannot create thumbnail cache dir: {e}");
        return;
    }
    let ffmpeg = crate::encoder::bin::ffmpeg_path();

    let mut pending: Vec<(usize, PathBuf, PathBuf)> = Vec::new();
    for (idx, video) in videos.iter_mut().enumerate() {
        let out = thumbs_dir.join(format!("{}.jpg", cache_key(&video.path, video.size_bytes)));
        if is_valid_thumb(&out) {
            video.thumb_path = Some(out.to_string_lossy().into_owned());
        } else {
            pending.push((idx, PathBuf::from(&video.path), out));
        }
    }

    let mut tasks: JoinSet<(usize, Option<PathBuf>)> = JoinSet::new();
    for (idx, input, out) in pending {
        while tasks.len() >= MAX_CONCURRENT {
            match tasks.join_next().await {
                Some(Ok((done_idx, thumb))) => apply(videos, done_idx, thumb),
                Some(Err(e)) => eprintln!("tamp: thumbnail task failed: {e}"),
                None => break,
            }
        }
        let ffmpeg = ffmpeg.clone();
        tasks.spawn(async move {
            let ok = generate(&ffmpeg, &input, &out).await;
            (idx, ok.then_some(out))
        });
    }
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok((idx, thumb)) => apply(videos, idx, thumb),
            Err(e) => eprintln!("tamp: thumbnail task failed: {e}"),
        }
    }
}
