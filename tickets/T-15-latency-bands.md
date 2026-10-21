# T-15 — Latency color bands 100/300 (failed/unknown distinct)

- **id**: T-15
- **title**: Latency color bands — ≤100 green / ≤300 yellow / >300 red / failed darker / unknown visually distinct from failure
- **epic**: E4 — Health-test accuracy pack
- **size**: S
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E4 T-15 line; gap §4.3 U1 (Throne §F.1 bands vs today's `health.rs:1962 latency_tone` = `<500 Fast / <1500 Slow / ≥1500 Poor`); "most Throne rows in practice land in 100–1500; ManyProxy's 500 cut means most healthy rows look 'fast'" — honesty problem; test URL stays gstatic (H5 — settled keep/diverge, don't churn `DEFAULT_TEST_URL`, health.rs:14).

## Why

Threshold honesty: a 400 ms row labeled "Fast" is a lie the user plans routes around; Throne's 100/300 bands are the ecosystem norm and land 95% of real rows in a truthful bucket. Also the enum needs the *unknown* tone distinct from failure — today `latency_tone`'s `None → LatencyTone::Unknown` exists, but a failed probe (no latency + error) and an untested row render too similarly in some surfaces.

## What

1. `health.rs:1962 pub fn latency_tone` — thresholds become: `Some(ms) if ms <= 100 → Fast`, `Some(ms) if ms <= 300 → Slow`, `Some(…) → Poor` (the boundary cases per Throne §F.1: ≤100 green, ≤300 yellow, >300 red). Keep `None → Unknown`.
2. Failed-vs-unknown distinction — visual only (no enum change required if tone rendering can distinguish): in the row/detail renderers, a *failed* sample (sample present with error, no latency or error-only sample) renders with the Poor-adjacent darker tone and the existing cause text; an *untested/no-sample* `Unknown` stays the neutral tone. Concretely: extend `LatencyTone` with a `Failed`-style variant OR pass error-presence at call sites — implementer picks the smaller diff; acceptance is the visually distinct-ness in (a) row health text tone, (b) Health tab detail line, (c) sparkline caption.
3. Keep `RowState::{Live,Dead,Unknown}` classification (`classify_health_cause` untouched, health.rs:1294) — bands are presentation only; **no test-cutoff semantics change** (a 350 ms row keeps its sample; the tones rank presentation).
4. Sparkline + stagnation behavior untouched (`url_test_sparkline`, health.rs:1109).

## Files to touch

- `src/health.rs` — `latency_tone` thresholds (+ optional tone variant), tests
- `src/main.rs` — tone rendering call sites (row health map, Health tab, speed summaries) if the Failed-distinction needs a call-site flag

## Acceptance criteria

- [ ] `latency_tone` returns Fast ≤100, Slow ≤300 (boundary values 100/300 inclusive), Poor >300; None → Unknown unchanged.
- [ ] Failed sample tone ≠ unknown/no-sample tone in row + Health-tab + speed-summary contexts (three render assertions).
- [ ] No behavior change to probing, classification, or persistence (`record_outcome`/`status()` byte-behavior, health.rs:1697/1728).
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t65_`, verified unused): `t65_latency_bands_100_300_matrix`, `t65_failed_tone_distinct_from_unknown_in_row_text`, `t65_failed_tone_distinct_in_health_tab_detail`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData / No-migration / Plaintext / Hijack / Sidecars: N/A.
- [x] No system mutation: presentation only.
- [x] Clipboard `bind:port` discipline: N/A.
- [x] Redaction: N/A.
- [x] Test URL stays `DEFAULT_TEST_URL` (`http://www.gstatic.com/generate_204`, health.rs:14) — cp.cloudflare divergence is settled (H5, keep).

## Out of scope

- EWMA/threshold settings, per-row band overrides, theme-level band coloring redesign.
- `SpeedtestMode` display changes (mode labels untouched).
- Sparkline restyling beyond tone awareness.
