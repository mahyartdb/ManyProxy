---
name: implement
description: Build only what tickets and spec authorize, in thin vertical slices, fanning parallel implementer subagents across the ticket frontier. Use when a spec or ticket set exists and code must be written ("build this", "implement the tickets"). Refuses without tickets ("Implementation plan is required"). Do not use for vague ideas (idea-refine), writing the plan itself (to-tickets), or verifying output (code-review, cross-check).
---

# Implement

Implement the work described by the user in the spec or tickets.

You have been provided a spec. This spec should have tickets associated with it, describing how to implement the spec.

Only make the changes explicitly specified in the tickets. If the user has not passed the tickets as an input, respond with: "Implementation plan is required."

Implement ONLY what is specified in the tickets. DO NOT WRITE ANY CODE OUTSIDE OF WHAT IS SPECIFIED IN THE TICKETS.

The tickets are not a list of steps. They are a **task graph** with blocking relationships between them. This means there is always a **frontier** of tickets which are ready to be grabbed.

Communication to and from subagents should be sparse. Communicate primarily through **context pointers**: to the spec, tickets, research notes, and previous commits. Don't duplicate information already available via pointers.

Read the spec and tickets. Read enough to understand the task graph.

If this changes the **frontier** of available tickets, kick off more **implementer subagents** to work on the new tickets. This allows for maximum concurrency.

Before a large push, write a compact matrix:

```text
Lane | Can run in parallel? | Write surface | Risk | Verification
Repo scan | yes | none | low | rg/git status outputs
Backend patch | maybe | src/api | medium | unit tests
Frontend patch | maybe | app/components | medium | browser screenshot
Deploy readback | after build | remote service | high | live URL + logs
```

Only run lanes in parallel when their write surfaces do not collide.

Before the first edit to a file, present these facts:

1. List ALL files that import/require this file (search the tree — Glob/Grep, or find/grep via Bash)
2. List the public functions/classes affected by this change
3. If this file reads/writes data files, show field names, structure,
   and date format (use redacted or synthetic values, not raw production data)
4. Quote the user's current instruction verbatim

Before creating a new file, present these facts:

1. Name the file(s) and line(s) that will call this new file
2. Confirm no existing file serves the same purpose (search the tree — Glob/Grep, or find/grep via Bash)
3. If this file reads/writes data files, show field names, structure,
   and date format (use redacted or synthetic values, not raw production data)
4. Quote the user's current instruction verbatim

Before any destructive command (`rm -rf`, `git reset --hard`, `git push --force`, `drop table`, etc.):

1. List all files/data this command will modify or delete
2. Write a one-line rollback procedure
3. Quote the user's current instruction verbatim

## The Increment Cycle

Build in thin vertical slices — implement one piece, test it, verify it, then expand. Avoid implementing an entire feature in one pass. Each increment should leave the system in a working, testable state.

For each slice:

1. **Implement** the smallest complete piece of functionality
2. **Test** — run the test suite (or write a test if none exists)
3. **Verify** — confirm the slice works as expected (tests pass, build succeeds, manual check)
4. **Commit** -- save your progress with a descriptive message
5. **Move to the next slice** — carry forward, don't restart

Use test-driven-development where possible, at pre-agreed seams.

## Optional domain skills (max 1-2 per slice)

Never load all domain skills. Per slice, load at most 1-2 owning skills matching the slice's triggers:

| Slice concern          | Trigger              | Owning skill(s)                                                                                                                                                                                                                        |
| ---------------------- | -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Language               | stack in use         | typescript, python-patterns, golang-patterns, rust-mastery + matching rust-\*, java-coding-standards, kotlin-patterns, swiftui-patterns, cpp-coding-standards, dotnet-patterns / csharp-testing, fsharp-testing, dart-flutter-patterns |
| Framework              | framework in slice   | react-patterns, vue, nuxt, fastapi-patterns, django-patterns, kotlin-ktor-patterns, rust-axum                                                                                                                                          |
| Contract + data        | contract change      | api-and-interface-design; schema/cache change → database-migrations + supabase-postgres-best-practices / supabase / sqlite + redis-patterns                                                                                            |
| Security / perf / a11y | auth, input, PII     | security-and-hardening; perf budget → performance-optimization; UI → accessible-frontend                                                                                                                                               |
| Styling / motion       | styles, animation    | css / tailwind / unocss / animate / gsap-\*                                                                                                                                                                                            |
| Cloudflare             | workers, edge, KV/DO | workers-best-practices / wrangler / durable-objects / turnstile                                                                                                                                                                        |

Record the loaded skill(s) in the ledger line for the slice.

Run typechecking regularly, single test files regularly, and the full test suite once at the end.

## Slicing Strategies

### Vertical Slices (Preferred)

Build one complete path through the stack:

```
Slice 1: Create a task (DB + API + basic UI)
    → Tests pass, user can create a task via the UI

Slice 2: List tasks (query + API + UI)
    → Tests pass, user can see their tasks

Slice 3: Edit a task (update + API + UI)
    → Tests pass, user can modify tasks

Slice 4: Delete a task (delete + API + UI + confirmation)
    → Tests pass, full CRUD complete
```

Each slice delivers working end-to-end functionality.

### Contract-First Slicing

When backend and frontend need to develop in parallel:

```
Slice 0: Define the API contract (types, interfaces, OpenAPI spec)
Slice 1a: Implement backend against the contract + API tests
Slice 1b: Implement frontend against mock data matching the contract
Slice 2: Integrate and test end-to-end
```

### Contract Change Protocol

Never change implementation first and update the contract afterward.

1. Propose the consumer need and compatibility impact.
2. Change the canonical artifact.
3. Review the contract diff with affected consumers and the provider.
4. Regenerate types, clients, or fixtures.
5. Update provider and consumer implementations.
6. Run consumer and provider verification.
7. Merge only when all affected sides agree on the new contract.

For an additive change, verify that old consumers continue to work. For a breaking change, use the repository's versioning or migration policy rather than silently repurposing an existing field.

Contract shape rules live in [references/api-design-checklist.md](references/api-design-checklist.md). Error handling rules live in [references/error-handling.md](references/error-handling.md).

### Risk-First Slicing

Tackle the riskiest or most uncertain piece first:

```
Slice 1: Prove the WebSocket connection works (highest risk)
Slice 2: Build real-time task updates on the proven connection
Slice 3: Add offline support and reconnection
```

If Slice 1 fails, you discover it before investing in Slices 2 and 3.

## Implementation Rules

### Rule 0: Simplicity First

Before writing any code, ask: "What is the simplest thing that could work?"

After writing code, review it against these checks:

- Can this be done in fewer lines?
- Are these abstractions earning their complexity?
- Would a staff engineer look at this and say "why didn't you just..."?
- Am I building for hypothetical future requirements, or the current task?

```
SIMPLICITY CHECK:
✗ Generic EventBus with middleware pipeline for one notification
✓ Simple function call

✗ Abstract factory pattern for two similar components
✓ Two straightforward components with shared utilities

✗ Config-driven form builder for three forms
✓ Three form components
```

Three similar lines of code is better than a premature abstraction. Implement the naive, obviously-correct version first. Optimize only after correctness is proven with tests.

### Rule 0.5: Scope Discipline

Touch only what the task requires.

Do NOT:

- "Clean up" code adjacent to your change
- Refactor imports in files you're not modifying
- Remove comments you don't fully understand — route comment-dense diffs via no-comments instead
- Add features not in the spec because they "seem useful"
- Modernize syntax in files you're only reading

If you notice something worth improving outside your task scope, note it — don't fix it:

```
NOTICED BUT NOT TOUCHING:
- src/utils/format.ts has an unused import (unrelated to this task)
- The auth middleware could use better error messages (separate task)
→ Want me to create tasks for these?
```

### Rule 1: One Thing at a Time

Each increment changes one logical thing. Don't mix concerns:

**Bad:** One commit that adds a new component, refactors an existing one, and updates the build config.

**Good:** Three separate commits — one for each change.

### Rule 2: Keep It Compilable

After each increment, the project must build and existing tests must pass. Don't leave the codebase in a broken state between slices.

## Rulings, not stalls.

Conflicts, ambiguities, ticket defects — decide them. The spec is the binding authority, the tickets are its argument, and your judgment settles what neither answers. Record every decision in the ledger as `Ruling: <what you decided> — <why> — <what it costs if wrong>`, and keep going. Deviating from the tickets without a ledgered ruling is a decision made in secret.

Track progress in a ledger file, not only in todos. Harness todos are a live view; the ledger is the record.

Stall signals — stop and recover, don't grind:

- loop churn without measurable progress
- repeated retries with same root cause
- merge queue stalls
- cost drift from unbounded escalation

Recovery: freeze loop, reduce scope to failing unit, replay with explicit acceptance criteria.

- **The code is wrong.** Use diagnosing-bugs. Find the cause; never patch the symptom to make the step's output match.
- **The ticket is wrong** — a step contradicts the spec, an interface from an earlier ticket doesn't match what this ticket consumes, a command that cannot work. Rule on the smallest change that satisfies the spec, ledger it as `Task <N>: Ruling: <finding> — <what you decided and why>`, and continue.

## The completion contract

Before a ticket's ledger line, all of the following are true, with evidence in this session — not inferred from the diff looking right:

- Every test the ticket names exists and ran in this ticket, and you read the output.
- The final test run for the ticket passed, and it writes the command and result into the ledger line.
- Every `Expected:` line in the ticket was compared against real output.
- Every deviation from the ticket has a `Ruling:` line in the ledger.

`Task <N>: complete (commits <base7>..<head7>, tests: <command> → <result>)`

## Verification Gate

Run in order using the repo's runner commands (see test-driven-development); the phases and their order are what matter:

Optional (verify-<app>): when a project-local `verify-<app>` skill exists, drive the user path through it as the end-to-end proof.

### Phase 1: Build Verification

If build fails, STOP and fix before continuing.

### Phase 2: Type Check

Report all type errors. Fix critical ones before continuing.

### Phase 3: Lint Check

### Phase 4: Test Suite

Run tests with coverage. Target: 80% minimum.

Report:

- Total tests: X
- Passed: X
- Failed: X
- Coverage: X%

### Phase 5: Security Scan

Grep for secrets, api keys, and stray console.log in source.

### Phase 6: Diff Review

```bash
# Show what changed
git diff --stat
git diff HEAD~1 --name-only
```

Review each changed file for:

- Unintended changes
- Missing error handling
- Potential edge cases

After running all phases, produce a verification report:

```
VERIFICATION REPORT
==================

Build:     [PASS/FAIL]
Types:     [PASS/FAIL] (X errors)
Lint:      [PASS/FAIL] (X warnings)
Tests:     [PASS/FAIL] (X/Y passed, Z% coverage)
Security:  [PASS/FAIL] (X issues)
Diff:      [X files changed]

Overall:   [READY/NOT READY] for PR

Issues to Fix:
1. ...
2. ...
```

## Optional enhancements

- Optional (explorer): when importer mapping or call-site discovery is needed, invoke explorer (read-only).
- Optional (coding-standards, security-and-hardening, performance-optimization, api-and-interface-design): invoke per ticket type — standards for quality, security for input/auth/PII, performance for perf tickets, api-design for Contract-First slices.
- Optional (inherit-legacy-style): when `.ai-style-rules.md` exists, comply with its Golden Files / Naming Rules / DONTs on the first edit.
- Optional (no-comments): when the diff is comment-dense, route via no-comments.
- Optional (frontend-design, hallmark, impeccable, antfu-design, design-md): for UI tickets — greenfield direction via frontend-design, page-scale builds via hallmark, product/design-system UI via impeccable, UnoCSS token/component work via antfu-design, locked tokens via design-md (gpui-kit-design-guides for desktop GPUI).
- full-output-enforcement [PAUSED]: on truncation, end with `[PAUSED — X of Y… continue]` and resume on "continue"; never default to it.

## Next

Required: run `/converge` (drift check) once done.

- `tasks_appended` → `/implement` the appended tasks, then reconverge.
- `converged` → `/review`, then `/finish`.
- MAX 3 `implement`/`converge` rounds total (nested gates share the count), then escalate.

Commit your work to the current branch.

Use finishing-a-development-branch.

## Optional

- test-driven-development (RED-GREEN-refactor at pre-agreed seams)
- e2e-testing (user-facing flows)
- observability-and-instrumentation (instrument alongside the feature, verify before review)
- diagnosing-bugs (unknown cause only — `/debug`, Iron Law: no fix without root cause)
- cross-check (trigger-gated: high-stakes / production output with no human review)
- ponytail-review (after a passing verdict — simplicity pass, never before correctness)
