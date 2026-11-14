# Glossary

Code terms with a meaning more specific than their English one. Product-voice
terms (`you`, `me`, `user`, `project`) live in `AGENTS.md`; this page is for
spelunking the source.

- **row / `ProxyConfig`** — one stored config: protocol, `params` JSON,
  `bind_ip`, `port`, group, enabled. The library lists rows; the engine runs
  them. In user-facing copy a row is a **config**.
- **library** — the table of rows. `visible` is the filtered and sorted list of
  indices into `rows`; the library never writes the store.
- **selection** — the one selection model: a set of row ids plus one anchor and
  one focus. It drives the inspector, the batch bar, and every bulk verb.
  There is no second set; the retired checked set is gone.
- **scope** — the nav's filter for the library: `All configs`, a group,
  `Ungrouped`, or a source. Clearing the selection is part of a scope change.
- **inspector** — the right-hand panel for the selected row; one stacked
  scroll, no tabs.
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
  the state glyph and the health column.
- **traffic** — per-row rx/tx counters and the deltas between polls.
- **tick** — a gpui timer owned by `AppView` through `spawn_tick`.
- **engine bar** — the bottom transport: engine state, running count, session
  traffic, Start all / Stop all / Console, and the overflow menu.
- **console** — the dock above the engine bar: Logs / Connections / Traffic
  with All / Selected scopes; starts collapsed.
- **root** — the kit `Root` wrapper around `AppView`. Dialogs require it, so
  code that runs before it defers with `after_root`.
