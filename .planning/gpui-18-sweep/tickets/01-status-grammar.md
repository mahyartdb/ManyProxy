# 01: Status grammar on kit primitives

**What to build:** Row serving state reads the same everywhere and announces itself — lamp + checked chip + badge agreement via kit primitives with tooltips and accessible names.

**Blocked by:** None (can start immediately).

**Owns:** gpui-kit composition (max 1-2 slices skills: gpui-kit + accessible-frontend for announce review).

**Status:** done (commit pending ledger line)

- [ ] Scenario: seeded rows mixed serving/stopped/failed → Action: render library + inspect one selected+checked row → Expected: lamp uses kit status primitive with tooltip + accessible name (`Serving`/`Stopped`/`Failed`), numeral chip uses kit chip with position text, wash (selection) and rim+chip (checked) both visible when combined, lamp/badge/detail-`Badge.dot` agree. Verification: automated test (headless `assert_painted` `row-dot-<id>` + `row-check-chip-<id>` + tooltip/aria asserts + pinned `serving_badge` strings green). Priority: required.
- [ ] Scenario: color removed (grayscale review) → Action: scan rows → Expected: state distinguishable by text/icon, not hue alone. Verification: manual UX review. Priority: important.
- [ ] Prohibited side effect: no invented kit APIs; no hardcoded colors; no pinned id/string renames without same-ticket test updates; no per-row start spinners.

Retry cap: 2, then escalate with failing assert + attempted primitive.
