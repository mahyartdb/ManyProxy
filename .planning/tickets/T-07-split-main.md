# T-07 — Split `src/main.rs` into modules (pure moves)

Baseline: HEAD `aa0a94f`. `src/main.rs` = 58,685 lines / 2.26 MB, with 868 `crate::` paths.
Target: `main.rs` ≈ 150 lines (inner attrs, module decls, root `use m::*;` seams, `fn main`,
crash hook) across ~39 files, max depth 2 under `src/`.

## Decision history

The user chose the full split; the reviewer argued for splitting only consts + tests first.
Resolution: reorder rather than truncate — the full split still happens, but UI-facing
clusters move first and the rest follows the UI work.

Why: T-02..T-06 rewrite ~2,700 lines of `impl AppView`, ~60% of the split cost. Moving it
first lets those tickets edit dedicated small files instead of relocating rewritten code.

## Non-negotiable discipline

1. **Pure moves.** No renames, cleanup, behavior edits, or "while I'm here" fixes. Only item
   visibility (`pub(crate)`), `use` headers, and `mod` declarations may change.
2. **Each slice compiles and passes `cargo test --locked` alone.** A slice that cannot get
   green is reverted, not patched forward.
3. **One commit per slice**, `refactor: split <cluster> out of main.rs` + one line of why.
4. **Prove it's a move.** Review `git diff --color-moved=zebra --stat`; a hunk that changes
   logic is a defect.
5. T-01 commits first; do not carry its in-flight changes into a slice.

## Visibility strategy (the cost lever)

Visibility is widened lazily: each item becomes `pub(crate)` at the moment it moves, and
nothing else changes in that slice. A separate visibility-only sweep of all ~370 `AppView`
methods was rejected — it churns every method to make later diffs readable, but most of those
methods do not move for many slices, so the sweep is pure noise and hides the real move.

At the crate root add `use <module>::*;` per module; a private `use` at the crate root is
visible to descendants, so the 868 `crate::X` paths keep resolving. Add explicit imports for
the two cfg-split duplicate names (`open_url_shell_execute`, `win32_process_sample`).

## Phase 1 — UI-facing slices, then stop

| # | Cluster moved | New file(s) | Notes |
|---|---|---|---|
| 1 | consts, keys, appearance, text | `consts.rs` `keys.rs` `appearance.rs` `text.rs` | DONE `eb09b33` |
| 2 | `mod tests` verbatim | `tests.rs` | DONE `605f3c1` |
| 3 | library list + columns + match | `library/mod.rs` `library/columns.rs` `library/match.rs` | move `RowMetrics` with both impls |
| 4 | shell + toolbar + render | `app/shell.rs` `app/toolbar.rs` `app/render.rs` | |
| 5 | detail/inspector + status + search/bulk | `app/detail.rs` `app/status.rs` `app/search.rs` `app/bulk.rs` | |
| 6 | settings | `app/settings.rs` | |

Stop after slice 6. T-02..T-06 run here, inside these files.

## Phase 2 — after the UI work

Forms, fetch/update, subs, presets, import, health, logs, io, ticks, models, dialog kit,
`win32_process`. The UI work does not touch these, so they split afterwards without conflict.

### Phase 2 slices (measured against the 20,899-line `main.rs`)

| # | Cluster moved | New file(s) | Anchors in `main.rs` |
|---|---|---|---|
| 2a | forms + the dialog/QR kit | `forms/{config,import,sub,preset,rules}.rs` `dialog/{kit,qr}.rs` | `ConfigForm` 741, `ImportForm` 6562, `SubForm` 8054, `PresetForm` 8471, `SimpleRuleBoxes` 8613, `dialog_field`/`dialog_footer`/`sheet_footer`/`footer_commit_label`, `advanced_settings_button`/`password_row`/`advanced_toggle_button`/`panel_toggle_button`, `secret_state_caption`/`field_error_target`, `QrGrid`/`QrPlate`/`qr_grid`/`share_qr_*`/`paint_share_qr` |
| 2b | import resolution + fetch/update/semver | `import_flow/{resolve,preview}.rs` `update/{fetch,release,semver}.rs` | `resolve_import_inputs` 6754, `parse_import_inputs` 6779, `ImportFileRead`/`read_import_file` 6919, `first_import_failure_reason` 7189, `merge_browse_results` 7209, `FetchedSubscription` 7339, `FetchedHttp` 7360, `resolve_subscription_host` 7471, the redirect helpers 7494–7529, `parse_release_tag` 7587, `compare_parsed_tags` 7638, `parse_release_feed` 7731, `update_check_outcome` 7771, `effective_update_feed_url` 7830, `open_release_in_browser` 7951, `open_url_shell_execute` 7963 AND 7994 (the cfg-split pair — see trap 9) |
| 2c | groups, subscriptions, presets, rule sets | `preset/{mod,groups,rules,sets}.rs` (turn the existing `src/preset.rs` into `src/preset/mod.rs`) | `redact_subscription_url` 6699, `subscription_label_from_url` 6724, `subscription_row` 8151, `GroupRowView`/`group_row_views` 8335, `group_quota_line` 8400, `update_target_subs` 8441, `update_all_targets` 8451, `preset_row` 8665, `PresetRulesOp` 8765, `rule_set_row` 8771, `preset_rule_attribute_folds` 8832, `preset_advanced_section` 8856 |
| 2d | models — rows, traffic, inspector tab, palette ops, sampler, chains | `models/{row,traffic,connections,tab,palette,chain,sampler}.rs` | `ConnRow`/`ConnCopyTarget`/`build_conn_rows` 9817, `ExitTraffic` 9884, `TrafficExitSplit` 9914, `traffic_top9_split` 9930, `aggregate_exit_traffic` 9971, `InspectorTab` 10003, `ProcessSample`/`SampleRead`/`cpu_percent_of_one_core` 10070, `ShardRing` 10112, `ChainLibraryEntry` 9620, `WgPeerRefs` 9707, `generate_uuid_v4` 9727, `generate_wireguard_key` 9786, `PaletteOp` 9786 |
| 2e | pickers — facet, protocol, panel and every `*_choices` list | `pickers/{facet,protocol,panel,choices}.rs` | `FacetMenu`/`FacetChoice`/`FacetChoiceList` 9028, `FacetPicker` 9222, `ProtocolChoice`/`ProtocolChoiceList` 9260, `PanelChoice`/`PanelChoiceList` 9339, the `*_choices` functions 9424–9610, `chain_pick_choices` 9626, `chain_slot_kind` 9642, `chain_existing_kinds` 9656, `chain_hop_label` 9680 |
| 2f | logs, health tone, row helpers, the bind-host picker | `library/{tone,rows,host}.rs` (existing `library/` dir) | `log_level_text_color` 322, `log_line_level` 336, `parse_log_cap` 345, `log_level_matches` 7228, `ServingTone`/`health_tone_color` 6810, `EndpointUrl` 6832, `RowPhase` 6846, `library_row_lines`/`library_row_cells`/`library_row_full_route`/`library_row_matches` 6976, `MatchContext` 7043, `BindHostChoice`/`BindHostChoiceList` 215, `RowEndpointInputs` 289, `SortMenuAction` 311, `upstream_host_port` 6942 |
| 2g | bootstrap — the eight boot functions, the window, the tray, `main` | `app/bootstrap.rs` `app/window.rs` | `app_window_options` 177, `stored_window_geometry` 206, `new`, `with_store`, `create_tray`, `restore_on_launch`, `commit_restore_subset`, `open_restore_lan_confirm`, `handle_launch_intake`, `reload`, `spawn_hidden` 135, `main`, the crash hook |

After 2g, `main.rs` holds only: the inner attributes (`#![recursion_limit]`,
`#![cfg_attr(windows, windows_subsystem = "windows")]`), the `mod` declarations, the crate-root
`use …::*;` seams, `fn main`, and the crash hook — target ≈150 lines.

### Slice 2g's scan re-point (do it exactly this way)

`direct_overlay_calls` asserts `stripped.matches(marker).count() == 1` — exactly once, per
marker, inside whatever source it is handed. Moving the eight boot functions out of `main.rs`
therefore fails loudly with `marker names exactly one function` (count 0), which is correct.

Re-point by composing the scanned set, not by dropping markers and not by scanning only one file:

```rust
let source = format!(
    "{}\n{}",
    include_str!("main.rs"),
    include_str!("app/bootstrap.rs"),
);
let found = direct_overlay_calls(&source, &BOOT_FUNCTIONS);
```

The exactly-once assertion stays byte-for-byte as it is. The CRLF pin composes the SAME set
before converting endings. If a later slice moves a boot function again, add its file to the
same `format!` — never split the assertion, never scope it per file.

## Traps (verified — do not skip)

1. **Boot-path source scan.** `include_str!("main.rs")` ×2 (`52547`, `52562`);
   `direct_overlay_calls` asserts each marker matches **exactly once** (`52625-52640`).
   Moving a boot function makes it fail loudly — that is correct. In the same slice,
   re-point the scan at the new file(s) and keep the exact-once assertion across the scanned
   set. Never weaken, skip, or delete the guard. `BOOT_FUNCTIONS` names `new`, `with_store`,
   `create_tray`, `restore_on_launch`, `commit_restore_subset`, `open_restore_lan_confirm`,
   `handle_launch_intake`, `reload`.
   **Slices 3–6 move none of those eight**, so the scan stays untouched there — leave it
   exactly as it is. **Slice 2g moves them**: re-point the scan at the new file(s) in that
   slice and keep the exactly-once assertion across the scanned set. The assertion stays
   exactly as strict — a scanner that reads several files still asserts each marker once.
2. **`std::fs::read_to_string(file!())`** (`29247`) asserts the source contains
   `const APP_VERSION` + `env!("CARGO_PKG_VERSION")`. Moving tests breaks it: retarget to
   `consts.rs` and keep that const on one line.
3. **`include_str!("../tests/fixtures/uia-*.json")`** resolves relative to the source file, so
   test modules stay flat: `src/tests.rs`, not `src/tests/mod.rs`.
4. **`#[cfg(test)]` methods inside `impl AppView`** (10) and free fns (6) lose their attribute
   on a bad move; only clippy `dead_code` under `-D warnings` catches it.
5. **Inner attributes** `#![recursion_limit]` and `#![cfg_attr(... windows_subsystem ...)]`
   stay the first lines of `main.rs`.
6. **`use gpui_kit::*;` is file-scoped.** Every new module needs its own header; stray unused
   imports are hard errors under `-D warnings`.
7. **`static LOCK`** (`56374`) serialises the only test that mutates a process-global env var;
   keep that test and its function in the same file.
8. **`mod win32_process`** uses `super::{ProcessSample, …}`; after extraction,
   `crate::models::`.
9. **Two cfg-split twins.** `open_url_shell_execute` exists TWICE in `main.rs` (7963 and
   7994) and `win32_process_sample` has the same shape — one per `cfg` arm. Moving BOTH
   into one module is fine only while they stay cfg-gated; if a crate-root glob seam would
   import both names at once, add the explicit import the ticket's visibility section
   names, and check the module does not define a second item with the same name.

## Per-slice protocol (with the line ratchet)

1. Record `src/main.rs` line count **before** the slice.
2. Move one slice; run `cargo fmt`.
3. `cargo clippy --all-targets --locked -- -D warnings` clean; `cargo test --locked` green.
4. Record the line count **after**. `main.rs` must shrink by the moved block; a slice that
   does not shrink `main.rs` is rejected. Put `main.rs: <before> -> <after>` in the commit.
5. Verifier reads the diff, checks for logic drift, dropped `#[cfg(test)]`, and that the
   boot scan still sees all 8 markers exactly once.
6. Commit the slice.

## Acceptance

- After phase 2, `main.rs` ≤ 200 lines; every new file has one clear responsibility.
- Test count does not drop. Record the baseline at slice 0; do not reuse a stale number.
- The boot-path scan still enumerates all 8 boot functions, each exactly once.
- `git log` shows one `refactor:` commit per slice, each independently revertable.
- Phase 1 stops after slice 6; T-02..T-06 then run in files under ~1,500 lines (inspector and
  toolbar may be larger).
- `cargo fmt --check`, clippy, and `cargo test --locked` are green at every slice.
