# 05: Standard multi-engine

**What to build:** N standard-protocol configs active at once, each on its own loopback mixed (SOCKS5+HTTP) endpoint: start/stop/start-all, unique ports, inbound→outbound route mapping, WireGuard via endpoints, config validated before start, per-endpoint traffic isolation, zero OS network mutation.

**Blocked by:** 02 (Library CRUD + store).

**Owns:** rust-networking

**Status:** ready-for-agent

- [ ] Scenario: 3 stopped standard configs → Action: start all → Expected: 3 reachable endpoints, each returning its own egress on both socks5:// and http:// forms. Verification: integration check. Priority: required.
- [ ] Scenario: 2 running, stop 1 → Action: stop one → Expected: its endpoint refuses connections, the other keeps serving. Verification: integration check. Priority: required.
- [ ] Scenario: port collision → Action: start config on a taken port → Expected: start blocked with error naming both configs, nothing crashes. Verification: automated test. Priority: required.
- [ ] Scenario: full session → Action: snapshot OS proxy/DNS/routes before and after → Expected: snapshots identical. Verification: operational check. Priority: required.
- [ ] Prohibited side effect: no system proxy, no virtual interface, no DNS rewrite, no admin prompt for loopback use.
