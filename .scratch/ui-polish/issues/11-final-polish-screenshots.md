# 11: Final polish and screenshots

**What to build:** Full sweep closed: verify trio green, all pinned ids and
strings green, four screenshots recaptured on the seeded demo library with
no real secrets, PRODUCT.md stale-screenshots note updated.

**Blocked by:** 01, 02, 03, 04, 05, 06, 07, 08, 09, 10 (all build tickets).

**Owns:** rust-testing

**Status:** ready-for-agent

- [ ] Scenario: repo root → Action: run `cargo fmt --check`, `cargo clippy
  --all-targets --locked -- -D warnings`, `cargo test --locked` → Expected:
  all green including `assert_no_system_mutation`-family. Verification: automated test. Priority: required.
- [ ] Scenario: seeded demo library at 1000x700 → Action: capture library,
  Add, Import, Subscriptions → Expected: `assets/screenshots/*.png` match
  shipped UI, synthetic hosts only. Verification: manual UX review. Priority: required.
- [ ] Scenario: PRODUCT.md evidence section → Action: read line 41 area →
  Expected: stale-screenshots note updated to the fresh capture.
  Verification: integration check. Priority: optional.
- [ ] Prohibited side effect: screenshots with real upstream hosts,
  credentials, or tokens committed; unrelated README claims altered.

Retry cap: 2, then escalate to the user.
