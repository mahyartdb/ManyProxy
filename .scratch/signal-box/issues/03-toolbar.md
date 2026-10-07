# 03: Lever-frame toolbar

**What to build:** the toolbar as labeled lever plates over guarded throws: the four core actions keep short labels with visible shortcut chips, Import returns as a persistent fifth button, Library/Manage/Engine/Help menus carry shortcut hints, and search gets a visible affordance. Tooltips keep naming every action and the drop hint stays honest about blocked drops.

**Blocked by:** None (can start immediately).

**Owns:** impeccable

**Status:** ready-for-agent

- [ ] Scenario: fresh window → Action: read the toolbar → Expected: Add, Start, Stop, Test, and Import visible with shortcut chips; remaining verbs grouped in named menus with hints. Verification: manual UX review. Priority: required.
- [ ] Scenario: keyboard-only pass → Action: trigger every toolbar verb without a mouse → Expected: all reachable with visible focus and full `Name (Key)` accessible names intact. Verification: accessibility check. Priority: required.
- [ ] Prohibited side effect: no stable button or menu-item id may be renamed; shortcut bindings must not change.

Retry cap: 2 attempts, then escalate to the user.
