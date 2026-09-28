# 50: Security pass (Phase 2)

**What to build:** Close the P0/P1/P2 holes against the Phase-1 code: LAN-gate coverage for custom/extra-core parsed listens, O_EXCL temp files, sealed sub/preset URLs, fail-closed sealed rows, real ACLs without env trust, redirect-hop SSRF re-checks with https-strict toggle, extended redactor key table, 128-bit Zeroizing clash secrets, DB path canonicalization, sidecar hash-pinning, push-time redaction, in-memory secret hygiene, encrypted-export option, plus audit/deny gates.

**Blocked by:** 40-store-v5-foundation, 41-engine-reliability, 42-protocols-serve-all, 43-ui-honesty, 44-multi-select-facets, 45-subs-history-logs, 46-share-qr-theme, 47-perf-off-ui, 48-testing-gaps, 49-a11y-error-ux (runs only on Phase-1-green tree).

**Owns:** security-and-hardening

**Status:** ready-for-agent

- [ ] Scenario: custom/extra-core config binding 0.0.0.0 with loopback row bind → Action: start → Expected: LAN confirm gates it (parsed listens fed into unconfirmed_lan). Verification: security review. Priority: required.
- [ ] Scenario: pre-planted symlink at predictable temp path → Action: start → Expected: O_EXCL refusal via tempfile crate; secret never follows the link. Verification: security review. Priority: required.
- [ ] Scenario: token-bearing sub/preset URL at rest → Action: inspect DB bytes + export → Expected: sealed (schema v5 follow-on) or tokens stripped to a sealed side column; display stays redacted. Verification: security review. Priority: required.
- [ ] Scenario: plaintext-injected sealed row → Action: open → Expected: fails loudly; no permanent silent fallback (explicit opt-in escape hatch only). Verification: automated test. Priority: required.
- [ ] Scenario: DB file + WAL/SHM + export tmp → Action: inspect ACLs → Expected: user-only (no Administrators read grant, no env-USERNAME trust); hardening failure surfaces as warning. Verification: security review. Priority: important.
- [ ] Scenario: sub/preset fetch via public hostname with 302 to loopback/link-local/169.254 → Action: fetch → Expected: blocked (per-hop re-check + resolve-and-check connected IP); https-strict toggle available; plaintext-http Warn path unchanged by default. Verification: security review. Priority: required.
- [ ] Scenario: sidecar line echoing bare key / vmess id / headerprotectionkey / psk / seed → Action: ingest → Expected: redacted by extended key table with regression test. Verification: automated test. Priority: important.
- [ ] Scenario: clash secret lifecycle → Action: start, poll, stop → Expected: 128-bit getrandom, Zeroizing wrapper, cleared on stop, never persisted or echoed. Verification: automated test. Priority: important.
- [ ] Scenario: MANYPROXY_DB pointing outside app data (incl. symlink) → Action: launch → Expected: canonicalized, refused or loudly warned. Verification: security review. Priority: important.
- [ ] Scenario: swapped sidecar binary in app dir → Action: start → Expected: hash-pin mismatch refuses with guidance; probe-then-spawn TOCTOU closed by re-verify at spawn. Verification: security review. Priority: important.
- [ ] Scenario: secret-bearing stderr burst → Action: crash → Expected: push-time redaction (no raw pre-redaction window); FATAL lines pinned past eviction. Verification: automated test. Priority: important.
- [ ] Scenario: export dialog → Action: inspect options → Expected: encrypted-export (DPAPI-sealed payload) offered alongside portable plaintext with verbatim warning. Verification: manual UX review. Priority: important.
- [ ] Scenario: dependency tree → Action: cargo audit + cargo deny → Expected: gates green in CI. Verification: integration check. Priority: important.
- [ ] Prohibited side effect: no behavior that weakens Phase-1 guarantees; autostart/Run stays opt-in default-off with quoted-exe-only value.
