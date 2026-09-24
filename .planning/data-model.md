# ManyProxy — Data Model (Phase 1)

## ProxyConfig

- `id` (stable string, never reused), `name` (non-empty), `protocol` (enum: shadowsocks | trojan | vmess | vless | tuic | hysteria | hysteria2 | anytls | wireguard | amneziawg-variant | ssh | xray-vless | custom-outbound | custom-config | extra-core | chained)
- `group` (string, default `default`), `tags[]`, `enabled` (bool), `endpoint_ref` (at most one active LocalEndpoint)
- `remote` (host, port 1–65535, plus protocol params as opaque map validated per protocol), `custom_json` (optional, schema-checked), `extra_core_path` (optional, must exist + be executable when used)
- Validation: name non-empty; host non-empty; port in range; custom JSON parses and names a known engine type; extra-core path exists.
- Transitions: created (disabled) → enabled → starting → running → stopping → stopped; any state → error (with named cause) → stopped.

## Subscription

- `id`, `label`, `url`, `group`, `auto_refresh` (bool), `last_refresh_at`, `last_error`
- Relationship: refresh upserts ProxyConfigs in its group; never deletes hand-created configs; conflicts keep user edits.
- Validation: URL must be http(s); refresh failure keeps prior members and records `last_error`.

## LocalEndpoint

- `config_id` (1:1 with running ProxyConfig), `bind_ip` (default loopback), `port` (unique across endpoints), `mode` (mixed socks5+http), `state` (starting | ready | failed)
- Validation: port unique + free at start; non-loopback bind requires explicit confirm flag; failure names conflicting config/process where detectable.
- Transitions: starting → ready | failed; ready → stopping → closed.

## HealthSample

- `config_id`, `at`, `kind` (url-test | tcp-ping), `latency_ms` (null on failure), `error` (null on success)
- Rule: samples append-only; list shows latest per config.

## LogRecord

- `at`, `config_id` (nullable for engine-global), `level`, `message` (secrets redacted)
- Rule: per-config filter shows only matching `config_id`; export redacts secrets.
