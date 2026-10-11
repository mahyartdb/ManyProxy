# 05: Hysteria v1/v2 + TUIC pages

**What to build:** Hysteria version-switch layout (v1 auth-family vs v2 password, v1-only up/down gate) and the 7-field TUIC page with forced-TLS — all kit primitives.

**Blocked by:** 04-wireguard-amneziawg (same write surface — linear).

**Owns:** gpui-kit composition (kit-maximal: `Select`/`RadioGroup`/`Switch`/`Input`/`Field`), sqlite (fold/validate, no migration).

**Status:** ready-for-agent

- [ ] Scenario: Hysteria Add → Action: toggle kit version 1|2 → Expected: v1-only (auth_type+auth/recv_window(_conn)/disable_mtu_discovery) vs v2-only (password) layouts swap as kit primitives; common (server_ports/hop_interval/up/down/obfs) persist. Verification: automated test (layout-toggle hide-preserves). Priority: required.
- [ ] Scenario: v1 commit blank up/down → Action: save → Expected: typed error names up_mbps/down_mbps; v2 blank up/down commits (BBR-by-omission). Verification: automated test. Priority: required.
- [ ] Scenario: TUIC Edit → Action: read form → Expected: uuid + password + cc (`bbr|cubic|new_reno`) + relay (`native|quic`) + udp_over_stream + zero_rtt + heartbeat + S2-forced + S4, all kit. Verification: automated test. Priority: required.
- [ ] Prohibited side effect: no hand-rolled toggles/dropdowns; no invented kit APIs; no new probe paths.

Retry cap: 2, then escalate.
