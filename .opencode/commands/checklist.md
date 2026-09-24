---
description: Generate reviewer-owned requirements-quality checklist for a feature domain. Runs after plan, before tickets. Drives checklist.
---

## User Input

```text
$ARGUMENTS
```

Feature directory plus domain (`ux`, `api`, `security`, `performance`).

## Run

Load the `checklist` skill with the input. Follow it exactly: clarify intent, generate `checklists/[domain].md` with CHK IDs, all `[ ]`. Never verifies implementation.

## Next

Required chain from here:

1. Reviewer works the checklist (`[x]` = criterion satisfied).
2. `/to-tickets`, then `/decide` (post-tickets gate), then `/implement` (reads state, never modifies markers).

## Optional

- Re-run per domain — one file each, same run-when (after `/plan`, before `/to-tickets`).
- `ask-me` — resolve `[Ambiguity]` markers reviewer kicks back (during review).
- `/decide` — post-tickets gate stays mandatory even when green (after `/to-tickets`, before `/implement`).
