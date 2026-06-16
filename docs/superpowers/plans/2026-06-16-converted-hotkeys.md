# Converted-tab Keyboard Navigation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development / executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Keyboard-drive the Converted tab — `↑↓` move a row cursor over visible rows (singles, group headers, and the parts of an expanded group); act on the selected row by type.

**Branch:** `converted-tree`.

**Files:** `src/views/converted.ts`, `src/views/converted.test.ts` (create), `src/styles.css`.

**Key scheme (approved):**
- `↑`/`↓` — move the cursor; accent highlight; `scrollIntoView`.
- **single:** `Enter`/`Space` play (openFile) · `c` copy · `r` reveal.
- **group header:** `Enter` toggle · `→`/`e` expand · `←` collapse · `c` copy all parts · `r` open folder.
- **part:** `Enter`/`Space` play · `c` copy · `r` reveal.
- `Esc` — collapse the selected open group, else hide the window.

**Current `converted.ts` (read it):** `createConvertedView()` returns `{ el, refresh }`. Helpers: `playButton(outputPath)`, `copyButton(outputPath)`, `revealButton(path, title)` (each builds a button whose click calls `openFile`/`copyFile`/`reveal` with `showToast` errors); `singleRow(rec)`; `partRow(part, index)`; `groupNode(node)` (builds `wrap.conv-tree` › `parent.conv-tree-parent` + `children.conv-children`; `parent` click toggles `wrap.is-open` + `children.hidden`); `refresh()` rebuilds from `groupConversions(records)`. `el` is the view root; it is `hidden` when the tab is inactive.

---

## Conventions
- Frontend only (repo root): `C:\Program Files\nodejs` FIRST on PATH; `bunx tsc --noEmit`, `bun run test`. No servers. One commit per task.

---

## Task 1: Keyboard navigation in `converted.ts`

**Files:** `src/views/converted.ts`, `src/views/converted.test.ts`, `src/styles.css`

- [ ] **Step 1 — failing test** — create `src/views/converted.test.ts` for the pure index helper:

```ts
import { describe, expect, it } from "vitest";
import { nextNavIndex } from "./converted";

describe("nextNavIndex", () => {
  it("moves down within range", () => { expect(nextNavIndex(0, 1, 3)).toBe(1); });
  it("clamps at the bottom", () => { expect(nextNavIndex(2, 1, 3)).toBe(2); });
  it("clamps at the top", () => { expect(nextNavIndex(0, -1, 3)).toBe(0); });
  it("selects the first row from no selection moving down", () => { expect(nextNavIndex(-1, 1, 3)).toBe(0); });
  it("selects the last row from no selection moving up", () => { expect(nextNavIndex(-1, -1, 3)).toBe(2); });
  it("returns -1 when there are no rows", () => { expect(nextNavIndex(0, 1, 0)).toBe(-1); });
});
```

- [ ] **Step 2 — run** (root): `bun run test converted` → FAIL (`nextNavIndex` not exported).

- [ ] **Step 3 — add the pure helper** at module scope in `converted.ts`:

```ts
/** Next cursor index when moving by `dir`, clamped to [0, len-1]. From "no
 *  selection" (-1), down → first row, up → last row. -1 when there are no rows. */
export function nextNavIndex(cur: number, dir: number, len: number): number {
  if (len === 0) return -1;
  if (cur < 0) return dir > 0 ? 0 : len - 1;
  return Math.min(len - 1, Math.max(0, cur + dir));
}
```

- [ ] **Step 4 — import the window API.** At the top of `converted.ts` add:

```ts
import { getCurrentWindow } from "@tauri-apps/api/window";
```

- [ ] **Step 5 — view-scope action helpers + a row-actions registry.** Inside `createConvertedView`, near the top (after `scroll` is created), add:

```ts
  type RowActions = {
    kind: "single" | "group" | "part";
    play?: () => void;
    copy: () => void;
    reveal: () => void;
    expand?: () => void;
    collapse?: () => void;
    isOpen?: () => boolean;
  };
  const rowActions = new WeakMap<HTMLElement, RowActions>();
  let selEl: HTMLElement | null = null;

  const doPlay = (p: string) => openFile(p).catch((e) => showToast(String(e)));
  const doCopy = (p: string) =>
    copyFile(p).then(() => showToast("Copied to clipboard")).catch((e) => showToast(String(e)));
  const doReveal = (p: string) => reveal(p).catch((e) => showToast(String(e)));
```

- [ ] **Step 6 — tag rows `conv-nav` and register their actions.**
  - In `singleRow(rec)`: after building `r`, add `r.classList.add("conv-nav")` and:
    ```ts
    rowActions.set(r, {
      kind: "single",
      play: () => doPlay(rec.outputPath),
      copy: () => doCopy(rec.outputPath),
      reveal: () => doReveal(rec.outputPath),
    });
    ```
  - In `partRow(part, index)`: `r.classList.add("conv-nav")` and:
    ```ts
    rowActions.set(r, {
      kind: "part",
      play: () => doPlay(part.outputPath),
      copy: () => doCopy(part.outputPath),
      reveal: () => doReveal(part.outputPath),
    });
    ```
  - In `groupNode(node)`: replace the inline click-toggle with explicit functions, add `conv-nav` to `parent`, and register:
    ```ts
    const expand = () => { wrap.classList.add("is-open"); children.hidden = false; };
    const collapse = () => { wrap.classList.remove("is-open"); children.hidden = true; };
    const isOpen = () => wrap.classList.contains("is-open");
    parent.addEventListener("click", () => (isOpen() ? collapse() : expand()));
    parent.classList.add("conv-nav");
    rowActions.set(parent, {
      kind: "group",
      copy: () => Promise.all(node.parts.map((p) => copyFile(p.outputPath)))
        .then(() => showToast("Copied to clipboard")).catch((e) => showToast(String(e))),
      reveal: () => doReveal(node.folder),
      expand, collapse, isOpen,
    });
    ```
    (Keep the existing `copyAll`/folder *buttons* as they are — the registry just mirrors their actions for the keyboard. If the existing parent already had a click toggle, replace it with the one above; do not double-register the toggle.)

- [ ] **Step 7 — selection + key handler.** Inside `createConvertedView` add:

```ts
  function navRows(): HTMLElement[] {
    return Array.from(scroll.querySelectorAll<HTMLElement>(".conv-nav")).filter((el) => {
      const kids = el.closest(".conv-children") as HTMLElement | null;
      return !kids || !kids.hidden; // skip parts of a collapsed group
    });
  }
  function setSelected(node: HTMLElement | null): void {
    if (selEl) selEl.classList.remove("is-sel");
    selEl = node;
    if (node) {
      node.classList.add("is-sel");
      node.scrollIntoView({ block: "nearest" });
    }
  }
  function moveSel(dir: number): void {
    const rows = navRows();
    const cur = selEl ? rows.indexOf(selEl) : -1;
    const i = nextNavIndex(cur, dir, rows.length);
    setSelected(i >= 0 ? rows[i] : null);
  }
  function onKeyDown(e: KeyboardEvent): void {
    if (el.hidden) return; // only when the Converted tab is active
    if (e.key === "ArrowDown") { e.preventDefault(); moveSel(1); return; }
    if (e.key === "ArrowUp") { e.preventDefault(); moveSel(-1); return; }
    if (!selEl) return;
    const a = rowActions.get(selEl);
    if (!a) return;
    switch (e.key) {
      case "Enter":
      case " ":
        e.preventDefault();
        if (a.kind === "group") (a.isOpen?.() ? a.collapse : a.expand)?.();
        else a.play?.();
        return;
      case "ArrowRight":
      case "e":
        if (a.kind === "group" && !a.isOpen?.()) { e.preventDefault(); a.expand?.(); }
        return;
      case "ArrowLeft":
        if (a.kind === "group" && a.isOpen?.()) { e.preventDefault(); a.collapse?.(); }
        return;
      case "c": e.preventDefault(); a.copy(); return;
      case "r": e.preventDefault(); a.reveal(); return;
      case "Escape":
        if (a.kind === "group" && a.isOpen?.()) { e.preventDefault(); a.collapse?.(); return; }
        e.preventDefault();
        void getCurrentWindow().hide().catch(() => {});
        return;
    }
  }
  document.addEventListener("keydown", onKeyDown);
```

- [ ] **Step 8 — reset the cursor on refresh.** At the end of `refresh()` (after rows are appended; not in the empty-state branch), select the first row:

```ts
    selEl = null;
    setSelected(navRows()[0] ?? null);
```

- [ ] **Step 9 — highlight CSS.** Append to `src/styles.css`:

```css
.conv-nav.is-sel {
  background: var(--row-hover);
  box-shadow: inset 0 0 0 1.5px var(--accent);
  border-radius: 8px;
}
```

- [ ] **Step 10 — run** (root): `bun run test converted && bunx tsc --noEmit && bun run test` — the 6 new `nextNavIndex` tests pass; typecheck clean; full suite green (no regressions).

- [ ] **Step 11 — commit**

```bash
git add src/views/converted.ts src/views/converted.test.ts src/styles.css
git commit -m "feat: Converted-tab keyboard navigation (cursor, play/copy/reveal, expand/collapse)"
```

---

## Task 2: Footer hint + changeset + verification

**Files:** `src/main.ts` (the Converted footer text is already set in `setTab`; confirm it reads `↑↓ select · ⏎ play · c copy · r reveal · esc back` and extend it to mention expand), `.changeset/converted-hotkeys.md`

- [ ] **Step 1 — footer hint.** In `src/main.ts` `setTab`, update the `converted` branch's footer text to include expand:

```ts
      footer.textContent = "↑↓ select · ⏎ play · →/e expand · c copy · r reveal · esc back";
```

- [ ] **Step 2 — changeset** `.changeset/converted-hotkeys.md`:

```markdown
---
"tamp": minor
---

Keyboard navigation on the Converted tab. ↑/↓ move a cursor over the history;
Enter/Space play the selected output (→/e expand a multi-part group, ← collapse);
c copies, r reveals (copy-all / open-folder on a group header); Esc collapses or
closes. The footer hint lists the keys.
```

- [ ] **Step 3 — verify** (root): `bunx tsc --noEmit && bun run test` — all green.

- [ ] **Step 4 — commit**

```bash
git add src/main.ts .changeset/converted-hotkeys.md
git commit -m "feat: Converted-tab footer hint + changeset for keyboard nav"
```

---

## Self-review notes
- `nextNavIndex` is the single pure helper (Task 1), unit-tested, used by `moveSel`.
- `navRows()` recomputes visible rows each time (parts of a collapsed group filtered via `.conv-children[hidden]`), so expand/collapse needs no separate nav rebuild.
- The handler is gated by `el.hidden`, so it's inert on other tabs and coexists with `list.ts`'s document keydown handler (also `el.hidden`-gated).
- Actions are registered in a `WeakMap` keyed on the row element; the keyboard mirrors the existing button actions (no behavior divergence).
- `refresh()` re-selects the first row so the cursor is always valid after a re-render.
