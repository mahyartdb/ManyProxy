# T-05 — Bulk proxy workflow (the antidetect use case)

Owner: implementer (engine + UI). Depends on: T-03 (bulk bar exists).

Context: users run 20–100 antidetect profiles, one local SOCKS proxy each. Today they copy
50 URLs one row at a time, and `Start checked` restarts the engine once per row.

## 1. `Start checked` must be additive and apply once

Today `commit_bulk_start_ids` (`src/main.rs:26916-27015`) loops `start_one_confirmed`
(`26955`); each call replaces `engine.active` and runs a full `apply()` (`engine.rs:6768`),
so starting one row drops every already-serving listener.

`start_all_subset` (`engine.rs:6524`) is **not** the batch path: it builds a candidate only
from the passed configs and assigns `self.active = candidate` (`6587`) before `apply`, so
starting 20 checked rows would stop the other 30 serving rows.

Fix: add `engine.start_many_additive(configs, confirmed)` and call it once:
- start from `self.active.clone()`; replace by id or push each checked config;
- validate the union once, `enforce_lan_confirm` once;
- assign `active`, call `apply()` once; on error restore the previous `active`/`status`;
- return the ids newly applied plus per-row validation failures.
The UI keeps its dirty-draft and unreadable skips, builds the config list, calls the method
once, compares requested vs applied for messaging, and sets enabled flags. Do not change
`Start all` / `Stop all`.

Test `start_checked_is_additive`: A and B are serving, only C is checked; after the call
A and B remain in `engine.active` and were not stopped (one apply: fake `check_calls` +1,
`running_count` 3). Their listeners are not torn down.

## 2. Bulk copy

Add `Copy proxies` to the bulk bar immediately after `Move to group`. It copies one
tab-separated line per checked-and-running row:

```
p-usa-atlanta-p1<TAB>socks5://127.0.0.1:10813
```

- Running only; toast honestly: `Copied 12 proxies (3 checked rows are stopped and were
  skipped)`.
- Nothing checked → act on all running rows.
- Use `store::proxy_urls` (`store.rs:641`) for the URL; it carries the stored bind host, so
  do not hardcode `127.0.0.1` or reformat by hand.
- Add the same item to the row context menu for one row; keep the existing `Copy URLs` item
  as the full SOCKS + HTTP pair.

## 3. Port assignment must check the socket

`store.rs:8301-8318` (`auto_port_excluding`) picks the first port not present in the DB. Add
a bind probe to the candidate loop (reuse `probe_bind_available`, `engine.rs:5390-5415`);
skip occupied ports. Cap the probe attempts like the existing retry loop so add/import stays
fast.

## 4. Port failures: split the variant before claiming row-local

`PortUnavailable` is raised in three places: the per-row probe (`engine.rs:7110-7116`), the
clash controller range (`6915-6922`), and xray bridge holds (`7031-7038`). `apply()` returns
on the first; `is_row_local_error` (`5278-5299`) excludes all of them; the failure path marks
**every** candidate failed. A row-local classification cannot simply continue the batch.

Order of work:
1. Split the variant: `RowPortUnavailable { bind, port, name }` for the per-row probe, and
   `SharedPortUnavailable { bind, port, name }` for clash/bridge. Display both.
2. Add only `RowPortUnavailable` to `is_row_local_error`.
3. Preflight checked rows' `bind:port` before the union; skip and name occupied rows, then
   run the one `apply()` for the rest. This is what lets a batch survive one bad row.
4. If `apply` still returns `RowPortUnavailable`, name that one row — the apply failed, so
   do not claim the rest continued. `SharedPortUnavailable` names the controller/bridge
   port, never a row.

The old acceptance "one occupied port starts the rest" is false until the split and
preflight exist; do not implement it by catching one variant and continuing.

## Corrections (reviewer)

- `start_all_subset` is subtractive, not additive: it replaces `active`. The batch path is a
  new additive method, not that function.
- `PortUnavailable` has three causes (row, clash controller, xray bridge). Classification
  alone cannot continue a batch; split the variant and preflight row ports first.
- Removed the old "one occupied port starts the rest" acceptance until that split exists.

## Acceptance

- A/B serving, C checked → one `apply()`, A/B stay active, C starts.
- 20 checked rows → exactly one `apply()` (count fake checks, not per-row starts).
- `Copy proxies` yields 20 tab-separated lines that paste into a spreadsheet as two columns.
- Auto-assignment skips a port held by a listening temp socket.
- A checked row whose port is occupied is named and skipped; the rest start; no every-row
  failure is reported.
- `cargo test --locked` green.
