# T-07 — Split `src/main.rs` into modules (pure moves)

Baseline: HEAD `66e33ff`. `src/main.rs` = 58,657 lines / 2.26 MB.
Target: `main.rs` ≈ 150 lines (inner attrs, module decls, root `use m::*;` seams, `fn main`,
crash hook) across ~39 files, max depth 2 under `src/`.

## Non-negotiable discipline

1. **Pure moves.** No renames, no cleanup, no behavior edits, no "while I'm here" fixes.
   The only permitted edits are: item visibility (`pub(crate)`), `use` headers, and moving
   `mod` declarations.
2. **Each slice compiles and passes `cargo test --locked` on its own.** A slice that cannot
   get green is reverted, not patched forward.
3. **One commit per slice**, `refactor: split <cluster> out of main.rs` + one line of why.
4. **Prove it's a move.** `git diff --color-moved=zebra --stat` reviewed per slice; any
   hunk that changes logic is a defect.
5. Do not touch the working tree's in-flight changes from T-01; T-01 commits first.

## Visibility strategy (the cost lever)

In each new module mark exported items `pub(crate)`; add `use <module>::*;` at the crate
root for each. A private `use` in the crate root is visible to all descendants, so the 915
existing `crate::X` paths keep resolving with no edits. Explicit imports for the two
cfg-split duplicate names (`open_url_shell_execute`, `win32_process_sample`) to avoid glob
ambiguity.

## Slices (in order; S = safest first)

| # | Cluster moved | New file(s) | Notes |
|---|---|---|---|
| 1 | consts, keys, appearance, pure text/message helpers | `consts.rs` `keys.rs` `appearance.rs` `text.rs` | ~150 items to `pub(crate)` |
| 2 | `mod tests` verbatim | `tests.rs` | 29,617 lines, 3 edits; halves the file |
| 3 | leaf types, facets, models, `win32_process` | `facets.rs` `models.rs` `win32_process.rs` | `super::` → `crate::models::` |
| 4 | dialog/sheet/QR kit | `dialog_kit.rs` | |
| 5 | `ConfigForm` + panel planning | `config_form.rs` `config_form_panels.rs` | largest single move (5.8k) |
| 6 | row-columns + match/validate | `library/columns.rs` `library/match.rs` | move `RowColumns` with both impls |
| 7 | forms/widgets | `import_form.rs` `subs_ui.rs` `preset_ui.rs` | big import blocks |
| 8 | fetch + update/semver | `fetch.rs` `update.rs` | keep both `#[cfg]` variants |
| 9 | library list + `ListDelegate` impls | `library/mod.rs` | move trait impls whole |
| 10 | bootstrap + render | `app/mod.rs` `app/render.rs` | `AppView` methods → `pub(crate)` |
| 11 | rows, engine, window/tray | `app/rows.rs` `app/engine.rs` `app/window.rs` | |
| 12 | settings, stats/palette, health, logs, io, dialogs | 6 files under `app/` | |
| 13 | import, subs, presets, rule sets | `app/import.rs` `app/subs.rs` `app/presets.rs` | move refresh-batch as one block |
| 14 | ticks, detail/inspector, bulk, search, status | 5 files under `app/` | last, so nothing dangles |

Estimated total ≈ 1,480 agent-minutes (~25 h). Slices 10–14 are ~60% of the cost.

## Traps (verified — do not skip)

1. **`include_str!("main.rs")` ×2** (`main.rs:52491`, `:52506`) is a source-scan pinning that
   boot paths never push overlays before `Root`. `BOOT_FUNCTIONS` names `new`, `with_store`,
   `create_tray`, `restore_on_launch`, `commit_restore_subset`, `open_restore_lan_confirm`,
   `handle_launch_intake`, `reload` — after the split they live in 5 files and the scan
   silently passes having found nothing. **The scan must become multi-file in the same slice**
   that moves them, and must fail if it finds fewer than the expected symbols.
2. **`std::fs::read_to_string(file!())`** (`:29191`) asserts the source contains
   `const APP_VERSION` + `env!("CARGO_PKG_VERSION")`. Moving tests breaks it: retarget to
   `consts.rs` and keep that const on one line.
3. **`include_str!("../tests/fixtures/uia-*.json")`** resolves relative to the source file, so
   test modules stay flat: `src/tests.rs`, not `src/tests/mod.rs`.
4. **`#[cfg(test)]` methods inside `impl AppView`** (10 of them) and free fns (6) lose their
   attribute on a bad move; nothing fails except clippy `dead_code` under `-D warnings`.
5. **Inner attributes** `#![recursion_limit]` and `#![cfg_attr(... windows_subsystem ...)]`
   stay the first lines of `main.rs`.
6. **`use gpui_kit::*;` is file-scoped.** Every new module needs its own header; stray unused
   imports are hard errors under `-D warnings`.
7. **`static LOCK`** (`:56374`) serialises the only test that mutates a process-global env var;
   keep that test and its function in the same file.
8. `mod win32_process` uses `super::{ProcessSample, …}`; after extraction, `crate::models::`.

## Per-slice protocol

1. Implementer moves one slice, runs `cargo fmt`.
2. `cargo clippy --all-targets --locked -- -D warnings` → must be clean.
3. `cargo test --locked` → must be green; test count must not drop (146 baseline).
4. Verifier reads the diff, checks for logic drift and dropped `#[cfg(test)]`, confirms the
   trap-1 scan still sees all boot functions, and confirms file counts.
5. Commit the slice.

## Acceptance

- `main.rs` ≤ 200 lines; every new file has a single clear responsibility.
- Test count unchanged; the boot-path source scan still enumerates all 8 boot functions
  across files.
- `git log` shows one `refactor:` commit per slice, each independently revertable.
- Later UI tickets (T-02…T-06) can be executed entirely inside files under ~1,500 lines,
  except the inspector and toolbar.
