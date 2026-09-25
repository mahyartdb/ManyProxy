# 06: Health + logs + stats

**What to build:** Per-row health at scale: URL test/latency, TCP ping, traffic counters, connections view, per-config log filter with secrets redacted, dead-config flagging.

**Blocked by:** 05 (Standard multi-engine).

**Owns:** observability-and-instrumentation

**Status:** done (commit 428a829; `cargo test` 107 passed, verified)

- [ ] Scenario: 5 running → Action: run URL test → Expected: each row shows latency or a named failure within 30s. Verification: integration check. Priority: required.
- [ ] Scenario: active config with traffic → Action: open its logs → Expected: only its lines show, secrets redacted. Verification: automated test. Priority: required.
- [ ] Scenario: one endpoint killed upstream → Action: re-test → Expected: its row flagged failed while others keep values. Verification: integration check. Priority: important.
- [ ] Prohibited side effect: health checks never route through the wrong config's egress; logs never store plaintext secrets.
