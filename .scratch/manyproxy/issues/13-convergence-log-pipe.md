# 13: Convergence — live sidecar log pipe

**What to build:** The running sidecars' stderr streams into `LogStore` (per-config tagging via existing `in-`/`out-` tag match, redaction on ingest, bounded buffer), so the logs dialog shows live lines in production instead of an empty pane. Fail-closed: unreadable pipe surfaces a named error, never stale or cross-config lines.

**Blocked by:** 09 (Hardening + release proof).

**Owns:** rust-networking

**Status:** done (verified: `cargo test` 177 passed)

- [ ] Scenario: engine running with mock sidecar emitting tagged lines → Action: open logs dialog → Expected: lines appear under the right config, secrets redacted. Verification: integration check. Priority: important.
- [ ] Scenario: sidecar exits → Action: observe logs → Expected: exit noted once, pipe closed cleanly, no panic. Verification: automated test. Priority: important.
- [ ] Prohibited side effect: no unbounded memory growth (bounded ring); no cross-config line attribution.
