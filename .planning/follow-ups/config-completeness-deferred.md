# Follow-ups: config-completeness deferred items (explicit, not dropped)

Source: `.planning/config-completeness` ledger Rulings (T02–T08). None of these block the
program's success signal; each needs its own ticket when scheduled. No silent JSON: every item
is either preserved-not-emitted today or guidance-only by decision.

## Engine-mapped later (panels deferred to keep `check` green)

1. **S4 TLS-extras UI** (`disable_sni`, `min/max_version`, `enable_ech` + server/config,
   `cert_sha256`, `client_cert/key`, `certificate` content) — needs engine `tls{}` mapping first;
   T02/T05 shipped the dial half only.
2. **S2 fragment/tricks UI** (`fragment` + fall-delay, `tls_rec_frag`, `tricks`) — same gate.
   Hand JSON carrying them rides lenient passthrough to a loud `check` failure (never silent).
3. **S1 `headers`/early-data inputs** — no panel inputs by decision; hand JSON with a non-trivial
   `headers` map now fails typed at generate naming the field (`apply_transport` review fix;
   blank/empty shapes stay no-ops; the `http`-protocol arm keeps path/headers via its own
   allowlist). Early-data still rides to a loud `check` failure.
4. **Brutal speeds emission** (`brutal_up/down_mbps`) — consumed + preserved, never emitted (no
   sing-box 1.14.2 mapping); mux-force holds via folded `mux:"on"`. Garbage now fails at commit
   naming the field (store numeric gate, review fix) instead of riding silently.

## Phased programs (separate tickets by design)

5. **Xray XHTTP full matrix (Ph2)** — ~30-field `xPaddingBytes`/uplink/session/xmux/
   `downloadSettings` set (spec CAP-10.3); Ph1 shipped, Ph2-only keys preserved + validated.
6. **Share wiring (US-78)** — serializer absent in-tree at T08; Share submenu deferred until the
   US-78 program lands (T08 Ruling; guidance errors stand).
7. **New-protocol implementations** — mieru/juicity/trusttunnel/tailscale/autoselector/MASQUE/
   openvpn/openconnect stay import-reject-with-guidance → `custom-config`/`custom-outbound`
   (spec CAP-12); autoselector is its own program if ever approved.

## Verify-before-close (manual, opt-in)

8. **SSH Browse click-through** — left to manual review per T06 (headless pins the wiring).
9. **Live `sing-box check` on panel JSON** — hermetic `generate()`-shape proofs ship per ticket;
   run real `check` under the live gates (`MANYPROXY_LIVE_CHECK=1`) before release.
10. **QR render (US-79)** — untouched by this program; remains the intake/row-op dependency.
