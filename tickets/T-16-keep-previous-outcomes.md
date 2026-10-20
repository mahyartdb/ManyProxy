# T-16 — Keep-previous outcomes (0 / aborted never overwrite ring)

- **id**: T-16
- **title**: HealthHistory keep-previous — 0/aborted probe results do not overwrite the sample ring; decisive Live/Dead classification unchanged
- **epic**: E4 — Health-test accuracy pack
- **size**: S
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E4 T-16 line; gap §4.5 H3 (Throne §F.2: latency>0 → value; 0 → neutral *keep previous*; −1 → failure; aborted/context-canceled → keep previous) + §2 health row "Result application … no 0-keeps-previous contract"; ManyProxy's HealthHistory strength preserved (`MAX_SAMPLES` ring, `record_outcome`, health.rs:1697 + `push_sample` 1599; `status()` classification at 1728).

## Why

Probes are the user's tool for judging rows; a flash-failing *instrument* (timeout abort, stop-flag abort, zero-reading quirk of a cold listener) must not erase the row's last honest sample — Throne documents exactly this contract (§F.2 test runner) and ManyProxy's ring already stores enough history to honor it (the `stale`/`(stale)` tracking shows the intent exists). The fix is pure gating at the write boundary: neutral outcomes don't rewrite history; decisive results are unchanged.

## What

1. Define neutrality precisely (in-code, from ManyProxy's own outcome shapes — `TestOutcome` at health.rs:128 with `HealthSample {latency_ms: Option<u64>, error: Option<String>}` at 112):
   - aborted/stop-flag-issued probes: the runner sets an explicit aborted flag today via `GROUP_TEST_CONCURRENCY` runners + `stop` param (`health.rs:862 test_group`, `886 run_group` carry `stop: &AtomicBool`) — an aborted run must not push samples for its still-running rows;
   - a **zero-latency** live report (`Some(0)`) or an error-only sample whose cause string matches the local-abort family (classify via `classify_health_cause` → `HealthCause::Local`, health.rs:1294, **without** inventing new string matching) is neutral: **do not `push_sample`**, keep the ring's previous entry as the latest;
   - a Dead result (error of Dns/Egress class or listener-refused class — any decisive cause) still overwrites, exactly as today: "Dead/Live classification unchanged where decisive" (brief wording).
2. Implementation point: gate inside `HealthStore::record_outcome` (health.rs:1697) with a small pure classifier fn `fn outcome_is_neutral(outcome: &TestOutcome) -> bool` (unit-testable); probes that traffic-counted bytes via `outcome.success` keep their traffic accounting regardless (traffic is not latency history — brief note preserve).
3. `(stale)` surfacing and `mark_stale(_all)` untouched; `hydrate()` (1710) untouched.
4. The visible outcome text for a neutral probe: rendered ONLY in the progress/last-run line as today (no fake "kept" fraud in the ring); the Health tab's detail line continues to show the ring's actual latest.

## Files to touch

- `src/health.rs` — `outcome_is_neutral` + the `record_outcome` gate + tests (+ runner plumbing only if an explicit aborted marker needs propagation from `run_group`)
- `src/main.rs` — none expected

## Acceptance criteria

- [ ] Zero-latency outcome: ring's previous URL-test sample stays latest (`status()` unchanged); traffic counters (if any bytes) still accrue.
- [ ] Aborted (stop-flagged) group test: rows still probing at stop time gain no sample overwrite; the already-completed probes' outcomes stand.
- [ ] Decisive failures/deads still overwrite exactly as today (classification matrix green).
- [ ] A live positive result always overwrites (keep-previous applies to neutral only).
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t66_`, verified unused): `t66_outcome_is_neutral_matrix`, `t66_zero_latency_keeps_previous_sample`, `t66_aborted_group_probe_does_not_overwrite`, `t66_decisive_dead_still_overwrites`, `t66_live_result_still_overwrites`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData / No-migration / Plaintext / Hijack / Sidecars: N/A.
- [x] No system mutation: probe-behavior polish only; loopback-only prober contract untouched.
- [x] Clipboard discipline / Redaction: N/A (error strings already redacted upstream of the ring).

## Out of scope

- Changing probe concurrency/stagger (`GROUP_TEST_CONCURRENCY` health.rs:18, batch 100/200 ms poll).
- Two-probe warm-up (T-18's ticket).
- `RowState` classification semantics or `classify_health_cause`'s marker lists.
- Test-URL default (H5 settled keep).
