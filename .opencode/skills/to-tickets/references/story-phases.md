# Story phases + parallel markers (grafted from spec-kit `tasks-template.md`, adapted to `to-tickets`)

Ticket files stay this skill's shape (one file per ticket, blocking edges); this reference adds phase order, the `[P]` marker, and delivery strategy.

## Phase order

1. **Setup** (shared infrastructure): project structure, lint/format. No dependencies — starts immediately.
2. **Foundational** (blocking prerequisites): schema, auth framework, routing, base models, error/logging, env config. CRITICAL: no story work starts until this phase is complete. Ship it as blocker tickets with the same exit criteria — a phase, not a horizontal layer.
3. **User stories** (P1 → P2 → P3): each story independently implementable, testable, deliverable. Checkpoint per story: the story works on its own.
4. **Polish** (cross-cutting): docs, cleanup, perf, hardening, validation-guide run.

## `[P]` marker

`[P]` = can run in parallel (different files, no dependencies). Mark it on the ticket's scenarios where true. Models before services, services before endpoints, tests fail before implementation.

## Delivery strategy

- **MVP first**: Setup + Foundational + P1 story, then STOP and validate independently. Deploy/demo if ready.
- **Incremental**: add P2, P3 each tested independently without breaking earlier stories.
- **Parallel team**: after Foundational, one developer per story.
- Commit after each ticket or logical group. Vague tickets, same-file conflicts, and cross-story dependencies that break independence are defects in the breakdown — re-split.
