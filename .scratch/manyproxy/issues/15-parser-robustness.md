# 15: Parser robustness — accept what clients emit

**What to build:** The parser accepts real-world input variants that currently fail: trailing-slash ss links, case-variant/case-mixed wg conf markers, .conf files holding JSON/links, pasted Clash YAML, BOM-prefixed JSON, dotfile stems.

**Blocked by:** 13 (Convergence — live sidecar log pipe).

**Owns:** api-and-interface-design

**Status:** done (commit 49aaaea; `cargo test` 204 passed incl. RED-first TDD, verified)

- [ ] Scenario: `ss://method:pw@host:8388/` → Action: import → Expected: parses (trailing slash tolerated). Verification: automated test. Priority: required.
- [ ] Scenario: pasted `[interface]` lowercase conf → Action: import text → Expected: parsed as WireGuard conf. Verification: automated test. Priority: required.
- [ ] Scenario: `.conf` file containing JSON array or link lines → Action: drop file → Expected: content-sniffed (JSON/links parsed, conf parsed as conf), never forced through wg parser with wrong-taxonomy error. Verification: automated test. Priority: required.
- [ ] Scenario: pasted Clash YAML with proxies → Action: import text → Expected: proxies imported (same limited support as subscription bodies). Verification: automated test. Priority: required.
- [ ] Scenario: BOM-prefixed JSON → Action: import → Expected: parses after BOM strip. Verification: automated test. Priority: important.
- [ ] Scenario: dotfile `.conf` → Action: import → Expected: sensible stem, never `.conf-p1`. Verification: automated test. Priority: optional.
- [ ] Prohibited side effect: no previously-passing fixture changes outcome (corpus stays green).
