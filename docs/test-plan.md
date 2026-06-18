# tamp — confidence test catalog

The exhaustive set of tests that, **if all green, justify saying "tamp works as
users expect in normal situations and edge cases."** This is the bar: it lists
every meaningful scenario per subsystem, the level it should be tested at, and
its **current status**. The gap between ✅ and ❌/⚠️ is exactly the work left to
reach full confidence.

**Status legend:** ✅ automated & passing · ⚠️ partial (logic tested, real I/O
not) · ❌ not covered (manual or missing) · 🔁 manual on-device check.
**Levels:** U = frontend unit (vitest) · RU = Rust unit · RI = Rust integration
(`src-tauri/tests/`) · E2E = Playwright UI (mocked IPC) · NAT = tauri-driver
native smoke · M = manual/on-device.

Today's automated totals: **163 U · 221 RU · 19 RI · 18 E2E · NAT smoke**. The
biggest structural gaps are bolded in the **Confidence summary** at the end.

---

## 1. Folder scanning & recents

| # | Scenario | Level | Status |
|---|----------|-------|--------|
| 1.1 | Lists video files in watched folders, newest first, capped at `recentsLimit` | RU/RI | ✅ |
| 1.2 | Empty watched folder → "no recordings" empty state (not an error) | RU + E2E | ✅ |
| 1.3 | Unreachable folder (offline UNC / permission-denied) → banner, not empty state | RU + E2E | ✅ |
| 1.4 | Non-UTF-8 filename → skipped + noted, never mangled | RU | ✅ |
| 1.5 | "(tamped …)" outputs whose original is gone → annotated as orphans from the journal | RU | ⚠️ (annotation logic ✅; real scan 🔁) |
| 1.6 | Instant panel open: rows return immediately, thumbs/durations lazy-load | RU + E2E | ⚠️ (non-blocking contract ✅; visual fill 🔁) |
| 1.7 | 200-video folder: open stays responsive, no eager decode of all thumbs | — | ❌ (no perf test) |
| 1.8 | Flaky network share connect/disconnect mid-scan | — | ❌ (M) |

## 2. Conversion / encoding (the core flow)

| # | Scenario | Level | Status |
|---|----------|-------|--------|
| 2.1 | Single convert hits the target size (≤ N MB) on a real video | RI | ⚠️ (`encode_integration`; not exhaustive across inputs) |
| 2.2 | Bitrate plan: budget/audio/fps/width math, never emits a trim | RU | ✅ |
| 2.3 | Two-pass overshoot → retry at lower bitrate; gives up gracefully | RU | ✅ |
| 2.4 | Hardware encoder present → used; absent → software fallback (NVENC→QSV→AMF→MF→x264 / videotoolbox) | RU + M | ⚠️ (selection logic ✅; real HW 🔁) |
| 2.5 | webm / gif formats; gif retry params | RU | ✅ |
| 2.6 | Strip-audio preset | RU | ✅ |
| 2.7 | Reuse: identical existing output is served without re-encoding (journal-clean) | RU | ✅ |
| 2.8 | Split — smart / static-parts / static-seconds → one record, N outputs | RU + RI | ✅ |
| 2.9 | **Cancel mid-encode** → no output delivered, source intact | RU | ⚠️ (`should_deliver` ✅; real cancel 🔁) |
| 2.10 | **Cancel during Verifying / between split parts** → discard output, never trash, end Cancelled | RU | ⚠️ (logic ✅; real timing 🔁) |
| 2.11 | Target unreachable (can't hit size after retries) → honest failure, source intact | RU | ⚠️ |
| 2.12 | ffmpeg crashes / non-zero exit → row shows the error tail, no partial output left | RI/M | ❌ (M) |
| 2.13 | Disk full / write-permission-denied mid-encode | — | ❌ (M) |
| 2.14 | Windows long path (>260) output/split folder → verbatim `\\?\`, or actionable error | RU | ⚠️ (helper ✅; real deep path 🔁) |
| 2.15 | Concurrent jobs (drop many) queue + complete, drawer reflects state | E2E (drawer) + M | ⚠️ |

## 3. Delivery: clipboard, trash, reveal, open

| # | Scenario | Level | Status |
|---|----------|-------|--------|
| 3.1 | Auto-copy single output to clipboard | RU + M | ⚠️ |
| 3.2 | **Copy all parts of a split** → one CF_HDROP write, every part lands | E2E | ✅ (regression for the bug) |
| 3.3 | Trash original after convert (on/off); multi-preset guard blocks a 2nd convert | RU | ✅ (guard); ⚠️ (real trash 🔁) |
| 3.4 | Reveal a present file in Finder/Explorer | M | 🔁 |
| 3.5 | Reveal/open a **moved/deleted** output → "no longer exists" error toast | RU + E2E | ✅ |
| 3.6 | Open-after-convert (off / multipart / all) | M | 🔁 |

## 4. Conversion history (journal)

| # | Scenario | Level | Status |
|---|----------|-------|--------|
| 4.1 | One record per job (single = 1 output, split = N) | RU/RI | ✅ |
| 4.2 | Migration: legacy per-part records merge into one; standalone singles preserved | RI | ✅ |
| 4.3 | Migration dedups duplicate records; idempotent on reload (no re-rewrite) | RU/RI | ✅ (verified on real 112→47 data) |
| 4.4 | Atomic write (temp+rename); crash mid-write can't corrupt | RU | ✅ |
| 4.5 | Corrupt journal → backed up to `.bak`, starts fresh, stays usable | RU | ✅ |
| 4.6 | 200 logical-record cap, newest kept | RU | ✅ |
| 4.7 | Created-time frozen (captured at encode, backfilled once on migration) | RU/RI | ✅ |
| 4.8 | find_by_output matches any part of a split | RU | ✅ |

## 5. Converted tab (UI)

| # | Scenario | Level | Status |
|---|----------|-------|--------|
| 5.1 | Splits collapse into one expandable group; singles are flat rows | U + E2E | ✅ |
| 5.2 | Expand/collapse; part rows aligned; thumbnails lazy | U + E2E | ⚠️ (structure ✅; thumb visual 🔁) |
| 5.3 | Play / Reveal / Copy per row + group "Copy all"; pressed-state feedback | E2E | ✅ (copy); ⚠️ (play/reveal feedback 🔁) |
| 5.4 | Created-vs-Converted tooltip on the time | U + M | ⚠️ |
| 5.5 | **Refresh preserves selection + expanded groups** (background convert / tab switch) | U | ✅ |
| 5.6 | Refresh doesn't leak the body-attached tooltip | U | ⚠️ (cleanup logic ✅) |
| 5.7 | Empty state; first-load "Loading…" | E2E | ✅ |
| 5.8 | Full keyboard nav (↑↓ ⏎ →/e c r esc) | U + M | ⚠️ |

## 6. Videos tab (UI)

| # | Scenario | Level | Status |
|---|----------|-------|--------|
| 6.1 | Recents render; LENGTH/RECORDED; compressed badge for orphans | E2E | ✅ |
| 6.2 | Quick-pick picker opens; choose preset → enqueue; scrollable for many presets | E2E + M | ✅ (open/choose); ⚠️ (scroll 🔁) |
| 6.3 | Active-bar mode: ←→ cycles preset, 1–9 instant-convert | U + M | ⚠️ (digit/index logic ✅) |
| 6.4 | **Failed row click → retry same preset** (picker fallback if preset deleted) | U + E2E | ✅ |
| 6.5 | Drop a video on the panel → picker floats over any tab | E2E (panel-host) + M | ⚠️ (mount ✅; real OLE drag 🔁) |
| 6.6 | Right-click "Compress with tamp" from Explorer | M | 🔁 |
| 6.7 | Filter recordings | M | 🔁 |
| 6.8 | Hover preview (montage proxy) | M | 🔁 |

## 7. Preferences

| # | Scenario | Level | Status |
|---|----------|-------|--------|
| 7.1 | Preset CRUD (create/edit/delete/default) persists | E2E + M | ⚠️ |
| 7.2 | **Block duplicate preset names** (case-insensitive) inline + backend | U + RU + E2E | ✅ |
| 7.3 | Preset validation: name required, target>0, fps/width/scale ints, split bounds | U + RU | ✅ |
| 7.4 | Behavior toggles persist (clipboard, trash, GPU, launch-at-login, context-menu, update-check) | E2E + M | ⚠️ |
| 7.5 | Recents-limit 1–200 validation | U + RU + E2E | ✅ |
| 7.6 | Theme system/light/dark applies live | M | 🔁 |
| 7.7 | Watched folders add/remove + asset-scope widening | RU (scope) + M | ⚠️ |
| 7.8 | Shortcut editor: validates by registering; rolls back on failure | RU + M | ⚠️ |
| 7.9 | Settings file hand-edited/out-of-range → validated on load | RU | ⚠️ (validate ✅; load-repair ❌) |

## 8. Internationalization (i18n)

| # | Scenario | Level | Status |
|---|----------|-------|--------|
| 8.1 | `t()` lookup, interpolation, en→key fallback, never throws | U | ✅ |
| 8.2 | Plurals via `Intl.PluralRules` — en one/other, **uk one/few/many/other** | U | ✅ |
| 8.3 | en.json / uk.json key parity (no missing/extra) | U | ✅ |
| 8.4 | Language switch (System/English/Українська) re-renders the whole UI | E2E | ✅ |
| 8.5 | Ukrainian text fits the tight UI spots (no overflow/clipping) | M | 🔁 (CSS-hardened; visual 🔁) |
| 8.6 | Cyrillic + Latin render in one font (Montserrat), no per-script mismatch | M | 🔁 |
| 8.7 | Native notifications localized from the persisted locale | RU | ✅ |

## 9. Update check

| # | Scenario | Level | Status |
|---|----------|-------|--------|
| 9.1 | Off by default; opt-in on first run; togglable | U + E2E | ✅ |
| 9.2 | Newest semver (incl. prereleases) > installed → modal on panel open | RU + E2E | ✅ |
| 9.3 | Dismiss remembers the version → never re-nags; only a newer one reappears | U + E2E | ✅ |
| 9.4 | Failed check is silent (no error toast); sends no user data | RU + E2E | ✅ |
| 9.5 | Real GitHub API round-trip | M | 🔁 |

## 10. Onboarding, permissions, tray/panel

| # | Scenario | Level | Status |
|---|----------|-------|--------|
| 10.1 | First-run welcome notice (tray hint + reopen shortcut + update consent); once only | U + E2E | ✅ |
| 10.2 | Notification permission primed with context; denied → recoverable Preferences row | RU + M | ⚠️ |
| 10.3 | Tray icon shows; click toggles panel; pin keeps it open | NAT + M | ⚠️ |
| 10.4 | **Tray ink repaints on live light/dark OS theme flip** (Windows) | RU (dedup-key) + M | ⚠️ (logic ✅; real flip 🔁) |
| 10.5 | Panel positions at the tray; smart-hide on blur (release) | M | 🔁 |
| 10.6 | App boots and the panel renders (real built app) | NAT | ✅ (CI smoke) |

## 11. Global shortcuts

| # | Scenario | Level | Status |
|---|----------|-------|--------|
| 11.1 | Compress-latest fires; stale-recording warning when newest is old | RU (lang) + M | ⚠️ |
| 11.2 | Toggle-panel shortcut | M | 🔁 |
| 11.3 | Accelerator parse/resolve; registration conflict handling | RU + M | ⚠️ |
| 11.4 | "No recent videos found" notification path | RU | ⚠️ |

## 12. Accessibility (WCAG 2.1 AA)

| # | Scenario | Level | Status |
|---|----------|-------|--------|
| 12.1 | :focus-visible ring on every control incl. the switch track | M | 🔁 |
| 12.2 | ARIA tabs (aria-selected, tabpanel, ←→/Home/End roving) | M | 🔁 |
| 12.3 | Toast as a polite/assertive live region; success vs error | U (styling) + M | ⚠️ |
| 12.4 | Programmatic labels for inputs/radio groups/icon buttons | M | 🔁 |
| 12.5 | Modal dialog semantics + focus trap (picker, custom, update) | E2E (present) + M | ⚠️ |
| 12.6 | prefers-reduced-motion; placeholder contrast ≥4.5:1 | M | 🔁 |

## 13. Platform & distribution

| # | Scenario | Level | Status |
|---|----------|-------|--------|
| 13.1 | Windows context-menu register/unregister is all-or-nothing (rollback) | RU | ✅ (logic); 🔁 (real registry) |
| 13.2 | Uninstall removes autostart Run-key + caches/logs; keeps settings+history | M | 🔁 |
| 13.3 | macOS: legacy LaunchAgent cleanup; no Finder Quick Action yet | M | 🔁 |
| 13.4 | Builds + installs on Windows x64/arm64 + macOS (CI matrix) | CI | ✅ |
| 13.5 | Tag-driven beta: version derived from tag, Cargo↔package synced, no -beta on main | CI guards | ✅ (first run: beta.8) |
| 13.6 | Cross-version journal compat (old beta can't read new format) | — | ❌ (known one-way) |

---

## Confidence summary

**What I'm already confident in (strong automated coverage):** the conversion
*planning* math, the **history/journal** (schema, migration, dedup, atomicity,
corruption recovery — proven on real data), all the **pure UI logic**, the
**i18n** layer (parity + plurals), the **update-check** decisioning, and the
main **UI journeys** end-to-end via Playwright. These break loudly if regressed.

**To say "confident across normal AND edge cases," these gaps must close** (in
rough priority):

1. **Real encode correctness across inputs** (2.1, 2.11, 2.12) — a small corpus
   of fixture videos (tiny/large/no-audio/odd-codec/near-MAX_PATH) asserting the
   output fits the target, the source is intact, and failures surface cleanly.
   Today only one `encode_integration` happy-path exists.
2. **Cancel timing on the real pipeline** (2.9, 2.10) — drive a real encode and
   cancel during Pass/Verifying/between parts; assert no trash, source intact,
   Cancelled. Logic is unit-tested; the *timing* isn't.
3. **The IPC contract seam** — Playwright mocks the backend, so a command-shape
   change won't fail it. Add the mock-parity check (every `invoke` in `ipc.ts`
   has a mock entry) AND let the **tauri-driver native suite drive a real
   conversion** end-to-end (it currently only boots the panel).
4. **Platform side-effects on real OS** (3.1/3.3 clipboard+trash, 13.1 registry,
   13.2 uninstall, 10.4 tray theme) — these are logic-tested but their real OS
   effects are only manual. Promote the highest-value ones into the native suite.
5. **Accessibility + visual** (§12, 8.5/8.6) — no automated assertions. Add
   axe-core checks in Playwright for ARIA/contrast/focus, and a couple of visual
   snapshots (incl. a Ukrainian render) to catch layout/clipping regressions.
6. **Performance** (1.7, 2.15) — one test asserting panel-open stays under a
   budget with ~200 mocked recents, and that thumbnails don't all decode eagerly.

**Honest verdict:** the suite is solid for *logic and the main flows* and would
catch most regressions a normal user would hit. It is **not yet** sufficient to
claim edge-case confidence — the real ffmpeg pipeline, cancel timing, the
IPC seam, and OS side-effects are the four areas where a real bug could ship
green today. Closing items 1–4 above is what would let me say it with
confidence.
