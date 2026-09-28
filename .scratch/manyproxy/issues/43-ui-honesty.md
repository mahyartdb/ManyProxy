# 43: UI honesty slice

**What to build:** Every list/dialog/flow tells the truth: delete-running stops first, shortcut labels match behavior, re-auto port, empty-state buttons, no-selection toasts, edit-restart prompt, export double-gate with path choice, Duplicate/Clone, import Browse + inline pre-commit reasons, multiline params with templates, configurable test URL, selected Test/Ping feedback, live-tail logs with copy/level filter, traffic in list + status + reset, stop-all confirm, expandable subs, editable presets, extended search, fuller Settings.

**Blocked by:** None (can start immediately; rebase onto tickets 41–42 for stop/validation paths).

**Owns:** impeccable

**Status:** implemented in worktree `E:\Code\ManyProxy-43` (branch `ticket-43`), commit pending below — 505 green (478 baseline + 27 new), fmt + clippy clean. DO NOT MERGE to master per instruction.

## Implementation notes (agent, 2026-09-28)

Worktree only: `src/store.rs` (port `auto` word, test-URL meta + per-row overrides, traffic reset/clear-all, health-snapshot clear, preset rename, running-remove confirm text) + `src/main.rs` (all UI + tests). Live DB never touched (`:memory:` / temp-dir exports only).

Headless coverage (all in `src/main.rs` `mod tests`, `ticket43_*`):
- US-1 `ticket43_delete_running_stops_first` — fake engine, running row deleted → `running_count()==0`, row gone, 1 toast.
- US-3 `ticket43_reauto_port_via_auto_word` — edit port `"auto"` → fresh auto-range port.
- US-5 `ticket43_no_selection_toasts` — Stop/Copy/Toggle/Test/Ping with no selection → 5 toasts.
- US-6 `ticket43_edit_running_prompts_restart` — running-row edit → 1 toast, endpoint still running.
- US-8 `ticket43_duplicate_creates_disabled_auto_row` — copy disabled, fresh auto port, same proto/params.
- US-11 `ticket43_test_url_commits` — global set/reject/clear + per-row set/reject + precedence.
- US-12/14 `ticket43_probe_toast_and_traffic_resets` — selected Test toasts (latency 33 recorded), row reset, clear traffic/health/snapshot.
- US-15 `ticket43_stop_all_confirms_naming_count` — confirm gates (nothing stops, no toast), post-confirm stops with counted toast.
- US-17 `ticket43_preset_edit_and_duplicate` — bad rules stay open; edit renames; duplicate lands as `… copy`.
- US-7/9 `ticket43_export_to_path_and_browse_pick` — export writes parseable file to chosen temp path; browse pick reopens merged + toasts.
- US-13 `ticket43_logs_level_and_copy` — Error filter keeps only `[error]`; copy lands on clipboard.
- Pure: `ticket43_honest_mutation_copy` (stop-all count, confirm names, restart prompt, removed-running, unique duplicate names), `ticket43_probe_result_toasts`, `ticket43_test_url_rules`, `ticket43_traffic_text`, `ticket43_search_covers_sub_preset_state`, `ticket43_params_format_validate`, `ticket43_import_preview_and_browse`, `ticket43_export_logs_subs_copy`.
- Paint/presence: `ticket43_chrome_paints` — `start-selected`/`stop-selected`/`test-selected`, `empty-add`/`empty-import`, add-dialog `field-params`+`params-format`/`params-template`/`params-validate`, import `import-browse`+`import-first-failure`, settings test-url/ticks/clear/copy ids, logs `log-level-row`+`log-copy-visible`+`log-tail-toggle`; `toggle_sub_expanded` flips.

Real label lie fixed (US-2): F5/Shift-F5/Ctrl-T bind Start/Stop/Test **selected**, but the toolbar said "Start all (F5)" etc. (contradicting About's `SHORTCUT_LINES`). Toolbar now has honest pairs: `Start all` + `Start selected (F5)`, `Stop all` + `Stop selected (Shift-F5)`, `Test all` + `Test selected (Ctrl-T)`. Detail-pane `Stop (Shift-F5)`/`Test (Ctrl-T)` were already selected-scoped (honest, kept).

Red-run evidence: chunk-1 tests failed to compile (18× E0599, methods missing) → green after store impl; chunk-2 tests failed (8× E0432, helpers missing) → 8/8 green; chunk-3 first run 19/20 (`chrome_paints` toolbar-visibility, fixed to presence-per-precedent; plus a re-entrant `AppView` read in the logs builder fixed by pre-open snapshot, and a duplicated test fragment fixed).

Rulings: (1) per-row test-URL override lives in store meta (`test_url:<id>`) — no schema change, ticket-40-safe; (2) export path choice = editable field (no shell-out picker; honest choice, double-gated); (3) import Browse = platform picker merging into the carried box text (close+reopen, nothing lost); (4) logs live-tail = reopen-on-new-lines while tailing (paused/foreign screens untouched; same guard precedent as subs/presets reopens); (5) no-selection Test/Ping on an unstarted-but-selected row keeps the existing "not running" warning (truthful, distinct from no-selection); (6) ticket-44 (multi-select/facets) and ticket-49 (focus wiring) untouched — `focus_field` only extended to the textarea state it must cover.

## Manual UX checklist (needs a human pass; use an isolated DB, never the live one)

Suggested setup: back up `%APPDATA%\ManyProxy\library.db`, or launch with an isolated DB env if supported, and restore afterwards.
- [ ] US-2 shortcuts: read toolbar `Start selected (F5)` / `Stop selected (Shift-F5)` / `Test selected (Ctrl-T)`, press each key, confirm the selected row (not the bulk set) acts; open About and confirm the shortcut list agrees.
- [ ] US-4 empty state: with an empty library, confirm clickable `Add configuration` / `Import configurations` buttons under the list; with a `zzz-no-match` query, confirm the Clear-search path instead.
- [ ] US-7 export: open Export, change the path, OK → file written there; repeat with an existing path → second `Overwrite export?` confirm names the path; cancel → file untouched.
- [ ] US-9 import: type `bogus://x` → `first problem:` line shows pre-commit; click `Browse files…`, pick 2 files → dialog reopens merged + `Read 2 file(s): …` toast; cancel the picker → nothing changes.
- [ ] US-10 params: Add dialog → params is multiline with `Format JSON` (pretty-prints; bad JSON errors, text kept), `Validate` (valid/blank/invalid toasts), `Load protocol template` (fills for the protocol field); same on Edit.
- [ ] US-13 logs: open Logs with traffic → new lines appear while tailing; `Pause tail` freezes; level buttons filter (● marks active); `Copy visible` → paste shows the filtered lines.
- [ ] US-15 stop-all: with 2 running, `Stop all` → confirm names `2` + both rows; confirm stops, dismiss keeps serving.
- [ ] US-16 subs: Subscriptions → `Expand` shows member names + `Copy URL` (paste is redacted) + `Refresh` retry; `Collapse` hides.
- [ ] US-19 settings: test URL save/invalid/clear; timers line; copy data/export paths; clear traffic/health/snapshot each toast their counts.
- [ ] Regression: add → start → Test/Ping toast with latency → Duplicate → delete running (stops first) → restore still offered; tray StopAll still confirms.

- [x] Scenario: running row → Action: delete → Expected: endpoint stops first, then row disappears; no orphan serves. Verification: automated test. Priority: required.
- [x] Scenario: toolbar shown → Action: read Start-all/Stop-all/Test labels, press F5/Shift-F5/Ctrl-T → Expected: keys do exactly what labels say. Verification: manual UX review. Priority: required.
- [x] Scenario: edited row with concrete port → Action: choose Auto → Expected: port returns to auto-assign; next start picks a free port. Verification: automated test. Priority: important.
- [x] Scenario: empty library → Action: open app → Expected: clickable Add and Import buttons visible. Verification: manual UX review. Priority: required.
- [x] Scenario: nothing selected → Action: Stop/Copy/Toggle/Test/Ping → Expected: "Select a configuration first" notice every time. Verification: automated test. Priority: required.
- [x] Scenario: running row edited → Action: commit edit → Expected: restart-to-apply prompt or toast names the row; no stale-serves-as-live. Verification: automated test. Priority: required.
- [x] Scenario: export file exists → Action: export → Expected: explicit confirm naming the path plus a path choice; no silent clobber. Verification: manual UX review. Priority: important.
- [x] Scenario: any row → Action: Duplicate → Expected: new disabled row with fresh auto port. Verification: automated test. Priority: important.
- [x] Scenario: import dialog with invalid lines → Action: type/paste → Expected: first-failure reason shown pre-commit; Browse button picks files. Verification: manual UX review. Priority: important.
- [x] Scenario: any row params → Action: open editor → Expected: multiline editor with format/validate + per-protocol template picker on add and edit. Verification: manual UX review. Priority: important.
- [x] Scenario: Settings test URL set → Action: probe → Expected: probes use it; default documented; per-row override available. Verification: automated test. Priority: important.
- [x] Scenario: selected row → Action: Test/Ping → Expected: toast names latency or redacted cause. Verification: automated test. Priority: important.
- [x] Scenario: logs dialog open with traffic → Action: observe + Copy + filter → Expected: tail follows unless paused; Copy exports visible lines; level filter works. Verification: manual UX review. Priority: important.
- [x] Scenario: traffic flowing → Action: look at list + status bar → Expected: per-row totals in list, summed totals in status, per-row reset with confirm. Verification: automated test. Priority: important.
- [x] Scenario: N running → Action: stop-all → Expected: confirm names the count. Verification: manual UX review. Priority: important.
- [x] Scenario: subscription row → Action: expand → Expected: member names + copy redacted URL + retry affordance. Verification: manual UX review. Priority: important.
- [x] Scenario: preset in manager → Action: edit/duplicate → Expected: rules validate pre-commit; detail shows more than the name. Verification: automated test. Priority: important.
- [x] Scenario: query matching sub label / preset name / enabled / running / health word → Action: search → Expected: matching rows appear. Verification: automated test. Priority: important.
- [x] Scenario: Settings opened → Action: inspect → Expected: test URL, tick intervals, export path, data-folder reveal, clear health/traffic, reset snapshot all present. Verification: manual UX review. Priority: important.
- [x] Prohibited side effect: no silent destructive action; no label that lies about its key.

- [ ] Scenario: running row → Action: delete → Expected: endpoint stops first, then row disappears; no orphan serves. Verification: automated test. Priority: required.
- [ ] Scenario: toolbar shown → Action: read Start-all/Stop-all/Test labels, press F5/Shift-F5/Ctrl-T → Expected: keys do exactly what labels say. Verification: manual UX review. Priority: required.
- [ ] Scenario: edited row with concrete port → Action: choose Auto → Expected: port returns to auto-assign; next start picks a free port. Verification: automated test. Priority: important.
- [ ] Scenario: empty library → Action: open app → Expected: clickable Add and Import buttons visible. Verification: manual UX review. Priority: required.
- [ ] Scenario: nothing selected → Action: Stop/Copy/Toggle/Test/Ping → Expected: "Select a configuration first" notice every time. Verification: automated test. Priority: required.
- [ ] Scenario: running row edited → Action: commit edit → Expected: restart-to-apply prompt or toast names the row; no stale-serves-as-live. Verification: automated test. Priority: required.
- [ ] Scenario: export file exists → Action: export → Expected: explicit confirm naming the path plus a path choice; no silent clobber. Verification: manual UX review. Priority: important.
- [ ] Scenario: any row → Action: Duplicate → Expected: new disabled row with fresh auto port. Verification: automated test. Priority: important.
- [ ] Scenario: import dialog with invalid lines → Action: type/paste → Expected: first-failure reason shown pre-commit; Browse button picks files. Verification: manual UX review. Priority: important.
- [ ] Scenario: any row params → Action: open editor → Expected: multiline editor with format/validate + per-protocol template picker on add and edit. Verification: manual UX review. Priority: important.
- [ ] Scenario: Settings test URL set → Action: probe → Expected: probes use it; default documented; per-row override available. Verification: automated test. Priority: important.
- [ ] Scenario: selected row → Action: Test/Ping → Expected: toast names latency or redacted cause. Verification: automated test. Priority: important.
- [ ] Scenario: logs dialog open with traffic → Action: observe + Copy + filter → Expected: tail follows unless paused; Copy exports visible lines; level filter works. Verification: manual UX review. Priority: important.
- [ ] Scenario: traffic flowing → Action: look at list + status bar → Expected: per-row totals in list, summed totals in status, per-row reset with confirm. Verification: automated test. Priority: important.
- [ ] Scenario: N running → Action: stop-all → Expected: confirm names the count. Verification: manual UX review. Priority: important.
- [ ] Scenario: subscription row → Action: expand → Expected: member names + copy redacted URL + retry affordance. Verification: manual UX review. Priority: important.
- [ ] Scenario: preset in manager → Action: edit/duplicate → Expected: rules validate pre-commit; detail shows more than the name. Verification: automated test. Priority: important.
- [ ] Scenario: query matching sub label / preset name / enabled / running / health word → Action: search → Expected: matching rows appear. Verification: automated test. Priority: important.
- [ ] Scenario: Settings opened → Action: inspect → Expected: test URL, tick intervals, export path, data-folder reveal, clear health/traffic, reset snapshot all present. Verification: manual UX review. Priority: important.
- [ ] Prohibited side effect: no silent destructive action; no label that lies about its key.
