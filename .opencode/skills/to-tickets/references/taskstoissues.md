# Publishing tickets to a real tracker (grafted from spec-kit `taskstoissues.md`, adapted to `to-tickets`)

Default stays local files (one file per ticket). Use this variant when the project uses a real issue tracker (GitHub, Linear, …) — the tickets are the same, only the edges change shape.

## Dedupe first, then create

1. Build the set of ticket IDs you are about to publish (file numbers/titles, e.g. `01-signup-form`).
2. List existing issues (paginated; both open and closed) and match titles against the ticket IDs. Titles may be written `01 …`, `01: …`, or `[01] …`.
3. **Skip** any ticket whose ID already has an issue; report it (`01 already has an issue, skipping`). Create only the missing ones, in dependency order (blockers first).
4. One canonical title per issue: `<ID>: <description>` (e.g. `01: Signup form posts to /signup`).

## Guards

- NEVER create issues in a repository that does not match the project's remote. Verify `git config --get remote.origin.url` first.
- Use the tracker's native blocking / sub-issue relationship where it has one; otherwise set each ticket's "Blocked by" to the blocking issues.
- Do NOT close or modify any parent issue.
