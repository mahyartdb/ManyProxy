# Contracts: config-completeness

## C0 — Source-verified baselines (2026-10-10, read-only)

- Import: 23 JSON outbound types (`import.rs:1572-1596`), 31 link schemes (`import.rs:1602-1634`,
  audit 34 corrected), Clash `proxies:` block-style + inline JSON, WG `.conf` multi-peer with AWG
  detect, two-stage rejects (kcp preserved→engine-fail; ss-plugin empty→import-fail vs
  non-empty→engine-fail; http-version/cert_sha256→engine 1.15+ reject; socks-version≠5→engine fail;
  hysteria2+realm link-reject vs JSON-realm passthrough; ssr Clash-only skip).
- Engine: strict allowlists (tuic/hysteria2/socks/http/naive/snell/shadowtls) vs lenient
  `drop_foreign_flats` elsewhere; `DIAL_KEYS` ride-along everywhere; zero `mux|multiplex` handling
  (S3 engine work required); v1 up/down required (`engine.rs:3468-3495`) vs v2 optional
  (`engine.rs:3544-3557`); `flow` survives only on vless; wireguard coercions
  (allowed_ips default, reserved/mtu/keepalive) + `dns` stripped.
- UI today: type editable, Server/Port unconditional (no virtual-type hiding), edit starts expanded,
  template button overwrites destructively, `Duplicate` exists (`main.rs:5589-5630`), Share/QR do
  not exist in code (prior-spec US-78/79 deps), no Ctrl+V binding (palette path exists).

## C1 — Guided fold contract (extends `fold_guided_into_params`)

- Each panel field maps to exactly one params key (flat v2ray-style keys per R3; `xray_*` keys on
  the xray page; AWG `jc/jmin/jmax/s*/h*/i*/v3-*` keys on the AWG page).
- Blank input ⇒ key absent-or-untouched (JSON byte-identical for that key); non-blank ⇒ key written
  (numbers as numbers, toggles as bools, comma-lists as split arrays/strings per existing engine
  convention — ticket records the per-key shape against `engine.rs`).
- Prefill (`guided_prefill_value`-family) reads across aliases (`uuid/id`, `password/auth`,
  `private_key/privatekey`) case-insensitively; numbers stringify.
- Type-switch rewrites params **only** when current JSON equals
  `protocol_params_template(old type)` byte-for-byte; otherwise JSON stands and the new page
  prefills whatever keys overlap.

## C2 — Template contract (extends `protocol_params_template`)

- Per-type template = `{type, server:"", server_port}` + blank credential slots (existing) —
  unchanged. Panels never add non-blank defaults (no fabricated `flow`, `method`, `version`).
- Ports: 51820 wg/awg, 22 ssh, 1080 socks, else 443 (unchanged).

## C3 — Validation errors (extends `validate_protocol_params`)

- Every required-field failure names `"<row>": <field> <reason>` (e.g. `hysteria needs up_mbps and
  down_mbps`, `amneziawg needs private_key and public_key`).
- Unknown keys: strict reject with typo hint (`securty → did you mean security?`) — no silent drops.
- Reserved clash/Xray-bridge ranges, bind/privileged-port rules unchanged.

## C4 — Import contract (unchanged except `direct`)

- Current 31 schemes + 23 JSON types + explicit rejects with guidance stand (C0). Only addition:
  `direct` intake (trivial, no fields) if CAP-12 approves in-ticket.
- Exotic links (mieru/juicity/trusttunnel/tailscale/MASQUE/openvpn/openconnect) keep
  reject-with-guidance naming `custom-config`/`custom-outbound` until a dedicated ticket ships them.
  (Snell already serves — not exotic.)

## C5 — UI contract (pinned ids/strings, gpui-kit 0.6)

- New ids: `protocol-select`, per-panel `s1-network`, `s2-*`, `s3-*`, `s4-*`, per-protocol inner ids
  (`vless-flow`, `vmess-aid`, `ss-method`, `hy-version`, `awg-version`, `chain-hops`, …) — tickets
  pin the exact list; headless `assert_painted` covers each panel's presence + conditional gating
  (ws extras, v1/v2 layouts, AWG 2.0/3.x, mux-force, edit-mode locks).
- Honest-toast + masked-secret + redaction + bind:port-only clipboard rules apply to every new
  control; ENTER in popups never commits (R12 regression test per dropdown ticket).
- Focus: dialog opens focused on first field; validation errors link to fields and move focus.
