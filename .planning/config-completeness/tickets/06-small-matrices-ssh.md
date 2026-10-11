# 06: SOCKS / HTTP(S) / AnyTLS / Naïve / SSH pages

**What to build:** The five smaller-matrix pages with their credential/peer fields + pickers (ssh key-file Browse) — all kit primitives, correct TLS gating per page.

**Blocked by:** 05-hysteria-tuic (same write surface — linear).

**Owns:** gpui-kit composition (kit-maximal: `Select`/`Switch`/`Input`/`Textarea`/`Button`/`Field`), sqlite (fold/prefill/validate, no migration).

**Status:** ready-for-agent

- [ ] Scenario: each of socks/http/anytls/naive/ssh Add → Action: read form → Expected: CAP-9 kit field lists present (socks version 5|4 + parity+ network/udp_over_tcp; http path/headers parity+; anytls interval/timeout/min; naive uot/quic/cc-4; ssh user/password/textarea key + path picker + pass + host_key/algs + client_version) + correct S2/S4 gating. Verification: automated test (headless ids per type). Priority: required.
- [ ] Scenario: ssh key file → Action: Browse pick → Expected: path fills, key loads without hand-paste; wrong path names the path. Verification: automated test + manual UX review. Priority: important.
- [ ] Prohibited side effect: no hand-rolled file inputs (kit `Button` + existing Browse pattern); no invented kit APIs; socks-version≠5 / http-version presence keep typed engine rejects.

Retry cap: 2, then escalate.
