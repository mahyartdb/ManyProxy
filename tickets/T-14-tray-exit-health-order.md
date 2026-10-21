# T-14 — Tray Select-exit health ordering (monitor, not switcher)

- **id**: T-14
- **title**: "Best rows first" by latest health in the tray's Select-exit surface — ordering/annotation only, no pools, no switching
- **epic**: E3 — Routing & interop borrow
- **size**: S
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E3 T-14 line; gap §4.2 C3 (auto-selector machinery — recommend NOT borrowing; "if the underlying need is 'keep pointing consumers at the best row', the in-job answer is health-driven UX, not runtime switching"); Q7 adopt-1-only.
- **surface correction (gap doc → as-built)**: Throne's Select-profile *submenu* maps in ManyProxy to a single tray item — `tray.rs:11 menu_id::SELECT_EXIT` ("tray-select-exit") → `TrayAction::SelectExit` (tray.rs:23) → `main.rs:13232 open_tray_exit_picker` → `open_preset_chooser_for_selected` → `open_preset_attach_dialog` (main.rs:19252) which lists **route presets** (`self.store.list_presets()` ordered by `id`, store.rs:4794–4798) and attaches one to the selected row. There is no row-listing submenu today; "ordering in the submenu" therefore lands as the dialog's monitor section below — the surface reality is documented here so nobody re-invents a switcher.

## Why

The user need behind Throne's auto-selector is "point consumers at the best row" — but consuming rows are things a user *subscribes to by hand* in ManyProxy; switching outbounds under a listener would silently move external clients' traffic paths (violating the row = stable endpoint promise; C3). The adopted piece (Q7 adopt-1-only) is the health-driven **monitoring** UX: when the user opens the exit-selection surface, show which rows are best right now, ordered by latest health — informational, never behavioral.

## What

1. New pure ordering helper (main.rs or health.rs): `fn best_rows_by_health<'a>(rows: &'a [ProxyConfig], health: &HealthStore) -> Vec<&'a ProxyConfig>` — sorts rows of the **selected row's group** by latest URL-test sample first-alive ordering: live-with-latest-latency ascending > live-unknown-error-free > dead last, stale-aware (reuse `HealthStore::latest_of(summary)` + `row_summary` patterns — no new sampling; `GroupTestStop` flags untouched).
2. Dialog change in `open_preset_attach_dialog` (main.rs:19252): **above** the preset list, a read-only "monitor" section: "Best by health in {group}: 1) <name> (45 ms) 2) <name> (220 ms) 3) …" (top 5, capped; empty group → section hidden). Rows are **not clickable to switch anything**; purely ordered text summary of the user's own data. Per-preset entries below behave exactly as today (Attach/Detach; `current` badge).
3. Tray side: no change to `tray.rs` (`SELECT_EXIT`/menu wiring untouched); tooltip stays "ManyProxy · N running" (`tray.rs:348 set_running`).
4. No settings, no persistence (read-only view computed per open; zero new meta keys).

## Files to touch

- `src/main.rs` — helper fn + `open_preset_attach_dialog` section + tests
- `src/health.rs` — none behaviorally (helper may live here instead; choose by cohesion with `row_summary`)

## Acceptance criteria

- [ ] Opening the tray Select-exit dialog shows the monitor section ordered by latest health, limited to the selected row's group, top 5; dead/unknown/no-sample rows render in a stable deterministic order; `(stale)` awareness consistent with row text today.
- [ ] Entries are display-only — no click target, no switching, nothing changes the engine; pools/selector-like machinery absent (test asserts no new protocol/outbound injection).
- [ ] Preset attach/detach flows byte-identical in behavior to today.
- [ ] Empty group / single row / all-unknown shapes render sanely (no empty-space glitch).
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t64_`, verified unused): `t64_best_rows_ordering_live_wins_wrong_dead_last`, `t64_dialog_monitor_section_renders_top_five`, `t64_dialog_monitor_hidden_for_empty_group`, `t64_no_switching_behavior_added`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData: N/A.
- [x] No-migration: N/A (no meta keys).
- [x] Plaintext-always-sealed: N/A.
- [x] Hijack non-concept: N/A.
- [x] No system mutation: strictly a read-only view of existing samples; no probe side effects; no traffic changes.
- [x] Clipboard `bind:port` discipline: N/A.
- [x] Redaction: the monitor shows names/latency only — same data row text already shows; secrets never enter the string.
- [x] Sidecars app-dir only: N/A.

## Out of scope

- Auto-selector pools or runtime outbound switching (dead).
- Health-driven *automatic* preset attach or row restarts (not asked; would be behavior, not monitoring).
- Tray icon state images per mode (PARKED-adjacent; not ticketed).
- Changing `list_presets` ordering in the store (the dialog change is presentation-local).
