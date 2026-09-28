# 43: UI honesty slice

**What to build:** Every list/dialog/flow tells the truth: delete-running stops first, shortcut labels match behavior, re-auto port, empty-state buttons, no-selection toasts, edit-restart prompt, export double-gate with path choice, Duplicate/Clone, import Browse + inline pre-commit reasons, multiline params with templates, configurable test URL, selected Test/Ping feedback, live-tail logs with copy/level filter, traffic in list + status + reset, stop-all confirm, expandable subs, editable presets, extended search, fuller Settings.

**Blocked by:** None (can start immediately; rebase onto tickets 41–42 for stop/validation paths).

**Owns:** impeccable

**Status:** ready-for-agent

- [ ] Scenario: running row → Action: delete → Expected: endpoint stops first, then row disappears; no orphan serves. Verification: automated test. Priority: required.
- [ ] Scenario: toolbar shown → Action: read Start-all/Stop-all/Test labels, press F5/Shift-F5/Ctrl-T → Expected: keys do exactly what labels say. Verification: manual UX review. Priority: required.
- [ ] Scenario: edited row with concrete port → Action: choose Auto → Expected: port returns to auto-assign; next start picks a free port. Verification: automated test. Priority: important.
- [ ] Scenario: empty library → Action: open app → Expected: clickable Add and Import buttons visible. Verification: manual UX review. Priority: required.
- [ ] Scenario: nothing selected → Action: Stop/Copy/Toggle/Test/Ping → Expected: "Select a configuration first" notice every time. Verification: automated test. Priority: required.
- [ ] Scenario: running row edited → Action: commit edit → Expected: restart-to-apply prompt or toast names the row; no stale-serves-as-live. Verification: automated test. Priority: required.
- [ ] Scenario: export file exists → Action: export → Expected: explicit confirm naming the path plus a path choice; no silent clobber. Verification: manual UX review. Priority: important.
- [ ] Scenario: any row → Action: Duplicate → Expected: new disabled row with fresh auto port. Verification: automated test. Priority: important.
- [ ] Scenario: import dialog with invalid lines → Action: type/paste → Expected: first-failure reason shown pre-commit; Browse button picks files. Verification: manual UX review. Priority: important.
- [ ] Scenario: any row params → Action: open editor → Expected: multiline editor with format/validate + per-protocol template picker on add and edit. Verification: manual UX review. Priority: important.
- [ ] Scenario: Settings test URL set → Action: probe → Expected: probes use it; default documented; per-row override available. Verification: automated test. Priority: important.
- [ ] Scenario: selected row → Action: Test/Ping → Expected: toast names latency or redacted cause. Verification: automated test. Priority: important.
- [ ] Scenario: logs dialog open with traffic → Action: observe + Copy + filter → Expected: tail follows unless paused; Copy exports visible lines; level filter works. Verification: manual UX review. Priority: important.
- [ ] Scenario: traffic flowing → Action: look at list + status bar → Expected: per-row totals in list, summed totals in status, per-row reset with confirm. Verification: automated test. Priority: important.
- [ ] Scenario: N running → Action: stop-all → Expected: confirm names the count. Verification: manual UX review. Priority: important.
- [ ] Scenario: subscription row → Action: expand → Expected: member names + copy redacted URL + retry affordance. Verification: manual UX review. Priority: important.
- [ ] Scenario: preset in manager → Action: edit/duplicate → Expected: rules validate pre-commit; detail shows more than the name. Verification: automated test. Priority: important.
- [ ] Scenario: query matching sub label / preset name / enabled / running / health word → Action: search → Expected: matching rows appear. Verification: automated test. Priority: important.
- [ ] Scenario: Settings opened → Action: inspect → Expected: test URL, tick intervals, export path, data-folder reveal, clear health/traffic, reset snapshot all present. Verification: manual UX review. Priority: important.
- [ ] Prohibited side effect: no silent destructive action; no label that lies about its key.
