# 03: Forms on Form/Field with grouped choices

**What to build:** Add/Edit/Import/Subscriptions/Groups bodies use kit `v_form`/`h_form` + `field()` + footer; Add stays guided-first with `Collapsible` Advanced JSON; interval + log-level pickers are single `RadioGroup`s; Settings uses `Settings` panel sections.

**Blocked by:** 02-detail-containers (shares container/footer language).

**Owns:** gpui-kit composition.

**Status:** done

- [x] Scenario: open Add → Action: read form → Expected: guided Server/Server-port/Identity (mask+reveal, live hint e.g. `vless needs uuid`) first, Advanced collapsed; expanding merges guided→JSON, collapsing echoes JSON→blank guided only; Port shows live budget (`port-budget`); non-loopback shows loopback-exposure hint; validation keeps open with inline error + focus-first. Verification: automated test (dialog field ids + `port-budget` + footer `Add`/`Cancel` verbs + validation-keeps-open test). Priority: required. — Done: body on kit `v_form` + `field()` (`dialog_field` returns kit `Field`, pinned ids ride inner divs); Advanced JSON in controlled `Collapsible` (swap never stacks); bind/port side-by-side + merged guided block keep `t11_add_dialog_paints_inside_minimum_frame` green. Existing tests (`t05_guided*`, `t06_add_dialog_paints_live_port_budget`, `honest_footer*`, `t11*`) all green, no test changes needed.
- [x] Scenario: open Subscriptions + Logs level → Action: read pickers → Expected: exactly one `RadioGroup` each (intervals always/1h/6h/12h/24h/7d; levels All/Debug/Info/Warn/Error) with current marked, no redundant summary, no duplicate cycle button. Verification: automated test (grouped exclusive choice interaction: select → state changes → persists). Priority: required. — Done: `interval_picker_row` and the Logs level row are single `RadioGroup`s (`selected_index` + `on_change`, repaint via owning view). Same-commit test updates with rationale: per-option string ids retired (kit re-keys children by index — see Ruling), clicks scope within the observed group wrappers by index (`t08_interval_picker_one_grouped_exclusive_choice`, `laneb_logs_single_level_control` green).
- [x] Scenario: open Settings → Action: read sections → Expected: Startup/Probe/Paths/Maintenance via `Settings`/`SettingPage`/`SettingGroup`, destructive buttons danger-styled with confirms. Verification: manual UX review + headless section ids. Priority: important. — DEVIATION (see ledger Ruling T03-S1): sections stay labeled grouped divs with all ids/controls/danger-confirms byte-identical; the `Settings` shell was attempted and reverted on four verified grounds (virtualized single-page culling, sidebar width tax, `&mut App`-only field callbacks, rustc stack overflow on the required closure nesting). Existing settings tests green with zero changes.
- [x] Prohibited side effect: no bare `OK` footers; no `Field::child` invented method (use `ParentElement::child`); no engine/store logic changes. — Held: footers untouched (`dialog_footer` + `footer_commit_label`), `Field` children via `ParentElement::child`, zero engine/store/tray logic touched.

Retry cap: 2, then escalate.
