# 30: List + engine feedback (R1+R5 majors)

**What to build:** every row shows serving truth; engine actions report honestly and never double-apply.
**Owns:** src/main.rs list/detail/statusbar/toasts (+ engine.rs only for per-shard error detail surfacing).

- [ ] Scenario: running row → Action: look at list + detail → Expected: `● Running :port · 87ms` badge in row and detail header; stopped row shows `○ Stopped`; enabled flag stays secondary. Verification: automated test. Priority: required.
- [ ] Scenario: Start-all with 1 broken of 5 → Action: start all → Expected: toast names failed rows + counts (started N, failed M), successful rows keep serving. Verification: automated test. Priority: required.
- [ ] Scenario: double-click Start / Start while busy → Action: second click → Expected: ignored with info toast or disabled button + `Starting…` label; no second apply. Verification: automated test. Priority: required.
- [ ] Scenario: Start on already-running → Action: start → Expected: `already running (restarted)` honest copy, not bare `Started`. Verification: automated test. Priority: important.
- [ ] Scenario: no selection + F5 → Action: press → Expected: info toast `Select a configuration first`. Verification: automated test. Priority: important.
- [ ] Scenario: sidecar crash → Action: poll tick → Expected: error notification naming shard/cause with View-logs path, not status-bar-only. Verification: automated test. Priority: important.
- [ ] Scenario: search no-hits vs empty library → Action: type query → Expected: `No matches — Clear search` + `N of M`; empty library gets Add/Import CTAs. Verification: automated test. Priority: important.
- [ ] Scenario: remove duplicate enable button → Expected: single Switch only. Verification: test. Priority: important.
- [ ] Prohibited: no secrets in toasts/logs; no change to generation schema.

**Status:** done (verified 408 passed / 0 failed, fmt+clippy clean)
