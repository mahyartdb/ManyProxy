# T-04 — Settings rebuild + close behaviour

Owner: implementer (UI). Depends on: T-03 (shares chrome work).

## Why it reads as a mess (measure, don't guess)

The dialog sets no explicit width or height; it renders as a narrow, centre-docked column
with no scrim. `Basic`, `Startup`, `Probe`, `Paths` and `Maintenance` are the same 12px grey
label at one indent, so three levels of hierarchy read as one flat column. Help prose
outweighs the controls. `Save test URL` / `Save feed URL` stretch to the container and read
as inputs; the update-feed value clips mid-URL; the only exit is a quiet `Close`.

Measure first: record the current dialog width/height, button widths and feed-field clip in
the ticket/commit, then state the target values. The earlier draft's pixel figures were not
in the code — do not bring them back.

## Structure

Six named sections in a left rail; the body shows one at a time:

```
General      Appearance (Dark/Light/System) · Startup (launch at login, start all) ·
             Window behaviour: ( ) Close to tray, keep proxies running   (•) Quit the app
Probing      Test URL · Timeout · Test all scope
Updates      Update feed URL (full width, no clip) · Check now · last checked
Engine       sing-box / sing-box-awg / xray status + paths · Auto-restart crashed engines
             · Bind addresses (the list row pickers read)
Maintenance  Open data folder · Save visible logs · Reset library (danger, confirms by name)
Shortcuts    every binding in `bind_app_keys` (`src/main.rs:139-160`)
```

Rules:
- Each setting is one row: label left, control right, at most one muted help line beneath.
  Help is one sentence; longer text goes behind a `?` tooltip.
- Buttons never stretch full width. Commit buttons sit right-aligned in the section footer,
  primary for the commit, ghost for Cancel.
- Field labels do not repeat their placeholder. `Test URL` label, `https://…` placeholder,
  no `(blank = default)` parenthetical.
- Toggles/radios auto-save on change; text fields commit on Enter or one `Save` in the
  section footer (never on blur). `Close` becomes `Done` in the dialog footer, primary.
- Settings opens as a modal with a real scrim (kit `Modal`), not a floating column.
- `Esc` closes it and returns focus to whatever opened it (T-06).

Routing and Preset summaries leave Settings: link to the presets manager from the Routing
menu and the inspector Presets pane, with no dead links. The bind-address list stays.

## Shortcuts section (must exist)

Ctrl-1..6 must be discoverable without hovering an aria label. List every binding from
`bind_app_keys` (`139-160`), not the current 16-line subset: `SHORTCUT_LINES` (`237-256`)
omits Ctrl-4/5/6 and Ctrl-R. Update `SHORTCUT_LINES` and the exact-list test at `55162` in
the same change. The section renders `Action — Key` rows, reads from that one list.

## Close behaviour (user decision)

Add a persisted `close_action` setting: `Tray` (default, keeps proxies running) and `Quit`
(stops all profiles and exits).
- `Settings → General → Window behaviour` exposes it as a radio pair with one line of
  explanation under the selected option.
- `on_window_should_close` (`29044-29099`) currently hides to tray unconditionally whenever a
  tray exists. Branch on `close_action` before that: `Tray` → `hide_to_tray`; `Quit` → the
  no-tray path (`close_veto_text`; confirm with `Stop all and close` only when proxies run,
  then `confirm_close_stop`, otherwise mark clean exit and close).
- First hide to tray: one-time toast `Still running in the tray — right-click the icon to
  quit.` Persist a `seen_tray_hint` flag.

## Corrections (reviewer)

- Kept a Shortcuts/Hotkey section; the first draft dropped it, leaving Ctrl-1..6 only in
  an aria label.
- Removed invented pixel values; the implementer measures current and states targets.
- DESIGN.md:234 ("exactly four labelled sections") is stale; update it to this set.

## Acceptance

- Measured current values and chosen targets are recorded in the commit/ticket.
- One modal, six named sections, no orphan controls; only deliberate full-width fields
  stretch — buttons size to content and sit right-aligned.
- `Update feed URL` renders its full value.
- Shortcuts lists every `bind_app_keys` binding; the test at `55162` matches.
- `Quit` exits and leaves no sidecars; `Tray` keeps them and shows the hint exactly once.
- `cargo test --locked` green; a test pins the `close_action` round-trip and default.
