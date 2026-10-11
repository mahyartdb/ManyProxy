# Quickstart: config-completeness validation

Prereqs: `cargo run` debug build, isolated `MANYPROXY_DB` demo seed with synthetic
`*.example.com` rows only (never the live DB). No admin needed (loopback defaults).

## Q1 — AWG headline round-trip (US-1)

1. Add → protocol `amneziawg`; confirm WG fields + `enable_amnezia` + `awg_version 2.0|3.x` +
   Jc/Jmin/Jmax + S1–S4 + H1–H4 + I1–I5 + v3 fields + Random Trailers + Disable Cookies visible.
2. Import a `.conf` with reserved/keepalive/MTU; open Edit; confirm values sit in fields (not JSON-only).
3. Save; `Test` the row; expect field-naming errors only for genuinely missing credentials, and
   `sing-box check` green on the generated config.

## Q2 — VLESS/VMess/SS/Trojan parity (US-2/US-3)

1. Import one link of each type with non-default transport/TLS (ws+reality, grpc, 2022-blake3-ss).
2. Open each in Edit; confirm §6 fields + S1–S4 match the link; toggle network (ws→tcp→grpc) and
   confirm conditionals show/hide without losing stored values (reopen to verify).
3. Confirm SS Settings shows Mux-only; SS Advanced shows dial-only (no TLS section).

## Q3 — Hysteria/TUIC/WG/socks/http/anytls/naive/ssh (US-4/US-5)

1. Hysteria: toggle 1|2; confirm layouts swap; v1 blank up/down → commit refuses naming fields;
   v2 blank up/down → commits (BBR-by-omission).
2. TUIC/ssh/socks/http/anytls/naive: one Add per type; confirm CAP-9 field lists + pickers;
   ssh key-file Browse loads without hand-paste.

## Q4 — Snell/ShadowTLS/Xray-Ph1/chain/extra-core (US-6)

1. Snell/ShadowTLS: version combos + gating correct.
2. Xray: Ph1 fields present; hand-add a Ph2-only key in Advanced JSON → save → reopen → key preserved.
3. Chain: build a 2-hop chain via hop picker; attempt illegal hops (chain→chain, extracore at index 1)
   → rejections name the rule. Extra-core: Browse fills path; Validate gates commit.

## Q5 — Intake/row-ops + new-protocol fates (US-7/US-8)

1. Clipboard link → paste-intake opens Import prefilled with counts.
2. Clone a row → new disabled row, fresh auto port, identical params.
3. Share → copy link → import the copy → params identical modulo formatting.
4. Import each CAP-12 exotic type → guided page (if approved) or guidance error naming the fallback.

## Regression gates (every ticket)

`cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`,
`cargo test --locked` (incl. `assert_no_system_mutation`-family) green. Headless id/string asserts
for every touched panel. No real hosts/credentials in captures.
