# 24: Parser criticals, credential integrity, redaction (import.rs)

**What to build:** No panics, no silent credential overwrites, no redaction bypasses, consistent verdicts across link/JSON/Clash paths.

**Blocked by:** none (owns import.rs).

**Owns:** api-and-interface-design

**Status:** done (commit 8b604d6, import.rs only; full-suite re-run deferred to 23 landing — tree mid-edit; 287 passed in scratch copy)

Findings to fix (all src/import.rs, verify each in code before fixing):
- CRITICAL byte panic (1782-1785 + store.rs:975-978 same class): `url[..7]`/`[..8]` and `trimmed[..7]` split UTF-8 → use strip_prefix/get. Both sites.
- HIGH extras-overwrite (529-580 consumed list; 795-797 hy1 empty; 825-828 tuic; 850-853/879-882 wg/awg; 772-778 hy2; 902-922/938-940/966-969 socks/http/anytls/naive): query extras applied AFTER core fields with incomplete consumed lists, so `?password=fake`/`?server=evil`/`?uuid=` silently win. Fix: core fields (userinfo/authority-derived) always win — apply extras first or skip keys that collide with core + consumed.
- HIGH redaction H7 (2128-2130 vs split_uri rfind 380-382): mask to LAST @, not first. H8: opaque no-@ payloads (vmess/throne/ss base64) must mask the payload, not echo it. H9: `?bare-key` pairs must be scrubbed, not kept verbatim.
- HIGH empty host (605-614): `"add":"[]"` → empty host success. Reject empty post-strip; fix `[::1` bracket handling.
- MED M1 path drop (376-379): don't silently drop URI path — preserve into transport opts or typed error.
- MED M2 host decode: percent-decode host + normalize before identity so link/JSON agree.
- MED M3 legacy vmess query/fragment ignored (1450-1455): honor #remark at minimum.
- MED M4 JSON credential checks (1072-1103): require credentials for credential-needing types like links do (keep full-config passthrough only for Custom kinds).
- MED M5 cipher→method map (2087-2093) so Clash SS dedupes with links.
- MED M6 type/protocol conflict (1076-1094): single canonical, never side-by-side.
- MED M7 email-as-name (1088): drop email from name candidates.
- MED M8 aliases (1466/1469 vs 1015-1040/1912-1936): fold hy2/wg/hysteria2+realm/naive in JSON/Clash too.
- MED M9/M10 link-batch dedupe_names + suffix collision (1506-1521, 1148-1156).
- MED M11 case: document case-sensitivity (spec #716) — at minimum make missing-PBK-style misses suggestive, keep behavior.
- MED M12 empty sni falls back to host (524-527/559-560, 768-774).
- MED M13 unknown network preserved, not silently tcp (479-495).
- MED M14 ss edges (694-732): inner-slash trim parity, decode parity, trim passwords.
- MED M15 socks empty user/pass rejected (903-911).
- MED M16 vmess tls-bool + host whitespace (645-648, 605-609).
- MED M17 single-blob JSON without :// (1880-1882, 1834-1858): try JSON decode.
- MED M18 sniff order unify (1490-1492 vs 1813-1823).
- MED M19 BOM loop/strip both paths (282-284, 1486, 1508-1509, 1419).
- LOWs: length caps on b64/query/name/error strings; empty-key pollution (415); port +/leading-zero strictness; .conf trailing-space name (1530); has_clash_proxies strictness (1902); data: case (1864); peerless-conf taxonomy (1300-1306); scheme message casing; identity normalization (trim ports, brackets, trailing dot); document flow/security exclusion from identity. SEC L1 bare-secret labels; SEC L2 static "invalid JSON" instead of serde display.

- [ ] Scenario: non-ASCII addsub payload → Action: parse → Expected: typed error, no panic. Verification: automated test. Priority: required.
- [ ] Scenario: `trojan://real@h:443?password=fake` → Action: parse → Expected: real password wins or typed rejection, never silent fake. Verification: automated test. Priority: required.
- [ ] Scenario: multi-@ userinfo failure → Action: fail → Expected: label fully redacted. Verification: automated test. Priority: required.
- [ ] Prohibited side effect: full existing corpus stays green; no verdict flips except the listed ones (each with a test).
