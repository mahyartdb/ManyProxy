# 32: Engine + store + health correctness (R6+R8+R9)

**What to build:** engine emits what 1.14.2 accepts; store identity exact; health truthful.
**Owns:** src/engine.rs + src/store.rs + src/health.rs (no UI except error strings).

- [ ] Scenario: route rules → Action: generate → Expected: `inbound` as array; `sing-box check` EXIT 0 on stock 1.14.2. Verification: automated test + real-binary check. Priority: required.
- [ ] Scenario: custom full-config rows → Action: conflict check → Expected: generate and validate agree (customs never conflict). Verification: automated test. Priority: required.
- [ ] Scenario: stop with failing re-gen → Action: stop_one/stop_all → Expected: error propagated or snapshot restored; never silent half-state. Verification: automated test. Priority: required.
- [ ] Scenario: AWG numerics (jc/jmin/jmax/s1-s4/h1-h4) as strings → Action: generate → Expected: coerced to numbers; truly opaque (h-ranges/i-chains) verbatim. Verification: automated test. Priority: required.
- [ ] Scenario: export file → Action: export → Expected: 0600/ACL hardened like DB. Verification: automated test. Priority: important.
- [ ] Scenario: two AWG rows same server+creds different jc → Action: refresh/import → Expected: two rows (identity includes AWG keys). Verification: automated test. Priority: important.
- [ ] Scenario: IPv6 bind → Action: copy URLs → Expected: `[::1]:port` bracketed. Verification: automated test. Priority: important.
- [ ] Scenario: URL probe fails after ping success → Action: status() → Expected: dead (worst-kind-wins or per-kind), stale samples expire. Verification: automated test. Priority: important.
- [ ] Prohibited: no schema change without migration test; no secrets in logs.

**Status:** done (verified 408 passed / 0 failed, fmt+clippy clean)
