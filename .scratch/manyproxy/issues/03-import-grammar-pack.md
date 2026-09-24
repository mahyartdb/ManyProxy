# 03: Import grammar pack

**What to build:** Throne-compatible import: paste/clipboard/drag-drop/file of share links (vless, vmess both forms, trojan, shadowsocks triple grammar, hysteria2+realm/v1, tuic, wireguard URI+conf, socks/anytls/http), JSON outbounds arrays, with per-item errors and a fixture corpus. No subscriptions yet.

**Blocked by:** 02 (Library CRUD + store).

**Owns:** api-and-interface-design

**Status:** ready-for-agent

- [ ] Scenario: clipboard with 5 mixed valid links → Action: paste-import into a group → Expected: 5 disabled records with correct host/port/protocol shown. Verification: automated test. Priority: required.
- [ ] Scenario: batch with 2 broken items → Action: import → Expected: valid items import, failures reported with item index + reason, no partial silent import. Verification: automated test. Priority: required.
- [ ] Scenario: sing-box outbounds JSON array → Action: import file → Expected: all entries land in the chosen group. Verification: automated test. Priority: important.
- [ ] Prohibited side effect: import never starts configs, never duplicates on re-import of identical body, never crashes on malformed base64/JSON.
