# Batch 2 — Accessibility (WCAG 2.1 AA)

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development / executing-plans. Checkbox (`- [ ]`) steps.

**Goal:** Bring tamp to WCAG 2.1 AA on the issues from the audit (`docs/QUALITY-AUDIT-2026-06-16.md`, Accessibility + related): visible focus, ARIA tabs, live-region toast, control labels, reduced-motion, placeholder contrast, modal semantics + focus trap. Decision: full AA; a single uniform accent focus ring app-wide.

**Branch:** `converted-tree`. Frontend-only. **Conventions:** repo root, `C:\Program Files\nodejs` FIRST on PATH; `bunx tsc --noEmit`, `bun run test`. No servers. One commit per task. Read the cited code and adapt. a11y is largely DOM/attribute work — where no pure logic exists, the verification is tsc + the attributes being present + a `// manual:` note for the screen-reader/keyboard check.

---

## Task 1: Visible focus + reduced-motion + placeholder contrast (CSS)

**Audit:** styles.css has only `.input:focus` and an `outline:none`; NO `:focus-visible` on custom controls; the switch hides its native checkbox so Tab shows nothing (WCAG 2.4.7). No `@media (prefers-reduced-motion)` — the `.preview-loading` shimmer loops forever (2.3.3). `--placeholder` is 2.1–2.4:1 in both themes (1.4.3).

**Files:** `src/styles.css`

- [ ] **Step 1 — uniform focus ring.** Add a global rule:
```css
:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
/* the toggle hides its native checkbox; ring the visible track instead */
.switch input:focus-visible + .track {
  box-shadow: 0 0 0 2px var(--accent);
}
```
Find the existing `outline: none` (~line 873) and ensure it does not suppress `:focus-visible` (scope it to `:focus:not(:focus-visible)` if needed). Keep `:focus-visible` (not `:focus`) so mouse clicks don't ring.

- [ ] **Step 2 — reduced-motion.** Append:
```css
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: 0.001ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.001ms !important;
    scroll-behavior: auto !important;
  }
  .preview-loading { animation: none; }
}
```

- [ ] **Step 3 — placeholder contrast (≥4.5:1).** Raise `--placeholder` in both palettes: dark `--placeholder: #8a8a93;` (matches --text-dim, ~5:1 on --surface-2 #222226), light `--placeholder: #73737d;` (~4.6:1 on #e9e9ec). Verify the new ratios are ≥4.5:1 against `--surface-2`.

- [ ] **Step 4 — verify.** `bunx tsc --noEmit` (clean). Leave a `// manual:` note: Tab shows a ring on every control incl. toggles; OS Reduce-motion stops the shimmer.

- [ ] **Step 5 — commit** `git add src/styles.css && git commit -m "a11y: visible focus-visible ring on all controls, prefers-reduced-motion support, accessible placeholder contrast"`

---

## Task 2: ARIA tabs pattern

**Audit:** `main.ts` declares `role=tablist`/`role=tab` but never sets `aria-selected`; views have no `role=tabpanel`/`aria-labelledby`; seg buttons have click-only handlers (no Left/Right roving). Declaring the role without states misleads AT (WCAG 4.1.2).

**Files:** `src/main.ts`

- [ ] **Step 1 — selected state + panel association.** Give each seg button an `id` and each view root an `id`; set `aria-controls` on the button → its panel and `aria-labelledby` on the panel → its button; set `role="tabpanel"` on each view root (listView.el / convertedView.el / prefsView.el). In `setTab`, set `aria-selected="true"` on the active seg button and `"false"` on the others (alongside the existing `is-active` class).

- [ ] **Step 2 — roving tabindex + arrow keys.** Implement the ARIA tabs keyboard pattern on the tablist: the active tab has `tabindex="0"`, the others `tabindex="-1"`; `ArrowLeft`/`ArrowRight` (and Home/End) move between tabs (wrapping), moving focus and activating. Add a `keydown` handler on the seg buttons; guard so it doesn't interfere with the views' own document key handlers (the tablist handler is on the buttons, not document).

- [ ] **Step 3 — verify.** `bunx tsc --noEmit && bun run test` clean. `// manual:` note: SR announces the selected tab; Left/Right move tabs.

- [ ] **Step 4 — commit** `git add src/main.ts && git commit -m "a11y: complete the ARIA tabs pattern (aria-selected, tabpanel association, arrow-key roving)"`

---

## Task 3: Toast as a live region (+ success/error semantics)

**Audit:** the toast `<div>` has no `role`/`aria-live`; `showToast` just sets textContent + flips `hidden`, so all errors and "Copied" are silent to SR (WCAG 4.1.3). Also (from Visual area) every toast carries a red error border even for success.

**Files:** `src/main.ts`, `src/lib/toast.ts`, `src/styles.css`

- [ ] **Step 1 — live region.** Give the toast container `role="status"` `aria-live="polite"` and keep it in the DOM (don't `display:none` it — toggle a visual `is-shown` class via opacity/visibility, or re-set textContent after un-hiding so the change is announced). Extend `showToast` to accept a kind (`"info" | "success" | "error"`) and set `role="alert"` / `aria-live="assertive"` for errors, polite otherwise (or use a single polite region and a visual variant).

- [ ] **Step 2 — success vs error styling.** Style success (e.g. `--success` accent/check) distinct from error (`--danger`/red). Update the call sites that are confirmations ("Copied to clipboard" in converted.ts/list.ts/drawer.ts) to pass `success`, and the validation/IPC failures to pass `error`. Default unspecified to info/neutral.

- [ ] **Step 3 — verify.** `bunx tsc --noEmit && bun run test` clean. `// manual:` note: SR announces toasts; success looks distinct from error.

- [ ] **Step 4 — commit** `git add src/main.ts src/lib/toast.ts src/styles.css src/views/converted.ts src/views/list.ts src/lib/drawer.ts && git commit -m "a11y: toast is a polite/assertive live region; success vs error styling"`

---

## Task 4: Programmatic labels for inputs, switches, radio groups, and icon buttons

**Audit:** `forms.ts field()` emits a `<span class="field-label">` next to an input with no `<label for>`/`id`/`aria-labelledby` → fields announce "edit, blank" (1.3.1/4.1.2). Radio groups (`radioRow`, videos-layout) lack a group name. Icon-only buttons: `buildRevealButton` (list.ts) has `tabIndex=-1` + only `title`, no `aria-label`.

**Files:** `src/lib/forms.ts`, `src/views/preferences.ts`, `src/views/custom.ts`, `src/views/list.ts`, `src/views/converted.ts`, `src/lib/drawer.ts`

- [ ] **Step 1 — bind field labels.** In `forms.ts field()`, associate the label with the input: generate a unique id, set it on the input, and either use a real `<label for={id}>` or `aria-labelledby={labelId}`. Keep the `.field` flex layout. Apply so all preset-editor / custom-page / shortcut / recents-limit fields get an accessible name.

- [ ] **Step 2 — name radio groups.** Wrap each radio set (`radioRow` in forms.ts, the videos-layout options, the open-after-convert options in preferences.ts) in a `role="radiogroup"` (or `<fieldset><legend>`) with an `aria-label`/legend naming the group (e.g. "Split mode", "Open in file manager after converting", "Videos screen layout").

- [ ] **Step 3 — label icon-only buttons.** Add `aria-label` to every icon-only button that lacks one: `buildRevealButton` (list.ts) and any reveal/play/copy buttons in converted.ts/list.ts/drawer.ts that only set `title`. (Converted's copy already has aria-label; mirror it everywhere.) Decide reveal/arrows tab order: since the app is arrow-key-driven, keep them out of the Tab order is acceptable IF they have an aria-label and a documented key — but the per-row reveal has NO key, so give the reveal button `tabindex` 0 (in the Tab order) OR add it to the list's key scheme; pick one and make it operable + named.

- [ ] **Step 4 — verify.** `bunx tsc --noEmit && bun run test` clean. `// manual:` note: SR announces each field/group/button by name.

- [ ] **Step 5 — commit** `git add -A src/lib/forms.ts src/views src/lib/drawer.ts && git commit -m "a11y: programmatic labels for inputs, named radio groups, and icon-only buttons"`

---

## Task 5: Modal semantics + focus management for the picker and Custom page

**Audit:** the quick-pick overlay and the custom-convert page have no `role="dialog"`/`aria-modal`/label and don't trap focus or move SR focus in; Tab can walk behind them to the covered tabs/rows (WCAG 2.4.3/4.1.2).

**Files:** `src/views/list.ts` (quick-pick), `src/views/custom.ts`

- [ ] **Step 1 — dialog roles.** Add `role="dialog"` `aria-modal="true"` and an `aria-label` (e.g. "Choose a preset", "Custom conversion") to the quick-pick overlay and the custom page root.

- [ ] **Step 2 — focus management.** On open: move DOM focus into the dialog (first item / first field). On close: restore focus to the trigger (custom already calls focusFilter — keep). Implement a Tab focus trap (cycle Tab/Shift+Tab within the dialog), or set `inert`/`aria-hidden` on the background `.panel` content while the dialog is open. The quick-pick currently relies on the document key handler — keep that, but also move focus in so SR users land inside.

- [ ] **Step 3 — verify.** `bunx tsc --noEmit && bun run test` clean. `// manual:` note: opening the picker/custom announces a dialog; Tab stays inside; closing restores focus.

- [ ] **Step 4 — commit** `git add src/views/list.ts src/views/custom.ts && git commit -m "a11y: dialog semantics + focus trap for the preset picker and custom-convert page"`

---

## Self-review notes
- Uniform `:focus-visible` accent ring + a dedicated switch-track ring (Task 1) covers the custom-painted controls the UA outline misses.
- Tabs get the full pattern (Task 2); the toast becomes the announced status channel (Task 3); every input/group/icon-button gets a name (Task 4); modals are announced + trapped (Task 5).
- These are attribute/DOM changes; the durable check is tsc + the attributes present + the per-task `// manual:` screen-reader/keyboard pass I'll run on-device after the batch.
