---
description: Deliberate four-voice decision entry for ambiguous tradeoffs and go/no-go calls. Use when multiple valid paths exist and structured disagreement is needed first. Drives council.
---

## User Input

```text
$ARGUMENTS
```

The decision question plus constraints and what counts as success. Vague questions get one clarifying question first.

## Run

Load the `council` skill with the question. Follow it exactly: architect position first, three independent voices in parallel, synthesis with dissent preserved, compact verdict. Council decides between paths; it never verifies output.

## Next

Required chain from here:

1. New direction → `/decide` (then `/spec` on go). Design-only on settled spec → `/plan`. Buildable tickets → `/decide` post-tickets gate then `/implement`. Kill-grade call → `/decide`.

## Optional

- `cross-check` — the verdict needs proving against output (after the decision, before acting on it).
