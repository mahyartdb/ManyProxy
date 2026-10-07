# 11: Resize-aware pass at minimum size

**What to build:** at the 880x620 minimum window, the list, facet bar, inspector, status strip, and Add dialog all paint inside the frame with nothing exceeding the frame width, so shrinking the window never clips content.

**Blocked by:** 09-startup-window-size, 10-toolbar-dropdowns.

**Owns:** gpui-kit-design-guides

**Status:** ready-for-agent

- [ ] Scenario: 880x620 window with seeded rows and a selection → Action: paint → Expected: facet bar, 1+ rows, inspector, and status strip all paint with right edges inside 880 (same containment rule as the t08 helper). Verification: automated test. Priority: required.
- [ ] Scenario: status strip at minimum width → Action: read strip → Expected: library totals always visible; if segments do not fit, traffic labels collapse first, then center — never the left counts. Verification: automated test. Priority: important.
- [ ] Scenario: Add dialog at minimum size → Action: open Add → Expected: all fields paint inside the frame. Verification: automated test. Priority: important.
- [ ] Prohibited side effect: new features; horizontal scrollbars as the fix for toolbar/facet overflow (dropdowns + wrapping own that); whole-window scroll.
