# Converted-tab Tree + Enhancements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Group multi-part (split) conversions in the Converted tab into an expandable tree, add a thumbnail + ▶ play button per conversion, a hover-time tooltip (Recorded vs Converted), and make the Videos-tab recents count 50 and configurable.

**Architecture:** A split job writes one journal record per part, all sharing a `(tamped …)` output folder. The frontend groups flat `ConversionRecord[]` into single rows + multi-part groups (parent collapses to one folder-style row; expands into per-part child rows). New backend bits: a `recents_limit` setting, an `inputCreatedMs` journal field, and `open_file` + `conversion_thumb` commands.

**Tech Stack:** Rust (Tauri 2), TypeScript/Vite (vitest). Bun. ARM64 Windows.

**Branch:** `converted-tree` (off `release/0.3.0`, already created).

**Mockup (target):** `docs/mockups/converted-tree/` in the `design-mockups` branch — see `6-enhanced-tree.html` / `screenshots/tree-6-enhanced.png`.

---

## Conventions
- Rust: `cd src-tauri`; `%USERPROFILE%\.cargo\bin` on PATH; `cargo test`, `cargo fmt`, `cargo clippy --all-targets -- -D warnings`.
- Frontend (repo root): `C:\Program Files\nodejs` FIRST on PATH; `bunx tsc --noEmit`, `bun run test`.
- Some steps say "read file X and adapt" — do read it and match the existing helpers/patterns rather than inventing names. Reviewers verify against the real code.
- No servers (`bun tauri dev`, `cargo run`). Build/test only. One commit per task.

---

## Task 1: Configurable Videos-tab recents limit

**Files:** `src-tauri/src/settings.rs`, `src-tauri/src/commands.rs`, `src/lib/ipc.ts`

- [ ] **Step 1 — failing test** (settings.rs test module): a `Settings` deserialized from `{}` has `recents_limit == 50`; `validate` rejects 0 and > 200.

```rust
#[test]
fn recents_limit_defaults_to_50_and_is_bounded() {
    let s: crate::settings::Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(s.recents_limit, 50);
}
```
(Add a `validate` range check + a test that `validate` errors for `recents_limit = 0` and `= 201`, matching how `validate` is written in settings.rs — read it first.)

- [ ] **Step 2 — run** `cd src-tauri && cargo test recents_limit` → FAIL (field missing).

- [ ] **Step 3 — implement.** In `settings.rs`: add to `Settings` (after `context_menu_enabled`):

```rust
    /// How many recent videos the Videos tab lists. 1..=200.
    #[serde(default = "default_recents_limit")]
    pub recents_limit: usize,
```
Add `fn default_recents_limit() -> usize { 50 }`, set `recents_limit: 50` in `default_settings`, and in `validate` add a bound check (`if !(1..=200).contains(&s.recents_limit) { return Err(...) }`) following the existing validation style.

In `commands.rs` `list_recents`: replace the `RECENTS_LIMIT` constant use with the setting — read `recents_limit` from `SettingsState` (alongside `watched_folders`) and pass it to `scanner::scan(&folders, limit)`. Remove the now-unused `const RECENTS_LIMIT` if nothing else uses it (grep first).

In `src/lib/ipc.ts` add to `Settings`: `recentsLimit: number;`.

- [ ] **Step 4 — run** `cargo test recents_limit && cargo clippy --all-targets -- -D warnings` (clean) and `bunx tsc --noEmit` (clean).

- [ ] **Step 5 — commit** `feat: configurable Videos-tab recents limit (default 50)`

---

## Task 2: Journal records the input's recorded time

**Files:** `src-tauri/src/journal.rs`, `src-tauri/src/encoder/mod.rs`, `src/lib/ipc.ts`

- [ ] **Step 1 — failing test** (journal.rs tests): extend the `record(...)` test helper to set `input_created_ms` and assert it serializes as `inputCreatedMs` and that a JSON missing it deserializes to `0`.

```rust
#[test]
fn input_created_ms_defaults_to_zero_when_absent() {
    let json = r#"{"inputPath":"/i","inputBytes":1,"outputPath":"/o","outputBytes":1,"presetHash":"h","presetName":"p","completedAtMs":2}"#;
    let rec: ConversionRecord = serde_json::from_str(json).unwrap();
    assert_eq!(rec.input_created_ms, 0);
}
```
(Also add `"inputCreatedMs"` to the `records_serialize_camel_case` key list.)

- [ ] **Step 2 — run** `cargo test journal` → FAIL.

- [ ] **Step 3 — implement.** In `journal.rs` `ConversionRecord` add (after `completed_at_ms` is fine; serde is name-based):

```rust
    /// The source file's creation time (ms since epoch); 0 when unknown
    /// (older records, or the time couldn't be read).
    #[serde(default)]
    pub input_created_ms: u64,
```
Update the test helper `record(...)` to set `input_created_ms: 0`.

In `encoder/mod.rs` `append_journal` (the fn that builds the `ConversionRecord` — read it): compute the input's created time and set the field. Add a helper near it:

```rust
fn input_created_ms(input: &std::path::Path) -> u64 {
    std::fs::metadata(input)
        .and_then(|m| m.created().or_else(|_| m.modified()))
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
```
and set `input_created_ms: input_created_ms(&job.input)` (use the real input-path field name from the job struct) when constructing the record.

In `src/lib/ipc.ts` `ConversionRecord` add `inputCreatedMs: number;`.

- [ ] **Step 4 — run** `cargo test journal && cargo clippy --all-targets -- -D warnings` (clean); `bunx tsc --noEmit`.

- [ ] **Step 5 — commit** `feat: journal the source recorded time for Converted-tab tooltips`

---

## Task 3: `open_file` + `conversion_thumb` commands

**Files:** `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`, `src/lib/ipc.ts`

- [ ] **Step 1 — implement `open_file`.** In `commands.rs` (mirror the existing `reveal` command which uses `OpenerExt`):

```rust
/// Opens `path` in the OS default application (used by the Converted tab's
/// ▶ play button to preview an output).
#[tauri::command]
pub fn open_file(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt as _;
    app.opener()
        .open_path(&path, None::<&str>)
        .map_err(|e| format!("couldn't open {path}: {e}"))
}
```

- [ ] **Step 2 — implement `conversion_thumb`.** Read `thumbs.rs` to find how a thumbnail is generated for one file (the per-file generation behind `ensure_thumbs`). Add a command that ensures+returns a cached thumbnail path for a single video `path`, reusing that generator. Shape:

```rust
/// Ensures (generating on miss) a thumbnail for a single video and returns its
/// cached path, or None on failure. Backs the Converted tab's per-row preview.
#[tauri::command]
pub async fn conversion_thumb(app: AppHandle, path: String) -> Option<String> {
    // Reuse the same single-frame thumbnail generation ensure_thumbs uses.
    // (Read thumbs.rs; call its per-file helper, or refactor one out.)
    crate::thumbs::ensure_one(&app, std::path::Path::new(&path)).await
}
```
If `ensure_thumbs` has no reusable per-file helper, refactor a `pub async fn ensure_one(app, path) -> Option<String>` out of it and have `ensure_thumbs` call it per video (keep its behavior identical; the existing thumbs tests must still pass).

- [ ] **Step 3 — register** both commands in `lib.rs` `generate_handler!` (after `pick_videos`).

- [ ] **Step 4 — ipc bindings** in `src/lib/ipc.ts`:

```ts
export const openFile = (path: string): Promise<void> =>
  invoke<void>("open_file", { path });
export const conversionThumb = (path: string): Promise<string | null> =>
  invoke<string | null>("conversion_thumb", { path });
```

- [ ] **Step 5 — run** `cargo test && cargo clippy --all-targets -- -D warnings` (clean, existing thumbs tests still pass); `bunx tsc --noEmit`.

- [ ] **Step 6 — commit** `feat: open_file + conversion_thumb commands for the Converted tab`

---

## Task 4: Grouping logic (pure) + tests

**Files:** Create `src/lib/convgroup.ts`, `src/lib/convgroup.test.ts`

- [ ] **Step 1 — failing test** (`convgroup.test.ts`):

```ts
import { describe, expect, it } from "vitest";
import { groupConversions } from "./convgroup";
import type { ConversionRecord } from "./ipc";

const rec = (outputPath: string, bytes = 1000, completedAtMs = 1): ConversionRecord => ({
  inputPath: "C:\\v\\Long meeting.mp4", inputBytes: 9000, outputPath, outputBytes: bytes,
  presetHash: "h", presetName: "Slack (25MB)", targetMb: 25, completedAtMs, inputCreatedMs: 0,
});

describe("groupConversions", () => {
  it("keeps a single output as a flat node", () => {
    const out = groupConversions([rec("C:\\v\\Long meeting (tamped Slack 25MB h).mp4")]);
    expect(out).toHaveLength(1);
    expect(out[0].kind).toBe("single");
  });
  it("groups parts that share a (tamped …) folder", () => {
    const out = groupConversions([
      rec("C:\\v\\Long meeting (tamped Slack 25MB h)\\Long meeting 1.mp4", 100, 3),
      rec("C:\\v\\Long meeting (tamped Slack 25MB h)\\Long meeting 2.mp4", 200, 5),
    ]);
    expect(out).toHaveLength(1);
    expect(out[0].kind).toBe("group");
    if (out[0].kind === "group") {
      expect(out[0].parts).toHaveLength(2);
      expect(out[0].totalBytes).toBe(300);
      expect(out[0].completedAtMs).toBe(5); // newest part
    }
  });
  it("orders nodes newest-first by completion", () => {
    const out = groupConversions([
      rec("C:\\v\\a (tamped X)\\a 1.mp4", 1, 10),
      rec("C:\\v\\b (tamped X).mp4", 1, 20),
    ]);
    expect(out[0].completedAtMs).toBe(20);
  });
});
```

- [ ] **Step 2 — run** `bun run test convgroup` → FAIL (module missing).

- [ ] **Step 3 — implement `src/lib/convgroup.ts`:**

```ts
import type { ConversionRecord } from "./ipc";

export type ConvNode =
  | { kind: "single"; rec: ConversionRecord; completedAtMs: number }
  | {
      kind: "group"; folder: string; inputPath: string; inputBytes: number;
      presetName: string; inputCreatedMs: number; completedAtMs: number;
      totalBytes: number; parts: ConversionRecord[];
    };

function parentDir(p: string): string {
  const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  return i < 0 ? "" : p.slice(0, i);
}
const TAMPED = /\(tamped .+\)$/;
/** A split part lives in a "(tamped …)" output folder; a single output sits
 *  directly in its source folder. */
export function isPartPath(outputPath: string): boolean {
  return TAMPED.test(parentDir(outputPath));
}

/** Flat journal records → singles + multi-part groups, newest-first. */
export function groupConversions(records: ConversionRecord[]): ConvNode[] {
  const groups = new Map<string, ConversionRecord[]>();
  const nodes: ConvNode[] = [];
  for (const r of records) {
    if (isPartPath(r.outputPath)) {
      const key = parentDir(r.outputPath);
      (groups.get(key) ?? groups.set(key, []).get(key)!).push(r);
    } else {
      nodes.push({ kind: "single", rec: r, completedAtMs: r.completedAtMs });
    }
  }
  for (const [folder, parts] of groups) {
    parts.sort((a, b) => a.outputPath.localeCompare(b.outputPath, undefined, { numeric: true }));
    const completedAtMs = Math.max(...parts.map((p) => p.completedAtMs));
    nodes.push({
      kind: "group", folder, inputPath: parts[0].inputPath, inputBytes: parts[0].inputBytes,
      presetName: parts[0].presetName, inputCreatedMs: parts[0].inputCreatedMs,
      completedAtMs, totalBytes: parts.reduce((s, p) => s + p.outputBytes, 0), parts,
    });
  }
  return nodes.sort((a, b) => b.completedAtMs - a.completedAtMs);
}
```

- [ ] **Step 4 — run** `bun run test convgroup` → PASS; `bunx tsc --noEmit`.

- [ ] **Step 5 — commit** `feat: group multi-part conversions for the Converted tab`

---

## Task 5: Absolute-time formatter + tests

**Files:** `src/lib/format.ts`, `src/lib/format.test.ts`

- [ ] **Step 1 — failing test** (`format.test.ts`): `formatAbsolute(ms)` returns a stable `"Mon DD, YYYY · HH:MM"` string for a fixed epoch (assert it contains the year and a `·`). Use a fixed UTC ms and assert substrings to avoid TZ flakiness:

```ts
import { formatAbsolute } from "./format";
it("formatAbsolute shows a date and time", () => {
  const s = formatAbsolute(1_700_000_000_000);
  expect(s).toMatch(/\d{4}/);   // a year
  expect(s).toContain("·");
});
it("formatAbsolute returns em dash for unknown (0)", () => {
  expect(formatAbsolute(0)).toBe("—");
});
```

- [ ] **Step 2 — run** `bun run test format` → FAIL.

- [ ] **Step 3 — implement** in `format.ts`:

```ts
/** "Jun 12, 2026 · 23:41" for a tooltip; em dash when the time is unknown (0). */
export function formatAbsolute(ms: number): string {
  if (!ms) return "—";
  const d = new Date(ms);
  const date = d.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
  const time = d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", hour12: false });
  return `${date} · ${time}`;
}
```

- [ ] **Step 4 — run** `bun run test format` → PASS.

- [ ] **Step 5 — commit** `feat: absolute-time formatter for the Converted-tab tooltip`

---

## Task 6: Converted tab — tree rendering, thumbnails, play, tooltip

**Files:** `src/views/converted.ts`, `src/styles.css`

Rebuild `createConvertedView`'s rendering on top of `groupConversions`. Read the current `converted.ts` first and preserve its public shape (`{ el, refresh }`) and the copy/reveal SVG buttons.

- [ ] **Step 1 — implement rendering.** `refresh()` calls `listConversions()`, then `groupConversions(records)`, then renders each node:
  - **single** → a row: lazy thumbnail (`conversionThumb(rec.outputPath)` → `convertFileSrc`), name = basename(inputPath), sub = `before → after · presetName · <time>`, and action buttons **▶ play** (`openFile(rec.outputPath)`), **copy** (`copyFile(rec.outputPath)`), **reveal** (`reveal(rec.outputPath)`).
  - **group** → a collapsible parent: thumbnail, a chevron, name = basename(inputPath), sub = `inputBytes → totalBytes · "<n> parts" badge · presetName · <time>`, and parent buttons **copy all** (`copyFile` on each part) + **open folder** (`reveal(node.folder)`). Collapsed by default; clicking the parent toggles a `.children` container of per-part rows (part number, basename(part.outputPath), `formatBytes(part.outputBytes)`, ▶/copy/reveal on each part).
  - `<time>` is a `.conv-time` element (see Step 2) carrying the hover tooltip.
  - Keep the existing empty-state message.

- [ ] **Step 2 — hover-time tooltip.** Make the time element show a tooltip with two rows on hover. Build it as a child element revealed on hover (the `title` attribute can't carry two styled rows):

```ts
function timeEl(recordedMs: number, convertedMs: number): HTMLElement {
  const wrap = document.createElement("span");
  wrap.className = "conv-time";
  wrap.textContent = formatRelativeTime(convertedMs);
  const tip = document.createElement("span");
  tip.className = "conv-tip";
  tip.innerHTML =
    `<span class="conv-tip-row"><b class="rec">Recorded</b><span></span></span>` +
    `<span class="conv-tip-row"><b class="conv">Converted</b><span></span></span>`;
  const vals = tip.querySelectorAll("span span");
  (vals[0] as HTMLElement).textContent = formatAbsolute(recordedMs);
  (vals[1] as HTMLElement).textContent = formatAbsolute(convertedMs);
  wrap.appendChild(tip);
  return wrap;
}
```
Import `formatAbsolute` and `formatRelativeTime` from `../lib/format`, `openFile`/`conversionThumb` from `../lib/ipc`, and `groupConversions`/types from `../lib/convgroup`.

- [ ] **Step 3 — styles.** Append to `src/styles.css` (reuse the existing theme vars; the Converted styles already exist — add the new bits): `.conv-thumb` (≈50×32, `object-fit:cover`, `background:var(--surface-2)`), `.conv-play` (accent), `.conv-tree-parent`/`.conv-children`/`.conv-part`/`.conv-partno` with an indent + a 1px `var(--border)` connector, `.badge-parts` (accent-deep pill), and `.conv-time { position:relative }` + `.conv-tip` (absolute popover, hidden, shown on `.conv-time:hover`; two rows; `--overlay` bg, `--shadow`, with `.rec`→`var(--amber)` and `.conv`→`var(--success)` labels). The drawer/Converted already theme via tokens — match that so it works in light/dark.

- [ ] **Step 4 — refresh-on-done.** `main.ts` already refreshes the Converted view on a finished encode — leave that wiring; just confirm `refresh()` still works after the rewrite (it re-groups every call).

- [ ] **Step 5 — run** `bunx tsc --noEmit && bun run test` (clean; all prior tests pass).

- [ ] **Step 6 — commit** `feat: Converted tab tree view with thumbnails, play, and a recorded/converted tooltip`

---

## Task 7: Preferences — recents-limit control

**Files:** `src/views/preferences.ts`

- [ ] **Step 1 — implement.** Read `preferences.ts` and reuse its existing control factory (the same one the other settings use — e.g. a number/stepper or the toggle factory pattern). Add a control labelled "Recent videos shown" bound to `settings.recentsLimit` (range 1–200) that persists via the same path the other prefs use (`saveSettings`/the view's onSettings). Clamp out-of-range input and surface errors with `showToast`, mirroring the context-menu toggle's revert pattern.

- [ ] **Step 2 — run** `bunx tsc --noEmit && bun run test` (clean).

- [ ] **Step 3 — commit** `feat: Preferences control for the recents count`

---

## Task 8: Changeset + final verification

- [ ] **Step 1 — changeset** `.changeset/converted-tree.md`:

```markdown
---
"tamp": minor
---

The Converted tab now groups a multi-part (split) conversion into one
expandable row — a folder-style parent that drills into its parts — instead of
listing each part separately. Every conversion gets a thumbnail (timestamp-named
recordings are easy to tell apart), a ▶ play button that opens the output in the
default player, and a hover tooltip on the time showing when the source was
recorded vs when it was converted. The Videos tab now lists 50 recent videos by
default and the count is configurable in Preferences.
```

- [ ] **Step 2 — full verification:** `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`; then root `bunx tsc --noEmit && bun run test`. All green.

- [ ] **Step 3 — commit** `chore: changeset for the Converted-tab tree + enhancements`

---

## Self-review notes
- **Coverage:** tree grouping (T4), play (T3/T6), thumbnail (T3/T6), recorded-vs-converted tooltip (T2/T5/T6), recents 50 + configurable (T1/T7). 
- **Type consistency:** `groupConversions`/`ConvNode`/`isPartPath` (T4) consumed in T6; `inputCreatedMs` added in T2, used in T6; `formatAbsolute` (T5) used in T6; `openFile`/`conversionThumb` (T3) used in T6; `recentsLimit` (T1) used in T7.
- **Watch-outs:** read `thumbs.rs` before T3 (refactor `ensure_one` only if no per-file helper exists; keep existing thumbs tests green); read `preferences.ts` before T7 and reuse its real control factory; read `encoder/mod.rs` `append_journal` before T2 for the exact input-path field name.
