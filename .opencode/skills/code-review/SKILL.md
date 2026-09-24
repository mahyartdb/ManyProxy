---
name: code-review
description: Correctness review of a diff via a read-only reviewer: plan alignment, bugs, security, testing, production readiness. Use before merge, after major features, after each ticket. Over-engineering goes to ponytail-review instead.
---

# Code Review

Dispatch a code reviewer subagent to catch issues before they cascade. The reviewer gets precisely crafted context for evaluation — never your session's history. Reviewing the diff inline burns the context window you need to keep driving the work: the diff and the evaluation live in its context, and only the findings come back to you.

**Core principle:** Review early, review often.

The unit of review is the **agreement**, not the file. Correctness is the core — agreements between participants across a boundary. Performance and security are sub-cases of the same engine: Performance anchors workload → resource demand → growth or contention → consequence; Security anchors source → trust boundary → sink, with an attacker controlling the source. One root cause can carry correctness and security impact — report it once, with both impacts.

## When to Request Review

**Mandatory:**

- After each ticket in subagent-driven development
- After completing major feature
- Before merge to main

**Optional but valuable:**

- When stuck (fresh perspective)
- After fixing complex bug

**Never:** skip review because "it's simple".

Not this skill: spec-to-code gaps → `converge`; high-stakes adversarial verification → `cross-check`; requirements quality → `checklist`.

## Route the diff (load at most 1-2 owning skills)

| Diff signal                        | Owning skill(s)                                       |
| ---------------------------------- | ----------------------------------------------------- |
| `*.vue`, Pinia stores, Nuxt routes | vue, pinia, nuxt                                      |
| `*.rs`                             | rust-mastery + matching rust-\*                       |
| `migrations/*`, SQL, Supabase      | database-migrations, supabase-postgres-best-practices |
| `Dockerfile`, compose, image       | docker-patterns                                       |
| auth, input, PII, secrets          | security-and-hardening                                |
| UI / a11y markup                   | accessible-frontend                                   |

## How to Request

**1. Get git SHAs:**

```bash
BASE_SHA=$(git rev-parse HEAD~1)  # or: git merge-base origin/main HEAD
HEAD_SHA=$(git rev-parse HEAD)
```

**2. Dispatch reviewer:** invoke `@general` (or a code-review subagent) with description, plan/requirements, BASE_SHA, HEAD_SHA filled into `reviewer-template.md` beside this file. The reviewer is read-only: no working-tree mutation, no subagent spawning — one reviewer, one pass; if the diff feels too large, it reviews in passes and says so.

**3. Act on feedback:**

- Fix Critical issues immediately
- Fix Important issues before proceeding
- Note Minor issues for later
- Push back if reviewer is wrong (with technical reasoning and code/tests as proof)

## Change Sizing

Small, focused changes are easier to review, faster to merge, and safer to deploy:

```
~100 lines changed   → Good. Reviewable in one sitting.
~300 lines changed   → Acceptable if it's a single logical change.
~1000 lines changed  → Too large. Split it.
```

**Watch file size, not just diff size.** Around 1000 _total_ lines in a single file is a common inspection signal. When a change materially grows an already-large file, extract helpers or split modules _first_, before piling more on.

## Severity

Label every comment with its severity so the author knows what's required vs optional:

| Prefix                        | Meaning            | Author Action                                           |
| ----------------------------- | ------------------ | ------------------------------------------------------- |
| _(no prefix)_                 | Required change    | Must address before merge                               |
| **Critical:**                 | Blocks merge       | Security vulnerability, data loss, broken functionality |
| **Nit:**                      | Minor, optional    | Author may ignore — formatting, style preferences       |
| **Optional:** / **Consider:** | Suggestion         | Worth considering but not required                      |
| **FYI**                       | Informational only | No action needed — context for future reference         |

**Lead with what matters.** Order findings by leverage: correctness and security first, then structural regressions and missed simplifications, then everything else. Don't bury a real issue under cosmetic nits — a few high-conviction comments beat a long list. If you have one structural problem and ten nits, the structural problem _is_ the review.

## Structural Remedies

When you flag a structural problem, propose the move — not just the problem:

- **Replace a chain of conditionals** with a typed model or an explicit dispatcher.
- **Collapse duplicate branches** into a single clearer flow.
- **Separate orchestration from business logic** so each reads on its own.
- **Move feature-specific logic** out of a shared module into the package that owns the concept.
- **Reuse the canonical helper** instead of a bespoke near-duplicate.
- **Delete a pass-through wrapper** that adds indirection without clarifying the API.
- **Extract a helper, or split a large file** into focused modules.

Prefer the remedy that removes moving pieces over one that spreads the same complexity around.

**Don't accept "I'll clean it up later."** Deferred cleanup rarely happens. Require cleanup before submission unless it's a genuine emergency.

## Smell Baseline

On top of whatever the repo documents, always carry this fixed set of Fowler code smells (_Refactoring_, ch.3). Two rules bind it:

- **The repo overrides.** A documented repo standard always wins; where it endorses something the baseline would flag, suppress the smell.
- **Custom project rules.** If the repo defines review rules, resolve them in this priority order: explicit flag, repo rule file, home rule file, built-in defaults. By default, the first matching user rule replaces the built-in rule.
- **Always a judgement call.** Each smell is a labelled heuristic, never a hard violation. Skip anything tooling already enforces.

- **Mysterious Name**: name doesn't reveal what it does or holds. → rename it; if no honest name comes, the design's murky.
- **Duplicated Code**: same logic shape in more than one hunk or file. → extract the shared shape, call it from both.
- **Feature Envy**: method reaches into another object's data more than its own. → move the method onto the data it envies.
- **Data Clumps**: same few fields travelling together. → bundle into one type, pass that.
- **Primitive Obsession**: primitive standing in for a domain concept. → give the concept its own small type.
- **Repeated Switches**: same switch on the same type recurs. → polymorphism, or one shared map.
- **Shotgun Surgery**: one logical change forces scattered edits. → gather what changes together.
- **Divergent Change**: one module edited for several unrelated reasons. → split per reason.
- **Speculative Generality**: abstraction for needs the spec doesn't have. → delete it until a real need shows.
- **Message Chains**: long `a.b().c().d()` navigation. → hide behind one method.
- **Middle Man**: mostly delegates onward. → cut it, call direct.
- **Refused Bequest**: subclass ignores most of what it inherits. → composition.

## Security Checklist

The description claims security; this is the checklist that makes it real. Full FAIL/PASS pairs and verification steps live in [references/security-checklist.md](references/security-checklist.md) — the reviewer reads it on every review. If the diff touches auth, data, or user input, the security checklist gates the verdict, not the smell baseline.

## Release Audit

Production readiness is a separate lens from diff correctness. Full evidence order, score caps, and output shape live in [references/release-audit.md](references/release-audit.md) — the reviewer reads it before declaring production readiness. Score caps are hard: missing auth, non-idempotent payment webhooks, unsafe migrations, exposed secrets, or no rollback path cap the score at 69; red CI or untested launch-critical path caps at 84.

## Acting on Findings

When receiving review feedback: READ the complete feedback without reacting. UNDERSTAND: restate each item in your own words. VERIFY each against codebase reality before implementing. EVALUATE: technically sound for THIS codebase? RESPOND with technical acknowledgment or reasoned pushback. IMPLEMENT one item at a time, testing each. If any item is unclear, stop and clarify all unclear items first — never partially implement a mixed set. Route fixes through `/implement` → `/converge` → `/review` (MAX 3 rounds total across nested gates, then escalate).

## Optional enhancements

- Optional (security-and-hardening, performance-optimization, coding-standards): invoke per diff — security for source→sink, performance for workload→demand, standards for quality.
- Optional (jev): when findings exceed 10, batch-triage in one `jev_score` call; `review` decisions escalate.
- Optional (no-comments): when the diff is comment-dense, route comment hygiene via no-comments.
- Optional (explorer): when the diff spans subsystems, invoke explorer (read-only) for cross-subsystem traces.

## Next

Required: fixes → `/implement` → `/converge` → `/review`, then `/finish`. MAX 3 rounds total across nested gates, then escalate. Gates stay red until required findings are addressed or rebutted with proof.

## Optional

- diagnosing-bugs (`/debug` — unknown cause only)
- cross-check (high-stakes / production output with no human review)
- ponytail-review (simplicity pass after correctness)
- e2e-testing (user-facing flows)
- no-comments (constraint-claiming / dead comments in the diff)
- explorer subagent for cross-subsystem traces
