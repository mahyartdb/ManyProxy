# T-27 — Header sort pickers (health/traffic key menus)

- **id**: T-27
- **title**: Header sort pickers — latency/DL/UL/out-IP and traffic total/down/up; persisted beside `library_columns`
- **epic**: E6 — UI affordance pack
- **size**: M
- **depends-on**: T-23 (the filter/sort state family they persist beside)
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E6 T-27 line; gap §4.3 U6 (Throne per-column header menus: TestResult → latency/DL/UL/out-IP; Traffic → total/down/up — §B.1; "persist chosen sort in library_columns-adjacent meta"; "Mechanical ListState work"); verified as-built: only type/security sort toggle exists (`type_security_sort` at main.rs:812 применение via `compare_type_security`, store.rs:643; `security_rank`/`compare_type_security` referenced at 812–814), meta persistence pattern `META_LIBRARY_COLUMNS` (store.rs:6961).
## Why

Sort is the second answer after filter for "find the row I care about": by latency, by throughput, by exit identification. Today's binary toggle can't rank by anything quantitative. Seat the pickers in the header-family UX Throne solves with right-click menus — same breathing, ManyProxy's own labels.

## What

1. **Sort model** (meta `library_sort`, JSON `{key: <enum>, direction: <asc|desc>}` — additive; persisted via store.rs meta family like `META_LIBRARY_COLUMNS`, alongside but separate from it):
   - keys: `none` (default — the stable unsorted order today), `latency` (latest URL-test sample asc; unknowns/dead pinned to the end, stable by row id), `download`/`upload` (by per-row current traffic counters), `out_ip` (lexical on the resolved out-IP text; unresolved pinned last), `traffic_total` / `traffic_down` / `traffic_up` (on per-row accumulated rx/tx within the current counting window),
   - `none` preserves byte-stable ordering (`apply_filter`'s unchanged base order).
2. **`apply_filter` integration** (main.rs:798): after the port/facet predicates (compose cleanly with T-23), sort per the model when `key != none`; the existing `type_security_sort` toggle **retires into the picker** as key `type_security`(replaced — the old toggle's behavior is the `type_security` key now, direction fixed as today) to keep one sort concept, not two competing ones. Any test pinned to the toggle keeps passing through the equivalent key.
3. **Picker UI**: each sortable column header (the health/status column and the traffic column headers in the visible library list columns from `library_columns`) gains a header menu (right-click; the same menu affordance family the column-width config uses) offering the key list + direction toggle; current choice carries the marker.
4. **Stale tolerance**: sorts are computed on render from existing per-row maps (health/traffic mirrors) — no extra polling; unknowns sort deterministically last (stable in a row-id secondary order) so lists don't jump between ticks.

## Files to touch

- `src/main.rs` — sort model + `apply_filter` integration, header menus, tests
- `src/store.rs` — `META_LIBRARY_SORT` const + typed accessor + tests

## Acceptance criteria

- [ ] Keys sort by the named data; directions togglable; unknown/missing values deterministic at the end (stable across ticks; no flicker).
- [ ] `type_security` key behaves exactly as today's toggle did; toggle-less regressions pinned by existing ordering tests.
- [ ] Choice persists across restarts (meta round trip); `none` key returns the pre-feature order byte-stably.
- [ ] Composes with port/facet filters (T-23) without interference.
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t77_`, verified unused): `t77_sort_keys_order_correctly`, `t77_unknown_values_sort_deterministically_last`, `t77_type_security_key_matches_old_toggle`, `t77_sort_model_meta_round_trip`, `t77_header_menu_renders_pickers`, `t77_sort_composes_with_facets_and_port_filter`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData / No-migration / Plaintext / Hijack / Sidecars: N/A.
- [x] No system mutation: presentation + one additive meta key.
- [x] Clipboard / Redaction: N/A.

## Out of scope

- Column reordering UI (T-27 is sort, not layout).
- Multi-key sorts (single key + direction; Throne is the same).
- Sparkline or per-column custom renderers beyond picker affordance.
