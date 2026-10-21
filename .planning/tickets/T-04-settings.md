# T-04 — Settings rebuild + close behaviour

Owner: implementer (UI). Depends on: T-03 (shares chrome work).

## Why it reads as a mess

The panel is 440px wide, 900px tall, centre-docked with no scrim, and renders three levels
of hierarchy — `Basic`, `Startup`, `Probe` — as one flat left-aligned column of 12px grey
labels with no dividers. Help prose outweighs the controls ~4:1. `Save test URL` and
`Save feed URL` stretch to 386px, so they look like the inputs above them. `Update feed URL`
clips mid-URL. The only exit is a quiet 44px `Close`.

## Structure

Two-column settings surface inside a 720px dialog (not a 440px column), with a left section
rail and a scrolling body:

```
General      Appearance (Dark/Light/System) · Startup (launch at login, start all on launch)
             Window behaviour: ( ) Close to tray, keep proxies running   (•) Quit the app
             [one line of explanation below the chosen option only]
Probing      Test URL · Timeout · Test all scope
Updates      Update feed URL (full width, no clip) · Check now · last checked
Engine       Engine binary status (sing-box / sing-box-awg / xray: found/missing + path)
             Auto-restart crashed cores → rename to Auto-restart crashed engines
Maintenance  Open data folder · Save visible logs · Reset library (danger, confirms by name)
```

Rules:
- Left rail lists the six section names; the body shows one section at a time. No section
  numbering, no nested sub-headings at the same indent.
- Each setting is one row: label left, control right, at most one line of help beneath, muted
  and smaller. Help prose is capped at one sentence; move anything longer behind a `?`
  tooltip.
- Buttons never stretch full width. Commit buttons sit in the section footer, right-aligned,
  primary fill for the commit, ghost for Cancel.
- Field labels do not repeat their placeholder. `Test URL` label, placeholder
  `https://…`, no `(blank = default)` parenthetical — the placeholder already says it.
- Auto-save on change; sections with a pending write show a single `Save` in the section
  footer. `Close` moves to the dialog footer as `Done`, primary weight.
- Settings opens as a modal with a real scrim (kit `Modal`), not a floating column.
- `Esc` closes it and returns focus to whatever opened it (see T-06).

## Close behaviour (user decision)

Add a persisted `close_action` setting with two values: `Tray` (default, keeps proxies
running) and `Quit` (stops all profiles and exits).
- `Settings → General → Window behaviour` exposes it as a radio pair with a one-line
  explanation under the selected option.
- `src/main.rs:28987-29025` `on_window_should_close` reads the setting:
  `Tray` → existing `hide_to_tray` path; `Quit` → the existing `confirm_close_stop` path
  (honest confirm when proxies are running, no confirm when nothing runs).
- First time the window is hidden to tray, show a one-time toast:
  `Still running in the tray — right-click the icon to quit.` Persist a `seen_tray_hint` flag.

## Acceptance

- Settings is one modal, 720px, with six named sections and no orphan controls.
- No button wider than 200px except a deliberate full-width field.
- `Update feed URL` renders its full value.
- Closing with `Quit` exits the process and leaves no sidecars; closing with `Tray` keeps them
  and shows the hint exactly once.
- `cargo test --locked` green; a test pins the setting round-trip.
