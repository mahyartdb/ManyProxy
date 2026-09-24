---
description: Pre-launch gate: readiness checklist, staged rollout, monitoring, rollback strategy. Use when production is next. Drives shipping-and-launch. Short door name.
---

## User Input

```text
$ARGUMENTS
```

The launch scope: what ships, to whom, in what stages.

## Run

Load the `shipping-and-launch` skill with the scope. Follow it exactly: pre-launch checklist, rollout plan, monitoring, rollback. Qmode stops at mergeable — this door is the separate explicit consent for going live. Refuse if no green `/review` verdict covers HEAD.

## Next

Required chain from here:

1. Green checks → staged rollout per plan; monitor, then done. Prod feedback starts new loop at `/refine` (or `/research` for unknowns), through `/decide`.

## Optional

- `diagnosing-bugs` — launch surfaces a defect (pause rollout, root-cause, resume or roll back).
- `observability-and-instrumentation` — rollout telemetry (during rollout, before done).
- `canary-watch` — staged cohort monitoring (during rollout, before full release).
- `ci-cd-and-automation` — release pipeline checks (before rollout, during launch).
