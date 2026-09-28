# Quickstart-complete: validation guide (no code)

Prerequisites: debug build with sidecars (`sing-box.exe 1.14.2` + fork + `xray.exe` beside exe), isolated `MANYPROXY_DB` temp copy for destructive scenarios, `cargo test` baseline green.

## A — Zero dead rows
1. Import one link each: socks, http, hy2 (no up/down), hy2+realm, quic-transport vless, WG conf, trojan allowInsecure, mieru/juicity (expect loud import-reject), shadowtls, snell.
2. Expected: every accepted row starts and serves; every rejected intake names the reason at import, never at start.

## B — Crash recovery + ports
1. Start 2 rows, kill a sidecar externally, observe Crashed + Restart; hard-kill app, relaunch.
2. Expected: reaper swept only ours, no manual port cleanup; crashed shard dropped from served/clash sets; reserved-range rows rejected at save with named range.

## C — Restore atomicity + quarantine
1. Seed a library, corrupt one row's params, open/export/list; run restore with a kill mid-restore (test harness).
2. Expected: good rows load with quarantine banner; export offers skip-or-flag; old library intact after mid-kill; backup file exists.

## D — Off-UI ticks
1. Run 2+ slow clash stubs, type/scroll the 50-row list during ticks.
2. Expected: UI responsive; one tx per traffic tick; search has no per-row JSON; idle wakes match longest interval.

## E — DPAPI + lifecycle tests
1. `cargo test` full suite; DPAPI-down injection tests abort loudly; stub-executable lifecycle tests pass; tray dispatch headless tests pass.
2. Expected: green + fmt + clippy; live jobs (`MANYPROXY_LIVE_TRAFFIC=1`, `MANYPROXY_LIVE_CHECK=1`) green with pinned binary.

## F — Keyboard + focus
1. Keyboard-only: open each dialog, confirm focus lands; invoke actions with nothing selected; trigger crash notice, remove-while-running, settings toggles, empty import, busy Start.
2. Expected: focus + announcements correct; every dead click toasts; risks styled as risks; About sectioned with copy-paths.

## G — Multi + facets
1. Check 3 rows across groups, set group/status/protocol facets + query; bulk Start/Stop/Test/Delete/Clear; Esc with/without dialog.
2. Expected: predicates AND correctly; `N selected (M visible)` honest; partial failures named per row; anchor/detail unaffected.

## H — Subs + history + sparkline
1. Set per-sub interval, observe due tick + last_refresh_at; run probes, open detail sparkline; reset one row's counters; check status-bar totals.
2. Expected: intervals honored; 30–90d retention bounded; reset confirms; totals match.

## I — Share + QR + logs
1. Copy share link per protocol, re-import, confirm equivalence; render QR, scan with phone; Save-visible logs + Copy-all + level filter + tail toggle.
2. Expected: round-trips pass; QR scans; log file matches visible lines.

## J — Security + packaging gates (Phase 2/3 entry)
1. Negative: custom config binding 0.0.0.0 with loopback row (expect LAN gate); symlink temp plant (expect O_EXCL refusal); plaintext-injected sealed row (expect loud fail); redirect-to-loopback sub (expect block).
2. Packaging: clean-VM install/run/uninstall residue checklist; old-binary preflight names need-≥X; About shows versions; VERSIONINFO present; moved-exe stale-Run repair offered; docs match app.
