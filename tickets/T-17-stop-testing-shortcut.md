# T-17 — Stop Testing shortcut `Ctrl-.` (menu item already exists)

- **id**: T-17
- **title**: Stop Testing affordance completion — `Ctrl-.` keybinding + shortcut-table line; the Tools menu item already exists (correction vs gap doc)
- **epic**: E4 — Health-test accuracy pack
- **size**: S
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E4 T-17 line; gap §4.5 H2 ("Stop flag exists [C]; add a visible 'Stop testing' toolbar/menu item mapping to the same flag — Throne's Ctrl+. pattern") — **with a surface correction found during this drafting's verification**: the visible menu item ALREADY exists — `main.rs:24081–24098` renders Tools ▸ "Stop Testing" (`topbar_menu_row("stop-testing", "Stop Testing", …, "Stop the running group test")`) wired to `stop_group_tests` (main.rs:14624), which flips `probe_stop.store(true, Ordering::SeqCst)` on the existing `probe_stop: Arc<AtomicBool>` (main.rs:11232). What is missing is the keybinding (`bind_app_keys`, main.rs:132–153, has no `ctrl-.`) and the discoverability line in `SHORTCUT_LINES` (main.rs:191) and the palette row (`main.rs:33237 ("stop-testing", "Stop Testing")`).

## Why

Stop-flinging-needless-probes is a keyboard-distance question: the ad-hoc hidden path exists but keying it (Throne's `Ctrl-.`, §B.1 runtime additions) makes it discoverable & instant. The plumbing to be bound is long done — no flag surgery, no new state; this ticket is small precisely because the gap-analysis surface description was out of date on the menu item.

## What

1. Add an action to the `actions!` block (main.rs ~112–127): `StopTesting`.
2. `bind_app_keys` (main.rs:132): `KeyBinding::new("ctrl-.", StopTesting, Some(APP_KEY_CONTEXT))`.
3. Wire the action to `AppView::stop_group_tests` (dialog-free straight dispatch; no-op when nothing in flight — `stop_group_tests` already sets the flag idempotently).
4. `SHORTCUT_LINES` gains `("Stop Testing", "Ctrl-.")` (main.rs:191 list; its rendering is the Settings hotkey list + the shortcut table at main.rs:361; the integrity test `crate::SHORTCUT_LINES` pinning loop at main.rs:46553–46554 uses the expected count — extend that test's data, don't break).
5. Tools-menu item (already present) gains the key text hint via `topbar_menu_row`'s existing keybinding display slot (the same pattern "Start selected (F5)" uses; the interception to pass Some(("ctrl-.", "(Ctrl-.)", "Ctrl-.")) mirrors the shift-f5 item at main.rs:23800).
6. Palette line already exists (`("stop-testing", "Stop Testing")`, main.rs:33237) — verify the palette op dispatches the same `stop_group_tests` path (already false-gap; keep).

## Files to touch

- `src/main.rs` only — actions! block, `bind_app_keys`, SHORTCUT_LINES data + its pinning test data, menu item hint

## Acceptance criteria

- [ ] Pressing `Ctrl-.` in the library context stops an in-flight group test (flag flips exactly as the menu item does — same fn called).
- [ ] No-op when idle (no spurious notifications/flag churn beyond the existing fn).
- [ ] Settings shortcut table shows the new line; the `SHORTCUT_LINES` pinning test updated and green.
- [ ] The menu item tooltip/key hint shows "Ctrl-." without duplicating the action handler.
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t67_`, verified unused): `t67_shortcut_lines_lists_stop_testing_key`, `t67_ctrl_period_stops_group_test`, `t67_menu_item_shows_key_hint`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData / No-migration / Plaintext / Hijack / Sidecars: N/A.
- [x] No system mutation: keyboard plumbing only.
- [x] Clipboard `bind:port` discipline: N/A.
- [x] Redaction: N/A.

## Out of scope

- Toolbar button (the item is menu+palette; a toolbar chip is new surface no decision asks for).
- Global hotkeys outside the app window (Throne hotkey map — Missing, parked-adjacent; not ticketed).
- Changing `probe_stop` ownership or the in-flight gating (`probe_kind_in_flight`).
