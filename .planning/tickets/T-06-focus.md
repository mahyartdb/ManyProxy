# T-06 — Keyboard focus survival + visible focus ring

Owner: implementer. Depends on: T-04 (overlay teardown).

## Defect

After an overlay (Settings, Add, Import, palette) is dismissed with `Esc`, the shell holds no
focused element: `Ctrl-N`, `Ctrl-O`, `Ctrl-F` and typing all silently no-op until the user
clicks something. Independently, `Ctrl-N` / `Ctrl-O` no-op silently when an overlay is already
open. There is exactly one `track_focus` in 58k lines (`src/main.rs:27850`) and no
focus-ring styling anywhere.

## Fix

1. Root shell element (the one carrying `track_focus(&self.focus)` at `27850`) must reclaim
   focus whenever an overlay is dismissed — every path: Esc, Cancel, commit, backdrop click,
   and the close-confirm. Put the reclaim in one place (an overlay-dismissed helper), not at
   five call sites.
2. When a shortcut is refused because an overlay is open, do not swallow it: either route the
   key into the overlay (Esc closes, Enter commits) or toast
   `Close the open dialog first`. Silent no-op is the defect.
3. `Esc` must dismiss every modal, including the Add dialog (currently it does not).
4. Visible focus: add one theme-level focus treatment (2px accent rim, drawn inside the
   control's radius) and apply it to buttons, row cards, inputs, tabs, and the checkbox.
   Use `is_focused`; do not invent a second color — `theme().accent` or `selection-rim`.
5. Tab reachability: the library list must be traversable with Up/Down once focused, and the
   selected row must set `ListItem::selected(true)` so assistive tech reads it (currently
   selection is paint-only).

## Acceptance

- Open Settings → Esc → press Ctrl-N: the Add dialog opens without a click.
- Open the palette with an overlay already open: no silent no-op.
- Every focusable control paints a visible ring when tabbed to.
- UIA reports the selected row as selected.
- Tests: one for focus reclaim after overlay dismissal, one for Esc dismissing Add.
