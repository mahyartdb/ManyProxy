# 02: Toolbar and menu accessibility

**What to build:** Every toolbar lever and menu action reachable, named, and
announced: 5 levers (Add, Start, Stop, Test, Import) plus Library, Manage,
Engine, Help menus with all items exposed to assistive tech; toolbar fits the
880px minimum on one row.

**Blocked by:** None (can start immediately). [P]

**Owns:** accessible-frontend, rust-testing

**Status:** ready-for-agent

- [ ] Scenario: main window focused → Action: traverse toolbar and all four
  menus with assistive tech → Expected: every control announces its visible
  label (plus shortcut where bound); no silent menu items. Verification: accessibility check. Priority: required.
- [ ] Scenario: window at 880px wide → Action: render toolbar → Expected: all
  levers and menus on one row, nothing clipped. Verification: automated test. Priority: required.
- [ ] Scenario: keyboard only → Action: open each menu and activate one item →
  Expected: item runs the same handler as its button/palette twin.
  Verification: manual UX review. Priority: important.
- [ ] Prohibited side effect: Kbd chips added to unbound actions; pinned
  toolbar/menu ids or `Name (Key)` accessible strings changed without
  same-ticket test updates.

Retry cap: 2, then escalate to the user.
