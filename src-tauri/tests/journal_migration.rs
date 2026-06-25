//! Integration coverage for the one-time journal migration, driven entirely
//! through `tamp_lib::journal::Journal`'s public API (`load_from_path`,
//! `records`, `find_by_output`) — the same entry point the app uses at launch.
//!
//! The per-fn unit tests in `journal.rs` exercise `migrate`/`dedup` directly;
//! this file proves the end-to-end flow: a real legacy per-part
//! `conversions.json` on disk is loaded, the parts merge into one multi-output
//! record, duplicates are deduped, the standalone single is preserved, the file
//! is rewritten exactly once, and a second load is idempotent (no rewrite, byte
//! stable).

use std::path::PathBuf;

use tamp_lib::journal::Journal;

/// A legacy per-part journal in the OLD `outputPath`/`outputBytes` shape:
///  - three bare parts (clip 1/2/3) sharing one `(tamped …)` folder — a split,
///  - the SAME `clip 2` part recorded a second time (a re-run) — a duplicate
///    part inside the group that must collapse,
///  - a standalone single (`other (tamped …).mp4`) whose own name carries the
///    suffix — its own record, never merged into the folder group.
fn legacy_per_part_json() -> &'static str {
    r#"[
        {"inputPath":"/in/clip.mov","inputBytes":3000000,
         "outputPath":"/out/clip (tamped 823f)/clip 2.mp4","outputBytes":110000,
         "presetHash":"823f","presetName":"Discord (10MB)","targetMb":10.0,
         "completedAtMs":50,"inputCreatedMs":7},
        {"inputPath":"/in/clip.mov","inputBytes":3000000,
         "outputPath":"/out/clip (tamped 823f)/clip 1.mp4","outputBytes":100000,
         "presetHash":"823f","presetName":"Discord (10MB)","targetMb":10.0,
         "completedAtMs":40,"inputCreatedMs":7},
        {"inputPath":"/in/clip.mov","inputBytes":3000000,
         "outputPath":"/out/clip (tamped 823f)/clip 3.mp4","outputBytes":120000,
         "presetHash":"823f","presetName":"Discord (10MB)","targetMb":10.0,
         "completedAtMs":55,"inputCreatedMs":7},
        {"inputPath":"/in/clip.mov","inputBytes":3000000,
         "outputPath":"/out/clip (tamped 823f)/clip 2.mp4","outputBytes":111000,
         "presetHash":"823f","presetName":"Discord (10MB)","targetMb":10.0,
         "completedAtMs":90,"inputCreatedMs":7},
        {"inputPath":"/in/other.mov","inputBytes":500000,
         "outputPath":"/out/other (tamped 823f).mp4","outputBytes":90000,
         "presetHash":"823f","presetName":"Discord (10MB)","targetMb":10.0,
         "completedAtMs":60,"inputCreatedMs":3}
    ]"#
}

fn write_journal(json: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("conversions.json");
    std::fs::write(&path, json).unwrap();
    (dir, path)
}

#[test]
fn legacy_per_part_journal_migrates_merges_and_dedups() {
    let (_dir, path) = write_journal(legacy_per_part_json());

    let journal = Journal::load_from_path(path.clone());
    let recs = journal.records();

    // Two records survive: the merged split + the standalone single. The
    // duplicate `clip 2` collapsed into the group rather than producing a
    // third record.
    assert_eq!(
        recs.len(),
        2,
        "the three parts (+ the re-run dup) merge to one record; the single stays"
    );

    // The split: one record carrying all three unique parts, sorted
    // numeric-aware (1, 2, 3), each part listed once.
    let split = journal
        .find_by_output("/out/clip (tamped 823f)/clip 1.mp4")
        .expect("the bare parts must merge into one record");
    assert_eq!(split.outputs.len(), 3, "three unique parts, dup collapsed");
    assert_eq!(split.outputs[0].path, "/out/clip (tamped 823f)/clip 1.mp4");
    assert_eq!(split.outputs[1].path, "/out/clip (tamped 823f)/clip 2.mp4");
    assert_eq!(split.outputs[2].path, "/out/clip (tamped 823f)/clip 3.mp4");
    // The re-run of clip 2 wins on bytes (newest) without doubling the row.
    assert_eq!(split.outputs[1].bytes, 111000, "newest dup bytes win");
    // completedAtMs is the max across every part record (incl. the re-run).
    assert_eq!(split.completed_at_ms, 90);
    assert_eq!(split.input_path, "/in/clip.mov");
    assert_eq!(split.input_bytes, 3_000_000);
    assert_eq!(split.input_created_ms, 7);

    // Any part path resolves to the same merged record.
    let by_third = journal
        .find_by_output("/out/clip (tamped 823f)/clip 3.mp4")
        .expect("the third part must resolve to the merged record");
    assert_eq!(by_third.completed_at_ms, split.completed_at_ms);
    assert_eq!(by_third.outputs.len(), 3);

    // The standalone single is preserved as its own 1-output record, untouched.
    let single = journal
        .find_by_output("/out/other (tamped 823f).mp4")
        .expect("the standalone single must survive migration");
    assert_eq!(single.outputs.len(), 1);
    assert_eq!(single.input_path, "/in/other.mov");
    assert_eq!(single.completed_at_ms, 60);
}

#[test]
fn migration_rewrites_once_then_reloads_are_idempotent() {
    let (dir, path) = write_journal(legacy_per_part_json());
    let original = std::fs::read(&path).unwrap();

    // First load: the legacy records changed, so the migration rewrites the
    // file once into the new `outputs[]` shape (bytes differ from the legacy
    // input) and leaves no staging temp behind.
    let _journal = Journal::load_from_path(path.clone());
    let after_first = std::fs::read(&path).unwrap();
    assert_ne!(
        original, after_first,
        "the legacy journal must be rewritten into the new shape exactly once"
    );
    assert!(
        !dir.path().join("conversions.json.tmp").exists(),
        "the atomic rewrite must not leave a .json.tmp file"
    );

    // Second load of the now-migrated file: nothing legacy remains, so the
    // file is NOT rewritten — the bytes are byte-for-byte stable.
    let reloaded = Journal::load_from_path(path.clone());
    let after_second = std::fs::read(&path).unwrap();
    assert_eq!(
        after_first, after_second,
        "a second load must be idempotent: no rewrite, bytes stable"
    );

    // The migrated content is identical across the two loads.
    let first_records = _journal.records();
    let second_records = reloaded.records();
    assert_eq!(first_records.len(), second_records.len());
    assert_eq!(second_records.len(), 2);
    for (a, b) in first_records.iter().zip(second_records.iter()) {
        assert_eq!(a.input_path, b.input_path);
        assert_eq!(a.completed_at_ms, b.completed_at_ms);
        assert_eq!(a.outputs.len(), b.outputs.len());
        for (oa, ob) in a.outputs.iter().zip(b.outputs.iter()) {
            assert_eq!(oa.path, ob.path);
            assert_eq!(oa.bytes, ob.bytes);
        }
    }
}

#[test]
fn migrated_journal_loads_as_new_format_records() {
    // After migration the on-disk file is already in the new `outputs[]` shape;
    // round-tripping it through a fresh `Journal` yields the same records — the
    // proof that the rewritten file is itself a valid new-format journal.
    let (dir, path) = write_journal(legacy_per_part_json());
    let _migrated = Journal::load_from_path(path.clone());

    let new_format_json = std::fs::read_to_string(&path).unwrap();
    let (dir2, path2) = write_journal(&new_format_json);
    let before = std::fs::read(&path2).unwrap();
    let journal = Journal::load_from_path(path2.clone());
    let after = std::fs::read(&path2).unwrap();
    assert_eq!(
        before, after,
        "a journal already in the new shape must not be rewritten"
    );
    assert!(!dir2.path().join("conversions.json.tmp").exists());
    assert_eq!(journal.records().len(), 2);
    drop((dir, dir2));
}
