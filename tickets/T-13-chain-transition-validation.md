# T-13 — Chain transition-order validation

- **id**: T-13
- **title**: Chain transition-order validation pre-build (walking order sing→xray→sing; ≤1 extra-core hop; no selector hop)
- **epic**: E3 — Routing & interop borrow
- **size**: S
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E3 T-13 line; gap §4.2 C1 (Throne validates hop order `(sing→xray→sing)` + constraints at §C Meta-profile with exact errors; ManyProxy's flat chains + `CHAIN_EGRESS_RULE` mixing rejection exist — "the missing piece is transition-order validation — ManyProxy currently learns core boundaries only at build time"); verified symbols: `engine.rs:1231 ChainHopKind::{Standard,Xray,Custom,Extra}`, `engine.rs:178 CHAIN_EGRESS_RULE` = "Cannot use Extracore or Custom or Xray configs in a chain", `engine.rs:1178 validate_extra_row`, `store.rs:2404 ChainHopSlot` (Autoselector refused for chains — store.rs:2423 region).

## Why

Chains are flat and pre-validated for mixing/nesting/selector slots, but the **core-boundary order** is only discovered during `build_main_config` — which produces confusing build-time errors at the worst moment. Throne validates the walking order up-front so a user editing a chain learns in the **form**, not the log. The fix is a pure pre-build validation next to the existing chain helpers; zero behavior change for already-valid chains.

## What

1. New pure fn in engine.rs: `pub fn validate_chain_transition_order(chains: &[(&str, &[ChainHop])]) -> Result<(), EngineError>` (exact signature at implementer's discretion; one call validating all user-built chains pre-apply, invoked from the apply path next to where `validate_extra_row`/`CHAIN_EGRESS_RULE` checks run today) with **ManyProxy's own** error strings:
   - core-transition walking order: a standard hop → xray hop → standard hop is the only permitted core-crossing shape; two consecutive xray hops ("chain must stay sing-box → xray → sing-box"); an xray hop followed by an extra/custom hop → error naming the pattern ("chain hop sequence must stay sing-box → xray → sing-box");
   - at most **one** extra-core hop per chain (a second `ChainHopKind::Extra` anywhere → error);
   - no autoselector slot — already refused at slot insert (store.rs `ChainHopSlot` guard) — validate anyway as defense-in-depth at this layer with the same-family error (no new machinery; pure function re-check).
2. Error strings carry the chain name + hop positions in the same style as `CHAIN_EGRESS_RULE` detail wrapping (`engine.rs:546` "{CHAIN_EGRESS_RULE} (chain \"{name}\"): {detail}").
3. **No behavior change otherwise**: valid chains generate exactly as today (byte-level test pin below); the validation runs before build so a bad chain now fails in the apply dialog step with the typed error, not deep in generation.

## Files to touch

- `src/engine.rs` — new validation fn + call-site wiring in the apply/plan path + tests

## Acceptance criteria

- [ ] `sing → xray → sing` ok; consecutive xray / xray→extra / xray→custom / extra→xray patterns fail with "chain hop sequence must stay sing-box → xray → sing-box";
- [ ] second Extra hop in any chain fails; Autoselector slot re-check fails with the family error;
- [ ] valid today-chains generate byte-identically to before (contract test naming today's chain configs in engine tests).
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t63_`, verified unused): `t63_transition_order_accepts_sing_xray_sing`, `t63_rejects_consecutive_extra_core_hops`, `t63_rejects_xray_custom_xray_shapes`, `t63_rejects_selector_slot`, `t63_valid_chains_byte_identical_after_validation`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData: N/A.
- [x] No-migration: N/A.
- [x] Plaintext-always-sealed: N/A.
- [x] Hijack non-concept: N/A.
- [x] No system mutation: pure validation — no config change, no sidecar change, no OS touch.
- [x] Clipboard discipline / Redaction / Sidecars: N/A.

## Out of scope

- Auto-selector machinery (never; store-level refusal stays).
- Per-group front/landing hops (C2 — gap doc judgment call; rank below the grammar borrows; only if the owner later asks).
- Bridge internals (`XRAY_BRIDGE_PORT_START..END`, engine.rs:175–176) — retirement belongs to `EPIC-fork-core.md`, not here.
