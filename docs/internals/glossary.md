# Glossary

Code terms with a meaning more specific than their English one. Product-voice
terms (`you`, `me`, `user`, `project`) live in `AGENTS.md`; this page is for
spelunking the source.

- **row / `ProxyConfig`** — one stored config: protocol, `params` JSON,
  `bind_ip`, `port`, group, enabled. The library lists rows; the engine runs
  them.
- **library** — the table of rows. `visible` is the filtered and sorted list of
  indices into `rows`; the library never writes the store.
- **selection vs checked** — selection drives the inspector. The checked set is
  the bulk-action target. They are deliberately independent.
- **inspector** — the right-hand panel for the selected row.
- **serve / start / running** — an engine process bound to a row's endpoint.
  Running is the set of row ids whose process is alive.
- **engine** — an external proxy binary (xray, sing-box, ...) started by
  `EngineController`; the app never links one.
- **intake** — the open-with argv of a launch (`OpenWithKind::BareHttp` or
  `Batch`), handled by `AppView::handle_launch_intake`.
- **preset** — a bundled multi-config template or rule set under
  `crates/core/src/preset/`.
- **subscription** — a remote config source that refreshes into rows.
- **probe / health** — one test outcome for a row (kind and latency) shown by
  the status lamp and the health column.
- **traffic** — per-row rx/tx counters and the deltas between polls.
- **tick** — a gpui timer owned by `AppView` through `spawn_tick`.
- **status strip** — the bottom truth line: counts, engine and health state,
  summed traffic.
- **root** — the kit `Root` wrapper around `AppView`. Dialogs require it, so
  code that runs before it defers with `after_root`.
