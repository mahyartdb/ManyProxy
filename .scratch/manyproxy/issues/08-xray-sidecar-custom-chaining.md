# 08: Xray sidecar + custom + chaining

**What to build:** Xray-only VLESS, custom outbound JSON / custom full config (per core), user extra-core executable, and chaining — via an Xray sidecar bridged over loopback ingress listeners, with bridge bookkeeping, loopback hygiene, unprivileged extra-exe execution, and the egress-only rule for Extra/Custom/Xray chain nodes.

**Blocked by:** 05 (Standard multi-engine).

**Owns:** rust-networking

**Status:** ready-for-agent

- [ ] Scenario: Xray-only VLESS + custom-JSON + stock configs → Action: start all → Expected: each endpoint serves its own egress in isolation. Verification: integration check. Priority: required.
- [ ] Scenario: broken custom JSON → Action: save + start → Expected: start blocked naming the offending field; other endpoints unaffected. Verification: automated test. Priority: required.
- [ ] Scenario: chain with Extra/Custom/Xray node mid-chain → Action: validate → Expected: rejected with the egress-only rule error. Verification: automated test. Priority: important.
- [ ] Prohibited side effect: sidecar bridge never leaks traffic between endpoints; extra exe never runs privileged and never joins group bulk-tests (active-profile test only).
