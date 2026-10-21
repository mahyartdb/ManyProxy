# T-05 — Bulk proxy workflow (the antidetect use case)

Owner: implementer (engine + UI). Depends on: T-03 (bulk bar exists).

Context: the app's users run 20–100 antidetect browser profiles, one local SOCKS proxy per
profile. Today they must copy 50 URLs one row at a time, and pressing `Start checked`
restarts the whole engine once per row.

## 1. `Start checked` must apply once

Today `src/main.rs:26936-26969` loops `engine.start_one_confirmed` per id; each call runs a
full `apply()` (`engine.rs:6644-6677` → `7171-7196`) which stops and restarts every shard, so
each started proxy drops every already-serving listener.

Fix: collect the checked ids that pass the existing preconditions (dirty draft skip,
unreadable skip), then call the existing batch path `engine.start_all_subset` once
(`engine.rs:6523`, already used by `main.rs:15505`). Keep the current per-item failure
messaging by comparing requested vs started afterwards.

Do not change `Start all` / `Stop all` behaviour.

## 2. Bulk copy

Add to the bulk bar, immediately after `Move to group`:

`Copy proxies` — copies one line per checked-and-running row:

```
p-usa-atlanta-p1<TAB>socks5://127.0.0.1:10813
```

- Include stopped rows? No. Copy running only, and toast honestly:
  `Copied 12 proxies (3 checked rows are stopped and were skipped)`.
- If nothing checked, act on all running rows.
- Reuse `store::proxy_urls` (`store.rs:641-652`) for the URL; do not reformat by hand.
- Also add `Copy proxies` to the row context menu for the single row, and keep the existing
  `Copy URLs` item as the full `socks5 + http` pair.

## 3. Port assignment must check the socket

`src/store.rs:8301-8318` picks the first port not present in the DB. Add a bind probe to the
candidate loop (reuse `probe_bind_available` logic from `engine.rs:5390-5415`); skip occupied
ports. Cap the probe attempts like the existing retry loop so add/import stays fast.

## 4. One port failure must not abort the batch

`PortUnavailable` is not treated as row-local (`engine.rs:5280-5297`), so one conflict aborts
`Start all` (`main.rs:15494-15503`). Classify it as row-local, report it against that row,
and continue the batch. The toast names the rows that failed.

## Acceptance

- Starting 20 checked rows issues exactly one `apply()` (assert by counting shard restarts in
  a unit test or by an existing hook).
- `Copy proxies` yields 20 tab-separated lines that paste into a spreadsheet as two columns.
- Auto-assignment skips a port held by a listening socket (test with a bound temp socket).
- A batch containing one occupied port starts the rest and names the failure.
- `cargo test --locked` green.
