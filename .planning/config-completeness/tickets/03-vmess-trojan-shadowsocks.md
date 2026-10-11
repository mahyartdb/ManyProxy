# 03: VMess / Trojan / Shadowsocks pages (reuse S1–S4)

**What to build:** VMess (uuid+uuidgen, aid, security, packet_encoding), Trojan (password), Shadowsocks (18-method combo, password, plugin+opts, uot) pages reusing the T02 shared panels — all kit primitives, Mux-only Settings on SS.

**Blocked by:** 02-vless-shared-panels-mux-engine (shares S1–S4 + mux engine arm).

**Owns:** gpui-kit composition (kit-maximal: `Select`/`RadioGroup`/`Switch`/`Input`/`Button`/`Field`), sqlite (fold/prefill/validate, no migration).

**Status:** ready-for-agent

- [ ] Scenario: VMess Edit → Action: read form → Expected: kit `Input` uuid + kit `Button` uuidgen + kit `Input` aid stepper + kit `Select` security (auto|zero|none|chacha20-poly1305|aes-128-gcm) + kit `Select` packet_encoding + S1+S2+S3+S4. Verification: automated test (headless ids + dropdown counts 5/3). Priority: required.
- [ ] Scenario: Shadowsocks Edit → Action: read form → Expected: kit `Select` method (18 entries) + kit `Input` password + kit `Select` plugin (`""|obfs-local|v2ray-plugin`) + plugin_opts + kit `Switch` uot + Mux-only Settings (no Network/Security); Advanced shows kit dial fields only (no TLS section — HasTLS gating). Verification: automated test. Priority: required.
- [ ] Scenario: Trojan Edit → Action: read form → Expected: password + full S1+S2+S3+S4 as kit primitives. Verification: automated test. Priority: required.
- [ ] Prohibited side effect: no hand-rolled dropdowns/divs; no invented kit APIs; `vmess.global_padding/authenticated_length` preserved import-only (no UI, never stripped); hidden keys preserved.

Retry cap: 2, then escalate.
