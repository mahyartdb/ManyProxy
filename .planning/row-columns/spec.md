# Spec: one-line library rows with toggleable columns + dirty-gated Start

Status: spec (planning only — implementation follows `plan.md` + `tasks.md`).
Branch: `row-columns`. User answers locked: one line per config; columns the
user can add/remove with a safe subset on by default; Start gated when the
row has unapplied endpoint edits.

## Defects (all five idea lenses agreed)

1. One row does 4 jobs across 3 lines (identity + editing + acting + bulk
   state); `bind:port` renders 3×, serving state 3× (lamp + meta token +
   badge); meta packs 5 equal-weight facts; Start sits 4px from two text
   fields and silently commits-then-launches a half-typed draft; ~250 tab
   stops for 50 rows; trailing cells jitter with name length.

## Design

One line per row: lamp + checkbox + check-chip (leading, square-ish, with
count) + name (flex truncate) + optional columns + bind/port editors +
Start/Stop + Edit. Locked core (always on): lamp, checkbox, name, endpoint
editors, actions. Toggleable columns with safe-subset defaults:

| column     | default | content                                          |
|------------|---------|--------------------------------------------------|
| `upstream` | ON      | `host:port` text, middle-truncating              |
| `health`   | ON      | badge text when serving/failed, empty + gray lamp when stopped (no more `○ Stopped` line) |
| `group`    | OFF     | group tag                                        |
| `traffic`  | OFF     | `↓/↑` totals, running rows only                  |

Meta line and badge line are deleted; the full route survives as the row
hover tooltip (`library_row_full_route`, unchanged). Serving state rides
lamp + health cell; search matching is untouched (SearchBlob, not display
text). Dirty bind/port inputs carry a visible dirty dot; Start on a dirty
row toasts `"X" has unapplied endpoint edits — press Enter to apply or Esc
to revert` and does NOT launch (row button, F5, bulk-checked starts skip
dirty rows and name them). Edit keeps flush-before-open. Start-all keeps
stored-value semantics (noted, unchanged).

Columns UI: `Tools` menu → `Row columns…` + `Ctrl-K` twin opens a dialog
with one checkbox per toggleable column; toggling persists immediately via
the store `meta` table (`library_columns` CSV; unfamiliar/missing values
fall back to defaults). No new facet-bar button (bar stays lean).

## Non-goals

- No keyboard overhaul (roving tabindex, Esc single-duty, SR row roles are
  recorded follow-ups, not this PR).
- No edit-on-demand / single host:port field / Edit demotion (proposed,
  declined for this slice — editors stay always-on).
- No engine/store/import/health/tray logic beyond meta read/write.
- No new palette, typeface, radius, or color meanings.

## Constraints (AGENTS.md + DESIGN.md)

- Every NEW control gets a stable test id; removed ids (`row-meta-*`) get
  their tests updated in the same ticket with rationale — no silent drops.
- `serving_badge` keeps exact copy (detail header uses it; its test stays).
- No hardcoded colors; flat; two sizes; 880px minimum containment holds
  with all default-ON columns (pinned by updated containment tests).
- Verify per ticket: `cargo fmt --check`, `cargo clippy --all-targets
  --locked -- -D warnings`, `cargo test --locked` (subset in flight,
  full before PR). Tests hermetic (TEMP, isolated MANYPROXY_DB).

## Acceptance

- 50 seeded rows: each paints exactly one card line; toggling each column
  shows/hides its cells live and survives restart; defaults match table.
- Dirty row + Start (button, F5, bulk) → honest toast, nothing launches;
  Enter then Start launches.
- Full suite green; fresh 1000x700 + 880x620 captures with all default
  columns on; screenshots replaced.
