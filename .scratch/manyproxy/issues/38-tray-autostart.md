# 38: Tray + autostart

**What to build:** tray presence with hide-on-close and menu, plus opt-in autostart with session restore.

**Blocked by:** None (can start immediately; implement last to avoid main.rs collision).

**Owns:** gpui-kit

**Status:** ready-for-agent

- [ ] Scenario: running endpoints → Action: close window → Expected: window hides, tray icon shows `ManyProxy · N running`, endpoints keep serving. Verification: manual UX review. Priority: required.
- [ ] Scenario: tray → Action: use Show / Start all / Stop all / Quit → Expected: each works; Quit stops engines and exits with no orphans. Verification: manual UX review. Priority: required.
- [ ] Scenario: autostart opt-in → Action: enable → Expected: HKCU Run value written; disable removes it; default off; no silent writes. Verification: automated test + manual UX review. Priority: required.
- [ ] Scenario: restore opt-in with running set → Action: relaunch → Expected: previously running set restarts (LAN confirm gate still enforced). Verification: automated test. Priority: important.
- [ ] Prohibited side effect: no tray without hide (no dishonest presence); no autostart default-on; no new art assets (app icon only).
