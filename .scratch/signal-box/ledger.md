# Signal Box ledger (append-only — agents add lines, never rewrite)

Task 01: complete (commits 7f5d475..197e0d8, tests: cargo test --bin manyproxy → 686 passed, 0 failed)
- Slice 1 (cd24a4c): bulk_label always `N checked (of M visible)`; 10 pinned asserts updated to the new wording.
- Slice 2 (5f4251e): checked rows get 2px accent rim + `row-check-chip-<id>` numeral (checked_position, library order); wash stays selection-only; checkbox accessible labels name the bulk set (list + table spike).
- Slice 3 (197e0d8): detail header echoes `Acting on: <name>` (`detail-acting-on`, acting_on_label); F5/detail Start already act on selected_config (verified, unchanged).
- New tests: t01_checked_positions_and_selection_stay_distinct, t01_selection_grammar_paints (chips + bulk counts + acting-on paint), t01_detail_header_names_acting_row. Selections row_ (19), bulk (2), detail (2) all green; cargo check clean.
- Ruling: bulk-label wording changed despite preserve-pinned-strings — the ticket explicitly demands `N checked (of M visible)` with both counts always; the old `N selected` wording was the ambiguity under repair — cost if wrong: wording-only revert, no other ticket touches the bulk label.
- Ruling: no per-row amber starting lamp — starting is a global transient flag with no per-row state; the Starting phase already surfaces as the `Starting…` button label + spinner, and dots/badges verified in agreement (green serving, dark stopped, red dead) — cost if wrong: a future ticket with per-row start tracking can add the lamp without touching this grammar.
- Ruling: checkbox accessible labels changed from `Select <name>` to `Check <name> for bulk actions` / `Checked <name> (bulk action target)` — scenario 3 demands checked-vs-selected distinguishable without color; no test pinned the old strings (grep-verified) — cost if wrong: string-only revert.
- Ruling: count chip shows the 1-based position in the checked set (library order) — the ticket's `count chip` read as an ordinal so multiple checks stay distinguishable as text — cost if wrong: chip-content-only change.
