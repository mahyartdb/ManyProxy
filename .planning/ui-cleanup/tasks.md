# Tasks: UI cleanup — library shell de-clutter

Spec: `.planning/ui-cleanup/spec.md` · Plan: `.planning/ui-cleanup/plan.md`
Branch: `ui-cleanup`. Check a box only when fmt + clippy + targeted tests are
green for that ticket. Full suite + live screenshots before PR.

- [x] **T1 — Bands breathing room** (`src/main.rs`: `render_search_count`,
      `render_facet_bar`, `render_bulk_bar`). `py_1` on all three band roots.
      Ids/order/strings kept. Verified: full suite green + live 880x620.
- [x] **T2 — Row header wraps** (`src/main.rs` header ~:1224). `flex_wrap`
      added; lamp/checkbox/name/editors/chip ids + order kept. Verified:
      full suite green + live 880x620 (editors on one row, no clip).
- [x] **T3 — Endpoints hug values** (`src/main.rs` `render_detail_endpoints`).
      Endpoint-rows column capped at `max_w(px(620.))`; `w_full` rows, ids
      `endpoint-value-*`, `copy-*` unchanged. Verified: full suite green +
      live 880x620 (Copy buttons pulled in from the far edge).
- [x] **T4 — Dialog columns (verified OK, no change).** Live Add on HEAD is
      legible; screenshot truncation was stale-build-only. Column `min_w` +
      wrap broke `t11` min-frame fit → reverted, nothing ships.
- [x] **T5 — Master throw theme radius** (`src/main.rs` ~:23381). Deleted
      `.rounded(px(999.))` on `global-start-stop`. Label/a11y/tooltip kept.
      Tests: `cargo test global_toggle --locked` + `t02_global_toggle*`.
- [x] **T6 — Status bar (verified OK, no change).** Lowering `min_w` floors
      broke `t10` fixed-slot stability → reverted, nothing ships. Floors
      210/430/150 stand.
- [x] **T7 — DESIGN.md tabs + filter language** (`DESIGN.md` Layout +
      Navigation). Six tabs documented; filter paragraph describes the
      one-row bar. Verified: prose diff only.
- [x] **T9 — One-row filter bar** (`src/main.rs`: AppView `search_input` +
      subscription, `facet-bar` merge, kit `.searchable(false)`). Full suite
      green (1021); live 1000x700 + 880x620 verified; screenshots retaken.
      One re-entrancy fix on the way (`clear_library_search` touches the
      input directly instead of re-entering AppView update).
- [x] **T8 — Final verification.** `cargo fmt --check` + `cargo clippy
      --all-targets --locked -- -D warnings` + `cargo test --locked` (1021
      green) + fresh-build live screenshots (1000x700, 880x620) confirming
      spec acceptance. Then open PR.
