# 17: UX feedback across library flows

**What to build:** Every library action gives feedback: success toasts for add/edit/remove, import preview before commit, drop affordances, export preview + overwrite guard, subscription failure visibility, no toast spam, honest start/stop/copy messages, findable rows.

**Blocked by:** 13 (Convergence — live sidecar log pipe).

**Owns:** gpui-kit

**Status:** ready-for-agent

- [ ] Scenario: add/edit/remove config → Action: complete → Expected: success toast naming the row. Verification: manual UX review. Priority: important.
- [ ] Scenario: import dialog with pasted text → Action: type/paste → Expected: live `X valid · Y invalid` preview before commit. Verification: automated test. Priority: important.
- [ ] Scenario: drag files over shell → Action: hover → Expected: visible drop hint; dialog shows file names after drop. Verification: manual UX review. Priority: optional.
- [ ] Scenario: export dialog → Action: open → Expected: shows row counts + destination path + overwrite warning when file exists. Verification: automated test. Priority: important.
- [ ] Scenario: subscription refresh with item failures → Action: refresh → Expected: failures viewable per subscription (not just aggregated count). Verification: automated test. Priority: important.
- [ ] Scenario: background tick refreshes 3 subs → Action: wait → Expected: at most one coalesced toast, not three. Verification: automated test. Priority: optional.
- [ ] Scenario: Start all on empty library / Stop on stopped row → Action: trigger → Expected: honest "nothing to start/stop" message, never "Started 0". Verification: automated test. Priority: important.
- [ ] Scenario: search "10.0.0.5" or "10801" → Action: type in search → Expected: matching rows by host/bind/port, not just name/protocol/group. Verification: automated test. Priority: important.
- [ ] Scenario: subscription add form → Action: open → Expected: auto-refresh checkbox present (no more create-then-toggle). Verification: automated test. Priority: important.
- [ ] Scenario: add/edit with 0.0.0.0 bind → Action: type → Expected: inline hint that starting will need LAN confirm. Verification: manual UX review. Priority: optional.
- [ ] Prohibited side effect: no toast on every keystroke; no blocking modal for informational feedback.
