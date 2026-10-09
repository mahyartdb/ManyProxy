# 09: Restrained motion and focus

**What to build:** Dialog open/close, inspector tab switch, and bulk-bar
appear/disappear animate at ≤200ms honoring reduced-motion; every interactive
element shows hover and keyboard-focus indicators; in-flight work shows
label + spinner and ignores double-activation — all within the flat
signal-box world (no shadows, glow, gradients).

**Blocked by:** 01 (Scannable library rows), 04 (Detail pane fit — animates
final structure, not pre-change layout).

**Owns:** accessible-frontend, rust-testing

**Status:** ready-for-agent

- [ ] Scenario: reduced-motion enabled → Action: open dialog, switch
  inspector tab, toggle bulk bar → Expected: state changes perceivable
  without animation. Verification: manual UX review. Priority: required.
- [ ] Scenario: keyboard traversal → Action: tab through toolbar, rows,
  inspector, dialogs → Expected: visible focus indicator on every stop.
  Verification: accessibility check. Priority: required.
- [ ] Scenario: start in flight → Action: press Start again → Expected:
  `Starting…` label + spinner shown, second activation ignored.
  Verification: automated test. Priority: important.
- [ ] Prohibited side effect: shadows/glow/gradients/glass introduced;
  second corner radius or third text size added; per-row start spinners.

Retry cap: 2, then escalate to the user.
