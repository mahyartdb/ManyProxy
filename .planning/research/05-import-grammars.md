# Research 05 — Import grammars (primary: Xray #716, v2rayN, SIP002/008, Hysteria, sing-box, Clash, Throne deeplinks)

**Decision this informs:** parser scope, field tables, error taxonomy, refresh semantics.
**Confidence:** high on grammars; refresh merge is a proposal (upstream unspecified).

## Share links

- Canonical: `protocol://uuid@host:port?params#remark`, unordered unique params, percent-encoded values, case-sensitive names. Source: XTLS/Xray-core#716.
- VLESS `vless://uuid@host:port?...`: uuid 1–30 chars or GUID; port 1–65535; IPv6 `[]`; `encryption` (`none` vs PQ string) ≠ VMess `security`; `flow`; `type` (tcp/raw|kcp|ws|http|grpc|httpupgrade|xhttp; unknown → raw/tcp); `security` (none|tls|reality); `sni` (def host, required w/ TLS); `fp` (R w/ reality); `alpn`; `pbk/sid/spx/pqv/ech/pcs/vcn/fm`; per-transport path/host/serviceName/mode/seed/mtu/quic/xhttp extras. Sources: #716, v2rayN VLESSFmt/BaseFmt, INCY share-links.
- VMess dual: (A) legacy `vmess://base64(json)` dominant (`add/port/id/ps` R; `v/scy/aid/net/type/host/path/tls/sni/fp/alpn/insecure/vcn/pcs`); (B) new URI like VLESS with `encryption`=security set. Sources: v2rayN VMess wiki, INCY.
- Trojan `trojan://password@host:port?...`: base trojan-gfw + VLESS-compatible extras; `security` def `tls`; `peer` SNI fallback; wild `allowInsecure/headerType`. Sources: trojan-url, INCY, v2rayN TrojanFmt.
- SS triple grammar: legacy `ss://base64(method:password@host:port)`; SIP002 `ss://userinfo@host:port?plugin=` (userinfo = base64url or plain for AEAD-2022); modern-split `ss://base64(method:password)@host:port`. Validate address/port/method/password non-empty; AEAD-2022 split on first colon. Sources: v2rayN ShadowsocksFmt, SIP002.
- Hysteria2 `hysteria2://|hy2://[auth@]host[:port]/?…#name` (port om→443; multi-port first-wins; `sni/insecure/pinSHA256/ech/alpn/obfs/obfs-password/mport/hopInt`; `up/down` never in URI; realm variant `hysteria2+realm://`). V1 legacy `hysteria://` separately. Sources: v2 Hysteria URI docs, v1 URI docs, v2rayN notes.
- TUIC `tuic://uuid:password@host:port?...` (first-colon split; `congestion_control/udp_relay_mode/udp_over_stream/zero_rtt/heartbeat`); Clash v4 token vs v5 uuid+password. Sources: v2rayN TuicFmt, sing-box tuic docs, Meta-Docs tuic.
- WireGuard URI (private_key userinfo; `publickey` R; `presharedkey/address/mtu/dns/reserved/fm`) + `.conf` (`[Interface] PrivateKey` R, `[Peer]` → one profile each w/ Endpoint R). Sources: v2rayN WireguardFmt.
- Cheap others: `socks://`, `anytls://` (+required tls), `http(s)://`, `naive+https://`; Mieru/Juicity/ShadowTLS/SSH/AmneziaWG/Snell via `throne://add` JSON only.

## Target schema (sing-box outbound field names)

`{type,tag,server,server_port,+Dial}`; per-type required/optional tables for vless/vmess/trojan/ss/wireguard(deprecated→endpoint)/hysteria2/hysteria/tuic/anytls + `tls{}` + `transport{}` (no raw/tcp/mkcp/domainsocket; ws early-data Xray-compat header). Sources: sing-box outbound + shared tls/transport pages.

## File/subscription bodies

Accept: sing-box outbounds array or full config; Xray full JSON array; SIP008 (`version/servers[]`, HTTPS + JSON content-type; bare list tolerated); v2rayN internal `v2rayn://`; Clash YAML `proxies[]` limited (groups/rules ignored). Container detection order per V2Fly (Base64URLLine/JsonFieldArray/Yaml2Json/URLLine/DataURLSingle). Sources: Throne README/intro, deeplinks, SIP008, v2rayN subscription wiki, Clash refs, V2Fly subscription docs.

## `throne://` links

`throne://<cmd>/<base64>`, case-insensitive cmd, no query params, missing payload drops, unknown cmd logs. `addsub` (standard-b64 only, `<url>[#group]`, confirm, auto-update def on); `remoteroute` (http(s) lines only, one auto-update answer); `route` (envelope `kind=throne-route-profile v=1`, urlsafe/standard/raw accepted, legacy bare array opens editor); `add` (single outbound JSON, no confirm, current group). Source: deeplinks docs.

## Error taxonomy

Bad base64 (incl. wrong alphabet) → reject/silent-drop; unknown scheme/cmd → skip+count/log; bad host/port (∅/0/range, IPv6) → reject (hy2 om→443 exception); missing credential (uuid/password/keys/pbk-for-reality) → reject; invalid JSON → route-parse error / keep-old-group; empty payload → silent drop/empty-link message; ss plugin/mux fail → reject item; Clash dup names/opts-without-network → dedupe/inert; non-UTF8/trailing-slash ss → trim/lenient then reject on FormatException.

## Refresh semantics (PROPOSAL — upstream unspecified)

Upsert keyed by normalized identity (type+server+port+credential+method+sni/path), not remark. New adds, changed updates in place, vanished removes only subscription-owned rows. No-op on identical body. Hand-edits as overlay (remark/tags/enabled/manual) never overwritten; deleted rows tombstoned. Atomic per-group swap; per-item failures skip + count.
