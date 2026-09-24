---
description: Gated build run over tickets with frontier fan-out across implementer subagents. Refuses without tickets. Drives implement.
---

## User Input

```text
$ARGUMENTS
```

Ticket IDs, a `tasks.md` path, or the feature directory. No tickets anywhere is an error ("Implementation plan is required"), not a guess.

## Run

Load the `implement` skill with the resolved tickets. Follow it exactly: ticket graph, frontier fan-out, thin vertical slices, completion contract per ticket (tests ran, Expected: compared, Rulings ledgered), verification gate.

## Next

Required chain from here:

1. `/converge` (drift check), then `/review`, then `/finish`.

## Optional

- `diagnosing-bugs` — failure mid-slice (pause the slice, root-cause, resume in-scope only).
- `test-driven-development` — red-green-refactor discipline at pre-agreed seams (during slices).
- `cross-check` — adversarial pass over slices (after slices, before `/converge`).
- `e2e-testing` — user-facing slices need end-to-end cover (after slices, before `/converge`).
