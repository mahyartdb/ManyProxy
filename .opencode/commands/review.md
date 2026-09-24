---
description: Gate diff review before merge: plan alignment, bugs, security, testing, production readiness. Drives code-review. Short door name; engine skill is code-review.
---

## User Input

```text
$ARGUMENTS
```

Base..head range, branch, or PR scope. Defaults to the current diff against the base branch.

## Run

Load the `code-review` skill with the resolved scope. Follow it exactly: read-only verdict, structured findings. Route: `code-review` default; `cross-check` with no human review; `ponytail-review` for over-engineering.

## Next

Required chain from here:

1. Fixes → `/implement` → `/converge` → `/review`, then `/finish` (merge/PR menu). MAX 3 rounds total across nested gates, then escalate to the user.
2. Required gate: `/finish`, `/pr-ready`, `/ship` stay red until this gate is green.

## Optional

- `cross-check` — adversarial dual-review for high-stakes output (after `/implement`, before this gate).
- `ponytail-review` — simplicity pass on flagged over-engineering (after verdict, before fixes).
- `no-comments` — comment hygiene (after verdict, before fixes).
- `jev` — calibrate severity during review (during review, before verdict).
