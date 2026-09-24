---
name: git-commit
description: 'Execute git commit with conventional commit message analysis, intelligent staging, and message generation. Use when user asks to commit changes, create a git commit, or mentions "/commit". Supports: (1) Auto-detecting type and scope from changes, (2) Generating conventional commit messages from diff, (3) Interactive commit with optional type/scope/description overrides, (4) Intelligent file staging for logical grouping'
---

# Git Commit with Conventional Commits

Create standardized, semantic git commits using the Conventional Commits specification. Analyze the actual diff to determine appropriate type, scope, and message.

## Workflow

### 1. Assess the working tree

```bash
git status
git diff --stat
git diff --cached --stat
```

### 2. Decide commit boundaries (split if needed)

Split by: feature vs refactor, backend vs frontend, formatting vs logic, tests vs prod code, dependency bumps vs behavior changes.

If changes are mixed in one file, plan to use patch staging. Default to multiple small commits when there are unrelated changes.

### 3. Handle unstaged changes

If nothing is staged (`git diff --cached` is empty):

1. Show the user what files have changed
2. Suggest what to stage based on logical grouping (e.g., "these 3 files are all related to the auth refactor")
3. Ask if they want to stage all, or select specific files
4. Stage the approved files with `git add <files>`

If changes are already staged, proceed to analysis.

### 4. Stage only what belongs in the next commit

Prefer patch staging for mixed changes: `git add -p`. To unstage: `git restore --staged -p` or `git restore --staged <path>`.

### 5. Analyze the diff

Read the full staged diff:

```bash
git diff --cached
```

Determine the commit type from the changes:

| Signal                                              | Type       |
| --------------------------------------------------- | ---------- |
| New files with new functionality                    | `feat`     |
| New test files or test additions                    | `test`     |
| Changes to existing logic fixing incorrect behavior | `fix`      |
| Structural changes without behavior change          | `refactor` |
| package.json, tsconfig, CI config changes           | `chore`    |
| Build/bundler config changes                        | `build`    |
| README, docs, comments only                         | `docs`     |
| Formatting, whitespace, semicolons only             | `style`    |
| Performance improvements                            | `perf`     |

Determine the scope from the primary directory or module affected:

- `src/api/` -> `api`
- `src/components/auth/` -> `auth`
- `tests/` -> `tests`
- Root config files -> omit scope
- Multiple unrelated areas -> omit scope

Full type table: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`.

### 6. Check for user overrides

If the user provided arguments via `$ARGUMENTS`:

- Single word (e.g., `fix`) -> use as commit type
- Two words (e.g., `refactor api`) -> use as type and scope
- Otherwise -> use auto-detected values

### 7. Review what will actually be committed

```bash
git diff --cached
```

Sanity checks:

- no secrets or tokens
- no accidental debug logging
- no unrelated formatting churn

Describe the staged change in 1-2 sentences ("What changed?" + "Why?"). If you cannot describe it cleanly, the commit is probably too big or mixed; go back to step 2.

### 8. Compose the commit message

Format: `type(scope): imperative short description`

Rules:

- Subject line max 72 characters
- Use imperative mood ("add", "fix", "refactor", not "added", "fixes")
- Don't end with a period
- Body explains **WHY** this change was made, not what changed (the diff shows what)
- If changes are trivial (typo fix, formatting), skip the body
- Reference issues: `Closes #123`, `Refs #456`

Example:

```
feat(auth): add JWT refresh token rotation

Tokens were expiring mid-session for users with slow connections.
Rotating refresh tokens extends the session without compromising
security, since each refresh token can only be used once.
```

The body should answer "if someone reads this commit in 6 months, will they understand WHY?"

Breaking changes: add `!` after the scope (`feat(api)!: change response format`), plus a `BREAKING CHANGE:` footer.

Multi-line execution:

```bash
git commit -m "$(cat <<'EOF'
<type>[scope]: <description>

<optional body>

<optional footer>
EOF
)"
```

### 9. Confirm and commit

Show the user the proposed commit message and ask for confirmation.

If confirmed, run `git commit -m "<message>"` (prefer `git commit -v` for multi-line). Then verify with `git log --oneline -1` and show the hash.

### 10. Verify and repeat

Run the repo's fastest meaningful check (unit tests, lint, or build) before moving on. Repeat from step 2 until the working tree is clean.

Run after completing a logical unit of work, not after every file change.

## Git Safety Protocol

- **NEVER update git config.**
- **NEVER run destructive commands (--force, hard reset) without explicit request.**
- **NEVER skip hooks (--no-verify) unless user asks.**
- **NEVER force push to main/master.**
- **NEVER amend pushed/shared commits.** Amend only unpushed work with explicit request.
- **NEVER commit secrets** (.env, credentials.json, private keys).
- Watched patterns — stop and confirm before executing, suggesting a safer alternative: rm -rf (especially /, ~, or project root), git push --force, git reset --hard, git checkout . (discard all changes), DROP TABLE / DROP DATABASE, docker system prune, kubectl delete, chmod 777, sudo rm, npm publish (accidental publishes), any command with --no-verify.
- If commit fails due to hooks, fix and create NEW commit (don't amend).
- Sign (`-S`) when the repo requires it.

## Next

Committed → `/finish` (merge/PR menu) or keep working.

## Optional

- make-pr-easy-to-review (PR body, reviewer guidance once the branch is ready)
