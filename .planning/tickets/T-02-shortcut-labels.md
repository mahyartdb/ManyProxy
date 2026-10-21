# T-02 — Shortcut label purge

Owner: implementer. Depends on: T-01 (console) merged, so main.rs edits don't collide.

## Rule

No keyboard shortcut may appear as visible text inside a button, tab, toggle, or menu
*label*. Shortcuts stay functional and move to tooltips and the existing Settings → Hotkey
reference. Acceptable exception: dropdown menus may keep a Kbd chip on its own trailing
element (standard Windows menu convention) — the chip is not part of the label text.

## Sites (from code map; verify each before editing)

Visible label text / Kbd chip suffix to remove:
- `src/main.rs:25672-25674`, `25691-25693`, `25710-25711` — inspector operate row:
  remove `shortcut_kbd(...)` suffix, strip `(F5)` / `(Shift-F5)` / `(Ctrl-T)` from
  `accessibility_label` and tooltip text. Tooltip keeps the key: `Start (F5)`.
- `src/main.rs:24763-24798` — inspector tabs: remove `(Ctrl-1)`…`(Ctrl-6)` suffix chips.
- `src/main.rs:26419` — search field aria: `Search configurations` only.
- `src/main.rs:26692` — bulk label tooltip: drop `(F5)`.
- `src/main.rs:26715` — Uncheck all tooltip: drop `(Esc)`.
- `src/main.rs:2136-2146` — row serve button: accessible name becomes
  `Start "<name>"`, tooltip keeps `F5`.
- Menus `28044-28825`: keep the Kbd chip, but strip the key from the accessible *label*;
  the chip element remains the visible affordance.

## Tests that pin the old strings (update, do not delete)

`38755-38801`, `45925-45984`, `48271`, `49576-49596`, `55085-55100`, plus
`shortcut_buttons_keep_pinned_accessible_labels` at `45895`. Rename that test to match
what it now pins: accessible names without key hints, tooltips with key hints.

## Acceptance

- `grep -n "(F5)\|(Ctrl-\|(Shift-F5)\|(Del)" src/main.rs` returns only tooltip strings and
  the `SHORTCUT_LINES` reference table.
- Every key still works (F5, Shift-F5, Ctrl-T, Ctrl-1..6, Ctrl-F, Ctrl-N, Ctrl-O, Ctrl-K).
- Hovering any affected control shows the key.
- `cargo test --locked` green.
