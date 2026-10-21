# T-25 — Connections counters `N active · M TCP · K UDP`

- **id**: T-25
- **title**: Connections counters — `N active · M TCP · K UDP` line in the status bar derived from clash snapshot rows
- **epic**: E6 — UI affordance pack
- **size**: S
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E6 T-25 line; gap §4.3 U5 (Throne format `%1 active · %2 TCP · %3 UDP`, §F.1 status presentation; clash snapshot rows carry `metadata.network` — verified parse body shape in health.rs:3571 test data with `"network":"tcp"`); verified consumer chain: `poll_clash_entries` (health.rs:1425) → AppView traffic mirror (`ensure_traffic_mirror` per inventory) → status bar count line in `render_status_bar` (main.rs:21708).

## Why

The clash snapshot already delivers per-connection rows to the app every traffic tick; a three-number counter line is the free accuracy upgrade for "is it working" glances, per Throne's established presentation (§F.1), while staying pure derivation — **zero new network surface**.

## What

1. **Derivation** (in health.rs, next to `poll_clash_entries`'s row parse): a small typed struct `ConnCounts { active: usize, tcp: usize, udp: usize }` and a pure fn/classifier over the parsed connection rows — `network` field: `"tcp"` → tcp, `"udp"` → udp, other/blank → still counts into active only; aggregate across the per-core snapshots the AppView poll already merges (same attribution family the TrafficSeenMap uses).
2. **Status bar**: the left/center status region (`render_status_bar`, main.rs:21708) gains the line — displayed when the connections/traffic collection is live: `N active · M TCP · K UDP` (exact separator format `·` matches inventory's status-bar conventions); hidden cleanly when no snapshot rows exist (e.g. no running cores — label collapses as today's "health: no probes this session" style does at main.rs:14431–14438).
3. **Freshness/staleness**: counts update on the existing traffic tick cadence (`TRAFFIC_TICK_SECS`/`TRAFFIC_FAST_TICK_SECS`, main.rs:160/162) and reuse the staleness conventions traffic totals already honor (no new timers).
4. Tooltip: the status segment's min-width/truncate/tooltip pattern (documented at main.rs:21708 region) applies so narrow windows don't hide the numbers.

## Files to touch

- `src/health.rs` — `ConnCounts` + classifier + tests (off snapshot fixtures like health.rs:3571)
- `src/main.rs` — status-bar segment render + tests

## Acceptance criteria

- [ ] Counts derive exactly from latest snapshot rows (tcp/udp/other split matches the fixture data); `0 active · 0 TCP · 0 UDP` renders sanely when running but idle; segments hidden when no cores run.
- [ ] Tick updates ride the existing traffic cadence (no extra network calls anywhere — one snapshot parse).
- [ ] Narrow-window truncation keeps the numbers accessible via tooltip.
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t75_`, verified unused): `t75_conn_counts_from_snapshot_rows`, `t75_conn_counts_tcp_udp_split`, `t75_status_bar_renders_counter_line`, `t75_counter_hidden_when_no_running_cores`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData / No-migration / Plaintext / Hijack / Sidecars: N/A.
- [x] No system mutation: purely derived from existing snapshot polls (the clash API loopback reads are already the app's own traffic-polling surface, unchanged).
- [x] Clipboard `bind:port` discipline: N/A.
- [x] Redaction: counts only — no connection content enters the bar.

## Out of scope

- Kill-switch on connections (Throne explicitly has none either).
- Per-row-per-waypoint counters or a grouping view (T-24 handles cells; this is the aggregate).
- Status-bar re-layout beyond adding the one segment.
