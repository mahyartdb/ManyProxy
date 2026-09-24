---
description: Deliberate entry to idea shaping: triage, adversarial questioning, kill/keep pressure. Use when an idea is vague, untested, or needs stress before planning. Drives idea-refine (+ask-me).
---

## User Input

```text
$ARGUMENTS
```

The raw idea (1–2 sentences plus why-now suffices; more context welcome).

## Run

Load the `idea-refine` skill with the input. Follow it exactly: triage aloud, Entry capture, divergent/convergent questioning through `ask-me` mechanics, Problem Definition, Scorecard, HARD-GATE. No implementation, no spec.

## Next

Required chain from here:

1. Settled intent or verdict needed → `/decide` (go), then `/spec`, then `/plan`, then `/to-tickets`.
2. Unsettled, needs evidence → `/research` (then back to refine).

## Optional

- `/council` — ambiguous tradeoff surfaced mid-refine (before committing to a direction).
- `prototype` skill — feasibility question cheaper to probe than to argue (invoke directly, no door; spike first, refine after).
- `ask-me` — divergent/convergent questioning mechanics (during refine, as Run holds).
