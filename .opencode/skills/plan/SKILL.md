---
name: plan
description: Execute implementation planning for a feature with spec.md present. Use when spec is settled and design artifacts are next (Technical Context, Constitution Check, research.md, data-model.md, contracts/, quickstart.md). Runs after spec, before to-tickets. Do not use without a spec.
---

# Plan

Feature design from settled spec. Ends with design artifacts, no tasks, no code.

## Setup

Resolve FEATURE_DIR (input or current directory), SPEC (`spec.md` inside it), artifact paths (`plan.md`, `research.md`, `data-model.md`, `contracts/`, `quickstart.md` under FEATURE_DIR). Load the spec and the `spec` skill default constitution. Empty input with no discoverable spec is an error.

## Workflow

1. Fill Technical Context (mark unknowns as "NEEDS CLARIFICATION"). Shape: `references/technical-context.md`.
2. Fill Constitution Check from constitution. Evaluate gates (ERROR if violations unjustified).
3. Phase 0: generate research.md, resolve all NEEDS CLARIFICATION.
4. Phase 1: generate data-model.md, contracts/, quickstart.md.
5. Re-evaluate Constitution Check post-design.

## Phase 0: Outline & Research

1. Extract unknowns: each NEEDS CLARIFICATION to a research task, each dependency to a best-practices task, each integration to a patterns task. Map each dependency to its owning domain skill:
   | Dependency kind            | Owning skill reference                                        |
   | -------------------------- | ------------------------------------------------------------- |
   | API / contract             | api-and-interface-design                                      |
   | Schema / Postgres / SQLite | database-migrations, supabase-postgres-best-practices, sqlite |
   | Deploy / containers        | deployment-patterns, docker-patterns                          |
   | Auth / input / PII         | security-and-hardening                                        |
   | Perf budget                | performance-optimization                                      |
   | Edge / workers             | workers-best-practices, wrangler, durable-objects             |
2. Dispatch research (subagents where available, else inline).
3. Consolidate in `research.md`: Decision, Rationale, Alternatives considered. Record the owning skill name + version for each decision.

Output: research.md with all NEEDS CLARIFICATION resolved.

## Phase 1: Design & Contracts

Prerequisite: `research.md` complete.

1. Entities from spec to `data-model.md`: name, fields, relationships, validation rules, state transitions.
2. Interface contracts to `contracts/` (public APIs, command schemas, endpoints, grammars, UI contracts). Skip if purely internal.
3. Validation guide to `quickstart.md`: runnable end-to-end scenarios, prerequisites, setup/run commands, expected outcomes. No implementation bodies, migrations, or full test suites.

Output: data-model.md, contracts/*, quickstart.md.

## Rules

- Absolute paths for filesystem ops; project-relative paths in docs.
- ERROR on gate failures or unresolved clarifications.

## Done when

- [ ] Plan workflow executed and design artifacts generated.
- [ ] Completion reported with plan path and artifacts.

## Optional enhancements

- Optional (architecture-designer): when system shape is still open, invoke for ADRs and component boundaries.
- Optional (api-and-interface-design): when contracts/ needs stable naming, versioning, or error shapes, invoke.
- Optional (explorer): run an explorer Phase 0 pass (read-only) to ground research.md in real code.
- Optional (technical-writing): when quickstart.md or research.md must be publishable, invoke technical-writing + unslop.

## Next

- → `/checklist` (optional requirements-quality gate) → `/to-tickets` → `/decide` → `/implement`.
- Phase 0 unknowns → `/research` door (resolve, then return here).
- Ambiguous tradeoff → `/council` (recommendation only, then back).
- Constitution ERROR → back to `/spec` (fix the spec; never push a violating design forward).
