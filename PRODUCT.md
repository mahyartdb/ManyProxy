# Product

<!-- impeccable:product-schema 1 -->

## Platform

desktop (Windows 10/11 64-bit GPUI app — outside the web/ios/android/adaptive schema, so no mobile or web native references apply)

## Users

Solo Windows user managing a personal pool of proxy configurations. They add configs manually, import share links/files/subscriptions, organize rows into groups, and serve each config on its own `127.0.0.1:<port>` endpoint (SOCKS5 + HTTP) to point individual apps at. They start, stop, test, and inspect endpoints one row at a time, and value local control with no system-wide side effects.

## Product Purpose

ManyProxy is a local-only proxy library: store configs, import share links, refresh subscriptions, and serve each config on its own loopback endpoint. It exists so a user can run and probe personal proxy endpoints without ever changing system proxy settings, network routing, DNS, or installing services/drivers. Success means the user can point apps at `socks5://127.0.0.1:<port>` / `http://127.0.0.1:<port>` and trust that nothing outside the app folder and its own data file was touched.

## Positioning

Local-only, zero system mutation — a claim a neighboring proxy client could not truthfully copy while offering system proxy switching, TUN interfaces, or silent component downloads. ManyProxy never touches the system proxy, TUN/virtual interfaces, OS DNS, routing rules, elevation paths, telemetry, or silent binary downloads. Each running config serves exactly one loopback endpoint; everything else (import breadth, subscriptions, health evidence) serves that core.

## Operating Context

Windows 10/11 64-bit desktop app (GPUI), run via `cargo run` in dev or the built exe. The app exe and sidecar binaries (`sing-box`, `sing-box-awg`, `xray`, at most one user-supplied extra-core executable) sit in the same folder; resolution is app-dir only, never `PATH`. Data lives in a local user-owned SQLite file; secrets at rest are sealed with Windows DPAPI (user scope). Core workflows: Add dialog (Name, Protocol, Params JSON, Group, Bind, Port with auto-assign), Import dialog (paste box, files, drag-drop, clipboard, live valid/invalid preview), Subscriptions manager (label, URL, group, auto-refresh with per-subscription intervals), per-row Start/Stop/Test/Ping/Copy URLs/Logs, bulk bar for multi-select, JSON export with plaintext-secret warning. Shortcuts ride the buttons: Add `Ctrl-N`, Import `Ctrl-O`, Start selected `F5`, Stop selected `Shift-F5`, Test selected `Ctrl-T`. A tray icon and a close-confirm naming running rows guard shutdown. No admin rights needed for default loopback use.

## Capabilities and Constraints

Confirmed: per-config loopback endpoints (SOCKS5 + HTTP, bind + port only, never credentials over copy); broad import grammar (vless, vmess, trojan, shadowsocks, hysteria v1/v2, tuic, wireguard/awg URIs and `.conf`, socks, http/https, anytls, naive, throne links, sing-box/Xray JSON, SIP008, limited Clash YAML); subscription refresh that preserves renames, avoids duplicates, and keeps last-good members on failure; per-endpoint health (URL test + TCP ping against a visible default URL) and persisted data-plane usage; explicit LAN-bind confirm for any non-loopback bind with post-start warning toast.

Durable prohibitions (must survive all future work): no system proxy switching, no TUN/virtual interface, no OS DNS rewrite, no routing-rule editor, no admin/elevation path, no auto-update replacing binaries, no telemetry beyond user-initiated subscription fetches and probes through the user's own endpoints, no silent binary downloads.

Explicitly undecided: future surfaces beyond the current library/detail/dialog flow; whether the dense-table spike (`table_spike`, default off) ever replaces the card list.

## Brand Commitments

Name and window title: ManyProxy (single branding seam, `APP_TITLE`). Voice: honest, explicit copy — toasts state true outcomes (`Nothing to start — the library is empty`, `"<name>" is already stopped`), errors name rows plus expected files/directories, the export warning names secret classes verbatim, destructive and LAN actions confirm by naming affected rows. No logo, typeface, palette, or mascot commitments established.

## Evidence on Hand

- `README.md`, including the claim → code map table (audit anchor for toolbar, defaults, formats, toasts, subscriptions, endpoints, health, export, missing-binary/LAN/no-mutation behavior).
- `QUICKSTART-RUN.md` scenarios A–D: manual live-egress and snapshot-diff verification procedure.
- `assets/screenshots/library.png`, `add-dialog.png`, `import-dialog.png`, `subscriptions-dialog.png` — live captures, currently stale (they predate the grouped toolbar menus).
- Test suite: 683 unit tests including `assert_no_system_mutation` scans and import-grammar coverage.
- Absences future work must not fabricate: no testimonials, customers, benchmarks, pricing, or licensing claims exist.

## Product Principles

1. Local-only, always — if a feature needs system mutation, elevation, or a driver, it does not ship.
2. Honest state over silent action — confirms name affected rows, failures list per item, toasts never claim clean sweeps.
3. Nothing acquired silently — binaries are placed by the user, never downloaded; updates never replace executables behind the user's back.
4. Secrets stay sealed — DPAPI at rest, redacted URLs and logs, clipboard carries binds and ports, never credentials.
5. Density serves the task — this is an Operate tool; scanability, consistent affordances, and keyboard-visible shortcuts outrank expression.
