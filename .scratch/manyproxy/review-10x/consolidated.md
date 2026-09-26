# 10-reviewer sweep — consolidated findings (2026-09-26)

10 parallel reviewers (R1–R10), read-only, file:line evidence. Full reports in session history; this file is the build index.

## Blocker/major clusters (build now)
- **A — list + engine feedback (R1+R5):** no per-row Start/Stop/running badge (main.rs:225-250 vs 3706-3793); Enabled≠Serving invisible (918-935, 3678-3689); duplicate enable controls (3712-3734); no keyboard nav/multi-select/sort/filter-beyond-search; start-all all-or-nothing first-error-only (main 1782-1827, engine 4222-4254/4638-4693); no busy guard — sync blocking apply on UI thread (engine 3175-3187, 3248-3267); asymmetric start/stop copy; toasts transient, crash path silent (tick_engine_poll 3581-3588).
- **B — dialogs/import/subs (R2+R3+R4):** running-edit desync; no per-protocol form (raw JSON params); opaque credential requirements; inline errors + focus missing; no bind/port collision pre-check; import SUB findings detailed in report; subs: no edit sub (store 1610/1687), dup URL allowed, Warn policy invisible (main 991-994), offline undifferentiated, group-follow forced.
- **C — engine/store/health correctness (R6+R8+R9):** route rule singular inbound (engine 2628-2632) vs xray array; custom-config port disagreement (2072-2085 vs 1741-1742); stop_one/stop_all swallow apply errors (4195-4204); AWG numeric coercion incomplete (s1-s4/h1-h4); N=100 serial apply + O(N·M) lookups; export not hardened (store 2323-2354); widened_identity misses AWG keys (store 957-984); proxy_urls breaks IPv6 (430-435); redaction misses bare secrets (health 858-967); status() ping-masks-URL-failure (561-581); logs reload-coupled stale snapshot (main 1395-1397, 2129).
- **D — production parity (R10):** P0 close-kills-proxies no tray (main 3953-3971); P0 no autostart/restore; P1 ticket-10 routing presets parked (throne://route dies generic); P1 probe-bytes labeled traffic (health 75-89); P1 secrets plaintext no DPAPI; P1 docs contradict app-dir-only (README:11, PACKAGING-NOTES:11-15) + stale no-export-UI claims; P2 no version/update, no single-instance, no protocol handler, no share/QR, unmeasured perf.

## Tickets filed from this sweep
- 30-list-engine-feedback.md (cluster A)
- 31-dialogs-import-subs.md (cluster B)
- 32-engine-store-health.md (cluster C)
- 33-parity-polish.md (cluster D)

## Done correctly (keep)
Per-reviewer DONE lists: stable row ids + selection-by-id, secret-free rows/toasts/logs, honest start-all-0 copy, ANSI-clean errors, LAN confirm gates, sub merge/tombstones/adopt, export atomicity, sidecar app-dir-only typed errors, icon embedded.
