# 12: Convergence — health persistence + background ticks

**What to build:** Health samples survive restart (persist latest-per-config sample + dead flag; logs stay session-only by ruling) and the three deferred timers tick without user clicks: engine crash poll, due-subscription auto-refresh pass, and periodic re-probe of running rows — each bounded, each stoppable, none touching the network except user-configured endpoints.

**Blocked by:** 09 (Hardening + release proof).

**Owns:** sqlite

**Status:** done (verified: `cargo test` 177 passed)

- [ ] Scenario: 2 rows with latencies → Action: restart app → Expected: rows still show last latency + dead flags, not blank. Verification: automated test. Priority: important.
- [ ] Scenario: engine killed externally → Action: wait one tick → Expected: row flagged with named cause, no click needed. Verification: integration check. Priority: important.
- [ ] Scenario: timers idle with nothing running → Action: observe one tick → Expected: no network traffic, no errors. Verification: automated test. Priority: optional.
- [ ] Prohibited side effect: no telemetry, no auto-start of stopped configs, no timer-driven subscription fetch beyond already-due subs.
