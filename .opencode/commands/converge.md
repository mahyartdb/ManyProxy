---
description: User-typed entry to the converge skill. Runs the drift check now, after implement.
---

## User Input

```text
$ARGUMENTS
```

Feature directory holding `spec.md`, `plan.md`, and `tasks.md` (defaults to current directory when they sit at its root).

## Run

Load the `converge` skill with the resolved directory. Follow it exactly: locate artifacts (STOP naming the prerequisite if any is missing), assess, summarize findings, append or report converged, recommend next actions.

## Next

Required chain from here:

1. `tasks_appended` → `/implement` (complete the appended Convergence tasks), then `/converge` again (should find fewer or none). MAX 3 rounds total across nested gates, then escalate to the user.
2. `converged` → `/review`, then `/finish` (merge/PR menu).

## Optional

- `cross-check` — adversarial dual-review of the implementation (after `/implement`, before review). Catches what converge's gap lens misses.
- `diagnosing-bugs` — drift is a defect with a repro (during convergence, before appending tasks).
