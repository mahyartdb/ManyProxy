# 01: Foundation — picker, locks, pristine template, ENTER guard

**What to build:** Add/Edit dialog gets a protocol dropdown over the 23 stored types, type locked on edit, Server/Port hidden for virtual types, pristine-only template rewrite, and combo ENTER-guard — the scaffolding every later panel ticket lands on.

**Blocked by:** None (can start immediately).

**Owns:** gpui-kit composition (kit-maximal: `Select` + `Field`/`v_form`, wrappers only at 0.6.6 limits), sqlite (no migration — list/flag table in code only).

**Status:** ready-for-agent

- [ ] Scenario: Add → Action: read Protocol control → Expected: kit `Select` lists exactly the 23 canonical types, no free-text typos possible. Verification: automated test (headless id `protocol-select` paints + lists 23). Priority: required.
- [ ] Scenario: Edit existing row → Action: open Edit → Expected: type control locked (read-only text, no switcher). Verification: automated test (edit paints lock, commit keeps original type). Priority: required.
- [ ] Scenario: Add `chain` / `custom-outbound` / `custom-config` / `extra-core` → Action: read form → Expected: Server/Port inputs hidden (Throne gating parity). Verification: automated test (virtual-type hides address/port ids). Priority: required.
- [ ] Scenario: user-typed JSON + type switch → Action: switch protocol → Expected: JSON stands (rewrite only when JSON is byte-identical pristine template output). Verification: automated test (pristine rewrites, dirty preserves). Priority: required.
- [ ] Scenario: open dropdown → Action: press ENTER inside popup → Expected: popup consumes it, dialog does NOT commit a half-filled row. Verification: automated test (ENTER-guard regression). Priority: required.
- [ ] Prohibited side effect: no per-protocol inner fields yet; no engine/import changes; no `user_version` bump; no bare OK footers; no hand-rolled dropdown divs (kit `Select` only); no invented kit APIs.

Retry cap: 2, then escalate.
