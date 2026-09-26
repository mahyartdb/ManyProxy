# 33: Production parity + polish (R10)

**What to build:** app feels finished: no silent footguns, honest labels, accurate docs, visible version/shortcuts.
**Owns:** src/main.rs (close-confirm, about, shortcuts hints, traffic relabel) + docs + README/QUICKSTART/PACKAGING-NOTES fixes. Tray/autostart: confirm-dialog + documented behavior now; full tray seam only if kit supports it without new deps.

- [ ] Scenario: close window with running endpoints → Action: close → Expected: confirm naming running rows (or minimize-to-tray if available); never silent kill. Verification: automated test where possible + manual screenshot. Priority: required.
- [ ] Scenario: traffic display → Action: read row/detail → Expected: labeled `probe:` until data-plane lands, or probes-count+latency only. Verification: automated test. Priority: required.
- [ ] Scenario: About/version → Action: open → Expected: version + git hash + sidecar presence + data paths + shortcuts list. Verification: manual screenshot. Priority: important.
- [ ] Scenario: toolbar buttons → Action: look → Expected: shortcut hints visible (F5/Shift-F5/Ctrl-T/Ctrl-N/Ctrl-O). Verification: screenshot. Priority: important.
- [ ] Scenario: docs → Action: read → Expected: README + PACKAGING-NOTES say app-dir-only; no `no export UI` claims; QUICKSTART covers export→wipe→restore + spillover + shortcuts. Verification: doc diff. Priority: important.
- [ ] Scenario: throne://route link → Action: import → Expected: named `routing presets not supported in v1` error, not generic unknown-scheme. Verification: automated test. Priority: important.
- [ ] Prohibited: no silent autostart/registry writes; no auto-update; no new deps without approval.

**Status:** done (verified 408 passed / 0 failed, fmt+clippy clean)
