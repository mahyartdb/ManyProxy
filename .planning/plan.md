# ManyProxy — Plan

## Technical Context

- **Language/Version**: Rust (stable, edition 2021+)
- **Primary Dependencies**: `gpui-kit` (sole UI dependency; `component`/`base`/`assets` layers), proxy engines as sidecar processes/files: official engine + Amnezia-compatible fork/sidecar + Xray sidecar + user extra-core executable
- **Storage**: Local SQLite (WAL) for library + subscriptions; JSON file export/import; per-run generated engine configs in temp dir (never the source of truth)
- **Testing**: `cargo test` for parsers/engine-mapping; `#[gpui_kit::test]` + headless window harness for list/dialogs/keys/actions
- **Target Platform**: Windows 10/11 64-bit first (loopback default, no admin); Linux/macOS later
- **Project Type**: desktop-app
- **Performance Goals**: 50-row list smooth (virtualized); 20 concurrent endpoints startable in under 60s; per-endpoint loopback curl p50 under 2s after ready
- **Constraints**: No system proxy/TUN/DNS mutation; loopback default; unique port per endpoint; secrets redacted; LAN bind behind explicit warning; no admin for default use
- **Scale/Scope**: 50 stored configs, 20 concurrent active; no hard cap in design

## Constitution Check (pre-Phase 0)

Default constitution: spec is truth, everything testable, smallest scope, out-of-scope written down.

- Scope keeps v1 to local endpoints + Throne-compatible intake; system proxy/TUN/DNS and routing editor stay out (spec Non-goals). Pass.
- Every CAP has stories with Given/When/Then; no implementation names in stories. Pass.
- No governance file ratified yet; no amendment in this run (feature design only). Pass.
  GATE: pass → proceed to Phase 0/1.

## Phase 0 → `research.md`

Consolidated from conversation + three research passes (Throne features; engine multi-endpoint mapping; gpui-kit constraints). All Technical Context unknowns resolved; spec carried zero `NEEDS CLARIFICATION` markers.

## Phase 1 → `data-model.md`, `contracts/`, `quickstart.md`

- Entities: ProxyConfig, Subscription, LocalEndpoint, HealthSample, LogRecord.
- Contracts: `contracts/library.md` (library actions), `contracts/endpoints.md` (per-endpoint behavior + routing isolation), `contracts/import.md` (accepted formats + errors).
- Validation: `quickstart.md` scenarios runnable against a built app with no code knowledge.

## Constitution Check (post-design)

- Design adds no system-network mutation, no new storage beyond spec, no scope creep (routing editor still out). Pass.
- One canonical owner per fact: spec owns requirements; data-model owns fields; contracts own behaviors. Pass.
  GATE: pass → ready for `/checklist` → `/to-tickets`.
