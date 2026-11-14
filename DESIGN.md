---
name: ManyProxy
description: Local-only proxy library for Windows — a railway signal-box instrument panel.
# Every role below names a gpui-kit `Theme` field. The values come from the
# signed lab skin, ported into `crates/ui/src/skin.rs` — that file is the only
# place that names a color. `crates/ui/src/appearance.rs` stamps the skin onto
# the theme and pins the palette claims in tests.
tokens:
  ink: foreground
  paper: background
  quiet-surface: group_box
  hover: list_hover
  hairline: border
  divider: table_row_border
  muted-ink: muted_foreground
  selection-wash: list_active
  selection-rim: list_active_border
  focus-ring: ring
  primary: primary
  signal-go: success
  signal-stop: danger
  signal-warn: warning
  info: info
  chrome-band: title_bar
  overlay-surface: popover
  scrollbar-thumb: scrollbar_thumb
typography:
  body:
    fontFamily: "system-ui, -apple-system, 'Segoe UI', sans-serif"
    fontSize: "1rem"
    lineHeight: 1.25
  small:
    fontFamily: "system-ui, -apple-system, 'Segoe UI', sans-serif"
    fontSize: "0.875rem"
    lineHeight: 1.25
  mono:
    fontFamily: "ui-monospace, Consolas, monospace"
    fontSize: "0.75rem"
rounded:
  md: "6px"
  full: "9999px"
---

# Design System: ManyProxy

## Overview

**Creative North Star: "The Signal Box"**

ManyProxy is a three-pane workbench: a scope-carrying nav on the left, the
library table in the middle, a stacked inspector on the right, and a fixed
engine bar along the bottom. Library rows are track circuits — a state glyph, a
nameplate, and the row's numbers on a paper ground. Color is signal-only and
rides the ported skin's theme tokens everywhere; the only literal color in app
code is the QR plate (black on white, `skin.rs::qr_plate`/`qr_module`), because
a scanner needs that exact pair.

Selection is one model: a set of row ids plus one anchor and one focus. There
is no checked set. A shortcut never appears inside a label or an accessible
name — the key rides in the tooltip and in Settings → Shortcuts.

**Key characteristics:**

- **Workbench shell.** Title bar (32/34) → nav (220, 56 collapsed) → action bar
  (44/40) → scope strip (32, sources only) → filter row (36/34) or the batch
  bar in the same slot → header (32/30) → rows (36/30 pitch) → console dock
  (240/220 when open) → engine bar (40/36) spanning main + inspector. The
  engine bar never moves; opening the console takes height from the table.
- **One selection grammar.** Click, Ctrl+click to toggle, Shift+click or
  Shift+Up/Down for a range, Ctrl+A for everything visible, Escape clears. The
  selection wash is the row's only highlight; one selected row shows its
  sections in the inspector, two or more show the aggregate, none shows the
  library summary.
- **Scopes in the nav.** `All configs` carries the library total; `Groups` is
  flat and alphabetical with `Ungrouped` last; `Sources` lists each
  subscription with a freshness dot and a per-source refresh; `Routing presets`
  and `Settings` stay pinned at the bottom. Below 1200px the nav collapses to
  its 56px letters.
- **Honest states.** Every action stays enabled and explains itself on press.
  Empty library (`No configs yet` + `Add or import a config`) is a different
  surface from no match (`No configs match` + `Clear filters`); `N of M` is
  always visible and reads `M of M` when nothing is filtered.

## Colors

The signed lab skin's Dark and Light modes are the two appearances; System
resolves to one of them. `crates/ui/src/skin.rs` names every value;
`apply_theme` stamps them onto the kit theme after `Theme::change` rebuilds it,
so Dark, Light, and System all restyle from one place. The monochrome ramps
invert between modes; the signal hues stay in their families — green serving,
red dead-or-destructive, amber advisory, blue selection.

- **Paper** ({tokens.paper}): app and list background. Dialogs and popovers sit
  one step off it ({tokens.overlay-surface}).
- **Ink** ({tokens.ink}): body text on paper. Muted ink ({tokens.muted-ink})
  carries row metadata, group labels, and table heads.
- **Hairline** ({tokens.hairline}): every 1px separator — shell bands, header
  base, selection rims, panel edges. Row dividers use the lighter
  {tokens.divider}.
- **Selection Wash + Rim** ({tokens.selection-wash} / {tokens.selection-rim}):
  the selected row's fill and its 1px rim — selection only.
- **Focus Ring** ({tokens.focus-ring}): the keyboard focus indicator; clears
  3:1 against paper in both modes (pinned by the contrast test).
- **Signal Go / Stop / Warn** ({tokens.signal-go} / {tokens.signal-stop} /
  {tokens.signal-warn}): serving, dead or destructive, advisory. As text on
  light paper they derive darker tones in `library/tone.rs`; hue carries the
  meaning, lightness belongs to the ground.
- **Primary** ({tokens.primary}): the Add and Start commitments and the active
  accent preset's stamped tokens (Settings → Appearance); health and error
  colors never follow the accent.

**Named rules.**

- **Signal-only.** Color marks primary actions, selection, and serving state —
  never decoration.
- **One token source.** No color literal in feature code. `skin.rs` names the
  palette; the QR plate pair is the named exception. The guard test and the
  contrast test in `appearance.rs`/the dialogs binary enforce both.

## Typography

Platform sans (system-ui, Segoe UI) plus the kit mono face for endpoint data.
Two text roles: body (1rem) for names, titles, and buttons; small (0.875rem)
for metadata, badges, filter controls, and the `Acting on:` echo. The type
scale is 14 / 13 / 12 inside table rows (row name / cells / group). The
Settings text-size slider scales the rem both roles resolve against (100–150%);
it never introduces a third role.

## Layout

```
Window (open 1200x700, minimum 1100x640)
├─ Title bar 32/34      app name · running count · native controls
├─ Row (flex_1)
│  ├─ Nav 220/56        All configs · Groups · Sources · utilities
│  └─ Column
│     ├─ Action bar     title + count · Import · Add · palette · ⋯
│     ├─ Scope strip    sources only: freshness, quota, Refresh, Manage
│     ├─ Filter row     Search · Status · Protocol · Filter · N of M · Clear
│     │  or batch bar   × · N selected · running · hidden · Start Stop Test
│     │                 Delete · More
│     ├─ Header 32/30   seven columns, one menu per header
│     ├─ Rows 36/30     virtualized, horizontal scroll shared with the header
│     └─ Console        Logs | Connections | Traffic, All | Selected
│     └─ Inspector 300  stacked scroll; follows the selection; collapsible
└─ Engine bar 40/36     engine · running · throughput · Start all Stop all
                        Console · ⋯ (Test all, Stop testing, Clear results)
```

- Seven production columns: Name, Group, Protocol, Upstream, Endpoint, Health,
  Traffic. Name and Endpoint are locked visible; every visible header sorts on
  click and opens one identical menu (Size to Fit, Size All, visibility,
  More…). The Columns sheet adds order buttons and the resets. Widths, order,
  and visibility persist as one tolerant `library_column_layout` value.
- Below 1200px the nav collapses to 56px; below 1080px the inspector overlays
  from the right and the table never auto-hides a column — the table scrolls
  sideways instead, header and rows sharing one offset.
- A data folder that refuses a write raises one standing banner between the
  action bar and the table; the table shrinks rather than being covered.
- High display scaling clamps the minimum height to the primary display's work
  area, so the engine bar never sits below the desktop.

### Named rules

- **No auto-hide.** Crowded columns scroll sideways with the header; no column
  disappears on its own.
- **Endpoints above the fold.** The inspector's reason is endpoint and health
  evidence; the stacked scroll opens on Identity and keeps the operate row
  reachable.
- **The engine bar never moves.** The console dock takes height from the table;
  the transport stays where it is.

## Row anatomy

One row is one config: the state glyph (a read-only signal, never a control),
the name (14px), Group (12px), Protocol + Upstream, the Endpoint chip
(`:port` on the default bind, `host:port` otherwise, IPv6 bracketed; copy on
click, chevron for SOCKS/HTTP/Both, hover pencil to edit), Health (tone text
plus sparkline caption), and Traffic. Hover reveals Start / Stop / More inline;
More carries Test, Edit config, Move to group, Copy URLs, Restore on launch.
Rows expose list-item semantics with the config name as the accessible name;
selection is exposed as state, not only painted. The density switch
(comfortable 36px / compact 30px) owns the pitch and every chrome height; at
1,000 rows the pitch is unchanged (pinned by the fixture tests).

## Interaction rules

- **One selection model.** Selection drives the inspector, the batch bar, and
  bulk verbs; a scope change clears it; deleting prunes it. No header checkbox,
  no second set, no "checked" anywhere.
- **Batch bar swaps, never moves.** With two or more selected, the batch bar
  replaces the filter row in the same slot: `× · N selected · N running ·
  N hidden (click to clear filters) · Start Stop Test Delete · More`.
- **One editor per surface.** The endpoint chip's pencil, the inspector's
  Edit, and `Ctrl+E` open the same draft; validation is live, collisions name
  the row, Apply on a running row restarts with rollback.
- **Destructive confirms name the object.** Bulk delete, prune, and group
  deletes confirm with names and counts; a single delete is immediate with
  Undo. Stopping N running endpoints on close asks with the names.
- **Keyboard covers the mouse.** `Ctrl+K` palette, `Ctrl+F` search, `Ctrl+N`
  add, `Ctrl+O` import, `Ctrl+E` endpoint, `Ctrl+Shift+E` config, `Ctrl+I`
  inspector, `` Ctrl+` `` console, `Ctrl+R` update scope, `Ctrl+.` stop
  testing, `F5`/`Shift+F5`/`Ctrl+T`, `Ctrl+U` copy SOCKS, `Delete` delete,
  `Ctrl+A` select visible, `Space` toggle, `Esc` clears layer by layer and
  returns focus to the table. Every icon-only control names itself to
  assistive tech and carries a tooltip; overlays trap and restore focus.

## Retired surfaces

The five-menu toolbar, the three-segment status strip, the facet band, the
checked set and its bulk bar, the row checkbox and occupancy lamp, the
five-tab inspector, `Ctrl+1..6`, the Row columns dialog, the standalone
latency picker, and the old 880×620 window minimum are gone. Their evidence
lives in the console and the stacked inspector; their verbs live in the nav,
the action bar, the header menu, the row More menu, and the engine bar.
