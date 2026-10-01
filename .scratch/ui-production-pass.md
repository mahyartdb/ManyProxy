# ManyProxy production-grade UI pass (overnight run, 2026-10-01)

## Exit predicate
`cargo check` clean AND `cargo test` 641/641 green AND app launches live
(`MainWindowTitle=ManyProxy`, `Responding=True`) AND every toolbar/detail/
bulk/facet control carries a hover tooltip AND destructive actions use the
danger variant AND README claims match shipped behavior.

## Baseline (measured)
- `cargo test`: 641 passed, 0 failed (80s).
- Live screenshot (cua-driver, 1538x1081): flat ungrouped toolbar, raw drop
  hint, no detail empty-state CTA, flat status bar, text-only `●/○` badges.
- gpui-kit 0.6.6: Badge/Tooltip/Separator/Empty/Alert/ButtonVariants all
  available, none used except plain Button/Dialog/List/Switch/StatusBar.

## Work list
1. [done] Enable cua-driver MCP (`disabled: true -> false` in opencode.json).
2. [done] Toolbar: 4 separators (Library/Manage/Engine/Health/Help) + 13
   tooltips. Labels and ids unchanged.
3. [done] Button hierarchy: Add + Start selected `.primary()`; Delete +
   bulk-delete `.danger()` (needs `ButtonVariants` import).
4. [done] Detail empty state: centered guidance + Add/Import CTAs.
5. [done] Status bar: vertical separators between count/engine/health/traffic.
6. [done] Facet pickers + Groups + Clear filters tooltips.
7. [done] Bulk bar: Start/Stop/Enable/Disable/Move/Delete/Select-visible
   tooltips (Clear selection already had one).
8. [done] Empty-library CTAs tooltips.
9. [done] Docs: route->presets, per-sub refresh interval, tray presence,
   data-plane usage metering, toolbar labels, shortcut list, DPAPI note.
10. [open] Row cards: themed surface/border/selected-highlight + inline
    colored status dot (needs `ActiveTheme` in `Context` scope; deferred to
    keep this pass compile-safe and test-green).
11. [open] Facet pickers as Popover/Select instead of dialogs (behavior
    change; needs headless interaction tests first).
12. [open] Second live screenshot after rebuild (Code Mode `execute`
    transport ended mid-run; verified via live process + headless suite).

## Verification
- `cargo check`: clean.
- `cargo test`: 641/641 green after UI edits; store-note test re-run after
  doc-const edit (see below).
- Live: PID 23280 `ManyProxy` responding (measured via `Get-Process`).
- Pinned strings untouched: `serving_badge`, `library_row_lines`,
  `search_count_text`, button ids.

## Principles applied (leaf SKILL.md read this session)
- prove-it-works: real `cargo test`, live process, screenshot over claims.
- sequence-verifiable-units: one edit group per `cargo check` gate.
- laziness-protocol: tooltips/separators/variants only; no new layers.
- model-the-domain: kept badge/row strings as the single pinned shape.
- experience-first: primary/danger/tooltips/empty-CTAs serve the operator.
- guard-the-context-window: bulk audit routed to three subagents.
