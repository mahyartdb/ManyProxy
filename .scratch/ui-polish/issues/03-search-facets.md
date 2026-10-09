# 03: Discoverable search and facets

**What to build:** A persistently visible library search field with live
`N of M` count, three unmistakable clears (query, facet filters, checked set),
and facet pickers showing active choice plus result count.

**Blocked by:** None (can start immediately). [P]

**Owns:** rust-testing

**Status:** ready-for-agent

- [ ] Scenario: library with rows → Action: look at filter band → Expected:
  visible text field with placeholder `Search configurations` and adjacent
  live `N of M` count. Verification: automated test. Priority: required.
- [ ] Scenario: query typed + facets set + rows checked → Action: activate each
  clear in turn → Expected: each clear names its target and affects only that
  target. Verification: automated test. Priority: required.
- [ ] Scenario: facet choice active → Action: read picker trigger → Expected:
  active choice and its result count shown. Verification: manual UX review. Priority: important.
- [ ] Prohibited side effect: search query state moved off the list-state seam
  (`ListState::set_query` path); filter AND-semantics changed.

Retry cap: 2, then escalate to the user.
