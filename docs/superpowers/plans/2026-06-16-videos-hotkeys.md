# Videos-tab Hotkeys Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development / executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Convert with different profiles using only the keyboard on the Videos tab — `←`/`→` cycle the active profile (active-bar mode), and `1`–`9` instantly convert the selected video with that preset (both modes). Plus a contextual footer hint.

**Branch:** `converted-tree` (current; off `release/0.3.0`).

**Files:** `src/views/list.ts`, `src/views/list.test.ts`, `src/main.ts`.

**Context — the current `onKeyDown` in `list.ts` (read it):** after the modal/quickPick/`el.hidden`/filter guards and the `ArrowDown`/`ArrowUp`/`Escape` switch, there is `if (inFilter) return;`, then an active-bar `[`/`]` block (`cycleActive`), then `const selected = …` with `Enter`/`d` → `onRowClick` and `e` → `toggleExpand`, then a printable→filter fallback. Existing helpers in scope: `layoutMode()`, `cycleActive(dir)`, `orderedPresets()` (default first), `doEnqueue(v, presetId)`, `collapseExpanded()`, `getSettings()`.

---

## Conventions
- Frontend (repo root): put `C:\Program Files\nodejs` FIRST on PATH; `bunx tsc --noEmit`, `bun run test`.
- No servers. One commit per task.

---

## Task 1: `←`/`→` profile cycle + `1`–`9` instant convert

**Files:** `src/views/list.ts`, `src/views/list.test.ts`

- [ ] **Step 1 — failing test** in `src/views/list.test.ts` (a pure helper drives the digit→preset-index mapping):

```ts
import { presetIndexForDigit } from "./list";

describe("presetIndexForDigit", () => {
  it("maps '1' to the first preset", () => {
    expect(presetIndexForDigit("1", 3)).toBe(0);
  });
  it("maps '3' to index 2 when in range", () => {
    expect(presetIndexForDigit("3", 3)).toBe(2);
  });
  it("returns null when the digit is past the preset count", () => {
    expect(presetIndexForDigit("4", 3)).toBeNull();
  });
  it("returns null for a non-digit", () => {
    expect(presetIndexForDigit("x", 3)).toBeNull();
  });
});
```

- [ ] **Step 2 — run** (repo root): `bun run test list` → FAIL (`presetIndexForDigit` not exported).

- [ ] **Step 3 — add the pure helper** at module scope in `list.ts` (next to `shouldPickPreset`):

```ts
/** Zero-based preset index a 1–9 key selects, or null if out of range / not a
 *  digit. Used for the "press a number to convert with that preset" shortcut. */
export function presetIndexForDigit(key: string, count: number): number | null {
  const n = Number(key) - 1;
  return Number.isInteger(n) && n >= 0 && n < count ? n : null;
}
```

- [ ] **Step 4 — `←`/`→` cycle the active preset.** Replace the existing active-bar `[`/`]` block in `onKeyDown` with one that also accepts the arrows:

```ts
    // Active-bar mode: [ ] and ← → cycle the active preset.
    if (layoutMode() === "active-bar") {
      if (e.key === "[" || e.key === "ArrowLeft") {
        e.preventDefault();
        cycleActive(-1);
        return;
      }
      if (e.key === "]" || e.key === "ArrowRight") {
        e.preventDefault();
        cycleActive(1);
        return;
      }
    }
```

(This sits after `if (inFilter) return;`, so the arrows still move the caret while the filter is focused; they only cycle the profile once a row is selected / focus is off the filter.)

- [ ] **Step 5 — `1`–`9` instant convert.** In the `if (selected) { … }` block, after the `e` (expand) handler and before the printable→filter fallback, add:

```ts
      if (e.key >= "1" && e.key <= "9") {
        const idx = presetIndexForDigit(e.key, orderedPresets().length);
        if (idx !== null) {
          e.preventDefault();
          collapseExpanded();
          void doEnqueue(selected, orderedPresets()[idx].id);
          return;
        }
      }
```

(`orderedPresets()` is default-first, matching the quick-pick overlay's numbering, so `1` = the default preset in both layout modes. This intercepts digits before they fall through to the filter — intended per the approved design.)

- [ ] **Step 6 — run** (root): `bun run test list && bunx tsc --noEmit` → tests pass (incl. the 4 new), typecheck clean.

- [ ] **Step 7 — commit**

```bash
git add src/views/list.ts src/views/list.test.ts
git commit -m "feat: Videos hotkeys — arrows cycle the active profile, 1-9 convert with that preset"
```

---

## Task 2: Contextual footer hint

**Files:** `src/views/list.ts`, `src/main.ts`

- [ ] **Step 1 — add `footerHint()` to the list view.** In `createListView`, add a function and expose it on the returned `ListView` (also add `footerHint(): string;` to the `ListView` interface):

```ts
  function footerHint(): string {
    return layoutMode() === "active-bar"
      ? "↑↓ select · ←→ profile · 1–9 quick profile · ⏎ convert · esc back"
      : "↑↓ select · ⏎ pick preset · 1–9 quick profile · e expand · esc back";
  }
```

- [ ] **Step 2 — drive the footer from `main.ts`.** Read `main.ts`. Grab the footer element once (`const footer = app.querySelector(".panel-footer") as HTMLElement`). In `setTab`, set its text per tab:

```ts
    if (tab === "videos") {
      footer.textContent = listView.footerHint();
      void listView.refresh();
      listView.focusFilter();
    } else if (tab === "converted") {
      footer.textContent = "↑↓ select · ⏎ play · c copy · r reveal · esc back";
      void convertedView.refresh();
    } else {
      footer.textContent = "esc back";
    }
```

Also refresh the hint when settings change while on Videos: in the existing `onSettingsChanged` handler, after `listView.onSettingsChanged()`, add `if (!listView.el.hidden) footer.textContent = listView.footerHint();`.

(The Converted hint is forward-looking; its key handlers land in the phase-2 plan. Match the real structure of `setTab`/`onSettingsChanged` in `main.ts` — adapt names rather than assume.)

- [ ] **Step 3 — run** (root): `bunx tsc --noEmit && bun run test` → clean; all tests pass.

- [ ] **Step 4 — commit**

```bash
git add src/views/list.ts src/main.ts
git commit -m "feat: contextual Videos-tab footer hint reflecting the available hotkeys"
```

---

## Task 3: Changeset + verification

- [ ] **Step 1 — changeset** `.changeset/videos-hotkeys.md`:

```markdown
---
"tamp": minor
---

Keyboard-first preset switching on the Videos tab. In "Keep one preset active"
mode, ← / → cycle the active profile (alongside [ / ]). In both modes, pressing
1–9 instantly converts the selected video with that preset — no menu. The footer
hint reflects the keys available in the current mode.
```

- [ ] **Step 2 — verify** (root): `bunx tsc --noEmit && bun run test` (all green).

- [ ] **Step 3 — commit**

```bash
git add .changeset/videos-hotkeys.md
git commit -m "chore: changeset for Videos-tab hotkeys"
```

---

## Self-review notes
- `presetIndexForDigit` (Task 1) is the single pure helper, unit-tested, consumed in `onKeyDown`.
- `←`/`→` only act in active-bar mode and only past the `inFilter` guard (caret movement in the filter is preserved).
- `1`–`9` require a selected row (inside `if (selected)`) and map via `orderedPresets()` (default-first), consistent with the quick-pick overlay numbering.
- Footer hint is mode-aware (Task 2); Converted hint is a placeholder until phase 2.
