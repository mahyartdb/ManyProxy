# 07: DataTable spike view-only default off

**What to build:** `DenseTableDelegate` plus `TableState` as a read-through view over `visible_rows()` plus health/running/traffic maps behind a default-off flag; `List` stays mounted as the committed path.

**Blocked by:** 01-dense-rows.

**Owns:** gpui-kit-design-guides

**Status:** ready-for-agent

- [ ] Scenario: flag on with seeded rows → Action: paint table → Expected: rows mirror the `List` filter, checkbox toggles the same `checked` set, selection follows config id not index. Verification: automated test. Priority: optional.
- [ ] Scenario: flag off (default) → Action: open library → Expected: `List` path unchanged, no second filter state. Verification: automated test. Priority: required.
- [ ] Prohibited side effect: hand-rolled virtualization; forked filter/sort state; silent cutover to the table.
