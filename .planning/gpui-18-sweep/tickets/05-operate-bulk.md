# 05: Operate row + bulk bar regrouped

**What to build:** Detail operate row groups primaries (`ButtonGroup` Start/Stop/Test) with management behind one `Row actions…` menu (`DropdownButton`/`DropdownMenu`); bulk bar groups with separators, destructive trailing; both dispatch through existing handlers.

**Blocked by:** 04-density-empty (shares small/trigger language).

**Owns:** gpui-kit composition.

**Status:** done

- [x] Scenario: row selected at 880x620 → Action: view `detail-operate` → Expected: primaries grouped, Edit/Delete/Duplicate/resets behind exactly one disclosed `Row actions…` trigger naming its contents, row ≤2 wrapped lines, never h-scrolls, same toasts + in-flight guards as toolbar (scope-named labels only). Verification: automated test (operate ids + menu-item ids paint + dispatch-through-same-handler test: operate Start == toolbar Start outcome). Priority: required.
- [x] Scenario: N rows checked → Action: read bulk bar → Expected: verbs grouped with separators (selection | state | destructive trailing `Delete.danger`), opposites unambiguous, label `N checked (of M visible)`, wraps ≤3 lines without sideways scroll. Verification: automated test (bulk ids + `bulk_label` strings). Priority: required.
- [ ] Prohibited side effect: no new verbs/shortcuts; no per-row start tracking; no duplicated handler logic (dispatch, don't copy).

Retry cap: 2, then escalate.
