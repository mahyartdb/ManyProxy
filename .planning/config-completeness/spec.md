# Spec: Config completeness — guided editors for every protocol (Throne parity)

Status: spec (planning only — no implementation authorized by this document).
Scope source: `research/config-completeness-throne-gap.md` (2026-10-10 audit, §§1–10, visual + source-verified).
Feature dir: `.planning/config-completeness/`.

## Kernel

### 1. Why

From the user's perspective: ManyProxy runs configs but makes the user hand-write JSON for
everything beyond one secret. Throne shows one tailored page per protocol — UUID + Flow +
Transport + TLS + Mux for VLESS, J/S/H/I matrix for AmneziaWG, version switch for Hysteria,
key pickers and generators — while ManyProxy shows one static form (Name / Protocol free-text /
Server / Server port / Identity / Group / Bind / Port) plus an Advanced JSON box. Import already
accepts far more fields than the dialog exposes, so users paste a link, open Edit, and find most
settings invisible and uneditable without learning sing-box 1.14.2 key names. The headline pain is
AmneziaWG: the entire Jc/Jmin/Jmax/S/H/I/v3 matrix is JSON-only and the version is invisible.
The user asked for every gap in the audit to be planned and specced so no supported protocol stays
JSON-only.

### 2. Capabilities

Stable IDs, never reused. Each is user-observable and testable without implementation knowledge.

**CAP-1 — Protocol picker + per-protocol dynamic form.**
Intent: the dialog never accepts a typo'd protocol and never shows irrelevant fields.
1. Protocol is a dropdown listing the 23 stored types (`vless, vmess, trojan, shadowsocks,
   hysteria2, hysteria, tuic, wireguard, amneziawg, socks, snell, shadowtls, anytls, ssh, noise,
   http, https, naive, xray, custom-outbound, custom-config, extra-core, chain`), hidden on edit
   (type locked once created, Throne parity §7.7).
2. Selecting a type swaps the inner form (same dialog chrome: Name / Server / Server port /
   Group / Bind / Port / Test URL + Advanced disclosure); switching type rewrites the params
   template to `protocol_params_template(new type)` only when the JSON is still a pristine
   template, never over user-edited JSON.
3. Address/port inputs hide for virtual types (`chain, custom-*, extra-core`, plus future
   `direct/autoselector/tailscale` if approved), matching Throne's gating.
4. Guided↔JSON never disagrees about what a row needs: hint line (`<proto> needs <keys>`) and
   commit validation name the same credential set.

**CAP-2 — Shared Transport panel (S1).**
Intent: one transport UI reused by trojan/vless/vmess (Throne `stream_box` + `network_box`).
1. Sing-box network dropdown `tcp | ws | httpupgrade | http | grpc | quic`. Xray page uses Xray
   networks `raw | xhttp | ws | httpupgrade | grpc` (verified `xrayStreamSetting.h:5`; no tcp/quic
   on the Xray page).
2. Conditional fields appear per network: `path, host, headers` (ws/httpupgrade/http);
   `method` (http only); `service_name` (grpc only); `ws_early_data_name + ws_early_data_length`
   (ws only); tcp/quic/raw show no extra fields.
3. Values fold into params JSON transport keys the engine already accepts (`network/path/host/
   method/service_name/early-data`) and round-trip: reopening Edit shows the same values.
   `headers` has no input by decision and no transport mapping: hand JSON carrying it fails
   typed at generate naming the field (never silently dropped).

**CAP-3 — Shared TLS panel (S2) + forced-TLS variant.**
Intent: one TLS UI reused everywhere TLS applies; forced-on where the protocol mandates it.
1. Optional variant (trojan/vless/vmess/http/https): enable toggle + `sni, alpn (comma list),
   fp/utls (`""|chrome|firefox|edge|safari|360|qq|ios|android|random|randomized`), insecure
   toggle, reality `pbk + sid` (non-empty pbk means REALITY), fragment (`Keep Default|On|Off`) +
   `tls_frag_fall_delay`, `tls_rec_frag`, tricks (`Keep Default|On|Off` = mixedcase_sni)`.
2. Forced variant (hysteria v1/v2, tuic, anytls, naive, shadowtls + future juicity/trusttunnel if
   approved): same fields minus the enable toggle, security locked `tls`.
3. Certificate/sha256/ECH/client-cert fields live in Advanced dial (CAP-5), not in this panel.

**CAP-4 — Shared Mux+Brutal panel (S3) + engine emission.**
Intent: one multiplex UI for ss/trojan/vless/vmess (Throne HasMux gating).
1. Mode `Keep Default | On | Off`; `brutal_enable` + `brutal_d_speed / brutal_u_speed` (Mbps);
   enabling brutal forces mux On (Throne behavior).
2. Shadowsocks page shows Mux+Brutal only (no Transport/TLS panels — HasMux-only parity).
3. `flow=xtls-rprx-vision` auto-disables mux on the vless page (Throne behavior).
4. Engine scope (verified 2026-10-10: `engine.rs` has zero `mux|multiplex` handling — panels alone
   would write keys the engine drops/rejects): the S3 slice includes `engine.rs` mux emission for
   ss/trojan/vless/vmess (strict-1.14.2 shape, recorded per-key in-ticket against `check`), plus
   `check`-green proof. S3 UI ships only with its engine arm — never UI-first with dead keys.

**CAP-5 — Shared Advanced dial panel (S4) + TLS-extras.**
Intent: Throne `edit_advanced.ui` parity without leaving Add/Edit.
1. Dial fields: `reuse_addr, tcp_fast_open, udp_fragment, tcp_multi_path` toggles;
   `connect_timeout, bind_interface, inet4_bind_address, inet6_bind_address` (combo + history;
   ManyProxy's row-level Bind/Test-URL stay where they are — these are upstream dial fields).
2. TLS extras (only when the page has TLS): `disable_sni, min_version/max_version, enable_ech +
   ech_server_name + ech_config, cert_sha256, client_cert/client_key, certificate` content.
3. File-backed entries (`certificate`, `ech_config`, `client_cert/client_key`, ssh `private_key_path`,
   extra-core `path`) use a Browse picker; the dialog never requires hand-typed file contents.

**CAP-6 — VLESS / VMess / Trojan / Shadowsocks full pages (P0–P1 core).**
Intent: the four most-subscribed types stop being JSON-only.
1. VLESS: `uuid` (+generate button), `flow (""|xtls-rprx-vision)`, `packet_encoding` + S1+S2+S3+S4.
2. VMess: `uuid` (+uuidgen button), `aid` stepper, `security
   (auto|zero|none|chacha20-poly1305|aes-128-gcm)`, `packet_encoding` + S1+S2+S3+S4.
3. Trojan: `password` + S1+S2+S3+S4.
4. Shadowsocks: `method` combo (all 18 incl. `2022-blake3-*`, `none`, `aes-*-gcm/ctr/cfb`,
   `chacha20-ietf*`, `xchacha20*`, `rc4-md5`), `password`, `plugin (""|obfs-local|v2ray-plugin)` +
   `plugin_opts`, `uot` toggle + S3+S4.

**CAP-7 — Hysteria v1/v2 + TUIC pages.**
Intent: version confusion and required-field breakage end.
1. Hysteria: `protocol_version (1|2)` switch toggling layout; common `server_ports, hop_interval,
   up_mbps/down_mbps, obfs`; v1-only `auth_type (""|STRING|BASE64) + auth, recv_window_conn,
   recv_window, disable_mtu_discovery`; v2-only `password`; + S2-forced + S4.
2. Commit refuses a **v1** row missing `up/down` (engine `translate_outbound` hard-requires for v1,
   `engine.rs:3468-3495`); v2 leaves them optional (BBR-by-omission, `engine.rs:3544-3557`).
   Errors name the fields.
3. TUIC: `uuid, password, congestion_control (bbr|cubic|new_reno), udp_relay_mode (native|quic),
   udp_over_stream, zero_rtt, heartbeat` + S2-forced + S4.

**CAP-8 — WireGuard + AmneziaWG full pages (headline).**
Intent: the biggest single gap closes; AWG version is visible and editable.
1. WireGuard: `private_key, public_key (peer), preshared_key, reserved (comma ints),
   persistent_keepalive, local_addr (comma CIDRs), mtu, workers, sys_ifc` + keygen /
   warp-autogen equivalent + S4. `udp_timeout` stays import-only (Throne model-only, no `.ui`
   widget, no `edit_wireguard.cpp` handling — `wireguard.h:37` vs no widget).
2. AmneziaWG: all WG fields + `enable_amnezia, awg_version (2.0|3.x), jc/jmin/jmax, s1–s4,
   h1–h4, i1–i5`, v3 `header_protection_key, content_padding_addition, rekey_after_time,
   rekey_timeout, reject_after_time, keepalive_timeout, max_handshake_attempts`, plus installed-build
   extras `Random Trailers + Disable Cookies` checkboxes (audit §10.1 — installed 1.3.2 newer than
   source snapshot; engine pass-through-or-preserve verified in-ticket against fork `check`).
3. Reserved/DNS/MTU/keepalive imported from `.conf` round-trip through the form (no silent drops).

**CAP-9 — SOCKS / HTTP(S) / AnyTLS / Naïve / SSH pages.**
Intent: credential-plus-tuning types stop hiding fields in JSON.
1. SOCKS: `version (5|4), username, password` (Throne `edit_socks.ui` parity) + `network,
   udp_over_tcp` as parity+ (ManyProxy engine accepts them, `engine.rs:3686-3692`; Throne dialog has
   neither — Throne `socks.uot` is model-only) + S4.
2. HTTP/HTTPS: `username, password, path, headers` + S2-optional + S4 (path/headers are parity+;
   Throne `edit_http.ui` lacks them but both engines accept them).
3. AnyTLS: `password, interval (def 30s), timeout, min` + S2-forced + S4.
4. Naïve: `username, password, uot, quic, congestion_control (bbr|bbr2|cubic|reno)` + S2-forced + S4.
5. SSH: `user, password, private_key` (textarea) + path picker + pass, `host_key + host_key_algs`
   (comma lists), `client_version` + S4.
6. VMess extras `global_padding, authenticated_length` stay import-only (Throne model-only, no
   widgets) — preserved byte-identical, never stripped, no UI in this program.

**CAP-10 — Snell / ShadowTLS / Xray-VLESS (phased) + JSON-first types.**
Intent: small matrices ship; the Xray monster is phased; JSON-first types get pickers not pages.
1. Snell: `psk, version (4|6), obfs_mode/host, userkey, reuse, network, mode` + S4 (ManyProxy-engine
   driven — verified 2026-10-10: Throne source snapshot has zero `snell` hits, so there is no Throne
   editor to copy; field list comes from `engine.rs:3772-3826` + `import.rs:1041-1055`).
2. ShadowTLS: `version (1|2|3), password` + S2-forced + S4.
3. Xray-VLESS Ph1: `xray_uuid, xray_enc (def none), xray_flow (""|xtls-rprx-vision|
   xtls-rprx-vision-udp443)` + Xray network/security/mux subset (`sni/fp/alpn,
   reality-pbk+sid+spiderx, network/mode`); Ph2 (full XHTTP+xmux matrix: `xPaddingBytes/ObfsMode/
   Key/Header/Placement/Method`, uplink `HTTPMethod/DataPlacement/DataKey/ChunkSize`,
   `noGRPCHeader/noSSEHeader`, `scMax*/scMin*/scStream*`, session `IDPlacement/Key/Table/Length`,
   `seqPlacement/Key`, `serverMaxHeaderBytes`, xmux `maxConcurrency/maxConnections/
   cMaxReuseTimes/hMaxRequestTimes/hMaxReusableSecs/hKeepAlivePeriod`, `downloadSettings` JSON)
   is a separately-ticketed follow-up.
4. `noise / custom-outbound / custom-config`: stay JSON-first with Validate; `extra-core` gains a
   path picker + args + config editor; `chain` gains a hop-list editor (ordered list + add/replace
   from library, rejecting chain/autoselector hops and enforcing ≤1 extracore at index 0 — Throne
   `listWidget+select_profile` parity). Never replicate Throne's `no_logs` non-persist bug.

**CAP-11 — Intake + row-op parity (context-menu gaps).**
Intent: getting rows in and out matches Throne's table menu where it doesn't violate local-only.
1. Clipboard intake shortcut (`Ctrl+V`-equivalent) into Import; Import keeps paste box + Browse +
   live valid/invalid preview (no behavior regression). Verified 2026-10-10: palette
   `ImportClipboard` + paste box + drag-drop exist (`main.rs:2208-2264,8887-8959`); only the key
   binding is new.
2. Duplicate exists today as `Duplicate` (`main.rs:5589-5630`, disabled copy + fresh auto port) —
   this CAP adds only a `Clone` alias/label if wanted plus the Share-submenu wiring; no new dupe logic.
3. Share: config→share-URI serializer + Copy-share-link do NOT exist in code (verified zero
   `to_uri|share_uri|encode_link` hits) — they exist only as prior spec US-78. This CAP is wired
   after US-78 lands: row Share submenu (copy link(s), copy deep links where defined).
4. QR *scan* stays out (camera/file-decode dependency); QR *render* does NOT exist in code either
   (zero `qr|qrcode` hits) — it exists only as prior spec US-79 and stays a dependency of any
   QR UI here. Per-row test/resolve reuse existing selected-scope Test/TestAll/Ping handlers
   (`main.rs:12632-12692`) — no new probe paths, no DNS-resolve button (DNS stays internal).
5. Out of scope for local-only: TUN/System-proxy-coupled profiles, `warp` endpoint design,
   `remoteroute` auto-refresh (existing guidance errors stand).

**CAP-12 — New-protocol decision (explicit, no silent JSON).**
Intent: every Throne type has a declared fate.
1. `direct`: trivial row type (no fields, verified `edit_direct.ui` label-only) — approved for build
   inside this program (new `VALID_PROTOCOLS` entry + import intake + trivial serve).
2. `mieru / juicity / trusttunnel / tailscale / autoselector / MASQUE / openvpn / openconnect`:
   decision per type — implement (full page + import + engine) or keep explicit import-reject with
   guidance pointing at `custom-config`/`custom-outbound`. Default: reject-with-guidance stays
   until a ticket explicitly approves implementation; MASQUE/OpenVPN/OpenConnect/Snell have zero
   hits in the Throne source snapshot (verified 2026-10-10) so MASQUE/OpenVPN/OpenConnect stay
   guidance-only pending newer evidence. Snell is the exception: ManyProxy already serves it, so
   its page ships under CAP-10.
3. Autoselector, if ever approved, is its own program (group/filter/balance/plan + advanced tuning
   per audit §3.2) — never smuggled into this build.

### User stories

**US-1 — AmneziaWG user edits junk-resistance without JSON (P0).**
Covers CAP-8, CAP-5, CAP-1.
As an AmneziaWG user, I want every J/S/H/I/v3 field editable with version visible, so I stop
hand-editing JSON.
Acceptance:
1. Given Add → protocol `amneziawg`, when the form renders, then WG peer/net fields +
   `enable_amnezia` + `awg_version (2.0|3.x)` + Jc/Jmin/Jmax + S1–S4 + H1–H4 + I1–I5 + v3 fields +
   Random Trailers + Disable Cookies are visible with v2-only/v3-only gating.
2. Given a `.conf` import with reserved/keepalive/MTU, when opened in Edit, then those values
   appear in their fields (nothing JSON-only).
3. Given version 3.x selected, when v2-only fields are hidden, then their stored values are
   preserved (not wiped) unless explicitly cleared.

**US-2 — VLESS subscriber pastes a Reality+Vision link and edits it (P0).**
Covers CAP-6, CAP-2, CAP-3, CAP-4.
As a VLESS user, I want flow/TLS/transport/mux editable, so imported links stay maintainable.
Acceptance:
1. Given an imported `vless://` with `security=reality&flow=xtls-rprx-vision`, when opened in
   Edit, then uuid/flow/packet_encoding + TLS (pbk/sid/sni/fp/alpn) + transport + mux state match
   the link.
2. Given `flow=xtls-rprx-vision`, when viewing Mux, then mux reads disabled with the reason shown.
3. Given ws network, when selected, then path/host/headers/early-data fields appear; switching to
   tcp hides them without wiping stored values.

**US-3 — VMess / Trojan / Shadowsocks parity (P0).**
Covers CAP-6, CAP-2, CAP-3, CAP-4.
As a VMess/Trojan/Shadowsocks user, I want the same Settings box Throne shows, so tuning works.
Acceptance:
1. Given VMess Edit, when viewed, then uuid + uuidgen button + aid stepper + security dropdown +
   packet_encoding + S1+S2+S3+S4 are present.
2. Given Shadowsocks Edit, when viewed, then method combo (18 entries) + password + plugin +
   plugin_opts + uot + Mux-only Settings (no Network/Security) are present; Advanced shows dial
   fields only (no TLS section — HasTLS gating parity).
3. Given Trojan Edit, when viewed, then password + full S1+S2+S3+S4 are present.

**US-4 — Hysteria v1/v2 + TUIC without broken rows (P1).**
Covers CAP-7, CAP-3.
As a Hysteria/TUIC user, I want the version switch and required fields enforced, so rows start.
Acceptance:
1. Given Hysteria Add, when toggling 1|2, then v1-only (auth_type+auth/recv_window(_conn)/
   disable_mtu_discovery) vs v2-only (password) layouts swap; common
   (server_ports/hop_interval/up/down/obfs) persist.
2. Given a v1 commit with up/down blank, when saving, then a typed error names the missing fields
   (no parse-ok/start-fail row); v2 with blank up/down commits (BBR-by-omission).
3. Given TUIC Edit, when viewed, then all 7 tuning fields + forced-TLS panel are present.

**US-5 — WG + SOCKS + HTTP + AnyTLS + Naïve + SSH (P1).**
Covers CAP-8, CAP-9, CAP-5.
As an operator of smaller-matrix types, I want every credential/peer field guided, so JSON is
the exception.
Acceptance:
1. Given each of socks/http/anytls/naive/ssh, when Add renders, then the CAP-9 field list for
   that type is present with pickers where specified (ssh key-file picker).
2. Given ssh with a key file, when picked, then the path fills and the key content loads without
   hand-paste; wrong-path errors name the path.
3. Given socks version 4 vs 5, when toggled, then credential requirements update (no stale error).

**US-6 — Snell / ShadowTLS / Xray-Ph1 / chain / extra-core (P2).**
Covers CAP-10, CAP-5.
As a Snell/ShadowTLS/Xray/chain user, I want the small matrices plus hop-list/picker, so edge
types are operable.
Acceptance:
1. Given Snell/ShadowTLS Edit, when viewed, then version combos + peer fields + correct TLS
   gating are present.
2. Given Xray-VLESS Edit, when viewed, then Ph1 subset fields are present; Ph2-only keys already
   in JSON are preserved untouched and validated (never stripped).
3. Given chain Edit, when managing hops, then ordered list + add/replace from library works with
   Throne rejection rules (no chain/autoselector hops, ≤1 extracore at index 0).
4. Given extra-core Edit, when picking a path, then Browse fills `path_combo`, args/config edit
   inline, Validate gates commit.

**US-7 — Intake + row ops without new probe paths (P2).**
Covers CAP-11.
As a daily user, I want clipboard-intake, Clone, and Share where Throne has them, so migration
is one click.
Acceptance:
1. Given a share link on the clipboard, when invoking paste-intake, then the Import dialog opens
   prefilled with live valid/invalid counts.
2. Given a row, when duplicating (existing `Duplicate`, plus `Clone` alias if shipped), then a new
   disabled row appears with a fresh auto port and copied params.
3. Given a row, when using Share (after US-78 serializer lands), then Copy-share-link round-trips
   through import (import of the copied link yields identical params modulo formatting).

**US-8 — New-protocol fates declared (P3 decision).**
Covers CAP-12.
As a migrant with exotic rows, I want a straight answer per type, so nothing dies silently.
Acceptance:
1. Given each of `direct/mieru/juicity/trusttunnel/tailscale/autoselector/MASQUE/openvpn/
   openconnect`, when imported, then it either opens a guided page (approved) or fails at import
   with guidance naming the nearest supported path (`custom-config`/`custom-outbound`).
2. Given `direct`, when added, then a no-fields row saves and starts (trivial final).
3. Given MASQUE/OpenVPN/OpenConnect, when imported, then guidance-only (no fabricated editor)
   until newer Throne evidence lands.

### 3. Constraints

1. Windows 10/11 64-bit, single crate edition 2021; `gpui-kit 0.6` sole UI dep (`use gpui_kit::*`),
   DESIGN.md visual world locked (flat, hairlines, one 6px radius, two text sizes, signal-only color).
   gpui-kit-maximal (this program's binding rule): every new control is a kit primitive — `Select`
   for protocol/network/mode/version dropdowns, `RadioGroup` for exclusive small sets where it reads
   better, `Switch`/`Checkbox` for toggles, `Input` for text/numeric, `Textarea` for key blobs,
   `Field`/`v_form`/`h_form` for layout, `Collapsible` for Advanced disclosure, `Button` (+
   `Button.loading` where in-flight) for generators/pickers, `Tooltip`/`Kbd`/`Empty`/`Alert` where
   they apply. Hand-rolled divs for these are forbidden; wrappers exist only at the verified 0.6.6
   limits (no `Select::tooltip` → outer wrapper, no `Tab::id` → wrapper+marker, no
   `PopupMenuItem::id`, no `StatusBar::center()`, no `Sheet::open` → `WindowExt::open_sheet`, no
   `Skeleton::shimmer` — per gpui-18-sweep R1–R14). Each ticket ledger cites the kit constructor +
   builders used.
2. Local-only always: no system proxy/TUN/DNS rewrite/routing editor/elevation/services/drivers/
   telemetry/silent downloads (`assert_no_system_mutation`-family stays green).
3. Sidecars app-dir only, never `PATH`, never downloaded; sing-box 1.14.2 schema only (never emit
   1.15.0-only fields, e.g. masque outbound).
4. Secrets: DPAPI at rest, redacted logs/URLs/labels, clipboard bind:port only except explicit
   Share/Export with verbatim warnings; no secrets in tests.
5. Params JSON stays the storage source of truth (no SQLite schema change); guided panels fold into
   it via `fold_guided_into_params`-family; blank guided inputs leave JSON byte-identical.
   Engine strictness is per-protocol, not uniform (verified 2026-10-10): strict allowlists on
   `tuic/hysteria2/socks/http/naive/snell/shadowtls` (+`DIAL_KEYS` ride-along everywhere), lenient
   `drop_foreign_flats` on the rest; unknown keys fail loudly with typo hints where strict, and
   `flow` survives only on `vless`. New panels must record their per-key shape against `engine.rs`
   in-ticket and never write keys the engine drops — with two fenced exceptions, both loud, never
   silent: brutal speeds are commit-validated numeric (garbage fails naming the field) while their
   emission stays deferred (no 1.14.2 mapping; mux-force holds), and flat-built Xray rows reject
   unmapped keys typed at generate (stored JSON still preserves them).
6. Template-button behavior change (verified destructive today, `main.rs:5351-5371` overwrites):
   type-switch rewrites the template only when the JSON is still a pristine
   `protocol_params_template(old)` output; otherwise JSON stands.
6. Hermetic tests: TEMP files only, isolated `MANYPROXY_DB`, no real sidecar spawn unless live gates
   set; verify trio per ticket (`cargo fmt --check`, `cargo clippy --all-targets --locked --
   -D warnings`, `cargo test --locked`).
7. Never invent kit APIs (established 0.6.6 limits hold — wrappers + `WindowExt` + `ParentElement`
   are the only legal paths); never replicate Throne's `no_logs`
   non-persist bug; ENTER in combo popups must not commit half-filled rows (audit §10.9).
8. gpui-kit-maximal verification: every new panel paints kit primitives with pinned ids + headless
   `assert_painted` coverage; `.small()` density + tooltip + focus + honest-footer rules from
   gpui-18-sweep hold on every touched dialog.

### 4. Non-goals

1. No engine schema inventions: no new outbound types beyond sing-box 1.14.2 + current fork/xray/
   extra-core topology; no MASQUE/OpenVPN/OpenConnect emission.
2. No Xray XHTTP full matrix in this program (Ph2 is a separate follow-up ticket).
3. No autoselector/failover, TUN/VPN mode, routing-rule editor, QR scan, telemetry, auto-update,
   PATH resolution, silent downloads.
4. No import-grammar expansion beyond declared fates (grammars keep current explicit rejects).
5. No DESIGN.md rebrand, palette/typeface change, web/mobile work.

### 5. Success signal

1. Zero JSON-only supported protocols: every `VALID_PROTOCOLS` row opens a tailored guided page;
   an independent walk of all 24 types shows no field from audit §6 editable only in JSON (except
   declared JSON-first `noise/custom-*` + Xray-Ph2 keys + declared import-only extras
   `vmess.global_padding/authenticated_length`, `wireguard.udp_timeout`, plus decided-deferred
   panel keys documented in `.planning/follow-ups/config-completeness-deferred.md`: S4 TLS-extras,
   S2 fragment/tricks, S1 headers/early-data inputs — all preserved-or-rejected loudly, never
   silently dropped).
2. AmneziaWG round-trip: `.conf` + URI imports open with version + full J/S/H/I/v3 + trailers/
   cookies visible; edits save and `sing-box check` passes.
3. No broken-row class: v1 hysteria without up/down cannot commit (v2 may omit for BBR); all commits
   validate before sidecar spawn with field-naming errors; mux panels ship only with engine emission
   (no dead keys).
4. `cargo fmt --check` + clippy `-D warnings` + `cargo test --locked` green; mutation-family green;
   headless id/string asserts cover every new panel.

## Seams

Preferred existing seams first:
1. `store.rs`: `VALID_PROTOCOLS`, `canonical_protocol`, `protocol_params_template`,
   `guided_identity_fields`, `guided_prefill_value`, `fold_guided_into_params`,
   `validate_protocol_params` — fold/prefill/validate extensions land here.
2. `main.rs` `ConfigForm` (+ `expand_merge`/`collapse_echo`/`effective_params`/`as_new`/`as_patch_vs`)
   + headless `assert_painted` id/string asserts — all panel UI + tests land here. Includes the
   template-overwrite→pristine-only change and type-lock + virtual-type address/port hiding (both
   verified absent today: type editable `main.rs:1726,2192`, Server/Port unconditional
   `main.rs:1926-1951`).
3. `engine.rs` `translate_outbound` + `check` — mux emission is new work (zero handling today);
   all other panels preserve-or-write keys the engine already maps (per-key shape recorded
   in-ticket); `import.rs` grammars unchanged except `direct` intake if approved (link schemes are
   31 in `VALID_LINK_SCHEMES`, not 34 — audit count corrected 2026-10-10).
4. New seams only if unavoidable: one capability-flag table (HasTLS/HasTransport/HasMux/MustTLS per
   protocol mirroring Throne), one per-protocol field-spec table driving form swap + fold, one
   hop-list helper (chain), one file-picker helper (ssh/extra-core/cert). Nothing per-row start
   tracking.

## Boundaries

**Always**
- Smallest scope per ticket; P0 first (AWG+WG, VLESS, VMess, SS) before P1/P2/P3.
- Params JSON is truth; guided blanks never wipe; unknown keys fail loudly with hints.
- Secrets discipline: DPAPI, redaction, masked inputs + reveal, picker paths never echo contents.
- Pinned ids/strings stay green or update same-ticket with ledger rationale.
- Live user DB never touched by tests; screenshots use synthetic `*.example.com` rows only.

**Ask first**
- Approving any CAP-12 implementation (mieru/juicity/trusttunnel/tailscale/autoselector/MASQUE/
  openvpn/openconnect) beyond guidance-only.
- Xray Ph2 full-matrix build (separate program).
- DESIGN.md token changes; tray/autostart semantics touches.

**Never**
- System mutation, `PATH` sidecars, plaintext fallbacks, silent downloads/telemetry.
- Fabricated MASQUE/OpenVPN/OpenConnect editors or 1.15.0-only emission.
- Throne `no_logs` bug copy; ENTER-commit of half-filled combo rows.
- Screenshots with real hosts/credentials/tokens.

## Assumptions

ASSUMPTIONS I'M MAKING:
1. sing-box 1.14.2 schemas underlying current `engine.rs`/`import.rs` are correct except the verified
   mux gap (zero handling — new emission needed, recorded per-key in-ticket).
2. `ConfigForm` guided→JSON fold pattern scales to S1–S4 panels (no form rewrite needed).
3. gpui-kit 0.6 `Select`/`RadioGroup`/`Input`/`Textarea` cover dropdown/stepper/combo needs (no new UI dep).
4. File pickers reuse the existing Import-Browse pattern (no new platform crate).
5. Chain hop rules (Throne rejections) are desired verbatim.
→ Correct me now or I'll proceed with these.

## Verification

- Every CAP has ≥1 US scenario with Given/When/Then observable outcomes (fields visible, values
  round-trip, errors name fields) — no "works correctly".
- Automated: store fold/prefill/template/validate unit tests per protocol; engine `check` on
  panel-produced JSON; headless GPUI id/string asserts per panel + layout-toggle tests (hysteria
  1|2, AWG 2.0|3.x, network conditionals, mux-force, edit-mode locks).
- Live proof opt-in: isolated `MANYPROXY_DB` demo seed, per-protocol Add→Edit round-trip captures.
- Capability evals: pass@3 ≥ 0.90 on panel suites. Regression: full suite green (pass^3 = 1.00) +
  mutation-family green.
- Anti-patterns avoided: seeded demo rows only; no sleeps in in-flight asserts; no whole-temp-dir scans.

## Companions

- Field truth: `research/config-completeness-throne-gap.md` §§3–6 + §10 (visual log).
- Design: `plan.md`, `research.md`, `data-model.md`, `contracts/`, `quickstart.md` (this dir).
- System of record: `PRODUCT.md`, `DESIGN.md`, `AGENTS.md`, `.planning/spec.md`.

## Sources

1. Audit `research/config-completeness-throne-gap.md` (2026-10-10): §§1–2 method/chrome, §3 exact
   Throne fields, §4 ManyProxy state, §5 gap table, §6 page spec, §7 build order, §10 visual log
   (WG+Amnezia, VLESS, VMess+Advanced, SS, context menu, Type list, backlog, incident).
2. Source verification 2026-10-10 (this program, read-only): Throne `include/ui/profile/*.ui` (53
   files) + `src/ui/profile/*.cpp` behaviors + `include/configs/outbounds/*.h` capability flags +
   `OutboundFactory.cpp` type list (no masque/openvpn/openconnect/snell hits); ManyProxy
   `src/store.rs:26-50,855-1147` (23 protocols, aliases, templates, guided map, fold),
   `src/main.rs` ConfigForm + `Duplicate:5589-5630` + missing Share/QR + no Ctrl+V binding,
   `src/import.rs:1572-1634` (23 JSON types, 31 link schemes — audit 34 corrected) + rejects,
   `src/engine.rs` per-protocol allowlists vs `drop_foreign_flats` + `DIAL_KEYS` + zero mux handling
   + v1-require/v2-optional up-down + wireguard coercions. False positives fixed: scheme count,
   mux engine gap, v1-only gate, socks/http model-only vs parity+, Xray network values, Snell
   sourcelessness, Duplicate-exists vs Share/QR-missing, template-overwrite→pristine-only.
3. Prior specs: `.planning/spec.md` (CAP-4 protocol parity), `.planning/spec-complete.md` (US-78/79
   share/QR as unbuilt deps, US-8 dupe), `.planning/gpui-18-sweep/spec.md` (form/overlay rules).
4. User 2026-10-10: "all of these things that this app doesnt have to be planned and specced out" +
   "research and refine from the sources, fix false positives and missing info, then continue".

## Maintain

| Role | Owner (reuse in place, link don't copy) |
| ---- | --------------------------------------- |
| Constitution | `AGENTS.md` + DESIGN.md Do's/Don'ts. This spec adds nothing to governance. |
| Map | This spec owns config-completeness; `plan.md` owns order; tickets own slices; the audit stays evidence. |
| Status | Open until P0–P2 green, then P3 decisions; ledger records each ticket close. Delete-zone: none yet. |
| History | Throne-gap audit (2026-10-10) + §10.9 no-live-drive rule are durable lineage. |

Next (not executed here): `/plan` → `/to-tickets` → `/decide` → `/implement`.
