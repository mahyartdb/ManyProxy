# 06: Brass-and-ink token pass plus states sweep

**What to build:** the whole surface on the new token language — lamps, plates, track lines, modules — with every state restyled and covered: default, hover, focus, active, disabled, loading, error, and empty. The status bar becomes the describer (counts, engine, health, usage), endpoints gain per-row copy, failing copy reads `never tested` with a next step, and the drop hint dims whenever drops are blocked.

**Blocked by:** 01, 02, 03, 04, 05 (needs every region restyled first so the pass unifies rather than invents).

**Owns:** impeccable

**Status:** ready-for-agent

- [ ] Scenario: full pass over toolbar, list, detail, dialogs, status bar → Action: exercise every control state → Expected: all eight states render in the new language with visible keyboard focus throughout and no orphaned old-world styling. Verification: manual UX review. Priority: required.
- [ ] Scenario: empty library, no-hits query, dead row, in-flight start → Action: visit each → Expected: each state teaches the next step in honest copy with no dead ends. Verification: manual UX review. Priority: important.
- [ ] Prohibited side effect: no secret, credential, or upstream-auth material may surface in copy, logs, search matching, or clipboard payloads.

Retry cap: 2 attempts, then escalate to the user.
