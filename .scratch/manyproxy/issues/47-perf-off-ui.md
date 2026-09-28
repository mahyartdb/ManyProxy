# 47: Performance off-UI

**What to build:** A window that never freezes: traffic/engine/log ticks off the UI thread with concurrent bounded clash polls and deadlines, minimal idle timers, batched health persistence without second list scans, bounded maps with VecDeque rings and indexed latest-health, cached search/upstream blobs with ingest-time log stripping, concurrent bounded probes, and a fast-startup path (lazy totals, deferred refresh, cached presence/perms).

**Blocked by:** 40-store-v5-foundation (batch tx + tables; rebase onto tickets 41/45 for poll/history paths).

**Owns:** performance-optimization

**Status:** ready-for-agent

- [ ] Scenario: 2+ slow clash controllers → Action: traffic ticks run while typing/scrolling → Expected: UI responsive; per-core timeout 1–2s with total deadline; in-flight tick skipped never queued. Verification: automated test (stub servers). Priority: required.
- [ ] Scenario: idle app → Action: observe timers → Expected: tray pump 500–1000ms; one timer per interval; no 1s slicing wakeups. Verification: automated test. Priority: important.
- [ ] Scenario: probe round completes → Action: persist → Expected: one transaction per round (batched snapshots); log tagging reuses rows without a second full list. Verification: automated test. Priority: important.
- [ ] Scenario: churny short connections + sustained sidecar spam → Action: ticks + log ingest run → Expected: traffic map capped with TTL eviction; log/sample rings VecDeque; latest-health O(1); crash-cause lines pinned. Verification: automated test. Priority: important.
- [ ] Scenario: 50 rows → Action: type in search, scroll list, open logs → Expected: zero per-row serde_json at render/filter; precomputed blobs; ingest-stripped log lines. Verification: automated test. Priority: important.
- [ ] Scenario: test-all on N rows during 60s tick → Action: run → Expected: bounded concurrent probes (8-way); per-kind in-flight flags; manual test never blocked by tick. Verification: automated test. Priority: important.
- [ ] Scenario: cold launch → Action: start app → Expected: first paint precedes sub refresh; traffic totals lazy; sidecar presence + hardened-file flags cached. Verification: automated test. Priority: important.
- [ ] Prohibited side effect: no tick may run blocking I/O on the UI thread; no behavior change to probe semantics.
