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

// keep in lockstep with src/lib/convgroup.ts (parentDir/basename/isPartPath)
//
// The "(tamped …)" heuristic below mirrors the TS regexes exactly so that the
// one-time migration groups legacy per-part journals the same way the old
// frontend did. After migration the frontend reads structure straight from
// `outputs[]`; this stays only as the migration mirror.

/// Slice before the last '\\' or '/'; "" if there is no separator.
fn parent_dir(p: &str) -> &str {
    match p.rfind(['\\', '/']) {
        Some(i) => &p[..i],
        None => "",
    }
}

/// The path component after the last '\\' or '/' (the whole string if neither).
fn basename(p: &str) -> &str {
    match p.rfind(['\\', '/']) {
        Some(i) => &p[i + 1..],
        None => p,
    }
}

/// Mirrors TS `TAMPED_DIR = /\(tamped .+\)$/` on a directory's own name: the
/// name ends in ")" and has "(tamped " somewhere before it with ≥1 char in
/// between (so "(tamped )" alone does not match).
fn is_tamped_dir(name: &str) -> bool {
    if !name.ends_with(')') {
        return false;
    }
    match name.rfind("(tamped ") {
        // The chars between the marker and the trailing ')' must be ≥1, i.e.
        // the marker's end index is strictly before the final ')'.
        Some(i) => i + "(tamped ".len() < name.len() - 1,
        None => false,
    }
}

/// Mirrors TS `TAMPED_FILE = /\(tamped [^)]+\)\.[^.]+$/` on a file's basename:
/// the name ends in `(tamped <no-')' chars>).<no-'.' chars>` — a standalone
/// re-compressed output that carries the "(tamped …)" suffix in its OWN name.
fn is_tamped_file(name: &str) -> bool {
    // The extension: chars after the last '.', non-empty and dot-free by
    // construction (everything after the final '.').
    let Some(dot) = name.rfind('.') else {
        return false;
    };
    let ext = &name[dot + 1..];
    if ext.is_empty() {
        return false;
    }
    // The stem must end in `(tamped <≥1 non-')'>)`.
    let stem = &name[..dot];
    if !stem.ends_with(')') {
        return false;
    }
    let Some(marker) = stem.rfind("(tamped ") else {
        return false;
    };
    // The chars between "(tamped " and the closing ')' must be ≥1 and contain
    // no ')' (mirrors `[^)]+`).
    let inner = &stem[marker + "(tamped ".len()..stem.len() - 1];
    !inner.is_empty() && !inner.contains(')')
}

/// A split *part* is a bare part file inside a "(tamped …)" output folder whose
/// OWN name is not itself a "(tamped …)" file. Mirrors TS `isPartPath`.
fn is_part_path(output_path: &str) -> bool {
    is_tamped_dir(basename(parent_dir(output_path))) && !is_tamped_file(basename(output_path))
}

/// The source file's creation time (ms since epoch); 0 when it can't be read
/// (the source was moved/deleted, or the FS has no creation time). Mirrors
/// `encoder::input_created_ms` / `commands::file_created_ms`.
fn disk_created_ms(input_path: &str) -> u64 {
    std::fs::metadata(input_path)
        .and_then(|m| m.created().or_else(|_| m.modified()))
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A natural-sort key for an output path: split into runs of digits and
/// non-digits so numeric runs compare by value, mirroring TS
/// `localeCompare(.., { numeric: true })` for the part ordering we care about.
fn numeric_key(path: &str) -> Vec<NaturalChunk> {
    let mut chunks = Vec::new();
    let mut chars = path.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            let mut digits = String::new();
            while let Some(&d) = chars.peek() {
                if d.is_ascii_digit() {
                    digits.push(d);
                    chars.next();
                } else {
                    break;
                }
            }
            // Compare numerically; fall back to string length for runs that
            // overflow u128 (paths never realistically hit this).
            match digits.parse::<u128>() {
                Ok(n) => chunks.push(NaturalChunk::Num(n)),
                Err(_) => chunks.push(NaturalChunk::Text(digits)),
            }
        } else {
            let mut text = String::new();
            while let Some(&t) = chars.peek() {
                if t.is_ascii_digit() {
                    break;
                }
                text.push(t);
                chars.next();
            }
            chunks.push(NaturalChunk::Text(text));
        }
    }
    chunks
}

#[derive(PartialEq, Eq)]
enum NaturalChunk {
    Text(String),
    Num(u128),
}

impl PartialOrd for NaturalChunk {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NaturalChunk {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        use NaturalChunk::*;
        match (self, other) {
            (Num(a), Num(b)) => a.cmp(b),
            (Text(a), Text(b)) => a.cmp(b),
            // A numeric run sorts before a text run, matching the digit-first
            // ordering of the part filenames we group.
            (Num(_), Text(_)) => Ordering::Less,
            (Text(_), Num(_)) => Ordering::Greater,
        }
    }
}

/// One-time migration: new-format records pass through; legacy split parts
/// (single-output records whose path `is_part_path`) merge by parent folder
/// into one N-output record; legacy non-part records stay 1-output. For any
/// record with `input_created_ms == 0`, backfill the source-created time from
/// disk ONCE (the value is frozen thereafter). Returns the migrated records and
/// `changed = true` if any legacy record existed (so the caller rewrites once).
fn migrate(mapped: Vec<(ConversionRecord, bool)>) -> (Vec<ConversionRecord>, bool) {
    let changed = mapped.iter().any(|(_, is_legacy)| *is_legacy);

    let mut out: Vec<ConversionRecord> = Vec::new();
    // Folder -> index into `out` for the group record being accumulated, so the
    // merged record keeps the position of its first part (chronological order).
    let mut group_index: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for (rec, is_legacy) in mapped {
        let is_split_part =
            is_legacy && rec.outputs.len() == 1 && is_part_path(&rec.outputs[0].path);
        if !is_split_part {
            out.push(rec);
            continue;
        }

        let folder = parent_dir(&rec.outputs[0].path).to_string();
        match group_index.get(&folder).copied() {
            Some(idx) => {
                let group = &mut out[idx];
                let new_out = rec.outputs.into_iter().next().unwrap();
                // The old per-part journal could re-record the same part path on
                // a re-run of the split; collapse it (newest bytes win) instead
                // of listing the part twice.
                match group.outputs.iter_mut().find(|o| o.path == new_out.path) {
                    Some(existing) => existing.bytes = new_out.bytes,
                    None => group.outputs.push(new_out),
                }
                group.completed_at_ms = group.completed_at_ms.max(rec.completed_at_ms);
                if group.input_created_ms == 0 {
                    group.input_created_ms = rec.input_created_ms;
                }
            }
            None => {
                let idx = out.len();
                group_index.insert(folder, idx);
                out.push(rec);
            }
        }
    }

    // Sort each merged group's parts numeric-aware (mirrors the TS part sort)
    // and backfill any still-zero created time from disk once.
    for rec in &mut out {
        if rec.outputs.len() > 1 {
            rec.outputs.sort_by_key(|o| numeric_key(&o.path));
        }
        if rec.input_created_ms == 0 {
            rec.input_created_ms = disk_created_ms(&rec.input_path);
        }
    }

    // Collapse whole records that share an identical output-path set — legacy
    // duplicates from re-running the same conversion (e.g. a single output
    // recorded twice). Newest by completed_at_ms wins. Mirrors append()'s dedup,
    // applied once across the migrated batch.
    let before_len = out.len();
    let out = dedup_by_output_set(out);
    let changed = changed || out.len() != before_len;

    (out, changed)
}

/// Drop records whose output-path set duplicates another's, keeping the newest
/// by `completed_at_ms`. The kept record stays in its (chronological) position.
fn dedup_by_output_set(records: Vec<ConversionRecord>) -> Vec<ConversionRecord> {
    use std::collections::{BTreeSet, HashMap};
    // Output-path set -> index of the newest record carrying it.
    let mut winner: HashMap<BTreeSet<String>, usize> = HashMap::new();
    let mut keep = vec![true; records.len()];
    for (i, rec) in records.iter().enumerate() {
        let set: BTreeSet<String> = rec.outputs.iter().map(|o| o.path.clone()).collect();
        match winner.get(&set).copied() {
            Some(j) if rec.completed_at_ms >= records[j].completed_at_ms => {
                keep[j] = false;
                winner.insert(set, i);
            }
            Some(_) => keep[i] = false,
            None => {
                winner.insert(set, i);
            }
        }
    }
    records
        .into_iter()
        .zip(keep)
        .filter_map(|(r, k)| k.then_some(r))
        .collect()
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
        let (records, changed) = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<Vec<RawRecord>>(&bytes) {
                Ok(raw) => {
                    let mapped = raw.into_iter().map(RawRecord::into_record).collect();
                    migrate(mapped)
                }
                Err(e) => {
                    crate::log_warn!("conversion journal is unreadable, starting fresh: {e}");
                    backup_corrupt(&path);
                    (Vec::new(), false)
                }
            },
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    crate::log_warn!("cannot read conversion journal: {e}");
                }
                (Vec::new(), false)
            }
        };
        let journal = Journal {
            records: Mutex::new(records),
            path: Some(path),
        };
        // Rewrite once if the migration changed anything (legacy records merged
        // and/or created times backfilled), so the migration is one-time and
        // the stored created time is frozen thereafter.
        if changed {
            let records = journal.records.lock().unwrap();
            journal.persist(&records);
        }
        journal
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
        migrate(raw.into_iter().map(RawRecord::into_record).collect()).0
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

    #[test]
    fn is_part_path_matches_bare_parts_only() {
        // A bare-numbered part inside a (tamped …) folder is a split part.
        assert!(is_part_path("/out/clip (tamped Discord 823f)/clip 2.mp4"));
        assert!(is_part_path(r"C:\out\clip (tamped 823f)\clip 10.mp4"));
        // A standalone single (its OWN name is "(tamped …)") is NOT a part.
        assert!(!is_part_path("/out/clip (tamped Discord 823f).mp4"));
        // A re-compressed file inside a tamped folder carries the suffix in its
        // own name → its own conversion, not a part.
        assert!(!is_part_path(
            "/out/clip (tamped 823f)/clip 2 (tamped 9ca1).mp4"
        ));
        // Not inside a tamped folder at all.
        assert!(!is_part_path("/out/plain/clip 2.mp4"));
    }

    #[test]
    fn legacy_split_parts_merge_into_one_record() {
        // Two legacy part records in the same (tamped …) folder plus one
        // standalone single. After migration: 2 records — the split collapsed
        // to one 2-output record (sorted, completedAtMs = max), the single
        // untouched.
        let json = r#"[
            {"inputPath":"/in/clip.mov","inputBytes":2000000,
             "outputPath":"/out/clip (tamped 823f)/clip 2.mp4","outputBytes":110000,
             "presetHash":"823f","presetName":"Discord (10MB)","targetMb":10.0,
             "completedAtMs":50,"inputCreatedMs":7},
            {"inputPath":"/in/clip.mov","inputBytes":2000000,
             "outputPath":"/out/clip (tamped 823f)/clip 1.mp4","outputBytes":100000,
             "presetHash":"823f","presetName":"Discord (10MB)","targetMb":10.0,
             "completedAtMs":40,"inputCreatedMs":7},
            {"inputPath":"/in/other.mov","inputBytes":500000,
             "outputPath":"/out/other (tamped 823f).mp4","outputBytes":90000,
             "presetHash":"823f","presetName":"Discord (10MB)","targetMb":10.0,
             "completedAtMs":60,"inputCreatedMs":3}
        ]"#;
        let recs = load_records(json);
        assert_eq!(recs.len(), 2);

        let split = recs
            .iter()
            .find(|r| r.outputs.len() == 2)
            .expect("the two parts merge into one 2-output record");
        // Outputs sorted numeric-aware: part 1 before part 2.
        assert_eq!(split.outputs[0].path, "/out/clip (tamped 823f)/clip 1.mp4");
        assert_eq!(split.outputs[1].path, "/out/clip (tamped 823f)/clip 2.mp4");
        // completedAtMs = max over the parts; other fields from the first part.
        assert_eq!(split.completed_at_ms, 50);
        assert_eq!(split.input_path, "/in/clip.mov");
        assert_eq!(split.input_bytes, 2_000_000);
        assert_eq!(split.input_created_ms, 7);

        let single = recs
            .iter()
            .find(|r| r.outputs.len() == 1)
            .expect("the standalone single stays a single record");
        assert_eq!(single.outputs[0].path, "/out/other (tamped 823f).mp4");
        assert_eq!(single.completed_at_ms, 60);
    }

    #[test]
    fn recompressed_part_stays_separate() {
        // A legacy (tamped …) FILE that happens to live inside a tamped folder
        // (re-compressing a part) is its OWN single record, not merged into the
        // folder's split. Here both the bare part and the re-compressed file
        // sit in the same folder.
        let json = r#"[
            {"inputPath":"/in/clip.mov","inputBytes":2000000,
             "outputPath":"/out/clip (tamped 823f)/clip 1.mp4","outputBytes":100000,
             "presetHash":"823f","presetName":"Discord (10MB)","targetMb":10.0,
             "completedAtMs":40,"inputCreatedMs":7},
            {"inputPath":"/out/clip (tamped 823f)/clip 1.mp4","inputBytes":100000,
             "outputPath":"/out/clip (tamped 823f)/clip 1 (tamped 9ca1).mp4","outputBytes":50000,
             "presetHash":"9ca1","presetName":"Discord (10MB)","targetMb":10.0,
             "completedAtMs":70,"inputCreatedMs":7}
        ]"#;
        let recs = load_records(json);
        assert_eq!(recs.len(), 2, "the re-compressed file is its own record");
        for rec in &recs {
            assert_eq!(rec.outputs.len(), 1);
        }
        assert!(recs
            .iter()
            .any(|r| r.outputs[0].path == "/out/clip (tamped 823f)/clip 1 (tamped 9ca1).mp4"));
    }

    #[test]
    fn new_format_records_pass_through() {
        // A journal already in outputs[] form: unchanged, and NOT rewritten.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conversions.json");
        let json = r#"[{
            "inputPath":"/in/clip.mov","inputBytes":2000000,
            "outputs":[
                {"path":"/out/clip (tamped 823f)/clip 1.mp4","bytes":100000},
                {"path":"/out/clip (tamped 823f)/clip 2.mp4","bytes":110000}
            ],
            "presetHash":"823f","presetName":"Discord (10MB)","targetMb":10.0,
            "completedAtMs":50,"inputCreatedMs":7
        }]"#;
        std::fs::write(&path, json).unwrap();
        let before = std::fs::read(&path).unwrap();

        // migrate reports no change for a purely new-format journal.
        let raw: Vec<RawRecord> = serde_json::from_slice(&before).unwrap();
        let (recs, changed) = migrate(raw.into_iter().map(RawRecord::into_record).collect());
        assert!(!changed, "a new-format journal must not be marked changed");
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].outputs.len(), 2);

        // load_from_path must NOT rewrite the file (bytes identical).
        let _journal = Journal::load_from_path(path.clone());
        let after = std::fs::read(&path).unwrap();
        assert_eq!(before, after, "new-format journal must not be rewritten");
        assert!(!dir.path().join("conversions.json.tmp").exists());
    }

    #[test]
    fn created_time_backfilled_once() {
        // A legacy record with inputCreatedMs:0 whose input file exists on disk
        // gets a non-zero created time after load (backfilled once).
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("source.mov");
        std::fs::write(&input, b"video bytes").unwrap();
        let input_str = input.to_string_lossy().replace('\\', "\\\\");

        let path = dir.path().join("conversions.json");
        let json = format!(
            r#"[{{
                "inputPath":"{input_str}","inputBytes":11,
                "outputPath":"/out/source (tamped 823f).mp4","outputBytes":5,
                "presetHash":"823f","presetName":"Discord (10MB)","targetMb":10.0,
                "completedAtMs":50,"inputCreatedMs":0
            }}]"#
        );
        std::fs::write(&path, json).unwrap();

        let journal = Journal::load_from_path(path.clone());
        let rec = journal
            .find_by_output("/out/source (tamped 823f).mp4")
            .expect("record must load");
        assert!(
            rec.input_created_ms > 0,
            "created time must be backfilled from the on-disk source"
        );

        // It was frozen: the rewritten file carries the non-zero value, and a
        // reload reads it back without touching disk again.
        let reloaded = Journal::load_from_path(path);
        let rec2 = reloaded
            .find_by_output("/out/source (tamped 823f).mp4")
            .unwrap();
        assert_eq!(rec2.input_created_ms, rec.input_created_ms);
    }

    #[test]
    fn migration_dedups_duplicate_parts_in_a_group() {
        // The old per-part journal could append the same part path twice (a
        // re-run of the same split). Migration lists each part once, with the
        // newest bytes, not a doubled part row.
        let json = r#"[
            {"inputPath":"/in/clip.mov","inputBytes":2000000,
             "outputPath":"/out/clip (tamped 823f)/clip 1.mp4","outputBytes":100000,
             "presetHash":"823f","presetName":"Discord (10MB)","targetMb":10.0,
             "completedAtMs":40,"inputCreatedMs":7},
            {"inputPath":"/in/clip.mov","inputBytes":2000000,
             "outputPath":"/out/clip (tamped 823f)/clip 2.mp4","outputBytes":110000,
             "presetHash":"823f","presetName":"Discord (10MB)","targetMb":10.0,
             "completedAtMs":45,"inputCreatedMs":7},
            {"inputPath":"/in/clip.mov","inputBytes":2000000,
             "outputPath":"/out/clip (tamped 823f)/clip 1.mp4","outputBytes":101000,
             "presetHash":"823f","presetName":"Discord (10MB)","targetMb":10.0,
             "completedAtMs":90,"inputCreatedMs":7}
        ]"#;
        let recs = load_records(json);
        assert_eq!(recs.len(), 1);
        let g = &recs[0];
        assert_eq!(
            g.outputs.len(),
            2,
            "the duplicate part 1 collapses; two unique parts remain"
        );
        assert_eq!(g.outputs[0].path, "/out/clip (tamped 823f)/clip 1.mp4");
        assert_eq!(
            g.outputs[0].bytes, 101_000,
            "newest bytes for the re-run part"
        );
        assert_eq!(g.outputs[1].path, "/out/clip (tamped 823f)/clip 2.mp4");
        assert_eq!(g.completed_at_ms, 90);
    }

    #[test]
    fn migration_dedups_duplicate_singles() {
        // Two legacy single records for the SAME output path (re-running the
        // same single conversion) collapse to one record, newest wins.
        let json = r#"[
            {"inputPath":"/in/a.mov","inputBytes":1000,
             "outputPath":"/out/a (tamped 823f).mp4","outputBytes":500,
             "presetHash":"823f","presetName":"Discord (10MB)","targetMb":10.0,
             "completedAtMs":10,"inputCreatedMs":0},
            {"inputPath":"/in/a.mov","inputBytes":1000,
             "outputPath":"/out/a (tamped 823f).mp4","outputBytes":480,
             "presetHash":"823f","presetName":"Discord (10MB)","targetMb":10.0,
             "completedAtMs":20,"inputCreatedMs":0}
        ]"#;
        let recs = load_records(json);
        assert_eq!(
            recs.len(),
            1,
            "duplicate single output paths collapse to one"
        );
        assert_eq!(recs[0].completed_at_ms, 20, "newest record wins");
        assert_eq!(recs[0].outputs[0].bytes, 480);
    }
}
