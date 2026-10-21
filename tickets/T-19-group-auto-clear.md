# T-19 — Per-group auto-clear-unavailable (opt-in, protected rows)

- **id**: T-19
- **title**: Per-group opt-in auto-clear-unavailable after group URL tests — never running/editing rows, own confirm note, skip-delete-confirm ≠ bypass
- **epic**: E4 — Health-test accuracy pack
- **size**: M
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E4 T-19 line; gap §4.5 H4 (Throne §6.3 auto-delete failed rows after test when enabled; equivalently the manual prune `remove_unavailable_in_group`, store.rs:4459, exists today "asks-first"); group model as-built = rows' `group` text field + facets (no groups table; verified store.rs table inventory) — per-group opt-in therefore stores as meta JSON, not a new table.

## Why

Big subscription groups accumulate dead rows after every provider rotation; manual prunes (`remove_unavailable_in_group`, the "Unavailable" group-menu action asking first) work but nobody re-marks forty groups nightly. Throne's auto-clear is the hygiene win — adopted with three human-safety riders that define this ticket: never touch the running row or the row under edit (the delete-block protections Throne itself applies to active profiles), an explicit opt-in per group with its own confirmation note, and the existing skip-delete-confirm setting counts as **acknowledgment, not bypass** (auto-deletions require their own opt-in note; skipping manual confirms must not enable automation).

## What

1. **Storage**: meta key `local:` none — a new `META` const `auto_clear_groups` (store.rs `META_*` family pattern at 6777–6982) storing a JSON map `{"<group>": {enabled: true, on_date: <unix>, note_seen: true}}`. Additive; no tables, no columns (groups are free-text field values — a map tolerates rename cascades through `rename_group`'s existing outcome path, store.rs:4302, which must migrate map keys too, one behavioral edge).
2. **Opt-in surface**: the Groups dialog + the group facet area gets "Auto-clear unavailable" per-group checkbox; **toggling on** opens its own confirm note (dialog): "Unavailable rows in \"<group>\" will be removed automatically after group tests pass with dead rows. Running rows and the row being edited are never removed." (Own ack, text is ManyProxy's own, skip-delete-confirm setting is irrelevant to it — it does not inherit skip-delete-confirm.)
3. **Application point**: end of group-URL-test sweeps (the same completion path that surfaces test summaries in main.rs) — for each group with `auto_clear_groups[group].enabled == true` AND this sweep covered it: compute available/unavailable the same way `remove_unavailable_in_group` does today (consuming the sweep's outcomes — not a separate probe pass), then remove **candidates only**:
   - exclude the **running rows** (engine running map — same source `remove_unavailable_in_group` already avoids? verified: the manual prune asks-first but does the running-row protection itself? The protection requirement is this ticket's: implement running + editing-row exclusions at the candidate-filter level regardless of what the manual path does),
   - exclude the **row under edit** (ConfigForm open with unsaved edits targeting that id — AppView's edit-in-flight state already tracks the open dialog's row),
   - exclude rows that are part of an in-flight batch (no double-trigger during batch ops).
   - Removal triggers the existing removal pipeline commit (tombstone/order semantics shared) and posts ONE summary notification per sweep: "Auto-cleared N unavailable rows in \"<group>\" (kept: running/editing list if >0)".
4. **skip-delete-confirm independence**: the "Skip delete confirmations" setting (store `META_SKIP_DELETE_CONFIRM` 6949) changes nothing here per requirement 3 — auto-clear's confirmation is its own opt-in note, and the sweep notification names the count. Tests assert independence.
5. No auto-clear on *manual* single-row tests (only group sweeps where the flag applies; re-probe cadence 60s tick (`REPROBE_TICK_SECS`, main.rs:158) does not auto-clear either — **only the user-initiated group test sweep**).

## Files to touch

- `src/store.rs` — meta map accessor + `rename_group` key migration + tests
- `src/main.rs` — Groups-dialog checkbox + its confirm note + sweep-time application + notification + tests

## Acceptance criteria

- [ ] Opt-in per group persists via the meta map; toggling off stops future auto-clears; group rename carries the flag.
- [ ] Sweep auto-clear removes only dead/unavailable rows excluding running + editing rows + in-flight batches; one summary notification names removed count and kept-reasons.
- [ ] The skip-delete-confirm setting does NOT bypass the opt-in note or alter auto behavior (independence test).
- [ ] Manual prunes behave byte-identically to today (`remove_unavailable_in_group` untouched).
- [ ] Fresh-groups/new meta absence = feature off (default-off posture everywhere).
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t69_`, verified unused): `t69_auto_clear_groups_meta_round_trip`, `t69_group_rename_carries_flag`, `t69_sweep_clears_only_dead_excluding_running_and_editing`, `t69_summary_notification_names_count_and_kept`, `t69_skip_delete_confirm_is_not_a_bypass`, `t69_optin_confirm_note_required`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData: N/A.
- [x] No-migration: additive meta only; no migration of anything (rename-migration is a live semantics edge, not a data migration path).
- [x] Plaintext-always-sealed: N/A (flags only).
- [x] Hijack non-concept: N/A.
- [x] No system mutation: library-row removal only (store op); no sidecar/OS touch.
- [x] Clipboard `bind:port` discipline: N/A.
- [x] Redaction: notifications name group/labels only.
- [x] Sidecars app-dir only: N/A.

## Out of scope

- "Insecure prune" (Throne's fourth group-op — gap §2 marks it absent deliberately; `row_security_label` sorting exists instead).
- Duplicates/Invalid prunes modifications (all ask-first today).
- Any auto-*start* or auto-restart changes.
- Running auto-clear as part of the 60 s reprobe tick or any background cadence.
