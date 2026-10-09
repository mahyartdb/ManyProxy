# 01: Scannable library rows

**What to build:** Library rows a user can skim at 50-row density: name at full
emphasis, serving state readable without color alone, meta line of at most 3
facts, full route still available via tooltip and detail pane, 12px-minimum
lamp, selection wash and checked rim + numeral chip distinct when combined.

**Blocked by:** None (can start immediately).

**Owns:** rust-testing

**Status:** ready-for-agent

- [ ] Scenario: 50 seeded rows, mixed states → Action: render library →
  Expected: each row's name and serving state distinguishable without opening
  detail; meta line carries ≤3 facts. Verification: automated test. Priority: required.
- [ ] Scenario: row both selected and bulk-checked → Action: render row →
  Expected: wash plus rim + numeral chip both visible. Verification: automated test. Priority: required.
- [ ] Scenario: hover or focus any row element carrying truncated text →
  Action: hold pointer/keyboard focus → Expected: full-text tooltip.
  Verification: manual UX review. Priority: important.
- [ ] Prohibited side effect: pinned row ids (`library-row-<id>`, `row-name-<id>`,
  `row-meta-<id>`, `row-dot-<id>`, `row-check-chip-<id>`) renamed or dropped
  without same-ticket test updates; per-row start tracking introduced.

Retry cap: 2, then escalate to the user.
