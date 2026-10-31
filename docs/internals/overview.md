# Overview

ManyProxy is one Windows desktop process serving N stored proxy configs
through external engine binaries. Each row can bind its own local `ip:port`.
The UI is a library list, an inspector, and the dialogs around them.

## Boundaries

Two crates and a bin. `manyproxy-core` holds the logic and links no GUI at
all — `cargo tree -p manyproxy-core` finding `gpui` is the failure this split
exists to prevent. `manyproxy-ui` holds every element, `AppView`, and the
tests. The root package is a shell around `manyproxy_ui::run()`.

| Path | Owns | Does not |
|---|---|---|
| `crates/core/src/store.rs` | SQLite (the only `rusqlite` user), every row/subscription/settings write, the data dir | UI state, engine processes |
| `crates/core/src/secret.rs` | `Secret<String>` (zeroize on drop, redacted `Debug`), the `dpapi1:` seal/unseal marker | Deciding what is secret |
| `crates/core/src/dpapi.rs` | The raw DPAPI FFI pair | Formatting or storage policy |
| `crates/core/src/engine.rs` | Starting and stopping engine children, port ownership, the startup reap | Store writes, dialogs |
| `crates/core/src/health.rs` | Probes, outcomes, traffic deltas, logs | Scheduling |
| `crates/core/src/import.rs` | Share-link and subscription parsing into `NewConfig` | Opening dialogs |
| `crates/core/src/library/` | The pure row logic: rows, match, columns, metrics | Rendering, `ListDelegate` |
| `crates/ui/src/library/` | The list adapter: the delegate, row render, tone, shard, host picker | Store mutations |
| `crates/ui/src/app/` | `AppView`, dialogs, ticks, the wire between the modules above | Anything another module imports |
| `crates/ui/src/tests.rs` | Every windowed test | Production code |

Within `manyproxy-ui`, `app` may depend on everything below it and nothing
below `app` may import `app`. Actions (start, stop, delete, import) are
`AppView` methods even when the button lives in `library` or a dialog.
`manyproxy-ui` re-exports core (`pub use manyproxy_core::*`), so `crate::store`
and `crate::ProxyConfig` keep resolving from either side of the seam.

Anything UI tests need from core is gated `#[cfg(any(test, feature =
"test-support"))]`, and both crates declare `serde_json` with
`preserve_order` because the byte-identical JSON tests compare key order.

## Startup

`src/main.rs` installs the crash hook, then `crates/core/src/single_instance.rs`
either forwards this launch's argv to the live window and exits, or binds the
instance pipe and continues. The window is created inside `app.run`;
`AppView::new` opens the store and restores sessions before `Root::new`
wraps the view. Open-with payloads are classified once by
`classify_open_with` and handled by `AppView::handle_launch_intake`, which
defers its dialog until `Root` exists (`after_root`).

## Time and threads

The UI is single-threaded. The render path does no blocking I/O and does not
parse config params — the library caches upstream once per `set_rows`.
Engine polls, due subscriptions, reprobes and traffic all run on gpui timers
started once by `start_background_ticks`; tick bodies may block because they
are not the render path. Engine children own dedicated log-reader threads,
and the instance pipe has its own listener thread that hands argv to the UI
through a channel.

## Motion

One vocabulary, three shapes: a ≤160 ms ease-out entry, a ≤160 ms accent
settle on the row that starts or stops, and toasts for completed actions
(delete also carries Undo). All of it returns zero duration under
`cx.reduce_motion()`. Nothing loops. The kit owns dialog and sheet entry
(250 ms and 150 ms, `pub static ANIMATION_DURATION`) — an immutable kit
constant, so the app does not wrap another fade inside it.
