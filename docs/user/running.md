# Running the pool

Start and stop control serving right now — what binds a local port and carries
traffic. The `Start with ManyProxy` switch in the config form is separate: it
only restores rows when ManyProxy itself launches.

## One config

Select a row and press `F5` to start, `Shift+F5` to stop, `Ctrl+T` to test —
or use the row's hover actions: `Start` and `Stop` appear when you hover, and
`More` carries `Test` plus the rest. The state glyph and the Health column show
the result; the console's Logs tab shows the engine's own output.

## Everything at once

The engine bar carries `Start all` and `Stop all`. If any row binds beyond
loopback, a confirm names each config and its bind address before they start.
`Test all` and `Stop testing` live in the engine bar's `⋯` menu; `Ctrl+.`
stops a running test from anywhere.

## Watching

- **Console** (`Ctrl+``, or the engine bar's `Console` button) opens the dock
  above the engine bar: `Logs` (with pause, clear, and a level filter),
  `Connections` (live per-destination traffic), and `Traffic` (session totals
  and runtime stats). The `All | Selected` switch narrows everything to the
  selected rows.
- **Health.** Each test records a probe; the inspector's Health section shows
  the sparkline, the latest latency, and the failure reason of the last bad
  probe. `Clear test results` in the engine bar's `⋯` menu wipes the evidence
  after a confirm.
- **Session restore.** Whatever was running when ManyProxy last exited comes
  back on launch when Settings → General keeps the restore switch on. LAN
  binds still ask first.

## When something refuses

Every action stays enabled and explains itself instead of disabling: starting
with an empty library says so, an already-running row says so, a failed start
carries the engine's error and keeps the old endpoint serving. The tray icon
keeps the pool alive when the window closes (that choice is in Settings too),
and right-clicking it offers `Start all`, `Stop all`, `Show`, and `Quit`.
