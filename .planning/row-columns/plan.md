# Plan: one-line rows with toggleable columns + dirty-gated Start

All code in `src/main.rs` (+ `src/store.rs` meta accessors), tests beside
it. Order is bottom-up: model first, render second, seams third, UI
fourth, gate fifth, docs sixth. Band order, toolbar, dialogs, and all
non-row behavior frozen.

1. **R1 columns model + persistence.** New `RowColumns` bitflags-ish struct
   (6 bools, `Default` = safe subset, `parse/display` CSV round-trip) +
   `Store::library_columns()` / `set_library_columns()` over the `meta`
   table mirroring `appearance()` (`src/store.rs:6869`). AppView loads it
   in `with_store`, falls back to defaults on parse failure. Pure unit
   tests (parse/display/defaults/round-trip/persistence). No UI yet.
2. **R2 one-line render.** Rewrite the `render_item` card: single header
   line only (lamp, checkbox, leading check-chip, name, optional column
   cells, bind/port inputs with dirty dot, Start/Stop, Edit); delete the
   meta/badge arms and `row-meta-*` ids; keep `row-dot/name/bind/port/
   serve/edit` ids, tooltip (full route), collision/guard warnings, draft
   Enter/Esc, flush-before-act paths. Update painted/containment tests to
   the one-line shape in-ticket.
3. **R3 text seams.** Retire `library_row_dense_lines_with_upstream` from
   render (keep `library_row_full_route` for tooltips, keep
   `serving_badge` + its test for the detail header); rewrite
   `t01_dense_meta_carries_at_most_three_facts` onto the column model;
   audit the sibling `*_lines*` fns at ~8573/8599 (keep or remove with
   their tests, no orphans). SearchBlob/matching untouched.
4. **R4 columns picker UI.** `Tools` menu item `Row columns…` + palette
   twin + dialog with 6 checkboxes reflecting live state; toggle writes
   meta immediately and repaints; unknown stored tokens ignored. Tests:
   dialog paints, toggle flips a column + persists (re-read meta),
   defaults on corrupt value.
5. **R5 dirty-gated Start.** Delegate helper `endpoint_is_dirty(id)` (pure
   compare, mirrors `revert_row_inputs` without mutating); gate at top of
   `start_row` with the honest toast; row-serve button stops
   flush-before-start (keeps flush-before-edit); bulk start skips dirty
   rows and names them in its summary toast. Tests: dirty+Start toasts
   and launches nothing; clean+Start launches; bulk mixed set.
6. **R6 docs + verify + PR.** DESIGN.md row section rewritten (one line,
   column table, dirty dot, gate); screenshots retaken (seeded demo DB);
   full trio + live 1000x700/880x620; PR.

## Rollback

One commit per ticket; each reverts cleanly. R2/R3 move together if
reverted (render + its seam tests).
