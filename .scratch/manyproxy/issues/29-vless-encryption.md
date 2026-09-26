# 29: VLESS encryption routing + ANSI-clean engine errors

**What to build:** VLESS rows carrying `encryption` start successfully instead of dying in `sing-box check` with `unknown field "encryption"`, and engine error toasts are human-readable.

**Blocked by:** none (owns engine.rs emission paths).

**Owns:** rust-networking

**Status:** ready-for-agent

Findings (live-proven 2026-09-26):
- Stock sing-box 1.14.2 strict-decodes and rejects the `encryption` key on vless outbounds — even `"none"`. A real user row (`encryption` + `packetEncoding` camelCase + ws opts) fails Start-all with `outbounds[0].encryption: json: unknown field "encryption"`.
- Xray 26.3.27 is the sidecar for Xray-only shapes; verify with `xray check` whether it accepts the VLESS `encryption` value, and route accordingly.
- Toast shows raw ANSI (`[31mFATAL[0m`) from check stderr — strip escape codes before surfacing on all four backends.

Fix direction (verify each against the real binaries in target/debug):
- `encryption` absent/`"none"` → drop the key, stay stock (do NOT pass through).
- Real `encryption` value → Xray sidecar VLESS outbound with fields passthrough (verify `xray check` accepts; packetEncoding naming per xray schema). If xray rejects, typed error naming the field + required sidecar.
- Strip ANSI escapes from check stderr on stock/fork/xray/extra paths (keep the FATAL text).

- [ ] Scenario: vless row with encryption=none → Action: start → Expected: runs on stock, no unknown-field error. Verification: automated test. Priority: required.
- [ ] Scenario: vless row with real encryption value → Action: start → Expected: served via xray sidecar on its own endpoint (or typed error if xray rejects the value). Verification: automated test. Priority: required.
- [ ] Scenario: failing check → Action: read toast → Expected: plain text, no escape codes. Verification: automated test. Priority: important.
- [ ] Prohibited side effect: no change for rows without encryption; no secrets in errors.
