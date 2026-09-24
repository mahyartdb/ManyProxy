# User stories (grafted from spec-kit `spec-template.md`, adapted to `spec`)

Stories are prioritized user journeys, each independently testable: implement just one and still have a viable slice. IDs follow this skill's kernel scheme (`US-1`, mapped to the CAPs they cover).

## Shape per story

### User Story N - [Brief Title] (Priority: P1/P2/P3)

[Plain-language journey. P1 = most critical, ships the MVP.]

**Why this priority**: [value + why this rank]

**Independent Test**: [how this story is verified on its own and what value it delivers alone]

**Acceptance Scenarios** (Given/When/Then, verifiable per the kernel's rule — no "works correctly"):

1. **Given** [initial state], **When** [action], **Then** [expected outcome]
2. …

## Rules

- Order stories by importance (P1 first). Each story is developable, testable, and demoable independently.
- Every scenario carries an independent test: the story's slice must be provable without the other stories.
- Acceptance criteria stay implementation-free (languages, frameworks, APIs leak nowhere — see Verification in `../SKILL.md`).
