---
name: checklist
description: Generate reviewer-owned requirements-quality checklists for a feature domain. Use when spec or plan needs unit tests for English (completeness, clarity, consistency, measurability) before tickets. Never verifies implementation; never marks items [x].
---

# Checklist

Unit tests for requirements writing. Tests whether requirements are well-written, complete, unambiguous, ready for implementation. Never tests code or behavior.

## Prohibited vs required

PROHIBITED (implementation test): items starting Verify/Test/Confirm/Check plus behavior; Click/navigate/render/load/execute; "displays correctly", "works properly"; test cases, QA procedures.

REQUIRED patterns: "Are [requirements] defined for [scenario]?" / "Is [vague term] quantified?" / "Are requirements consistent between [A] and [B]?" / "Can [requirement] be objectively measured?" / "Are [edge cases] addressed?" / "Does the spec define [missing aspect]?"

Dimensions per item: Completeness, Clarity, Consistency, Measurability, Coverage (Primary, Alternate, Exception/Error, Recovery, Non-Functional).

## Steps

1. Setup: resolve FEATURE_DIR, read `spec.md` (+ `plan.md`, `tasks.md` if present). Load `spec` default constitution. All paths absolute.
2. Clarify intent: up to THREE contextual questions from phrasing plus spec/plan signals. Format options as compact table A-E max. At most TWO follow-ups (Q4/Q5). Defaults when non-interactive: depth Standard, audience Reviewer (PR) if code-related else Author, focus top-2 clusters.
3. Understand request: combine input plus answers into theme, must-haves, category scaffolding. Infer from spec/plan/tasks, never hallucinate.
4. Load feature context progressively: only focus-area portions; summarize long sections to bullets; never dump full files.
5. Generate `FEATURE_DIR/checklists/[domain].md` (`ux.md`, `api.md`...): new file numbers from CHK001; existing file appends continuing last CHK ID; never delete. Leave every item `[ ]`. Item shape: question plus dimension tag plus traceability ref (`[Spec §X.Y]` or `[Gap]`, `[Ambiguity]`, `[Conflict]`, `[Assumption]`). Traceability on >=80% of items. Cap ~40 raw candidates by risk; merge duplicates; fold 5+ low-impact edges into one. Include rollback where state mutates.
6. Structure: H1 title, purpose/created meta, ownership note (`[x]` = reviewer approval of requirements quality), `##` sections with `- [ ] CHK###` lines, notes. Shape template: `references/checklist-shape.md`. `implement` reads state but never modifies markers.
7. Report: full path, item count, created vs appended, focus areas, depth, actor/timing, must-haves. Multiple domains coexist; clean up obsolete checklists when done. Done when the file exists, counts are reported, and every item carries a dimension tag plus traceability ref.

## Ownership

- `[x]` means reviewer judged the criterion satisfied, never implementation completion.
- Never mark generated items `[x]`. Agent assists evaluation only on explicit reviewer request.

## Optional enhancements

- Optional (technical-writing): run technical-writing + unslop on every pass so items read unambiguous.
- Optional (jev): when items exceed 5, batch severity in one `jev_score` call; `review` decisions escalate.

## Domain → checklist map

| Checklist domain    | Owning skill(s)                                       |
| ------------------- | ----------------------------------------------------- |
| api                 | api-and-interface-design                              |
| schema / migrations | database-migrations, supabase-postgres-best-practices |
| auth / input / PII  | security-and-hardening                                |
| budgets / perf      | performance-optimization                              |
| UI / a11y           | accessible-frontend, web-design-guidelines            |

## Next

- Reviewer approval → `/to-tickets` → `/decide` → `/implement`.
- Items tagged `[Gap]` / `[Ambiguity]` / `[Conflict]` → `/spec` or `/plan` (fix the artifact), then regenerate the affected checklist.
