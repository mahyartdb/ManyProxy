# 45: Subs intervals + history sparkline + usage + logs export

**What to build:** Controllable subscriptions (per-sub interval, visible last-refresh, kill-switch, refresh-all-due), trend-visible health (history table with bounded retention + detail sparkline), manageable usage (buckets, reset, status totals), and logs that can leave the app (save/copy/level/tail).

**Blocked by:** 40-store-v5-foundation (needs `interval_secs` + `health_history` tables).

**Owns:** sqlite

**Status:** ready-for-agent

- [ ] Scenario: sub with custom interval → Action: set interval, wait for due tick → Expected: `due_for_refresh` honors it; row shows last refresh time; global kill-switch + Refresh-all-due work. Verification: automated test. Priority: required.
- [ ] Scenario: row with probe history → Action: open detail → Expected: mini time-series sparkline renders from bounded history (30–90d retention enforced in-tx). Verification: manual UX review. Priority: required.
- [ ] Scenario: accumulated usage → Action: reset row counters → Expected: zeroed with confirm; status bar shows summed totals. Verification: automated test. Priority: important.
- [ ] Scenario: visible log lines → Action: Save + Copy-all + level filter + tail toggle → Expected: file matches visible lines via export-path pattern; tail pauses correctly. Verification: manual UX review. Priority: important.
- [ ] Prohibited side effect: latest-snapshot fast path never pays a history scan; no auto-refresh without consent.
