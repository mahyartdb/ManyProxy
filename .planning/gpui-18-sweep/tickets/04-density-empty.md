# 04: Density, truncation, TitleBar, Empty states

**What to build:** Consistent `.small()` density, full-text tooltips on everything that truncates, kit title text, and teaching `Empty` states with actions inside.

**Blocked by:** 03-forms-groups (shares footer/small language).

**Owns:** gpui-kit composition + accessible-frontend (announce order).

**Status:** done

- [x] Scenario: walk toolbar/facet/bulk/operate/endpoint-copies/search-clears/Advanced-toggle/log-actions/empty-buttons → Action: measure + hover → Expected: all `.small()` (footers stay `.small()`), every truncating surface (row name/meta, endpoint values, status segments, preset/usage lines, log notes) has full-text tooltip, `Kbd` only where bound with `Name (Key)` accessible names, TitleBar uses kit text. Verification: automated test (ids paint + tooltip asserts where harness supports + `clear-search-hint`/Advanced-toggle/log-action sizes). Priority: required.
- [x] Scenario: empty library / no-matches / no-selection / no-log-lines → Action: view each → Expected: kit `Empty` with title + description + working buttons in content (Add/Import/Clear-search/Clear-facets), honest counts (`N of M`, `N checked (of M visible)`), screen-reader order hears empty then actions together. Verification: automated test (empty ids + CTA-click tests: hint clears query, CTAs open add/import). Priority: required.
- [ ] Prohibited side effect: no third text size / second radius; no Kbd on unbound actions; no stranded CTAs outside `Empty`.

Retry cap: 2, then escalate.
