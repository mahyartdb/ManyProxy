# Spec: complete-the-product batch (Phase 1: cats 1,2,3,4,6,7,9,10 → Phase 2 security → Phase 3 package/ship)

## Kernel

### Why

ManyProxy works (478 green, tray + autostart + presets + traffic shipped) but 10 audit agents found it incomplete: UI dead-ends and label lies, importable-but-unservable protocols, crash orphans with no reaper, restore-without-backup and whole-list poisoning on one corrupt row, UI-thread blocking ticks, untested DPAPI/lifecycle paths, silent no-selection clicks and missing focus, and roadmap gaps (no multi-select, no history/sparkline, no share/QR, no logs export). The user authorized the full program: Phase 1 ships categories 1,2,3,4,6,7,9,10; Phase 2 is a security pass; Phase 3 packages and ships.

### Capabilities

- CAP-1 — UI completeness (cat 1): every list/dialog/flow is honest and closable.
  - US-1: As a user, I want deleting a running row to stop its endpoint first, so no orphan keeps serving. Given a running row, when I delete it, then its endpoint stops and the row disappears.
  - US-2: As a keyboard user, I want F5/Shift-F5/Ctrl-T labels to match behavior, so I trust shortcuts. Given the toolbar, when I read Start-all/Stop-all/Test labels, then the bound keys do exactly what the labels say.
  - US-3: As a user, I want Edit to offer re-auto port, so I can return to auto-assign. Given an edited row with a concrete port, when I choose Auto, then the port clears to auto and the next start picks a free port.
  - US-4: As a new user, I want empty-state Add/Import buttons, so I never dead-end. Given an empty library, when I open the app, then clickable Add and Import buttons are visible.
  - US-5: As a user, I want silent no-selection actions to toast, so I know what happened. Given nothing selected, when I invoke Stop/Copy/Toggle/Test/Ping, then a "Select a configuration first" notice appears.
  - US-6: As a user, I want edit-while-running to prompt restart-to-apply, so I never think stale is live. Given a running row I just edited, when the edit commits, then a restart-to-apply prompt or toast names the row.
  - US-7: As a user, I want export-overwrite to need an explicit second gate plus a file path I chose, so I never clobber by accident. Given an existing export file, when I export, then a confirm names the path and I can pick a different path.
  - US-8: As a user, I want Duplicate/Clone, so adding the next proxy is one click. Given a row, when I duplicate it, then a new disabled row appears with a fresh auto port.
  - US-9: As a user, I want Import Browse + inline per-line failure reasons pre-commit, so bad lines are fixable. Given an import dialog, when lines are invalid, then the first-failure reason shows before commit and a Browse button picks files.
  - US-10: As a user, I want multiline params with per-protocol template picker on edit, so JSON is editable. Given any row, when I open params, then a multiline editor with format/validate and template choice appears.
  - US-11: As a user, I want a configurable test URL (global + per-row override), so health matches my egress. Given Settings, when I set a test URL, then probes use it and the default remains documented.
  - US-12: As a user, I want selected Test/Ping to toast results, so I get feedback. Given a selected row, when Test/Ping finishes, then a toast names latency or cause.
  - US-13: As a user, I want logs with live-tail toggle + Copy + Clear-filter + level filter, so crashes are diagnosable. Given the logs dialog, when new lines arrive, then tail follows unless paused and Copy exports visible lines.
  - US-14: As a user, I want traffic in the list + status totals + per-row reset, so usage is visible. Given running rows, when traffic flows, then the list shows per-row totals and the status bar shows the sum.
  - US-15: As a user, I want stop-all to confirm when N are running, so one click never kills everything silently. Given N running, when I stop-all, then a confirm names the count.
  - US-16: As a user, I want subscription rows to expand members + copy redacted URL + retry, so subs are inspectable. Given a sub row, when I expand it, then member names appear with copy-URL and Refresh.
  - US-17: As a user, I want preset manager edit/duplicate/rule-preview, so presets are maintainable. Given a preset, when I edit it, then rules validate before commit and detail shows more than the name.
  - US-18: As a user, I want search to cover sub label, preset name, enabled/running/health, so filtering finds things. Given a query matching a sub or preset or state word, when I search, then matching rows appear.
  - US-19: As a user, I want Settings to hold test URL, tick intervals, export path, data-folder reveal, clear health/traffic, reset snapshot, so tuning needs no hand-edits. Given Settings, when I open it, then those controls exist.
- CAP-2 — Protocol serve-everything (cat 2): every importable type serves; nothing is a dead row.
  - US-20: As a user, I want socks/http inbound-type links to serve as outbounds, so imports never die at start. Given a socks/http link, when I start it, then sing-box 1.14.2 `socks`/`http` outbound JSON is emitted per research schema.
  - US-21: As a user, I want hysteria2 without up/down to start via BBR, so every hy2 link works. Given a hy2 link with no up/down, when I start it, then `up_mbps`/`down_mbps` are omitted (BBR) per docs.
  - US-22: As a user, I want `hysteria2+realm://` to map or loudly reject at import, so no parse-ok/start-fail. Given a realm variant, when I import it, then it either serves via the realm block or import refuses with guidance.
  - US-23: As a user, I want quic transport + kcp decision + Reality/ECH extras mapped, so transports don't die at generate. Given vmess/vless/trojan with quic or ech options, when I start, then 1.14.2 transport/tls JSON is emitted or import rejects with a named bad-option.
  - US-24: As a user, I want mieru/snell/juicity/shadowtls/masque/openvpn/trusttunnel to serve or explicitly reject at import with per-protocol guidance. Given such a link, when I import, then it serves via native 1.14.2 schema where upstream exists (socks/http/shadowtls/snell/hy2) or import fails loudly naming the gap; mieru/juicity (no upstream schema) reject at import, never at start.
  - US-25: As a user, I want WG `.conf` key parity with URI intake (reserved/fm/keepalive/dns/mtu), so conf and URI behave identically. Given a `.conf`, when I import it, then reserved bytes and all mapped keys survive.
  - US-26: As a user, I want trojan `allowInsecure` mapped and HY2/TUIC extras validated against 1.14.2, so wild params never die in `check`. Given such params, when I start, then tls.insecure mapping or typed bad-option appears.
  - US-27: As a user, I want `v2rayn://` lines, bare SIP008 server lists, Clash flow-mappings/inline-JSON/Meta types handled, so subs import. Given such a sub payload, when I refresh, then members land or per-item skips name the reason.
  - US-28: As a Throne migrant, I want `throne://route` urlsafe/raw + `kind` envelope accepted and `remoteroute` refresh wired or claim removed, so presets interoperate. Given a route link in any of the three encodings, when I import, then the preset stores; given a remoteroute, explicit refresh works.
  - US-29: As a user, I want `proxy` alias for `route` plus explicit `warp` guidance error, so Throne profiles don't die with BadRule. Given a preset using `proxy`, when attached, then it serves as `route`; given `warp`, a guidance error names the missing endpoint design.
- CAP-3 — Engine reliability (cat 3): crashes heal, ports are honest, validation is early.
  - US-30: As a user, I want a startup reaper that kills only our stale sidecars and sweeps stale secret temp files, so a hard kill never wedges ports. Given a previous hard crash, when the app launches, then stale `manyproxy-*.json` files are swept and only matching sidecar binaries holding our ports are reaped.
  - US-31: As a user, I want crash recovery (manual Restart + opt-in auto-restart with backoff/max-retries), so one dead core doesn't sit Crashed forever. Given a crashed shard, when I invoke Restart, then it restarts; auto-restart only with opt-in.
  - US-32: As a user, I want crashed shards dropped from `served[]`/clash targets and skipped by traffic/reprobe ticks, so dead sockets aren't trusted. Given a crash, when ticks run, then dead cores are skipped and totals marked stale.
  - US-33: As a user, I want clash-port reservation without TOCTOU plus retry, and user rows rejected from `18501-18999`/`11801-11999` at validation, so collisions fail early with a named range. Given a row in a reserved range, when I save, then a typed error names the range.
  - US-34: As a user, I want `experimental` merged not clobbered and 128-bit crypto clash secrets, so future telemetry survives and secrets are strong. Given a custom config with `experimental`, when clash injects, then existing keys survive.
  - US-35: As a user, I want start/stop serialized (one engine queue or generation counter) and stop failures surfaced, so interleaves never lie. Given a stop during an in-flight start, when it lands, then ordering is safe and failure toasts instead of false success.
  - US-36: As a user, I want start-all to compute the maximal healthy subset and apply once, so N rows don't flap O(n²). Given N rows with one bad, when I start-all, then healthy rows serve once and the bad row is named.
  - US-37: As a user, I want egress `server` resolved at validation with a DNS-vs-egress cause classifier, so typos fail fast. Given a bad hostname, when I validate, then a typed error names resolution failure; health distinguishes DNS from egress failure.
  - US-38: As a user, I want probe attribution to normalize (lowercase, strip port/trailing dot) and match resolved IPs, so probe bytes never leak into usage. Given probe flows, when traffic is counted, then they are excluded even when dest is a resolved IP.
  - US-39: As a user, I want `bind_ip` parsed and `port != 0` + privileged-port guidance at validation, so bad binds fail before sidecars spawn. Given garbage bind or port 0, when I start, then a typed error appears before any spawn.
  - US-40: As a user, I want custom full-config/extra-core listens parsed into conflict+probe sets (or allowlisted), so two replacements can't silently share a port. Given two replacements binding the same port, when I start both, then a conflict error names them.
  - US-41: As a user, I want xray bridge ports recognized as own in the skip set, so re-apply never false-positives PortUnavailable on its own bridge. Given active xray bridges, when I re-apply, then own bridge ports are skipped.
  - US-42: As a user, I want `translate_outbound` misspellings to warn with hints instead of silently dropping, so typos surface. Given `securty` instead of `security`, when I start, then a warning names the likely key.
- CAP-4 — Store v5 + integrity (cat 4): backups first, tolerant reads, strict writes.
  - US-43: As a user, I want restore to take a timestamped backup and rebuild in one transaction (temp DB + validate + quick_check + rename), so a crash mid-restore never halves my library. Given a restore payload, when I confirm, then a backup file exists before any wipe and a mid-process kill leaves the old library intact.
  - US-44: As a user, I want one corrupt row to quarantine instead of poisoning list/export/startup, so I can still recover. Given one bad params blob, when I open/export/list, then good rows load with a banner naming the quarantined ids and export offers skip-or-flag.
  - US-45: As a user, I want `user_version` bumped to 5 only after the seal/data migration commits, so transient DPAPI failure never strands plaintext as migrated. Given a DPAPI-down migration, when it fails, then version stays and next open retries.
  - US-46: As a user, I want a corrupt/foreign DB quarantined to `library.db.corrupt-<ts>` with integrity_check and a restore-from-export offer, never silent in-memory loss. Given a corrupt file, when the app opens, then the bad file is preserved aside and recovery is offered.
  - US-47: As a user, I want export to be full-fidelity (route_presets + per-row preset linkage by name + traffic + meta) or dialog copy that names what is excluded. Given an export, when I restore it elsewhere, then routing policy and counters survive or the dialog said otherwise.
  - US-48: As a user, I want traffic ticks in one transaction with saturating single-statement updates and surfaced errors, so concurrent ticks never lose deltas. Given traffic flowing, when ticks persist, then one tx per tick commits and errors are logged not swallowed.
  - US-49: As a user, I want tombstones keyed per subscription-URL with rotation-core matching and GC, so re-subscribe doesn't resurrect deletes. Given a deleted member, when I re-add the same sub URL, then the delete stays honored.
  - US-50: As a user, I want preset attach re-validated with check+write in a tx plus an orphan-sweep banner, so dangling ids fail at attach not at serve. Given a deleted preset id, when I attach it, then attach refuses loudly.
  - US-51: As a user, I want staged v4→v5 migration (interval_secs default 86400, health_history table, preset extension columns) with drift-check extended to new columns and v0-v3 fixture tests. Given a v4 DB, when the app opens, then it migrates idempotently and proves shape.
  - US-52: As a user, I want running-snapshot saves to filter to known ids and surface write errors, so read-only disks never silently lose restore sets. Given an unwritable disk, when snapshot saves, then an error is shown.
- CAP-5 — Performance off-UI (cat 6): ticks never block the window.
  - US-53: As a user, I want traffic/engine/log ticks off the UI thread with concurrent clash polls and total deadlines, so 2+ cores never freeze the window. Given slow cores, when ticks run, then the UI stays responsive and a prior in-flight tick is skipped.
  - US-54: As a user, I want one tray timer at 500-1000ms and one timer per interval (no 1s slicing), so idle wakes are minimal. Given idle, when I observe timers, then wakeups match the longest interval.
  - US-55: As a user, I want health persistence batched (one tx per probe round) and log tagging without a second full list(), so writes don't amplify. Given a probe round, when results persist, then one tx commits.
  - US-56: As a user, I want bounded traffic maps (cap + TTL eviction), VecDeque logs/samples, indexed latest-health, and precomputed tag maps, so churn never grows memory or O(R·S) scans. Given many short connections, when ticks run, then memory stays bounded.
  - US-57: As a user, I want cached upstream strings + search blobs + stripped log lines at ingest, so per-frame JSON parses and per-keystroke parses disappear. Given 50 rows, when I type or scroll, then no per-row serde_json runs.
  - US-58: As a user, I want concurrent bounded probes (8-way) with per-kind in-flight flags, so test-all doesn't serialize 5s per row. Given test-all on N rows, when it runs, then probes overlap and manual test isn't blocked by the 60s tick.
  - US-59: As a user, I want startup to lazy-load traffic totals and defer sub refresh until after first paint, plus cached sidecar presence and hardened-file flags, so launch paints fast. Given launch, when the window appears, then first paint precedes refresh work.
- CAP-6 — Testing gaps (cat 7): load-bearing paths proven.
  - US-60: As a maintainer, I want DPAPI roundtrip + rejection-table tests plus store DPAPI-failure injection (write abort, foreign blob, half-migrated), so sealing is proven. Given DPAPI down, when writes run, then they abort loudly and reads quarantine.
  - US-61: As a maintainer, I want stub-executable sidecar lifecycle tests (stop_child, poll_exit, drain) plus an opt-in CI live job with pinned sing-box, so lifecycle isn't Fake-only. Given CI with the binary, when live tests run, then check/run/clash-auth/totals-move are exercised.
  - US-62: As a maintainer, I want tray action dispatch headless tests (every menu id into start/stop handlers) while windowing stays manual-tour, so logic is covered without windows. Given a menu id, when dispatched headless, then the right handler runs.
  - US-63: As a maintainer, I want port-bind race tests (exhaustion None arm, wait_child give-up arm) with reservation semantics, so ticket-39 class flakes stay dead. Given exhausted ports, when allocation runs, then None is handled.
  - US-64: As a maintainer, I want clash error-arm table tests (oversize, 401, non-200, malformed) on the existing TcpListener harness plus is_probe_destination edge table. Given each error shape, when polled, then the right typed outcome appears.
  - US-65: As a maintainer, I want preset attach→generate→detach-with-deleted-id integration plus sealed-row refresh conflict tests plus tick wiring (stub clash → traffic columns move). Given each flow, when run, then assertions hold.
  - US-66: As a maintainer, I want preset fetch-time SSRF re-check + BOM tests and non-Windows DPAPI loud-stub or documented Windows-only. Given a BOM-prefixed preset, when parsed, then it succeeds.
- CAP-7 — A11y + error-UX (cat 9): keyboard-first, never silent, never ambiguous.
  - US-67: As a keyboard user, I want initial focus on every dialog open and labels/errors programmatically linked, so screen readers work. Given any dialog, when it opens, then focus lands in the first field and errors are announced.
  - US-68: As a user, I want crash notices actionable (open Logs button), remove-while-running guarded, settings immediacy labeled, warnings styled with prefix/icon, so risks read as risks. Given each risk path, when shown, then copy and affordance match severity.
  - US-69: As a user, I want empty-import to warn (not success), export-on-empty guarded, no-hits hint unified with the real button, first hide-to-tray toasted once, busy Start disabled with aria-busy, so states never confuse. Given each empty/busy state, when shown, then the right notice appears.
  - US-70: As a user, I want preset/sub failures inline (not toast-only) and restore per-row causes named redacted, so dialog errors behave like Add/Edit. Given a preset attach failure, when it fails, then the dialog shows inline error with focus.
  - US-71: As a user, I want About sectioned with copy-paths affordance, so paths/hashes are usable. Given About, when opened, then version/sidecars/paths/shortcuts are sectioned and copyable.
- CAP-8 — Roadmap completeness (cat 10): subs, history, groups, facets, logs, share.
  - US-72: As a user, I want per-sub interval picker + visible last_refresh_at + global kill-switch + Refresh-all-due, so auto-update is controllable. Given a sub, when I set its interval, then due_for_refresh honors it and the row shows last refresh time.
  - US-73: As a user, I want latency history table + sparkline in detail (bounded retention 30-90 days), so trends are visible. Given a row with history, when I open detail, then a mini time-series renders via gpui-kit native chart/canvas (plotters only for PNG export).
  - US-74: As a user, I want usage history buckets + reset counters + status-bar totals, so usage is manageable. Given accumulated totals, when I reset a row, then counters zero with confirm.
  - US-75: As a user, I want multi-select (id-keyed HashSet per research) + bulk bar (Start/Stop/Test/Delete/Clear) with partial-failure copy, so subsets are operable. Given checked rows, when I bulk-start, then healthy rows serve and row-local failures are named per row.
  - US-76: As a user, I want group facet + status facet + protocol facet ANDed with search plus group management (rename/delete/move/counts), so 50+ rows are navigable. Given facets set, when I filter, then visible rows satisfy all predicates.
  - US-77: As a user, I want logs Save-visible + Copy-all + level filter + tail toggle, so evidence leaves the app. Given visible logs, when I save, then a file is written via the export-path pattern.
  - US-78: As a user, I want config→share-URI serializer + Copy-share-link (mirror of import parsers) with secret-warning copy, so sharing is safe. Given a row, when I copy its share link, then the URI round-trips through import and the warning matches export_warning_text.
  - US-79: As a user, I want QR render for the share URI (qrcode 0.14.1 primary per research) as incremental UI on the serializer, so phones can scan. Given a share URI, when I open QR, then a scannable code renders (PNG via image or SVG string into GPUI image element).
  - US-80: As a user, I want theme capability spike first (does gpui-kit 0.6 expose dark/light tokens?) then a settings toggle with contrast audit of badges/health/logs, so appearance is controllable. Given the spike result, when supported, then a toggle ships; when unsupported, docs say so.
- CAP-9 — Security pass (Phase 2, after Phase 1 green): harden what Phase 1 touched.
  - US-81: As a user, I want LAN-gate coverage for custom/extra-core parsed listens, O_EXCL temp files (tempfile crate), sealed sub/preset URLs (schema v5 follow-on) or token-stripping, so the P0 holes close. Given a custom config binding 0.0.0.0 with loopback row bind, when I start, then LAN confirm gates it.
  - US-82: As a user, I want fail-closed sealed rows (no permanent plaintext fallback), hardened ACLs without env-USERNAME trust, redirect-hop SSRF re-checks + https-strict toggle, redactor key-table extension, so P1 holes close. Given a plaintext-injected sealed row, when opened, then it fails loudly.
  - US-83: As a user, I want 128-bit getrandom clash secrets wrapped in Zeroizing with cleared stop, MANYPROXY_DB canonicalization, sidecar hash-pinning, push-time redaction, so P2 holes close. Given a clash secret, when inspected, then it is 128-bit random and zeroed on stop.
- CAP-10 — Packaging + ship (Phase 3, after security green): install, verify, release.
  - US-84: As a user, I want a portable-zip script + optional per-user installer that guarantees sidecar adjacency and cleans Run value + data on uninstall, plus stale-autostart repair (stored path vs current exe), so install/uninstall is clean. Given a moved exe with stale Run value, when the app launches, then a fix is offered.
  - US-85: As a user, I want stock/xray min-version preflight with versions in About, VERSIONINFO in the exe, first-run missing-sidecar banner with Copy-path/Recheck, so skew is diagnosable. Given old binaries, when I start, then a typed need-≥X error appears.
  - US-86: As a user, I want docs reconciled (README/PACKAGING/PACKAGING-NOTES/QUICKSTART vs current tray/settings/shortcuts) plus a manual-update pointer with no auto-update, so packagers follow truth. Given the docs, when read, then install/tray/version/update statements match the app.
  - US-87: As a releaser, I want hash/sign commands + pinned sidecar table + release build record, so provenance is verifiable. Given a release, when built, then SHA-256 hashes and source URLs are recorded.

### Constraints

- v5 migration allowed; preserve user data; bump `user_version` last; idempotent re-runnable migration; reject newer-than-supported loudly.
- New deps allowed without per-ticket approval BUT research-first (this spec's research briefs are the approval); prefer: `qrcode 0.14.1`, `tempfile 3.27`, `aho-corasick 1.1.5`, `getrandom 0.4` + `zeroize 1.9` (+`secrecy`), `plotters` export-only, gpui-kit native charts first, `windows` crate for ACLs; avoid stale `windows-acl`/`windows-permissions`, dead `sparkline`, fork-confused `qrcode-svg 2.x`.
- Serve-everything: implement native 1.14.2 emission where upstream schema exists; where no schema exists (mieru/juicity) reject at import loudly — never parse-ok/start-fail. hysteria2 omits up/down for BBR; never emit 1.15.0-only fields.
- Multi+facets ship as one slice per user decision; ListState stays single-select, multi is app-modeled id-keyed HashSet; Enter stays List-owned (consume Confirm).
- Live user DB never touched by tests; isolated `MANYPROXY_DB` only; tour uses backup + hash-verified restore.
- TDD with red-run evidence; full suite + fmt + clippy green per commit; no placeholder code; `[PAUSED — X of Y]` on truncation.
- DPAPI failures loud; autostart/registry only behind opt-in default-off; no silent network writes; preset fetch explicit only.

### Non-goals

- TUN/VPN mode, per-process connection trees, auto-selector/failover, remote preset auto-update subscriptions, i18n/RTL.
- masque as 1.14.2 outbound (1.15.0+ only); openvpn/masque as outbounds (they are endpoints); inline ACME (deprecated 1.14.0).
- Auto-update of the app itself; silent binary replacement; PATH-based sidecar resolution; silent downloads.

### Success signal

- Zero dead rows: every importable link either starts or was loudly rejected at import with guidance.
- Crash-kill + relaunch reaps only ours and serves again without manual port cleanup.
- Restore mid-kill leaves the old library intact; one corrupt row quarantines with a banner while export still works.
- 2+ slow cores never freeze the window; idle timers minimal; 50 rows filter/scroll without per-row JSON.
- New tests cover DPAPI, lifecycle, tray dispatch, clash arms, preset integration, tick wiring; suite green + fmt + clippy per commit.
- Keyboard-only add→start→copy works; every dialog opens focused; no silent dead clicks.
- Multi-select bulk bar + three facets work together; per-sub intervals + last-refresh visible; history sparkline renders; share-URI round-trips and QR scans; logs save/copy works.
- Security pass closes P0/P1/P2 holes; packaging ships portable zip + installer with clean uninstall and versioned exe.

## Seams

- Highest existing: `generate()` JSON output (protocol emission + preset merge + clash inject), `Store` CRUD + `user_version` migration + tx batch paths, dialog/commit/test seams in `main.rs`, `TcpStream` clash poll + `spawn_probe` background pattern, `ListState` delegate filter pipeline (`set_rows`/`apply_filter`/`visible`).
- New per research: `subscriptions.interval_secs`, `health_history(config_id,at)` + index, preset extension columns (additive, no FK), temp-DB + rename restore path, id-keyed `checked: HashSet<String>` + facet predicate fields in delegate, GPUI canvas sparkline, share-URI serializer mirror of import parsers, QR PNG/SVG bridge into GPUI image element.
- Dep seams: `tempfile::NamedTempFile` for sidecar configs, `aho-corasick`/`memchr` for log tagging + search, `getrandom`/`zeroize`/`secrecy` for clash secrets, `qrcode` for QR, `windows` crate for ACLs.

## Boundaries

- Always: smallest scope per ticket; secrets never in logs/errors/tests; redacted copy reuse; loud failures over silent fallbacks; preserve existing opt-in/default-off posture.
- Ask first: none remaining (user pre-authorized full program; deps research-first per this spec).
- Never: TUN/VPN; test writes to live DB; plaintext DPAPI fallback; tray/autostart without consent; silent preset fetch or auto-update; 1.15.0-only fields in 1.14.2 output; placeholder code.

## Assumptions

1. sing-box 1.14.2 schemas in the research brief are accurate (docs-fetched 2026-09-28) over training memory.
2. gpui-kit 0.6.6 chart/canvas primitives suffice for sparklines (README-level evidence; spike confirms before committing to plotters-in-app).
3. `qrcode 0.14.1` + `image` PNG bytes bridge into the GPUI image element (exact element name verified at implementation).
4. One sidecar per row remains the topology; per-process clash totals still equal per-row usage.
5. v4 user DBs migrate forward without manual steps; no downgrade support needed.

## Verification

- Every US gets an automated test at the highest seam (store tx tests, engine generate/`check` tests, headless GPUI tests, stub-server clash tests, dispatch tests); live-binary proofs for clash shape, `sing-box check` on generated configs, QR scan round-trip, and computer-use tour for tray/close/focus/facets.
- Regression gates: full `cargo test` + `cargo fmt --check` + `cargo clippy --all-targets` green per commit; parallel-flake watch (ticket-39 rule: no whole-temp-dir scans).
- Security pass verified by negative tests (LAN bypass attempt, symlink temp plant, plaintext injection, redirect-to-loopback, corrupt-row open).
- Packaging verified by clean-VM install/run/uninstall with residue checklist and hash verification.

## Sources

- 10-agent audit outputs (2026-09-28): cats 1,2,3,4,5,6,7,8,9,10 findings with file:line refs.
- Research: `.planning/research/deps-20260928.md`, `singbox-serve-20260928.md`, `sqlite-v5-20260928.md`, `gpui-multiselect-20260928.md` (+ priors `01-05`).
- Codebase: `src/main.rs` dialogs/ticks/delegate, `src/engine.rs` generation/lifecycle, `src/store.rs` schema v4, `src/health.rs` polling, `src/import.rs` grammars, `src/preset.rs` route intake, `src/tray.rs` autostart/restore.
- User answers (2026-09-28): v5 OK; deps research-then-add no approval; serve everything; multi+facets one slice; phased order 1,2,3,4,6,7,9,10 → security → package/ship.

## Maintain

- Constitution: spec is truth; smallest scope that solves it; out-of-scope written down.
- Map: this spec owns the complete-batch; `.planning/plan-complete.md` (next) owns execution order; tickets own slices; prior `spec-parity2.md` stays history for 35-38.
- Status: open until Phase 1 green, then security pass, then packaging; ledger records each ticket close.
- History: parity batch (35-38) + ticket 39 remain the durable lineage; this spec supersedes nothing, it continues the program.
