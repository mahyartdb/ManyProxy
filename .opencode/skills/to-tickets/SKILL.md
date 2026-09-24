---
name: to-tickets
description: Break a plan, spec, or the current conversation into a set of tracer-bullet tickets, each declaring its blocking edges, published to the configured tracker (edges as text in one file per ticket locally, or native blocking links on a real tracker). Use when a spec or plan is approved and buildable tickets come next, before implement. Do NOT use for design artifacts (use spec/plan) or code (use implement).
---

# To Tickets

Break a plan, spec, or conversation into a set of **tickets**: tracer-bullet vertical slices, each declaring the tickets that **block** it.

Local files by default (one file per ticket). If the project uses a real issue tracker, publish there instead — the tickets are the same either way, only the shape of the blocking edges changes.

## Process

### 1. Gather context

Work from whatever is already in the conversation context. If the user passes a reference (a spec path, an issue number or URL) as an argument, fetch it and read its full body and comments.

### 2. Explore the codebase (optional)

Optional (explorer): when codebase grounding is needed, invoke @explorer read-only. If you have not already explored the codebase, do so to understand the current state of the code. Ticket titles and descriptions should use the project's domain glossary vocabulary, and respect ADRs in the area you're touching.

Look for opportunities to prefactor the code to make the implementation easier. "Make the change easy, then make the easy change."

### 3. Draft vertical slices

Break the work into **tracer bullet** tickets.

<vertical-slice-rules>

- Each slice cuts a narrow but COMPLETE path through every layer (schema, API, UI, tests): vertical, NOT a horizontal slice of one layer
- A completed slice is demoable or verifiable on its own
- Each slice is sized to fit in a single fresh context window
- Any prefactoring should be done first

</vertical-slice-rules>

Give each ticket its **blocking edges**: the other tickets that must complete before it can start. A ticket with no blockers can start immediately. Phase order, `[P]` parallel markers, MVP-first delivery: `references/story-phases.md`.

Each ticket's exit condition must be judgable yes/no by a machine. "All tests pass and a change-list is produced" converges; "make it good" either never passes or passes at random. Define boundary conditions alongside the done-criterion ("what it must NOT do") — missing boundaries are a license to cheat. Set a retry cap with a failure fallback: retry cap N, then escalate to the user when exceeded.

**Wide refactors are the exception to vertical slicing.** A **wide refactor** is one mechanical change (rename a column, retype a shared symbol) whose **blast radius** fans across the whole codebase, so a single edit breaks thousands of call sites at once and no vertical slice can land green. Don't force it into a tracer bullet; sequence it as **expand–contract**. First expand: add the new form beside the old so nothing breaks. Then migrate the call sites over in batches sized by blast radius (per package, per directory), each batch its own ticket blocked by the expand, keeping CI green batch to batch because the old form still exists. Finally contract: delete the old form once no caller remains, in a ticket blocked by every migrate batch. When even the batches can't stay green alone, keep the sequence but let them share an integration branch that all block a final integrate-and-verify ticket; green is promised only there.

### 4. Quiz the user

Present the proposed breakdown as a numbered list. For each ticket, show:

- **Title**: short descriptive name
- **Blocked by**: which other tickets (if any) must complete first
- **What it delivers**: the end-to-end behaviour this ticket makes work

Ask the user:

- Does the granularity feel right? (too coarse / too fine)
- Are the blocking edges correct: does each ticket only depend on tickets that genuinely gate it?
- Should any tickets be merged or split further?

Ceremony scales to blast radius. Score the request on three signals, take the **highest** tier any signal reaches, and state the result in one line so the user can override:

| Tier     | Files touched        | New dependency / contract                   | Design ambiguity             |
| -------- | -------------------- | ------------------------------------------- | ---------------------------- |
| trivial  | 1, a few lines       | none                                        | none — the change is obvious |
| small    | 1 file / 1 function  | none                                        | clear once you read the code |
| standard | 2–5 files            | maybe a new internal module                 | one real choice to make      |
| large    | many / cross-cutting | new external dep, public API, or a spec doc | multiple open questions      |

The tie-breaker: anything touching a security trigger or a public API / contract is **at least** standard, regardless of file count.

Iterate until the user approves the breakdown.

Priority scoring (Jev): when the `jev` skill is available, score all tickets in one `jev_score` call (levels optional / important / required, each with concrete criteria) and fill each scenario's Priority from the result. `review` decisions go to the user alongside the breakdown for approval. Without Jev, assign Priority by judgment.

### 5. Publish the tickets

Publish the approved tickets. **How** depends on the tracker; the tickets are the same either way, only the shape of the blocking edges changes:

- **Local files** → write one file per ticket under `.scratch/<feature-slug>/issues/<NN>-<slug>.md`, numbered from `01` in dependency order (blockers first). Each file's "Blocked by" lists the numbers/titles it depends on. Use the per-ticket file template below: one ticket per file, never a single combined file.
- **A real issue tracker (GitHub, Linear, …)** → publish one issue per ticket in dependency order (blockers first) so each ticket's blocking edges can reference real identifiers. Use the platform's native blocking / sub-issue relationship where it has one; otherwise set each ticket's "Blocked by" to the blocking issues. The tickets are agent-grabbable by construction. Dedupe-first variant: `references/taskstoissues.md`.

Work the **frontier**: any ticket whose blockers are all done. For a purely linear chain that means top to bottom.

Optional (technical-writing): before publishing tickets, invoke technical-writing + unslop so titles and scenarios read unambiguous.

Do NOT close or modify any parent issue.

Next: `/decide` (post-tickets kill-gate), then `/implement`, then `/converge`.

<local-ticket-template>

# <NN>: <Ticket title>

**What to build:** the end-to-end behaviour this ticket makes work, from the user's perspective, not a layer-by-layer implementation list.

**Blocked by:** the numbers/titles of the tickets that gate this one, or "None (can start immediately)".

**Owns:** <owning domain skill(s), e.g. api-and-interface-design, database-migrations> (max 1-2 per ticket).

**Status:** ready-for-agent

- [ ] Scenario: <starting condition> → Action: <trigger> → Expected: <observable behavior>. Verification: <automated test | integration check | manual UX review | accessibility check | security review | operational check | stakeholder acceptance — pick only skill-mapped values that match Owns>. Priority: <required | important | optional>.
- [ ] Prohibited side effect (when meaningful): <what must not happen>

Do not use words such as "correctly", "securely", "fast", "intuitive", or "robust" without defining observable evidence or recording them as a human-review judgment.

</local-ticket-template>

<issue-template>

## Parent

A reference to the parent issue on the tracker (if the source was an existing issue, otherwise omit this section).

## What to build

The end-to-end behaviour this ticket makes work, from the user's perspective, not layer-by-layer implementation.

## Acceptance criteria

- [ ] Scenario: <starting condition> → Action: <trigger> → Expected: <observable behavior>. Verification: <automated test | integration check | manual UX review | accessibility check | security review | operational check | stakeholder acceptance — pick only skill-mapped values that match Owns>. Priority: <required | important | optional>.
- [ ] Prohibited side effect (when meaningful): <what must not happen>

**Owns:** <owning domain skill(s)> (max 1-2 per ticket).

Do not use words such as "correctly", "securely", "fast", "intuitive", or "robust" without defining observable evidence or recording them as a human-review judgment.

## Blocked by

- A reference to each blocking ticket, or "None (can start immediately)".

</issue-template>

In either form, avoid specific file paths or code snippets: they go stale fast. Exception: if a prototype produced a snippet that encodes a decision more precisely than prose can (state machine, reducer, schema, type shape), inline it and note briefly that it came from a prototype. Trim to the decision-rich parts, not a working demo, just the important bits.
