# 31: Dialogs + import + subscriptions (R2+R3+R4)

**What to build:** guided Add/Edit, honest import, editable subscriptions.
**Owns:** src/main.rs dialogs/import/subs UI + src/store.rs subscription update + src/import.rs gaps.

- [ ] Scenario: Add dialog → Action: pick protocol → Expected: per-protocol field template (not raw JSON first), credential-missing rows rejected with named-field error. Verification: automated test. Priority: required.
- [ ] Scenario: invalid field → Action: commit → Expected: inline field error + focus moves to error; dialog stays open. Verification: automated test. Priority: required.
- [ ] Scenario: bind/port collides → Action: type used pair → Expected: non-blocking inline warning naming colliding row; save still allowed. Verification: automated test. Priority: important.
- [ ] Scenario: edit subscription → Action: change label/URL/group → Expected: persists via `Store::update_subscription`, list refreshes. Verification: automated test. Priority: required.
- [ ] Scenario: duplicate sub URL → Action: add same URL → Expected: `already subscribed` friendly path, no double row. Verification: automated test. Priority: important.
- [ ] Scenario: http sub → Action: add → Expected: visible plaintext-fetch warning (Warn policy surfaced). Verification: automated test. Priority: important.
- [ ] Scenario: valid protocol list → Action: typo protocol → Expected: error names valid protocols. Verification: automated test. Priority: important.
- [ ] Prohibited: no silent dedup changes; no secrets in errors.

**Status:** done (verified 408 passed / 0 failed, fmt+clippy clean)
