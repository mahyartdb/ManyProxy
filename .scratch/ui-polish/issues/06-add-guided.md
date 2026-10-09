# 06: Guided Add form gaps

**What to build:** Add dialog opens guided-first with forgiving validation:
inline error naming field and fix, focus moved to first invalid field, live
auto-port budget on the Port field, loopback-exposure hint for non-loopback
binds before commit.

**Blocked by:** 05 (Honest dialog footers — Add/Save footer labels settled first).

**Owns:** rust-testing

**Status:** ready-for-agent

- [ ] Scenario: Add with blank name and malformed params → Action: commit →
  Expected: dialog stays open, inline error names field and fix, focus lands
  on first invalid field. Verification: automated test. Priority: required.
- [ ] Scenario: Add open → Action: read Port field → Expected: live budget
  `N of 499 in use (10801–10999, spill 18081–18380)`. Verification: automated test. Priority: important.
- [ ] Scenario: non-loopback bind typed → Action: before commit → Expected:
  loopback-exposure hint visible. Verification: manual UX review. Priority: important.
- [ ] Prohibited side effect: raw JSON becoming the default view; guided
  values lost when toggling Advanced; auto-assign port semantics changed.

Retry cap: 2, then escalate to the user.
