# sing-box 1.14.2 serve-side research (2026-09-28)

Target: sing-box **1.14.2** (patch on 1.14.x; GitHub release notes say only "Fixes and improvements").
Docs base: `https://sing-box.sagernet.org/`. Docs pages carry "Changes/Since" banners — anything marked
"Since 1.15.0" does **NOT** exist in 1.14.2. All claims below cite their page.

## 1. socks outbound (`type: socks`)

- Required: `server`, `server_port`. Optional: `version` (`4`|`4a`|`5`, default `5`),
  `username`, `password`, `network` (`tcp`|`udp`, both by default), `udp_over_tcp`, plus Dial Fields.
- Source: https://sing-box.sagernet.org/configuration/outbound/socks/

```json
{ "type": "socks", "tag": "socks-out", "server": "127.0.0.1", "server_port": 1080 }
```

- Version notes: no 1.14.x/1.15.x banner on this page — schema stable for 1.14.2.

## 2. http outbound (`type: http`)

- Required: `server`, `server_port`. Optional: `username`, `password` (Basic auth),
  `path`, `headers`, `tls` (shared TLS outbound block). Plus Dial Fields.
- **1.14.2 does NOT have**: `version`, `disable_version_fallback`, HTTP2 Fields, QUIC Fields —
  all marked "Since sing-box 1.15.0" (default would be v2; v1 only if `path`/`Host` set; v3 swaps
  HTTP2 fields for QUIC fields). Do not emit these for 1.14.2.
- Source: https://sing-box.sagernet.org/configuration/outbound/http/

```json
{ "type": "http", "tag": "http-out", "server": "127.0.0.1", "server_port": 8080,
  "username": "sekai", "password": "admin", "tls": { "enabled": true, "server_name": "example.com" } }
```
(`tls` omitted for plain HTTP.)

## 3. QUIC transport mapping (two different things)

- (a) **V2Ray transport** `{"type": "quic"}` — no extra fields, no added encryption
  (docs: "No additional encryption support… basically duplicate encryption").
  Used inside vmess/vless/trojan `transport` objects.
  Source: https://sing-box.sagernet.org/configuration/shared/v2ray-transport/ (`### QUIC` section).
- (b) **Shared QUIC Fields** (`initial_packet_size`, `disable_path_mtu_discovery`, plus HTTP2 Fields) —
  embedded in hysteria2 / http-out v3 / masque-client, **Since sing-box 1.14.0**.
  Source: https://sing-box.sagernet.org/configuration/shared/quic/
- Minimal transport example: `"transport": { "type": "quic" }`.

## 4. hysteria2 outbound (`type: hysteria2`) — up/down defaults

- Required: `server` (+`server_port`, unless `server_ports` ranges), `tls` (required).
  Optional: `server_ports` (1.11.0+), `hop_interval` (default `30s`, 1.11.0+),
  `up_mbps`/`down_mbps`, `obfs` (`type`: `salamander`|`gecko` + `password`), `password`, `network`.
- **up/down defaults**: docs state "If empty, the BBR congestion control algorithm will be used
  instead of Hysteria CC." So default = **omitted/empty → BBR**, no numeric default. Omit both fields
  unless the user sets explicit Mbps.
- 1.14.0 additions (present in 1.14.2): `hop_interval_max` (randomized hop range),
  `obfs.min_packet_size` (default 512) / `obfs.max_packet_size` (default 1200, gecko only),
  `bbr_profile` (`conservative`|`standard`|`aggressive`, default `standard`),
  `disable_chrome_parrot` (default false; Chrome QUIC parrot fixes `idle_timeout`=30s and overrides
  some QUIC params; Ed25519 certs fail with parrot — use ECDSA/RSA), `realm` rendezvous block
  (conflicts with `server`/`server_port`/`server_ports`).
- Source: https://sing-box.sagernet.org/configuration/outbound/hysteria2/

```json
{ "type": "hysteria2", "tag": "hy2-out", "server": "127.0.0.1", "server_port": 443,
  "password": "goofy_ahh_password", "tls": { "enabled": true, "server_name": "example.com" } }
```

## 5. snell outbound (`type: snell`) — NEW in 1.14.0

- Required: `server`, `server_port`, `version` (`4`|`6`), `psk`.
  Optional: `userkey`, `reuse`, `network` (both by default).
  v4-only: `obfs_mode` (`none` default|`http`), `obfs_host` (default `bing.com`).
  v6-only: `mode` (`default`|`unshaped`|`unsafe-raw`, default `default`; v6 `psk` must be 12–255 bytes).
- No v5/wire-QUIC: docs note v5 wire == v4 (QUIC proxy mode intentionally unsupported).
- Source: https://sing-box.sagernet.org/configuration/outbound/snell/

```json
{ "type": "snell", "tag": "snell-out", "server": "127.0.0.1", "server_port": 1080,
  "version": 4, "psk": "password" }
```

## 6. shadowtls outbound (`type: shadowtls`)

- Required: `server`, `server_port`, `tls` (required). Optional: `version` (`1` default|`2`|`3`),
  `password` (v2/v3 only). Plus Dial Fields.
- Source: https://sing-box.sagernet.org/configuration/outbound/shadowtls/

```json
{ "type": "shadowtls", "tag": "st-out", "server": "127.0.0.1", "server_port": 1080,
  "version": 3, "password": "fuck me till the daylight",
  "tls": { "enabled": true, "server_name": "example.com" } }
```

## 7. mieru / juicity — NOT in upstream sing-box (1.14.2 or current docs)

- Outbound type table (https://sing-box.sagernet.org/configuration/outbound/) lists:
  direct, bridge, block, socks, http, shadowsocks, vmess, trojan, wireguard (deprecated/removed path),
  hysteria, shadowtls, vless, tuic, hysteria2, anytls, snell, tailcat, tor, ssh, dns, selector, urltest, naive.
  **No `mieru`, no `juicity`.**
- mieru exists only in third-party fork `enfein/mbox` ("A fork of sing-box with mieru protocol support").
- juicity exists only as feature request SagerNet/sing-box#788 ("Add juicity support"), never merged.
- Implication: no JSON schema to emit; route mieru/juicity elsewhere or mark unsupported.

## 8. openvpn — endpoint only, NO outbound type

- `openvpn-client` is an **endpoint** (`"type": "openvpn-client"` in `endpoints[]`),
  Since 1.14.0 → present in 1.14.2. No `openvpn` outbound exists.
- Minimal shape: one of `server`+`server_port` **or** `servers[]` required; TLS mode (`mode: tls`
  default) requires `tls` block with one of `tls.certificate`/`tls.certificate_path`/`tls.peer_fingerprint`;
  full option surface (cipher/auth/mss/fragment/replay/compression/pull_filters/routes/ping/renegotiate,
  `system`/`name`/`mtu`, `on_demand` is 1.15.0+) per docs.
- Source: https://sing-box.sagernet.org/configuration/endpoint/openvpn-client/

```json
{ "type": "openvpn-client", "tag": "ovpn", "server": "127.0.0.1", "server_port": 1194,
  "username": "u", "password": "p",
  "tls": { "certificate_path": "/etc/openvpn/ca.crt", "server_name": "vpn.example.com" } }
```

## 9. masque — endpoint only AND 1.15.0+, absent from 1.14.2

- `masque-client` endpoint (CONNECT-IP, RFC 9484) is marked **"Since sing-box 1.15.0"**
  (default HTTP `version: 3`, `mtu: 1280`, optional `tls`, `advertise_routes`, `system`/`name`).
  **Do not emit for 1.14.2 targets.**
- Source: https://sing-box.sagernet.org/configuration/endpoint/masque-client/

## 10. ECH / TLS options (outbound-relevant, 1.14.2)

Base page: https://sing-box.sagernet.org/configuration/shared/tls/

- Outbound `tls` block: `enabled`, `engine` (`go` default|`apple`|`windows`; **1.14.0+**; non-go engines
  support only a subset — no disable_sni/cipher_suites/curve_preferences/client certs/fragment/ech/utls/reality),
  `disable_sni`, `server_name`, `insecure`, `alpn`, `min_version`/`max_version` (default 1.2–1.3),
  `cipher_suites` (TLS 1.0–1.2 only), `curve_preferences` (1.13.0+; default P256/P384/P521/X25519/X25519MLKEM768),
  `certificate`/`certificate_path`, `certificate_public_key_sha256` (1.13.0+),
  client cert/key fields (1.13.0+), `fragment`/`record_fragment`+`fragment_fallback_delay` (1.12.0+,
  plaintext-packet-matching firewalls only), `spoof`/`spoof_method` (**1.14.0+**, needs CAP_NET_RAW/root/Admin),
  `handshake_timeout` (**1.14.0+**, default 15s), `kernel_tx`/`kernel_rx` (1.13.0+, Linux 5.1+, TLS1.3 only).
- **NOT in 1.14.2**: `certificate_sha256` pinning (1.15.0+).
- **ECH (outbound client)**: `ech: { enabled, config | config_path, query_server_name (1.13.0+) }`;
  empty config → HTTPS-record DNS lookup; `key`/`key_path` are server-side only;
  generate via `sing-box generate ech-keypair`. QUIC supports ECH only.
  (`pq_signature_schemes_enabled`, `dynamic_record_sizing_disabled` removed in 1.13.0.)
- **uTLS**: `utls: { enabled, fingerprint }` — fingerprints chrome|firefox|edge|safari|360|qq|ios|android|
  random|randomized (chrome default; legacy chrome_* variants removed in 1.10.0 → fall back to chrome).
  Docs explicitly discourage uTLS (repeated fingerprinting vulns, unmaintained); recommend NaiveProxy instead.
- **Reality (client)**: `reality: { enabled, public_key (required), short_id (required, 0–8 hex digits) }`.
- **ACME inline (`tls.acme`) deprecated in 1.14.0**, removal slated 1.16.0 → use `certificate_provider`
  (tag string or inline object) instead. Source: https://sing-box.sagernet.org/deprecated/,
  https://sing-box.sagernet.org/migration/#migrate-inline-acme-to-certificate-provider

```json
{ "tls": { "enabled": true, "server_name": "example.com", "alpn": ["h2"],
  "ech": { "enabled": true }, "utls": { "enabled": false } } }
```

## 11. Custom full-config inbound parsing

- Top-level config is fixed-shape JSON: `{ $schema, log, dns, ntp, certificate,
  certificate_providers, http_clients, network_namespaces, endpoints, inbounds, outbounds,
  route, services, experimental }`. Validate with `sing-box check`; normalize with
  `sing-box format -w -c config.json -D dir`; combine fragments with `sing-box merge`.
  Source: https://sing-box.sagernet.org/configuration/
- `inbounds[]` items are `{ type, tag, ...Listen Fields, ...type-specific }`.
  Known types (no mieru/juicity/masque/openvpn inbound): direct, mixed, socks, http, shadowsocks,
  vmess, trojan, naive, hysteria, shadowtls, tuic, hysteria2, vless, anytls, snell, tun, redirect,
  tproxy, cloudflared, tailcat. `shadowtls`/`vless`/etc. inbounds are TCP-injectable via `detour`.
  Source: https://sing-box.sagernet.org/configuration/inbound/
- Listen Fields: `listen` (required), `listen_port`; 1.12.0+: `bind_interface`, `routing_mark`,
  `reuse_addr`, `netns` (1.14.0+: may be a network-namespace tag); `tcp_fast_open`, `tcp_multi_path`,
  `disable_tcp_keep_alive`/`tcp_keep_alive` (default 5m)/`tcp_keep_alive_interval` (default 75s),
  `udp_fragment`, `udp_timeout` (default 5m), `detour`. Legacy sniff/domain_strategy fields deprecated
  since 1.11.0 (use route rule actions). Source: https://sing-box.sagernet.org/configuration/shared/listen/
- Minimal passthrough examples (validate `type`+`tag`, keep unknown fields verbatim, run `sing-box check`):
  - mixed: `{"type":"mixed","tag":"mixed-in","listen":"127.0.0.1","listen_port":1080}` (+optional
    `users[]`, `set_system_proxy`). Source: https://sing-box.sagernet.org/configuration/inbound/mixed/
  - socks: `{"type":"socks","tag":"socks-in","listen":"127.0.0.1","listen_port":1080}` (+optional `users[]`).
    Source: https://sing-box.sagernet.org/configuration/inbound/socks/
  - http inbound in 1.14.2: `{"type":"http","tag":"http-in","listen":…,"listen_port":…}` (+`users[]`,
    `tls`, `set_system_proxy`); **`version`/HTTP2/QUIC serve options are 1.15.0+ — reject/ignore on 1.14.2.**
    Source: https://sing-box.sagernet.org/configuration/inbound/http/

## 12. 1.14.x version notes (serve-relevant deltas)

- 1.14.0 (all in 1.14.2): snell outbound+inbound added; openvpn-client endpoint added; shared QUIC Fields
  added; TLS `engine`, `handshake_timeout`, `spoof`/`spoof_method`, `certificate_provider` added;
  hysteria2 `hop_interval_max`/`bbr_profile`/`disable_chrome_parrot`/`realm`/gecko packet-size bounds added;
  inline `tls.acme` deprecated (→ certificate provider, removal 1.16.0).
- 1.14.1 / 1.14.2: release notes list only "Fixes and improvements" — no schema changes.
  Source: https://github.com/SagerNet/sing-box/releases/tag/v1.14.2
- 1.15.0 (DO NOT USE for 1.14.2): http outbound/inbound `version`+HTTP2/QUIC serve, masque-client endpoint,
  `certificate_sha256` pinning, TUN stack change.
