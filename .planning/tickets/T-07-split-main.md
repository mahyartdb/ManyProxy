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

Slice 0 widens every `AppView` method, field, and free helper the tests reference to
`pub(crate)`, without moving anything. Later slices are then pure text moves. At the crate
root add `use <module>::*;` per module; a private `use` at the crate root is visible to
descendants, so the 868 `crate::X` paths keep resolving. Add explicit imports for the two
cfg-split duplicate names (`open_url_shell_execute`, `win32_process_sample`).

## Phase 1 — UI-facing slices, then stop

| # | Cluster moved | New file(s) | Notes |
|---|---|---|---|
| 0 | visibility only | none | 369 `AppView` methods + test-reached fields/helpers → `pub(crate)` |
| 1 | consts, keys, appearance, text | `consts.rs` `keys.rs` `appearance.rs` `text.rs` | ~150 items |
| 2 | `mod tests` verbatim | `tests.rs` | ~29.6k lines; retarget `include_str!` / `file!()` |
| 3 | library list + columns + match | `library/mod.rs` `library/columns.rs` `library/match.rs` | move `RowMetrics` with both impls |
| 4 | shell + toolbar + render | `app/shell.rs` `app/toolbar.rs` `app/render.rs` | |
| 5 | detail/inspector + status + search/bulk | `app/detail.rs` `app/status.rs` `app/search.rs` `app/bulk.rs` | |
| 6 | settings | `app/settings.rs` | |

Stop after slice 6. T-02..T-06 run here, inside these files.

## Phase 2 — after the UI work

Forms, fetch/update, subs, presets, import, health, logs, io, ticks, models, dialog kit,
`win32_process`. The UI work does not touch these, so they split afterwards without conflict.

## Traps (verified — do not skip)

1. **Boot-path source scan.** `include_str!("main.rs")` ×2 (`52547`, `52562`);
   `direct_overlay_calls` asserts each marker matches **exactly once** (`52625-52640`).
   Moving a boot function makes it fail loudly — that is correct. In the same slice,
   re-point the scan at the new file(s) and keep the exact-once assertion across the scanned
   set. Never weaken, skip, or delete the guard. `BOOT_FUNCTIONS` names `new`, `with_store`,
   `create_tray`, `restore_on_launch`, `commit_restore_subset`, `open_restore_lan_confirm`,
   `handle_launch_intake`, `reload`.
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
