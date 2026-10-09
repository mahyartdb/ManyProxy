# 05: Honest dialog footers

**What to build:** Zero footer-label lies across all dialogs: closing without
saving reads `Close`/`Cancel`, commits name their verb, destructive commits
carry danger styling, and the Logs viewer distinguishes committing a filter
from discarding it.

**Blocked by:** None (can start immediately). [P]

**Owns:** rust-testing

**Status:** ready-for-agent

- [ ] Scenario: walk every dialog footer → Action: read commit/close labels →
  Expected: no button implies an action it does not perform (attach, add,
  done-as-close cases fixed). Verification: automated test. Priority: required.
- [ ] Scenario: any destructive confirm → Action: render confirm → Expected:
  danger styling on the affirmative naming its result. Verification: automated test. Priority: required.
- [ ] Scenario: Logs viewer with edited filter → Action: read footer →
  Expected: committing vs discarding the filter distinctly labeled.
  Verification: manual UX review. Priority: important.
- [ ] Prohibited side effect: bare `OK` footers introduced; README-claimed
  button labels altered beyond the lie fixes.

Retry cap: 2, then escalate to the user.
