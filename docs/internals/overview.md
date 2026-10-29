# Overview

ManyProxy is one Windows desktop process serving N stored proxy configs
through external engine binaries. Each row can bind its own local `ip:port`.
The UI is a library table, an inspector, and the dialogs around them.

## Boundaries

| Path | Owns | Does not |
|---|---|---|
| `src/store.rs` | SQLite (the only `rusqlite` user), every row/subscription/settings write, DPAPI sealing, the data dir | UI state, engine processes |
| `src/engine.rs` | Starting and stopping engine children, port ownership, the startup reap | Store writes, dialogs |
| `src/health.rs` | Probes, outcomes, traffic deltas, logs | Scheduling |
| `src/import.rs` | Share-link and subscription parsing into `NewConfig` | Opening dialogs |
| `src/library/` | The list adapter: rows, visibility, search, sort, row rendering | Store mutations |
| `src/app/` | `AppView`, dialogs, ticks, the wire between the modules above | Anything another module imports |
| `src/tests.rs` | Every test | Production code |

`app` may depend on everything below it; nothing below `app` may import
`app`. Actions (start, stop, delete, import) are `AppView` methods even when
the button lives in `library` or a dialog.

## Startup

`src/main.rs` installs the crash hook, then `src/single_instance.rs` either
forwards this launch's argv to the live window and exits, or binds the
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
