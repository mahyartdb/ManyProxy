# 49: A11y + error-UX pass

**What to build:** Keyboard-first, never-silent, never-ambiguous: initial focus + linked labels/errors on every dialog, actionable crash notices, guarded remove-while-running, labeled settings immediacy, styled warnings, honest empty/busy states, inline preset/sub errors, sectioned copyable About.

**Blocked by:** 43-ui-honesty (same dialog files; rebase onto ticket 44 bulk bar).

**Owns:** accessible-frontend

**Status:** ready-for-agent

- [ ] Scenario: every dialog (Add/Edit/Import/Sub/Logs/Settings/Presets) → Action: open keyboard-only → Expected: focus lands in first field; labels/errors programmatically linked and announced. Verification: accessibility check. Priority: required.
- [ ] Scenario: sidecar crash → Action: notice appears → Expected: actionable (opens Logs) instead of plain "View logs" text. Verification: manual UX review. Priority: important.
- [ ] Scenario: running row → Action: remove → Expected: warning names running state or stops endpoint; delete-confirm path covers it. Verification: automated test. Priority: required.
- [ ] Scenario: Settings toggles → Action: read labels, toggle, Cancel → Expected: labels state applies-immediately; Cancel cannot undo; failed write reverts the cell. Verification: manual UX review. Priority: important.
- [ ] Scenario: LAN hint / collision / plaintext-http warnings → Action: view → Expected: styled with prefix/icon/weight, never body-text lookalikes. Verification: manual UX review. Priority: important.
- [ ] Scenario: empty import / empty library export / no-hits / first hide-to-tray / busy Start → Action: trigger each → Expected: warn-not-success; guarded export; single real clear-search button; one-time tray toast; disabled Start with aria-busy. Verification: manual UX review. Priority: important.
- [ ] Scenario: preset attach / sub add-edit failure → Action: fail → Expected: inline dialog error with focus, matching Add/Edit behavior (never toast-only). Verification: manual UX review. Priority: important.
- [ ] Scenario: About opened → Action: inspect → Expected: version/sidecars/paths/shortcuts sectioned with copy-paths affordance. Verification: manual UX review. Priority: important.
- [ ] Scenario: restore with per-row failures → Action: restore subset → Expected: failed rows named with redacted cause (never bare count). Verification: automated test. Priority: important.
- [ ] Prohibited side effect: no color-only status; no keyboard trap in any dialog.
