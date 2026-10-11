# Data model: config-completeness (no storage changes)

No SQLite schema changes. No new tables/columns. No `user_version` bump. Params JSON stays the
row source of truth (DPAPI-sealed, strict per-protocol validation unchanged).

## Capability flags (new, in-memory only)

| Protocol (canonical) | transport (S1) | TLS optional (S2) | TLS forced | mux+brutal (S3) | notes |
| --- | --- | --- | --- | --- | --- |
| vless, vmess, trojan | yes | yes | no | yes | mux auto-off when `flow=xtls-rprx-vision` |
| http, https | no | yes | no | no | + parity+ path/headers |
| shadowsocks | no | no | no | yes | mux-only Settings |
| hysteria, hysteria2, tuic, anytls, naive, shadowtls | no | no | yes | no | security locked `tls` |
| socks, wireguard, amneziawg, ssh, snell, noise | no | no | no | no | inner fields + S4 only |
| xray | xray-panels | xray-panels | no | xray-mux | Ph1 subset (`raw\|xhttp\|ws\|httpupgrade\|grpc` networks); Ph2 separate |
| custom-outbound, custom-config, extra-core, chain, direct | no | no | no | no | JSON-first / list / trivial (`direct` new in this program) |

## UI state (new, view-layer only — `ConfigForm` extensions)

| State | Shape | Rule |
| --- | --- | --- |
| protocol pick | `Select` over 23 canonical types, locked on edit | typo-proof; pristine-template-only rewrite on switch |
| per-protocol inner fields | typed inputs per §6 page spec (text/numeric/combo/toggle/textarea) | blank leaves JSON byte-identical; validation names fields |
| S1 transport | network `Select` + conditional inputs | hide preserves JSON (R5) |
| S2 TLS (+forced) | toggle (optional only) + sni/alpn/fp/insecure/pbk/sid/fragment/rec-frag/tricks | non-empty pbk ⇒ REALITY; forced variant has no toggle |
| S3 mux+brutal | mode `Select`/`RadioGroup` + brutal toggle + 2 speeds | brutal-on forces mux On; vision-flow disables with reason |
| S4 advanced dial | 4 toggles + 4 timeout/bind combos + TLS-extras (cert/sha256/ech/client) | TLS-extras only when page has TLS (HasTLS gating) |
| xray Ph1 | uuid/enc/flow + sni/fp/alpn/pbk/sid/spiderx/network/mode + mux | unknown `xray_*` preserved byte-identical |
| chain hops | ordered `Vec<RowId>` + select-mode picker | Throne rejection rules verbatim |
| file picks | path strings (ssh key, extra-core exe, cert blobs) | Browse fills; contents never echoed in errors beyond path |

## Validation (extends `validate_protocol_params`, same strictness shape)

- Required-field gates per page (esp. v1-hysteria up/down; v2 optional for BBR; AWG version-gated
  fields; ssh username + password-or-key) with field-naming errors before any sidecar spawn.
- Numeric ranges (ports 1–65535, aid, timeouts, J/S/H/I ints, speeds Mbps) typed; comma-list fields
  (alpn, host_key_algs, reserved, local_addr) parsed with per-item errors.
- Preserved-keys rule: hidden/Ph2-only keys round-trip untouched and still validate. Import-only
  extras (`vmess.global_padding/authenticated_length`, `wireguard.udp_timeout`) stay preserved,
  never stripped, no UI.
- Version gates (verified): socks version 5-only; snell 4|6; shadowtls 1|2|3; tuic relay
  `native|quic`; naive-cc 4 vs trusttunnel-cc 6 vs tuic-cc 3 — conflation is a spec violation.

## Relationships / transitions

- Row lifecycle unchanged (disabled → enabled → starting → running …).
- Dialog: Add (type pickable) → commit (fold guided+S1–S4 into params, validate, save disabled) ;
  Edit (type locked) → same fold/validate → restart-to-apply prompt (existing behavior).
