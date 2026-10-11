# 02: VLESS + shared panels S1–S4 + mux engine emission

**What to build:** The shared Transport (S1), TLS (S2 optional variant), Mux+Brutal (S3), Advanced dial (S4) panels exist and work end-to-end on the VLESS page (uuid+generate, flow, packet_encoding); mux keys actually emit through the engine (zero handling today — UI ships only with its engine arm).

**Blocked by:** 01-foundation-picker-locks.

**Owns:** gpui-kit composition (kit-maximal: `Select`/`RadioGroup`/`Switch`/`Input`/`Field`/`v_form`/`Collapsible`/`Button`/`Tooltip`), sqlite (fold/prefill/validate extensions, no migration).

**Status:** ready-for-agent

- [ ] Scenario: Add `vless` → Action: read form → Expected: kit `Input` uuid (+kit `Button` generate), kit `Select` flow `""|xtls-rprx-vision`, kit `Select` packet_encoding + S1 kit `Select` (tcp|ws|httpupgrade|http|grpc|quic + conditionals) + S2-optional + S3 + S4 all paint as kit primitives. Verification: automated test (headless ids per panel + conditionals ws→tcp hide-preserves). Priority: required.
- [ ] Scenario: imported Reality+Vision link → Action: open Edit → Expected: uuid/flow/packet_encoding + TLS pbk/sid/sni/fp/alpn + transport + mux state match the link (round-trip byte-stable on untouched reopen). Verification: automated test (prefill/fold round-trip). Priority: required.
- [ ] Scenario: `flow=xtls-rprx-vision` → Action: read Mux → Expected: mux disabled with reason shown (Throne parity). Verification: automated test. Priority: required.
- [ ] Scenario: brutal on → Action: read Mux → Expected: mux forced On. Verification: automated test. Priority: important.
- [ ] Scenario: panel-produced VLESS JSON → Action: run engine `check` → Expected: green; mux shape recorded per-key in ledger against sing-box 1.14.2. Verification: automated test (engine check on panel JSON). Priority: required.
- [ ] Prohibited side effect: no 1.15.0-only fields; hidden/conditional keys preserved (never wiped); no other protocol pages yet; no hand-rolled panel divs (kit `Field`/`v_form`/`Select`/`RadioGroup`/`Switch`/`Input` only); no invented kit APIs.

Retry cap: 2, then escalate.
