# 44: Multi-select + bulk bar + facets

**What to build:** Subset operations for 50+ row libraries: id-keyed multi-select with a bulk action bar (Start/Stop/Test/Delete/Clear with partial-failure copy), plus group/status/protocol facets ANDed with search, and group management (rename/delete/move/counts).

**Blocked by:** 43-ui-honesty (same delegate files; rebase onto its filter pipeline).

**Owns:** impeccable

**Status:** ready-for-agent

- [ ] Scenario: 3 rows checked across groups → Action: bulk Start → Expected: healthy rows serve; row-local failures named per row; union-level failures stay atomic with existing partial-failure copy. Verification: automated test. Priority: required.
- [ ] Scenario: checked set with filter hiding members → Action: observe bar → Expected: `N selected (M visible)` label honest; bulk acts on the set. Verification: automated test. Priority: required.
- [ ] Scenario: group + status + protocol facets + text query set → Action: filter → Expected: visible rows satisfy all predicates. Verification: automated test. Priority: required.
- [ ] Scenario: group with N rows → Action: rename/delete/move-selected-to-group → Expected: counts shown; bulk move works; no free-text typos required. Verification: automated test. Priority: important.
- [ ] Scenario: bulk Delete on checked rows → Action: confirm → Expected: confirm names all affected rows, reuses detach-then-delete path. Verification: manual UX review. Priority: required.
- [ ] Scenario: keyboard → Action: Delete / Ctrl-F / Enter / Esc → Expected: Delete deletes (with confirm), Ctrl-F focuses search, Enter starts via list Confirm, Esc clears set only with no dialog open. Verification: manual UX review. Priority: important.
- [ ] Prohibited side effect: single-anchor detail/Enter behavior unchanged; List Enter never rebound at app level.
