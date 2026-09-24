---
name: spec
description: Turn settled intent into a buildable spec: kernel, seams, boundaries, verification. Use after intent is settled, before tickets or code. Synthesizes what is known — the command resolves clarifications, the artifact itself never interviews.
---

# Spec

The spec is the source of truth. Code is its expression. This skill takes the current conversation context and codebase understanding and produces a spec. Do NOT interview the user; just synthesize what you already know. Planning request authorizes planning only — never start implementation in the same response. No domain skill list lives here — domain routing lives in implement, plan, and code-review.

## Kernel

1. **Why** — the problem the user is facing, from the user's perspective.
2. **Capabilities** — what the system must do, each with intent + success. Stable IDs (`CAP-1`), never reused. User stories in actor/feature/benefit form: "As an <actor>, I want a <feature>, so that <benefit>". Story shape (priorities, independent tests, Given/When/Then): `references/user-stories.md`. Long, numbered, extensive. Stories carry IDs (`US-1`) mapped to the CAPs they cover. Acceptance criteria must be verifiable, not vague. "Works correctly" is bad. "Button shows confirmation dialog before deleting" is good. For any story with UI changes: always include browser verification as acceptance criteria. Write for junior developers or AI agents: be explicit and unambiguous, avoid jargon or explain it, number requirements, use concrete examples.
3. **Constraints** — technical and organizational limits.
4. **Non-goals** — explicit, at least one. What this is not.
5. **Success signal** — measurable, tech-agnostic outcomes. "Users can complete checkout in under 3 minutes" — never "API response time is under 200ms", never framework-specific. Translate vague requirements into thresholds: "faster dashboard" → "LCP < 2.5s on 4G".

## Seams

Sketch out the seams at which you're going to test the feature. Existing seams should be preferred to new ones. Use the highest seam possible. If new seams are needed, propose them at the highest point you can. The fewer seams across the codebase, the better — the ideal number is one.

## Boundaries

- **Always** — non-negotiable rules for this work.
- **Ask first** — needs explicit approval.
- **Never** — out of scope, no exceptions.

## Assumptions

Surface every guess before writing capabilities:

```
ASSUMPTIONS I'M MAKING:
1. <guess> (not <alternative>)
...
→ Correct me now or I'll proceed with these.
```

## Verification

- No implementation details (languages, frameworks, APIs leak nowhere).
- Every requirement testable; every criterion measurable without implementation knowledge.
- No `[NEEDS CLARIFICATION]` markers remain (max 3 ever existed; rest were guessed + documented).
- Two-pass self-check: coherence (sections agree), then preservation (every source claim landed somewhere).
- For behavior quality unit tests cannot capture, grade with: 1. Code grader (deterministic assertions) 2. Rule grader (regex/schema constraints) 3. Model grader (LLM-as-judge rubric) 4. Human grader (manual adjudication for ambiguous outputs).
- Capability evals: pass@3 >= 0.90. Regression evals: pass^3 = 1.00 for release-critical paths.
- Eval anti-patterns: overfitting prompts to known eval examples; measuring only happy-path outputs; ignoring cost and latency drift while chasing pass rates; allowing flaky graders in release gates.

## Companions

Load-bearing content that bloats the kernel (diagrams, glossaries, entity tables) lives in sibling files. The kernel cites them; they hold the detail. Downstream must read them.

## Sources

Every source claim used (research briefs, prior specs, tickets, user statements) is captured with a pointer. Preservation check: walk sources claim by claim — each lands in the kernel or a companion, none silently dropped.

## Maintain

After the build, the spec's documents take four roles. Map existing sources to the roles; reuse and link them in place.

| Role             | One job                                                                                  | Must not become                                      |
| ---------------- | ---------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| **Constitution** | Rules agents and contributors must obey, plus links to canonical detail                  | Live status, long explanations, or duplicated policy |
| **Map**          | What exists, where it lives, ownership, and where to look next                           | Health dashboard or event ledger                     |
| **Status**       | Current health, blockers, thresholds, and intentional-removal delete-zone                | Structural reference or historical narrative         |
| **History**      | Durable governance decisions, intentional removals, replacements, and material incidents | A duplicate of every commit, fix, or Git history     |

The discipline is **one canonical owner per fact**. Other files link to that owner rather than copying it.

Keep deleted paths in the delete-zone until recreation is no longer a realistic risk. Update only the role affected: structure changes update the map; threshold, blocker, or removal changes update status; hard-to-reverse decisions get a history entry or ADR.

When folding a delta back into the main spec, apply changes intelligently:

- **ADDED Requirements:** If the requirement doesn't exist in the main spec, add it. If it already exists, update it to match (treat as implicit MODIFIED).
- **MODIFIED Requirements:** Find the requirement and apply the changes — new scenarios, modified scenarios, changed descriptions. Preserve scenarios and content not mentioned in the delta.
- **REMOVED Requirements:** Remove the entire requirement block.

Write the plan to a file before compacting — a file persists on every version and every model. Treat the task list as a convenience that may be missing, never as your durable record.

## Default constitution

Unless the project ratified its own: spec is truth, everything testable, smallest scope that solves it, out-of-scope written down.

## Governance (amendment procedure)

Enduring principles live in the project's constitution file (or the Constitution role above when no separate file exists). Template for first ratification: `references/constitution-template.md`. Amending them follows this procedure — feature work never edits governance in passing.

**Scope guard**: amendment runs touch governance only. Feature implementation, code generation, refactoring, or deployment requests found alongside are extracted as deferred intents, never executed here. Unclear cases get clarified before changes.

**Procedure**:

1. Load the current constitution (or start from the default above for a first ratification). Preserve what still applies.
2. Collect values: conversation input first, repo context (README, docs, prior versions) otherwise. Governance dates: ratification = original adoption (ask or TODO if unknown); last-amended = today when changed.
3. Version by semver: MAJOR = incompatible removals/redefinitions; MINOR = new principle or materially expanded guidance; PATCH = clarifications, wording, typos. Ambiguous bumps get reasoning before finalizing.
4. Draft: every principle gets a succinct name, non-negotiable rules, and rationale where not obvious. Governance section lists this amendment procedure, the versioning policy, and compliance review expectations. Principles stay declarative and testable (vague "should" becomes MUST/SHOULD with rationale).
5. Sync Impact Report (scratch for human review, removed before commit): version old → new, modified/added/removed principles, deferred TODOs.
6. Validate: no unexplained bracket tokens, version line matches report, ISO dates, TODOs named (`TODO(<FIELD>): explanation`).
7. Summarize: new version + rationale, TODOs, suggested commit message (`docs: amend constitution to vX.Y.Z (…)`), and Next Actions for deferred intents (e.g. `spec` for a feature request).

## Next

- Required: `/plan` → `/to-tickets` → `/decide` → `/implement`.
- Optional: `/checklist` (requirements-quality gate before tickets); `/research` + `ask-me` (reshaped intent loops back through `/refine` / `/decide`, never straight to build); `/council` (ambiguous tradeoff); `prototype` skill (feasibility spike, decision only).

## Optional enhancements

- Optional (technical-writing): when capabilities or stories read vague or inconsistent, invoke technical-writing + unslop to tighten them before tickets.
- Optional (archify, baoyu-diagram): when Companions need a topology or flow diagram, invoke archify (showcase HTML) or baoyu-diagram (quick SVG).
- Optional (explorer): when grounding claims in existing code, invoke explorer (read-only) to verify.
