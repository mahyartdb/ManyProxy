# T-29 — Top-9 exits + Other in the traffic dialog

- **id**: T-29
- **title**: Traffic dialog gains Top-9 exits + "Other (n)" aggregate per period — hour-bucket history is PARKED separately
- **epic**: E6 — UI affordance pack
- **size**: S
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E6 T-29 line ("hour-bucket history table PARKED"); gap §4.3 U7 (Throne dashboard = top-9 *apps* — unreachable data-wise because stock clash API yields no process attribution; **explicit decision: top-9 *exits* is the in-job shape**, "per-exit aggregates exist" = `ExitTraffic` sorted); verified: `struct ExitTraffic` (main.rs:11032), traffic dialog render region main.rs:11040–11107 across `TrafficPeriod` {Day,Week,Month,Quarter} (store.rs:522; period label + "N exit(s) · ↓x ↑y" and stacked bar text `traffic_stacked_bar` at main.rs:11097; period-cycle affordance pinned near main.rs:14159).

## Why

Subscriptions bring 30+ exit rows; the traffic dialog must answer "where did the bytes go" in one glance — the Throne dashboard answer (top-N + rest cropped) transfers directly onto ManyProxy's per-exit data (already sorted by totals), without the per-app pretense the clash API cannot support (deliberate non-fake; gap U7 note).

## What

1. Render top 9 `ExitTraffic` entries (existing sort order — by totals desc as today) of the selected `TrafficPeriod`, then one **"Other (n more · ↓x ↑y)"** aggregate line summarizing everything below rank 9 (names never listed beyond the top 9; count + bytes only).
2. Formatting continuity: the aggregate line reuses the same `↓x ↑y` byte formatting the per-exit lines use (`traffic_stacked_bar` and detail-line conventions at main.rs:11097 region); stacked-bar visualization remains computed over **all** rows (not just top-9) so the proportional bar stays truthful.
3. Period semantics untouched — the cycle 24h→7d→30d→90d and its tooltip (pinned ~main.rs:14159) behave byte-identically; each period computes its own top-9/other split at render.
4. No persistence: purely a display computation over `ExitTraffic` aggregates already in memory; zero new meta, timers, or store columns (hour-bucket table = PARKED, do **not** build it here).

## Files to touch

- `src/main.rs` — traffic dialog top-9/other rendering + tests

## Acceptance criteria

- [ ] ≤9 exits → all rendered as today, no "Other" line; >9 → exactly 9 + the Other line whose count+bytes equal (total − top-9 sum) within accounting rules (same rounding).
- [ ] Sorting ties handled deterministically (same exit order across renders of same data).
- [ ] Stacked bar totals still reflect all rows (not top-9 only).
- [ ] All periods compute independently; no persistence added.
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t79_`, verified unused): `t79_top9_other_split_math`, `t79_top9_renders_without_other_when_few_exits`, `t79_other_line_count_and_bytes_correct`, `t79_periods_compute_independently`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData / No-migration / Plaintext / Hijack / Sidecars: N/A.
- [x] No system mutation: display-only.
- [x] Clipboard / Redaction: exit labels only.

## Out of scope

- Hour-bucket history DB (`hour_start, row_id, rx, tx`) — **PARKED** in INDEX (do not add tables/columns here).
- Per-app/process dashboards (impossible from stock clash API — DEAD category, never fake).
- COUNTRY-mode or geo-labeled stats (no geo assets — dead).
