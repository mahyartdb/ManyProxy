---
name: converge
description: Assess the codebase against a feature's spec, plan, and tasks, then append remaining work as traceable tasks so implement can complete it. Use after implement runs, when verifying whether specified work is actually built, or when the user says converge, drift check, or what remains. Never rewrites specs, plans, or existing tasks; never touches application code.
---

# Converge

Close the gap between what a feature's specification, plan, and tasks call for and what the codebase implements. Read `spec.md`, `plan.md`, and `tasks.md` as the **sole source of intent**, assess the current code, and append each piece of remaining work as a new, traceable task. This is not a diff tool and does not track changes: no git, no branch comparison, no history.

## Inputs

A feature directory containing `spec.md`, `plan.md`, and `tasks.md` (or explicit paths to the three). If any is missing, STOP and name the prerequisite — `spec` skill for a missing spec, `/plan` for a missing plan, `to-tickets` for missing tasks. Do not produce partial output.

## Operating Constraints

**APPEND-ONLY, NEVER REWRITE**: the only write is appending a single new `## Phase N: Convergence` section to `tasks.md`. It MUST NOT:

- modify `spec.md` or `plan.md` in any way;
- rewrite, renumber, reorder, or delete any existing task (including prior Convergence phases);
- modify, create, or delete any application code — completing appended tasks belongs to `implement`.

When the codebase satisfies everything, leave `tasks.md` **byte-for-byte unchanged** (no empty header) and report converged.

**Constitution authority**: code violating a MUST principle from the `spec` skill's default constitution is the highest-severity finding and produces a remediation task. An unfilled constitution template skips these checks gracefully.

## Load (Progressive Disclosure)

Minimal context only:

- **spec.md:** Functional Requirements, Success Criteria (buildable work only — exclude post-launch metrics), User Stories + Acceptance Scenarios, Edge Cases.
- **plan.md:** architecture/stack decisions, data model references, phases and named touch-points (files/components to be created or edited), technical constraints.
- **tasks.md:** task IDs (next ID + phase number), descriptions, phase grouping, referenced file paths.

## Intent Inventory

Internal model, never echoed raw:

- **Requirements inventory**: one stable key per requirement / acceptance scenario (e.g. `US1/AC2`), plus plan decisions and constitution principles imposing buildable obligations.
- **Code-scope map**: file paths named in plan/tasks plus keyword search for described concepts. Bound the assessment to these — never infer scope beyond the artifacts. Optional (explorer): when scope is unclear, invoke explorer (read-only) to build the map.

## Findings

Include every existing task regardless of checkbox state: completion claims are not evidence. Verify behavior against spec, plan, tasks, constitution; for corrective chains, assess resulting behavior, not superseded details. Surface unmet obligations and implementation that contradicts, exceeds, or falls outside intent.

Per gap record: stable id, `source-ref`, gap type, severity, one-line description with evidence (file/area observed).

Not this skill: diff correctness → `code-review`; high-stakes adversarial verification → `cross-check`; requirements quality → `checklist`; perf/security implementation audits → `performance-optimization` / `security-and-hardening`.

## Route the gap (load at most 1-2 owning skills)

| Gap signal                         | Owning skill(s)                                       |
| ---------------------------------- | ----------------------------------------------------- |
| `*.vue`, Pinia stores, Nuxt routes | vue, pinia, nuxt                                      |
| `*.rs`                             | rust-mastery + matching rust-\*                       |
| `migrations/*`, SQL, Supabase      | database-migrations, supabase-postgres-best-practices |
| `Dockerfile`, compose, image       | docker-patterns                                       |
| auth, input, PII, secrets          | security-and-hardening                                |
| UI / a11y markup                   | accessible-frontend                                   |

- **`missing`**: required work absent from the code entirely.
- **`partial`**: work exists but does not fully satisfy the requirement.
- **`contradicts`**: code conflicts with stated intent or a constitution MUST.
- **`unrequested`**: code not called for by spec/plan/tasks (awareness only — never delete; append a task to review/justify or remove it).

Edge cases: little or no code yet → entire scope is `missing`, not failure. Nothing remains → zero findings, converged branch.

## Severity

- **CRITICAL**: constitution MUST violation, or `missing`/`contradicts` blocking a P1 story.
- **HIGH**: `missing`/`partial` on a core requirement or acceptance criterion.
- **MEDIUM**: `partial` on a secondary requirement, or unclear `unrequested` addition.
- **LOW**: minor partials, polish, low-risk `unrequested` additions.

When more than 5 findings exist and the `jev` skill is available, batch severity in one `jev_score` call; `review` decisions escalate. Without Jev, grade by judgment.

## Summary Before Writing

Output a severity-graded table first — no file writes yet:

| ID  | Gap Type | Severity | Source | Evidence | Remaining Work |
| --- | -------- | -------- | ------ | -------- | -------------- |

Plus metrics: requirements checked, plan decisions checked, constitution principles checked, findings by gap type and severity.

## Append Contract

Findings exist: `M` = max existing task ID, `N` = highest phase + 1. Append one `## Phase N: Convergence` section, one checklist item per finding, CRITICAL/HIGH first, zero-padded IDs `T{M+1:03d}…`:

```markdown
- [ ] T042 <imperative description> per <source-ref> (<gap-type>)
```

`<source-ref>` traces origin (`FR-003`, `US1/AC2`, `plan: storage decision`, `Constitution II`). Never reuse or renumber IDs; prior Convergence phases stay untouched below the new one.

No findings: touch nothing. Report **"Converged — the implementation satisfies the spec, plan, and tasks."** with check counts.

Optional (technical-writing): before appending tasks, invoke technical-writing + unslop so appended items read unambiguous.

## Next Actions

MAX 3 `implement`/`converge` rounds total, then escalate to the user (nested gates share the count).

- `tasks_appended`: count + phase; recommend `/implement` on them (a follow-up converge should find fewer or none).
- `converged`: recommend `/review`, then `/finish` (merge/PR menu).

## Optional

- `/cross-check` after `/implement`, before `/review` — trigger-gated (high-stakes / production output with no human review).
