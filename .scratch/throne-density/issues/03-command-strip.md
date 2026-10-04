# 03: Consolidated command strip

**What to build:** one `div#command-bar` regroups the existing 14 toolbar buttons (Library / Manage / Engine / Health / Help) with the same ids, handlers, tooltips, and `F5/Shift-F5/Ctrl-T` hints, so all commands are reachable from one row.

**Blocked by:** 01-dense-rows.

**Owns:** gpui-kit-design-guides

**Status:** ready-for-agent

- [ ] Scenario: library open → Action: read command bar → Expected: Add, Import, From clipboard, Export, Subscriptions, Presets, Groups, Start sel, Stop sel, Start all, Stop all, Test sel, Test all, About, Settings all present with unchanged ids and keyboard hints. Verification: automated test. Priority: required.
- [ ] Scenario: LAN-bound row selected → Action: Start selected from the strip → Expected: LAN confirm naming config plus bind appears before anything starts; dismiss changes nothing. Verification: automated test. Priority: required.
- [ ] Prohibited side effect: any TUN / system-proxy / DNS control added; any button id renamed breaking tests.
