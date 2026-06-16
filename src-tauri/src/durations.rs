//! Per-file video duration cache ({app_cache_dir}/durations.json): the
//! thumbs-style cache key (path|mtime|size hash, see `thumbs::cache_key`)
//! mapped to the duration in seconds. The Videos tab lazy-fills each row's
//! `RecentVideo::duration_secs` from here (the `recent_duration` command via
//! `ensure_one`), probing misses with ffprobe.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

const CACHE_FILE: &str = "durations.json";
/// The map keeps at most this many entries; extras are evicted on persist.
const MAX_ENTRIES: usize = 500;

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
                crate::log_warn!("cannot resolve app cache dir for the duration cache: {e}");
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
                    crate::log_warn!("duration cache is unreadable, starting fresh: {e}");
                    HashMap::new()
                }
            },
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    crate::log_warn!("cannot read duration cache: {e}");
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

    /// Inserts one probed duration and persists, without the `current_keys`
    /// eviction preference `fill` applies (a single lazy probe has no list of
    /// currently-visible videos to favor). Best-effort: failures only log.
    fn insert_one(&self, key: String, secs: f64) {
        self.store(&[(key, secs)], &HashSet::new());
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
                crate::log_warn!("failed to serialize duration cache: {e}");
                return;
            }
        };
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                crate::log_warn!("failed to create duration cache dir: {e}");
                return;
            }
        }
        if let Err(e) = std::fs::write(path, json) {
            crate::log_warn!("failed to persist duration cache: {e}");
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

/// Resolves the duration for a single video: a cache hit returns immediately, a
/// miss probes once with ffprobe and caches the result. Backs the Videos tab's
/// per-row lazy `recent_duration` command (mirrors `thumbs::ensure_one`).
/// Returns `None` when the cache state is unavailable or the probe fails (the
/// failure is not cached, so it's retried on the next request).
pub async fn ensure_one(app: &AppHandle, path: &Path) -> Option<f64> {
    let state = app.try_state::<Durations>()?;
    let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let key = crate::thumbs::cache_key(&path.to_string_lossy(), size_bytes);
    if let Some(secs) = state.get(&key) {
        return Some(secs);
    }
    match crate::encoder::probe::probe(path).await {
        Ok(info) => {
            state.insert_one(key, info.duration_secs);
            Some(info.duration_secs)
        }
        Err(e) => {
            crate::log_warn!("duration probe failed for {}: {e}", path.display());
            None
        }
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
