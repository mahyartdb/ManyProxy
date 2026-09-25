# ManyProxy — Quickstart Run (ticket 09, release gate)

How to verify scenarios A–D from `.planning/quickstart.md`. Each scenario
lists its **automated half** (runnable now, no remote servers) and its
**live half** (needs real test proxy configs on distinct egresses).

Prereq for live halves: built desktop app, 3+ test proxy configs on distinct
egresses, loopback networking allowed, ports from 10801 free.

## Scenario A — Three distinct egresses, no system change

- Automated (no remotes): `session_fingerprint_roundtrip_proves_no_residue`
  (start-all 3 standard rows → serving fingerprint differs; generated config
  passes `assert_no_system_mutation`; stop-all → fingerprint equals the
  before-session snapshot) plus `generated_stock_config_passes_mutation_scan`
  (structural proof — no `tun`/`dns`/`set_system_proxy` at any depth).
- Live: snapshot OS proxy/DNS/routes before
  (`reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings"`,
  `ipconfig /displaydns` baseline, `route print`), import 3 configs, start
  all, curl a public-IP reflector through each endpoint's SOCKS5 and HTTP
  forms, confirm 3 distinct egress IPs, stop-all, re-run the three OS
  snapshots and diff — must be byte-identical.

## Scenario B — Custom bind + conflict

- Automated: `three_configs_yield_three_mixed_inbounds` + port-conflict
  engine tests (second start fails naming both configs, first unaffected) and
  the ticket-09 UI gates `lan_single_start_opens_confirm_and_stays_stopped`
  / `lan_start_all_opens_confirm_and_stays_stopped` /
  `loopback_single_start_skips_confirm`.
- Live: set config A to loopback port 10821, start, request succeeds; set
  config B to the same port, start → port-in-use error naming config A, A
  unaffected; set a LAN bind → confirm dialog appears, dismiss keeps the
  engine stopped.

## Scenario C — Subscription refresh preserves edits

- Automated: store `refresh_with_body` tests (rename preserved, no
  duplicates, upstream removal only unowns owned rows, failure keeps
  last-good) plus `subscription_label_strips_credentials_and_query`.
- Live: add a real subscription URL, refresh, rename one member, refresh
  again → members updated, no duplicates, renamed member keeps its name.

## Scenario D — Exotic + custom

- Automated: ticket-07 AWG matrix tests, ticket-08 Xray/custom/chain tests
  (broken JSON blocks start naming the field, others unaffected),
  `fork_missing_only_exotic_fails` (stock rows keep working).
- Live: one AmneziaWG-variant + one Xray-only VLESS + one custom-JSON config
  with real remotes, start all → each endpoint serves its own egress.

## Secrets audit (grep-level proof)

- `redact_secrets_covers_credential_forms` (health log ingest: key=value,
  bearer, userinfo, query forms), `logs_filter_to_config_and_redact_secrets`
  (per-config view), engine error tests (`unsupported_protocol_…`,
  `bad_params_error_…`, `awg_missing_credentials_…` — messages name
  config+field, never values), `failure_display_strings_carry_no_plaintext_secrets`
  - `redact_link_scrubs_query_values_keeps_keys` (import notification path),
    `subscription_label_strips_credentials_and_query` (subscription labels),
    `copy_urls_carry_binds_not_secrets` + `proxy_url_pair_format` (clipboard),
    `lan_confirm_text_names_config_without_secrets` (new ticket-09 output),
    `session_fingerprint_roundtrip_proves_no_residue` (new fingerprint output),
    `export_warning_names_secret_classes` (future export copy).
- Manual grep to re-run at release: search `src/` for `push_notification`,
  `write_to_clipboard`, `push_raw`, `format!` in error paths and confirm no
  `params`, `password`, `token`, `private_key`, `uuid`, `url` values flow in.
  v1 ships no export UI (local SQLite only).

## Prohibitions (asserted as tests where possible)

- No telemetry / no silent binary replacement / no new admin:
  `missing_binaries_state_never_downloads`,
  `binary_resolution_never_elevates_or_downloads`,
  `mutation_scan_rejects_forbidden_keys` +
  `session_fingerprint_roundtrip_proves_no_residue` (generated configs carry
  no `tun`/`dns`/`set_system_proxy`; stop-all leaves no residue).
- Structural (by construction, documented): the single `HttpClient` call
  site is `fetch_subscription_body` (user-supplied subscription URLs only,
  generic errors never echo the URL); health probes dial only the row's own
  `bind:port` toward the user-visible test URL; sidecars are spawned local
  processes, never downloads.
