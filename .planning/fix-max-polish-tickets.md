# Max-Polish Tickets — CUA heavy drive + 3 audits (2026-10-06)

Source evidence: live drive pid 20688 today + `CUA-AUDIT-2026-10-06.md` BUG-1..5 + DESIGN audit + robustness audit + UX friction audit.
Constraints: `AGENTS.md` — Windows-only, local-only (no system mutation), sidecars app-dir only, DPAPI, hermetic tests. Verify: `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`.

## Lane A — backend robustness (owns store.rs, engine.rs, health.rs, dpapi.rs, tray.rs, preset.rs, import.rs; keep public APIs compatible)

- [ ] A1 localhost bind: `engine.rs:8120 is_loopback_bind` treat `localhost` as loopback; reconcile `store.rs:755 validated_bind` vs `engine.rs:583 BadBind` message; check `tray.rs:258`. Test: localhost accepted as loopback, no LAN confirm.
- [ ] A2 DPAPI error type: `store.rs:2102,2112` stop mapping dpapi failures to `StoreError::Sql`; add distinct variant, render honest message at `store.rs:369`. Test: dpapi-fail seam aborts write, no partial row.
- [ ] A3 restore strict: `store.rs:4644-4652` reject missing/wrong-typed name/protocol/params/bind_ip via `invalid()` like `4620-4631`; add reserved-range (`engine.rs:87`) + duplicate bind:port check on restore path. Test: corrupt export rejected loudly.
- [ ] A4 ports: document non-unique-at-rest honestly or warn at save; `store.rs:2518,4044,5503` auto-assign consider bind (or document global pool); test exhaustion past both ranges (only `store.rs:7299` covers order today).
- [ ] A5 subscription SSRF: redirects + DNS-rebind coverage for `store.rs:182 policy`; move 8MiB/20s caps from comment into enforced code; keep plaintext-http Warn but ensure no token in logs. Test: redirect to 127.0.0.1 blocked.
- [ ] A6 sidecars: `engine.rs:632 MissingBinary` include expected filename like fork/xray variants; `engine.rs:4712 probe_fork_version` add timeout; pin Windows ACL executable-bit behavior. Test: missing binary names file + dir.
- [ ] A7 redaction unify: `health.rs:1986` vs `main.rs:1966` vs import `redact_link` — single owner or cross-test; pin truncate-vs-redact order; `engine.rs:622 CheckFailed` redact stderr. Test: uuid/password/token forms redacted in all three.
- [ ] A8 export/tests: dedupe `store.rs:4269` vs `2652` pipeline or cross-test new fields; cover empty-library round-trip, `subscriptions.failures` corrupt decode (`store.rs:1402`), clash-poll secret-never-in-error (`health.rs:1184`).

## Lane B — UI/DESIGN/UX (owns src/main.rs UI only; no new colors, no shadows, theme tokens only, keep pinned strings/ids)

- [ ] B1 BUG-1 search: `clear_library_search :11179` + `clear-search-hint :1213` route through `ListState::set_query("")`; test stale-text + selection works.
- [ ] B2 BUG-2 health wording: status `health: never tested` -> `health: no probes this session` or derive from persisted summary (`health.rs`/store helper). Keep detail `59 samples` intact.
- [ ] B3 BUG-3 truncate: `health.rs:1524` alert path full text or word-boundary cut; keep 1-line cap elsewhere. `main.rs:1987` URL scrubber untouched.
- [ ] B4 BUG-4 import empty: empty box -> `0 valid · 0 invalid` neutral, not `0 valid · 1 invalid`.
- [ ] B5 toolbar+bulk wrap: toolbar overflow at 880px (`Help` clips) `main.rs:12293` already wraps — add overflow menu or wrap for menu group; bulk bar `main.rs:11496` add `flex_wrap` like detail-operate `10845`; add `bulk-bar`-within-frame containment test (existing toolbar/endpoints tests at `22876-23047` as template).
- [ ] B6 DESIGN violations: Health/Presets render `render_detail_header` (`10527,10541` vs `10513`); lamp/badge/Badge single evidence rule (`1111,1125,10642`); bulk wording checked-set voice (`11514,11524,11566,11578`); detail Start `f5` Kbd chip (`10871`); Row-menu Delete danger reconcile (`10995` vs DESIGN:157); fix `Starting.` comments (`10830,10592`) to `Starting…`.
- [ ] B7 destructive confirms: Settings clears `6277,6286,6295` confirm like `7544,11882`; Groups Delete confirm naming count, no full `close_all_dialogs` (`12053,12199`); upstream Copy button (`10668`, via `upstream_host_port:2528`, non-secret); truncated endpoints tooltip (keep bind+port only).
- [ ] B8 secrets UX: guided Identity password mask/reveal `1629`; Logs Copy/Save route through `redact_secrets` + note (`6776,6801,6787`); Logs Save picker or overwrite gate + `MANYPROXY_LOGS` disclosure; Sub Copy URL split display vs full-with-warning (`3427`).
- [ ] B9 friction: `Row` -> `Row actions…` or promote Edit/Delete (`10999`); collapse Test/Ping/Logs overflow at 240px (`10840`); Logs single level control (`6936` vs `6997`); Import Browse/drop preserve state (`7831,7698,2087`); Preset Detach inline + rule-count (`9466,9487`); facet-aware empty + combined Clear all (`1200,11586,11263`); shortcuts Delete/Ctrl-F/Ctrl-E/Ctrl-1-3 (`100`), document Esc (`11566`); Add multi-error list + focus-first (`7392,1693`); protocol/group pickers/autocomplete (`1608,1657,1909,3311`); status bar running-ports/filter/tail segments (`11177`).

Verify each lane: `cargo fmt`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`. Keep toasts honest (name rows), no `.disabled()`, no hardcoded colors, no new system-mutation paths.
