# 01: Scaffold + shell

**What to build:** A fresh desktop app that boots to an empty proxy-library shell: window with title bar, status bar, searchable list region, theme styling, and keyboard navigation skeleton. No configs, engines, or imports yet — just the verified shell every later ticket builds on.

**Blocked by:** None (can start immediately).

**Owns:** gpui-kit

**Status:** done (commit a5bc819; `cargo test` 1 passed)

- [ ] Scenario: cold start → Action: launch app → Expected: main window opens with title bar, empty config list, status bar showing 0 configs. Verification: automated test. Priority: required.
- [ ] Scenario: overlay layers mounted → Action: open add-dialog route in headless window test → Expected: dialog layer renders (not a blank/no-op). Verification: automated test. Priority: required.
- [ ] Scenario: keyboard skeleton → Action: Tab through header, list, status regions → Expected: visible focus moves in a stable order, no focus traps or dead ends. Verification: manual UX review. Priority: important.
- [ ] Prohibited side effect: no network access, no OS proxy/DNS/route changes, no files written outside the app data dir.
