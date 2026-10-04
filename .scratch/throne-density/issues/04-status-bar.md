# 04: Three-region status inside left

**What to build:** `StatusBar.left` composes three regions (library totals / engine plus health / traffic plus probe) with the existing `status-count`, `engine-status`, `health-status`, `traffic-status` ids, since kit `StatusBar` has no center slot.

**Blocked by:** 01-dense-rows.

**Owns:** gpui-kit-design-guides

**Status:** ready-for-agent

- [ ] Scenario: 20 running, 30 stopped, traffic flowing → Action: read status bar → Expected: left shows stored plus running counts, middle shows engine state plus live/dead, right shows usage totals; all visible at once. Verification: automated test. Priority: required.
- [ ] Scenario: store read broken → Action: render status → Expected: traffic segment absent, never a lying zero. Verification: automated test. Priority: important.
- [ ] Prohibited side effect: custom status element outside kit `StatusBar`; whole-window scroll introduced.
