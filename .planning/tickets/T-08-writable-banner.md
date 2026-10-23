# T-08 — Persistent not-writable banner

Owner: implementer (store + shell). Depends on: T-03 (the shell layout).

## Why

The app's data lives BESIDE the exe — it ships as a portable ZIP, so the folder can sit
anywhere. A read-only folder, a locked database, a full disk, or an antivirus holding a
handle makes every SAVE fail. Today those failures are silent: the user edits a config,
the dialog closes, and the change evaporates. The library still READS fine, so nothing
looks wrong until the next launch shows the old value.

A toast is the wrong shape for this. The condition persists for minutes or hours, and a
toast is gone in seconds. This is a standing condition, so it gets a standing surface.

## Fix

1. **A write probe in the store.** `store.rs` knows its own path. Add a probe that
   actually writes (write + rename a temp file inside the data folder, then remove it) —
   do NOT infer writability from metadata or from a successful READ. Expose it as
   `store::write_probe() -> Result<(), String>` returning a human-readable reason
   (`permission denied`, `disk full`, the path).
   Probe on startup, after the store opens, and whenever a write fails.

2. **A persistent banner in the shell.** A full-width strip UNDER the toolbar and above the
   table — not a toast, not a dialog. It names the folder and the reason:
   `Cannot write to the data folder (<path>) — changes will not be saved.`
   Shipped copy leads with the consequence instead —
   `Changes will not be saved — cannot write to the data folder (<path>).` — because the
   strip truncates on a long path and the promise must never be the part that is cut; the
   OS reason rides the tooltip. An open failure (corrupt/locked database) says
   `cannot use` rather than `cannot write`, so it is not blamed on permissions.
   It carries one action: `Open data folder`.
   The banner is PERSISTENT: it stays while the probe fails and disappears when a probe
   succeeds. It never blocks the UI — the app starts, the library reads, and everything
   else works; only saving is affected.

3. **Re-probe.** Re-probe on the existing tick (the app already has one) and immediately
   after any failed write, so a fixed permission clears the banner without a restart.
   Do not spin a new timer thread; the tick machinery is the seam.

4. **The write failure path.** A save that fails because the folder is not writable must
   surface the SAME condition (set the banner state), not a toast that disappears. A save
   that fails for any other reason keeps its own honest toast.

## Rules

- The banner is a strip in the shell, so it must not push the table below the fold
  silently: it takes its own height and the table shrinks. The 880x620 minimum still
  holds — the banner is one line, not a panel.
- One accent, used as danger (the banner is a failure, not a decoration).
- No emoji, no glow, no icon — the words carry it.
- The banner is not a modal: it never takes focus and never blocks a click.

## Tests

- A test that a failing write probe turns the banner ON with the path in its text.
- A test that a succeeding probe turns it OFF again.
- A test that the banner does NOT appear while the probe succeeds (no false alarm).
- A test that a failed save sets the banner state.
- The containment test: the banner fits inside the 880x620 minimum frame.

## DESIGN.md, same commit

Add the banner to the shell section and to the Do's: a persistent not-writable condition
gets a persistent surface, and a write probe is the only honest test of writability.

## Acceptance

- With the data folder read-only, the banner appears at startup, names the folder, and
  `Open data folder` works.
- With the folder writable, no banner.
- Restoring write permission clears the banner without a restart.
- `cargo test --locked` green.
