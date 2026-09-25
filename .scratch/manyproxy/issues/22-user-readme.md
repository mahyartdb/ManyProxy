# 22: End-user README + quickstart (parallel-safe: new files only)

**What to build:** README.md + short user quickstart (install, add first config, import links, subscriptions, start/stop, per-app proxy setup, export, troubleshooting) written from the implemented behavior (tickets 01-19), with screenshots placeholders the user fills live. No src/ changes.

**Blocked by:** none.

**Owns:** none (docs only)

**Status:** done (commit 6970bdb; README claims spot-checked against code)

- [ ] Scenario: new user → Action: read README → Expected: from zero to first working local endpoint without asking questions. Verification: manual UX review. Priority: important.
- [ ] Prohibited side effect: no src/ changes; no invented features (every claim verified against code/tests).
