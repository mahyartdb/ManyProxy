# Research 02 — AmneziaWG / Noise / Xray / custom cores (primary: upstream issues, forks, Throne)

**Decision this informs:** exotic protocols need fork-core and/or sidecars, never native schema.
**Confidence:** high.

## TL;DR

Upstream sing-box = plain WireGuard only. AmneziaWG/Noise, XHTTP, VLESS encryption, Xray finalmask/noise are fork-only or sidecar-only. Throne model: stock sing-box + bundled Xray-core sidecar + user Extra Core exe, bridged over loopback ingress listeners, with egress-only chaining rules.

## Key findings

1. Native: plain WireGuard fields only, no `jc/jmin/jmax/s/h/i`. Fact. Source: .../configuration/outbound/wireguard/
2. Native transports: HTTP/WS/QUIC/gRPC/HTTPUpgrade; no `xhttp`. Fact. Source: .../configuration/shared/v2ray-transport/
3. Native VLESS: `uuid/flow/tls/transport/packet_encoding/multiplex`; no `encryption` PQ field. Fact. Sources: .../configuration/outbound/vless/, https://github.com/SagerNet/sing-box/issues/3599
4. AmneziaWG 2.0 upstream: closed not-planned (#4045); history #1928 wontfix, #2557 refusal, PR #2670 unmerged, #3159 closed. Fact. Sources: issue URLs.
5. XHTTP upstream: open PR #4326 only; vanilla "doesn't know about xhttp". Fact. Sources: PR URL, https://github.com/justinwoo280/sing-xhttp
6. Forks: `amnezia-vpn/amnezia-box` (AWG endpoint); `hoaxisr/amnezia-box` (AWG 2.0 H/S/I ranges); `Leadaxe/sing-box-lx` (AWG 2.0/3.x fields + XHTTP + native chain, tags `with_awg/with_xhttp/with_lx_chain`); runtime graft `Leadaxe/wireguard-go-awg2-lx`. Fact. Sources: repo URLs + releases.
7. Param semantics (Jc/Jmin/Jmax, S1-S4, H1-H4 non-overlap, I1-I5 chains, 3.1 HeaderProtectionKey/padding/trailers/timings). Fact. Source: https://docs.amnezia.org/documentation/amnezia-wg
8. "Amnezia Noise" ≠ Xray Noise: AWG obfuscation must match AWG server; Xray finalmask/noise decoys work with any UDP but both ends share finalmask. Inference from specs. Sources: amnezia-wg docs, https://xtls.github.io/en/config/transports/finalmask.html, https://xtls.github.io/en/config/outbounds/freedom.html, Xray PR #5657, issue #4372 recipe.
9. Throne: README credits sing-box + Xray-core; protocol list incl. AmneziaWG, Xray VLESS, custom outbound/config both cores, chaining, Extra Core. Fact. Sources: Throne README, release notes (1.1.5 v2 schematics, 1.2.2 v3, 1.3.0 update + finalMask + OpenVPN/MASQUE/Snell/Gecko).
10. Sidecar bridge: generated sing-box config + separate Xray config joined by loopback ingress listeners; breakage signature `xray ingress tags size does not match bridge count!`. Fact. Sources: issue #1464, releases 1.1.3/1.1.5, commit af498cb.
11. Chain rules (≥1.1.3): Xray chaining allowed; Extra/Custom/Xray egress-only; violation error `Cannot use Extracore or Custom or Xray configs in a chain`. Fact. Sources: 1.1.3 notes, issues #1226, #1399, #1383.
12. Custom JSON: standalone single-outbound merge or full-config replacement per core; chained as nodes subject to egress rule; `throne://add/<base64>` = one outbound JSON. Fact. Source: deeplinks docs.
13. Extra Core ops: user exe + config, unprivileged, GUI dir, unix socket/named pipe; excluded from group tests; admin-mode conflict. Fact. Sources: 4.3.5 notes, 1.1.3 note 2, issue #1482.

## Gaps / risks

- AWG version skew (2.0 vs 3.x sub-params); GUI schematics historically lagged.
- Fork XHTTP `stream-one`/`auto` bugs; sidecar XHTTP pins Xray version.
- VLESS PQ encryption is Xray-sidecar-only; downstream kernels may strip it.
- Silent mismatch (wrong-layer obfuscation connects nowhere); `reserved=[0,0,0]` + case-sensitive I-tags footguns.
- Ops: TUN-exclusion/loopback entries, AV flags, macOS quarantine/setuid.
