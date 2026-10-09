# 08: Subscriptions clarity

**What to build:** Subscriptions dialog where the global and per-subscription
auto-refresh switches each state what they govern, intervals form one grouped
exclusive choice (always/1h/6h/12h/24h/7d) with no redundant summary line, and
plain-HTTP URLs show a plaintext-fetch warning beside the field.

**Blocked by:** 05 (Honest dialog footers — Done/Cancel semantics settled first).

**Owns:** rust-testing

**Status:** ready-for-agent

- [ ] Scenario: Subscriptions open → Action: read interval control →
  Expected: exactly one grouped exclusive choice of the six intervals with
  the current value marked. Verification: automated test. Priority: required.
- [ ] Scenario: global auto-refresh off → Action: press Refresh on one
  subscription → Expected: only that subscription refreshes and reports its
  outcome. Verification: automated test. Priority: required.
- [ ] Scenario: plain-HTTP URL in the URL field → Action: read form →
  Expected: plaintext-fetch warning visible beside the URL field.
  Verification: manual UX review. Priority: important.
- [ ] Prohibited side effect: refresh scheduling semantics changed; global
  kill-switch removed; explicit refresh gated behind the switches.

Retry cap: 2, then escalate to the user.
