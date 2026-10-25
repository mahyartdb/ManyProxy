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
    leadingBarColor: "{colors.selection-rim}"
    leadingBarWidth: "3px"
---

# Design System: ManyProxy

## Overview

**Creative North Star: "The Signal Box"**

ManyProxy looks like the instrument it is: a railway signal-box panel for a personal proxy pool. Library rows are track circuits — an occupancy lamp, a nameplate, and the row's numbers on a paper-white ground. The toolbar is a menu bar with one labeled master throw (Start-all / Stop-all) at the trailing end. The detail pane is a right-hand inspector of ruled instruments (endpoints, health, usage, history), never stacked toolbars. Dialogs are ruled train orders: guided first, JSON only as Advanced.

Color is signal-only and rides gpui-kit theme tokens everywhere — app code carries zero hardcoded colors. Selection is a wash plus rim on one row only; the checked bulk set is a 3px accent bar on the row's leading edge plus a filled checkbox chip. Green means serving, red means dead or destructive, blue means selected-or-checked, black means primary action. Everything else is ink, paper, gray, or hairline.

A keyboard shortcut never appears as text inside a label. The accessible name of a control is the action it performs — `Start the selected endpoint`, never `Start (F5)`. The key lives in the control's tooltip and in the Settings Shortcuts section, where it is written out in full. Kbd chips remain legal on dropdown menu rows, inspector tab suffixes, and the operate row: a chip is a separate element, not label text.

**Key Characteristics:**
- Lamp-first rows: every row opens with an occupancy lamp in a fixed leading cell (green serving, red dead-while-serving, dark muted when stopped; flat fill, never glow).
- One selection grammar: wash marks the selected row and nothing else; checked rows carry a 3px leading accent bar plus a filled checkbox; the bulk bar always reads `N checked (of M)`; the detail header echoes `Acting on: <name>`.
- Menu-bar toolbar: five menus (Program, Settings, Groups, Routing, Tools) plus a labeled master throw (Start-all / Stop-all) at the trailing end; selection ops keep their key bindings and palette twins.
- Block-shelf detail: a right-hand inspector carries endpoints on every pane, one operate row (Auto-start switch + six inline ops + Row menu), manage actions behind the menu.
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
- **Checked Bar** ({colors.selection-rim}): a 3px accent bar on the checked row's leading edge (`row-check-bar-<id>`) plus the kit checkbox in its filled state — shape and fill, never hue alone. The bar belongs to the checked set; the 2px rim stays selection-only.
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
- **Small** (0.875rem, 1.25 line-height): row metadata, serving badges, checkbox labels, hints, status-bar segments, filter controls, `Acting on:` echo.
- No display, headline, or title roles exist — this system tops out at body.

### Named Rules

**The Two-Size Rule.** Names at base, metadata and status at small. No third size and no display faces in UI; emphasis comes from weight or status color.

## Layout

A fixed top band (a custom title bar whose trailing edge carries flat hairline minimize / maximize / close cells — hover wash, red wash on close, double-click the title area to toggle maximize — and the lever-frame toolbar), a horizontal split with the card list on the left (`flex_1`) and the inspector on the right (default 260px, 240–360px range, internal scroll only past the panel height), and a fixed status strip with three regions — counts left (`status-left`), engine + health center (`status-center`), usage totals right (`status-right`) — split by separators. Below a 1080px window width the inspector becomes a slide-over drawer instead, so the table keeps at least 640px of width; the split never trades the list's readability for the inspector's. Filters ride a lean band above the list: facet pickers first, then the search field with its `N of M` count and Clear search. Below 1080px while the drawer is open, the band reserves the drawer's width and wraps its controls instead of letting the overlay cover the count or Clear search; the bulk bar sits below only while rows are checked. A data folder that refuses a write inserts one standing strip between the toolbar and the list — it earns its own height so the table shrinks rather than being covered, it never takes focus and never blocks a click, and it stays until the app is genuinely persisting again. A standing condition gets a standing surface: a toast for a read-only folder is gone in three seconds while the condition lasts all session. Spacing runs on two steps throughout: tight 4px groups ({spacing.sm}: card text padding, header gaps, dialog field stacks) separated by generous 8px rhythms ({spacing.md}: toolbar padding, section gaps, dialog bodies).

The detail pane opens with the tab bar pinned above a scrolling body, then packs modules above one operate row: every pane carries the header plus Endpoints, then Details adds Usage, Health adds health + History, Presets adds the preset line + attach row, Logs shows the session log tail, Connections lists live sessions, Traffic charts totals. The selected row's bind/port editors ride a compact strip directly under those modules; a spacer keeps the operate row on the bottom edge, so the editors stay attached to Usage when the pane is tall. The body hugs its modules when they fit and scrolls when they do not. Below the modules sits a single `detail-operate` row (Auto-start switch + Start/Stop/Test/Ping/Logs/Copy URLs + `row-menu` trigger) under its own hairline. The switch writes the stored flag only (Start-all picks it up; Start/Stop control serving now). Eight small controls live in that one row and WRAP rather than scroll sideways — inside the 260px inspector they take four lines — so endpoints stay above the fold with no horizontal scroll at any width. The detail header names the row once via `Acting on: <name>` plus protocol/group Tags, bind endpoint, and serving Badge.

The window opens at 1000x700 with an 880x620 minimum; toolbar, detail, dialogs, and every status segment paint inside the minimum frame (pinned by containment tests).

### Named Rules

**The No-Sideways-Scroll Rule.** Crowded action sets wrap to a second row or collapse behind a menu; panes scroll vertically only. Horizontal scrolling is never the answer.

**The Endpoints-Above-Fold Rule.** The inspector's reason is its endpoints and health evidence — modules render above the operate row, and the panel's minimum height keeps failure evidence painted without resizing.

## Elevation & Depth

Flat, always, with hairlines doing the separation work: 1px borders in hairline color under the toolbar, under filter bands, around row cards, around the lever-frame plate, and between list and inspector. Tonal bands (chrome title/status bars, quiet-surface cards and plate on paper) supply the rest of the depth. No shadows exist in app code. Dialog and menu elevation belongs to the kit chrome, not to this system.

### Named Rules

**The Flat-By-Default Rule.** Surfaces are flat at rest. Separation comes from hairlines and tonal bands; shadows appear nowhere in app code and must not be introduced for decoration.

## Shapes

One radius everywhere it matters: the theme radius (6px, {rounded.md}) on row cards, buttons, inputs, textareas, dialogs, and group boxes including the lever-frame plate. Fully round ({rounded.full}) is reserved for tiny elements only — the occupancy lamp and Kbd shortcut chips. Checked rows take a 3px accent bar on the leading edge (an edge mark, not a second radius or a heavier border). Borders are 1px hairlines; a bordered card never also carries a shadow.

### Named Rules

**The One-Radius Rule.** Theme radius on cards, fields, buttons, and plates; pills only for lamps, key chips, and small controls. A second corner language would read as a second product.

## Components

Restrained and consistent: one button voice, one field voice, one chip voice per state, with tooltips naming every action and inline spinners for in-flight work.

### Buttons

- **Shape:** theme-radius corners ({rounded.md}).
- **Primary:** switchboard-ink fill ({colors.primary-ink}) with contrasting text ({colors.primary-ink-foreground}); hover deepens ({colors.primary-ink-hover}). Used for Add and Start — the two constructive commitments.
- **Hover / Focus:** kit hover tints plus visible keyboard focus. A key never appears inside a label or an accessible name: the accessible name is the action alone, and the key rides in the control's tooltip and in Settings → Shortcuts. Kbd chips stay on menu rows, tab suffixes, and the operate row. Destructive commits (remove, bulk delete, clear-checked) return focus to the shell, since the control they acted from unmounts. The inspector header carries the status role so selection changes announce.
- **Danger:** alarm-red fill ({colors.signal-stop}) with paper text (destructive confirms; the row-menu Delete gates through a naming confirm instead of carrying danger styling).
- **Small density:** `.small()` variant across toolbar, facet, bulk, detail operate row, and endpoint rows so the full control set fits without scrolling (the five menus plus the labeled master throw ride small so the strip fits the 880px minimum on one row).
- **Loading:** `Starting…` label plus inline spinner (`engine-start-spinner`) while a start is in flight; probe buttons spinner while testing or pinging. Tooltips name every action.
- **Enabled-with-honest-toast:** no control renders kit `.disabled()` — every action stays enabled and explains itself on press (`Nothing to start — the library is empty`, `"<name>" is already stopped`). Loading covers in-flight inertness.

### Menu bar (toolbar)

Five menus in fixed order — Program (`program-menu`: intake + lifecycle), Settings (`settings-menu`), Manage groups… (`groups-menu`: subscriptions + group ops), Routing (`routing-menu`: presets + Settings link), Tools (`tools-menu`: selection ops, diagnostics, palette) — plus a trailing labeled master throw (`global-start-stop`: `▶ Start all` / `■ Stop all`, `… Connecting…` while a start is in flight). Short visible labels, full `Name (Key)` names in tooltips, and Kbd chips on bound rows. Every key-bound action plus every menu-only destination also lives in the `Ctrl-K` palette: Library group (15 ops) and Manage group (9 ops) dispatched through the same handlers as buttons, menus, and keys.

### Chips

- **Style:** Tag chips for protocol and group in the detail header; Kbd chips for shortcut hints; Badge with dot for the serving line (green serving, red failed, gray stopped); the kit checkbox (filled when checked) plus the row's 3px leading bar for the checked set; port pills (`row-port-<id>`) pinning each row's `:port` at the trailing end.
- **State:** chips are read-only labels except Kbd hints, which are part of their buttons. Exclusive choices ride kit `Radio` with real checked state — the subscription interval picker, the log-level row, and the Settings Appearance picker — never bracketed (`[3600]`) or prefixed (`● Error`) text conventions.

### Cards / Containers

- **Corner Style:** theme radius ({rounded.md}).
- **Background:** quiet surface (`group_box`, {colors.quiet-surface}) on paper; selected rows take the selection wash ({colors.selection-wash}) with its rim so rim and card read as one highlight; checked rows keep the card's hairline and take the 3px leading accent bar (`row-check-bar-<id>`) plus a filled checkbox.
- **Shadow Strategy:** none — see Elevation & Depth.
- **Border:** 1px hairline ({colors.hairline}) everywhere; checked rows signal through the 3px leading bar, never a heavier border. The 2px accent rim belongs to selection alone.
- **Internal Padding:** 8px horizontal, 4px vertical ({spacing.md} / {spacing.sm}) on row cards; 8px ({spacing.md}) pane padding in toolbar, inspector, and dialogs.

### Inputs / Fields

- **Style:** hairline stroke, paper fill, theme radius; stacked label-over-field at 4px gaps. Params box fixed at 96px, import box at 160px. The Port field carries a live auto-port budget readout (`port-budget`: used of 499 across `10801–10999` plus spill `18081–18380`) on Add and Edit.
- **Row editors:** the library's bind/port fields are small kit inputs at fixed widths (bind 118px, port 76px) in the system face — the same hairline/paper/radius voice as dialog fields, with the kit focus ring. Placeholders read `127.0.0.1` / `auto`; blank port still means auto-assign.
- **Guided-first:** Add and Edit are 560px wide (the kit's 448px default clipped the Server placeholder and crowded the hint/Advanced row); Add opens on Server / Server-port / Identity mini-fields with a one-clause hint (`vless needs uuid.`); JSON is an Advanced disclosure toggled from the hint row (collapsed) or the Params header (expanded). Guided and JSON swap, never stack; expanding merges guided values into JSON, collapsing echoes JSON into blank guided inputs only. Edit opens expanded to preserve existing JSON.
- **Focus:** kit focus ring — the kit's own ring, never a hand-rolled rim or a second colour. A custom control that paints focus without a real focus handle is not focusable, and the selected library row is the case in point: the keyboard lands on the list, so the row wears the ring while that list holds focus. Dismissing an overlay returns keyboard focus to the shell — but only when no overlay is open and nothing already holds focus, checked at the END of the dismiss path, because a handler that closes one overlay and opens the next would otherwise steal the keystroke it just handed back. **Esc peels exactly one layer**: Advanced over Add closes Advanced first, then Add, and never "every modal at once". A shortcut that cannot run because an overlay owns the keyboard says `Close the open dialog first` — a refused key that does nothing is indistinguishable from a broken key. Validation keeps dialogs open with an inline error notification rather than closing; params errors auto-expand Advanced. After any overlay is dismissed, the shell reclaims keyboard focus only when no overlay is open and no element holds focus — a reclaim that fires while the user has already clicked elsewhere, or while a second overlay has just opened, steals the keystroke they were aiming. `Esc` dismisses exactly one overlay layer. Every focusable control paints a visible focus ring.
- **Error / Disabled:** inline error lines and collision warnings inside the form; `lan-hint` names the loopback guard in plain words (`Leaving loopback exposes this machine — …`); disabled rows read `disabled` in their muted meta line. No disabled-control styling exists — see honest-toast above.

### Navigation

- **Inspector tabs:** two stacked rows of three small segmented tabs (Details / Health / Presets, then Logs / Connections / Traffic), pinned above the scrolling pane body, with stable test anchors and aria labels; only one pane renders per frame, so shared ids never collide. Endpoints render on the overview above every pane; the operate row closes every pane under its own hairline.
- **Status bar:** three regions split by vertical hairlines — library counts (`N configurations · M enabled`, singular at 1; honest zero-totals on empty, never a lying zero, absent on a broken read), engine + health (`engine stopped` / `engine running` / `engine crashed (...)`; `health never tested` before the first probe, `health X live · Y dead` after), usage totals (`out 8.2 MB · in 41.0 MB`, absent when unreadable). Serving ports and the connections count left the bar: ports live in the engine's own surfaces, connections in its inspector tab. One grammar, `·` separators only, and never a glyph that degrades at the bar's size: an unlabeled `:10801 :10802` pair is a defect, because a reader cannot tell what the numbers are.
- **Search and bulk:** the inline search field with an `N of M` count beside it (Ctrl-F lands focus, no new binding) plus a Clear-search reset; the bulk bar always reads `N checked (of M)` with a wash-vs-bar legend on its tooltip. It is one line that never wraps and never slips under the inspector drawer at the 880px minimum: the label, `Start`, `Stop`, and a `More` overflow menu. More holds the secondary scope verbs (`Check all visible`, `Uncheck all`) above a separator, then the five row verbs (`Enable`, `Disable`, `Move to group`, `Copy proxies`, `Delete`), with Delete last and danger-colored inside its menu row.
- **Bulk start is additive.** `Start` unions the checked rows into the running set and applies ONCE through the batch path. It never stops an already-serving proxy: a batch that drops the listeners of the rows you did not touch is a regression, not a shortcut. `Copy proxies` copies one TAB-separated line per checked-and-RUNNING row (`name<TAB>socks5://host:port`) so twenty endpoints paste into a spreadsheet as two columns; it says how many stopped rows it skipped.
- **Port failures are attributed honestly.** A row whose OWN listen port is occupied is named and skipped while the rest of the batch starts. A port held by the clash controller's range or by an xray bridge is shared, not the row's fault, and is never attributed to a row. If the single apply itself fails, the message says nothing new started — never that the rest continued.

### Library Row (signature component)

One line per config, 30px pitch - one geometry spec, `RowMetrics`, is read by the header strip AND the row, so a column's label always sits at its value's first character (within 8px) and the row never re-measures itself into a column of stacked cards. Occupancy lamp (`row-dot-<id>`, fixed leading cell, flat fill) plus checkbox plus, while checked, the 3px leading accent bar (`row-check-bar-<id>`), then the name (`row-name-<id>`, flex truncate, full-route hover tooltip), then the visible columns, then one row-scoped `Copy proxy` action whose compact padding keeps the full label inside the fixed 72px cell. Visible columns toggle through Tools → Row columns… (`upstream` + `health` on by default, `group` + `traffic` off; persisted in the store): `row-upstream-<id>` (140px, truncating, middle-first), `row-health-<id>` (110px, serving color, empty when stopped — the gray lamp carries stopped, no fabricated latency), `row-group-<id>` (100px), `row-traffic-<id>` (120px, totals on serving rows only). Hover tints quiet; selection takes wash plus rim; checked takes the 3px leading bar and a filled checkbox; a row both checked and selected shows both marks. A successful import selects the first new row and checks all of them, so `F5` and the bulk bar act at once. The row is never taller than 32px and never wraps — the name truncates instead, and the row's only framed controls are the checkbox and `Copy proxy`.

The row's trailing cell is the serve endpoint, read not edited: the port rides as the quiet `row-port-<id>` pill, the bind as muted small text, and one `Copy proxy` action copies `socks5://<bind>:<port>` for that row alone — it reads the config by id, so it can never copy the selected row instead. The row's context menu carries the same row-scoped `Copy proxy` beside the full `Copy URLs` pair. The bind field (`row-bind-<id>`), the port input (`row-port-input-<id>`), Start/Stop (`row-serve-<id>`) and Edit (`row-edit-<id>`) live in the inspector for the selected row, where there is room to edit them — loopback IP:port:serve always on, one row. A dirty draft shows a `row-dirty-<id>` dot on the inspector's row header; Enter commits the draft through the same save gate as the Edit dialog (reserved ranges and garbage binds toast loudly, store untouched, draft kept); Esc reverts to stored truth and stops the global-Escape from clearing the checked set; clicking elsewhere never commits, and drafts survive scrolling while external writes still repaint untouched fields. Start never launches a dirty draft — row, `F5`, and bulk starts toast honestly (`"<name>" has unapplied endpoint edits — press Enter to apply or Esc to revert`) and bulk names skipped rows; Edit flushes a dirty draft first, then opens on visible values. A draft colliding with another row paints an inline `row-endpoint-warn-<id>` collision badge, and a non-loopback draft paints the loopback-guard hint — both read the draft, never just stored truth. Upstream and secrets stay dialog-owned and never render inline.

### Endpoints module

Compact one-line rows (fixed small muted label in the description-label token, whole `host:port` value, trailing small Copy): upstream display-only, then SOCKS and HTTP values each with its own small Copy button at the row end (`copy-socks` / `copy-http`, bind + port only, `Copied SOCKS/HTTP URL` toast). The row label carries the scheme, so the value stays whole at the 260px default; Copy still writes the full `socks5://` / `http://` URL. Copy URLs keeps the all-at-once path; per-row copy closes the all-or-nothing gap.

### Health and empty states

- **Health:** detail line reads `health: never tested - press Test to check` until the first probe; dead rows render the same pinned copy inside an error Alert titled `Endpoint failed`, with the same operate row inline beside it. The status bar keeps the short `health never tested` describer.
- **Serving badge string:** the badge reads `<state> · <bind>:<port> · <latency>` — a bare `Running :10801` with an orphaned colon and no host is a defect, not a style choice. The bind belongs to the endpoint, so the string carries it; the latency segment appears only once a probe has run.
- **Empty library:** the panel describes the state — title `No proxy configurations yet`, description `Add one proxy, or use Import for many at once.` — and holds no entry of its own (`empty-add` / `empty-import` stayed retired): toolbar Add/Import is the only door; counts read (0,0), the status bar keeps its segments honest (`health never tested`, usage absent-or-zero), and the bar's right end names the palette with its `Ctrl+K` chip. The no-selection inspector centers its `Empty` block in the pane instead of anchoring left.
- **File drops:** no persistent hint band — drops land anywhere on the shell, and a blocked drop (dialog open) toasts `Close the open dialog first — dropped files were not imported` instead of stacking.

### Dialogs

Titled modal, stacked fields, named commit footer sharing one commit closure (mouse and keyboard confirm identically — Add, Save, Import, Export, Move, Attach, Rename, Close — never a bare OK; the export dialog joins the shared footers audit). Confirms resolve with a named affirmative carrying its result (`Delete` / `Overwrite` / `Reset` / `Clear` / `Start` / `Restore` / `Stop all and close`), danger variant on destructive ones. Settings is a modal with a section rail and one section visible at a time — the flat wall of stacked fields is the failure mode this replaces. Six sections: **General** (Appearance radio `Dark (default) / Light / System→Windows AppsUseLightTheme`, persisted and live with no restart; launch at login; start the previous set on launch; **Window behaviour**; confirm before deleting), **Probing** (Test URL, Timeout, Test all scope), **Updates** (update feed URL, last checked, `Check now`), **Engine** (sing-box / sing-box-awg / xray status as lean `name · status` rows with the full path behind one `Details` tooltip; restart crashed engines; the bind-address list), **Maintenance** (open data folder, save visible logs, log cap, then clear traffic / health / snapshot and reset library as a neutral 2×2 grid — each click opens a naming confirm), **Shortcuts**. Routing and Preset are NOT Settings sections: their summaries live in the presets manager header and the inspector's Presets pane, and the Routing menu links to the presets manager. **Window behaviour** offers two exclusive choices: close-to-tray (the window closes and the proxies keep running) or quit-the-app (everything stops and the process exits). **Shortcuts** lists every binding from `bind_app_keys` — the whole table, not a subset — each row naming the action and its key; a shortcut reference that lists only some of the shortcuts is a trap, because the missing ones look unbindable.

Each setting is one row: label left, control right, at most one muted help line beneath, and longer prose behind a `?` tooltip. Toggles and radios auto-save on change; text fields commit on Enter or the dialog's one footer `Save` — never on blur, and a field label never repeats its placeholder. The dialog footer is one pair, `Cancel` (secondary) + `Save` (primary), bottom-right, with the Esc note beside them; the body hugs its content (capped, then it scrolls) so a short section never holds a black frame. Maintenance's four upkeep ops are neutral buttons in a 2×2 grid — red appears only in the naming confirm each click opens. Engine rows read `sing-box · ready`, with the full path behind one `Details` tooltip. The surface is 820px wide so the update feed URL renders in full instead of clipping mid-string. Import previews live valid/invalid counts; export states its plaintext-secret warning verbatim inside a kit error Alert (`export-warning-box`) with a visible safer path (`Copy URLs never carries secrets`); LAN binds stop at an explicit naming confirm naming the config and bind before start, with a post-start warning toast.

The close route is ONE decision, read from the stored `close_action`: tray closes the window and keeps the proxies running; quit stops everything and exits, confirming `Stop all and close` only when proxies are actually running. The first hide to tray says so once — `Still running in the tray — right-click the icon to quit.` — and never repeats.

## Do's and Don'ts

Concrete guardrails grounded in the implementation. Every `Button::new` id, pinned string, and commit handler they describe is load-bearing — restyle freely, but do not rename, reorder content, or drop states.

### Do:

- **Do** build new rows, bars, and panes from theme tokens (`group_box`, `border`, `radius`, `accent`, `list_hover`, `list_active`, `success`, `danger`, `secondary`, `muted_foreground`, `description_list_label_foreground`) so light-theme changes propagate.
- **Do** keep a key out of every label and out of every accessible name — the accessible name is the action alone (`Start the selected endpoint`); the key rides in the control's tooltip and in Settings → Shortcuts. Kbd chips stay on menu rows, tab suffixes, and the operate row.
- **Do** name affected rows in confirms, failures, and toasts — honesty is the brand voice.
- **Do** keep the serving lamp, badge, and detail Badge in agreement: green serving, red dead/failed, dark stopped.
- **Do** preserve all control states when restyling: default, hover, focus, active, loading, error, empty — and keep actions enabled-with-honest-toast rather than introducing disabled styling.
- **Do** keep the selection grammar split: wash for the selected row only, a 3px leading bar plus filled checkbox for the checked set, `N checked (of M)` on the bulk bar, `Acting on: <name>` on the detail header.
- **Do** keep endpoints above the operate row and failure evidence inside the inspector's minimum height.
- **Do** keep the row endpoint editors on stable per-row ids (`row-bind-<id>`, `row-port-input-<id>`, `row-serve-<id>`, `row-edit-<id>`) — now in the inspector for the selected row — with Enter-commit, Esc-revert, live draft warnings, and flush-before-act on serve/edit; the draft is never silently dropped, blurred away, or committed without the save gate. The row itself shows the port as the quiet `row-port-<id>` pill.
- **Do** decide writability with a probe that actually writes a temp file into the folder and removes it — metadata and a successful read are not evidence — and never clear the not-writable banner on a probe while the session is in memory: a writable folder does not make an in-memory session persist, so clearing it there would be the banner lying.
- **Do** lead the standing banner with the consequence (`Changes will not be saved — …`), not the cause. The strip is one truncated line and a folder path can outrun any window: with the promise last, a long path silently cut off the only part that matters. The path follows, and the OS reason rides the tooltip.

### Don't:

- **Don't** introduce display type, gradient text, glass, emoji icons, glow, or shadows — the world is plain, flat, and unadorned by decision.
- **Don't** spend color on decoration; blue is selection-or-checked-only, green/red are state-only, black is primary-action-only.
- **Don't** add a third text size or a second corner radius without replacing the system, not extending it.
- **Don't** turn confirm dialogs into silent actions or collapse per-item import failures into a single toast.
- **Don't** surface secrets: copy, logs, URLs, and search matching stay clear of credential fields.
- **Don't** invent per-row start tracking (the amber starting state stays global: `Starting…` label plus spinner plus busy guard) or per-row latency for stopped rows.
- **Don't** add Kbd chips to unbound actions — a hint without a binding is a lie.
- **Don't** put a shortcut inside a label or an accessible name — the label says what the control does, the tooltip says the key.
- **Don't** commit row endpoint drafts on blur or focus loss — Enter commits, Esc reverts, and serve/edit flush first; a draft the user hasn't confirmed must survive scrolling untouched.
