# 04: Detail pane fits the minimum window

**What to build:** At 880x620 with a row selected, endpoint values with Copy
buttons and the Start control visible without scrolling; operate row never
exceeds two wrapped lines and never scrolls horizontally; every pane names its
row (`Acting on: <name>`).

**Blocked by:** 01 (Scannable library rows — shared selection-grammar helpers
and row text functions).

**Owns:** rust-testing

**Status:** ready-for-agent

- [ ] Scenario: row selected at 880x620 → Action: view inspector → Expected:
  endpoint values, their Copy buttons, and Start visible without scrolling.
  Verification: manual UX review. Priority: required.
- [ ] Scenario: operate row at minimum width → Action: render Details pane →
  Expected: at most two wrapped lines, no horizontal scroll; management
  actions behind exactly one disclosed menu. Verification: automated test. Priority: required.
- [ ] Scenario: each of Details/Health/Presets → Action: open pane →
  Expected: header names the acted-on row. Verification: automated test. Priority: important.
- [ ] Prohibited side effect: endpoints pushed below the operate row;
  per-row start tracking introduced; copy buttons moving credentials.

Retry cap: 2, then escalate to the user.
