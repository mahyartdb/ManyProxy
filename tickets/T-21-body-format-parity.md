# T-21 — Body format parity (`json://` fragments + WireGuard INI inside subscription bodies)

- **id**: T-21
- **title**: Import surface parity — accept `json://` fragments and WireGuard INI inside subscription bodies (WG `.conf` currently files-only)
- **epic**: E5 — Subscription & exchange pack
- **size**: S
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E5 T-21 line; gap §4.4 S5 (Throne §D.1 body formats include `json://` inline fragments + WG INI in sub bodies; "ManyProxy accepts WG `.conf` from files (`looks_like_wg_conf`, import.rs:2250) but not inside subscription bodies [C]"); verified entry point `import_subscription_body` (import.rs:3105, consumed by `refresh_with_body_running` store.rs:5218).

## Why

Subscriptions that mix media — a raw vless link sitting next to a `json://`-prefixed inline config object, or a bare WG INI block — are shareable content today (foreign panels emit both). Every *parser primitive* to handle both already exists in the file path; the fix is plumbing both into the subscription-body dispatch, which is pure intake work with zero new network/OS surface.

## What

1. **`json://` inline fragments** — in `import_subscription_body` (import.rs:3105): lines/segments of the form `json://<b64|urlsafe-b64>[#name]` decode (same base64-tolerance family the preset/paste paths use — std and urlsafe, BOM-stripped, `PresetError::BadBase64`-class tolerance precedent) and feed the decoded JSON object into the existing single-config JSON intake; failure to decode/parse produces an `ImportFailure` with the line index (never silent, never drops the whole body).
2. **WG INI in bodies** — before scheme-line parsing, run `looks_like_wg_conf` (import.rs:2250) on candidate segments: a body containing a WG INI block routes that segment through the same WG importer used by files, producing one `wireguard` row from the single-peer config. Multi-WG-in-one-body = one row per INI block (bodies with WG + other lines process each).
3. **No semantic change on existing formats** — base64 blobs, JSON arrays, SIP008, Clash YAML, per-line links all keep today's behavior and tests.
4. Failures naming: "line N: <reason>" consistent with existing batch failure strings; `refresh_with_body_running`'s `batch.failures` path renders as today (no new report machinery).

## Files to touch

- `src/import.rs` — `import_subscription_body` dispatch (`json://` + WG INI segment detection + failure plumbing) + tests
- no changes to store/engine/health/main behavior (failures already render from `RefreshOutcome.failures`)

## Acceptance criteria

- [ ] `json://`-prefixed inline objects in bodies import as rows (std + urlsafe base64; name hint from `#name` honored; failure keeps the whole refresh at today's semantics — other lines still import and failures are reported, never silent).
- [ ] WG INI blocks inside bodies import as `wireguard` rows (params verified identical to the same content imported from a file — cross-path parity test).
- [ ] Existing body-format tests all pass unchanged (blob/array/SIP008/Clash/multi-JSON).
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t71_`, verified unused): `t71_json_fragment_imports_std_b64`, `t71_json_fragment_imports_urlsafe_b64`, `t71_json_fragment_failure_reports_line_index`, `t71_wg_ini_in_body_imports_one_row_per_block`, `t71_wg_ini_file_vs_body_params_parity`, `t71_mixed_body_processes_all_segments`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData / No-migration / Plaintext / Hijack / Sidecars: N/A.
- [x] No system mutation: intake parsing only.
- [x] Clipboard `bind:port` discipline: N/A.
- [x] Redaction: failure reasons redact the same as today's import-failure strings (secret material never lands in `reason` — verify with the existing redaction pass over failure text).
- [x] DPAPI: WG keys sealing unchanged (the row lands through the same `dpapi1:` param pipeline as file imports).

## Out of scope

- QR-pasted-media intake (scanner refusal stays — import.rs:2992).
- New share-link grammar / export_config native links (known stub — out of the borrowed scope; INDEX parked/dead lists untouched).
- Subscription **URL** fetch policy changes (untouched — 8 MiB cap/timeout/redirects identical).
