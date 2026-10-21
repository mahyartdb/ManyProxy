# T-06 — Keyboard focus survival + visible focus ring

Owner: implementer. Depends on: T-04 (overlay teardown).

## Defect

After an overlay (Settings, Add, Import, palette) is dismissed with `Esc`, the shell can hold
no focused element: `Ctrl-N`, `Ctrl-O`, `Ctrl-F` and typing silently no-op until a click.
`Ctrl-N`/`Ctrl-O` also no-op silently while an overlay is open. There is one `track_focus`
in 58k lines (`src/main.rs:27868`) and custom controls paint no focus treatment.

## Fix

1. One overlay-dismissed helper, called at the END of every dismiss path (Esc, Cancel,
   commit, backdrop, close-confirm). Reclaim focus only when
   `!overlay_open(window, cx) && window.focused(cx).is_none()`, then focus `self.focus`.
   The end matters: `run_palette_op` (`17484-17520`) closes all dialogs and opens a second
   overlay in the same handler; reclaiming earlier steals focus from it.
   (gpui 0.3.6 exposes `Window::focused(cx)`; use the kit equivalent if it offers
   `focused_handle()`.)
2. A refused shortcut must not be silent: route the key into the overlay (Esc closes the
   top layer, Enter commits) or toast `Close the open dialog first`.
3. `Esc` pops exactly one overlay layer. Advanced over Add: the first Esc closes Advanced
   and Add stays; the second closes Add. Pinned around `30078-30091`; do not "close every
   modal".
4. Visible focus uses the kit focus ring (`DESIGN.md:207`) with real focus handles on the
   custom controls. No hand-rolled 2px accent rim and no second color.
5. List: Up/Down traverses once focused; the selected row sets `ListItem::selected(true)`,
   so UIA reads it (selection is paint-only today).

## Corrections (reviewer)

- Added the exact predicate and the "evaluate at the end" rule; the palette's second overlay
  is why.
- "Esc dismisses every modal" was wrong; it pops one layer.
- The focus ring is the kit's, not a custom accent rim.

## Acceptance

- Settings → Esc → Ctrl-N opens Add without a click.
- Palette opened over an overlay: no silent no-op; focus lands in the new overlay, not the
  shell.
- Esc on Advanced leaves Add open; the next Esc closes Add.
- Every focusable control paints the kit ring when tabbed to.
- UIA reports the selected row as selected.
- Tests: focus reclaim after dismissal; one-layer Esc; palette second overlay.
