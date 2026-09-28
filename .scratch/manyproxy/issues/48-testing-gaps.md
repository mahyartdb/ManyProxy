# 48: Testing gaps closure

**What to build:** Proof for the load-bearing paths Phase 1 touches: DPAPI roundtrip + rejection + store failure injection, stub-executable lifecycle tests plus opt-in live CI job, tray dispatch headless tests, port-race tests, clash error-arm tables, preset integration, sealed-refresh conflicts, tick wiring, SSRF/BOM, non-Windows DPAPI stub.

**Blocked by:** 40-store-v5-foundation (rebase onto tickets 41/42/44/45/47 as their code lands; tests written against final seams).

**Owns:** test-driven-development

**Status:** ready-for-agent

- [ ] Scenario: DPAPI protect/unprotect → Action: roundtrip + tampered-blob + bad-base64 + non-UTF8 + unknown-marker + legacy-brace table → Expected: all arms behave per spec; store write aborts loudly when DPAPI down; foreign-blob reads quarantine. Verification: automated test. Priority: required.
- [ ] Scenario: stub executable as sidecar → Action: stop_child + poll_exit + drain_log_lines → Expected: lifecycle proven without real sing-box. Verification: automated test. Priority: required.
- [ ] Scenario: CI with pinned sing-box binary → Action: opt-in live job → Expected: check/run/clash-auth/totals-move exercised; default suite stays hermetic. Verification: integration check. Priority: important.
- [ ] Scenario: every tray menu id → Action: dispatch headless → Expected: correct start/stop handler runs; windowing stays manual-tour only. Verification: automated test. Priority: important.
- [ ] Scenario: exhausted ports / never-binding child → Action: allocate / wait → Expected: None arm + give-up arm handled without wall-clock flakes. Verification: automated test. Priority: important.
- [ ] Scenario: clash stub returning oversize / 401 / non-200 / malformed → Action: poll → Expected: typed outcome per arm; is_probe_destination edge table green. Verification: automated test. Priority: important.
- [ ] Scenario: preset attach → generate → delete preset → Action: run integration → Expected: dangling id fails at attach-time semantics; sealed-row refresh conflicts counted correctly; stub clash moves traffic columns via tick wiring. Verification: automated test. Priority: important.
- [ ] Scenario: BOM-prefixed preset JSON / preset fetch redirect → Action: parse / fetch → Expected: BOM stripped; fetch-time SSRF re-check blocks. Verification: automated test. Priority: important.
- [ ] Scenario: non-Windows build → Action: compile → Expected: loud DPAPI stub or documented Windows-only; no silent behavior. Verification: automated test. Priority: important.
- [ ] Prohibited side effect: no live-network dependency in the default suite; no whole-temp-dir scans (ticket-39 rule).
