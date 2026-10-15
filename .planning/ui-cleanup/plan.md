# Plan: UI cleanup — library shell de-clutter

All edits in `src/main.rs` (monolith — do not split) plus `DESIGN.md`.
Each ticket is a surgical flex/spacing edit; order is bottom-up so later
tickets compose on earlier ones. Behavior, ids, strings, and band order are
frozen; only padding, wrap, widths, and radius change.

## Ticket order and rationale

1. **T1 bands breathing room** (`render_search_count` :21140,
   `render_facet_bar` :21228, `render_bulk_bar` :21343). Add `py_1` to the
   count row and facet bar (bulk bar already has `flex_wrap` + bg + border).
   Bands keep ids/order/content; they just stop touching each other. Lowest
   risk, biggest visual payoff → first.
2. **T2 row header wrap** (`LibraryDelegate::render_item` header :1224-1380).
   Add `flex_wrap` to the header row so bind/port/serve/edit/chip flow to a
   second line at narrow widths instead of crushing the name and clipping the
   numeral chip. Name keeps `flex_1 min_w_0 truncate`. Lamp stays 12px.
3. **T3 endpoints hug values** (`endpoint_row` :20420-20494). Drop the `w_full`
   push on the outer row so the Copy button sits next to its value instead of
   at the far pane edge; keep label `w(px(72.))`, value `flex_1 min_w_0
   truncate`, button `flex_shrink_0`. Eliminates the 400px void, no reorder.
4. **T4 Add/Edit dialog (verified OK, no change).** Live Add dialog on HEAD
   shows all fields legible; the truncation in the user's screenshot came
   from the stale pre-`d0f688b` build. Attempted column `min_w` + wrap broke
   `t11_add_dialog_paints_inside_minimum_frame` (fields pushed out of the
   min frame) and was reverted. No change ships.
5. **T5 master throw radius** (`render_toolbar` :23380-23391). Remove
   `.rounded(px(999.))` so the throw uses the theme button radius like every
   other button. Label/a11y/tooltip/handler untouched.
6. **T6 status bar (verified OK, no change).** Lowering the `min_w` floors
   broke `t10_status_slots_hold_width_across_value_changes` (slots must hold
   fixed widths across value changes) and was reverted. Measured floors
   (210/430/150) fit the 880 minimum with truncate + tooltips. No change.
7. **T7 DESIGN.md tabs paragraphs** (Layout ~:145, Navigation ~:212).
   Document the 6 shipped inspector tabs (Details/Health/Presets/Logs/
   Connections/Traffic) instead of 3; note endpoints render on the overview
   above every pane. No token or rule changes.
8. **T9 one-row filter bar.** The `search-focus` button plus the kit List's
   own search box made three stacked rows for one job (count, facets,
   query). The kit search is disabled (`.searchable(false)`, placeholder
   builder removed); AppView owns one `search_input` rendered inline in
   `facet-bar` beside the `search-count` block, wired to the delegate via an
   `InputEvent::Change` subscription that needs no window (direct
   `query` + `apply_filter`, mirroring `perform_search`). Ctrl-F, Clear
   search, and both empty-state clears sync the box explicitly (`set_value`
   emits no `Change`, so no loops). Esc behavior unchanged (kit never
   cleared the query on Esc). Two tests re-expressed onto the new widget
   (`t03_inline_search_field_focusable`, `t06` focus handle); placeholder
   const and shortcut binding untouched.

## Verification per ticket

- `cargo fmt` (then `cargo fmt --check`), `cargo clippy --all-targets
  --locked -- -D warnings`, targeted `cargo test <substring> --locked`
  (bulk/tooltip/status/painted suites).
- Final: full `cargo test --locked` + fresh-build cua-driver screenshots at
  1000x700 and 880x620 confirming `spec.md` acceptance.

## Rollback

Each ticket is one commit; any single commit reverts cleanly (`git revert`)
without touching behavior or tests.
