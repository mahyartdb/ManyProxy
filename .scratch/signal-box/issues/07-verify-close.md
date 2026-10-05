# 07: Snapshot-close verification

**What to build:** proof the world landed: goldens recaptured from a live run, the critique re-run showing the P0/P1s cleared with an improved score, and DESIGN.md plus the design sidecar rewritten from the built world (never from the brief). The critique snapshot closes only when its priority issues are actually gone.

**Blocked by:** 06 (Brass-and-ink token pass — verification runs over the finished surface).

**Owns:** impeccable

**Status:** ready-for-agent

- [ ] Scenario: all build tickets done → Action: recapture goldens and re-run critique on the same target → Expected: P0 and P1 findings cleared, score above 26/40, new goldens committed. Verification: manual UX review. Priority: required.
- [ ] Scenario: world confirmed on screen → Action: regenerate DESIGN.md and the sidecar from the built tokens and components → Expected: token-bearing DESIGN.md plus sidecar match the implementation; no prose-only claims. Verification: stakeholder acceptance. Priority: required.
- [ ] Prohibited side effect: the old snapshot must not be closed while priority issues remain; trend history must be preserved.

Retry cap: 2 attempts, then escalate to the user.
