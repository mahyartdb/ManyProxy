# Plan: config-completeness (guided editors, Throne parity)

FEATURE_DIR: `.planning/config-completeness/` · SPEC: `spec.md` · Artifacts: `research.md` (done),
`data-model.md`, `contracts/guided.md`, `quickstart.md`.

## Technical Context

- Language: Rust stable, edition 2021, single crate (`manyproxy` 0.1.0).
- Sole UI dep: `gpui-kit 0.6` (`use gpui_kit::*`); no new deps (pickers reuse Import-Browse pattern).
  gpui-kit-maximal: `Select`/`RadioGroup`/`Switch`/`Checkbox`/`Input`/`Textarea`/`Field`/`v_form`/
  `h_form`/`Collapsible`/`Button`/`Tooltip`/`Kbd`/`Empty`/`Alert` for all new controls; hand-rolled
  divs forbidden; wrappers only at verified 0.6.6 limits (R1–R14); ledger cites constructor+builders.
- Storage: SQLite (WAL) via `store.rs`; **no schema change** — params JSON stays truth; fold/prefill/
  template/validate extensions only.
- Engine: sing-box 1.14.2 strict translation (`engine.rs`) unchanged except preserving panel-written
  keys; no 1.15.0-only emission; fork/xray/extra-core topology unchanged.
- Import: `import.rs` grammars unchanged except `direct` intake (trivial, in-ticket).
- Testing: `cargo test` (store fold/prefill/template/validate per protocol; engine `check` on
  panel JSON) + `#[gpui_kit::test]` headless (`assert_painted` ids, pinned strings, layout-toggle
  + edit-lock + ENTER-guard tests); live proof opt-in (isolated `MANYPROXY_DB`, synthetic rows).
- Platform: Windows 10/11 64-bit; loopback default, no admin.
- Perf: dialog-only work; no tick/timer/scale impact (50-row list assumptions hold).
- Constraints: local-only (mutation-family green), app-dir sidecars, DPAPI + redaction +
  bind:port-only clipboard (Share/Export keep verbatim warnings), theme-tokens-only, honest-toast,
  verified kit APIs only, ENTER-guard (R12), never copy Throne `no_logs` bug.

## Constitution Check (pre-Phase 0)

Default constitution (spec is truth, everything testable, smallest scope, out-of-scope written down):
- Scope keeps dialog/panels/intake-wiring; system proxy/TUN/DNS/routing editor stay out (spec
  Non-goals). Pass.
- Every CAP maps to US scenarios with Given/When/Then; no implementation names in stories. Pass.
- No governance amendment in this run (feature design only). Pass.
GATE: pass → Phase 0/1.

## Phase 0 → `research.md` (done)

All unknowns resolved (R1–R12: capability flags, fold contract, key mapping, widget mapping,
hide-preserve, edit locks, AWG extras, Xray phasing, chain rules, intake scope, protocol fates,
ENTER guard). No NEEDS CLARIFICATION remain.

## Phase 1 → `data-model.md`, `contracts/guided.md`, `quickstart.md`

- Entities: capability-flag table + view-layer panel state, no storage entities. See `data-model.md`.
- Contracts: fold/template/validation/import/UI contracts (ids, errors, focus, ENTER guard). See
  `contracts/guided.md`.
- Validation: per-story runnable walks (AWG, VLESS-family, Hysteria-family, edge types, intake,
  fates) + per-ticket verify trio. See `quickstart.md`.

## Build order (spec §US mapping; P0 first)

- P0: AWG+WG (US-1), VLESS (US-2), VMess/Trojan/SS (US-3) + shared S1–S4 scaffolding (incl. S3 engine
  emission — zero handling today, ships in-slice) + protocol dropdown + edit locks + ENTER guard +
  template pristine-only change.
- P1: Hysteria/TUIC (US-4; v1-gate vs v2-optional), SOCKS/HTTP/AnyTLS/Naïve/SSH (US-5).
- P2: Snell/ShadowTLS/Xray-Ph1/chain/extra-core (US-6; Snell engine-driven, no Throne source),
  intake/row-ops (US-7; `Duplicate` exists, Share/QR are US-78/79 deps).
- P3: new-protocol fates incl. `direct` (US-8; `direct` = new `VALID_PROTOCOLS` entry + import +
  trivial serve); Xray-Ph2 is a separate follow-up program.

## Constitution Check (post-design)

- Design adds no system mutation, no new storage, no new deps, no scope creep (QR scan, TUN,
  autoselector, full XHTTP matrix stay out). Pass.
- One canonical owner per fact: spec owns requirements; research owns decisions (R1–R12);
  data-model owns flags/state; contracts own fold/validation/UI; quickstart owns validation. Pass.
GATE: pass → ready for `/to-tickets`.
