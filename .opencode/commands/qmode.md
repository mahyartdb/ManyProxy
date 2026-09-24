---
description: User-typed entry to the qmode orchestrator. Explicit control plane the skill trigger cannot carry.
---

## User Input

```text
$ARGUMENTS
```

Phase (`phase`, `status`, `resume`) plus `--policy`, `--task`, `--spec` flags.

## Run

Parse args, invoke the `qmode` skill with the struct. Skill holds the state machine; this door holds no logic.

## Next

Required chain (`/qmode status` shows position):

1. `idea` → `/refine`, `/research`, `/decide` (kill-gate; go continues to `/spec`). `spec` → `/spec`, `/plan`, `/checklist`, `/to-tickets` (frozen-design gate), `/decide` post-tickets (kill-gate; go continues to `/implement`, never back to `/spec`).
2. `implement` → `/implement`, `/converge` (loop until converged), `/review`, `/finish`. `ship` → separate `/ship` run (`shipping-and-launch`; qmode stops at mergeable).

## Optional

- `cross-check` — adversarial pass (after `/implement`, before review).
- `/council` — tradeoff call mid-loop.
- `diagnosing-bugs` — failure (pauses loop).
- `prototype` — spike before starting, never mid-loop.
