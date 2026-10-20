# T-10 — Rule names + prevent-mods lock + completion

- **id**: T-10
- **title**: Per-rule names surfaced editor+share; prevent-modifications lock (column + readonly editor); outbound/exit suggestion completion
- **epic**: E3 — Routing & interop borrow
- **size**: M
- **depends-on**: T-08, T-09
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E3 T-10 line ("inside Q10-era R6 resolution"); gap §4.1 R6 (names + lock + completion recommended; verbatim-engine-route mode skipped deliberately — its value comes from TUN-era machinery ManyProxy does not synthesize); Throne §I route share carries per-rule `name`; §2 "Raw route JSON mode + Prevent modifications + outbound relinking" row Partial.

## Why

Shared route profiles get reviewed and adapted, not rewritten: a stable `name` per rule (Throne's share format carries it; §I) makes a 40-rule profile legible and diffable; the prevent-modification lock keeps imported/shared presets byte-stable so they keep working for other people; completion (valid outbound/exits dropdown) prevents the one loud failure class the ecosystem knows — mis-typed exit names — without ever *silently* rerouting (ManyProxy's line, gap §6 item 6).

## What

1. **Rule `name`**:
   - intake: rules may carry `"name": "<string>"` (Throne share-token `name`); keep in stored JSON (round-trip preserved — nothing to migrate, additive).
   - editor: PresetForm rule rows render the name (label column) and allow editing it; no re-ordering machinery.
   - share: the name flows through `preset_share_link` (preset.rs:497) and back through `decode_route_link` (preset.rs:362) — assert round-trip keeps names byte-exact.
   - emission: names are **not** emitted into generated sing-box `route.rules` (config hygiene; names stay an authoring artifact). One test pins the strip.
2. **Prevent-mods lock**:
   - store: add an additive column `locked INTEGER NOT NULL DEFAULT 0` on `route_presets` (via the existing `ensure_column` pattern in create_schema — no `SCHEMA_VERSION` jump; additive-only like current columns `synced_params` etc.); typed helper `set_preset_locked(id, bool)` + surfaced in the `RoutePreset` struct (store.rs:709).
   - lock toggle: checkbox in the presets manager row/details (main.rs presets dialog around 19103 region); detach/attach of a locked preset is **allowed** (locking protects the *rules*, not the attach relationship).
   - editor honoring: `open_presets_dialog` renders a **readonly** editor for locked presets (view-only fold rows, no save/copy-edit path) with an explicit "Locked — import a copy to modify" line; unlocking requires the same explicit toggle (asks-first taste, no new confirm dialog needed since the toggle is a positive action of the owner's).
3. **Completion** (small,_RDONLY): the compiled exits list in the attach/编辑 dialog — when editing a `route`-actioned rule's `outbound`, offer suggestions: `"direct"` + the serving row's outbound tag family (the real allowed values per engine semantics: the row's served outbound or `direct`; unknown values remain *typeable* but fail loud at emission with "names an unknown exit"). Feed the suggestions from the same values `preset_rules_for` accepts (engine.rs:2276–2288) — never invent silently-rerouting options (no "anything → proxy" autocomplete).
4. Loud-on-emission unchanged (engine.rs:2284 stays byte-for-byte behavior).

## Files to touch

- `src/store.rs` — `route_presets.locked` column + helper + `RoutePreset` field + tests
- `src/main.rs` — presets dialog (`open_presets_dialog` 19103, attach dialog at 19252 as needed): name column, readonly mode, lock toggle, completion list
- `src/preset.rs` — name round-trip/validation (string, ≤128 chars cap consistent with `cap_name` taste at preset.rs:85) + tests
- `src/engine.rs` — assertion tests only (names scrubbed from emission; loud failures untouched)

## Acceptance criteria

- [ ] Rule names round-trip (`preset_share_link` ↔ `decode_route_link`), are shown + editable in the editor when unlocked, and never reach generated config output.
- [ ] Locked presets render readonly with the copy-import hint; unlock/lock toggles persist across restarts (column + store test); attach/detach unaffected by lock.
- [ ] Completion offers only valid exits (`direct` + serving outbound values from the same set the engine accepts); everything typeable stays typeable — loud emission failure unchanged for unknown exits.
- [ ] Column add is additive-only (`ensure_column`); no user-version migrations (no-migration rule); a pre-existing DB opens cleanly with the new column on next launch.
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t60_`, verified unused): `t60_rule_name_round_trips_share_link`, `t60_rule_names_scrubbed_from_emission`, `t60_preset_locked_column_round_trip`, `t60_locked_preset_renders_readonly`, `t60_completion_lists_valid_exits_only`, `t60_unknown_exit_emission_stays_loud`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData: N/A.
- [x] No-migration: additive `ensure_column` only; no data-carrying migration path introduced.
- [x] Plaintext-always-sealed: N/A (names are labels).
- [x] Hijack non-concept: N/A.
- [x] No system mutation: config/store only.
- [x] Clipboard `bind:port` discipline: N/A.
- [x] Redaction: rule `name` values are already capped/redactable text; nothing secret is added (a name is a label like row labels — same treatment).
- [x] Sidecars app-dir only: N/A.

## Out of scope

- Verbatim "raw engine-route" mode (skipped per gap R6 recommendation — TUN-era machinery).
- Outbound *relinking* maps (Throne's outbound-id re-link; not adopted — completion covers the real gap).
- Rule *reordering* UI or drag handles (not asked anywhere).
- Auto-locking on import (lock is an explicit owner action).
