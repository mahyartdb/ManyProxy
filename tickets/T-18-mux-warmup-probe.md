# T-18 — Mux warm-up double probe

- **id**: T-18
- **title**: Mux warm-up double probe — second probe pass for rows whose params declare multiplex; first pass warms, second counts
- **epic**: E4 — Health-test accuracy pack
- **size**: S–M
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E4 T-18 line; gap §4.5 H1 (Throne probes non-current batches **twice** for mux warm-up w/ 2 ms stagger, §E.2/§F.2; "Single probe measures listener+exit cold-chain, which for muxed and chained rows reads pessimistically"); verified: engine emits `multiplex:{enabled}` from params (engine.rs:2956–2970; params `mux` values on/off; test `t02_mux_emission_shape_on_four_protocols` region — engine.rs ~11709/11738/11755), group runners `test_group` (health.rs:862) / `run_group` (health.rs:886), concurrency `GROUP_TEST_CONCURRENCY` (health.rs:18), `MAX_PROBE_CONCURRENCY` alias 623.

## Why

Muxed rows pay connection warm-up inside the first probe: latency measured on a cold mux session reads materially worse than steady state — a false-negative that pushes users away from information. Throne's answer is a warm-up pass. Scope-precisely: **only** rows whose config declares multiplex, second pass only when the first pass did not decisively fail, warm-up sample is not recorded, concurrency/stagger semantics of the batch untouched.

## What

1. Row selection: a pure fn `fn params_declare_mux(params_raw: &str) -> bool` reading the stored params (the `mux` fold values; mirror the engine's params-key logic at engine.rs:2956 so UI and engine can't drift). Placed per cohesion — engine.rs export, health.rs consumer.
2. Runner change in `test_group`/`run_group` (health.rs:862/886): batch rows ordered as today; for each row that `params_declare_mux` and whose first probe returned a **live-decisive-but-likely-warmup-affected** result (i.e. positive latency; failures/dead results stand — no retry on failure), issue a second URL-test pass through the same staged runner mechanics; **record only the second sample** (`record_outcome` with the second pass; the first pass is explicitly NOT recorded — T-16's neutrality machinery is not relied on; the first probe simply never reaches `record_outcome`).
3. Concurrency/stagger preserved: the second pass runs under the same `GROUP_TEST_CONCURRENCY`(10)/`MAX_PROBE_CONCURRENCY` cap and batch semantics (batch 100, 200 ms poll — untouched); rows with multiplex off keep **one** probe exactly as today; stop flag honored before the second pass starts (stop mid-pass → no second probes issued; already-started second probes behave per T-16 abort semantics).
4. Speed tests/tcp-ping untouched (single-pass as today).
5. UI copy: the in-flight group-test progress line may show an "(warm-up)" note for mux rows' second pass — implementer's judgment; no state added beyond the runner's existing bookkeeping.

## Files to touch

- `src/engine.rs` — `params_declare_mux` export (+ test)
- `src/health.rs` — `test_group`/`run_group` second-pass wiring + tests
- `src/main.rs` — none expected (or the warm-up progress note only)

## Acceptance criteria

- [ ] Params-declared-mux rows receive two URL tests per group sweep; the recorded sample is the second pass; non-mux rows unchanged (one probe).
- [ ] First-pass failure/dead results are NOT retried (decisive failure stands).
- [ ] Concurrency cap, batch size, stagger, and stop-flag behavior unchanged; a stop during a mux row's first pass prevents its second pass.
- [ ] `params_declare_mux` mirrors the engine's own params-mux logic (one shared rule; divergence is a bug — test both against the same fixtures used by the engine's multiplex emission tests e.g. the `mux:"on"` params shape).
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t68_`, verified unused): `t68_params_declare_mux_matches_engine_logic`, `t68_mux_rows_get_second_probe_only_live_first`, `t68_recorded_sample_is_second_pass`, `t68_non_mux_single_probe_unchanged`, `t68_stop_before_second_pass_prevents_it`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData / No-migration / Plaintext / Hijack / Sidecars: N/A.
- [x] No system mutation: loopback prober only; double probing is the user's explicit group-test action (actions bounded by stop flag).
- [x] Clipboard discipline / Redaction: N/A.

## Out of scope

- EWMA/multi-mode test runners, COUNTRY speed mode (Throne §E.2 — not adopted).
- Chained rows' warm-up (chained reads are a different cold-chain story; the brief scopes to "rows whose params declare multiplex" only).
- `GROUP_TEST_CONCURRENCY` value change (stays 10).
