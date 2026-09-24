# Research 01 — sing-box N-endpoint scale (primary: sing-box docs)

**Decision this informs:** one combined engine config serves N local endpoints.
**Confidence:** high.

## TL;DR

N `mixed` inbounds (unique tag + `127.0.0.1:port`) + N outbounds/`endpoints` + N `route.rules[]` (`inbound` → `action:route` → `outbound`) + `route.final` catch-all. No TUN, omit `set_system_proxy`. No documented count caps.

## Key findings

1. Top-level shape `{endpoints[], inbounds[], outbounds[], route{}}`. Fact. Source: https://sing-box.sagernet.org/configuration/
2. `mixed` = socks4/4a/5 + HTTP on one port; `socks` and `http` splits exist. Fact. Sources: .../configuration/inbound/, .../inbound/mixed/, .../inbound/socks/, .../inbound/http/
3. Per-inbound bind via `listen` (required) + `listen_port` (Listen Fields). Fact. Source: .../configuration/shared/listen/
4. `set_system_proxy` defaults false; `socks` inbound has no such field. Fact. Sources: .../inbound/mixed/, .../inbound/http/
5. Route by inbound tag: `route.rules[].inbound` = inbound tags. Fact. Source: .../configuration/route/rule/
6. Route action form `{"action":"route","outbound":"<tag>"}`; bare `outbound` deprecated 1.11. Fact. Sources: .../route/rule_action/, .../route/rule/
7. `route.final` catch-all; empty = first outbound. Fact. Source: .../configuration/route/
8. WireGuard client is `endpoints[]` since 1.11; `wireguard` outbound deprecated, removed 1.13. Fact. Sources: .../configuration/endpoint/, .../endpoint/wireguard/, .../outbound/wireguard/, .../migration/, .../deprecated/
9. CLI `sing-box check` / `format` / `merge` / `schema`. Fact. Sources: .../configuration/, .../configuration/schema/
10. No count caps documented on any array. Fact (negative search across configuration/inbound/outbound/endpoint/route/listen/migration/deprecated/schema).

## Pattern (2 pairs; scale by repetition, unique tags + ports)

```json
{
  "inbounds": [
    {
      "type": "mixed",
      "tag": "in-1",
      "listen": "127.0.0.1",
      "listen_port": 2080
    },
    {
      "type": "mixed",
      "tag": "in-2",
      "listen": "127.0.0.1",
      "listen_port": 2081
    }
  ],
  "outbounds": [
    {
      "type": "shadowsocks",
      "tag": "out-1",
      "server": "127.0.0.1",
      "server_port": 1080,
      "method": "aes-128-gcm",
      "password": "pass1"
    },
    {
      "type": "shadowsocks",
      "tag": "out-2",
      "server": "127.0.0.1",
      "server_port": 1081,
      "method": "aes-128-gcm",
      "password": "pass2"
    }
  ],
  "route": {
    "rules": [
      { "inbound": "in-1", "action": "route", "outbound": "out-1" },
      { "inbound": "in-2", "action": "route", "outbound": "out-2" }
    ],
    "final": "out-1"
  }
}
```

WireGuard upstreams live in `endpoints[]`; reference endpoint tag as rule `outbound`.

## Gaps

- Tag-uniqueness stated as "the tag of..." only; enforce via `sing-box check`.
- No explicit "N×127.0.0.1 supported" sentence; composition of documented primitives.
- CLI flag details thin; `/manual/` 404s.
