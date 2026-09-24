---
description: Scoped diagnosis-loop entry for broken, failing, or slow behavior. Drives diagnosing-bugs. Short door name; engine skill is diagnosing-bugs.
---

## User Input

```text
$ARGUMENTS
```

The symptom: what broke, what's failing, what's slow, with repro if known.

## Run

Load the `diagnosing-bugs` skill with the symptom. Follow it exactly: reproduce + minimise before theorizing, fix root cause in source with atomic commits, re-verify. Never patch symptoms to match expected output.

## Next

Required chain from here:

1. Fix verified → `/to-tickets` (fix ticket), then `/implement` (its chain converges + reviews). No direct-to-commit bypass: every fix lands through tickets. Tests fresh before landing.

## Optional

- `test-driven-development` — lock the fix with a failing-first regression test (during the fix, before closing).
- `e2e-testing` — user-facing symptom needs end-to-end cover (after the fix, before landing).
