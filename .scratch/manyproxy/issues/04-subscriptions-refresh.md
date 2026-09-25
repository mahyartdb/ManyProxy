# 04: Subscriptions + refresh

**What to build:** Subscription URLs with manual + auto refresh: upsert keyed by normalized identity (not remark), hand-edit overlay preserved, user-deleted rows tombstoned, atomic per-group swap keeping last-good on failure.

**Blocked by:** 03 (Import grammar pack).

**Owns:** api-and-interface-design

**Status:** done (commit ad00f9e; `cargo test` 81 passed, verified)

- [ ] Scenario: new subscription URL → Action: add + fetch → Expected: group created, members appear disabled, fetch recorded. Verification: automated test. Priority: required.
- [ ] Scenario: member renamed by hand → Action: refresh with unchanged upstream body → Expected: no duplicates, rename preserved (no-op effective). Verification: automated test. Priority: required.
- [ ] Scenario: upstream removes a member → Action: refresh → Expected: only subscription-owned row removed; hand-created configs untouched. Verification: automated test. Priority: important.
- [ ] Scenario: fetch fails → Action: refresh with network error → Expected: prior members kept, error recorded on subscription. Verification: integration check. Priority: important.
- [ ] Prohibited side effect: refresh never enables/starts configs, never overwrites hand-edited fields.
