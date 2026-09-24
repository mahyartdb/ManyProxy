# 10 (parked, not v1): Routing-profile presets read-only

**What to build:** View + import remote routing presets (preset repo, `throne://route`/`remoteroute` envelope, raw JSON view) without a rule editor. Explicitly out of v1; unblocks later work only.

**Blocked by:** 05 (Standard multi-engine).

**Owns:** api-and-interface-design

**Status:** parked

- [ ] Scenario: preset URL → Action: import → Expected: preset listed read-only with name + rules visible. Verification: automated test. Priority: optional.
- [ ] Prohibited side effect: no rule editing, no auto-application to endpoints in v1.
