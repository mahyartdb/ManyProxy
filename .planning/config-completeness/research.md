# Research: config-completeness (Throne parity)

All NEEDS CLARIFICATION resolved. Decisions below are build directives; rationale + alternatives recorded.

## R1 — Capability flags per protocol (Throne mirror)

Decision: hardcode one table in `store.rs` (or its UI-adjacent module): per canonical protocol
`{ has_transport, has_tls_optional, must_tls, has_mux }` mirroring audit §3 capability flags:
- `trojan, vless, vmess`: transport + optional-TLS + mux.
- `http, https`: optional-TLS only (+ parity+ path/headers fields).
- `shadowsocks`: mux only.
- `hysteria, hysteria2, tuic, anytls, naive, shadowtls`: must-TLS (locked, no toggle).
- `socks, wireguard, amneziawg, ssh, snell, noise, xray, custom-*, extra-core, chain`
  (+ future `direct`): no shared panels; inner fields only (xray gets its own Xray-panel set).
Rationale: one source drives panel attach, Advanced TLS-section gating, and SS mux-only proof
(visual §10.4). Alternatives: per-page booleans (rejected — drifts); runtime capability sniffing
of JSON (rejected — JSON is user-editable, flags must be protocol facts).

## R2 — Guided→JSON fold stays the commit contract (no schema change)

Decision: extend `fold_guided_into_params`-family, don't replace. Each panel contributes
`(key, value, blank-means-absent)` pairs; blank guided inputs leave JSON byte-identical;
type-switch rewrites the template only when JSON is still a pristine
`protocol_params_template(old)` output. Unknown keys keep strict per-protocol rejection with
typo hints (`translate_outbound` warn path).
Rationale: storage truth stays params JSON (zero migration, DPAPI untouched, engine strictness
unchanged); audit §4.1/§4.3 already proves the pattern. Alternatives: typed columns per field
(rejected — v5-scale migration for ~100 fields); separate sidecar struct (rejected — dual truth).

## R3 — Transport/TLS/Mux key mapping (sing-box 1.14.2, current engine)

Decision: panels write the flat v2ray-style keys `engine.rs` already translates
(`security/sni/fp/alpn/pbk/sid→tls{}`, `network/path/host/headers/method/service_name/
early-data→transport{}`, `up/down→up_mbps/down_mbps`, wireguard→`endpoints[]+peers[]`).
No new emission keys in this program except mux (R3b); Xray-Ph1 writes existing `xray_*` keys only.
Strictness is per-protocol (verified 2026-10-10), not uniform: strict allowlists
(`reject_unknown_params`) on tuic/hysteria2/socks/http/naive/snell/shadowtls (+`DIAL_KEYS`
ride-along: `bind_interface,routing_mark,reuse_addr,connect_timeout,tcp_fast_open,
tcp_multi_path,udp_fragment,netns,domain_resolver`); lenient `drop_foreign_flats` elsewhere
(silently drops inapplicable flats incl. `security/sni/flow/encryption/up/down/obfs/psk`);
`flow` survives only on vless; `certificate_sha256` pre-rejected for all (1.15+;
1.14.2 pins via `certificate_public_key_sha256`); `socks version` must be 5; `http version`
presence rejects (1.15+); `kcp` network preserved at import, typed-fail at engine; `ss plugin`
two-stage (empty fails import, non-empty fails engine); `hysteria2+realm` link rejects but JSON
native `realm` passes through. Each panel ticket records its per-key shape against `engine.rs`.
Rationale: import already covers these fields — dialog is the bottleneck (§4.3). Alternatives:
direct nested-JSON editing (rejected — breaks fold + strict validation).

## R3b — Mux has no engine mapping (new scope, blocks S3 UI)

Decision: `engine.rs` has zero `mux|multiplex` handling (verified 2026-10-10) — S3 UI ships only
with its engine arm (emission for ss/trojan/vless/vmess + `check`-green proof in the same slice).
Never UI-first with dead keys.
Rationale: panels writing keys the engine drops/rejects would be a dishonest form. Alternatives:
defer S3 (rejected — S3 is P0 core for the big four).

## R4 — Widget mapping (gpui-kit 0.6 maximal, no new UI dep)

Decision: every control is a kit primitive — `Select` for protocol/network/mode/version/flow/
security/packet-encoding/cc/relay dropdowns; `RadioGroup` (`selected_index` + `on_change`) for
mux-mode/fragment/tricks/version switches where grouped-exclusive reads better (one group per
choice, no bracketed-text conventions, per gpui-18-sweep CAP-3); `Switch`/`Checkbox` for
insecure/reuse/fast-open/frag/multipath/brutal/uot/quic/health/ephemeral/sys_ifc/trailers/cookies;
`Input` (masked + reveal for secrets) for text/numeric with validation; `Textarea` for
private_key/host blobs; `Field` + `v_form`/`h_form` + `field()` for layout with label-over-field;
`Collapsible` for Advanced disclosure (guided↔JSON swap never stacks); `Button` for
uuidgen/keygen/warp-autogen/Browse pickers; `Tooltip` on every truncating value, `Kbd` only where
a binding exists, `Empty` for empty hop-lists, `Alert` for commit errors. Hand-rolled divs for any
of these are forbidden. Wrappers only at verified 0.6.6 limits (R1–R14). Combo popups consume
ENTER (no fall-through to dialog commit — incident §10.9). Type-switch rewrites the template only
on pristine JSON (current `main.rs:5351-5371` overwrites destructively — behavior change in-scope).
Each ticket ledger cites kit constructor + builders used.
Rationale: stays inside sole-UI-dep + DESIGN.md locked world + gpui-18-sweep form language.
Alternatives: new picker crates (rejected — unneeded surface); hand-rolled dropdowns (rejected —
kit owns them).

## R5 — Conditional visibility without data loss

Decision: hiding a conditional section (ws→tcp, v2→v1-only fields, AWG 2.0↔3.x) hides inputs but
preserves stored JSON values; re-showing echoes them. Mux-force (`brutal` on, `flow=vision`)
disables the mux input with reason text, never strips the key.
Rationale: Throne-toggled layouts must not destroy imported links. Alternatives: clear-on-hide
(rejected — data loss).

## R6 — Edit-mode locks (Throne §7.7 parity)

Decision: `type` locked on edit; address/port hidden for `chain/custom-*/extra-core/direct`;
`advanced_button` equivalent (Advanced disclosure) always available (ManyProxy keeps JSON visible
— stricter than Throne's hide list, deliberate).
Rationale: prevents type-change orphaning params; virtual rows have no endpoint address.
Alternatives: free type-switch on edit (rejected — params-template mismatch).

## R7 — AWG v3 + installed-build extras

Decision: spec includes `awg_version 2.0|3.x`, all v3 fields, plus `Random Trailers` +
`Disable Cookies` (audit §10.1 — installed 1.3.2 newer than source). Engine mapping for the two
checkboxes: pass through as AWG keys if fork accepts, else preserve-and-validate (never strip);
ticket must verify against fork `check` and record outcome in ledger.
Rationale: product-newer-than-source means spec leads source. Alternatives: omit until source
updates (rejected — user runs the newer build).

## R8 — Xray phasing

Decision: Ph1 (common subset) in-program; Ph2 full XHTTP+xmux matrix as a separately-ticketed
follow-up with its own verify (generated Xray config `check`). Ph1 must preserve unknown `xray_*`
keys byte-identical.
Rationale: Ph2 matrix is ~30 fields (§3 Xray panels) — smuggling it into P2 blows up scope.
Alternatives: full matrix now (rejected — scope).

## R9 — Chain + extra-core rules

Decision: chain hop editor enforces Throne rules verbatim (ordered list, add/replace from library
select-mode, reject chain/autoselector hops, ≤1 extracore only at index 0). Extra-core gets path
picker + args + config + Validate; `no_logs` intentionally omitted (Throne non-persist bug — never copy).
Rationale: verbatim rules prevent unservable chains. Alternatives: free hop pick (rejected).

## R10 — Intake/row-ops: what ships vs stays out

Decision: clipboard-intake key binding + `Clone` alias (logic exists as `Duplicate`,
`main.rs:5589-5630`) + Share-submenu wiring ship (all reuse existing handlers/seams — no new
probe/fetch paths). Share serializer + QR render do NOT exist in code (verified zero hits) — they
are prior-spec US-78/US-79 deps that must land first; this program wires them, never rebuilds them.
QR scan, TUN-coupled profiles, `warp` endpoints, `remoteroute` auto-refresh stay out with guidance
errors preserved. Per-row test/resolve reuse selected-scope Test/TestAll/Ping (no inline probe
buttons, no DNS-resolve button — DNS stays internal).
Rationale: local-only + smallest-scope; no duplicate logic. Alternatives: full context-menu clone
incl. speedtest/resolve rows (rejected — new probe paths).

## R11 — New-protocol fates

Decision: `direct` approved (trivial, no fields — verified label-only `edit_direct.ui`). All others
stay import-reject-with-guidance until a dedicated ticket approves implementation with engine schema
proof; MASQUE/OpenVPN/OpenConnect/Snell have zero hits in the Throne source snapshot (verified
2026-10-10) so MASQUE/OpenVPN/OpenConnect are guidance-only (installed 1.3.2 newer than source —
needs newer evidence per §10.7–10.8). Snell is the exception: ManyProxy already serves it
(`import.rs` + `engine.rs:3772-3826` arms exist), so its page ships engine-driven under CAP-10.
Autoselector is its own program if ever approved.
Rationale: explicit fates end silent deaths without fabricating unsupported outbounds (never emit
1.15.0-only fields).
Owning skill refs: none (product decision, not a domain-skill call).

## R13 — Source-verification record (2026-10-10, read-only)

- Throne: `include/ui/profile/*.ui` (53 files) dumped; `dialog_edit_profile.cpp:500-520` gating
  (address/port hidden for chain/autoselector/direct/4×custom/extracore/tailscale; advanced hidden
  for chain/autoselector/extracore/tailscale/customs); `stream_box`/`network_box`/`security_box`/
  `tls_camouflage_box`/Xray boxes + `edit_advanced.ui` confirmed; per-protocol widgets + dropdown
  counts (ss-method 18, plugin 3, vmess-security 5, packet-encoding 3, vless-flows 1, xray-flows 2,
  tuic-cc 3 / relay-UI 2, naive-cc 4, trusttunnel-cc 6, mieru 2/5, shadowtls 3, socks 2, hysteria 2+3)
  confirmed; capability flags (HasTLS 11 / MustTLS 7 / HasTransport 3 / HasMux 4) confirmed;
  factory has no masque/openvpn/openconnect/snell; hysteria-toggle + warp_autogen + IsXray + MustTLS
  + vision-mux-off + brutal-forces-mux + fragment-fall-delay-disable confirmed; `extra_core.no_logs`
  dead (checkbox without persist); model-only fields (socks.uot, http.path/headers,
  vmess.global_padding/authenticated_length, wireguard.udp_timeout) confirmed import-only.
- ManyProxy: 23 `VALID_PROTOCOLS` + aliases + templates + guided map + fold semantics confirmed
  (`store.rs:26-50,855-1147`); type editable + Server/Port unconditional + edit-starts-expanded +
  template-overwrite confirmed (`main.rs:1708,1726,1926-1952,2192,5351-5371`); 23 JSON types + 31
  link schemes (audit 34 corrected) + Clash + WG-conf + two-stage rejects confirmed
  (`import.rs:1572-1634,1995-2280,2949+`); per-protocol allowlist-vs-lenient + DIAL_KEYS + zero-mux +
  v1-require/v2-optional + coercions confirmed (`engine.rs`); Duplicate-exists vs
  Share/QR-missing vs no-Ctrl+V confirmed (`main.rs` greps).
- False positives fixed in spec: scheme count 34→31; S3 without engine; v1-only gate; socks/http
  model-only vs parity+; Xray network values; Snell sourcelessness; Duplicate-vs-Share/QR status;
  template-overwrite→pristine-only.

## R12 — ENTER-commit guard (incident §10.9)

Decision: combo/dropdown popups consume ENTER/RETURN (no fall-through to dialog commit); commit
requires explicit Save with validation. Every new dropdown ticket carries a regression test.
Rationale: audit incident created + deleted real rows. Alternatives: none (safety fix).
