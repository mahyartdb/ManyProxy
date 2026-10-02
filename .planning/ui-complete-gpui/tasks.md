# Task list: ManyProxy UI completion (gpui-kit)

Run: `qmode-20261002-ui-complete-f095` · Decision: `.planning/ui-complete-gpui/decision.md` · Gate: awaiting post-tickets kill/go.

Scope (locked in triage): close `.scratch/ui-production-pass.md` items 10-12 **+** deliberate gpui-kit component adoption where it genuinely fits. Behavior changes allowed where flagged, tests-first. Out: engine/health/import/store/tray logic, proven README claims, no-fit kit components.

## Pinned seams — every ticket must keep these green or justify a conscious update in the ledger

- Strings: `serving_badge` (exact `assert_eq` at 16820), `library_row_lines*` (4/5-line length asserts 15932-15942 + content), `search_count_text`, `no_matches_text`, `empty_library_text`, `RowPhase::start_label`, `bulk_label`, `no_selection_message`, status-bar text fns.
- Element ids (assert_painted): `config-list`, `library-row-<id>`, `config-detail`, `detail-serving`, `facet-bar`, `facet-group/protocol/status`, `groups-dialog`, `clear-facets`, `bulk-*` (8), `library-empty`, `clear-search-hint`, `empty-library-ctas`, `search-count`, `config-endpoints/health/usage/history`, `status-count`, `engine-status`, `health-status`, all toolbar + dialog footer ids (audit loop 15526), `toggle-enabled` must stay **absent**.
- `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test` (660 baseline) after every ticket.

## Tickets

- [ ] **T01 — Theme foundation + themed row cards (pass item 10a).** Read `ActiveTheme` in `Context` scope (0 usages today). Give `ListItem` rows (`render_item` 796) a themed surface, border, and selected highlight; group the toolbar/drop-hint/status-bar/facet/bulk bars on the same theme tokens (no hardcoded colors — crate has zero today, keep it that way). Pinned: `library-row-*` ids, row line count/strings, `ListItem.selected` behavior. Red-first: a headless test asserting a themed row container id paints, failing before the markup lands.

- [ ] **T02 — Colored status dot + Badge/Tag in detail header (pass item 10b).** Inline colored dot next to the badge line in `render_item` (dot is a sibling element — `serving_badge` string seam untouched, exact-copy tests stay). Detail header: `detail-serving` becomes a `Badge`; protocol/group chips become `Tag`s (9194-9197, only id/text-presence asserted). Red-first: dot id test (`row-dot-<id>`, present iff serving) failing pre-impl.

- [ ] **T03 — Facet pickers as Popover/Select (pass item 11, behavior change).** **Tests first:** headless interaction tests for open → pick → facet applies → clear, over the existing delegate seams (`set_facet_group/protocol/status`, `apply_filter` untouched). Then replace `open_facet_*_dialog` triad (9794/9852/9912) with `Popover`-triggered `Select`/`Combobox` popovers on buttons `facet-group/facet-protocol/facet-status`. **Fallback: if popover interaction fails headless, keep the dialogs and record the miss in the ledger — do not ship a flaky picker.** Ids `facet-bar` + three buttons stay painted.

- [ ] **T04 — Empty states via `Empty` component.** Detail empty arm (9106-9141) and both `render_empty` arms (870-914) built on `Empty` (`header`/`content`), keeping ids `config-detail`, `library-empty`, `clear-search-hint`, `empty-library-ctas` and the pinned text fns. CTAs (`detail-add`, `detail-import`, `empty-add`, `empty-import`) remain real Buttons.

- [ ] **T05 — Detail sections: `GroupBox`/`DescriptionList` + dead-health `Alert`.** Sections `config-endpoints/health/usage/history` get themed `GroupBox` (or `DescriptionList` where label/value rows read better) without losing their ids. When the selected row is dead/crashed, the health section renders an `Alert` with the existing pinned crash copy (`honest_engine_feedback_copies` 16877-16950). LAN hint id untouched.

- [ ] **T06 — In-flight feedback: Spinner/`.loading`.** `engine-start`/`engine-stop` show `.loading(true)` while `start_in_flight`; `health-test`/`health-ping` while `probe_kind_in_flight` (5303). Pinned: button ids, `RowPhase::start_label` text, `toggle-enabled`-style absence rules. Red-first: headless test asserting loading state during in-flight (via the existing test seams), not sleeps.

- [ ] **T07 — Static hint duplication → interactive controls.** `clear-search-hint` (893) and `empty-library-ctas` (910) render as static text duplicating the real `clear-search` / `empty-add` / `empty-import` buttons. Make them working controls (or fold them into the real buttons) while keeping both ids painted (17405, 17426) — a hint that looks clickable but isn't is incomplete UI. Tests: click `clear-search-hint` clears the query; `empty-library-ctas` CTA opens add/import.

- [ ] **T08 — Shortcut hints via `Kbd` (labels unchanged).** Toolbar/detail button labels keep their exact pinned strings (`Add (Ctrl-N)` etc. — README-claimed); the keystroke part renders as a `Kbd` chip inside the label/tooltip where the kit allows without changing the accessible/pinned text. If `Kbd` forces a label-text change, skip and record — README claims stay authoritative.

- [ ] **T09 — README truth fixes (claims that are currently false).** (a) `toggle-enabled` does not exist and is test-asserted-absent — remove from README lines 17/84. (b) Toolbar shortcut-claim table lists old `Start all (F5)`-style labels; align with shipped `Start selected (F5)` etc. (c) Stale `src/main.rs:242` dialog_footer ref → `:953`. Only false claims get touched; everything proven stays.

- [ ] **T10 — Fresh live screenshots (pass item 12).** `cargo run` via cua-driver, capture: library (themed rows + facet bar + bulk bar + detail pane), Add dialog, Import dialog, Subscriptions dialog. Replace `assets/screenshots/*.png`, update README line 5 count/captions if surfaces are added.

- [ ] **T11 — Final verification + audit.** Full `cargo fmt --check` + `cargo clippy --all-targets` + `cargo test`; live window (`MainWindowTitle=ManyProxy`, `Responding=True`); re-read `.scratch/ui-production-pass.md` exit predicate; grep `src/` for hardcoded colors (must stay 0); every task box checked or its skip recorded in the ledger with a reason.

## Explicitly not doing

No-fit kit components (Avatar, Table, Sidebar, Tab, Dock, Command, Sheet, ContextMenu, Tree, Form/Field swap, SettingPage, DatePicker, etc.) — inventory C. No engine/health/import/store/tray logic changes. No new features beyond UI completeness. No `ToggleGroup` substitute for the pinned enabled `Switch`.

## Order & dependencies

T01 → (T02, T04, T05, T06, T07, T08 theming-dependent) · T03 independent (tests first) · T09 after any label-touching ticket resolves (T08 may cancel label edits) · T10 after T01-T09 · T11 last. One ticket = one commit; ledger line per ticket with test exit codes.
