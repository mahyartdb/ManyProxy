# 27: Protocol completion + caps lift + secrets hardening (cross-cutting, sequential last)

**What to build:** The user-directed scope: SSH + AnyTLS + standalone Noise connect end-to-end; N extra slots; wider auto-port range; secrets-at-rest hardening with keychain on backlog.

**Blocked by:** 23, 24, 25, 26 (touches import/store/engine/health — runs only after they land).

**Owns:** rust-networking

**Status:** done (commit 41f1e65; `cargo test` 354 passed, verified) (blocked)

- [ ] Scenario: `ssh://` link → Action: import + start + probe → Expected: connects through its own endpoint (engine has native ssh; parse + validate + generate + probe all wired). Verification: automated test. Priority: required.
- [ ] Scenario: AnyTLS row → Action: start + probe → Expected: connects (native engine support; flips current UnsupportedProtocol). Verification: automated test. Priority: required.
- [ ] Scenario: standalone Noise row → Action: start → Expected: connects via the sidecar path with mismatch guards (AWG-vs-Noise confusion rejected explicitly). Verification: automated test. Priority: required.
- [ ] Scenario: 3 extra-core rows → Action: start all → Expected: each serves its own endpoint (N named slots replace the singleton). Verification: automated test. Priority: required.
- [ ] Scenario: 250 auto-port configs → Action: add all → Expected: all assigned, honest exhaustion only past the widened range. Verification: automated test. Priority: required.
- [ ] Scenario: fresh DB + temp configs → Action: inspect ACLs → Expected: user-only permissions on DB and sidecar configs; keychain/DPAPI recorded as backlog non-goal. Verification: manual UX review. Priority: important.
- [ ] Scenario: sub fetch http URL / internal host → Action: refresh → Expected: policy enforced per ticket-27 decision (documented: warn/block/allow with rationale). Verification: automated test. Priority: important.
- [ ] Prohibited side effect: no verdict flips in 23-26 tests; traffic counters stay probe-bytes per spec note (data-plane deferred).
