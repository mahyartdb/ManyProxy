# The library

The library is ManyProxy's home: every stored config, listed in one table.

## The window

- **Left nav.** `All configs` is the home view and shows the library total.
  Below it, `Groups` lists every group alphabetically with its count
  (`Ungrouped` last), and `Sources` lists your subscriptions with a freshness
  dot and a refresh arrow. `Routing presets` and `Settings` are pinned at the
  bottom. Clicking any entry narrows the library to that scope; `All configs`
  brings everything back.
- **Action bar.** The view title with its count, then `Import`, `Add`, the
  command palette (`Ctrl+K`), and a `⋯` overflow carrying Export, About, and
  `Check for updates`.
- **Filter row.** Search (`Ctrl+F`), Status, Protocol, and a Filter popover for
  latency, bind, port range and custom endpoints. The `N of M` readout is
  always visible: `1000 of 1000` with no filter, `10 of 1000` after one. The
  buttons show their active values and clear individually; `Clear` resets
  everything. Search and filters are session-only — a restart opens unfiltered.
- **Table.** Seven columns: Name, Group, Protocol, Upstream, Endpoint, Health,
  Traffic. Click a header to sort, click again to reverse; every header has a
  menu with sizing, per-column visibility, and more. Drag column edges to
  resize (double-click resets that column), drag headers to reorder. Columns
  wider than the window scroll sideways — nothing hides on its own. Your
  layout persists between sessions.
- **Inspector.** `Ctrl+I` or a row click opens the right-hand panel: identity,
  endpoint, health, and actions for the selected row. Select several rows and
  it shows what they share. Its width is adjustable and remembered.
- **Engine bar.** The bottom strip: engine state, running count, session
  traffic, `Start all`, `Stop all`, `Console`, and a `⋯` menu.

## Adding and importing

`Add` (`Ctrl+N`) opens a guided form — server, port, identity — with the JSON
editor behind `Advanced`. `Import` (`Ctrl+O`) reads share links, config files,
and subscription URLs; you can also paste links straight into the window
(`Ctrl+V`) or drop files onto it. New rows land in the library unstarted.

## Selecting and bulk actions

Rows select like a file list: click to select, `Ctrl+click` to add or remove,
`Shift+click` or `Shift+Up/Down` for a range, `Ctrl+A` for everything visible,
`Esc` to clear. With two or more rows selected the filter row becomes the batch
bar: `Start`, `Stop`, `Test`, `Delete`, and `Move to group` act on the whole
selection at once. `N hidden` counts selected rows the current filter hides —
click it to clear the filters.

## Deleting

One row deletes immediately with an `Undo` toast. Several rows ask first, and
the confirm names what it is about to delete.

## Groups and sources

Right-click a group in the nav to rename, manage, or delete it; drag a row onto
a group to move just that row. Inside a source's scope the strip above the
table shows its freshness, quota, and traffic, with `↻` to refresh that source.
`Ctrl+R` refreshes whatever scope you are in; `All configs` refuses with a
reason rather than guessing.
