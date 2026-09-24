# Technical Context + Constitution Check (grafted from spec-kit `plan-template.md`, adapted to `plan`)

Fill this before Phase 0 research. Mark every unknown `NEEDS CLARIFICATION` — never guess here; Phase 0 resolves them.

## Technical Context

- **Language/Version**: [e.g. Python 3.11, Rust 1.75, or NEEDS CLARIFICATION]
- **Primary Dependencies**: [e.g. FastAPI, or NEEDS CLARIFICATION]
- **Storage**: [e.g. PostgreSQL, files, or N/A]
- **Testing**: [e.g. pytest, cargo test, or NEEDS CLARIFICATION]
- **Target Platform**: [e.g. Linux server, WASM, or NEEDS CLARIFICATION]
- **Project Type**: [library / cli / web-service / mobile-app / compiler / desktop-app, or NEEDS CLARIFICATION]
- **Performance Goals**: [domain-specific, e.g. 1000 req/s, 60 fps]
- **Constraints**: [domain-specific, e.g. <200ms p95, offline-capable]
- **Scale/Scope**: [e.g. 10k users, 50 screens]

## Constitution Check (gate)

GATE: must pass before Phase 0 research. Re-check after Phase 1 design.

Evaluate the plan against the project constitution (`../../spec/references/constitution-template.md` for first ratification; `spec` skill Governance procedure for amendments). Unjustified violations are an ERROR — stop, do not proceed to research or design.
