---
description: End-of-branch gate: fresh full-suite verify, environment detect, merge / pull-request / keep-as-is menu. Integration is the human's decision. Drives finishing-a-development-branch. Short door name.
---

## User Input

```text
$ARGUMENTS
```

Base branch guess (confirmed before merging — never assumed).

## Run

Load the `finishing-a-development-branch` skill. Follow it exactly: verify tests fresh on the current tree, detect repo vs worktree, confirm base, present the menu verbatim, wait, execute the choice, clean up. Refuse if no green `/review` verdict covers HEAD.

## Next

Required chain from here:

1. Merged locally → `/ship` when production is next, else done.
2. PR opened → `/pr-ready` if the PR needs polish, else wait on review.

## Optional

- `/commit` — uncommitted work blocks the gate (before verify, commit first).
- `/review` — green verdict over HEAD is the entry prerequisite (before verify, review first).
- `using-git-worktrees` — branch isolation strategy (before merging, when sharing the tree).
