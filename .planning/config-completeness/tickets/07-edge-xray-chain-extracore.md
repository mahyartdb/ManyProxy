# 07: Snell / ShadowTLS / Xray-Ph1 / chain hops / extra-core picker

**What to build:** Snell (engine-driven) + ShadowTLS versioned pages, Xray-VLESS Ph1 subset (unknown `xray_*` preserved), chain hop-list editor with Throne rejection rules, extra-core path picker — all kit primitives; `no_logs` deliberately omitted.

**Blocked by:** 06-small-matrices-ssh (same write surface — linear).

**Owns:** gpui-kit composition (kit-maximal: `Select`/`Input`/`Button`/`Field`/`Empty`/`Collapsible`), sqlite (fold/validate, no migration).

**Status:** ready-for-agent

- [ ] Scenario: Snell/ShadowTLS Edit → Action: read form → Expected: kit version combos (4|6 / 1|2|3) + peer fields + correct TLS gating. Verification: automated test. Priority: required.
- [ ] Scenario: Xray-VLESS Edit → Action: read form → Expected: Ph1 kit subset (uuid/enc/flow + sni/fp/alpn/pbk/sid/spiderx/network/mode with `raw|xhttp|ws|httpupgrade|grpc` networks); hand-added Ph2-only key in Advanced JSON saves → reopens preserved. Verification: automated test (preserve-not-strip). Priority: required.
- [ ] Scenario: chain Edit → Action: manage hops → Expected: kit ordered list + `Empty` when none + add/replace from library; illegal hops (chain→chain, extracore off-index-0) rejected naming the rule. Verification: automated test. Priority: required.
- [ ] Scenario: extra-core Edit → Action: Browse pick → Expected: kit path fills, args/config edit inline, Validate gates commit; no `no_logs` control anywhere. Verification: automated test. Priority: required.
- [ ] Prohibited side effect: no Throne `no_logs` bug copy; no hand-rolled list/picker divs; no invented kit APIs; no Ph2 matrix smuggling.

Retry cap: 2, then escalate.
