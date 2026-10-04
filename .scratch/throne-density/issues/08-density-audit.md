# 08: Density lock plus keyboard secrets audit

**What to build:** 1000x700 paint lock (>=3 rows plus strip plus inspector plus status, no clipping) plus keyboard and secrets audit: `Ctrl-N/F5/Shift-F5/Ctrl-T`/Esc/arrows intact, clipboard/logs/status carry bind plus port only.

**Blocked by:** 03-command-strip, 04-status-bar, 05-inspector-tabs.

**Owns:** accessible-frontend

**Status:** ready-for-agent

- [ ] Scenario: keyboard only, empty library → Action: Add, Start, Copy URLs → Expected: flow completes pointer-free, clipboard holds both `socks5://` and `http://` forms with bind plus port only. Verification: automated test. Priority: required.
- [ ] Scenario: secrets audit → Action: scan table, inspector, clipboard, logs, export payload → Expected: no keys, tokens, or passwords surfaced; export keeps the verbatim plaintext warning plus confirm. Verification: automated test. Priority: required.
- [ ] Prohibited side effect: focus trap broken across tabs; hover-only or modal-only state/latency/usage.
