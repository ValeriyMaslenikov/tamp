# i18n + Ukrainian

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development / executing-plans. Checkbox (`- [ ]`) steps.

**Goal:** Make tamp fully internationalized with **Ukrainian** as a second language. All user-facing copy lives in centralized per-locale JSON; a runtime `t()` resolves strings with interpolation + correct plurals; a Preferences language selector switches live; the UI font covers both Latin and Cyrillic; and `docs/i18n.md` documents how it works and how to add a language/string.

**Decisions (locked):**
- Storage: `src/i18n/en.json` + `src/i18n/uk.json` (one file per locale, nested keys by area). English is the source-of-truth / fallback.
- Runtime: `src/i18n/index.ts` — `t(key, params?)`, `{name}` interpolation, plural objects selected via `Intl.PluralRules(locale)` (en: one/other; **uk: one/few/many/other**), key-fallback to `en` then to the key string.
- Setting: `locale: "system" | "en" | "uk"` (default `system`, resolved from `navigator.language`). Preferences "Language" dropdown (System / English / Українська). On change: persist, set `<html lang>`, and re-render (a `location.reload()` is the simple, reliable apply; a full re-render is an acceptable alternative).
- Font: replace Poppins with **Montserrat** (Latin + Cyrillic) via `@fontsource`. Keep a system fallback stack that also covers Cyrillic (`"Segoe UI"`, `-apple-system`).
- Backend: the handful of Rust **native-notification** strings (stale-recording warning, "no recent videos") become locale-aware (they fire from the global shortcut without the panel, so the frontend can't localize them).

**Branch:** `converted-tree`. Frontend-heavy + a little Rust. **Conventions:**
- Frontend (repo root): `C:\Program Files\nodejs` FIRST on PATH; `bunx tsc --noEmit`, `bun run test`.
- Rust (`cd src-tauri`): `%USERPROFILE%\.cargo\bin` on PATH; `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
- No servers, no pushing. One commit per task. Preserve ALL prior work (7 batches + copy fix + update-check). Telemetry strictly none.

**Execution:** two phases — **Phase 1 (foundation)** then **Phase 2 (extraction + Ukrainian + QA)**, verifying between. Phase-2 extraction tasks are SEQUENTIAL because they all append to the shared `en.json`/`uk.json`.

---

## PHASE 1 — Foundation

### Task 1.1: i18n runtime + en/uk skeleton + tests

**Files:** `src/i18n/index.ts` (new), `src/i18n/en.json` (new), `src/i18n/uk.json` (new), `src/i18n/i18n.test.ts` (new)

- [ ] **Step 1 — JSON files.** Create `en.json` and `uk.json` with a nested structure seeded with a few shared keys to establish conventions, e.g.:
```json
{ "app": { "name": "tamp" },
  "common": { "save": "Save", "cancel": "Cancel", "later": "Later", "gotIt": "Got it" },
  "units": { "parts": { "one": "{count} part", "other": "{count} parts" } } }
```
`uk.json` mirrors the keys with Ukrainian values; **plural objects in uk use one/few/many/other** (e.g. `"parts": { "one": "{count} частина", "few": "{count} частини", "many": "{count} частин", "other": "{count} частини" }`).
- [ ] **Step 2 — runtime.** `src/i18n/index.ts`:
  - `import en from "./en.json"; import uk from "./uk.json";` `const DICTS = { en, uk }`.
  - `export type Locale = "en" | "uk";`
  - `resolveLocale(setting: string, navLang: string): Locale` — `"en"|"uk"` pass through; `"system"` → `uk` iff `navLang` starts with `uk`, else `en`.
  - `setLocale(locale: Locale)` — sets the active dict + an `Intl.PluralRules(locale)` + `document.documentElement.lang = locale`.
  - `t(key: string, params?: Record<string, string | number>): string` — resolve the dotted key in the active dict; if the value is a **plural object** and `params.count` is a number, pick `pluralRules.select(count)` (fallback `other`); interpolate `{name}` from `params`; **fall back** to the `en` dict, then to the raw key if missing. Never throw.
  - Keep it dependency-free and synchronous (JSON is statically imported/bundled).
- [ ] **Step 3 — tests** (`i18n.test.ts`): key lookup, `{name}` interpolation, missing-key falls back to en then to the key, English plural (1→"1 part", 2→"2 parts"), Ukrainian plural categories (1→one, 2→few, 5→many, e.g. 1/2/5/21 map to one/few/many/one), `resolveLocale` for "system"/"en"/"uk".
- [ ] **Step 4 — verify.** `bunx tsc --noEmit && bun run test` clean.
- [ ] **Step 5 — commit** `git add src/i18n && git commit -m "feat(i18n): centralized per-locale JSON dictionaries + t() runtime with interpolation and Intl.PluralRules (en + uk)"`

### Task 1.2: locale setting + Preferences language selector + apply-on-change

**Files:** `src-tauri/src/settings.rs`, `src/lib/ipc.ts`, `src/views/preferences.ts`, `src/main.ts`

- [ ] **Step 1 — setting.** Add `locale: String` to the Rust `Settings` (camelCase serde, `#[serde(default = "default_locale")]` returning `"system"`); seed it in `default_settings`. Validate it's one of `system|en|uk` in `validate()` (reject otherwise). Mirror in the TS `Settings` (`locale: "system" | "en" | "uk"` — or `string`).
- [ ] **Step 2 — init at startup.** In `main.ts`, after the first `getSettings()`, call `setLocale(resolveLocale(settings.locale, navigator.language))` BEFORE building the views, so the initial render is localized.
- [ ] **Step 3 — selector.** Add a "Language" control to Preferences (a select/radio: System / English / Українська) bound to `locale`, persisted via the existing `persist()`. On change, after the save resolves, set the locale and re-render — simplest reliable apply is `location.reload()` (the webview reloads, re-reads settings, re-renders fully localized). Note the alternative (re-invoke each view's render) if a reload feels heavy.
- [ ] **Step 4 — verify.** `bunx tsc --noEmit && bun run test`; `cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` (settings round-trip + validate(locale)). `// manual:` note: switching language re-renders the whole UI.
- [ ] **Step 5 — commit** `git add -A src-tauri/src/settings.rs src/lib/ipc.ts src/views/preferences.ts src/main.ts && git commit -m "feat(i18n): locale setting + Preferences language selector (System/English/Ukrainian), applied at startup and on change"`

### Task 1.3: Cyrillic+Latin font (Montserrat replaces Poppins)

**Files:** `package.json`, `src/main.ts` (or a font CSS), `src/styles.css`, `src/assets/fonts/*` (remove Poppins)

- [ ] **Step 1 — add the font.** `bun add @fontsource/montserrat`. Import the Latin + **Cyrillic** subsets for the weights the app uses (400/500/600) — e.g. in `main.ts` (or a dedicated `src/fonts.css` imported there): `@fontsource/montserrat/latin-400.css`, `@fontsource/montserrat/cyrillic-400.css`, …500, …600. (Fontsource registers `@font-face` with the right `unicode-range` per subset.)
- [ ] **Step 2 — swap the stack.** Remove the three Poppins `@font-face` blocks (styles.css:1-21) and delete `src/assets/fonts/poppins-*.woff2`. Change the body `font-family` (styles.css:126) to `"Montserrat", "Segoe UI", -apple-system, BlinkMacSystemFont, system-ui, sans-serif` — every fallback also covers Cyrillic.
- [ ] **Step 3 — verify.** `bunx tsc --noEmit && bun run test` clean; confirm the build resolves the font imports (a `bun run build` dry-run if quick, else trust Vite). `// manual:` note: Latin and Cyrillic both render in Montserrat with no per-script fallback mismatch.
- [ ] **Step 4 — commit** `git add -A package.json bun.lock src/main.ts src/styles.css src/assets/fonts && git commit -m "feat(i18n): switch the UI font to Montserrat (covers Latin + Cyrillic) so Ukrainian renders in one consistent typeface"`

### Task 1.4: documentation

**Files:** `docs/i18n.md` (new), link from `AGENTS.md`/`README.md` if appropriate

- [ ] **Step 1 — write `docs/i18n.md`** covering: where strings live (`src/i18n/en.json` is the source of truth, `uk.json` mirrors it); the key naming convention (nested by area, e.g. `videos.empty`, `converted.copyAll`); the `t()` API (interpolation with `{name}`, plural objects + `Intl.PluralRules`, the en→key fallback); **how to add a string** (add to en.json + every locale, reference via `t("area.key")`); **how to add a language** (add `xx.json`, register it in `index.ts` `DICTS` + `resolveLocale`, add the option to the Preferences selector, ensure plural categories for that language); the font (Montserrat covers Latin+Cyrillic; how to extend coverage for other scripts); and the backend native-notification localization note.
- [ ] **Step 2 — commit** `git add docs/i18n.md && git commit -m "docs(i18n): how translation works — JSON dictionaries, t(), plurals, adding strings/languages, font coverage"`

---

## PHASE 2 — Extraction + Ukrainian + QA (sequential; each appends to en.json/uk.json)

For EACH extraction task: read the file(s), find every user-facing literal, add a nested key to **both** `en.json` (value = the current English literal) and `uk.json` (a correct, natural **Ukrainian** translation), replace the literal with `t("area.key", params?)`, and convert any count-based "N things" to a plural object (en one/other, uk one/few/many/other). Keep keys grouped by area. Do NOT translate non-user-facing strings (CSS classes, preset ids, log messages, dev-only text). Run `bunx tsc --noEmit && bun run test` after each.

### Task 2.1: Videos tab
**Files:** `src/views/list.ts`, `src/lib/dragdrop.ts`, `src/lib/naming.ts`, `src/i18n/en.json`, `src/i18n/uk.json` — empty state, "Filter recordings…", "＋ Add file…", LENGTH/RECORDED labels, the footer/keyboard hints, active-bar text, drop-overlay copy, retry pill, unreachable-folder banner copy, any "N" counts. Commit: `i18n: extract Videos-tab strings (en + uk)`.

### Task 2.2: Converted tab
**Files:** `src/views/converted.ts`, `src/lib/convgroup.ts`, `src/lib/format.ts`, `src/i18n/en.json`, `src/i18n/uk.json` — "No conversions yet", Created/Converted tooltip labels, "Copy all"/"Copy file"/play/reveal aria-labels, the part-count badge ("{count} parts" → plural), `splitSummaryLabel` ("smart split"/"{count} parts"/"split ≈{n}"), `formatPercentSmaller` ("{n}% smaller"), the footer hints. (format.ts becomes i18n-dependent — keep its number formatting from Batch 5 intact.) Commit: `i18n: extract Converted-tab + format labels (en + uk)`.

### Task 2.3: Preferences
**Files:** `src/views/preferences.ts`, `src/lib/forms.ts`, `src/i18n/en.json`, `src/i18n/uk.json` — section labels, every field label/placeholder, toggle labels + sub-labels (Copy to clipboard, Move to Trash, GPU encoder, Launch at login, context menu, update-check, the new Language label), radio-group labels (theme, videos-layout, open-after-convert, split mode), the preset editor (Name/Target/FPS/etc., Save/Cancel, validation toasts), the version line, the notification-denied recovery row. Commit: `i18n: extract Preferences + forms strings (en + uk)`.

### Task 2.4: Shared UI + onboarding + modals + errors + backend notifications
**Files:** `src/lib/onboarding.ts` (+ test), `src/lib/updatemodal.ts`, `src/lib/drawer.ts`, `src/lib/toast.ts` call sites, `src/lib/errors.ts`, `src/views/custom.ts`, `src/main.ts`, `src/lib/theme.ts`, `src-tauri/src/shortcuts.rs` (native notifications), `src/i18n/en.json`, `src/i18n/uk.json` —
  - Frontend: onboarding welcome copy + the update-check consent, the update modal ("tamp {version} is available", What's new/Download/Later), drawer titles/states ("Compressing…", "{count} queued", "+{count} more", "Cancelled", "✓ done", "✕ failed"), every `showToast("…")` literal, the `friendlyError` catalog (errors.ts → `t()` so backend errors localize), custom-convert page copy, any `main.ts` hint strings, theme labels.
  - Backend: localize the native-notification strings in `shortcuts.rs` (stale-recording warning, "No recent videos found") — resolve `settings.locale` (system→en/uk) and pick the string from a small Rust map; keep it minimal and behind the existing notify path. (This is the only Rust string work.)
  Commit: `i18n: extract shared UI/onboarding/modals/errors + localize native notifications (en + uk)`.

### Task 2.5: QA, completeness sweep, and finalize
**Files:** `src/i18n/en.json`, `src/i18n/uk.json`, `docs/i18n.md`, `src/styles.css` (only if Ukrainian overflow needs a fix)
- [ ] **Completeness sweep:** grep the frontend for remaining user-facing English string literals (in `textContent =`, `placeholder`, `title`, `aria-label`, `.innerHTML` with text, `showToast("…")`) that weren't extracted; extract any stragglers. Confirm `en.json` and `uk.json` have identical key sets (no missing/extra keys in either) — add a small test or script asserting key-parity.
- [ ] **Ukrainian layout pass:** review the longest Ukrainian strings against the tight UI spots (tabs, footer hints, active-bar, buttons, preset chips, drawer title). Batch 5 added ellipsis/truncation; fix any remaining overflow with CSS only (no logic). Note what was checked.
- [ ] **Finalize docs:** ensure `docs/i18n.md` matches the final key conventions + lists the areas.
- [ ] **Verify:** `bunx tsc --noEmit && bun run test`; `cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` — all clean.
- [ ] **Commit** `i18n: completeness sweep + en/uk key parity + Ukrainian layout QA + docs finalize`.

---

## Self-review notes
- English is the source of truth and the runtime fallback; `uk.json` mirrors its keys (Phase 2.5 asserts parity).
- Plurals are the main correctness risk — Ukrainian needs one/few/many/other via `Intl.PluralRules`, not English's one/other; every count string uses a plural object.
- Font: Montserrat covers Latin + Cyrillic so there's no per-script fallback mismatch; the system fallbacks (Segoe UI / -apple-system) also cover Cyrillic.
- Backend native notifications are the only strings the frontend can't reach (they fire without the panel) — localized minimally in Rust from the persisted locale.
- Extraction tasks are sequential (shared JSON files); foundation (Phase 1) is verified before extraction (Phase 2).
