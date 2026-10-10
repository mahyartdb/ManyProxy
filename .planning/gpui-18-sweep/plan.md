# Plan: gpui-18-sweep

FEATURE_DIR: `.planning/gpui-18-sweep/` · SPEC: `spec.md` · Artifacts: `research.md` (done), `data-model.md`, `contracts/ui.md`, `quickstart.md`.

## Technical Context

- Language: Rust stable, edition 2021, single crate (`manyproxy` 0.1.0).
- Sole UI dep: `gpui-kit 0.6.6` (`use gpui_kit::*`; layers `component`/`base`/`assets`/`platform`). No new deps.
- Storage: SQLite (WAL) via `store.rs`; no schema changes in this sweep (presentation + wording only).
- Engine/health/import/tray: untouched logic; only UI call sites regrouped (same handlers, same toasts).
- Testing: `#[gpui_kit::test]` + `VisualTestContext` headless (`assert_painted` ids, pinned strings), `cargo test --locked`; live proof opt-in (isolated `MANYPROXY_DB`, 1000x700 captures).
- Platform: Windows 10/11 64-bit; default 1000x700, minimum 880x620 containment.
- Perf: library `List` stays (50 rows smooth); logs move to `VirtualList` (10k lines); no per-row spinners; in-flight via state flags (no sleeps).
- Constraints: local-only (mutation-family green), app-dir sidecars, DPAPI + redaction + bind:port-only clipboard, theme-tokens-only (0 hardcoded colors), honest-toast (no `.disabled()`), verified kit APIs only (R1–R14).

## Constitution Check (pre-Phase 0)

Default constitution (spec is truth, everything testable, smallest scope, out-of-scope written down):
- Scope keeps presentation/wording; engine/store/tray logic out (spec Non-goals 1). Pass.
- Every CAP maps to US scenarios with Given/When/Then; no implementation names in stories. Pass.
- No governance amendment in this run (feature design only). Pass.
GATE: pass → Phase 0/1.

## Phase 0 → `research.md` (done)

All unknowns resolved (R1–R14 with 0.6.6 file/line ground truth). No NEEDS CLARIFICATION remain.

## Phase 1 → `data-model.md`, `contracts/ui.md`, `quickstart.md`

- Entities: no schema changes; UI state only (selection, checked set, facet filter, inspector tab, in-flight flags, log filter/level, sheet/dialog stack). See `data-model.md`.
- Contracts: UI contract (pinned ids, pinned strings, overlay/focus rules, secrets rules). See `contracts/ui.md`.
- Validation: keyboard-only add→start→test→copy walkthrough + headless + live capture. See `quickstart.md`.

## Constitution Check (post-design)

- Design adds no system mutation, no new storage, no new deps, no scope creep (routing editor still out). Pass.
- One canonical owner per fact: spec owns requirements; research owns API decisions; contracts own ids/strings; quickstart owns validation. Pass.
GATE: pass → ready for `/to-tickets`.
