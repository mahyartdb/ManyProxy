# 03: Forms on Form/Field with grouped choices

**What to build:** Add/Edit/Import/Subscriptions/Groups bodies use kit `v_form`/`h_form` + `field()` + footer; Add stays guided-first with `Collapsible` Advanced JSON; interval + log-level pickers are single `RadioGroup`s; Settings uses `Settings` panel sections.

**Blocked by:** 02-detail-containers (shares container/footer language).

**Owns:** gpui-kit composition.

**Status:** ready-for-agent

- [ ] Scenario: open Add → Action: read form → Expected: guided Server/Server-port/Identity (mask+reveal, live hint e.g. `vless needs uuid`) first, Advanced collapsed; expanding merges guided→JSON, collapsing echoes JSON→blank guided only; Port shows live budget (`port-budget`); non-loopback shows loopback-exposure hint; validation keeps open with inline error + focus-first. Verification: automated test (dialog field ids + `port-budget` + footer `Add`/`Cancel` verbs + validation-keeps-open test). Priority: required.
- [ ] Scenario: open Subscriptions + Logs level → Action: read pickers → Expected: exactly one `RadioGroup` each (intervals always/1h/6h/12h/24h/7d; levels All/Debug/Info/Warn/Error) with current marked, no redundant summary, no duplicate cycle button. Verification: automated test (grouped exclusive choice interaction: select → state changes → persists). Priority: required.
- [ ] Scenario: open Settings → Action: read sections → Expected: Startup/Probe/Paths/Maintenance via `Settings`/`SettingPage`/`SettingGroup`, destructive buttons danger-styled with confirms. Verification: manual UX review + headless section ids. Priority: important.
- [ ] Prohibited side effect: no bare `OK` footers; no `Field::child` invented method (use `ParentElement::child`); no engine/store logic changes.

Retry cap: 2, then escalate.
