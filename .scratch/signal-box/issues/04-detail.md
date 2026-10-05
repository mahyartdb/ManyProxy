# 04: Block-shelf detail

**What to build:** the detail pane as ruled endpoint/health/usage modules with endpoints above the fold: manage actions (Edit, Delete, Duplicate, resets) collapse into a row menu while Start, Stop, Test, Ping, Logs, and Copy URLs stay inline in a single operate row. The `Acting on` echo from ticket 01 rides the module header so the pane always names its row.

**Blocked by:** 01 (Signal-lamp selection grammar — the `Acting on` echo and lamp language originate there).

**Owns:** impeccable

**Status:** ready-for-agent

- [ ] Scenario: row selected → Action: read the detail pane without scrolling → Expected: endpoints module visible above the fold with one operate row; manage actions behind a single row menu. Verification: manual UX review. Priority: required.
- [ ] Scenario: failed row selected → Action: open the Health module → Expected: the failure alert with evidence renders in place and engine actions operate on the same row. Verification: automated test. Priority: required.
- [ ] Prohibited side effect: the 240px minimum must keep failure evidence painted; no horizontal scrolling anywhere.

Retry cap: 2 attempts, then escalate to the user.
