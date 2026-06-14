//! Persistent log of completed conversions ({app_data_dir}/conversions.json).
//!
//! The encoder worker appends a record after every successful (non-reused)
//! encode; the recents scanner reads it back to annotate orphaned outputs
//! (outputs whose original was deleted) with the original size and preset.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::Manager;

const JOURNAL_FILE: &str = "conversions.json";
/// Newest records kept on append; older entries fall off so the file stays small.
const MAX_RECORDS: usize = 200;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionRecord {
    pub input_path: String,
    pub input_bytes: u64,
    pub output_path: String,
    pub output_bytes: u64,
    pub preset_hash: String,
    pub preset_name: String,
    /// The preset's byte target in MB when the encode ran; 0.0 = unknown for
    /// records written before the field existed.
    #[serde(default)]
    pub target_mb: f64,
    pub completed_at_ms: u64,
}

pub struct Journal {
    records: Mutex<Vec<ConversionRecord>>,
    /// None when the app data dir could not be resolved; the journal then
    /// works in memory only (persistence is best-effort anyway).
    path: Option<PathBuf>,
}

impl Journal {
    pub fn load(app: &tauri::AppHandle) -> Journal {
        match app.path().app_data_dir() {
            Ok(dir) => Journal::load_from_path(dir.join(JOURNAL_FILE)),
            Err(e) => {
                crate::log_warn!("cannot resolve app data dir for the conversion journal: {e}");
                Journal {
                    records: Mutex::new(Vec::new()),
                    path: None,
                }
            }
        }
    }

    /// Test seam for `load`: reads the journal at `path`, tolerating a
    /// missing file (fresh start) and a corrupt one (backed up to .bak,
    /// then fresh start).
    pub fn load_from_path(path: PathBuf) -> Journal {
        let records = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<Vec<ConversionRecord>>(&bytes) {
                Ok(records) => records,
                Err(e) => {
                    crate::log_warn!("conversion journal is unreadable, starting fresh: {e}");
                    backup_corrupt(&path);
                    Vec::new()
                }
            },
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    crate::log_warn!("cannot read conversion journal: {e}");
                }
                Vec::new()
            }
        };
        Journal {
            records: Mutex::new(records),
            path: Some(path),
        }
    }

    pub fn append(&self, rec: ConversionRecord) {
        let mut records = self.records.lock().unwrap();
        records.push(rec);
        if records.len() > MAX_RECORDS {
            let excess = records.len() - MAX_RECORDS;
            records.drain(..excess);
        }
        self.persist(&records);
    }

    /// All recorded conversions, newest first — for the Converted history view.
    pub fn records(&self) -> Vec<ConversionRecord> {
        self.records.lock().unwrap().iter().rev().cloned().collect()
    }

    pub fn find_by_output(&self, output_path: &str) -> Option<ConversionRecord> {
        self.records
            .lock()
            .unwrap()
            .iter()
            .rev() // newest record wins if a path was ever recorded twice
            .find(|r| r.output_path == output_path)
            .cloned()
    }

    fn persist(&self, records: &[ConversionRecord]) {
        let Some(path) = &self.path else { return };
        let json = match serde_json::to_vec(records) {
            Ok(json) => json,
            Err(e) => {
                crate::log_warn!("failed to serialize conversion journal: {e}");
                return;
            }
        };
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                crate::log_warn!("failed to create conversion journal dir: {e}");
                return;
            }
        }
        if let Err(e) = std::fs::write(path, json) {
            crate::log_warn!("failed to persist conversion journal: {e}");
        }
    }
}

/// Best-effort backup of an unreadable journal so the next append does not
/// destroy whatever data is still in the file.
fn backup_corrupt(path: &Path) {
    let backup = path.with_extension("json.bak");
    if let Err(e) = std::fs::copy(path, &backup) {
        crate::log_warn!(
            "failed to back up corrupt conversion journal to {}: {e}",
            backup.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(output_path: &str, completed_at_ms: u64) -> ConversionRecord {
        ConversionRecord {
            input_path: "/in/clip.mov".to_string(),
            input_bytes: 1_000_000,
            output_path: output_path.to_string(),
            output_bytes: 100_000,
            preset_hash: "823f".to_string(),
            preset_name: "Discord (10MB)".to_string(),
            target_mb: 10.0,
            completed_at_ms,
        }
    }

    #[test]
    fn missing_file_starts_empty() {
        let dir = tempfile::tempdir().unwrap();
        let journal = Journal::load_from_path(dir.path().join("conversions.json"));
        assert!(journal
            .find_by_output("/in/clip (tamped 823f).mp4")
            .is_none());
    }

    #[test]
    fn corrupt_file_is_backed_up_and_starts_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conversions.json");
        std::fs::write(&path, b"{not json").unwrap();

        let journal = Journal::load_from_path(path.clone());
        assert!(journal.find_by_output("/anything").is_none());
        assert_eq!(
            std::fs::read(dir.path().join("conversions.json.bak")).unwrap(),
            b"{not json"
        );

        // The journal stays usable: appends persist over the corrupt file.
        journal.append(record("/out/a.mp4", 1));
        let reloaded = Journal::load_from_path(path);
        assert!(reloaded.find_by_output("/out/a.mp4").is_some());
    }

    #[test]
    fn append_persists_and_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conversions.json");

        let journal = Journal::load_from_path(path.clone());
        journal.append(record("/out/clip (tamped 823f).mp4", 42));

        let reloaded = Journal::load_from_path(path);
        let found = reloaded
            .find_by_output("/out/clip (tamped 823f).mp4")
            .expect("record must survive a reload");
        assert_eq!(found.input_path, "/in/clip.mov");
        assert_eq!(found.input_bytes, 1_000_000);
        assert_eq!(found.output_bytes, 100_000);
        assert_eq!(found.preset_hash, "823f");
        assert_eq!(found.preset_name, "Discord (10MB)");
        assert_eq!(found.target_mb, 10.0);
        assert_eq!(found.completed_at_ms, 42);
    }

    #[test]
    fn records_serialize_camel_case() {
        let json = serde_json::to_value(record("/out/a.mp4", 7)).unwrap();
        for key in [
            "inputPath",
            "inputBytes",
            "outputPath",
            "outputBytes",
            "presetHash",
            "presetName",
            "targetMb",
            "completedAtMs",
        ] {
            assert!(json.get(key).is_some(), "missing key {key}");
        }
    }

    #[test]
    fn records_without_target_mb_deserialize_as_unknown() {
        // Journals written before targetMb existed must still load; the
        // missing field reads back as the 0.0 "unknown" sentinel.
        let json = r#"{
            "inputPath": "/in/clip.mov",
            "inputBytes": 1000000,
            "outputPath": "/out/clip (tamped 823f).mp4",
            "outputBytes": 100000,
            "presetHash": "823f",
            "presetName": "Discord (10MB)",
            "completedAtMs": 42
        }"#;
        let rec: ConversionRecord = serde_json::from_str(json).unwrap();
        assert_eq!(rec.target_mb, 0.0);
        assert_eq!(rec.completed_at_ms, 42);
    }

    #[test]
    fn append_caps_at_newest_200() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conversions.json");
        let journal = Journal::load_from_path(path.clone());
        for i in 0..205u64 {
            journal.append(record(&format!("/out/{i}.mp4"), i));
        }
        // Oldest five fell off; the newest 200 remain.
        assert!(journal.find_by_output("/out/4.mp4").is_none());
        assert!(journal.find_by_output("/out/5.mp4").is_some());
        assert!(journal.find_by_output("/out/204.mp4").is_some());

        let reloaded = Journal::load_from_path(path);
        assert!(reloaded.find_by_output("/out/4.mp4").is_none());
        assert!(reloaded.find_by_output("/out/204.mp4").is_some());
    }

    #[test]
    fn find_by_output_prefers_newest_record() {
        let dir = tempfile::tempdir().unwrap();
        let journal = Journal::load_from_path(dir.path().join("conversions.json"));
        journal.append(record("/out/a.mp4", 1));
        journal.append(record("/out/a.mp4", 2));
        assert_eq!(
            journal
                .find_by_output("/out/a.mp4")
                .unwrap()
                .completed_at_ms,
            2
        );
    }
}
