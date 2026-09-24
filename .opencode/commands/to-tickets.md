---
description: Scoped breakdown of a spec or plan into tracer-bullet tickets with blocking edges and an approval gate. Drives to-tickets. Exact-mirror door: command name matches the engine skill.
---

## User Input

```text
$ARGUMENTS
```

The `spec.md` path (or plan path); defaults to the current directory's spec.

## Run

Load the `to-tickets` skill with the resolved spec/plan. Follow it exactly: declare the task graph with blocking edges, publish per-ticket files (or native tracker links), present for approval. No code.

## Next

Required chain from here:

1. Approved tickets → `/decide` (post-tickets kill-gate), then `/implement`, then `/converge`.

## Optional

- `/council` — scope or sequencing dispute in the breakdown (before approval).
- `/plan` — repair-path only when no plan artifact exists (before tickets, after spec).
