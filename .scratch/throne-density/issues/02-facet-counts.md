# 02: Facet counts plus honesty

**What to build:** group/protocol/status `Select`s show live counts (`Group: name (n)`), `N of M` search count plus `Clear facets` / `Clear search` keep filter state honest at any group count.

**Blocked by:** None (can start immediately).

**Owns:** gpui-kit-design-guides

**Status:** ready-for-agent

- [ ] Scenario: 3 groups with 10 members each → Action: open group facet → Expected: each option shows its live count from all rows and selecting filters to that group only. Verification: automated test. Priority: required.
- [ ] Scenario: query active hiding rows → Action: read count row → Expected: `V of T` names visible and total plus a working clear action. Verification: automated test. Priority: important.
- [ ] Prohibited side effect: new tab strip as the only filter path; credential fields entering the search index.
