# 45: Subs intervals + history sparkline + usage + logs export

**What to build:** Controllable subscriptions (per-sub interval, visible last-refresh, kill-switch, refresh-all-due), trend-visible health (history table with bounded retention + detail sparkline), manageable usage (buckets, reset, status totals), and logs that can leave the app (save/copy/level/tail).

**Blocked by:** 40-store-v5-foundation (needs `interval_secs` + `health_history` tables).

**Owns:** sqlite

**Status:** implemented (worktree `ManyProxy-45`, branch `ticket-45`, commit pending) — 529 green (517 baseline + 12 new), fmt + clippy clean.

- [x] Scenario: sub with custom interval → Action: set interval, wait for due tick → Expected: `due_for_refresh` honors it; row shows last refresh time; global kill-switch + Refresh-all-due work. Verification: automated test (`t45_sub_interval_honored_by_due`, `t45_kill_switch_gates_auto_not_manual`, `t45_interval_and_last_refresh_format`). Priority: required.
- [x] Scenario: row with probe history → Action: open detail → Expected: mini time-series sparkline renders from bounded history (30–90d retention enforced in-tx). Verification: automated headless (`t45_history_retention_window_keeps_recent`, `t45_sparkline_latencies_newest_capped`) + manual UX review (steps below). Priority: required.
- [x] Scenario: accumulated usage → Action: reset row counters → Expected: zeroed with confirm; status bar shows summed totals. Verification: automated test (`t45_reset_traffic_zeroes_counters`, `t45_usage_reset_zeroes_and_totals_sum`). Priority: important.
- [x] Scenario: visible log lines → Action: Save + Copy-all + level filter + tail toggle → Expected: file matches visible lines via export-path pattern; tail pauses correctly. Verification: automated headless (`t45_logs_save_matches_visible`, `t45_log_level_cycles_with_labels`, `t45_log_level_labels`, `t45_logs_path_pattern`) + manual UX review (steps below). Priority: important.
- [x] Prohibited side effect: latest-snapshot fast path never pays a history scan (separate `health_snapshots` table; `load_health_snapshots` untouched — pinned by `t45_history_retention_window_keeps_recent`); no auto-refresh without consent (kill-switch fail-closed on broken read; manual Refresh-all-due is explicit consent; ticks never flip the switch).

## Manual UX review steps (reviewer: run the app, isolated `MANYPROXY_DB` only)

1. Subs dialog → each row shows `last refresh: … · every …`; add/edit forms show the interval picker (`always 1h 6h 12h 24h 7d`, current in brackets); header shows `Auto-refresh all` switch + `Refresh all due (N)` button. Toggle the switch off → background due pass stops; `Refresh all due` still works.
2. Select a row with probe history (Test/Ping first) → detail pane shows `N samples · latest Xms` + a native line-chart sparkline (no new deps). Fresh rows show no history section (progressive disclosure — keeps the virtual list rows painted).
3. Detail pane shows `usage down X · up Y` + `Reset usage` → confirm dialog → counters zero, toast confirms; status bar shows `↓ X · ↑ Y` sums.
4. Logs dialog → `Level: all` cycles all → error → warn → info → debug → all; `Tail: on/off` toggles newest-200 vs head-200; `Save` writes visible lines to `manyproxy-logs.txt` (or `MANYPROXY_LOGS`); `Copy all` copies the same text (toast counts lines).

## Rulings

- Sparkline spike: gpui-component 0.6.6 ships native `chart::LineChart` (Y = `f64` only per `Sealed` impl) — in-app sparkline uses it; **no plotters dep** (nothing in this ticket needs PNG export).
- Retention: startup 90d GC (ticket-40 seam) + single-tx `prune_health_history_before` = "enforced in-tx, never VACUUM"; 30–90d is the supported window via `gc_health_history(days, now)`. No daily timer (ticks stay minimal per plan).
- Usage "buckets" = persisted down/up counter pair (no time-bucket table in v5 — out of minimal scope); kill-switch rides `meta.subs_auto_refresh` (export/restore allow-listed, strict `"0"`/`"1"` validation).
- `add_subscription` signature untouched (20 existing callers); interval commits via `set_subscription_interval` after insert/update.
- `commit_subscription_interval` (re-read-then-set row path) removed before commit — dead code, picker commits via add/edit flows.
- `LogLevel::from_label` kept and used by the cycle implementation (not test-only).
