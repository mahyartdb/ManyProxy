# Changelog

All notable changes to ManyProxy are recorded here. The format follows Keep a Changelog 1.1.0. Ticket boards live by the artifact lifecycle in `AGENTS.md`. A board exists while work is in flight and is deleted when its outcome lands here.

## [0.0.2] - 2026-11-07

### Added

- Rows select like a file list: click, Ctrl+click to toggle, Shift+click or Shift+Up/Down for a range, and Ctrl+A for everything visible. Row menus and Delete act on the whole selection, and the bulk bar shows both counts when they differ.
- Column headers resize by dragging their edges (double-click resets) and reorder by dragging. Widths and order persist, and Reset columns restores both.
- A latency filter in the row bar: Any, at most 100, 300, or 1000 ms, plus an Include unprobed switch. It composes with group, status, and search, and Clear facet filters resets it.
- The row you just started or stopped shows a 150 ms accent on its leading edge, then settles. Nothing moves: the row height, the pitch, and the painted row count are unchanged.
- One-line library rows with a right-hand inspector drawer: one 5-tab switcher (Details, Health, Logs, Links, Traffic), a single-scroll Details tab with merged endpoints and one copy action, and a Start/Stop/Test kebab. The tab bar sizes to its labels at the default window width.
- Sort headers are buttons: click, Enter, or Space sorts by that column and reverses on a second press, and the command palette carries every sort key plus Reverse and Clear.
- A `Text size` slider in Settings → General scales the window's text from 100% to 150% in 5% steps, and the palette's `Toggle large text` flips between 100% and the 125% stop.
- Settings → General has an accent color picker: blue (the default), indigo, violet, magenta, rose, teal, and amber. A pick tints selection, checked controls, the focus ring, and active pills at once; health and error colors never follow it.
- Settings → General has a `Window corners` choice — Auto, Rounded, Small, Square — applied through Windows 11's DWM frame preference. Windows 10 and older ignore it and keep their stock corners.
- Settings → General has a `Show tray icon` switch. Turning it off removes the icon at runtime and makes closing ManyProxy quit, because there is no tray to restore from; turning it back on returns both the icon and the saved close choice.
- Filter row `Search · Group · N of M`. Clicking a row's status lamp filters by that endpoint state. The eight-way status dropdown is gone; Enabled and Disabled live under the Group menu.
- Checked-set bulk bar: `N checked (of M)` with Start, Stop, and `More ▾`. Checked rows carry a 3px accent bar and a filled chip. `Copy proxies` copies the running endpoints as one sheet, and bulk verbs route through the batch engine ops.
- Toasts, bottom-right above the status bar, at most two at a time: batched test sweeps (`Testing N…` then one `Tested N: X live · Y dead`), single-row test results, and delete with Undo. Deletion is immediate. An 8-second stash restores the rows and their group assignments, and expiry is permanent.
- Toasts can carry up to two buttons: Retry on a failed import or test sweep, and Show on a failed subscription refresh. Hovering holds a toast open while the pointer is on it — error toasts still leave after 15 seconds — and batched test sweeps show a progress ring with the remaining count.
- Direct import from clipboard when the content parses cleanly. File picks show one `Files:` row in the dialog, and nothing blocks on a preview unless the source is ambiguous.
- Custom titlebar with hairline min, maximize, and close buttons (red wash on close hover, double-click to maximize) plus placement persistence. The status bar has three segments: `N configs · M enabled`, engine and health, out and in usage.
- A `Ctrl+K` palette hint chip parked at the right end of the status bar.
- A standing not-writable banner when the data folder refuses a write, with re-probes on the tick and after a failed save.
- The taskbar carries a Test sweep's progress, and the window's thumbnail toolbar has Start all, Stop all, and Test while the window is minimized.
- Settings → General can register ManyProxy as the handler for `manyproxy://` links; a link opens the app and runs the same intake as a dropped file.
- While the window is hidden, an engine crash, a failed subscription refresh, a taken port, or a newer release raises a Windows notification. Clicking it restores the window with Logs, Subscriptions, or the update dialog open, and Settings → General can turn the balloons off.
- Settings → Updates has `Check for updates weekly`, on by default: at launch, after seven days, ManyProxy reads the release feed once and puts `Update available - Tools > Check for updates` in the status bar. Clicking it opens the update dialog; nothing downloads, and a background failure is silent.

### Changed

- Engineering: every interface string now resolves through an embedded Fluent catalog (English only, byte-identical output). No behavior, wording, or layout change; translations are not shipped.
- A row's status lamp is a 24px target around the same 12px dot: easier to hit, and it announces itself as a pressed/not-pressed endpoint-state filter to assistive tech.
- Motion is one vocabulary: panels fade in over 150 ms with an ease-out curve instead of appearing flat, and one duration, one easing, and the reduced-motion switch govern all of it.
- Every labeled field carries its visible label as the input's accessible name, so a screen reader hears `Bind address`, not an unlabeled text box; required-field stars stay visual.

- The Add/Edit dialog follows Throne grammar: Common fields, one collapsed Advanced Settings expander, then the protocol-specific section. The raw JSON editor is gone.
- Routing presets have one creation path: three grammar boxes with inline placeholders plus an `Add remote ruleset…` row. The Rules JSON textarea is gone.
- The Tools menu is 11 grouped rows and the row context menu is 7 core actions plus `More ▾`. The unreachable speedtest and resolve cluster is deleted end to end. Settings opens straight from the toolbar.
- The settings modal is one column: a filter field with an `N of M` badge, then six sections in fixed order, each under a caps header with a hairline under it. The filter matches a row's label or id and hides a section whose rows all miss. One Cancel/Save footer, an auto-height body that scrolls past the viewport, 2×2 maintenance buttons, and a click-time confirm for Reset library. The shortcuts reference lists every binding.
- App dialogs no longer ship an invisible close hit-target. Every dialog keeps a named footer and Esc.
- Update checks stay check-only: a newer release opens in your own browser, and nothing downloads or runs on its own.
- An empty list never claims to be empty while rows are on their way: a subscription fetch or a refresh in flight paints skeleton rows at the row pitch and the count line reads `Loading…`, a failed fetch names the reason and keeps a `Retry`, and a search that matches nothing still says so while a background refresh runs.
- Inspector panes build only the rows they show: the log, links, and traffic lists are windowed to the visible range, and coming back to a pane keeps the position it was left at.
- The `main.rs` monolith is split across `app/`, `forms/`, `pickers/`, and `library/` modules. The boot-path scan pins nine functions. The crate is a workspace: `manyproxy-core` holds the logic with no gpui dependency, `manyproxy-ui` holds the UI, and the root binary is a thin shell around `manyproxy_ui::run()`.
- `DESIGN.md` describes the theme that is in effect — every color is a named gpui-kit token, and CI fails on a color literal outside the QR dialog's deliberate black on white.

### Fixed

- The update check reads the published repository; the previous owner URL returned 404, so every check reported no releases.

- High display scaling no longer forces a 620-point window minimum onto a shorter work area: the window floor and a restored rectangle both clamp to the primary display, keeping the status strip reachable.
- Opening Settings costs what it paints: the three core binaries, the two data folders, and the theme are read once when the dialog opens instead of on every frame of the entry animation, and a row's filter check no longer allocates.
- A second launch wakes the running window the moment it forwards its argv. The 200 ms instance poll is gone, so an idle app no longer wakes four times a second.
- Ships as a GUI binary with no console window. The release workflow stamps the version, gates the tag, bundles licenses, and carries the arm64 row.
- Export no longer loses the file when Windows strips ACLs from the temp file before rename.
- Overlays created during boot no longer precede the Root layer, which had panicked at startup.
- Engine batches survive individual row failures: a dead active row, a bad port on one row, or a failed batch no longer stalls the rest or restores a stale Running status.
- Log tones stay legible in the light theme, with contrast asserted in tests.
- Health bands, the serving badge, and error text stay legible in the light theme, and a stopped endpoint now shows a real `Stopped` badge instead of one painted near-white. Muted labels clear AA on row cards and the status band in both themes.
- The inspector tab bar no longer clips Links and Traffic at the default drawer width. A paint test pins this.
- Stop and Test stay live on a stopped endpoint and answer with an honest toast; the operate row no longer fakes unavailability or lights a Stop spinner during a start. Import keeps its `Importing…` label and spinner while it works.
- The Group filter round-trips the status lamp's Running, Dead, and Stopped choices, so its collapsed label and tooltip always name the filter the list is actually showing.
- The row health cell reads `● Running`, `● Running · <probe>`, or `● Failed` without repeating the endpoint, so the latency fits its 110px column. `Copy SOCKS` fits its 96px cell whole.
- Selected rows paint the 2px selection rim; the dirty-dot and collision warnings use the amber warning tone instead of selection blue and danger red. Inline form errors and warnings stay legible in the light theme.
- A failed library read no longer adds a lying `0 of 0` to the status bar. Sort menus mark the current key with a real check state, not a `✓` glued into the label.
- Committing an empty Add rule set, Add subscription, or new preset keeps the dialog open with an explicit prompt instead of closing silently. Subscription, import, group, rule-set, and preset fields drop placeholders that only repeated their label.
- The Group edit footer says `Save` instead of `Close`, and the Edit dialog's commit is primary like every other footer.

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

[0.0.2]: https://github.com/mahyartdb/ManyProxy/releases/tag/v0.0.2
[0.0.1-beta.1]: https://github.com/mahyartdb/ManyProxy/releases/tag/v0.0.1-beta.1
