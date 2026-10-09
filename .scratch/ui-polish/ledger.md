# Ledger: ui-polish implementation

Base: `6b3ed87` (spec + tickets 01–11 + fresh screenshots).
Spec: `.planning/spec-ui-polish.md`. Tickets: `.scratch/ui-polish/issues/`.

Worktrees (isolated — all tickets share the `src/main.rs` monolith):
- `wt-01` → branch `impl/01-library-rows` → ticket 01
- `wt-02` → branch `impl/02-toolbar-menu-a11y` → ticket 02
- `wt-03` → branch `impl/03-search-facets` → ticket 03
- `wt-05` → branch `impl/05-honest-footers` → ticket 05

Integration: merge branches into main worktree in order 05, 02, 03, 01
(smallest blast radius first), verify trio after each merge.

## Matrix

Lane | Parallel? | Write surface | Risk | Verification
01 rows | worktree-isolated | render_item/text fns + headless tests | medium | cargo test --locked
02 toolbar/menus | worktree-isolated | render_toolbar region | medium | cargo test --locked
03 search/facets | worktree-isolated | filter-band region | medium | cargo test --locked
05 footers | worktree-isolated | dialog_footer + per-dialog footers | medium | cargo test --locked

## Lines

- Task 05: complete (commits 0fa693b, tests: `cargo test --locked` in wt-05 → 768 passed/0 failed; merged 10c2cc8).
- Task 03: complete (commits 9cae08e, tests: `cargo test --locked` in wt-03 → 766 passed/0 failed; merged f61f065).
- Task 01: complete (commits 6d9bc81, tests: `cargo test --locked` in wt-01 → 766 passed/0 failed; merged 4641726).
- Merged-tree verify: `cargo fmt --check` clean, `cargo clippy --all-targets --locked -- -D warnings` clean, `cargo test --locked` → 774 passed, 0 failed.
- Task 02: in progress (interim: red tests added, fix pending); merge on completion.
- Task 08: complete (commits be36c12, tests: `cargo test --locked` in wt-08 → 778 passed/0 failed; merged 64e5f57).
- Task 06: complete (commits 78fd191 tests-only, tests: `cargo test --locked` in wt-06 → 777 passed/0 failed; merged 7487939).
- Merged-tree verify (01+03+05+06+08): `cargo fmt --check` clean, clippy clean, `cargo test --locked` → 781 passed, 0 failed.
- Task 07: complete (commits d4e9155, tests: `cargo test --locked` in wt-07 → 780 passed/0 failed; merged f3d896d).
- Merged-tree verify (01+03+05+06+07+08): `cargo fmt --check` clean, clippy clean, `cargo test --locked` → 787 passed, 0 failed.
- Task 04: complete (commits dabf431, merged f406caf; Usage GroupBox → compact hairline line, same id/text).
- Merged-tree verify (01+03+04+05+06+07+08): `cargo fmt --check` clean, clippy clean, `cargo test --locked` → 790 passed, 0 failed.
- Task 02: complete (commits 23c9654, merged 72dda24; agent's final report lost, evidence = merged-tree trio below).
- Merged-tree verify (01+02+03+04+05+06+07+08): fmt clean, clippy clean, `cargo test --locked` → 793 passed, 0 failed.
- Task 10: complete (commits 632d259, tests: `cargo test --locked` in wt-10 → 798 passed/0 failed; merged b94c035).
- Merged-tree verify (01+02+03+04+05+06+07+08+10): fmt clean, clippy clean, `cargo test --locked` → 798 passed, 0 failed.
- Task 09: complete (commits a7d336b, merged acd3b0b; 150ms opacity motion on inspector/bulk-bar, 6 t09 tests).
- Merged-tree verify (all 10 build tickets): fmt clean, clippy clean, `cargo test --locked` → 804 passed, 0 failed.
- Task 11: complete (screenshots recaptured on final build, PRODUCT.md evidence updated).
- Sweep done: tickets 01–11 all merged; final trio: fmt clean, clippy clean, `cargo test --locked` → 804 passed, 0 failed.
- Manual-review debt (human eyes): row-tooltip hover pass; facet-trigger live read; logs-footer + interval-caption visual sign-off; reduced-motion traversal.
- Manual-review debt: ticket 01 row-tooltip hover pass; ticket 03 facet-trigger live-window read; ticket 05 logs-footer visual sign-off.

## Live app review (driver walkthrough, 2026-10-08)

Launched `target/debug/manyproxy.exe` on the seeded demo DB and drove every surface.

Verified live and good:
- Rows: 3-fact meta + 12px lamp (01). Hover tooltip carries group `europe` and
  `enabled` — facts absent from the visible meta line. Needed a fronted window
  plus real hover dwell; posted mouse-moves do not trigger it.
- Toolbar: single row at 880px (02). Facet menus show counts
  (`trojan (1)` / `vless (2)`), trigger renames to `Protocol: trojan (1)`,
  `Clear facet filters` appears and resets, status bar echoes
  `protocol: trojan` / `search: "berlin"` (03).
- Bulk bar: `1 checked (of 1 visible)` (10). Detail pane fits at 880px (04).
- Logs dialog: `Copy visible` vs `Apply filter`, Cancel/Apply verb labels,
  redaction stated verbatim, filter round-trips across reopen (05, 07).
- Add dialog: inline `vless needs uuid.`, port budget
  `3 of 499 in use (10801-10999, spill 18081-18380)`, LAN hint on leaving
  loopback (06). Subscriptions: grouped interval radios + honest empty (08).

Two real defects found (both outside what the headless seams assert):

1. Inspector body clips at the DEFAULT window height. At 1000x700 the
   Presets tab loses the operate row (Enabled/Start/Stop/Test/Ping/Logs/
   Copy URLs) and the Health tab loses its actions; at the 880x620 minimum
   both lose more. The pane IS `overflow_y_scroll` (main.rs:11276) so the
   content is reachable in principle, but no scrollbar paints and injected
   WM_MOUSEWHEEL never reached GPUI's scroller, so scrollability is
   unconfirmed. Ticket 04's "detail pane fits minimum window" does not hold
   for the Health and Presets tabs. Content needs ~251px; pane gets ~242px.

2. `history_line` returns a bare `div()` when there are fewer than 2 latency
   samples (main.rs:11096-11098), so the History GroupBox paints a titled box
   with a silent empty body. Inconsistent with the honest empty states the
   sweep added to Logs (`No log lines for this filter yet`), Subscriptions
   (`No subscriptions yet`), and the detail pane (`Select a configuration`).

False alarms ruled out (were downscale artifacts, not bugs): footer button
reads `Logs` not `Loass`; traffic order is canonically `↓ rx · ↑ tx`
(main.rs:3532), which is what renders.

Not verified live: reduced-motion traversal (needs an OS-level toggle; covered
by the `motion_fast_duration` unit seam returning `Duration::ZERO`).

### Fixes (both defects closed)

1. **Operate row pinned.** `render_detail_actions` was the last child INSIDE
   the scrollable `config-detail-body`, so a tall tab pushed Start/Stop/Test/
   Ping/Logs/Copy URLs past the pane's bottom edge at the 240px panel. Moved
   it out as a sibling below the scroller in `render_detail_selected`; the
   three `render_inspector_*` builders no longer render it. The pane body now
   scrolls its own sections and the controls stay painted at any window size
   and any divider position.

   Raising the panel start 240 -> 290 was tried first and rejected: the
   headless harness ignores `resizable_panel().size()` (the four alert/preset
   tests failed identically at both sizes) and live it cost a half-clipped
   third library row. The panel stays at 240.

   Instead, each pane now leads with its own subject above the endpoints
   table — the health verdict and its `config-health-alert` failure evidence
   on Health, the preset line and attach/detach on Presets. Both were buried
   under three endpoint rows before; that is also what had pushed
   `config-health-alert` and `preset-detach-inline` below the fold.

2. **History speaks.** `history_line` returned a bare `div()` under two
   samples, painting a titled box with a silent body. Now returns
   `config-history-empty` — "no health history yet - press Test to record a
   probe", or "1 probe recorded - no trend yet (Nms)" for the single-sample
   case so the number is not lost.

New test `operate_row_stays_pinned_on_every_inspector_tab` walks all three
tabs asserting `detail-operate` renders, sits inside the 700px frame
(unclipped), and carries all seven controls, plus the History empty state.
The History module itself is asserted by presence, not paint - it sits below
the fold in the 240px panel and scrolls, same contract as the operate row in
`t04_detail_block_shelf_packs_modules_with_operate_row`.

Verify trio on the fix: fmt clean, clippy clean, `cargo test --locked` ->
**805 passed, 0 failed**. Re-verified live at 1000x700 and 940px: operate row
pinned on all three tabs, 3 library rows intact, History speaks.
`assets/screenshots/library.png` recaptured on the fixed build.
