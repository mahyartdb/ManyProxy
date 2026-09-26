# 28: Simplicity cleanup (last)

**What to build:** Apply the ranked simplicity review (22 items, ~500 lines) after all behavior fixes land, so cleanup never conflicts with fixes and never masks a behavior change.

**Blocked by:** 23, 24, 25, 26, 27.

**Owns:** none (all modules, mechanical)

**Status:** done (commit d27ab9e; `cargo test` 354 passed before+after, verified; net -52 lines) (blocked)

- [ ] Scenario: full suite → Action: run → Expected: 100% green before AND after, zero verdict changes. Verification: automated test. Priority: required.
- [ ] Scenario: diff review → Action: inspect → Expected: deletions only (+ mechanical merges), no logic changes. Verification: manual UX review. Priority: required.
- [ ] Prohibited side effect: no behavior change of any kind; any item that turns out load-bearing stays, with a comment saying why.
