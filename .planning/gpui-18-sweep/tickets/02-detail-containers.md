# 02: Detail containers share one grammar

**What to build:** Every inspector module (Endpoints, Usage, Health, History, Preset) renders in the same `GroupBox` + `DescriptionList` rhythm with truncating values + tooltips and bind:port-only copies.

**Blocked by:** 01-status-grammar (reuses lamp/badge agreement + chip language).

**Owns:** gpui-kit composition.

**Status:** ready-for-agent

- [ ] Scenario: row selected on each of Details/Health/Presets → Action: read inspector → Expected: all 5 modules boxed with consistent gaps, label/value rows via `DescriptionList` where they read better, endpoint values truncate with full-text tooltip, per-row + Copy-URLs copy bind:port only with honest toast, upstream display-only, preset pane names rule count with inline detach. Verification: automated test (headless ids `config-endpoints/health/usage/history` + `endpoint-value-*` + `copy-upstream/socks/http` paint; clipboard-content test bind:port-only). Priority: required.
- [ ] Scenario: narrow 880px → Action: view detail header + Endpoints → Expected: header wraps ≤2 lines, Endpoints above operate row, no h-scroll. Verification: manual UX review + containment test if harness supports. Priority: important.
- [ ] Prohibited side effect: no secret material in copy/tooltip/logs; no second radius/palette; no module-order churn beyond frozen Header→Endpoints→tab-specific.

Retry cap: 2, then escalate.
