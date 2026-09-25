# 07: Fork-core exotic (AWG/Noise/XHTTP)

**What to build:** AmneziaWG-variant and Noise/XHTTP configs running beside standard ones via a compatible fork-core, with an explicit version matrix (2.0 vs 3.x params), wrong-layer mismatch guards, and documented fork-bug notes. No Xray sidecar in this ticket.

**Blocked by:** 05 (Standard multi-engine).

**Owns:** rust-networking

**Status:** done (commit c4ebd9c; `cargo test` 127 passed, verified)

- [ ] Scenario: AmneziaWG-variant link → Action: start → Expected: its endpoint routes traffic (not "unsupported type"). Verification: integration check. Priority: required.
- [ ] Scenario: config mixing AWG params with Xray-noise semantics → Action: validate → Expected: blocked with a wrong-layer error before any connection. Verification: automated test. Priority: important.
- [ ] Scenario: unsupported AWG sub-version → Action: start → Expected: error naming the missing param/version, standard configs unaffected. Verification: automated test. Priority: important.
- [ ] Prohibited side effect: fork-core never replaces stock handling of standard protocols; stock configs keep working if the fork binary is absent (with a clear missing-binary error only for exotic rows).
