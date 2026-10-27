# ManyProxy

ManyProxy is a Windows desktop app for a personal library of proxy/VPN
configurations. It stores the configs, imports share links, files and
subscriptions, and serves each config on its own loopback endpoint —
`socks5://127.0.0.1:<port>` and `http://127.0.0.1:<port>` — so individual apps
can be pointed at the proxy they need. Health testing (URL test, TCP ping) shows
which endpoints actually work.

It is local-only by design: **no system proxy, no TUN interface, no DNS rewrite,
no elevation.** Nothing outside the app folder and its own data folder is
changed.

## Supported platform

- **Windows 10/11, x64 (windows-amd64)** — the only supported target today.
- `windows-arm64` is drafted behind a gated release row (see
  `.github/workflows/release.yml`); it is not supported yet.
- Linux and macOS are tracked as an aspiration only (issue
  [#6](https://github.com/mahyartdb/ManyProxy/issues/6)), not supported.

There is no web, mobile, or CLI edition — only the native desktop app.

## Install (portable ZIP)

1. Download `manyproxy-v<tag>-windows-amd64.zip` from
   [GitHub Releases](https://github.com/QMahyar/ManyProxy/releases) and unzip it
   anywhere you like (a normal folder, another drive, a USB stick). Each release
   also publishes `SHA256SUMS.txt`.
2. Keep the unzipped files together in one folder: `manyproxy.exe`,
   `sing-box.exe`, `sing-box-awg.exe`, `xray.exe`, plus `README.md`,
   `THIRD-PARTY-NOTICES.md` and `licenses/`.
3. Run `manyproxy.exe`. No installer, no administrator rights, no AppData or
   Program Files writes — the defaults bind to loopback only.

The beta has no code-signing story; the published SHA-256 hashes are the
honesty floor, so compare them if you want to verify a download.

**Engines must sit next to the exe.** `sing-box.exe` (standard rows),
`sing-box-awg.exe` (AmneziaWG/Noise/XHTTP rows) and `xray.exe` (Xray-only VLESS
rows) are resolved from the app directory only — `PATH` is never consulted and
the app never downloads binaries. At most one extra, user-supplied core
executable can live in the same folder; reference its plain file name in the row
that uses it.

### Updating

There is no auto-update. Replace the folder by hand with the new release (keep
or move `data\` if you want to keep the old library).

## Where data lives

Everything the app writes lives in `data\` **beside `manyproxy.exe`**:

| Path | Contents |
| --- | --- |
| `data\library.db` | SQLite library: configurations, subscriptions, presets, health snapshots, usage totals |
| `data\manyproxy-export.json` | Default export path; written only after you confirm the plaintext-secret warning |
| `data\manyproxy-logs.txt` | Visible session logs |
| `data\library.db.bak-*`, `data\library.db.corrupt-*` | Automatic backups and quarantine copies |

Secrets at rest are sealed with Windows DPAPI (user scope, `dpapi1:` prefix);
logs, URLs and labels are redacted, and copied URLs carry bind + port only, never
credentials.

**Uninstall = delete the folder.** There is no installer, no uninstaller, and no
registry state beyond the opt-in HKCU Run autostart entry. Data outside the app
folder is never read or migrated. The `MANYPROXY_DB`, `MANYPROXY_EXPORT` and
`MANYPROXY_LOGS` environment variables override the paths above for testing.

## First run

1. **Add** (`Ctrl-N`) a configuration, or **Import** (`Ctrl-O`) share links, a
   file dropped on the window, clipboard text, or a WireGuard `.conf`.
2. Leave **Port** blank to auto-assign from the primary range `10801–10999`
   (spillover `18081–18380`), or type a port.
3. Select the row and press **Start** (`F5`), or **Start all** to start every
   row. Starting regenerates one combined engine config, runs the engine's own
   `check`, then (re)starts the sidecar. Stop with **Stop** (`Shift-F5`) or
   **Stop all**; stopping clears the row's restore-on-launch flag. That flag
   (row menu → **Restore on launch**) decides which rows the next launch
   reopens — Start all ignores it and starts every row.
4. **Copy URLs** (`Ctrl-U`) copies both forms for the row:
   `socks5://127.0.0.1:<port>` and `http://127.0.0.1:<port>`. Point one app at
   one endpoint — apps with separate SOCKS5/HTTP fields take the matching line.
5. **Test** (`Ctrl-T`) runs a URL test against
   `http://www.gstatic.com/generate_204`; **Ping** does a TCP ping. Results stay
   on the row (`87ms` / `dead`) and in `health: N live · M dead` on the status
   bar.

Closing the window while endpoints run opens a confirm naming every running
row: OK stops all and closes, Cancel keeps them running. The tray icon offers
Start all / Stop all / Show / Quit.

## Keyboard shortcuts

| Action | Keys |
| --- | --- |
| Add configuration | `Ctrl-N` |
| Import configurations | `Ctrl-O` |
| Paste clipboard into Import | `Ctrl-V` |
| Start selected | `F5` |
| Stop selected | `Shift-F5` |
| Test selected | `Ctrl-T` |
| Stop Testing | `Ctrl-.` |
| Edit selected | `Ctrl-E` |
| Delete selected | `Delete` |
| Focus search | `Ctrl-F` |
| Inspector details | `Ctrl-1` |
| Inspector health | `Ctrl-2` |
| Inspector presets | `Ctrl-3` |
| Uncheck all | `Esc` |
| Copy URLs | `Ctrl-U` |
| Command palette | `Ctrl-K` |

The same list appears in the About dialog and Settings → Hotkey, and these
key-bound actions also have toolbar or menu entries and `Ctrl-K` palette twins.

## What ManyProxy does not do

No system proxy switching, no TUN / virtual interface, no OS DNS rewrite, no
routing-rule editor, no admin/elevation path, no auto-update that replaces
binaries, no telemetry, and no silent binary downloads. The only network traffic
the app originates is user-initiated: a subscription fetch, the GitHub release
feed behind **Check for updates**, explicit `throne://remoteroute/` fetches, and
probes through your own endpoints to the visible test URL. These are durable
constraints, covered by the `assert_no_system_mutation` test family.

## Troubleshooting

- **Engine binary not found.** Start fails with an error naming the affected
  rows, the expected file name, and every directory searched, ending with
  `ManyProxy never downloads executables silently`. Put the named exe
  (`sing-box.exe`, `sing-box-awg.exe`, `xray.exe`, or your extra-core exe) next
  to `manyproxy.exe` and start again. A file quarantined by antivirus surfaces
  the same way — never a silent fallback.
- **Port already in use.** The second start fails naming both configs; the first
  one keeps serving. Edit the row's Port (blank = a fresh auto port) or stop the
  other endpoint, then start again.
- **Non-loopback bind.** Any non-loopback bind (`0.0.0.0`, `::`, a LAN address)
  stops at an explicit confirm naming the config and bind before anything
  starts; dismissing changes nothing. A firewall prompt for such a bind is
  Windows' own and expected.
- **The library looks empty.** A read failure shows `Cannot load library: …`
  instead of an empty list; if the list says `No proxy configurations yet`, the
  store really has no rows.
- **Subscription refresh failed.** The row keeps its last-good members and shows
  the error. Common causes: unreachable URL, non-2xx response, a body over
  8 MiB, or a non-UTF-8 body.
- **SmartScreen / antivirus warning.** Unsigned fresh binaries that spawn
  sidecars and listen on loopback routinely trip heuristics. Verify the file
  against `SHA256SUMS.txt`; never disable antivirus.

## Import formats

Accepted: `vless://`, `vmess://` (URI and legacy base64 JSON), `trojan://`,
`ss://` / `shadowsocks://` (all three grammars), `hysteria2://` / `hy2://` /
`hysteria2+realm://`, `hysteria://`, `tuic://`, `wireguard://`, `awg://` /
`amneziawg://` / `amnezia-wg://`, `socks://` / `socks5://`, `http://` /
`https://`, `anytls://`, `naive://`, `throne://add/...`, sing-box outbound JSON
(single object, array, or full config), Xray `protocol`-shaped objects, SIP008
`servers[]`, limited Clash `proxies[]` YAML, and WireGuard `.conf` (one config
per `[Peer]`).

Malformed items are listed per item as `item N (label): reason`; valid items
still land as disabled rows, and import never starts anything. Identical
re-imports add nothing. `throne://addsub/...` lines create a subscription and
refresh it once; `throne://route/...` links import into routing presets and
`throne://remoteroute/...` URLs fetch explicitly without auto-refresh.

## Subscriptions and auto-refresh

Toolbar **Subscriptions** opens the manager: label, subscription URL, group and
an auto-refresh checkbox (default on, 24 h interval). Each row shows the redacted
URL, group, member count, auto flag, last error, and per-item failures, with
Refresh / Auto on|off / Remove buttons. Refresh fetches with an 8 MiB cap, a 20 s
timeout and UTF-8 required; errors never echo the URL. Merges keep your renames,
add no duplicates, un-own upstream removals but leave hand rows untouched, and
keep the last-good set on failure. A background 5-minute tick picks up due
subscriptions (at most one coalesced toast).

## Export

Toolbar **Export** opens a confirm that shows the plaintext warning verbatim
(_"Export carries private keys, tokens, and passwords in plaintext. Anyone with
this file can use your proxies — store it somewhere safe."_) plus a preview
`N config(s) → <path>`. Confirm writes JSON (configs + subscriptions) to the
default export path; cancel writes nothing. The payload round-trips through the
import grammar, so re-import adds 0 new rows and reports duplicates.

## Health, usage, and logs

Each row shows persisted health samples and data-plane usage (`↓rx ↑tx`, from
the engine's own metering); probe-only bytes stay labeled `probe:` so the two
never mix. **Logs** opens the sidecar log for the selected config (or the global
session log) with a filter field; secrets are redacted at ingest. Health state
survives restarts; the health and usage counters can be cleared from Settings →
Maintenance.

## Building from source

```text
cargo run                  # dev run (no admin needed for loopback defaults)
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked        # live proofs are opt-in and skipped by default
```

Live proofs need a local `sing-box.exe` plus `MANYPROXY_SINGBOX_BIN`,
`MANYPROXY_LIVE_CHECK=1` and (for traffic) `MANYPROXY_LIVE_TRAFFIC=1`; CI runs
them from `.github/workflows/live.yml` only.

## Reference: claim → code map (audit)

| README claim                         | Proved by                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| ------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Toolbar buttons + dialogs + footers  | `Button::new("add-config"/"import-config"/"import-clipboard"/"export-library"/"subscriptions"/"engine-start-all"/"engine-stop-all"/"health-test-all"/"about-dialog"/"edit-config"/"delete-config"/"copy-urls"/"engine-start"/"engine-stop"/"health-test"/"health-ping"/"logs-open")` with shortcut-hint labels (`Add (Ctrl-N)`, `Import (Ctrl-O)`, `Start selected (F5)`, `Stop selected (Shift-F5)`, `Test selected (Ctrl-T)`, ticket 33), `open_add_dialog` / `open_edit_dialog` / `open_import_dialog` / `open_export_dialog` / `open_logs_dialog` / `open_about_dialog` / `subscription_row`, `dialog_footer` (`src/main.rs`); `dialog_footers_audit_every_plain_dialog` (ticket 18) |
| Defaults + auto port                 | `DEFAULT_BIND/PROTOCOL/GROUP` (`src/store.rs:6/8/10`), `AUTO_PORT_START/END = 10801–10999` + spillover `AUTO_PORT_SPILL_START/END = 18081–18380` (`src/store.rs:42-46`), `parse_port_field("")` = auto, `auto_ports_are_unique`                                                                                                                                                                                                                                                                                                                                             |
| Accepted formats                     | `src/import.rs` parsers + tickets 03/07/08/15 tests (239+ passing incl. `paste_and_drop_agree`, `conf_holding_json_or_links_sniffed`, `pasted_clash_yaml_imports`, `bom_prefixed_json_parses`)                                                                                                                                                                                                                                                                                                                 |
| Toasts honest                        | `config_added/updated/removed_message`, `start_all/stop_all/already_stopped_message`, `export_count_message`, `coalesced_refresh_message` (`src/main.rs:1533/1543/1680/1705/1715/1790`); ticket 17 UI tests                                                                                                                                                                                                                                                                                                                              |
| Subscriptions/auto-refresh           | `fetch_subscription_body`, `redact_subscription_url`, `due_for_refresh` + `DEFAULT_AUTO_REFRESH_SECS`, `DUE_SUBSCRIPTION_TICK_SECS`, `refresh_with_body` tests                                                                                                                                                                                                                                                                                                                                                 |
| Endpoints/copy URLs                  | `proxy_urls` (`src/store.rs:468`), `copy_selected_urls` + `copy_urls_carry_binds_not_secrets`                                                                                                                                                                                                                                                                                                                                                                                                                  |
| Health/search                        | `DEFAULT_TEST_URL`, `test_all_health` / `test_selected_health` / `ping_selected`, `library_row_matches`, `health_status_text`                                                                                                                                                                                                                                                                                                                                                                                  |
| Export                               | `export_warning_text` verbatim + `export_warning_names_secret_classes`, `default_export_path`, `commit_export`, `export_preview_description`                                                                                                                                                                                                                                                                                                                                                                   |
| Missing binaries / LAN / no-mutation | `EngineError::MissingBinary` + `missing_binaries_state_never_downloads`, `requires_lan_confirm` + `LAN_CONFIRM_TITLE` + `lan_hint_for_bind`, `assert_no_system_mutation` + `session_fingerprint_roundtrip_proves_no_residue`                                                                                                                                                                                                                                                                                   |
