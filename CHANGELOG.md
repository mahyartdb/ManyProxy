# Changelog

All notable changes to ManyProxy are recorded here. The format follows Keep a Changelog 1.1.0. Ticket boards live by the artifact lifecycle in `AGENTS.md`. A board exists while work is in flight and is deleted when its outcome lands here.

## [Unreleased]

### Added

- One-line library rows with a right-hand inspector drawer: one 5-tab switcher (Details, Health, Logs, Connections, Traffic), a single-scroll Details tab with merged endpoints and one copy action, and a Start/Stop/Test kebab. The tab bar sizes to its labels at the default window width.
- Filter row `Search · Group · N of M`. Clicking a row's status lamp filters by that endpoint state. The eight-way status dropdown is gone; Enabled and Disabled live under the Group menu.
- Checked-set bulk bar: `N checked (of M)` with Start, Stop, and `More ▾`. Checked rows carry a 3px accent bar and a filled chip. `Copy proxies` copies the running endpoints as one sheet, and bulk verbs route through the batch engine ops.
- Toasts, bottom-right above the status bar, at most two at a time: batched test sweeps (`Testing N…` then one `Tested N: X live · Y dead`), single-row test results, and delete with Undo. Deletion is immediate. An 8-second stash restores the rows and their group assignments, and expiry is permanent.
- Direct import from clipboard when the content parses cleanly. File picks show one `Files:` row in the dialog, and nothing blocks on a preview unless the source is ambiguous.
- Custom titlebar with hairline min, maximize, and close buttons (red wash on close hover, double-click to maximize) plus placement persistence. The status bar has three segments: `N configs · M enabled`, engine and health, out and in usage.
- A `Ctrl+K` palette hint chip parked at the right end of the status bar.
- A standing not-writable banner when the data folder refuses a write, with re-probes on the tick and after a failed save.

### Changed

- The Add/Edit dialog follows Throne grammar: Common fields, one collapsed Advanced Settings expander, then the protocol-specific section. The raw JSON editor is gone.
- Routing presets have one creation path: three grammar boxes with inline placeholders plus an `Add remote ruleset…` row. The Rules JSON textarea is gone.
- The Tools menu is 11 grouped rows and the row context menu is 7 core actions plus `More ▾`. The unreachable speedtest and resolve cluster is deleted end to end. Settings opens straight from the toolbar.
- The settings modal is one column: a filter field with an `N of M` badge, then six sections in fixed order, each under a caps header with a hairline under it. The filter matches a row's label or id and hides a section whose rows all miss. One Cancel/Save footer, an auto-height body that scrolls past the viewport, 2×2 maintenance buttons, and a click-time confirm for Reset library. The shortcuts reference lists every binding.
- App dialogs no longer ship an invisible close hit-target. Every dialog keeps a named footer and Esc.
- Inspector panes build only the rows they show: the log, links, and traffic lists are windowed to the visible range, and coming back to a pane keeps the position it was left at.
- The `main.rs` monolith is split across `app/`, `forms/`, `pickers/`, and `library/` modules. The boot-path scan pins nine functions. The crate is a workspace: `manyproxy-core` holds the logic with no gpui dependency, `manyproxy-ui` holds the UI, and the root binary is a thin shell around `manyproxy_ui::run()`.
- `DESIGN.md` describes the theme that is in effect — every color is a named gpui-kit token, and CI fails on a color literal outside the QR dialog's deliberate black on white.

### Fixed

- Ships as a GUI binary with no console window. The release workflow stamps the version, gates the tag, bundles licenses, and carries the arm64 row.
- Export no longer loses the file when Windows strips ACLs from the temp file before rename.
- Overlays created during boot no longer precede the Root layer, which had panicked at startup.
- Engine batches survive individual row failures: a dead active row, a bad port on one row, or a failed batch no longer stalls the rest or restores a stale Running status.
- Log tones stay legible in the light theme, with contrast asserted in tests.
- The inspector tab bar no longer clips Connections and Traffic at the default drawer width. A paint test pins this.

### Security

- Secrets live in one `Secret` type: the plaintext buffer is wiped when the value is dropped, `Debug` and `Display` print `***`, and storage keeps the same `dpapi1:` DPAPI marker, so an existing database opens unchanged and a copied database file does not. Proxy params, subscription URLs, and the clash controller token all go through it, and the token never reaches a command line. Export is still plaintext, because the user asked for a portable file and the dialog says so first.

## [0.0.1-beta.1] - 2026-10-21

### Added

**Foundation (2026-09).**

- gpui-kit app shell (Root, TitleBar, StatusBar, List) and library CRUD on a local SQLite store.
- Import grammar pack: vless, vmess, trojan, shadowsocks, hysteria v1/v2, tuic, wireguard and awg URIs plus `.conf`, socks, http/https, anytls, naive, throne links, sing-box and Xray JSON, SIP008, and limited Clash YAML. Parsers, batch commit, dialog, clipboard, and drag-drop intake.
- Subscriptions with labeled feeds, refresh intervals, and a store merge seam.
- Standard multi-engine on a combined `sing-box` sidecar with a mock seam. Fork-core exotics: AmneziaWG version matrix, wrong-layer guards, XHTTP path, dual sidecar. An `xray` sidecar bridge with custom rows, an extra-core slot, and chaining.
- Per-config health, logs, and stats probes with persisted traffic and redacted logs.
- Hardening and release proof: LAN-bind confirm gates, OS-mutation seam tests, secrets audit, packaging and run docs.

**Throne clean-room borrow (2026-10, T-01 through T-34).**

- Release pipeline: portable storage beside the exe, the release workflow for a windows-amd64 ZIP, a gated arm64 row, and the doc hard-rule sweep.
- Update-check client, JSON only and never executing, with a `ManyProxy/<version>` UA header.
- Routing and interop: the action enum extension with its live workflow proof family, rule attributes, rule names with the prevent-mods lock and completion, the simple-rules grammar boxes, `.srs` rule-set local staging, chain transition validation, and tray select-exit health ordering.
- Health accuracy: latency bands at 100/300 ms, keep-previous outcomes on aborted probes, the `Ctrl+.` stop-testing shortcut, the mux warm-up double probe, and per-group auto-clear of unavailable rows.
- Subscription and exchange: the named diff report, body-format parity for `json://` fragments and WireGuard INI, and the export envelope with restore-side negotiation.
- UI affordances: port-filter search grammar, click-to-copy cells, connection counters, the window title running marker, header sort pickers, QR encode of listener URLs, traffic dialog top-9 exits, per-shard CPU/RAM mini-charts, log polish with `log_max_lines`, window geometry persistence, and the bind-IP picker.
- Ops marker: `last_clean_exit` with an unclean-exit startup toast.

[0.0.1-beta.1]: https://github.com/mahyartdb/ManyProxy/releases/tag/v0.0.1-beta.1
