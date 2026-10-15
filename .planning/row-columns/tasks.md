# Tasks: one-line rows with toggleable columns + dirty-gated Start

Spec: `.planning/row-columns/spec.md` · Plan: `.planning/row-columns/plan.md`
Branch: `row-columns`. Check a box only when fmt + clippy + targeted tests
are green. Full suite + live screenshots before PR.

- [x] **R1 — Columns model + persistence.** `RowColumns` + CSV round-trip +
      `Store::library_columns_raw/set_library_columns` (meta table) + load
      in `with_store` with default fallback. Unit tests green.
- [x] **R2 — One-line render.** `render_item` single line (leading chip,
      dirty dot, column cells); meta/badge arms + `row-meta-*` deleted;
      `endpoint_is_dirty` helper. Painted/containment tests updated.
- [x] **R3 — Text seams.** New `library_row_cells` seam drives render;
      dense-lines fn deleted; `t01`, anatomy, merge, secrets tests rewritten
      onto cells; full-route tooltip + `serving_badge` kept.
- [x] **R4 — Columns picker UI.** Tools menu + palette twin + live dialog;
      persist + mirror + repaint; palette index test extended. (Headless
      Switch clicks don't fire `on_change` — test drives the handler path;
      live click verified in screenshots.)
- [x] **R5 — Dirty-gated Start.** Gate in `start_row` + no-flush serve
      button + bulk skip-dirty; honest toasts pinned; tests green.
- [ ] **R6 — Docs + verify + PR.** DESIGN row section; screenshots
      (seeded demo DB); full trio; live 1000x700/880x620; open PR.
