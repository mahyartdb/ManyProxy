# 01: Signal-lamp selection grammar

**What to build:** a single unmistakable selection language across the library: the blue wash means the selected row and nothing else, while checked rows carry a rim plus count chip. The bulk bar always reads `N checked (of M visible)` so the acting set is never ambiguous, and the detail header echoes `Acting on: <name>`. Lamps, dots, and badges stay in agreement (lit serving, dark stopped, amber starting, red dead). This kills the P0 where F5 and bulk actions could hit different rows under near-identical highlights.

**Blocked by:** None (can start immediately).

**Owns:** impeccable

**Status:** ready-for-agent

- [ ] Scenario: two rows checked, a third row selected → Action: read the list and bulk bar → Expected: checked rows show rim plus count chip, only the selected row shows the wash, and the bulk label names both counts. Verification: automated test. Priority: required.
- [ ] Scenario: row selected → Action: open the detail pane → Expected: header names the acting row (`Acting on: <name>`) and F5/the detail Start touch that same row. Verification: automated test. Priority: required.
- [ ] Scenario: keyboard plus screen-reader pass over the list → Action: distinguish checked from selected without color → Expected: shape and text (rim, chip, label) carry the difference, not hue alone. Verification: accessibility check. Priority: required.
- [ ] Prohibited side effect: bulk Start/Stop/Delete must not change targets; all safety confirms keep naming affected rows.

Retry cap: 2 attempts, then escalate to the user.
