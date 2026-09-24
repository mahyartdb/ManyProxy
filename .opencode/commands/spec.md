---
description: Phase gate turning settled intent into a buildable spec (kernel, seams, boundaries, verification). Use after intent is settled, before plan or tickets. Drives spec. Never interviews.
---

## User Input

```text
$ARGUMENTS
```

Settled intent: the decision plus constraints, or a pointer to the brief (idea-refine one-pager, decide handoff).

## Run

Load the `spec` skill with the input. Follow it exactly: synthesize kernel, seams, boundaries, verification. The skill never interviews — unclear aspects get informed guesses under Assumptions plus `[NEEDS CLARIFICATION]` markers.

## Next

Required chain from here:

1. `/plan`, then `/to-tickets`, then `/decide` (post-tickets gate), then `/implement`.

## Optional

- `ask-me` — resolve NEEDS CLARIFICATION markers research cannot answer (after spec, before plan).
- `/checklist` — requirements-quality gate (after `/plan`, before tickets).
- `archify` — Phase 1 framing (after spec, before plan).
- `technical-writing` — spec clarity pass (after drafting, before plan).
