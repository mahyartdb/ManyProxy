# T-02 — Shortcut labels: action-only names, visible Kbd chips

Owner: implementer. Depends on: T-01 merged, then T-07 slices 0–6 (UI code in small files).

## Rule

Visible key hints stay as Kbd chips: menu rows, tab suffixes, and the inspector operate
row (DESIGN.md:191 — a chip is an element, not label text). What goes is a key used as a
*name*: inline `(Key)` text inside a label and the `(Key)` suffix inside
`accessibility_label` / `aria_label`. Every control whose name loses the key keeps it in
its tooltip, and Settings → Hotkey (renamed Shortcuts in T-04) stays the written reference.

## Sites (from code map; verify each before editing)

- `src/main.rs:25684-25734` — inspector operate row: keep the `shortcut_kbd(...)` children;
  `accessibility_label` becomes `Start the selected endpoint`, `Stop`, `Test`. Tooltips:
  Start already has `(F5)`; Stop (`25712`) and Test (`25731`) gain their keys.
- `src/main.rs:24781-24817` — inspector tabs: keep the suffix chips.
  `InspectorTab::a11y_label()` (`13093-13102`) drops `(Ctrl-1)`…`(Ctrl-6)`.
- `src/main.rs:26437` — search field aria: `Search configurations` only.
- `src/main.rs:2136-2146` — row serve button: accessible name `Start "<name>"` /
  `Stop "<name>"`; tooltip keeps `F5`. T-03 later moves this control out of the row.
- Menus `28044-28825` — keep the Kbd chip and `aria_keyshortcuts`; drop the key from the
  accessible label only (`Add configuration`, `Import`, `Start selected`, …).
- `src/main.rs:26697`, `26733` — bulk legend and Uncheck-all tooltips already carry
  `(F5)` / `(Esc)`; leave them, they are tooltip text, not names.
- `SHORTCUT_LINES` (`237-256`) is the reference table; T-04 owns it. Do not edit it here.

## Tests that pin the old strings (update, do not delete)

- `src/main.rs:48300` — `Start "Alpha" (F5)` becomes `Start "Alpha"`.
- `src/main.rs:42412` — the "Settings renders no Cancel sibling" pin: re-run; unchanged
  unless the footer label changes.
- `t02_toolbar_menu_items_announce_names` (`38801`) — menu accessible names lose the key.
- `shortcut_buttons_keep_pinned_accessible_labels` (`45974`) — rename to
  `shortcut_buttons_pin_action_names_and_key_tooltips`; pins action-only names, chips
  present, tooltips carrying keys.
- Any other pin found by the grep below: update the expected string, never the rule.

## Corrections (reviewer)

- The first draft over-stripped: it removed the tab and operate-row chips. Chips stay;
  only label text and accessible names lose the key.
- Stop's tooltip had no key; Test's had none either. Adding both is an addition, not a
  restoration.
- Deleted the self-contradictory "strip from tooltip text. Tooltip keeps the key." line:
  tooltips are where the key lives.

## Acceptance

- `grep -n "(F5)\|(Ctrl-\|(Shift-F5)\|(Del)" src/main.rs` returns only Kbd chip fallbacks,
  tooltip strings, `aria_keyshortcuts`, and `SHORTCUT_LINES` — no `accessibility_label` /
  `aria_label` key text and no inline label text.
- Every key still works (F5, Shift-F5, Ctrl-T, Ctrl-1..6, Ctrl-F, Ctrl-N, Ctrl-O, Ctrl-K,
  Ctrl-E, Delete, Ctrl-U, Ctrl-R, Ctrl-V, Ctrl-.).
- Hovering any affected control shows the key.
- `cargo test --locked` green.
