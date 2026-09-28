# 37: Routing presets

**What to build:** import, manage, and per-row apply routing rule presets (Throne-compatible).

**Blocked by:** None (can start immediately; implement after 35–36 to avoid engine/main/store collision).

**Owns:** api-and-interface-design

**Status:** ready-for-agent

- [ ] Scenario: `throne://route` link → Action: import → Expected: named preset stored and listed with its rules (replaces the v1 named error). Verification: automated test. Priority: required.
- [ ] Scenario: preset URL / JSON file → Action: import → Expected: preset stored; malformed rules rejected naming the rule index and field. Verification: automated test. Priority: required.
- [ ] Scenario: attached preset → Action: start the row → Expected: `sing-box check` EXIT 0 with preset rules present before the isolation rule. Verification: automated test + live binary check. Priority: required.
- [ ] Scenario: preset manager → Action: add rule / remove rule / create preset / delete preset / detach → Expected: all persist and validate; delete of an attached preset is refused or detaches explicitly. Verification: automated test. Priority: important.
- [ ] Prohibited side effect: no silent fetch (explicit refresh only); no rule editing of rows without presets changing engine behavior for unattached rows; no secrets in errors.

**Status:** done (commit dab301a merged as e168c7f; 463 serial green combined, fmt+clippy clean, live sing-box check EXIT 0)
