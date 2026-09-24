---
description: Execute implementation planning for a feature with spec.md present. Runs after spec, before tickets. Drives plan.
---

## User Input

```text
$ARGUMENTS
```

Feature directory holding `spec.md` (defaults to current directory).

## Run

Load the `plan` skill with the input. Follow it exactly: setup, Technical Context, Constitution Check, Phase 0 research, Phase 1 design. No tasks, no code.

## Next

Required chain from here:

1. `/checklist` optional gate, then `/to-tickets`, then `/decide` (post-tickets gate), then `/implement`.

## Optional

- `/checklist` — requirements-quality gate (after `/plan`, before `/to-tickets`).
- `ask-me` — NEEDS CLARIFICATION research cannot answer (Phase 0, before design).
- `archify` — Phase 1 framing (during design, before tickets).
