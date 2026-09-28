# 38: Tray + autostart

**What to build:** tray presence with hide-on-close and menu, plus opt-in autostart with session restore.

**Blocked by:** None (can start immediately; implement last to avoid main.rs collision).

**Owns:** gpui-kit

**Status:** implemented — automated suite green; tray windowing pending manual UX review (steps below).

- [x] Scenario: running endpoints → Action: close window → Expected: window hides, tray icon shows `ManyProxy · N running`, endpoints keep serving. Verification: manual UX review. Priority: required.
- [x] Scenario: tray → Action: use Show / Start all / Stop all / Quit → Expected: each works; Quit stops engines and exits with no orphans. Verification: manual UX review. Priority: required.
- [x] Scenario: autostart opt-in → Action: enable → Expected: HKCU Run value written; disable removes it; default off; no silent writes. Verification: automated test + manual UX review. Priority: required.
- [x] Scenario: restore opt-in with running set → Action: relaunch → Expected: previously running set restarts (LAN confirm gate still enforced). Verification: automated test. Priority: important.
- [x] Prohibited side effect: no tray without hide (no dishonest presence); no autostart default-on; no new art assets (app icon only).

## Manual UX review steps (tray windowing is not headless-testable)

1. `cargo run` → tray icon appears (same art as the exe icon); hovering shows `ManyProxy · 0 running`.
2. Start one endpoint → tooltip becomes `ManyProxy · 1 running` within a poll tick.
3. Close the window (X) → window disappears from screen AND taskbar; endpoint still serves (curl through it); tray tooltip still counts it.
4. Left-click tray icon → window reappears with state intact (rows, selection, logs).
5. Tray menu Start all / Stop all → same toasts as the toolbar buttons; counts follow.
6. Tray Quit with endpoints running → icon disappears, `manyproxy.exe` and sidecars exit (Task Manager shows no orphans).
7. Settings → enable autostart → `reg query HKCU\...\Run /v ManyProxy` shows the quoted exe path; disable → value gone. Fresh profile shows both switches off.
8. Enable restore + autostart, run one loopback endpoint, kill the process (End task, simulating reboot-without-quit), relaunch → endpoint restarts with a "Restored 1 endpoint(s)" toast. With a `0.0.0.0` row in the set, relaunch shows the LAN confirm instead of starting it silently.
9. Tray-refused fallback: (hard to force on stock Windows) — if no tray, closing with running endpoints must show the old "Close ManyProxy?" confirm naming every row.

## Rulings (deviations from plan-parity2 §5–6)

- R1 — `png` as a **build**-dependency (not the hand-rolled ICO parser): all four `assets/icon.ico` entries are PNG-compressed, so a "minimal ICO parser" would need a PNG/zlib decoder anyway. The build script parses the ICO directory and PNG-decodes via `png` at build time only; zero new runtime deps, same asset, no new art.
- R2 — hide via raw `HWND` `ShowWindow(SW_HIDE/SW_SHOW)` (hand-rolled `user32` FFI, same shape as `dpapi.rs`): GPUI exposes only minimize/activate/remove — minimize keeps a taskbar button (not a hide), and remove destroys GPUI state. Native hide keeps the engine untouched by construction.
- R3 — no `autostart` mirror column in `meta`: the registry is read live for the checkbox (single source of truth); a mirror would drift when the key is edited outside the app.
- R4 — no schema-version bump for the `meta` table: purely additive `CREATE TABLE IF NOT EXISTS`, backward compatible with older builds (unlike the v3 DPAPI migration, which changed existing rows).
- R5 — restore starts loopback rows directly and opens the standard LAN confirm for LAN rows (instead of calling engine `start_all_confirmed`, which would silently mark LAN binds confirmed). The ticket's "gate still enforced" is read literally: no LAN row starts without an explicit confirm.
- R6 — `commit_autostart` (real Run-key writer) has no automated test by design; coverage is the temp-key round-trip in `tray.rs` plus step 7 above. The live DB is never touched by tests (in-memory stores only).
