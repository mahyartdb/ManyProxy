---
description: User-typed entry to the decide skill. Renders the go / needs-clarification / kill verdict now.
---

## User Input

```text
$ARGUMENTS
```

Pointer to the assessed idea: a `.planning/<slug>/` directory, an idea-refine one-pager path, or the problem statement itself.

## Run

Load the `decide` skill with the resolved inputs. Follow it exactly: score, verdict, record, report back with the next step. Never spec or build inside this run.

## Next

Required chain by verdict:

1. `go` → pre-spec gate: `/spec` (handoff summary is its input), then `/plan`, then `/to-tickets`. Post-tickets gate: `/implement` (tickets already approved) — never back to `/spec`.
2. `needs-clarification` → re-run the named stage (`/refine` or `/research`), then `/decide` again.
3. `kill` → none. Assessment closed; record remains.

## Optional

- `/council` — second-opinion deliberation before accepting a `go` on high-stakes calls (after verdict, before `/spec`).
- `ask-me` — grill the blocking questions behind a `needs-clarification` (during the revisit stage).
- `prototype` — feasibility spike cheaper than analysis (spike first, then re-run the stage).
