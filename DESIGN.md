---
name: ManyProxy
description: Local-only proxy library for Windows — a railway signal-box instrument panel.
colors:
  dark:
    primary-ink: "#fafafa"
    primary-ink-foreground: "#171717"
    primary-ink-hover: "#e5e5e5"
    paper: "#101010"
    ink: "#fafafa"
    quiet-surface: "#1e1e1e"
    hairline: "#333333"
    muted-ink: "#a3a3a3"
    selection-wash: "#bfdbfe33"
    selection-rim: "#60a5fa"
    signal-go: "#22c55e"
    signal-stop: "#ef4444"
    chrome-band: "#171717"
  light:
    primary-ink: "#171717"
    primary-ink-foreground: "#fafafa"
    primary-ink-hover: "#262626"
    paper: "#ffffff"
    ink: "#0a0a0a"
    quiet-surface: "#f5f5f5"
    hairline: "#e5e5e5"
    muted-ink: "#737373"
    selection-wash: "#bfdbfe33"
    selection-rim: "#60a5fa"
    signal-go: "#22c55e"
    signal-stop: "#ef4444"
    chrome-band: "#F8F8F8"
typography:
  body:
    fontFamily: "system-ui, -apple-system, 'Segoe UI', sans-serif"
    fontSize: "1rem"
    lineHeight: 1.25
  small:
    fontFamily: "system-ui, -apple-system, 'Segoe UI', sans-serif"
    fontSize: "0.875rem"
    lineHeight: 1.25
rounded:
  md: "6px"
  full: "9999px"
spacing:
  sm: "4px"
  md: "8px"
components:
  button-primary:
    backgroundColor: "{colors.primary-ink}"
    textColor: "{colors.primary-ink-foreground}"
    rounded: "{rounded.md}"
  button-primary-hover:
    backgroundColor: "{colors.primary-ink-hover}"
  button-danger:
    backgroundColor: "{colors.signal-stop}"
    textColor: "{colors.paper}"
  row-card:
    backgroundColor: "{colors.quiet-surface}"
    rounded: "{rounded.md}"
  row-endpoint-field:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink}"
    rounded: "{rounded.md}"
  row-selected:
    backgroundColor: "{colors.selection-wash}"
  row-checked:
    borderColor: "{colors.selection-rim}"
---

# Design System: ManyProxy

## Overview

**Creative North Star: "The Signal Box"**

ManyProxy looks like the instrument it is: a railway signal-box panel for a personal proxy pool. Library rows are track circuits — an occupancy lamp, a nameplate, and the row's numbers on a paper-white ground. The toolbar is a menu bar with one labeled master throw (Start-all / Stop-all) at the trailing end. The detail pane is a block shelf of ruled instruments (endpoints, health, usage, history), never stacked toolbars. Dialogs are ruled train orders: guided first, JSON only as Advanced.

Color is signal-only and rides gpui-kit theme tokens everywhere — app code carries zero hardcoded colors. Selection is a wash plus rim on one row only; the checked bulk set is a 2px accent rim plus a numeral count chip. Green means serving, red means dead or destructive, blue means selected-or-checked, black means primary action. Everything else is ink, paper, gray, or hairline.

**Key Characteristics:**
- Lamp-first rows: every row opens with an occupancy lamp in a fixed leading cell (green serving, red dead-while-serving, dark muted when stopped; flat fill, never glow).
- One selection grammar: wash marks the selected row and nothing else; checked rows carry rim plus count chip; the bulk bar always reads `N checked (of M visible)`; the detail header echoes `Acting on: <name>`.
- Menu-bar toolbar: five menus (Program, Settings, Groups, Routing, Tools) plus a labeled master throw (Start-all / Stop-all) at the trailing end; selection ops keep their key bindings and palette twins.
- Block-shelf detail: endpoints on every pane, one operate row (Auto-start switch + six inline ops + Row menu), manage actions behind the menu.
- Guided-first dialogs: Add opens on a Server / Server-port / Identity mini-form; JSON is an Advanced disclosure; validation keeps the dialog open with inline errors.
- Honest states: every action stays enabled with an honest toast (no disabled controls); loading shows label plus spinner; empty states teach the next step.

## Colors

Ink, paper, signal: a monochrome working surface where color is information, never ornament. Every value below rides the kit theme — `group_box`, `border`, `radius`, `accent`, `list_hover`, `list_active`, `success`, `danger`, `secondary`, `muted_foreground`, `description_list_label_foreground` — so a future theme propagates without touching app code.

**Dual palettes.** `{colors.X}` anywhere in this file means the active palette's `X` (`colors.dark.X` when Appearance is Dark, `colors.light.X` when Light). Dark is the default; System follows the Windows app-theme flag (`AppsUseLightTheme`) and resolves to one of the two. The monochrome ramps invert between palettes while the signal hues stay byte-identical — green is serving, red is dead-or-destructive, blue is selected-or-checked in both.

### Primary

- **Switchboard Ink** ({colors.primary-ink}): primary actions (Add, Start) and primary text on light surfaces (near-black on light, near-white on dark). Hover deepens to ({colors.primary-ink-hover}).
- **Paper** ({colors.paper}): app background, list background, dialog and popover surfaces (white on light, near-black on dark).

### Neutral

- **Working Ink** ({colors.ink}): body foreground on paper (near-black on light, near-white on dark).
- **Quiet Surface** ({colors.quiet-surface}): row cards and lever-frame plate (`group_box`), list hover ground (`list_hover`), tab bar background.
- **Hairline** ({colors.hairline}): every 1px separator (`border`) — toolbar base, facet-bar base, card rims, plate rim, title/status bar edges.
- **Muted Ink** ({colors.muted-ink}): secondary text (`muted_foreground`, `description_list_label_foreground`) — row metadata, stopped badges, endpoint labels, search context, the `Acting on:` echo.
- **Chrome Band** ({colors.chrome-band}): title bar and status bar bands, edged with hairlines.

### Selection and signal

Signal meanings are identical in both palettes: the wash and rim stay blue, serving stays green, dead-or-destructive stays red, stopped stays gray. Only the monochrome ground inverts.

- **Selection Wash** ({colors.selection-wash}): the selected row's card fill (`list_active` when the theme highlights, else `accent`) — selection only, never the checked set.
- **Checked Rim** ({colors.selection-rim}): a 2px accent border on checked rows plus a numeral count chip (`row-check-chip-<id>`, 1-based position in the checked set) — shape and text, never hue alone.
- **Serving Green** ({colors.signal-go}): running lamp, running badge text, healthy state (`success`).
- **Alarm Red** ({colors.signal-stop}): dead-row lamp and badge text, failure alerts (`danger`), Delete confirms.
- **Stopped Gray** ({colors.muted-ink}): dark lamp and `○ Stopped` badge on stopped rows (`secondary` on the detail Badge, `muted_foreground` in the list).

### Named Rules

**The Signal-Only Rule.** Color marks primary actions, current selection, the checked set, and serving state — never decoration. If an element is neither actionable-primary, selected, checked, nor state-bearing, it is ink, paper, gray, or hairline.

**The Theme-Token Rule.** No hardcoded colors in app UI code — every surface, rim, and state color resolves through kit theme tokens. Restyle by re-pointing tokens, never by inventing fills.

## Typography

**Body Font:** platform sans (system-ui, Segoe UI on Windows) with system fallback. No custom faces are loaded anywhere in the app.
**Label/Mono Font:** none distinct — the same sans carries labels, buttons, and data.

**Character:** one family doing all jobs. Names read at base size with label weight; everything subordinate drops one step to small. Line height stays tight (1.25) so three-line rows stay compact.

### Hierarchy

- **Body** (base, 1rem, 1.25 line-height): row names, dialog titles, button labels, endpoint values.
- **Small** (0.875rem, 1.25 line-height): row metadata, serving badges, check chips, hints, status-bar segments, filter controls, `Acting on:` echo.
- No display, headline, or title roles exist — this system tops out at body.

### Named Rules

**The Two-Size Rule.** Names at base, metadata and status at small. No third size and no display faces in UI; emphasis comes from weight or status color.

## Layout

A fixed top band (title bar, lever-frame toolbar), a vertical split with the card list on top (`flex_1`) and the inspector below (filling its resizable panel: 240px start, 240–480px range, internal scroll only past the panel height), and a fixed status strip with three regions — counts left (`status-left`), engine + health center (`status-center`), usage totals right (`status-right`) — split by separators. Filters stack as thin bands above the list: search-with-count plus a visible search-focus button, facet pickers, then the bulk bar only while rows are checked. Spacing runs on two steps throughout: tight 4px groups ({spacing.sm}: card text padding, header gaps, dialog field stacks) separated by generous 8px rhythms ({spacing.md}: toolbar padding, section gaps, dialog bodies).

The detail pane opens with the tab bar pinned above a scrolling body, then packs modules above one operate row: every pane carries the header plus Endpoints, then Details adds Usage, Health adds health + History, Presets adds the preset line + attach row, Logs shows the session log tail, Connections lists live sessions, Traffic charts totals — then a single `detail-operate` row (Auto-start switch + Start/Stop/Test/Ping/Logs/Copy URLs + `row-menu` trigger) under its own hairline. The switch writes the stored flag only (Start-all picks it up; Start/Stop control serving now). Eight small controls fit the 1000px frame in that one row, so endpoints stay above the fold with no horizontal scroll; at narrower widths the row wraps to a second line (never sideways scroll). The detail header names the row once via `Acting on: <name>` plus protocol/group Tags, bind endpoint, and serving Badge.

The window opens at 1000x700 with an 880x620 minimum; toolbar, detail, dialogs, and every status segment paint inside the minimum frame (pinned by containment tests).

### Named Rules

**The No-Sideways-Scroll Rule.** Crowded action sets wrap to a second row or collapse behind a menu; panes scroll vertically only. Horizontal scrolling is never the answer.

**The Endpoints-Above-Fold Rule.** The inspector's reason is its endpoints and health evidence — modules render above the operate row, and the 240px minimum keeps failure evidence painted without resizing.

## Elevation & Depth

Flat, always, with hairlines doing the separation work: 1px borders in hairline color under the toolbar, under filter bands, around row cards, around the lever-frame plate, and between list and inspector. Tonal bands (chrome title/status bars, quiet-surface cards and plate on paper) supply the rest of the depth. No shadows exist in app code. Dialog and menu elevation belongs to the kit chrome, not to this system.

### Named Rules

**The Flat-By-Default Rule.** Surfaces are flat at rest. Separation comes from hairlines and tonal bands; shadows appear nowhere in app code and must not be introduced for decoration.

## Shapes

One radius everywhere it matters: the theme radius (6px, {rounded.md}) on row cards, buttons, inputs, textareas, dialogs, and group boxes including the lever-frame plate. Fully round ({rounded.full}) is reserved for tiny elements only — the occupancy lamp, Kbd shortcut chips, check chips. Checked rows take a 2px accent border (border weight, not a second radius). Borders are 1px hairlines; a bordered card never also carries a shadow.

### Named Rules

**The One-Radius Rule.** Theme radius on cards, fields, buttons, and plates; pills only for lamps, key chips, check chips, and small controls. A second corner language would read as a second product.

## Components

Restrained and consistent: one button voice, one field voice, one chip voice per state, with tooltips naming every action and inline spinners for in-flight work.

### Buttons

- **Shape:** theme-radius corners ({rounded.md}).
- **Primary:** switchboard-ink fill ({colors.primary-ink}) with contrasting text ({colors.primary-ink-foreground}); hover deepens ({colors.primary-ink-hover}). Used for Add and Start — the two constructive commitments.
- **Hover / Focus:** kit hover tints plus visible keyboard focus; shortcuts ride as Kbd chips beside labels (Add, Start, Stop, Test, Import, Search) with the full `Name (Key)` string on the accessible name. Destructive commits (remove, bulk delete, clear-checked) return focus to the shell, since the control they acted from unmounts. The inspector header carries the status role so selection changes announce.
- **Danger:** alarm-red fill ({colors.signal-stop}) with paper text (destructive confirms; the row-menu Delete gates through a naming confirm instead of carrying danger styling).
- **Small density:** `.small()` variant across toolbar, facet, bulk, detail operate row, and endpoint rows so the full control set fits without scrolling (the five menus plus the labeled master throw ride small so the strip fits the 880px minimum on one row).
- **Loading:** `Starting…` label plus inline spinner (`engine-start-spinner`) while a start is in flight; probe buttons spinner while testing or pinging. Tooltips name every action.
- **Enabled-with-honest-toast:** no control renders kit `.disabled()` — every action stays enabled and explains itself on press (`Nothing to start — the library is empty`, `"<name>" is already stopped`). Loading covers in-flight inertness.

### Menu bar (toolbar)

Five menus in fixed order — Program (`program-menu`: intake + lifecycle), Settings (`settings-menu`), Groups (`groups-menu`: subscriptions + group ops), Routing (`routing-menu`: presets + settings link), Tools (`tools-menu`: selection ops, diagnostics, palette) — plus a trailing labeled master throw (`global-start-stop`: `▶ Start all` / `■ Stop all`, `… Connecting…` while a start is in flight). Short visible labels with full `Name (Key)` names on tooltips, accessible labels, and Kbd chips on bound rows. Every key-bound action plus every menu-only destination also lives in the `Ctrl-K` palette: Library group (15 ops) and Manage group (9 ops) dispatched through the same handlers as buttons, menus, and keys.

### Chips

- **Style:** Tag chips for protocol and group in the detail header; Kbd chips for shortcut hints; Badge with dot for the serving line (green serving, red failed, gray stopped); numeral check chips (`row-check-chip-<id>`) for checked-set position; port pills (`row-port-<id>`) pinning each row's `:port` at the trailing end.
- **State:** chips are read-only labels except Kbd hints, which are part of their buttons. Exclusive choices ride kit `Radio` with real checked state — the subscription interval picker, the log-level row, and the Settings Appearance picker — never bracketed (`[3600]`) or prefixed (`● Error`) text conventions.

### Cards / Containers

- **Corner Style:** theme radius ({rounded.md}).
- **Background:** quiet surface (`group_box`, {colors.quiet-surface}) on paper; selected rows take the selection wash ({colors.selection-wash}) with its rim so rim and card read as one highlight; checked rows take the 2px accent rim.
- **Shadow Strategy:** none — see Elevation & Depth.
- **Border:** 1px hairline ({colors.hairline}); 2px accent only for checked rows.
- **Internal Padding:** 8px horizontal, 4px vertical ({spacing.md} / {spacing.sm}) on row cards; 8px ({spacing.md}) pane padding in toolbar, inspector, and dialogs.

### Inputs / Fields

- **Style:** hairline stroke, paper fill, theme radius; stacked label-over-field at 4px gaps. Params box fixed at 96px, import box at 160px. The Port field carries a live auto-port budget readout (`port-budget`: used of 499 across `10801–10999` plus spill `18081–18380`) on Add and Edit.
- **Row editors:** the library's bind/port fields are small kit inputs at fixed widths (bind 118px, port 76px) in the system face — the same hairline/paper/radius voice as dialog fields, with the kit focus ring. Placeholders read `127.0.0.1` / `auto`; blank port still means auto-assign.
- **Guided-first:** the Add dialog opens on Server / Server-port / Identity mini-fields with a one-clause hint (`vless needs uuid.`); JSON is an Advanced disclosure toggled from the hint row (collapsed) or the Params header (expanded). Guided and JSON swap, never stack; expanding merges guided values into JSON, collapsing echoes JSON into blank guided inputs only. Edit opens expanded to preserve existing JSON.
- **Focus:** kit focus ring; validation keeps dialogs open with an inline error notification rather than closing; params errors auto-expand Advanced.
- **Error / Disabled:** inline error lines and collision warnings inside the form; `lan-hint` names the loopback guard in plain words (`Leaving loopback exposes this machine — …`); disabled rows read `disabled` in their muted meta line. No disabled-control styling exists — see honest-toast above.

### Navigation

- **Inspector tabs:** small segmented six-tab bar (Details / Health / Presets / Logs / Connections / Traffic), pinned above the scrolling pane body, with stable test anchors and aria labels; only one pane renders per frame, so shared ids never collide. Endpoints render on the overview above every pane; the operate row closes every pane under its own hairline.
- **Status bar:** three regions — library counts (`N configurations · M enabled`, singular at 1; honest zero-totals on empty, never a lying zero, absent on a broken read), engine + health (`health: never tested` short form; `N live · M dead` once probed), usage totals (absent when unreadable).
- **Search and bulk:** `N of M` search count with a Clear-search reset; a visible `search-focus` button in the facet bar lands focus in the query input (no new binding); the bulk bar always reads `N checked (of M visible)` with a wash-vs-rim legend on its tooltip, and its verbs name their scope (`Check all visible`, `Uncheck all`, `Start checked`, `Stop checked`).

### Library Row (signature component)

Occupancy lamp (`row-dot-<id>`, fixed leading cell, flat fill) plus checkbox plus name on line one, with the numeral check chip beside the name while checked; the name (`row-name-<id>`) and meta line (`row-meta-<id>`) truncate with full-text hover tooltips, like the endpoint rows. Small muted meta line (`protocol · upstream · group · bind:port · state`); small badge line in serving color with optional traffic totals. Hover tints quiet; selection takes wash plus rim; checked takes the 2px accent rim. Stopped rows honestly show pinned `○ Stopped` with no fabricated latency; a row both checked and selected shows both marks. A successful import selects the first new row and checks all of them, so `F5` and the bulk bar act at once.

The row's trailing cell is the serve endpoint, editable where it lives: a fixed bind field (`row-bind-<id>`, 118px small input) plus a fixed port field (`row-port-<id>`, 76px small input), then a small per-row Start/Stop (`row-serve-<id>`, `Start` when stopped, `■ Stop` when serving) and a small Edit (`row-edit-<id>`) — loopback IP:port:serve always on, one row. Enter commits the draft through the same save gate as the Edit dialog (reserved ranges and garbage binds toast loudly, store untouched, draft kept); Esc reverts to stored truth and stops the global-Escape from clearing the checked set; clicking elsewhere never commits, and drafts survive scrolling while external writes still repaint untouched fields. Serve/Edit flush a dirty draft first, then act on it, so the action always lands on visible values. A draft colliding with another row paints an inline `row-endpoint-warn-<id>` collision badge, and a non-loopback draft paints the loopback-guard hint — both read the draft, never just stored truth. Upstream and secrets stay dialog-owned and never render inline.

### Endpoints module

Compact one-line rows (fixed small muted label in the description-label token, truncating value, trailing small Copy): upstream display-only, then SOCKS and HTTP values each with its own small Copy button at the row end (`copy-socks` / `copy-http`, bind + port only, `Copied SOCKS/HTTP URL` toast). Copy URLs keeps the all-at-once path; per-row copy closes the all-or-nothing gap.

### Health and empty states

- **Health:** detail line reads `health: never tested — press Test to check` until the first probe; dead rows render the same pinned copy inside an error Alert titled `Endpoint failed`, with the same operate row inline beside it. The status bar keeps the short `health: never tested` describer.
- **Empty library:** empty copy with working Add (`empty-add`) and Import (`empty-import`) buttons beside it; counts read (0,0); usage is absent-or-zero, never nonzero. The no-selection inspector centers its `Empty` block in the pane instead of anchoring left.
- **File drops:** no persistent hint band — drops land anywhere on the shell, and a blocked drop (dialog open) toasts `Close the open dialog first — dropped files were not imported` instead of stacking.

### Dialogs

Titled modal, stacked fields, named commit footer sharing one commit closure (mouse and keyboard confirm identically — Add, Save, Import, Export, Move, Attach, Rename, Done, Close — never a bare OK; the export dialog joins the shared footers audit). Confirms resolve with a named affirmative carrying its result (`Delete` / `Overwrite` / `Reset` / `Clear` / `Start` / `Restore` / `Stop all and close`), danger variant on destructive ones. Settings groups into exactly four labelled sections — Basic (Appearance radio `Dark (default) / Light / System→Windows AppsUseLightTheme`, persisted and live with no restart, plus Startup, Probe, Paths, Maintenance), Routing (single-copy link into the presets manager), Preset (read-only inventory), Hotkey (the shortcut reference) — with hairline separators and danger styling on the three destructive Maintenance buttons. Import previews live valid/invalid counts; export states its plaintext-secret warning verbatim inside a kit error Alert (`export-warning-box`) with a visible safer path (`Copy URLs never carries secrets`); LAN binds stop at an explicit naming confirm naming the config and bind before start, with a post-start warning toast.

## Do's and Don'ts

Concrete guardrails grounded in the implementation. Every `Button::new` id, pinned string, and commit handler they describe is load-bearing — restyle freely, but do not rename, reorder content, or drop states.

### Do:

- **Do** build new rows, bars, and panes from theme tokens (`group_box`, `border`, `radius`, `accent`, `list_hover`, `list_active`, `success`, `danger`, `secondary`, `muted_foreground`, `description_list_label_foreground`) so light-theme changes propagate.
- **Do** keep shortcut hints visible on buttons and full `Name (Key)` strings on accessible names and tooltips.
- **Do** name affected rows in confirms, failures, and toasts — honesty is the brand voice.
- **Do** keep the serving lamp, badge, and detail Badge in agreement: green serving, red dead/failed, dark stopped.
- **Do** preserve all control states when restyling: default, hover, focus, active, loading, error, empty — and keep actions enabled-with-honest-toast rather than introducing disabled styling.
- **Do** keep the selection grammar split: wash for the selected row only, rim plus numeral chip for the checked set, `N checked (of M visible)` on the bulk bar, `Acting on: <name>` on the detail header.
- **Do** keep endpoints above the operate row and failure evidence inside the 240px minimum.
- **Do** keep row endpoint editors on stable per-row ids (`row-bind-<id>`, `row-port-<id>`, `row-serve-<id>`, `row-edit-<id>`) with Enter-commit, Esc-revert, live draft warnings, and flush-before-act on serve/edit — the draft is never silently dropped, blurred away, or committed without the save gate.

### Don't:

- **Don't** introduce display type, gradient text, glass, emoji icons, glow, or shadows — the world is plain, flat, and unadorned by decision.
- **Don't** spend color on decoration; blue is selection-or-checked-only, green/red are state-only, black is primary-action-only.
- **Don't** add a third text size or a second corner radius without replacing the system, not extending it.
- **Don't** turn confirm dialogs into silent actions or collapse per-item import failures into a single toast.
- **Don't** surface secrets: copy, logs, URLs, and search matching stay clear of credential fields.
- **Don't** invent per-row start tracking (the amber starting state stays global: `Starting…` label plus spinner plus busy guard) or per-row latency for stopped rows.
- **Don't** add Kbd chips to unbound actions — a hint without a binding is a lie.
- **Don't** commit row endpoint drafts on blur or focus loss — Enter commits, Esc reverts, and serve/edit flush first; a draft the user hasn't confirmed must survive scrolling untouched.
