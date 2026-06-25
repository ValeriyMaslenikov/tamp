# Batch 3 — History rework (one record per conversion)

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development / executing-plans. Checkbox (`- [ ]`) steps.

**Goal:** Make the conversion journal store **one logical record per conversion job** (single = 1 output, split = N outputs embedded), so the 200-record cap, dedup, and the Converted-tab grouping are *exact* instead of inferred from `(tamped …)` filenames. Add atomic journal writes, dedup on append, and freeze the source-created time (captured at encode, backfilled once on migration, never re-read live). Migrate existing per-part journals into the new shape on first load, losing no history.

**Decision (user):** Storage rework + migrate (not logical-only). The brittle filename heuristic stops being load-bearing.

**Branch:** `converted-tree`. **Touches Rust backend → the dev app must be stopped before building** (it locks `target/debug/tamp.exe`).

**Conventions:**
- Rust (`cd src-tauri`): `%USERPROFILE%\.cargo\bin` on PATH; `cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`.
- Frontend (repo root): `C:\Program Files\nodejs` FIRST on PATH; `bunx tsc --noEmit`, `bun run test`.
- No servers. One commit per task. Read the cited code and adapt to the real structure.

**Authoritative naming (from `encoder/plan.rs`):**
- Single output: `{stem} (tamped {name} {hash}).{ext}` next to the input (a file).
- Split: a folder `{stem} (tamped {name} {hash})/` containing parts `{stem} {i}.{ext}` (i = 1..=n). Parts are bare-numbered files inside a `(tamped …)` folder. A re-compressed part is itself a `(tamped …)` file inside that folder → its OWN conversion (matches `convgroup.ts isPartPath`).

**New model:**
```rust
pub struct Output { pub path: String, pub bytes: u64 }
pub struct ConversionRecord {
    pub input_path: String,
    pub input_bytes: u64,
    pub outputs: Vec<Output>,   // 1 for singles, N for splits
    pub preset_hash: String,
    pub preset_name: String,
    pub target_mb: f64,
    pub completed_at_ms: u64,
    pub input_created_ms: u64,
}
```
TS mirror (`src/lib/ipc.ts`): `outputs: { path: string; bytes: number }[]` replaces `outputPath`/`outputBytes`.

---

## Task 1: New journal schema — outputs[], atomic writes, dedup, find across outputs

**Files:** `src-tauri/src/journal.rs`

**Read first:** the whole file — the current `ConversionRecord`, `load_from_path`, `append`, `records`, `find_by_output`, `persist`, `backup_corrupt`, and every test (they encode the on-disk contract and must be updated).

- [ ] **Step 1 — schema.** Add `#[derive(Clone, Serialize, Deserialize)] #[serde(rename_all="camelCase")] pub struct Output { pub path: String, pub bytes: u64 }`. Replace `output_path: String` + `output_bytes: u64` on `ConversionRecord` with `pub outputs: Vec<Output>`. Keep all other fields (`target_mb`/`input_created_ms` keep their `#[serde(default)]`).

- [ ] **Step 2 — backward-compatible load.** Add a private `RawRecord` (serde, camelCase) that accepts BOTH shapes:
```rust
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRecord {
    input_path: String,
    input_bytes: u64,
    #[serde(default)] outputs: Option<Vec<Output>>,
    #[serde(default)] output_path: Option<String>,
    #[serde(default)] output_bytes: Option<u64>,
    preset_hash: String,
    preset_name: String,
    #[serde(default)] target_mb: f64,
    completed_at_ms: u64,
    #[serde(default)] input_created_ms: u64,
}
```
In `load_from_path`, deserialize `Vec<RawRecord>`. Map each to a `(ConversionRecord, is_legacy: bool)`: if `outputs` is `Some`, it's already new (`is_legacy=false`); else build `outputs = vec![Output{ path: output_path.unwrap_or_default(), bytes: output_bytes.unwrap_or(0) }]` (`is_legacy=true`). Keep the corrupt-file → `.bak` + empty behavior. **The legacy part-MERGE is Task 2** — for Task 1, legacy records become 1-output records 1:1 (a `migrate(records) -> Vec<ConversionRecord>` hook you’ll fill in Task 2; for now it is the identity on the mapped records). Do NOT rewrite the file yet in Task 1.

- [ ] **Step 3 — dedup on append.** In `append`, before pushing, remove any existing record whose output-path SET equals the new record's (same outputs, order-independent) so re-recording the same output(s) replaces rather than duplicates. Distinct conversions (different output paths) coexist (immutable history preserved). Then push, then cap to newest `MAX_RECORDS` (=200; now logical since 1 record = 1 job), then `persist`.
```rust
let key: std::collections::BTreeSet<&str> = rec.outputs.iter().map(|o| o.path.as_str()).collect();
records.retain(|r| {
    let k: std::collections::BTreeSet<&str> = r.outputs.iter().map(|o| o.path.as_str()).collect();
    k != key
});
```

- [ ] **Step 4 — find_by_output across outputs.** Change `find_by_output` to return the newest record where ANY `output.path == output_path`:
```rust
pub fn find_by_output(&self, output_path: &str) -> Option<ConversionRecord> {
    self.records.lock().unwrap().iter().rev()
        .find(|r| r.outputs.iter().any(|o| o.path == output_path))
        .cloned()
}
```

- [ ] **Step 5 — atomic persist.** Rewrite `persist` to write to a sibling temp file then rename over the target (Rust `std::fs::rename` replaces on Windows):
```rust
let tmp = path.with_extension("json.tmp");
if let Err(e) = std::fs::write(&tmp, &json) { crate::log_warn!("…"); return; }
if let Err(e) = std::fs::rename(&tmp, path) { crate::log_warn!("…"); let _ = std::fs::remove_file(&tmp); }
```
Keep `create_dir_all(parent)` first. Keep `backup_corrupt` as-is.

- [ ] **Step 6 — update tests.** Update every existing test to the new shape: the `record(...)` helper builds `outputs: vec![Output{ path: output_path.into(), bytes: 100_000 }]`; `records_serialize_camel_case` checks `outputs` (and that nested `path`/`bytes` serialize) instead of `outputPath`/`outputBytes`; `records_without_target_mb_deserialize_as_unknown` and `input_created_ms_defaults_to_zero_when_absent` feed LEGACY JSON (`outputPath`/`outputBytes`) and assert it loads as a 1-output record. Add tests: (a) `dedup_replaces_same_output_set` — append two records with the same single output, only one remains (newest); (b) `atomic_persist_leaves_valid_file` — after append, the file parses back and no `.json.tmp` remains; (c) `find_by_output_matches_any_part` — a 2-output record is found by either part path.

- [ ] **Step 7 — verify.** `cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test journal` (and full `cargo test` if quick) — clean.

- [ ] **Step 8 — commit** `git add src-tauri/src/journal.rs && git commit -m "feat(journal): one record per conversion (outputs[]), atomic writes, output-set dedup, find across parts"`

---

## Task 2: One-time migration — merge legacy split parts + backfill & freeze created time

**Files:** `src-tauri/src/journal.rs`

**Read first:** `src/lib/convgroup.ts` `isPartPath`/`parentDir`/`basename` (the heuristic to mirror) and Task 1's `load_from_path` mapping + the `migrate` hook.

- [ ] **Step 1 — port the part heuristic to Rust.** Add private helpers mirroring `convgroup.ts` exactly:
```rust
fn parent_dir(p: &str) -> &str { /* slice before the last '\\' or '/'; "" if none */ }
fn basename(p: &str) -> &str { /* after the last '\\' or '/' */ }
// TAMPED_DIR: parent dir name ends in "(tamped …)"
// TAMPED_FILE: file is itself "(tamped …).ext"
fn is_part_path(output_path: &str) -> bool { /* parent is a (tamped …) dir AND basename is NOT a (tamped …) file */ }
```
Use the same regexes as TS: dir `\(tamped .+\)$` on the parent's own name; file `\(tamped [^)]+\)\.[^.]+$` on the basename. (Add the `regex` crate only if it's already a dep; otherwise hand-roll with `rfind`/`ends_with`/`contains` — prefer no new dep.) Mirror `convgroup.ts` and keep a `// keep in lockstep with src/lib/convgroup.ts` comment on both sides.

- [ ] **Step 2 — merge migration.** Implement `migrate(mapped: Vec<(ConversionRecord, bool /*is_legacy*/)>) -> (Vec<ConversionRecord>, bool /*changed*/)`:
  - New-format records (is_legacy=false) pass through unchanged.
  - Legacy records whose single output `is_part_path` are grouped by `parent_dir(output)`: each group becomes ONE record with `outputs` = the group's part outputs sorted by path numeric-aware (mirror `localeCompare(..,{numeric:true})` — sort by `(natural key)`), `input_path`/`input_bytes`/`preset*`/`target_mb` from the first part, `completed_at_ms` = max over parts, `input_created_ms` = first non-zero over parts.
  - Legacy non-part records become 1-output records (already mapped that way).
  - Preserve overall newest-first-by-`completed_at_ms` ordering on return (records are stored oldest-first internally; match the existing convention — the file currently stores in append order and `records()` reverses. Keep storage order = chronological; just merge, don't reorder beyond grouping).
  - `changed = true` if any legacy record existed (so the caller rewrites the file once).

- [ ] **Step 3 — backfill + freeze created time.** During migration, for any record with `input_created_ms == 0`, read the source's created time from disk ONCE via the existing helper (mirror `encoder::input_created_ms` / `file_created_ms`; if there's a shared util use it, else read `std::fs::metadata(input_path).created()` → ms, 0 on error). This is the ONLY place created-time is backfilled now — Task 4 removes the per-call backfill in `commands.rs`, so the stored value is frozen thereafter.

- [ ] **Step 4 — rewrite once.** In `load_from_path`, after `migrate`, if `changed`, `persist` the migrated records immediately (atomic write from Task 1) so the migration is one-time. Guard: only when `path` is `Some`.

- [ ] **Step 5 — tests.** Add: (a) `legacy_split_parts_merge_into_one_record` — three legacy records, two of them parts in the same `(tamped …)` folder + one standalone single → loads as 2 records, the split one having 2 sorted outputs, `completed_at_ms` = max; (b) `recompressed_part_stays_separate` — a legacy `(tamped …)` FILE inside a tamped folder is NOT merged (its own single record); (c) `new_format_records_pass_through` — a journal already in `outputs[]` form is unchanged and NOT rewritten (`changed=false`); (d) `created_time_backfilled_once` — a legacy record with `inputCreatedMs:0` whose input file exists on disk gets a non-zero created time after load. Use `tempfile` + real files for (d).

- [ ] **Step 6 — verify.** `cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test journal` — clean. Leave a `// manual:` note: an existing real `conversions.json` from a prior build loads, old splits collapse to single tree rows, and the file is rewritten once.

- [ ] **Step 7 — commit** `git add src-tauri/src/journal.rs && git commit -m "feat(journal): one-time migration — merge legacy split parts into one record, backfill+freeze source-created time"`

---

## Task 3: Encoder writes one record per job

**Files:** `src-tauri/src/encoder/mod.rs`

**Read first:** `append_journal` (~1320-1356), its call sites in `run_single` (~572) and `run_split_set` (~871, inside the per-part loop), and the `find_by_output` reuse gate `is_journal_clean` (~979-990). Trace how `run_split_set` iterates/produces each part's output path and final bytes.

- [ ] **Step 1 — append signature.** Change `append_journal` to take the full output set: replace the single `output: &Path, … actual: u64` tail with `outputs: &[crate::journal::Output]` (or `&[(PathBuf,u64)]` mapped inside). It builds ONE `ConversionRecord { outputs: outputs.to_vec(), … }`. Keep the `try_state` best-effort guard and `input_created_ms(&job.input)`.

- [ ] **Step 2 — run_single.** Call `append_journal(inner, job, &[Output{ path: plan.output.to_string_lossy().into(), bytes: actual }], preset_hash)` once (unchanged timing — after delivery).

- [ ] **Step 3 — run_split_set.** Remove the per-part `append_journal` call inside the loop. Accumulate each delivered part as `Output{ path, bytes }` into a `Vec` as the loop produces them, and after the loop (once the whole set is delivered Done) call `append_journal` ONCE with all parts. Preserve the existing cancel/cleanup behavior from Batch 1 (a cancel before delivery still skips the append). Make sure the append happens only on full success of the set.

- [ ] **Step 4 — callers of find_by_output.** Confirm `is_journal_clean` (single reuse gate) still compiles and is correct: a single's record now has `outputs=[that path]`, so `find_by_output(expected)` still matches and the field checks (`input_bytes` etc.) are unchanged. No behavior change intended.

- [ ] **Step 5 — adapt tests.** Update any encoder test that constructs/【reads a `ConversionRecord` or asserts journal contents to the new `outputs[]` shape. Keep coverage of "single appends one record" and add/extend "a split appends exactly one record with N outputs" if a test harness reaches `run_split_set`; otherwise leave a `// manual:` note for the split-append integration check.

- [ ] **Step 6 — verify.** `cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` — clean, all pass.

- [ ] **Step 7 — commit** `git add src-tauri/src/encoder/mod.rs && git commit -m "feat(encoder): append exactly one journal record per job (single = 1 output, split = N outputs)"`

---

## Task 4: Frontend model + drop the live created-time backfill

**Files:** `src/lib/ipc.ts`, `src/lib/convgroup.ts`, `src/lib/convgroup.test.ts`, `src/views/converted.ts`, `src/views/converted.test.ts`, `src-tauri/src/commands.rs`

**Read first:** `convgroup.ts` (grouping + `ConvNode`), `converted.ts` (how it renders single vs group, the play/reveal/copy actions and which path they use), `convgroup.test.ts`, `converted.test.ts`, and `commands.rs` `list_conversions` (the per-call disk backfill to remove) + the recents annotation at `commands.rs:36` (`find_by_output`).

- [ ] **Step 1 — ipc.ts.** Replace `outputPath: string; outputBytes: number;` on `ConversionRecord` with `outputs: { path: string; bytes: number }[];`.

- [ ] **Step 2 — convgroup.ts.** Grouping now reads structure from the record, not the filename:
  - `record.outputs.length > 1` → a `group` node: `folder = parentDir(outputs[0].path)`, `parts` derived from `outputs` (each part carries its `path`/`bytes`), `totalBytes = sum(outputs.bytes)`, `inputPath`/`presetName`/`inputCreatedMs`/`completedAtMs` from the record.
  - `record.outputs.length === 1` → a `single` node using `outputs[0]`.
  - DELETE `isPartPath`/`TAMPED_DIR`/`TAMPED_FILE` and the folder-keyed grouping map. Keep `parentDir`/`basename` only if still used. Keep newest-first sort by `completedAtMs`.
  - Adjust the `ConvNode`/`group.parts` types so `converted.ts` still gets per-part `{ path, bytes }` (+ whatever it renders). If `parts` was `ConversionRecord[]`, change it to the per-output shape; update `converted.ts` accordingly.

- [ ] **Step 2b — convgroup.test.ts.** Rewrite for the new shape: the `rec(...)` helper builds `outputs`. A split is ONE record with 2 outputs → one `group` node with 2 parts; a single is one record with 1 output → `single`. The "re-compressing parts makes separate records" case becomes: TWO separate single-output records (each a re-compressed file) → two `single` nodes, plus the original 2-output split record → one `group`. Assert counts/totals/`completedAtMs` as before.

- [ ] **Step 3 — converted.ts.** Update rendering/actions to the new node shape: part rows iterate the group's `outputs`/`parts` (`path`/`bytes`); single rows use `outputs[0].path`/`bytes`. Play/▶, reveal, copy act on the specific part's `path`. Keep the thumbnail/tooltip ("Created"/"Converted") behavior; `inputCreatedMs`/`completedAtMs` come from the record unchanged. Update `converted.test.ts` to the new shape.

- [ ] **Step 4 — commands.rs.** Remove the per-call `input_created_ms` disk backfill from `list_conversions` (created-time is frozen, backfilled once in Task 2). For the recents annotation at `commands.rs:36`: `find_by_output(&video.path)` now returns the whole record; set `ConversionMeta.outputBytes` to the matching output's bytes (`record.outputs.iter().find(|o| o.path == video.path).map(|o| o.bytes)`), `originalBytes` = `record.input_bytes`, `presetName` = `record.preset_name`. Keep the rest.

- [ ] **Step 5 — verify.** Frontend (root): `bunx tsc --noEmit && bun run test` clean. Rust: `cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` clean.

- [ ] **Step 6 — commit** `git add src/lib/ipc.ts src/lib/convgroup.ts src/lib/convgroup.test.ts src/views/converted.ts src/views/converted.test.ts src-tauri/src/commands.rs && git commit -m "feat(history): frontend reads outputs[] from the record; drop the filename grouping heuristic and the live created-time backfill"`

---

## Self-review notes
- One model end to end: encoder writes 1 record/job (Task 3), journal stores/dedups/caps/migrates it (Tasks 1–2), frontend reads parts straight from `outputs[]` (Task 4). The `(tamped …)` filename heuristic survives only as a one-time Rust migration mirror, then is deleted from the live frontend path.
- Invariants: no history lost on migration (legacy parts merge, singles pass through, file rewritten once); created-time frozen (encode-time capture + one migration backfill, no live re-read); atomic writes (temp+rename) so a crash mid-write can't corrupt the journal; dedup replaces same-output-set records but keeps genuinely distinct conversions (immutable history).
- Order matters: Task 1 (schema) → Task 2 (migration) → Task 3 (encoder) → Task 4 (frontend). Sequential — heavy file overlap in `journal.rs` and the Rust/TS schema coupling.
