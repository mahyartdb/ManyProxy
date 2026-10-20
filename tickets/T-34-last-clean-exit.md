# T-34 — `last_clean_exit` marker + unclean-exit toast

- **id**: T-34
- **title**: `last_clean_exit` meta marker flipped on clean quit; next launch toasts if the previous run carried running endpoints and did not exit cleanly
- **epic**: E7 — Ops marker
- **size**: S
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E7 T-34 line (Q11c; "running.marker file idea REJECTED (SQLite snapshot covers it)" — gap §4.7 K2); gap §4.7 K1's K1b variant ("add the Throne-style warning — 'the previous run carried running endpoints; a crash marker was found' … This adds back Throne's 'did not shut down cleanly last time' without touching the logs constraint"); K1's stderr-stencil write is **REJECTED** (session-only logs constraint stays untouched).
- **verified surfaces**: clean-quit sites `quit_from_tray` (main.rs:13252 — stops engine, clears enabled flags, `snapshot_running_set(&[])`) and `confirm_close_stop` (main.rs:13101 — same trio); snapshot machinery `save_running_snapshot`/`load_running_snapshot`/`clear_running_snapshot` (store.rs:6861–6883, `META_RUNNING_IDS` store.rs:6779); startup snapshot consumption `with_store` (`last_snapshot` at main.rs:11349) + `restore_on_launch` (13953).

## Why

Today a crash leaves (a) last session's running snapshot persisted, and (b) no memory of whether that quit was dirty. A user running endpoints overnight wonders why restore fired or why rows are now disabled. The clean-exit marker answers with one boolean, session-sourced state — no log parsing, no files (SQLite snapshot already carries the state; the *semantic* is the new piece). The crash-time stderr-to-disk stencil idea behind K1 is permanently rejected (session-only logs).

## What

1. **Meta key** `last_clean_exit` (store.rs `META_*` const family at 6777–6982): `"1"` = previous session quit through a clean path.
2. **Flip on clean quit** (the two existing clean-exit sites): `quit_from_tray` (main.rs:13252) and `confirm_close_stop` (main.rs:13101) — immediately after `snapshot_running_set(&[])`, call `store.set_meta(META_LAST_CLEAN_EXIT, "1")`. Both paths already stop-all + clear the snapshot; the flag rides the same transactional writer moment (no extra flush machinery). Also flip on a plain window-close exit with zero running rows when such a path exists (implementer verifies the non-running close path; if the only close is hide-to-tray with running rows and plain exit-with-no-rows is the other case — cover every orderly exit; the invariant is: any orderly exit marks clean; only kill/crash leaves it at "0").
3. **Set to `0` at startup-assume-dirty**: on `with_store` (and at `Store::open*` time for the session, same act as the running-snapshot hydration phase at main.rs:11349 — before restore logic), write `last_clean_exit = "0"` so the invariant becomes: `store.meta == "1"` ⇒ the previous shutdown was orderly.
4. **Toast at next launch** (before `restore_on_launch`, main.rs:13953): after reading `last_snapshot` (main.rs:11349): if the persisted flag at app start (read **before** step 3's write) was `"1"`-absent-or-`"0"`? correctness: read the flag FIRST, then write "0". Condition:
   - flag = clean-absence(`"0"` or missing) **AND** `last_snapshot` is non-empty → push the notification exactly: **"Previous run carried running endpoints and did not exit cleanly."** (session-sourced toast; no dialogs; no log spam).
   - flag clean (`"1"`) → no toast regardless of snapshot shape (snapshot from an orderly *stop-all* exit is empty anyway; a non-empty snapshot + clean flag means auto-started-then-quitting-quickly edge — no toast).
5. **Log-files constraint**: no file touched — session logs stay session-only (`MANYPROXY_LOGS` path untouched; this marker is a store meta value, not a log artifact).

## Files to touch

- `src/store.rs` — `META_LAST_CLEAN_EXIT` const + typed helper + tests
- `src/main.rs` — flip sites (2), startup-read+write, toast + tests

## Acceptance criteria

- [ ] Clean quits (tray Quit, confirm-close-stop, orderly no-rows close) persist "1"; process kill/crash leaves the flag as it stood (the boot-time "0" write makes that "0" unless the previous session ended cleanly).
- [ ] Boot with non-empty running snapshot + unclean flag → exactly the toast above, once; restore-on-launch behavior unchanged in every respect.
- [ ] Boot after clean quit → never toasts, whether snapshot empty or not.
- [ ] No log files written, no new files (the `running.marker` file idea stays rejected).
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t84_`, verified unused): `t84_clean_quit_flags_last_clean_exit`, `t84_boot_assumes_dirt	until_quits_cleanly` (name without the tab: `t84_boot_assumes_dirty_until_quits_cleanly`), `t84_unclean_boot_with_running_snapshot_toasts_once`, `t84_clean_boot_never_toasts_even_with_snapshot`, `t84_no_marker_file_ever_written`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData: N/A (meta value).
- [x] No-migration: additive meta; absent key = unclean-neutral default.
- [x] **Plaintext-always-sealed**: N/A (boolean only).
- [x] Hijack non-concept: N/A.
- [x] **No system mutation**: none — no registry, no files beyond the DB; the session-only logs constraint is explicitly preserved (K1 stencil rejected).
- [x] Clipboard / Redaction: N/A.
- [x] Sidecars app-dir only: N/A.

## Out of scope

- Crash-time stderr/log stencils, minidumps, WER keys (dead; K1 rejected).
- `running.marker` file (rejected — SQLite snapshot covers it).
- Auto-restart/repair behavior changes (AutoRestartConfig untouched).
- Logoff/session-end teardown event handling (Throne WM_QUERYENDSESSION borrow — documented as Partial in the gap matrix; not ticketed here).
