# 06: Group management parity

**What to build:** rename / move / bulk-move / delete-group refresh facet counts and inspector with existing `Store` group ops; delete refuses while rows remain.

**Blocked by:** 02-facet-counts, 05-inspector-tabs.

**Owns:** sqlite

**Status:** ready-for-agent

- [ ] Scenario: rename group `home` to `cabin` → Action: commit → Expected: facet options and any open picker show the new name with correct counts. Verification: automated test. Priority: important.
- [ ] Scenario: delete group holding rows → Action: confirm → Expected: refused with rows named, nothing deleted. Verification: automated test. Priority: required.
- [ ] Prohibited side effect: store schema change; group names with slashes breaking element ids (slugify ids, display verbatim).
