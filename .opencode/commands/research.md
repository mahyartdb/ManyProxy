---
description: Scoped background investigation against high-trust primary sources, filed as findings. Use when facts must be gathered before deciding or specifying. Drives research.
---

## User Input

```text
$ARGUMENTS
```

The question plus any must-read sources or constraints.

## Run

Load the `research` skill with the input. Follow it exactly: investigate, label Fact/Inference/Assumption, file the brief. Findings, not decisions.

## Next

Required chain from here:

1. Findings → `/decide`, then `/spec`; when idea still forming → `/refine` then `/decide`.
2. Evidence against an idea → `/decide`.

## Optional

- `ask-me` — scope the question before delegating (during briefing, before the background run).
