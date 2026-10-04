# 10: Toolbar dropdown consolidation

**What to build:** the 14-button toolbar becomes 4 dedicated core-flow buttons (Add, Start selected, Stop selected, Test selected — same ids, labels, Kbd hints, handlers) plus dropdown menus (Library: Import / From clipboard / Export; Manage: Subscriptions / Presets / Groups; Engine: Start all / Stop all / Test all; Help: About / Settings), so the strip fits narrow windows with no overflow.

**Blocked by:** None (different function than 09; run after 09 to avoid same-file collision).

**Owns:** gpui-kit-design-guides

**Status:** ready-for-agent

- [ ] Scenario: library open → Action: read command strip → Expected: 4 dedicated buttons plus Library / Manage / Engine / Help menu triggers paint; opening each menu shows its items with the existing ids, labels, tooltips, and handlers. Verification: automated test. Priority: required.
- [ ] Scenario: keyboard user → Action: press Ctrl-N / F5 / Shift-F5 / Ctrl-T → Expected: same behavior as before; SHORTCUT_LINES and About list unchanged. Verification: automated test. Priority: required.
- [ ] Scenario: existing toolbar tests → Action: run suite → Expected: `command_strip_all_ids_present`, `ticket43_chrome_paints`, t08 updated for menus (open-then-assert) and pass. Verification: automated test. Priority: required.
- [ ] Prohibited side effect: any TUN / system-proxy / DNS control added; any button id renamed; any handler behavior changed; LAN confirm or secrets handling touched.
