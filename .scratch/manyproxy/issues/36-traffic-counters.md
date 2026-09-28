# 36: Data-plane traffic counters

**What to build:** per-row ↑/↓ counters measuring real proxied bytes via per-sidecar clash API, surviving restart.

**Blocked by:** None (can start immediately; implement after 35 to avoid store.rs collision).

**Owns:** rust-networking, sqlite

**Status:** ready-for-agent

- [ ] Scenario: running endpoint passing 1 MB → Action: read the row → Expected: counters grow by ≈1 MB data-plane bytes. Verification: automated test + live binary check. Priority: required.
- [ ] Scenario: probes only, no user traffic → Action: wait through probe ticks → Expected: data-plane counters do not move (probe bytes never written to traffic columns). Verification: automated test. Priority: required.
- [ ] Scenario: app restart → Action: relaunch → Expected: counters resume from persisted totals, never zeroed. Verification: automated test. Priority: required.
- [ ] Scenario: clash shape → Action: poll stock sing-box 1.14.2 `/connections` → Expected: totals parse, Bearer auth accepted (verified live before building). Verification: live binary check. Priority: required.
- [ ] Prohibited side effect: no new dependencies; controller ports never collide with endpoint ports; secrets never logged.

**Status:** done (commit 3d0ef54; 424 serial green, fmt+clippy clean, live clash proof vs sing-box 1.14.2)
