# T-03 — Dense table + right inspector (Variant A)

Owner: implementer (UI). Depends on: T-02. Reference: `.planning/prototypes/variant-a.png`
(approximate: four tabs; this ticket keeps six). Update `DESIGN.md` in the same commit.
## Row bug, root cause
`List` measures one item at `AvailableSpace::MinContent` (gpui-component `list.rs:434`) and
applies that height to every row (`cache.rs:216`). The row at `src/main.rs:2000` is
`.flex_wrap()`, so at min width it stacks into a column and every row inherits ~104-120px.
Fix: single-line, non-wrapping, definite-height rows; text truncates; target 30px pitch.
## One geometry spec, used twice
Header (`26548-26660`) and row (`1753-2160`) hardcode widths twice and disagree. Add
`RowMetrics` (not `RowColumns` — visibility flags at `9469`) with
`widths(show_group, show_protocol)`; both read it. Header labels sit at the value's x.

Order: `[lamp+check 28] [name flex min 160] [group 100 opt] [protocol 90 opt]
[upstream 140] [endpoint 120] [health 110] [copy 72]`.
Hide protocol, then group, when the table falls under the 640px floor; name, endpoint,
health and copy never hide.
## Row contents
- Keep: lamp, checkbox, check chip, name (truncate, full-route tooltip). Port stays as the
  DESIGN.md "port pill": `row-port-<id>` is quiet mono text with a tooltip;
  `row-port-input-<id>` leaves.
- Add `Copy proxy` (72px) calling new `copy_row_socks(id, window, cx)`.
  `copy_single_endpoint_url` (`15019`) reads `self.selected_config(cx)` and takes no id, so
  reusing it would copy the wrong row. `copy_row_socks` reads by id and uses
  `store::proxy_urls(&config.bind_ip, config.port)` — stored bind host, not loopback.
- One SOCKS path: the inspector's SOCKS control calls `copy_row_socks(selected_id, …)`;
  `EndpointUrl::Socks` leaves `copy_single_endpoint_url` (HTTP/Upstream stay). Update
  `45390-45413`.
- Move `Start`/`Edit` and bind/port editors into the inspector operate row. A 30px row
  cannot hold five framed controls; `row-bind-*`, `row-port-input-*`, `row-serve-*`,
  `row-edit-*` leave the row.
## Inspector: bottom → right
`28908-28919` is `v_resizable("main-split")` with a 240px bottom panel. Make it horizontal:
list left, inspector right, default 260px, range 240–360, session-only (the current split
persists no width — do not claim it remembers).

Minimum stays 880×620 (`29117-29139`), default 1000×700. The table keeps ≥ 640px only when
the window is ≥ 1080px. **Decision:** below 1080px the inspector collapses to a slide-over
drawer (Esc closes it, focus returns to the list); the table is never squeezed under 640px.
Do not raise the minimum window width.

- Empty state: centered `Add configuration` / `Import configurations` only for an empty
  library; rows but no selection → `Select a configuration`.
- At 260px the operate row wraps; no horizontal scroll (No-Sideways-Scroll rule).
- Keep six panes: Details / Health / Presets / Logs / Connections / Traffic. The four-tab
  prototype is approximate; T-02 and DESIGN.md:212 pin Ctrl-1..6.
## Chrome
- Nav (`28007-28859`) is dropdown menus, not pages: flat menu bar, Program first. Remove the
  duplicate `Groups` button on the facet row; rename the survivor to `Manage groups…`.
- `+ Add configuration` top right, primary; `Import` secondary beside it.
- Search keeps its `flex_1`/`min_w(140)` behaviour (no new fixed cap unless measured);
  `N of M` beside it; facet pickers right-aligned on that row.
- Bulk bar: `N checked (of M visible)` — never "selected"; selected is one row (wash),
  checked is a set (rim + numeral chip). Then `[Check all visible] [Uncheck all] |
  [Start checked] [Stop checked] | [Enable] [Disable] [Move to group] | [Delete]`.
- Status bar: one grammar, `·` separators only, no unlabeled `:10801 :10802` pairs.
## Health string bug
`serving_badge(is_running, port, health)` (`9252`) has no bind host, so it only prints
`Running :10801`. Change to `serving_badge(is_running, bind: &str, port, health)` and format
`Running · 127.0.0.1:10801 · 87ms` (the summary already carries `ms`). Update calls `9767`,
`9790`, `25430`, `25432` and the tests pinning the old string: `48099-48107`, `48155`,
`55866`. The 110px row cell truncates with a full tooltip; the inspector shows it unclipped.
## Corrections (reviewer)
- 340px inspector was impossible at the 880px minimum → 260px (240–360), drawer below 1080.
- "Reuse the copy-socks handler" was wrong: it copies the selected row; add
  `copy_row_socks(id, …)`.
- Bulk label stays `N checked (of M visible)`; keep six panes (the four-tab prototype is
  approximate) and keep the port in the row as quiet `row-port-<id>` text.
## Tests to update, not delete
- `r3_row_endpoint_controls_paint` (`48253-48313`) pins `row-bind-`, `row-port-`,
  `row-serve-`, `row-edit-` in the row, widths 118/76 and `Start "Alpha" (F5)`. Rewrite:
  `row-port-<id>` is the pill; bind/serve/edit are absent; assertions move to the inspector.
  T-02 already changed the name; this ticket moves the control.
- `r3_row_serve_button_starts_that_row` (`48320`), escape tests (`48535`, `48621`),
  bind-picker tests (`58486`, `58654`) follow the moved editors.
- Bulk-label pins: `t44_checked_set_honest_label_and_prune`,
  `t01_checked_positions_and_selection_stay_distinct`, `t44_ui_bars_and_row_checks_paint`,
  `t01_selection_grammar_paints`, plus the bulk-bar/reduce-motion/keyboard tests at
  `45282`, `45915`, `55345`.
- `serving_badge_copy` and the canary test at `55866`.
## DESIGN.md, same commit
"Layout" (vertical split, 240-480 panel), "Library Row", "Endpoints module", "The
Endpoints-Above-Fold Rule", "Chips" (port pill), "Row editors" (`205`), "Inspector tabs"
(`212`), and the Do bullets pinning row editors.
## Acceptance
- 20+ rows at 1920×1032; UIA-measured row pitch ≤ 32px.
- Header labels within 8px of their column's first character.
- No `flex_wrap` on the row; no row taller than 32px at the 880px minimum.
- Inspector 260 default, 240–360; drawer below 1080px; table ≥ 640px at 1080px+.
- Row framed controls: checkbox and Copy only.
- `cargo test --locked` green; updated containment and bulk-label tests pass.
