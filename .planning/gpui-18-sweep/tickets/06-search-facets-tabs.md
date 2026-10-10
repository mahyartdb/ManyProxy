# 06: Search, facets, inspector tabs as one system

**What to build:** One visible search field with scoped clears; facet `Select`s keep choice+count with legal tooltip wrappers; inspector tabs carry Ctrl-1/2/3 `Kbd` hints with frozen pane order and scroll reset.

**Blocked by:** 05-operate-bulk (shares facet/bulk/tab bar language).

**Owns:** gpui-kit composition.

**Status:** done

- [x] Scenario: type query + set facets + check rows → Action: press each clear → Expected: `Clear-search` affects query only, `Clear-facets` affects facets only, bulk clear affects checked only; search field persistent with `Search configurations` placeholder + live `N of M`. Verification: automated test (query/facet/checked isolation tests via existing `ListState::set_query` + facet setters + checked-set seams). Priority: required.
- [x] Scenario: switch Details/Health/Presets via click + Ctrl-1/2/3 → Action: observe → Expected: tabs show `Kbd` hints, stable anchors, scroll resets, pane order frozen Header→Endpoints→tab-specific (Endpoints above fold), no per-tab id invention (keep wrapper+markers — `Tab::ix` is `pub(crate)`). Verification: automated test (tab switch renders expected pane ids + scroll-reset via existing seam). Priority: important.
- [ ] Prohibited side effect: no `Select::tooltip` invention (outer wrapper only); no `Tab::id` invention; no facet-dialog resurrection (Selects stay).

Retry cap: 2, then escalate.
