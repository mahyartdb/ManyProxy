# 04: WireGuard + AmneziaWG full pages (headline)

**What to build:** WG peer/net fields + keygen/warp-autogen-equivalent and the full AWG matrix (2.0|3.x switch, J/S/H/I, v3 fields, Random Trailers + Disable Cookies) — all kit primitives, version-gated, `.conf` values round-trip.

**Blocked by:** 03-vmess-trojan-shadowsocks (same `ConfigForm`/`store.rs` write surface — linear to avoid collisions).

**Owns:** gpui-kit composition (kit-maximal: `Select`/`RadioGroup`/`Switch`/`Input`/`Textarea`/`Button`/`Field`), sqlite (fold/prefill/validate, no migration).

**Status:** ready-for-agent

- [ ] Scenario: Add `wireguard` → Action: read form → Expected: kit inputs private_key + public_key + preshared_key + reserved + keepalive + local_addr + mtu + workers + sys_ifc + kit `Button` keygen/warp-autogen + S4. Verification: automated test (headless ids). Priority: required.
- [ ] Scenario: Add `amneziawg` → Action: read form → Expected: all WG fields + enable_amnezia + kit `Select`/`RadioGroup` awg_version 2.0|3.x + jc/jmin/jmax + s1–s4 + h1–h4 + i1–i5 + v3 fields + trailers/cookies switches with v2/v3 gating. Verification: automated test (version-toggle hide-preserves, never wipes). Priority: required.
- [ ] Scenario: `.conf` import with reserved/keepalive/MTU → Action: open Edit → Expected: values sit in kit fields (nothing JSON-only). Verification: automated test (prefill round-trip). Priority: required.
- [ ] Prohibited side effect: `udp_timeout` stays import-only (never stripped); no hand-rolled divs; no invented kit APIs; trailers/cookies engine pass-through-or-preserve verified against fork `check` in-ledger.

Retry cap: 2, then escalate.
