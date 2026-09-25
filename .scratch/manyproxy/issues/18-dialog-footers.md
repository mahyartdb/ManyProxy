# 18: Dialog footers — every open_dialog needs clickable OK/Cancel

**What to build:** Every `window.open_dialog` dialog (add, edit, import, subscriptions, export) renders an explicit footer with OK/Commit and Cancel buttons wired to the same logic as the Enter/Escape key paths. Root cause: plain `Dialog` only paints footer buttons for `AlertDialog`; `button_props.on_ok` on a plain dialog fires on the Confirm key only, leaving mouse users with no commit path.

**Blocked by:** 17 (UX feedback across library flows).

**Owns:** gpui-kit

**Status:** done (commit 24a5137; `cargo test` 243 passed, verified)

- [ ] Scenario: import dialog with valid text → Action: click OK button → Expected: rows created, dialog closes, success toast names count. Verification: automated test. Priority: required.
- [ ] Scenario: add dialog → Action: click Cancel → Expected: dialog closes, no row created. Verification: automated test. Priority: required.
- [ ] Scenario: every open_dialog call site → Action: audit → Expected: each has a footer with stable button ids; no dialog relies on key-only commit. Verification: automated test. Priority: required.
- [ ] Scenario: footer OK with invalid form → Action: click OK → Expected: same behavior as Enter path (stays open + error, never silent close). Verification: automated test. Priority: important.
- [ ] Prohibited side effect: no double-commit (OK click must not also fire the key path); AlertDialog paths unchanged.
