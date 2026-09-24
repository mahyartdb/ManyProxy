# 02: Library CRUD + store

**What to build:** The config library itself: create, edit, remove, enable/disable proxy configs persisted locally, each with stable identity, validated bind address/port, plus copy-proxy-URL. Verifiable without any engine running.

**Blocked by:** 01 (Scaffold + shell).

**Owns:** sqlite

**Status:** done (commit 8d3f901; `cargo test` 20 passed, verified)

- [ ] Scenario: empty library → Action: add a config with name, protocol params, loopback bind, auto port → Expected: disabled record appears in list, persists across restart. Verification: automated test. Priority: required.
- [ ] Scenario: existing stopped config → Action: edit fields with invalid port (e.g. 99999) → Expected: field-level error, record unchanged. Verification: automated test. Priority: required.
- [ ] Scenario: existing config → Action: remove it → Expected: confirm dialog names the config; on confirm the record is gone. Verification: automated test. Priority: required.
- [ ] Scenario: existing config → Action: copy proxy URL → Expected: clipboard holds both socks5:// and http:// forms for its bind. Verification: integration check. Priority: important.
- [ ] Prohibited side effect: editing never starts network activity; secrets never appear in logs or exports without explicit warning.
