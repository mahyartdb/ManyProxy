# 08: Intake shortcut + Clone alias + Share wiring + direct fate

**What to build:** Paste-intake key binding, `Clone` alias over existing `Duplicate`, Share-submenu wiring (after US-78 serializer lands), trivial `direct` row type, exotic-type guidance intact — all kit primitives, no new probe/fetch paths.

**Blocked by:** 07-edge-xray-chain-extracore (same write surface — linear).

**Owns:** gpui-kit composition (kit-maximal: menu/palette/toolbar entries + `Kbd` hints only where bound).

**Status:** ready-for-agent

- [ ] Scenario: share link on clipboard → Action: invoke paste-intake binding → Expected: Import dialog opens prefilled with live valid/invalid counts (existing preview, no regression). Verification: automated test (binding dispatches same handler as palette `ImportClipboard`) + manual UX review. Priority: required.
- [ ] Scenario: row → Action: Duplicate/Clone → Expected: new disabled row, fresh auto port, identical params (existing `commit_duplicate` logic; alias only). Verification: automated test. Priority: required.
- [ ] Scenario: row → Action: Share copy-link (post-US-78) → Expected: copied link round-trips through import (identical params modulo formatting). Verification: automated test. Priority: important.
- [ ] Scenario: `direct` Add → Action: save → Expected: no-fields row saves and serves as trivial final (new `VALID_PROTOCOLS` entry + import intake, no migration). Verification: automated test. Priority: required.
- [ ] Scenario: exotic import (mieru/juicity/trusttunnel/tailscale/MASQUE/openvpn/openconnect) → Action: import → Expected: guidance error naming `custom-config`/`custom-outbound` (no fabricated editors, no 1.15.0 emission). Verification: automated test. Priority: required.
- [ ] Prohibited side effect: no QR scan; no TUN/warp/remoteroute smuggling; no new probe paths; no hand-rolled menu divs (kit menu primitives); `Kbd` only on bound actions.

Retry cap: 2, then escalate.
