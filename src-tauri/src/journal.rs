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

/// One delivered output of a conversion job. Singles have exactly one; a
/// split set embeds one per part.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    pub path: String,
    pub bytes: u64,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionRecord {
    pub input_path: String,
    pub input_bytes: u64,
    /// The job's delivered outputs: one for a single, N for a split set.
    pub outputs: Vec<Output>,
    pub preset_hash: String,
    pub preset_name: String,
    /// The preset's byte target in MB when the encode ran; 0.0 = unknown for
    /// records written before the field existed.
    #[serde(default)]
    pub target_mb: f64,
    pub completed_at_ms: u64,
    /// The source file's creation time (ms since epoch); 0 when unknown
    /// (older records, or the time couldn't be read).
    #[serde(default)]
    pub input_created_ms: u64,
}

/// On-disk record shape used only for loading. Accepts BOTH the new
/// (`outputs`) and the legacy (`outputPath`/`outputBytes`) layouts so old
/// journals keep loading. Mapping to `ConversionRecord` happens in
/// `load_from_path`.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRecord {
    input_path: String,
    input_bytes: u64,
    #[serde(default)]
    outputs: Option<Vec<Output>>,
    #[serde(default)]
    output_path: Option<String>,
    #[serde(default)]
    output_bytes: Option<u64>,
    preset_hash: String,
    preset_name: String,
    #[serde(default)]
    target_mb: f64,
    completed_at_ms: u64,
    #[serde(default)]
    input_created_ms: u64,
}

impl RawRecord {
    /// Map a raw record to `(record, is_legacy)`. New-format records (those
    /// carrying `outputs`) pass through; legacy ones collapse their single
    /// `outputPath`/`outputBytes` into a one-element `outputs` vec.
    fn into_record(self) -> (ConversionRecord, bool) {
        let (outputs, is_legacy) = match self.outputs {
            Some(outputs) => (outputs, false),
            None => (
                vec![Output {
                    path: self.output_path.unwrap_or_default(),
                    bytes: self.output_bytes.unwrap_or(0),
                }],
                true,
            ),
        };
        (
            ConversionRecord {
                input_path: self.input_path,
                input_bytes: self.input_bytes,
                outputs,
                preset_hash: self.preset_hash,
                preset_name: self.preset_name,
                target_mb: self.target_mb,
                completed_at_ms: self.completed_at_ms,
                input_created_ms: self.input_created_ms,
            },
            is_legacy,
        )
    }
}

/// One-time migration hook. Identity for now (Task 1); Task 2 fills it in to
/// merge legacy split parts and backfill the source-created time.
fn migrate(mapped: Vec<(ConversionRecord, bool)>) -> Vec<ConversionRecord> {
    mapped.into_iter().map(|(rec, _is_legacy)| rec).collect()
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
            Ok(bytes) => match serde_json::from_slice::<Vec<RawRecord>>(&bytes) {
                Ok(raw) => {
                    let mapped = raw.into_iter().map(RawRecord::into_record).collect();
                    migrate(mapped)
                }
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
        // Dedup by output-path SET (order-independent): re-recording the same
        // output(s) replaces the prior record (newest wins) rather than
        // duplicating. Distinct conversions (different paths) coexist.
        let key: std::collections::BTreeSet<&str> =
            rec.outputs.iter().map(|o| o.path.as_str()).collect();
        records.retain(|r| {
            let k: std::collections::BTreeSet<&str> =
                r.outputs.iter().map(|o| o.path.as_str()).collect();
            k != key
        });
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
            .find(|r| r.outputs.iter().any(|o| o.path == output_path))
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
        // Atomic write: stage to a sibling temp file, then rename over the
        // target (std::fs::rename replaces atomically on Windows), so a crash
        // mid-write can never leave a half-written journal.
        let tmp = path.with_extension("json.tmp");
        if let Err(e) = std::fs::write(&tmp, &json) {
            crate::log_warn!("failed to stage conversion journal: {e}");
            return;
        }
        if let Err(e) = std::fs::rename(&tmp, path) {
            crate::log_warn!("failed to persist conversion journal: {e}");
            let _ = std::fs::remove_file(&tmp);
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
            outputs: vec![Output {
                path: output_path.to_string(),
                bytes: 100_000,
            }],
            preset_hash: "823f".to_string(),
            preset_name: "Discord (10MB)".to_string(),
            target_mb: 10.0,
            completed_at_ms,
            input_created_ms: 0,
        }
    }

    /// Load a journal directly from a JSON string (legacy or new shape) via the
    /// same `RawRecord` mapping `load_from_path` uses.
    fn load_records(json: &str) -> Vec<ConversionRecord> {
        let raw: Vec<RawRecord> = serde_json::from_str(json).unwrap();
        migrate(raw.into_iter().map(RawRecord::into_record).collect())
    }

    #[test]
    fn input_created_ms_defaults_to_zero_when_absent() {
        // Legacy shape (outputPath/outputBytes) must still load as a 1-output
        // record, with the absent inputCreatedMs reading back as 0.
        let json = r#"[{"inputPath":"/i","inputBytes":1,"outputPath":"/o","outputBytes":1,"presetHash":"h","presetName":"p","completedAtMs":2}]"#;
        let recs = load_records(json);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].input_created_ms, 0);
        assert_eq!(recs[0].outputs.len(), 1);
        assert_eq!(recs[0].outputs[0].path, "/o");
        assert_eq!(recs[0].outputs[0].bytes, 1);
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
        assert_eq!(found.outputs.len(), 1);
        assert_eq!(found.outputs[0].path, "/out/clip (tamped 823f).mp4");
        assert_eq!(found.outputs[0].bytes, 100_000);
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
            "outputs",
            "presetHash",
            "presetName",
            "targetMb",
            "completedAtMs",
            "inputCreatedMs",
        ] {
            assert!(json.get(key).is_some(), "missing key {key}");
        }
        // The nested Output also serializes camelCase (path/bytes).
        let first = &json["outputs"][0];
        assert_eq!(first["path"], "/out/a.mp4");
        assert_eq!(first["bytes"], 100_000);
    }

    #[test]
    fn records_without_target_mb_deserialize_as_unknown() {
        // Legacy journals written before targetMb existed (and in the old
        // outputPath/outputBytes shape) must still load; the missing field
        // reads back as the 0.0 "unknown" sentinel and the single legacy
        // output collapses into a 1-element outputs vec.
        let json = r#"[{
            "inputPath": "/in/clip.mov",
            "inputBytes": 1000000,
            "outputPath": "/out/clip (tamped 823f).mp4",
            "outputBytes": 100000,
            "presetHash": "823f",
            "presetName": "Discord (10MB)",
            "completedAtMs": 42
        }]"#;
        let recs = load_records(json);
        assert_eq!(recs.len(), 1);
        let rec = &recs[0];
        assert_eq!(rec.target_mb, 0.0);
        assert_eq!(rec.completed_at_ms, 42);
        assert_eq!(rec.outputs.len(), 1);
        assert_eq!(rec.outputs[0].path, "/out/clip (tamped 823f).mp4");
        assert_eq!(rec.outputs[0].bytes, 100_000);
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

    #[test]
    fn dedup_replaces_same_output_set() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conversions.json");
        let journal = Journal::load_from_path(path.clone());

        // Two records with the same single output: the second replaces the
        // first rather than duplicating.
        journal.append(record("/out/a.mp4", 1));
        journal.append(record("/out/a.mp4", 2));

        let reloaded = Journal::load_from_path(path);
        let all = reloaded.records();
        assert_eq!(all.len(), 1, "same-output-set append must replace, not add");
        assert_eq!(all[0].completed_at_ms, 2, "newest record wins");
    }

    #[test]
    fn atomic_persist_leaves_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conversions.json");
        let journal = Journal::load_from_path(path.clone());
        journal.append(record("/out/a.mp4", 1));

        // The persisted file parses back cleanly...
        let bytes = std::fs::read(&path).unwrap();
        let raw: Vec<RawRecord> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(raw.len(), 1);

        // ...and no staging temp file is left behind.
        assert!(
            !dir.path().join("conversions.json.tmp").exists(),
            "atomic write must not leave a .json.tmp file"
        );
    }

    #[test]
    fn find_by_output_matches_any_part() {
        let dir = tempfile::tempdir().unwrap();
        let journal = Journal::load_from_path(dir.path().join("conversions.json"));

        // A split set: one record with two part outputs.
        journal.append(ConversionRecord {
            input_path: "/in/clip.mov".to_string(),
            input_bytes: 2_000_000,
            outputs: vec![
                Output {
                    path: "/out/clip (tamped 823f)/clip 1.mp4".to_string(),
                    bytes: 100_000,
                },
                Output {
                    path: "/out/clip (tamped 823f)/clip 2.mp4".to_string(),
                    bytes: 110_000,
                },
            ],
            preset_hash: "823f".to_string(),
            preset_name: "Discord (10MB)".to_string(),
            target_mb: 10.0,
            completed_at_ms: 5,
            input_created_ms: 0,
        });

        // Either part path finds the same one record.
        let by_first = journal
            .find_by_output("/out/clip (tamped 823f)/clip 1.mp4")
            .expect("first part must match");
        let by_second = journal
            .find_by_output("/out/clip (tamped 823f)/clip 2.mp4")
            .expect("second part must match");
        assert_eq!(by_first.outputs.len(), 2);
        assert_eq!(by_second.outputs.len(), 2);
        assert_eq!(by_first.completed_at_ms, by_second.completed_at_ms);
    }
}
