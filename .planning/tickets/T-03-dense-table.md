# T-03 — Dense table + right inspector (Variant A)

Owner: implementer (UI). Depends on: T-02. Reference: `.planning/prototypes/variant-a.png`.
Design authority: `DESIGN.md` (update it in the same change where noted).

## The row bug, root cause

`List` measures one item at `AvailableSpace::MinContent` (gpui-component `list.rs:434`) and
applies that height to every row (`cache.rs:216`). The row at `src/main.rs:1979-1984` is
`.flex_wrap()`, so at min width it stacks into a column, and every row inherits ~104-120px
of height.

Fix: rows are a single line, non-wrapping, definite height. No `flex_wrap`. Text columns
truncate. Target 30px row pitch, so ~21 rows fit at 1032px.

## One column spec, used twice

The header (`render_library_column_headers`, `26530-26582`) and the row (`render_item`,
`1735-2119`) currently hardcode the same widths in two places and disagree. Introduce one
struct (e.g. `RowColumns { lead, name, group, protocol, upstream, endpoint, health, action }`)
with `fn widths(&self, show_group: bool, show_protocol: bool) -> ...`; both the header and the
row read it. Header labels must render at exactly the same x as their values.

Column order: `[lamp+check 28] [name flex] [group 100 opt] [protocol 110 opt] [upstream 180]
[endpoint 140] [health 150] [copy 96]`.

## Row contents

- Keep: occupancy lamp, checkbox, check chip, name (truncating, tooltip full route).
- Add: `Copy proxy` button (96px) copying `socks5://127.0.0.1:<port>` — reuse the existing
  `copy-socks` handler (`15018-15048`), do not invent a second clipboard path.
- Move `Start/Edit` and the bind/port editors OUT of the row into the inspector. A 30px row
  cannot hold five framed controls. The row keeps: check, name, metadata columns, copy.
  Update `DESIGN.md` §"Library Row" accordingly — this is a deliberate contract change.
- Port must remain visible at a glance: render `:10813` as quiet mono text inside the
  endpoint column, not an input.

## Inspector: bottom panel → right panel

`src/main.rs:28908-28919` builds `v_resizable(...)` (Axis::Vertical) with a 240px bottom
panel. Change to a horizontal split: list left, inspector right at 340px
(range 300–440, resizable, remembered like the current split).
- Empty state: one sentence, centered in the pane, with `Add configuration` / `Import
  configurations` — but only when the library itself is empty. When the library has rows and
  nothing is selected, show a single muted line: `Select a configuration`.
- Inspector must not force horizontal scrolling at 340px: the operate row wraps to two lines
  (DESIGN.md No-Sideways-Scroll rule).
- Keep all existing panes: Details / Health / Presets / Logs / Connections / Traffic.

## Chrome

- Nav (`28007-28859`): these are dropdown menus, not pages, so drop the fake tab styling.
  Render them as a flat menu bar (text items, hover tint, no borders), Program first. Remove
  the duplicate `Groups` button on the facet row; rename any remaining manage button to
  `Manage groups…`.
- One primary action per screen: `+ Add configuration` at top right, primary fill; `Import`
  secondary beside it.
- Search width capped at 380px; `N of M` beside it; the three facet pickers right-aligned on
  the same row.
- Bulk bar: count left (`56 of 59 selected`, singular-safe), then grouped actions with
  vertical separators: `[Check all visible] [Uncheck all] | [Start checked] [Stop checked] |
  [Enable] [Disable] [Move to group] | [Delete]`, Delete last and danger-tinted.
- Status bar: one grammar, `·` separators only, no unlabeled `:10801 :10802` pairs.

## Health string bug

`Running :10801` (stray space before colon, no host). Format must read
`Running · 127.0.0.1:10801 · 1078 ms` and must not be clipped by the neighbouring control.
Fix the format string at its source, and add a unit test pinning it.

## Acceptance

- 20+ rows visible at 1920×1032; row pitch measured from UIA frames ≤ 32px.
- Every header label sits within 8px of its column's first character.
- No `flex_wrap` on the row container; no row taller than 32px at minimum window width.
- Right inspector resizes 300–440px and remembers its width; list keeps ≥ 640px.
- Nothing in the row is a framed control except the checkbox and Copy.
- `cargo test --locked` green; existing containment tests still pass.
