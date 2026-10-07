# 02: Timetable row anatomy

**What to build:** every library row in one exact fixed-cell anatomy — occupancy lamp, nameplate, numbers, state badge — with rank carried by weight and case at a single size step, tabular numerals for ports and latencies, and no numberless rows. Meta and badge lines stay subordinate to the name exactly as the world brief's timetable discipline requires.

**Blocked by:** 01 (Signal-lamp selection grammar — the lamp and wash language must exist first).

**Owns:** impeccable

**Status:** ready-for-agent

- [ ] Scenario: library with mixed running, stopped, and dead rows → Action: render the list → Expected: identical row geometry on every row, each carrying port, endpoint, and health numbers with lamp, name, meta, and badge in fixed positions. Verification: automated test. Priority: required.
- [ ] Scenario: 50-row stress library → Action: scroll and filter → Expected: rows stay uniform and scannable with no ragged cards or overflow. Verification: automated test. Priority: important.
- [ ] Prohibited side effect: pinned row strings and stable test anchors must not change meaning; lamp glow or shadow decoration must not appear.

Retry cap: 2 attempts, then escalate to the user.
