# 09: Startup window size

**What to build:** the app opens at 1000x700 with a minimum size of 880x620, instead of the OS default, so first launch matches the tested density frame instead of a huge window.

**Blocked by:** None (can start immediately).

**Owns:** gpui-kit-design-guides

**Status:** ready-for-agent

- [ ] Scenario: fresh launch with no persisted geometry → Action: open app → Expected: window content area is 1000x700. Verification: automated test. Priority: required.
- [ ] Scenario: user drags the window smaller → Action: resize below 880x620 → Expected: window stops at 880x620. Verification: automated test. Priority: required.
- [ ] Prohibited side effect: titlebar setup (`TitleBar::window_options`, branded title) changed; persisted-geometry behavior added; any other window option altered.
