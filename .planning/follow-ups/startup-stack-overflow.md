# Follow-up: main-thread stack overflow at startup — FIXED

Status: **fixed** · Source: qmode run `qmode-20261002-ui-complete-f095`, ticket T10 investigation · Filed `cc9c42b`, fixed on master.

## Original symptom

`cargo run` (debug) died instantly, no window ever appeared:

```text
thread 'main' (19764) has overflowed its stack
error: process didn't exit successfully: `target\debug\manyproxy.exe` (exit code: 0xc00000fd, STATUS_STACK_OVERFLOW)
```

Proven pre-existing: baseline `b160bf5` overflowed identically.

## Root cause

Debug builds do not optimize, so each function's stack frame keeps every
temporary of its builder chains. `Render::render` built the whole window tree
in one frame — `render_detail` alone was a ~450-line chain (GroupBox/
DescriptionList/Alert sections), with `render_toolbar`, `render_shell`, and the
list/detail children nested inside the same frame. First paint therefore
needed more than the 1 MB main-thread stack Windows gives a debug process.
Release builds inlined and optimized the temporaries away, which is why only
debug `cargo run` overflowed.

## Fix

Split the oversized builders into small per-frame functions:

- `render_detail` → dispatches to `render_detail_empty` / `render_detail_selected`,
  each section (`render_detail_header`, `_endpoints`, `_health`, `_usage`,
  `_history`, `_actions`) its own frame (`src/main.rs`).
- `render_shell`, `render_toolbar` extracted from `Render::render` the same way.

Content unchanged: same ids, strings, handlers, and the 240px detail scroll
cap. A comment in `render_detail` cites this file.

## Verification

- `cargo run` (debug, `RUSTFLAGS` **empty**) launches to a responding main
  window (`MainWindowTitle=ManyProxy`, `Responding=True`) — checked repeatedly
  on the fixed build during live screenshot capture.
- `cargo test`: 670 passed, 0 failed; `cargo fmt --check` and
  `cargo clippy --all-targets` clean.
- The `/STACK:8MB` workaround is no longer needed and is not used.

## Acceptance (all met)

- [x] `cargo run` launches on a default toolchain with no `RUSTFLAGS` overrides.
- [x] Root cause diagnosed (oversized debug frames, not a deep recursion) and
      fixed at the source rather than by raising the stack.
- [x] T11 live clause re-verified without workarounds.
