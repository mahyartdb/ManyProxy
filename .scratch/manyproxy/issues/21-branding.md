# 21: Window branding + title (blocked by 19)

**What to build:** The app window identifies itself: proper window title, app name in TitleBar, exe icon. Touches main.rs shell code, so it runs only after ticket 19 lands.

**Blocked by:** 19 (Library list renders — owns main.rs shell until done).

**Owns:** gpui-kit

**Status:** done (commit be51ab2; `cargo test` 251 passed, verified)

- [ ] Scenario: launch app → Action: look at taskbar/title bar → Expected: "ManyProxy" title, app icon, no "untitled"/default icon. Verification: manual UX review. Priority: important.
- [ ] Prohibited side effect: no behavior change; icon asset committed in repo (no external fetch at build).
