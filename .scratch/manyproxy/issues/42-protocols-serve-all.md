# 42: Protocols serve-everything

**What to build:** Zero dead rows: native 1.14.2 emission for every importable type where an upstream schema exists (socks/http outbounds, hy2 BBR-by-omission, realm mapping, quic transport, WG conf key parity, trojan mapping, HY2/TUIC extras validated), loud import-time rejection with guidance where no schema exists, plus intake gaps (v2rayn lines, bare SIP008 lists, Clash flow/inline/Meta handling) and the route envelope (urlsafe/raw + kind, proxy alias, warp guidance, remoteroute refresh).

**Blocked by:** None (can start immediately).

**Owns:** backend-patterns

**Status:** ready-for-agent

- [ ] Scenario: socks/http share link → Action: import then start → Expected: serves via native sing-box outbound JSON; `sing-box check` EXIT 0. Verification: automated test. Priority: required.
- [ ] Scenario: hysteria2 link without up/down → Action: start → Expected: omits both fields (BBR) and serves. Verification: automated test. Priority: required.
- [ ] Scenario: hysteria2+realm link → Action: import → Expected: serves via realm block or import refuses with guidance; never parse-ok/start-fail. Verification: automated test. Priority: required.
- [ ] Scenario: vmess/vless/trojan with quic transport or ech/reality extras → Action: start → Expected: 1.14.2 transport/tls JSON emitted, or typed bad-option naming the field; no 1.15-only fields ever emitted. Verification: automated test. Priority: required.
- [ ] Scenario: mieru/juicity/snell/shadowtls/masque/openvpn/trusttunnel link → Action: import → Expected: serves natively where upstream schema exists, else import fails loudly naming the gap; never start-fail. Verification: automated test. Priority: required.
- [ ] Scenario: WG `.conf` with reserved/fm/keepalive/dns/mtu → Action: import → Expected: identical key survival as URI intake. Verification: automated test. Priority: important.
- [ ] Scenario: trojan allowInsecure / HY2-TUIC wild extras → Action: start → Expected: mapped (e.g. tls.insecure) or typed bad-option; `check` never dies cryptically. Verification: automated test. Priority: important.
- [ ] Scenario: sub payload with v2rayn lines / bare SIP008 list / Clash flow-mappings / inline JSON / Meta types → Action: refresh → Expected: members land or per-item skips name the reason. Verification: automated test. Priority: important.
- [ ] Scenario: throne route link in standard-b64, urlsafe-b64, raw JSON → Action: import → Expected: preset stores in all three encodings; kind envelope checked when present. Verification: automated test. Priority: required.
- [ ] Scenario: preset using `proxy` action / `warp` action → Action: attach + start → Expected: `proxy` serves as route alias; `warp` yields guidance error naming the missing endpoint design. Verification: automated test. Priority: required.
- [ ] Scenario: remoteroute link → Action: explicit refresh → Expected: fetch works with fetch-time re-check, or the refresh claim is removed from UI copy. Verification: automated test. Priority: important.
- [ ] Prohibited side effect: no emitted config may contain 1.15.0-only fields; no parse-ok/start-fail rows remain.
