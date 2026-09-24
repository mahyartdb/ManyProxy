---
description: Conventional-commit run with intelligent staging and message generation. Drives git-commit. Short door name; engine skill is git-commit.
---

## User Input

```text
$ARGUMENTS
```

Optional type/scope/description overrides. Empty input means auto-detect all three from the diff.

## Run

Load the `git-commit` skill. Follow it exactly: analyze the diff, stage a logical group, generate the conventional message, confirm, commit. Bypass flags (`--no-verify`) are rejected, never executed.

## Next

Required chain from here:

1. Committed → `/finish` when the branch is done, or keep working. `/finish` still requires green `/review` over HEAD; no `/converge`/`/review` bypass.

## Optional

- `make-pr-easy-to-review` — history needs tidying before the PR (after commits, before push).
- `using-git-worktrees` — isolate the commit run on a worktree (before committing, when the tree is dirty).
