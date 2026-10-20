# T-32 — Window geometry persistence

- **id**: T-32
- **title**: Window geometry persistence — size/position restore via meta (Throne §G.2 borrow)
- **epic**: E6 — UI affordance pack
- **size**: S
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E6 **T-32 line** + gap §4.6 P2 ("ManyProxy persists columns/widths but not window geometry [C]; Borrow the meta keys … 1 function in store + restore-on-`with_store`") — note the hand-off: where the gap doc mentions Throne's `geometry_base64`-style meta, ManyProxy stores plain JSON; verified as-built: fixed `WindowBounds::Windowed(Bounds::new(point(0,0), size(1000,700)))` (main.rs:170–181 `app_window_options`), size persisted today only for `conn_column_widths` + `library_columns`.
## Why

Power users park the window on a second monitor; closing it and coming back to the default rectangle is the annoyance Throne's persistence kills. "One function in store + restore-on-with_store" — cheap durability, no migration, additive meta.

## What

1. **Meta key** (store.rs family): `geometry` — JSON `{x, y, width, height}` (logical points).
2. **Save**: on orderly exit paths where the running snapshot save already happens (`snapshot_running_set` invocation sites / clean-quit paths, e.g. `confirm_close_stop` at main.rs:13101 and `quit_from_tray` at 13252) capture the current window bounds via the gpui window handle and `store.set_meta(META_GEOMETRY, ...)`; only write when sane (min size ≥ `window_min_size` 880×620 clamp; position clipped to ≥ `(-32000,-32000)` sanity or set-get time monotinized — keep it simple: clamp width/height to the min bounds; clamp x/y into a -999..9999 sane box).
3. **Restore**: `app_window_options()` (main.rs:170) gains an optional override — when a valid stored geometry exists, seed `WindowBounds::Windowed` from it; the fixed 1000×700 stays the **first-run default** (absence = exactly today's behavior).
4. No minimums changes; no maximized-state tracking (Windowed only; maximize is a session state — out of scope).
5. Multi-monitor: restoring to a monitor that has since been disconnected → gpui/OS clamps as it does today for any window; do not special-case (no display enumeration).

## Files to touch

- `src/store.rs` — `META_GEOMETRY` const + typed get/set + tests
- `src/main.rs` — options seeding in `app_window_options` + save hooks on the orderly-exit paths + tests

## Acceptance criteria

- [ ] Move/resize → quit → relaunch restores the rectangle; no stored meta → defaults indistinguishable from today (test pins zero-geometry case).
- [ ] Sane clamps: width/height never below 880×620; out-of-range x/y clamps within the sanity box; corrupt JSON → defaults (no crash).
- [ ] Save happens on the same clean paths the running snapshot already uses (crash-tolerance matches snapshot semantics — killed process = last save, acceptable).
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t82_`, verified unused): `t82_geometry_meta_round_trip`, `t82_app_options_seeds_stored_geometry`, `t82_default_bounds_when_meta_absent_or_corrupt`, `t82_geometry_clamps_respect_mins`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData: N/A (meta key only).
- [x] No-migration: additive; absent key = today.
- [x] Plaintext / Hijack / Sidecars / Clipboard / Redaction: N/A.
- [x] No system mutation: no registry writes (this is app-internal meta, same as columns).

## Out of scope

- Maximized/fullscreen/kiosk state persistence.
- Per-monitor DPI profiles, window-span preservation.
- Tray-window geometry (tray is an icon).
