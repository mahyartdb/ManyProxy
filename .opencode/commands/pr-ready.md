---
description: PR polish run: clean noisy history, write what/why body, add reviewer guidance. Never changes code behavior. Drives make-pr-easy-to-review. Short door name.
---

## User Input

```text
$ARGUMENTS
```

PR URL or branch. Defaults to the current branch's PR.

## Run

Load the `make-pr-easy-to-review` skill. Follow it exactly: tidy commits, body with what/why, reviewer guidance. Behavior changes are out of scope — route those to `/implement`.

## Next

Required chain from here:

1. PR ready → wait on review; review feedback → `/implement` (fixes), then `/converge`, then `/review`, then `/finish`.

## Optional

- `/review` — self-review pass before publishing (after polish, before marking ready).
- `no-comments` — low priority, consider only if comment noise blocks review (after polish, before marking ready).
