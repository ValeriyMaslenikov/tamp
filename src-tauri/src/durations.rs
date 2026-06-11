//! Per-file video duration cache ({app_cache_dir}/durations.json): the
//! thumbs-style cache key (path|mtime|size hash, see `thumbs::cache_key`)
//! mapped to the duration in seconds. `commands::list_recents` fills
//! `RecentVideo::duration_secs` from here, probing misses with ffprobe.

use crate::scanner::RecentVideo;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tokio::task::JoinSet;

const CACHE_FILE: &str = "durations.json";
/// The map keeps at most this many entries; extras are evicted on persist.
const MAX_ENTRIES: usize = 500;
/// At most this many ffprobe duration probes run at once.
const MAX_CONCURRENT: usize = 4;

/// Managed state (see `lib.rs`) holding the cache, loaded from disk once.
pub struct Durations {
    map: Mutex<HashMap<String, f64>>,
    /// None when the app cache dir could not be resolved; the cache then
    /// works in memory only (persistence is best-effort anyway).
    path: Option<PathBuf>,
}

impl Durations {
    pub fn load(app: &AppHandle) -> Durations {
        match app.path().app_cache_dir() {
            Ok(dir) => Durations::load_from_path(dir.join(CACHE_FILE)),
            Err(e) => {
                eprintln!("tamp: cannot resolve app cache dir for the duration cache: {e}");
                Durations {
                    map: Mutex::new(HashMap::new()),
                    path: None,
                }
            }
        }
    }

    /// Test seam for `load`: reads the cache at `path`, tolerating a missing
    /// or unreadable file (both just mean every duration gets re-probed).
    pub fn load_from_path(path: PathBuf) -> Durations {
        let map = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<HashMap<String, f64>>(&bytes) {
                Ok(map) => map,
                Err(e) => {
                    eprintln!("tamp: duration cache is unreadable, starting fresh: {e}");
                    HashMap::new()
                }
            },
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("tamp: cannot read duration cache: {e}");
                }
                HashMap::new()
            }
        };
        Durations {
            map: Mutex::new(map),
            path: Some(path),
        }
    }

    fn get(&self, key: &str) -> Option<f64> {
        self.map.lock().unwrap().get(key).copied()
    }

    /// Records the probed durations, caps the map at `MAX_ENTRIES` (entries
    /// for the videos just listed are kept in preference to older ones), and
    /// persists the result. Best-effort: persistence failures only log.
    fn store(&self, probed: &[(String, f64)], current_keys: &HashSet<String>) {
        let mut map = self.map.lock().unwrap();
        for (key, secs) in probed {
            map.insert(key.clone(), *secs);
        }
        cap(&mut map, MAX_ENTRIES, current_keys);
        let Some(path) = &self.path else { return };
        let json = match serde_json::to_vec(&*map) {
            Ok(json) => json,
            Err(e) => {
                eprintln!("tamp: failed to serialize duration cache: {e}");
                return;
            }
        };
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("tamp: failed to create duration cache dir: {e}");
                return;
            }
        }
        if let Err(e) = std::fs::write(path, json) {
            eprintln!("tamp: failed to persist duration cache: {e}");
        }
    }
}

/// Evicts entries until `map` holds at most `max`, preferring to evict keys
/// not in `keep` (entries for videos no longer listed); beyond that the
/// eviction order is arbitrary.
fn cap(map: &mut HashMap<String, f64>, max: usize, keep: &HashSet<String>) {
    if map.len() <= max {
        return;
    }
    let mut excess = map.len() - max;
    let evictable: Vec<String> = map
        .keys()
        .filter(|k| !keep.contains(*k))
        .take(excess)
        .cloned()
        .collect();
    for key in evictable {
        map.remove(&key);
        excess -= 1;
    }
    let arbitrary: Vec<String> = map.keys().take(excess).cloned().collect();
    for key in arbitrary {
        map.remove(&key);
    }
}

/// Fills `duration_secs` for every video: cache hits immediately, misses by
/// probing with ffprobe (at most `MAX_CONCURRENT` at a time). Probe failures
/// are tolerated (the duration stays `None` and is retried next listing).
pub async fn fill(app: &AppHandle, videos: &mut [RecentVideo]) {
    let Some(state) = app.try_state::<Durations>() else {
        return;
    };

    let mut current_keys: HashSet<String> = HashSet::new();
    let mut pending: Vec<(usize, String, PathBuf)> = Vec::new();
    for (idx, video) in videos.iter_mut().enumerate() {
        let key = crate::thumbs::cache_key(&video.path, video.size_bytes);
        current_keys.insert(key.clone());
        match state.get(&key) {
            Some(secs) => video.duration_secs = Some(secs),
            None => pending.push((idx, key, PathBuf::from(&video.path))),
        }
    }
    if pending.is_empty() {
        return;
    }

    let mut probed: Vec<(String, f64)> = Vec::new();
    let apply = |videos: &mut [RecentVideo],
                 probed: &mut Vec<(String, f64)>,
                 (idx, key, secs): (usize, String, Option<f64>)| {
        if let (Some(video), Some(secs)) = (videos.get_mut(idx), secs) {
            video.duration_secs = Some(secs);
            probed.push((key, secs));
        }
    };

    let mut tasks: JoinSet<(usize, String, Option<f64>)> = JoinSet::new();
    for (idx, key, input) in pending {
        while tasks.len() >= MAX_CONCURRENT {
            match tasks.join_next().await {
                Some(Ok(done)) => apply(videos, &mut probed, done),
                Some(Err(e)) => eprintln!("tamp: duration probe task failed: {e}"),
                None => break,
            }
        }
        tasks.spawn(async move {
            let secs = crate::encoder::probe::probe(&input)
                .await
                .ok()
                .map(|info| info.duration_secs);
            (idx, key, secs)
        });
    }
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(done) => apply(videos, &mut probed, done),
            Err(e) => eprintln!("tamp: duration probe task failed: {e}"),
        }
    }

    if !probed.is_empty() {
        state.store(&probed, &current_keys);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map_of(keys: &[&str]) -> HashMap<String, f64> {
        keys.iter().map(|k| (k.to_string(), 1.0)).collect()
    }

    #[test]
    fn cap_is_a_noop_below_the_limit() {
        let mut map = map_of(&["a", "b"]);
        cap(&mut map, 2, &HashSet::new());
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn cap_prefers_evicting_keys_outside_the_current_set() {
        let mut map = map_of(&["keep1", "keep2", "old1", "old2"]);
        let keep: HashSet<String> = ["keep1", "keep2"].iter().map(|s| s.to_string()).collect();
        cap(&mut map, 2, &keep);
        assert_eq!(map.len(), 2);
        assert!(map.contains_key("keep1"));
        assert!(map.contains_key("keep2"));
    }

    #[test]
    fn cap_evicts_arbitrary_extras_when_keep_exceeds_the_limit() {
        let mut map = map_of(&["a", "b", "c", "d"]);
        let keep: HashSet<String> = map.keys().cloned().collect();
        cap(&mut map, 3, &keep);
        assert_eq!(map.len(), 3);
    }

    #[test]
    fn load_persist_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("durations.json");

        let cache = Durations::load_from_path(path.clone());
        assert_eq!(cache.get("k1"), None);
        cache.store(&[("k1".to_string(), 42.5)], &HashSet::new());

        let reloaded = Durations::load_from_path(path);
        assert_eq!(reloaded.get("k1"), Some(42.5));
    }

    #[test]
    fn unreadable_cache_starts_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("durations.json");
        std::fs::write(&path, b"{not json").unwrap();
        let cache = Durations::load_from_path(path);
        assert_eq!(cache.get("anything"), None);
    }

    #[test]
    fn store_caps_at_max_entries() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Durations::load_from_path(dir.path().join("durations.json"));
        let many: Vec<(String, f64)> = (0..MAX_ENTRIES + 7)
            .map(|i| (format!("k{i}"), i as f64))
            .collect();
        cache.store(&many, &HashSet::new());
        assert_eq!(cache.map.lock().unwrap().len(), MAX_ENTRIES);
    }
}
