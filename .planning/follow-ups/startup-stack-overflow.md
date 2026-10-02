# Follow-up: main-thread stack overflow at startup (pre-existing)

Status: open · Source: qmode run `qmode-20261002-ui-complete-f095`, ticket T10 investigation · Out of scope for that run (frozen task list T01–T11; no engine/startup mandate).

## Symptom

`cargo run` (debug) dies instantly, no window ever appears:

```text
thread 'main' (19764) has overflowed its stack
error: process didn't exit successfully: `target\debug\manyproxy.exe` (exit code: 0xc00000fd, STATUS_STACK_OVERFLOW)
```

Windows Application log corroborates: `Faulting application name: manyproxy.exe`, exception `0xc00000fd`, fault in `manyproxy.exe` itself (records on 10/01 and 10/02).

## Proven pre-existing (not a UI-run regression)

- Baseline `b160bf5` (pre-run master) built in a clean worktree and run against a fresh `MANYPROXY_DB` overflows identically.
- Current master overflows identically with a fresh DB and with a seeded DB.
- Native crash, no Rust panic/backtrace — the overflow handler cannot unwind.

## Workaround (screenshots only, NOT committed)

```powershell
$env:RUSTFLAGS = "-C link-arg=/STACK:8388608"
```

With an 8 MB main-thread stack reserve the app launches, renders, and responds to input normally (T10's live screenshots were captured this way via cua-driver: 6-row library, Add/Import/Subscriptions dialogs). The flag is environment-only; no repo change carries it.

## Repro

1. `cargo run` (debug profile, default 1 MB main-thread stack).
2. Observe instant `STATUS_STACK_OVERFLOW`, exit code `0xc00000fd`.

## Fix acceptance (for whoever takes this)

- `cargo run` launches to a responding main window on a default toolchain with no `RUSTFLAGS` overrides.
- Root-cause the deep recursion or oversized startup frames (startup path: `main` → `application().run` → `AppView::new`/`with_store`, tray creation, first frame) rather than only raising the stack; if raising it, do it in build config with a comment citing this file.
- Re-verify the T11 live clause (`MainWindowTitle=ManyProxy`, `Responding=True`) without workarounds.
