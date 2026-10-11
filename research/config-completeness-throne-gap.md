# Config completeness: ManyProxy vs Throne — gap audit

Date: 2026-10-10. Auditor: OpenCode (cua-driver + source reads).
Scope: per-protocol **create/edit pages** (the user's "configuration completeness" session).
Throne source: `E:\Code\Throne`. ManyProxy source: `E:\Code\ManyProxy`.
Installed Throne: `E:\apps\Vpn\Throne\Throne.exe` (observed running `[Admin] Throne 1.3.2`, pid 58584).
ManyProxy: `E:\Code\ManyProxy\target\debug\manyproxy.exe` (launched fresh, pid 64204, driven via UIA).

## 1. Method — what was actually run and checked, button by button

### 1.1 Both apps were run
- **Throne**: already running elevated (`[Admin] Throne 1.3.2`, window 1143×843 at 10,57).
  Main-window screenshot captured via `get_window_state` (UIA tree is empty — Qt custom-drawn,
  only `TitleBar` exposed — so the screenshot is the evidence). Visible and confirmed:
  top bar `Program | Settings | Groups | Routing | Tools`, `Tun Mode` + `System Proxy` checkboxes,
  group tabs `Default | Personal | Proton | Privado 8`, profile table columns
  `Type | Address | Name | Test Result | Traffic` with 6 rows
  (Chain Proxy/MIM, Socks/express, Socks/oblivion, **MASQUE**/Masque2, **MASQUE**/masque,
  VLESS (Xray)/xray2-direct-…-reality), bottom tabs `Logs | Connections | Traffic Graph | Runtime Stats`,
  status bar `Not Running`, `Mixed: 127.0.0.1:2080`.
- **ManyProxy**: launched from `target\debug\manyproxy.exe`, window 1002×701.
  Full UIA tree available (20 elements idle). Drove it via `element_index` clicks:
  - `Add (Ctrl-N)` [5] → Add dialog opened (32 elements). All fields read from UIA + screenshot.
  - `Advanced (raw JSON)` [25] → Params JSON mode (33 elements). JSON default captured:
    `{"type":"vless","server":"","server_port":443,"uuid":""}` + `Format JSON | Load protocol template | Validate`
    + `Back to guided`.
  - `Cancel` → dialog closed (back to 20 elements). Verified close works.
  - `Import (Ctrl-O)` [9] → Import dialog opened (26 elements): `Links, JSON, or conf` multiline
    `One share link per line, JSON, or a WireGuard conf`, `Group=default`, `Browse files…`,
    `0 valid · 0 invalid` counter, `Cancel | Import`. Screenshot captured.
  - `Cancel` [23] → closed. Verified.
  - Protocol field `set_value` to `amneziawg` → guided form adapted live:
    `Identity (private_key)` + hint `amneziawg needs private_key and public_key.` Screenshot captured.
    (Same adaptive-hint mechanism confirmed earlier for `vless` → `Identity (uuid)` +
    `vless needs uuid.`)

### 1.2 Throne input is BLOCKED by UIPI — critical access note
Clicking Throne's `Program` button (pixel 50,65) returned:

> `UIPI: target hwnd … (pid 58584) is at High integrity; cua-driver is at Medium integrity.
> PostMessage to a higher-integrity window is silently dropped… Run the daemon elevated…`

So **no Throne button could be clicked this session**. Everything below about Throne's
Add/Edit dialogs comes from **source reads** (authoritative `.ui` + `.cpp`), not guesses:
- All 24 widget lists in `include/ui/profile/*.ui` dumped verbatim (§3).
- `dialog_edit_profile.ui` common + stream/security/network/mux/brutal + Xray XHTTP panels dumped (§3).
- `mainwindow.ui` action list dumped (all Program/Settings/Groups/Routing/Tools actions, §2.2).
- Per-protocol `.cpp` behavior (hysteria v1/v2 layout toggle, wireguard `warp_autogen`,
  Xray `IsXray()` gating, `MustTLS` locking) read from `src/ui/profile/*.cpp`.
- Two prior subagent inventories (ManyProxy `import.rs`/`store.rs`/`engine.rs` grammar pack;
  Throne `src/ui/profile` + `src/configs/outbounds` + `src/configs/common`) cross-checked.

**Ask from user (needed to finish the pixel walkthrough):** either
(a) restart Throne **non-elevated** (Tun Mode off, no admin) so cua-driver can click it, or
(b) relaunch the cua-driver daemon **elevated** (then I re-run the click tour:
Program → New profile → cycle type combo through every protocol + Advanced + each
Settings/Groups/Routing/Tools menu). Until then, §3–§5 treat the `.ui` files as ground truth —
they are, but a visual pass will still be worth doing once input works.

## 2. Throne chrome around the editors (what the buttons do)

From `include/ui/mainwindow.ui` action dump (exact strings):

- `menu_add_from_input` = **New profile** (opens `DialogEditProfile("", …)` — type combo visible).
  Double-clicking a row opens `DialogEditProfile("", id, …)` (type hidden, edit mode).
- `menu_add_from_clipboard` = Add profile from clipboard. `actionAdd_profile_from_File` = Add from File(s).
  `menu_qr` = QR Code and link. `menu_scan_qr` = Scan QR Code.
- Row ops: `menu_clone` Clone, `menu_delete` Delete, `menu_move` Move, `menu_copy_link(s)` Copy link(s),
  `menu_copy_links_nkr` Copy deep links, `menu_profile_debug_info` Debug Info,
  `menu_export_config` Export Sing-box config, `actionExport_Xray_config` Export Xray config.
- Test ops: `actionUrl_Test_Selected`, `menu_full_test`, `menu_stop_testing`,
  `actionSpeedtest_{Current,Selected,Group}`, `menu_resolve_selected`, `menu_resolve_domain`,
  `actionResolve_{Out_IP,Selected_Out_IP}`, `menu_remove_{invalid,unavailable,insecure}`,
  `menu_delete_repeat` Remove Duplicates, `menu_clear_test_result`, `actionCopy/Clear_Test_Result`.
- Groups: `actionAdd/Edit/Delete_Group`, `menu_manage_groups`.
- Settings: `menu_basic_settings`, `menu_hotkey_settings`, `menu_vpn_settings` (Tun),
  `menu_routing_settings`, `menu_open_routing_manager`.
- Program: `menu_start` (Enter), `menu_stop`, `actionRestart_Proxy`, `actionRestart_Program`,
  `menu_open_config_folder`, `actionCheck_For_Update`, `menu_update_subscription`,
  `actionUpdate_All_Subscriptions`.
- View: `menu_start`/`menu_stop`, `actionRuntime_Stats`, `actionTraffic_Stats`, `actionAuto_Selector`,
  `actionShow/Hide_window`, `menu_select_all`, `actionRefresh_Column_Widths`, `menu_reset_traffic`.

Note the **MASQUE** rows in the live screenshot: Throne build 1.3.2 shows `MASQUE` as a profile
type, but `OutboundFactory.cpp` in this source snapshot has no `masque.cpp` include —
it has `juicity/trusttunnel/mieru/tailscale` but no `masque`. So the installed build is
**newer than the checked-out source** for at least MASQUE. Treat MASQUE as a real Throne-supported
type (visible in product) even though its editor isn't in this source tree.

## 3. Throne editors — exact fields (from `.ui` widget dumps, verbatim)

### 3.1 Shared `DialogEditProfile` chrome (`dialog_edit_profile.ui` + `.cpp`)
Always visible: `type` (combo; hidden in edit mode), `name`, `address`, `port`
(hidden for `chain/autoselector/direct/custom*/extracore/tailscale`), `advanced_button`
(hidden for the same set minus address/port cases).

**sing-box `stream_box`** (only when the outbound has the capability flags):
- `network` = `tcp | ws | httpupgrade | http | grpc | quic`
- `security` = `"" | tls`
- `multiplex` = `Keep Default | On | Off`
- `brutal_enable` + `brutal_d_speed` / `brutal_u_speed` (Mbps; forces mux On)

**`network_box`** (visibility follows `network`):
`headers`, `method` (http only), `path`, `host`, `service_name` (grpc only),
`ws_early_data_name` + `ws_early_data_length` (ws only).

**`security_box` + `tls_camouflage_box`**:
`insecure`, `certificate` (multiline editor), `sni`, `alpn` (comma list),
`fragment` (Keep Default|On|Off) + `tls_frag_fall_delay`, `tls_rec_frag`,
`tls_tricks` (Keep Default|On|Off = mixedcase_sni),
`utlsFingerprint` (`"" | chrome | firefox | edge | safari | 360 | qq | ios | android | random | randomized`),
`reality_pbk` + `reality_sid` (non-empty pbk → REALITY).

**Advanced** (`edit_advanced.ui`, `EditAdvanced`):
dial: `connect_timeout`, `bind_interface`, `inet4_bind_address`, `inet6_bind_address` (combos+history),
`reuse_addr`, `tcp_fast_open`, `udp_fragment`, `tcp_multi_path`;
TLS (only if `HasTLS`): `disable_sni`, `min_version`/`max_version`, `enable_ech` + `ech_server_name` +
`ech_config`, `cert_sha256`, `client_cert`/`client_key`.

**Xray panels** (only `IsXray()` = `xrayvless` + CustomXrayOutbound):
`xray_network` (`raw | xhttp | ws | httpupgrade | grpc`), `xray_security` (`"" | tls | reality`),
`xray_mux` (Keep Default|Enabled|Disabled);
TLS: `xray_sni`, `xray_fp`, `xray_alpn`, `xray_pinned_peer_cert_sha256`,
`xray_verify_peer_cert_by_name`, `xray_reality_pbk/sid/spiderx`;
network: `xray_host/path/headers`, `xray_ed_length` (ws/httpupgrade), `xray_multi_mode` (grpc);
XHTTP full: `xray_mode` (auto|packet-up|stream-up|stream-one),
`xPaddingBytes/ObfsMode/Key/Header/Placement/Method`,
uplink `HTTPMethod/DataPlacement/DataKey/ChunkSize`, `noGRPCHeader/noSSEHeader`,
`scMaxEachPostBytes/scMinPostsIntervalMs/scMaxBufferedPosts/scStreamUpServerSecs`,
session `IDPlacement/Key/Table/Length`, `seqPlacement/Key`, `serverMaxHeaderBytes`,
xmux `maxConcurrency/maxConnections/cMaxReuseTimes/hMaxRequestTimes/hMaxReusableSecs/hKeepAlivePeriod`,
`downloadSettings` JSON editor (cleared in `stream-one`).

Capability flags (`include/configs/outbounds/*.h`):
- MustTLS, security locked `tls`: `anytls, hysteria, tuic, juicity, naive, trusttunnel, shadowtls`
- HasTLS+HasTransport+HasMux: `trojan, vless, vmess`
- HasTLS only: `http`
- HasMux only: `shadowsocks`
- None (inner fields only): `socks, mieru, wireguard, ssh, tailscale, direct, chain, autoselector, custom, extracore`

### 3.2 Per-protocol inner editors (widget names verbatim from each `.ui`)
- **socks** (`edit_socks.ui`): `version` (5|4), `username`, `password`. Model has `uot`, no dialog field.
- **http** (`edit_http.ui`): `username`, `password`. TLS optional via shared panel.
  Model parses/builds `path/headers` (`http.cpp`) with no dialog field.
- **shadowsocks** (`edit_shadowsocks.ui`): `method` (editable combo, 18 entries:
  `2022-blake3-*`, `none`, `aes-*-gcm/ctr/cfb`, `chacha20-ietf*`, `xchacha20*`, `rc4-md5`),
  `password`, `plugin` (`"" | obfs-local | v2ray-plugin`), `plugin_opts`, `uot`. Mux panel only.
- **trojan** (`edit_trojan.ui`): `password`. TLS optional + transport + mux via shared panels.
- **vmess** (`edit_vmess.ui`): `uuid`, `aid` (alter_id), `security`
  (`auto | zero | none | chacha20-poly1305 | aes-128-gcm`), `packet_encoding` (`"" | packetaddr | xudp`),
  `uuidgen` button. TLS optional + transport + mux.
- **vless (sing-box)** (`edit_vless.ui`): `uuid`, `flow` (`"" | xtls-rprx-vision`), `packet_encoding`.
  TLS optional + transport + mux (mux auto-disabled when `flow=xtls-rprx-vision`).
- **VLESS (Xray)** (`edit_xrayvless.ui`): `xray_uuid`, `xray_enc` (default `none`),
  `xray_flow` (`"" | xtls-rprx-vision | xtls-rprx-vision-udp443`). Xray panels only.
- **hysteria1/2** (`edit_hysteria.ui`): `protocol_version` (1|2), `server_ports`, `hop_interval`,
  `up_mbps`/`down_mbps`, `obfs`; v1-only `auth_type` (""|STRING|BASE64), `auth`,
  `recv_window_conn`, `recv_window`, `disable_mtu_discovery`; v2-only `password`
  (layout toggled in code). MustTLS locked.
- **tuic** (`edit_tuic.ui`): `uuid`, `password`, `congestion_control` (bbr|cubic|new_reno),
  `udp_relay_mode` (native|quic), `udp_over_stream`, `zero_rtt` (bool), `heartbeat`. MustTLS locked.
- **juicity** (`edit_juicity.ui`): `uuid`, `password`. MustTLS locked.
- **naive** (`edit_naive.ui`): `username`, `password`, `uot`, `quic`,
  `congestion_control` (bbr|bbr2|cubic|reno). MustTLS locked.
- **trusttunnel** (`edit_trusttunnel.ui`): `username`, `password`, `health_check`, `quic`,
  `congestion_control` (bbr|cubic|reno|bbr_standard|bbr2|bbr_variant). MustTLS locked.
- **anytls** (`edit_anytls.ui`): `password`, `interval` (idle check, def 30s),
  `timeout` (idle timeout), `min` (min idle sessions). MustTLS locked.
- **mieru** (`edit_mieru.ui`): `username`, `password`, `transport` (TCP|UDP),
  `multiplexing` (""|OFF|LOW|MIDDLE|HIGH), `traffic_pattern`, `server_ports` (9000-9010,…).
- **shadowtls** (`edit_shadowtls.ui`): `version` (1|2|3), `password`. MustTLS locked.
- **wireguard / amneziawg** (`edit_wireguard.ui`): `private_key`, `public_key` (peer),
  `preshared_key`, `reserved` (comma ints), `persistent_keepalive`, `local_addr` (comma CIDRs),
  `mtu`, `workers`, `sys_ifc`, `warp_autogen` (fills keys/addrs/endpoint);
  Amnezia subgroup `amnezia_group`: `enable_amnezia`, `jc/jmin/jmax`, `s1–s4`, `h1–h4`, `i1–i5`,
  v3 `header_protection_key`, `content_padding_addition`, `rekey_after_time`, `rekey_timeout`,
  `reject_after_time`, `keepalive_timeout`, `max_handshake_attempts`. No TLS/transport/mux;
  endpoint via common address/port.
- **ssh** (`edit_ssh.ui`): `user`, `password`, `private_key` (text area), `private_key_path` + `choose_pk`
  file picker, `private_key_pass`, `host_key` + `host_key_algs` (comma lists), `client_version`.
- **tailscale** (`edit_tailscale.ui`): `state_dir`, `auth_key`, `control_plane`, `ephemeral`,
  `hostname`, `accept_route`, `exit_node`, `exit_node_lan_access`, `advertise_routes` (comma),
  `advertise_exit_node`, `global_dns`. No address/port.
- **direct** (`edit_direct.ui`): no fields (label only).
- **chain** (`edit_chain.ui`): ordered `listWidget` of profile IDs + `select_profile`
  (add/replace via select-mode; rejects chain/autoselector hops; ≤1 extracore, only at index 0).
- **autoselector** (`edit_autoselector.ui`): `group`, `name_filter` (regex), `balance` +
  `balance_mode` (rotate|connection) + `balance_interval`, `pinned_name` + `pinned_clear`,
  `plan_summary`; advanced: `country_filter`, `build_limit`, `pool_cap`, `result_validity`,
  `exclude_unavailable`, `expected/active_size/interval/bench_interval/watch_interval/sampling`,
  `tolerance/max_rtt/dial_retries/interrupt_on_switch`, `test_url/connectivity_url`.
- **custom** (`edit_custom.ui`): `config_simple` plaintext JSON + `as_json` editor
  (`core_l` label varies by CustomOutbound/CustomFullConfig/CustomXrayOutbound/CustomXrayFullConfig).
- **extra core** (`edit_extra_core.ui`): `socks_address`/`socks_port`, `path_combo` (editable) +
  `path_button`, `args`, `config` (plaintext), `no_logs` checkbox
  (BUG NOTE: `.ui` has `no_logs` but `edit_extra_core.cpp:onEnd()` does not persist it).

## 4. ManyProxy today — exact state (visually verified + source)

### 4.1 Add/Edit dialog (one generic form for all 23 protocols)
Guided mode fields: `Name`, `Protocol` (free-text Input, default `vless`; aliases accepted:
`ss/socks5/wg/awg/amnezia-wg/https/naive/…/custom…/chain`), `Server`, `Server port`,
`Identity (<key>)` (masked secret + reveal toggle; **hidden** for no-secret protocols),
`Group`, `Bind address` (default 127.0.0.1), `Port (blank/auto)`, `Test URL` (edit only),
`Advanced (raw JSON)` disclosure (collapsed on Add, expanded on Edit).
Commit merges non-blank `Server→server`, `Server port→server_port`, `Identity→<key>` into JSON
(`fold_guided_into_params`); blank leaves JSON byte-identical.
Template button writes `protocol_params_template(protocol)` =
`{type, server:"", server_port}` + blank credential slot(s); ports: 51820 wg/awg, 22 ssh,
1080 socks, else 443. `Format JSON | Validate` helpers included.

Guided identity map (`guided_identity_fields`, visually confirmed for vless + amneziawg):
vless/xray/vmess→`uuid`; trojan→`password`; shadowsocks→`password` (method JSON-only);
hysteria2→`password`; hysteria→`auth`; tuic→`uuid` (password JSON-only);
wireguard/amneziawg→`private_key` (public_key JSON-only); anytls→`password`;
naive→`password` (username JSON-only); ssh→`password|private_key` (username JSON-only);
noise→`key|password`; snell→`psk`; socks/http/https/shadowtls/custom*/chain→hidden.
**Everything else is Advanced-JSON-only** — no dropdown/checkbox/combo for any transport,
TLS, mux, obfuscation, or peer field.

### 4.2 Import (visually verified)
One paste box (`One share link per line, JSON, or a WireGuard conf`) + `Group` + live
`X valid · Y invalid` preview + `Browse files…`. Grammar pack (`import.rs`, 34 link schemes):
`vless/vmess/v2rayn/trojan/ss/shadowsocks/throne/xray/hysteria2/hy2/hysteria/tuic/wireguard/wg/
awg/amneziawg/amnezia-wg/socks/socks5/snell/shadowtls/anytls/ssh/noise/http/https/naive/naive+https`,
JSON outbounds (23 types), SIP008, Clash YAML `proxies:` (23 types + aliases), WG `.conf`
(multi-peer, AWG markers → auto-classified, default `awg_version 2.0`).
Explicit rejects with guidance (not silent): `mieru/juicity/openvpn/masque/trusttunnel/
hysteria2+realm`, Clash `ssr`, `kcp` transport, `ss plugin`, `http version`, `certificate_sha256`,
`socks version≠5`.

### 4.3 Engine/store (why JSON-only "works" but isn't completeness)
`store.rs` `VALID_PROTOCOLS` (23): `vless, vmess, trojan, shadowsocks, hysteria2, hysteria,
tuic, wireguard, amneziawg, socks, snell, shadowtls, anytls, ssh, noise, http, https, naive,
xray, custom-outbound, custom-config, extra-core, chain`.
`engine.rs` translates flat v2ray-style keys into strict sing-box 1.14.2 schema
(`security/sni/fp/alpn/pbk/sid→tls{}`, `network/path/host/…→transport{}`,
wireguard→`endpoints[]+peers[]`, `up/down→up_mbps/down_mbps`, etc.) and rejects unknown keys
with typed errors per strict protocol. So **import covers far more fields than the dialog exposes** —
the dialog is the bottleneck, which is exactly this session's mandate to fix.

## 5. Gap analysis — protocol by protocol

Legend: ✅ guided in ManyProxy = Throne parity · ⚠️ JSON-only in ManyProxy (import/engine OK,
no editor UI) · ❌ missing entirely (no editor, no first-class import).

| Protocol | Throne guided fields (§3.2 + shared panels) | ManyProxy today | Gap |
|---|---|---|---|
| VLESS (sing-box) | uuid, flow, packet_encoding + TLSopt + transport + mux/brutal + Advanced dial | ⚠️ uuid only; flow/fp/alpn/pbk/sid/sni/network/path… JSON-only | Need: flow dropdown, packet_encoding, full TLS panel (sni/alpn/fp/insecure/reality pbk+sid/fragment/ech), transport panel (tcp/ws/httpupgrade/http/grpc/quic + path/host/headers/method/service_name/early-data), mux+brutal, Advanced dial |
| VMess | uuid, aid, security, packet_encoding, uuidgen + TLSopt + transport + mux | ⚠️ uuid only; aid/security… JSON-only | Need: aid stepper, security dropdown, packet_encoding, uuidgen button, same TLS/transport/mux panels as VLESS |
| Trojan | password + TLSopt + transport + mux | ⚠️ password only | Need: same TLS/transport/mux panels |
| Shadowsocks | method(18-combo), password, plugin(+opts), uot + mux | ⚠️ password only; method/plugin JSON-only | Need: method combo (all 18 + 2022-blake3), password, plugin+opts, uot toggle, mux panel |
| Hysteria v1 | auth(+type), recv_window(_conn), disable_mtu_discovery, server_ports, hop_interval, up/down, obfs + MustTLS | ⚠️ auth only; up/down REQUIRED by engine but JSON-only (easy to produce a broken row) | Need: version 1\|2 switch with layout toggle, all v1 fields, shared MustTLS panel |
| Hysteria v2 | password, server_ports, hop_interval, up/down, obfs + MustTLS | ⚠️ password only; up/down/obfs/sni JSON-only | Need: same editor, v2 layout |
| TUIC | uuid, password, congestion_control, udp_relay_mode, udp_over_stream, zero_rtt, heartbeat + MustTLS | ⚠️ uuid only; password + all tuning JSON-only | Need: full 7-field editor + MustTLS panel |
| WireGuard | private_key, public_key, preshared_key, reserved, keepalive, local_addr, mtu, workers, sys_ifc, warp_autogen | ⚠️ private_key only; public_key + all peer/net JSON-only | Need: full 9-field editor + keygen/warp-autogen equivalent |
| AmneziaWG | all WG fields + enable_amnezia, jc/jmin/jmax, s1–s4, h1–h4, i1–i5, v3 header_protection_key/content_padding/rekey*/keepalive*/max_handshake* | ⚠️ private_key only; entire AWG matrix JSON-only, version invisible | Need: biggest single gap — version 2.0\|3.x switch, all J/S/H/I fields, v3 fields; this is the user's headline ask |
| SOCKS | version(5\|4), username, password | ⚠️ nothing (no-secret form); all JSON-only | Need: version combo, username, password, network/udp_over_tcp |
| HTTP/HTTPS | username, password + TLSopt | ⚠️ nothing; all JSON-only | Need: username, password, TLSopt panel, path/headers (Throne lacks these in dialog too — parity+; engine already allows) |
| AnyTLS | password, interval, timeout, min + MustTLS | ⚠️ password only | Need: 4-field editor + MustTLS |
| Naïve | username, password, uot, quic, congestion_control + MustTLS | ⚠️ password only; username JSON-only | Need: 5-field editor + MustTLS |
| SSH | user, password, private_key(+path picker, pass), host_key(+algs), client_version | ⚠️ password|private_key only; username JSON-only; no picker | Need: full 7-field editor + file picker |
| Snell | (no dedicated Throne .ui beyond scheme? version/psk/obfs via model) — ManyProxy engine: psk, version, obfs_mode/host… | ⚠️ psk only | Need: version, obfs_mode/host, userkey/reuse/network |
| ShadowTLS | version(1\|2\|3), password + MustTLS | ⚠️ nothing; version/password JSON-only (v1 vs v3 invisible) | Need: version combo + password + MustTLS |
| Xray VLESS | xray_uuid, xray_enc, xray_flow + FULL Xray panels (reality spiderX, XHTTP, xmux…) | ⚠️ uuid only; all Xray-only JSON-only | Need: enc/flow + Xray network/security/mux panels (at least the common subset; full XHTTP matrix is large — phase it) |
| noise (AWG fork) | n/a (ManyProxy fork type) | ⚠️ key only | OK short-term (JSON-only by design), document as advanced |
| custom-outbound / custom-config / extra-core / chain | config_simple JSON editor / path+args+config / ordered listWidget | ✅ rough parity (JSON editors exist; chain has guided hops in store but dialog is JSON-only) | Need: chain hop list UI (Throne's listWidget+select parity), extra-core path picker, custom JSON editor with Validate |
| ❌ mieru | username, password, transport, multiplexing, traffic_pattern, server_ports | ❌ rejected at import with guidance, no editor | Decision needed: implement vs keep custom-config guidance |
| ❌ juicity | uuid, password + MustTLS | ❌ same | Same decision |
| ❌ trusttunnel | username, password, health_check, quic, congestion_control + MustTLS | ❌ same | Same decision |
| ❌ tailscale | 9 fields, no address/port | ❌ missing | Same decision (likely out of scope: different auth model) |
| ❌ direct | (no fields) | ❌ missing (engine has direct final only) | Trivial to add as a row type |
| ❌ autoselector | group/filter/balance/plan + advanced tuning | ❌ missing | Larger feature; separate ticket |
| ❌ MASQUE | present in installed Throne build (2 live rows), editor not in this source snapshot | ❌ rejected at import with guidance | Confirm against newer Throne source; likely custom-config guidance short-term |

### Cross-cutting gaps (every protocol suffers these)
1. **No type dropdown** — Protocol is a free-text Input. Throne's `type` combo (hidden on edit)
   prevents typos; ManyProxy relies on `Validate` + commit errors.
2. **No shared Transport / TLS / Mux panels** — Throne gets these once and reuses across
   trojan/vless/vmess (+forced-TLS variants). ManyProxy needs the same three panels + Advanced dial.
3. **No per-protocol dynamic form** — one static guided form + JSON fallback vs Throne's
   `edit_{proto}.ui` swapped into `DialogEditProfile`. This is the core build.
4. **No picker/generator buttons** — Throne: `uuidgen` (vmess), `warp_autogen` (wireguard),
   `choose_pk` (ssh), `path_button` (extra-core), `certificate_edit`, `as_json` editor.
   ManyProxy: only reveal-toggle + Format/Template/Validate.
5. **No chain hop list** — Throne `listWidget + select_profile`; ManyProxy chain is JSON-only.
6. **No `no_logs` parity bug to copy** — Throne's extra-core `no_logs` isn't even persisted;
   don't replicate that bug.
7. **Edit mode hides nothing today** — Throne hides `type` on edit and `address/port` for
   virtual types; ManyProxy should adopt the same gating.

## 6. What to build — per-protocol page spec (field-by-field)

Shared panels (build once, attach per capability flags mirroring Throne):
- **S1 Transport**: network `tcp|ws|httpupgrade|http|grpc|quic|xhttp` + conditional
  path/host/headers/method/service_name/early-data-name+length. Attach: trojan/vless/vmess.
- **S2 TLS**: enable/security, sni, alpn, fp/utls, insecure, pbk+sid (reality auto),
  fragment+fall-delay, rec-frag, tricks, cert/sha256/ech/client-cert (Advanced).
  Forced-on variant (no enable toggle): hysteria/tuic/anytls/naive/shadowtls/(juicity/trusttunnel if added).
- **S3 Mux+Brutal**: mode Keep-Default|On|Off, brutal_enable + up/down Mbps. Attach: ss/trojan/vless/vmess.
- **S4 Advanced dial**: reuse_addr, tcp_fast_open, udp_fragment, tcp_multi_path, connect_timeout,
  bind_interface/inet4/inet6. (Matches Throne `edit_advanced`; ManyProxy already has bind_ip/port/test-url.)

Per-protocol pages (inner fields + which shared panels):
1. **vless**: uuid (+generate), flow `""|xtls-rprx-vision`, packet_encoding + S1+S2+S3+S4.
2. **vmess**: uuid (+uuidgen), aid stepper, security `auto|zero|none|chacha20-poly1305|aes-128-gcm`,
   packet_encoding + S1+S2+S3+S4.
3. **trojan**: password + S1+S2+S3+S4.
4. **shadowsocks**: method combo (18), password, plugin `""|obfs-local|v2ray-plugin` + plugin_opts,
   uot toggle + S3 + S4.
5. **hysteria**: version 1|2 switch; common server_ports/hop_interval/up/down/obfs;
   v1: auth_type+auth/recv_window(_conn)/disable_mtu_discovery; v2: password; + S2-forced + S4.
   Validate up/down presence (engine hard-requires for v1).
6. **tuic**: uuid, password, congestion_control, udp_relay_mode, udp_over_stream, zero_rtt,
   heartbeat + S2-forced + S4.
7. **wireguard**: private_key, public_key, preshared_key, reserved, persistent_keepalive,
   local_addr, mtu, workers, sys_ifc + keygen/warp-autogen equivalent + S4.
8. **amneziawg** (headline): all WG fields + enable_amnezia, awg_version 2.0|3.x,
   jc/jmin/jmax, s1–s4, h1–h4, i1–i5, v3 header_protection_key/content_padding_addition/
   rekey_after_time/rekey_timeout/reject_after_time/keepalive_timeout/max_handshake_attempts + S4.
9. **socks**: version 5|4, username, password, network, udp_over_tcp + S4.
10. **http/https**: username, password, path, headers + S2-optional + S4.
11. **anytls**: password, interval, timeout, min + S2-forced + S4.
12. **naive**: username, password, uot, quic, congestion_control + S2-forced + S4.
13. **ssh**: user, password, private_key + path picker + pass, host_key + algs, client_version + S4.
14. **snell**: psk, version 4|6, obfs_mode/host, userkey, reuse, network, mode + S4.
15. **shadowtls**: version 1|2|3, password + S2-forced + S4.
16. **xray-vless**: xray_uuid, xray_enc, xray_flow + Xray network/security/mux panels
    (phase 1: sni/fp/alpn/reality-pbk+sid+spiderx/network/mode; phase 2: full XHTTP+xmux matrix).
17. **noise / custom-*** / **extra-core**: keep JSON-first; add path picker (extra-core),
    JSON Validate (custom), hop-list editor (chain, Throne listWidget parity).
18. **New protocols** (needs product decision): mieru / juicity / trusttunnel / tailscale /
    direct / autoselector / MASQUE — §5 table has their Throne field lists if approved.

## 7. Suggested build order
- **P0** (user's headline): AmneziaWG + WireGuard full pages (spec item 8→7), then VLESS, VMess,
  Shadowsocks (1, 2, 4). These cover the vast majority of real subscriptions.
- **P1**: trojan, hysteria1/2, tuic, anytls, naive, socks, http/https, ssh (3, 5, 6, 9–13).
- **P2**: snell, shadowtls, xray-vless Ph1, chain hop-list, extra-core picker (14–17).
- **P3**: xray XHTTP full matrix, new-protocol decision (mieru/juicity/trusttunnel/tailscale/
  direct/autoselector/MASQUE), Throne pixel re-verification once input works.

## 8. Buttons checked this session (audit trail)
ManyProxy (clicked, UIA-verified): Add [5] ✅, Advanced (raw JSON) [25] ✅, Cancel (guided [29] ✅,
advanced [30]→Cancel ✅ re-verified via count 20), Import [9] ✅, Import-dialog Cancel [23] ✅,
Protocol set_value→amneziawg ✅ (+hint +screenshot).
ManyProxy (seen, not clicked): Start/Stop/Test, Library/Manage/Engine/Help tabs, filters, Groups,
row checkbox, Import-dialog Import/Browse, Advanced Format/Template/Validate, Back-to-guided.
Throne (seen in screenshot, NOT clicked — UIPI block): Program/Settings/Groups/Routing/Tools,
Tun Mode, System Proxy, group tabs, profile rows, Logs/Connections/Traffic Graph/Runtime Stats.
Throne (source-read, pixel tour pending): New profile + every `edit_*.ui` inner form + Advanced +
all menus in §2.2. Re-run requested once elevated/non-admin access is available (§1.2).

## 9. Repro / files
- Throne `.ui` dumps: `E:\Code\Throne\include\ui\profile\*.ui` (+ `mainwindow.ui` actions).
- Throne behaviors: `E:\Code\Throne\src\ui\profile\*.cpp`, `src\configs\outbounds\*`,
  `src\configs\common\{TLS,transport,multiplex,DialFields}.cpp`, `OutboundFactory.cpp`.
- ManyProxy: `src/import.rs` (34 schemes), `src/store.rs` (`VALID_PROTOCOLS`, templates,
  guided map), `src/engine.rs` (strict 1.14.2 translation), `src/main.rs` (`ConfigForm`).
- Screenshots: cua-driver captures this session (Throne main, ManyProxy Add guided/advanced/
  amneziawg-hint, Import). No secrets present (test rows only).

## 10. Visual verification log (user-supplied screenshots, 1-by-1)

### 10.1 WireGuard + Amnezia (6 shots: base + Amnezia Options)
- Base (`Throne_XyGK0vmQvk.png`, also driven live pid 70400): Common (Type/Name/Address/Port=0/
  Advanced Settings) + WireGuard group: Private Key, Public Key, Pre Shared Key, Reserved,
  Persistent Keepalive*, Local Address*, MTU=1420, Workers=0, `Use System Interface`,
  `Enable Amnezia`, `Amnezia Options` button, `Generate Warp Config` button, OK/Cancel.
- Amnezia Options sub-dialog (driven live, 58 UIA elements + screenshot): two-column grid —
  Jc/Jmin/Jmax (default 0), S1–S4 (0), H1–H4 (empty), I1–I5 (empty), Header Protection Key*,
  Content Padding Addition*, Rekey After Time*/Timeout*, Reject After Time*, Keepalive Timeout*,
  Max Handshake Attempts*, `Random Trailers*` + `Disable Cookies*` checkboxes, OK/Cancel.
  NOTE: Random Trailers / Disable Cookies exist in installed build 1.3.2 but NOT in this
  source snapshot (nor Snell/MASQUE rows) — installed build is newer than source. Spec for
  ManyProxy must include them.

### 10.2 VLESS (6 shots: tcp/httpupgrade/http/grpc/quic + live ws capture)
- Inner: UUID, Flow (dropdown), Packet Encoding=xudp default.
- Settings: Network* (tcp|ws|httpupgrade|http|grpc|quic), Security* (empty/tls),
  Multiplex* Keep Default, Enable TCP Brutal* + Brutal Download/Upload Speed* Mb/s (0).
- Conditional network panels (all visually confirmed):
  tcp/quic → none; ws → header*/Path*/Host*/EarlyData Length(=0)/EarlyData Name;
  httpupgrade → header*/Path*/Host*; http → header*/Method*/Path*/Host*; grpc → Service Name.
- ManyProxy gap (unchanged, now visual-grade): only uuid guided; flow/packet_encoding/
  entire Settings box/conditional panels JSON-only.

### 10.3 VMess (6 shots: tcp/ws/httpupgrade/http/grpc/quic + Advanced Settings dialog)
- Inner (all 6 shots): UUID, Alter Id=0 + `Generate UUID` button, Packet Encoding=xudp,
  Security=auto (dropdown: auto|zero|none|chacha20-poly1305|aes-128-gcm per source).
- Same Settings box + same conditional network panels as VLESS (ws panel incl. EarlyData
  Length=0 confirmed on VMess too).
- Advanced Settings dialog (`Throne_f1Ql1jXNvf.png`, opened from any HasTLS type):
  Dial Fields: Reuse Address, TCP Fast Open, UDP Fragment, TCP MultiPath (checkboxes),
  Connect Timeout, Bind Interface (combo), IPv4/IPv6 Bind Address (combos);
  TLS: Disable SNI, Enable ECH, TLS Min/Max Version, TLS Spoof* (Keep Default), Spoof SNI*
  ("Allowed SNI to forge"), Method, ECH Server Name*, ECH Config (Not Set button),
  Certificate SHA256 (Not Set), Client Certificate (Not Set), Client Key (Not Set), OK/Cancel.
  The "Not Set" buttons are file/content pickers — ManyProxy spec needs equivalents:
  certificate file content, sha256 string, client cert/key.
- ManyProxy gap: only uuid guided; aid/security/packet_encoding/uuidgen/Settings/
  conditional panels/Advanced-dial+TLS JSON-only.

### 10.4 Shadowsocks (2 shots: base + Advanced)
- Inner: Encryption (dropdown, empty default; 18 methods per source), Password,
  Plugin (dropdown: ""|obfs-local|v2ray-plugin per source), Plugin Args, `UDP over TCP`
  checkbox. Settings box = Multiplex* + Brutal ONLY (no Network/Security — confirms
  HasMux-only capability flag; no TLS/transport panels).
- Advanced Settings dialog for Shadowsocks shows Dial Fields ONLY, no TLS section —
  confirms TLS section is gated on HasTLS. (Dialog window title reads "Dialog".)
- ManyProxy gap: only `password` guided; method/plugin/plugin_opts/uot/mux JSON-only.

### 10.6 Profile-table context menu (right-click, `Throne_W6vjZ65Uvv.png`)
Intake: New profile (Ctrl+N) · Add profile from clipboard (Ctrl+V) ·
Add profile from File(s) (Ctrl+O) · Scan QR Code (Ctrl+Shift+Q).
Row ops: Start (Return) · Stop (Ctrl+S) · Share (submenu) · Delete (Del) ·
Select All (Ctrl+A) · Clone (Ctrl+D).
Tests (greyed when idle/unsupported): Url Test Selected (Ctrl+Shift+S) ·
Speedtest Selected (Ctrl+Shift+P) · Resolve Selected Out IP · Resolve Selected Domain ·
Clear Test Result · Reset Traffic (Ctrl+R).
ManyProxy parity: Add (Ctrl+N) ✅ · Import (Ctrl+O, paste box + Browse files) ✅ ·
Start/Stop/Test selected ✅. Gaps: no clipboard-intake shortcut (Ctrl+V),
no QR scan, no Clone, no Share/Copy-link submenu, no per-row test/resolve actions.

### 10.7 Installed-build Type list (4 dropdown shots, build 1.3.2 — NEWER than source)
Top: Auto Selector, Socks, HTTP, Shadowsocks, Trojan, VMess, VLESS, VLESS (Xray),
Hysteria, TUIC. Mid: Juicity, Naive, TrustTunnel, AnyTLS, Mieru, Snell, ShadowTLS,
WireGuard, MASQUE, OpenVPN. Bottom: OpenConnect, Tailscale, SSH, Direct,
Custom (sing-box outbound), Custom (sing-box config), Custom (Xray outbound),
Custom (Xray config), Extra Core, Chain Proxy.
NEW vs source snapshot: MASQUE, OpenVPN, OpenConnect appear in the product but have
no editor/model in this source tree. Custom entries are explicitly labeled by core.

### 10.8 ADD-LATER backlog (screenshot tour parked — user: "enough for now")
Visually verified: WireGuard+Amnezia (§10.1), VLESS all transports (§10.2),
VMess all transports + Advanced (§10.3), Shadowsocks + dial-only Advanced (§10.4),
context menu (§10.6), full Type list (§10.7).
NOT yet visually covered (source-spec only, §3/§5): Trojan, Hysteria v1+v2 toggle,
TUIC, AnyTLS, Naive, Mieru, Juicity, TrustTunnel, ShadowTLS, SSH, HTTP, Socks,
Xray VLESS panels, Snell, Chain listWidget, Custom JSON, Extra Core picker, Direct,
Tailscale, Autoselector Advanced, Security=tls TLS panel, Advanced on non-TLS types.
New-build unknowns (no source at all): MASQUE, OpenVPN, OpenConnect editors —
needs newer Throne source or screenshots before speccing.
Also open: ManyProxy-side spec sign-off per protocol (§6) + intake/row-op gaps (§10.6).

### 10.9 Incident (2026-10-10, live-driving Throne)
- ENTER/RETURN inside combobox popups falls through to the dialog OK button and saves
  half-filled rows (created stray empty VMess, then empty VLESS rows).
- Cleanup of the VLESS stray deleted the user's real `xray2-direct-mci1-fs-ce-reality`
  (VLESS Xray, freesocks) profile instead — selection had shifted. Verified via read-only
  `throne.db` queries: Personal=[966,967,1,451,450,1200], no xray2 rows, no group sub URLs.
- Leftover: empty unnamed VLESS row id=1200 in Personal (user to delete or explicitly permit).
- RULE: no more live-driving of the user's Throne. All further evidence via user screenshots
  + source reads. ManyProxy (dev app, UIA, hermetic test DB) remains drivable.
