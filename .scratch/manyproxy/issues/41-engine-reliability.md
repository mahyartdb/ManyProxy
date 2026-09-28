# 41: Engine reliability core

**What to build:** Crash-safe sidecar lifecycle: startup reaper (ours-only) + stale secret-temp sweep, crash recovery with manual Restart and opt-in auto-restart (backoff + max-retries), served/clash-set drop on crash with tick skipping + stale totals, clash-port reservation without TOCTOU plus reserved-range validation, experimental-merge, 128-bit crypto secrets, serialized start/stop with surfaced stop failures, single-apply start-all subset, egress DNS resolution + cause classifier, probe-attribution normalization, bind/port early validation, custom-config listen parsing, xray bridge own-skip, typo hints on dropped keys.

**Blocked by:** None (can start immediately; rebase onto ticket 40 for batched traffic persist).

**Owns:** backend-patterns

**Status:** ready-for-agent

- [ ] Scenario: hard-killed app with orphan sidecars + stale secret temp files → Action: launch → Expected: reaper kills only matching sidecar binaries holding our ports and sweeps stale temp files; no manual port cleanup. Verification: automated test (stub executables). Priority: required.
- [ ] Scenario: one crashed shard → Action: Restart invoked → Expected: shard restarts; auto-restart only with opt-in, bounded retries with backoff; dead cores skipped by traffic/reprobe ticks and totals marked stale. Verification: automated test. Priority: required.
- [ ] Scenario: row saved with port in clash/bridge reserved ranges → Action: save → Expected: typed error naming the range before any spawn. Verification: automated test. Priority: required.
- [ ] Scenario: custom config carrying `experimental` → Action: clash inject + start → Expected: existing keys survive merge; clash secret is 128-bit random, in-memory only, cleared on stop. Verification: automated test. Priority: required.
- [ ] Scenario: stop during in-flight start → Action: both land → Expected: serialized ordering, no stale rollback; stop failure toasts instead of false success. Verification: automated test. Priority: required.
- [ ] Scenario: N rows with one bad → Action: start-all → Expected: single apply of maximal healthy subset; bad row named (row-local vs union-level split preserved). Verification: automated test. Priority: important.
- [ ] Scenario: bad egress hostname → Action: validate + health probe → Expected: typed resolution failure before spawn; health distinguishes DNS from egress failure. Verification: automated test. Priority: important.
- [ ] Scenario: probe flows with resolved-IP/port/case variants → Action: traffic poll → Expected: excluded from usage counters in all variants. Verification: automated test. Priority: required.
- [ ] Scenario: garbage bind_ip / port 0 → Action: start → Expected: typed error before any spawn. Verification: automated test. Priority: important.
- [ ] Scenario: two custom replacements binding same port → Action: start both → Expected: conflict error naming both. Verification: automated test. Priority: important.
- [ ] Scenario: active xray bridges → Action: re-apply → Expected: own bridge ports skipped, no false PortUnavailable. Verification: automated test. Priority: important.
- [ ] Prohibited side effect: reaper never kills non-matching processes; no silent auto-restart without opt-in.
