# 05: Inspector tabs over existing detail

**What to build:** bottom detail becomes `Details / Health / Presets` tabs driving the existing `render_detail_*` builders (`config-detail`, 240px cap, resizable 240-480), so copy/restart/log-dive keeps table context.

**Blocked by:** 01-dense-rows.

**Owns:** gpui-kit-design-guides

**Status:** ready-for-agent

- [ ] Scenario: failed running row selected → Action: open Health pane → Expected: named failure plus Start/Stop, Test, Ping, Open-logs actions operating on the same row. Verification: automated test. Priority: required.
- [ ] Scenario: arrow-key selection walk → Action: move selection → Expected: inspector follows within one step and viewport follows selection. Verification: automated test. Priority: important.
- [ ] Prohibited side effect: logs moved inline with secrets unredacted; per-process connection tree or traffic graph added.
