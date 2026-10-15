# Spec: UI cleanup — de-clutter the library shell (2026-10-15)

Status: spec (planning only — implementation follows `plan.md` + `tasks.md`).

Source: live launch of `target/debug/manyproxy.exe` (pid 134236, 1002x701 dark,
cua-driver UIA + screenshots) plus the user-supplied annotated screenshot
(`1 of 1` library, 1 checked row, 6 inspector tabs, red "empty space" callout
on the Endpoints block). Note: the launched exe predated `d0f688b`
(inline row editors) and `05cba2d` (scoped bulk verbs, labeled master throw),
so part of the screenshot staleness is a stale build. Tickets below target
current HEAD (`a055335`) and re-verify live on a fresh build.

## Defects (user-visible, all confirmed against HEAD code)

1. **Filter bands are cramped and read as one wall.** `search-count`
   (`src/main.rs:21140`, no padding) → `facet-bar` (`:21228`, `px_2` only, no
   vertical padding) → `bulk-bar` (`:21343`, `px_2` only) → kit `List` search
   input stack with zero breathing room. In the screenshot the bulk bar sits
   directly on the `Search configurations` placeholder; arrows flag every band.
2. **Endpoints block has a ~400px void.** `endpoint_row` (`:20420`) is `w_full`
   with value `flex_1`, so Copy buttons hug the far right edge while values
   are short. User marked it "empty space".
3. **Add/Edit dialog truncates in two-column mode.** `ConfigForm::element`
   (`:6042-6059`) puts Common and TLS-Security columns in a no-wrap `flex_row`
   of two `flex_1 min_w_0` columns; the Server/Server-port pair splits the left
   column again (`:5940-5989`). Live Add capture shows `Server (e.g. …`,
   `Port (e.g. 44…`, `Port (blank or auto = auto-as` clipped.
4. **Library row header cannot wrap.** Header (`:1224`) is a no-wrap `flex_row`
   carrying lamp + checkbox + name (`flex_1 truncate`) + fixed 118px bind +
   76px port + Start/Stop + Edit + numeral chip. At the 880px minimum the name
   is squeezed to nothing and the chip clips at the trailing edge (visible in
   screenshot).
5. **Master throw breaks the one-radius rule.** `global-start-stop`
   (`:23381`) carries `.rounded(px(999.))` (pill). DESIGN.md mandates theme
   radius everywhere except lamps/key-chips/check-chips.

Checked and cleared (no HEAD change needed): the Add-dialog truncation and
the status-bar overflow from the user's screenshot do not reproduce on
current HEAD — the launched exe predated `d0f688b`/`05cba2d`. The live Add
dialog on HEAD shows all fields legible, and the status-bar floors
(210/430/150) plus the `t10` fixed-slot stability test forbid narrowing.
6. **DESIGN.md drifts from shipped UI.** Says 3 inspector tabs
   (Details/Health/Presets); code ships 6 (`:19756`: + Logs/Connections/
   Traffic). Screenshots in `assets/screenshots/` still show the pre-menu
   lever-frame (stale goldens, retake recorded as follow-up).

## Non-goals

- No behavior changes: same buttons, same order, same ids, same pinned
  strings. Restyle/spacing/wrap only.
- No engine/store/import/health/tray logic.
- No new features, menus, shortcuts, or settings.
- No screenshot retakes in this sweep (build-gated; recorded as follow-up in
  the PR body). Screenshots stay byte-identical.

## Constraints (from AGENTS.md + DESIGN.md Do's/Don'ts)

- Keep every `Button::new` id, pinned string, and commit handler; restyle
  freely, never rename/reorder/drop states.
- No hardcoded colors: theme tokens only.
- No new palette, typeface, shadow, glow, gradient, or second radius.
- Verify per ticket: `cargo fmt --check`, then
  `cargo clippy --all-targets --locked -- -D warnings`, then
  `cargo test --locked` (subset during work, full suite before PR).
- Tests are hermetic: TEMP files, isolated `MANYPROXY_DB`, no live sidecars.

## Acceptance

- Fresh-build screenshots at 1000x700 and 880x620: bands separated by
  hairlines + air, no placeholder collision, endpoints buttons hug values,
  Add dialog fields fully legible (columns stack when narrow), row header
  wraps instead of clipping the chip, status bar fits 880 without clipping.
- `cargo fmt --check` + clippy + full `cargo test --locked` green.
- DESIGN.md tabs paragraph matches the 6 shipped tabs.
