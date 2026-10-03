# Decision: ManyProxy UI completion via gpui-kit adoption

- **Slug**: ui-complete-gpui
- **Decided**: 2026-10-02
- **Verdict**: go (pending kill-gate confirmation at ticket review — see qmode gate 2)
- **Artifacts reviewed**: .scratch/ui-production-pass.md, .scratch/overnight-giant-spec.md, explore-agent inventory of src/main.rs vs gpui-component 0.6.6, README.md, .planning/plan-complete.md

## Scorecard

| Criterion | Rating | Justification |
| --- | --- | --- |
| Problem validity | strong | Three pass items are documented-open by the previous run; screenshots predate shipped toolbar/facet/bulk/detail work; no surface reads the theme at all (0 `ActiveTheme` matches). |
| Evidence strength | strong | Line-numbered inventory of every surface vs every kit component; pinned test seams enumerated (assert_painted ids, `serving_badge`, `library_row_lines*`); baseline cargo test 660/660 green. |
| Value vs. inaction | strong | UI is the product for a desktop app; rows/detail/status currently unstyled defaults, facet pickers open full dialogs for a 3-option choice. |
| Feasibility / appetite | strong | All work is in src/main.rs render paths + their tests; kit already at 0.6.6 with Badge/Popover/Select/Empty/Alert present; dialog fallback de-risks the one behavior change. |
| Strategic fit | strong | Prior run explicitly deferred these items to keep its pass compile-safe; spec-complete and README already assume a polished UI. |
| Risk posture | strong | Main risk = breaking ~40 pinned element-id/string seams; mitigated by tests-first on the behavior change, id-preserving swaps, dialog fallback, and the pinned-seam list carried into every ticket. |

## Verdict & Rationale

Go. The problem is concrete and self-documented, the evidence is a measured inventory rather than assumption, the appetite is bounded to render paths, and the one behavior change (facet pickers) ships behind new headless tests with a dialog fallback. Scope locked from triage round: (b) close pass items 10-12 + deliberate kit adoption where it genuinely fits; behavior changes allowed; engine/health/import/store/tray logic and proven README claims untouched.

## If go — Handoff

- **Problem**: ManyProxy's gpui-kit UI ships unstyled default surfaces, dialog-based micro-pickers, plain-text badges, and stale screenshots — the production pass ended with three items open.
- **Chosen approach**: theme-aware row cards + colored status dots; Popover/Select facet pickers (tests first, dialog fallback); conservative kit-component adoption (Empty, Badge/Tag, Alert, Spinner/loading, GroupBox/DescriptionList) only where ids and pinned strings survive; README truth fixes for the two false claims found; fresh live screenshots.
- **Success metrics**: cargo fmt/clippy/test green (660+), every pinned seam assertion untouched-or-consciously-updated with justification, live window screenshot showing themed rows, task list 100% checked.
- **Out of scope**: engine/health/import/store/tray logic, non-UI features, proven README claims, kit components with no fit (Avatar/Table/Sidebar/Tab/Dock/Command/etc. — inventory section C "no fit").
- **Carried open questions**: none blocking; popover-in-headless feasibility is answered by ticket T03 tests-first ordering.
