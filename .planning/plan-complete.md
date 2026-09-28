# Plan: complete-the-product batch (spec-complete.md)

## Technical Context

- **Language/Version**: Rust 2021, edition 2021 (`Cargo.toml:4`).
- **Primary Dependencies**: gpui-kit 0.6, rusqlite 0.40 bundled, serde_json 1, base64 0.22, futures 0.3, tray-icon 0.25, winreg 0.55, raw-window-handle 0.6; build: embed-resource 3, png 0.18. New (research-approved, spec-authorized): `qrcode 0.14.1`, `tempfile 3.27`, `aho-corasick 1.1.5` (+`memchr` complement), `getrandom 0.4` + `zeroize 1.9` (+`secrecy`), `plotters` export-only, `windows` crate for ACLs.
- **Storage**: SQLite (single-connection rusqlite, WAL), schema v4 → v5 staged `user_version` migration; `:memory:` in tests; live DB never touched by tests.
- **Testing**: `cargo test` (478 green at baseline) + `cargo fmt --check` + `cargo clippy --all-targets`; gated live tests (`MANYPROXY_LIVE_TRAFFIC`, `MANYPROXY_LIVE_CHECK`) + computer-use tour for windowing.
- **Target Platform**: Windows desktop (Win32 HWND hide, HKCU Run, crypt32 DPAPI, icacls hardening); cross-compile DPAPI needs loud stub.
- **Project Type**: desktop-app (Rust + gpui-kit, one sidecar process per active row).
- **Performance Goals**: UI never blocks on ticks (traffic/engine/log off-thread, concurrent clash polls, total deadlines); 50 rows filter/scroll with zero per-row serde_json; idle timer wakes minimal (no 1s slicing).
- **Constraints**: DPAPI loud-fail; autostart/registry opt-in default-off; no silent network writes; preset fetch explicit only; sing-box output stays 1.14.2-valid (no 1.15.0-only fields); ListState single-select → multi app-modeled.
- **Scale/Scope**: 50 stored rows, 20 concurrent sidecars, 10k-sample health ring, 5k log ring, 30–90d history retention.

## Constitution Check (pre-design gate)

Default constitution (spec is truth, everything testable, smallest scope, out-of-scope written down):
- Spec `spec-complete.md` is truth; this plan expresses it, adds no capabilities.
- Every US maps to a design element below; all testable at listed seams.
- Out-of-scope (TUN, auto-update, silent downloads, PATH resolution, 1.15-only fields) preserved in design.
- Verdict: PASS, no violations, proceed to Phase 0/1.

## Phase 0 — research consolidation (all NEEDS CLARIFICATION resolved)

No unknowns remained in the spec; the four briefs below are the decision record. Owning skill per plan-skill table noted each line.

1. **Dependency crates (owns: research; table: N/A — new-dep selection).** Decision: `qrcode 0.14.1` primary (+`image` raster bridge), `fast-qr` styled alt, `qrcodegen` zero-dep fallback; sparkline = gpui-kit native chart/canvas first, `plotters` PNG-export only; `tempfile 3.27` canonical; `aho-corasick 1.1.5` + `memchr`; `getrandom 0.4` + `zeroize 1.9` + `secrecy`; perms = std readonly + `windows` crate (`SetNamedSecurityInfoW`); avoid `windows-acl`/`windows-permissions` (stale), dead `sparkline`, fork-confused `qrcode-svg 2.x`. Rationale: primary sources (crates.io/docs.rs/GitHub, 2026-09-28) with max-download + active-maintenance + license evidence. Alternatives considered per area in brief. Source: `.planning/research/deps-20260928.md`.
2. **sing-box 1.14.2 serve schemas (owns: rust-networking + api-and-interface-design).** Decision: emit native `socks`/`http`/`shadowtls`/`snell`/`hysteria2` per docs schemas; hy2 omits up/down → BBR (no numeric default); quic transport = `{"type":"quic"}`; mieru/juicity have no upstream schema → loud import-reject; openvpn/masque are endpoints not outbounds; masque excluded (1.15+); never emit 1.15-only fields (`version`, HTTP2/QUIC fields, `certificate_sha256`). Rationale: docs-fetched schemas with minimal examples + version banners. Source: `.planning/research/singbox-serve-20260928.md`.
3. **SQLite v4→v5 (owns: sqlite + database-migrations).** Decision: keep staged `user_version` pattern, bump to 5 last; `interval_secs INTEGER NOT NULL DEFAULT 86400` online ADD COLUMN; `health_history` append table + `(config_id,at)` index + time-based retention in-tx (no VACUUM); `route_presets` additive-only, keep no-FK; atomic restore = temp sibling DB + validate + `quick_check` + rename (VACUUM INTO is backup-only); traffic = one tx + prepared UPDATE loop; reads tolerant (skip-and-count quarantine), writes strict. Rationale: sqlite.org mechanics + rusqlite 0.40 Transaction mapping. Source: `.planning/research/sqlite-v5-20260928.md`.
4. **GPUI multi-select/facets/background (owns: performance-optimization for threading; list pattern app-side).** Decision: ListState/TableState single-select only → app-modeled `checked: HashSet<String>` keyed by config id; bulk bar = conditional app row reusing commit paths + partial-failure copy; facets = delegate predicate fields ANDed in `apply_filter` (`Select`/`Combobox multiple`/`Checkbox`/`TabBar`); keyboard via `actions!`+`bind_app_keys`, never rebind Enter (consume `ListEvent::Confirm`); search = precomputed lowercase blob in `set_rows`; background = `spawn_probe` pattern (`background_executor().spawn` + `cx.update` + guard + weak + detach) applied to engine apply + clash polling. Open spikes (checkbox bubbling, keystroke strings, combobox trigger, clash parallelism) deferred to tickets. Source: `.planning/research/gpui-multiselect-20260928.md`.
5. **Prior art (owns: N/A — repo history).** Decisions from `plan-parity2.md` stand: clash-per-sidecar polling with hand-rolled TcpStream, preset-prepend merge validated by `sing-box check`, DPAPI `dpapi1:` shape, tray-icon 0.25 + build-time ICO decode, HKCU Run opt-in + `meta` restore flag.

## Phase 1 — design (summaries; detail in sibling artifacts)

- Data model delta: `.planning/data-model-complete.md` (v5: `interval_secs`, `health_history`, preset extension columns, batch/quarantine rules; in-memory `checked` set).
- Contracts: `.planning/contracts-complete/` — `share-uri.md` (serializer mirror + QR bridge + warning copy), `clash-poll.md` (concurrency + staleness + attribution normalization), `bulk-facet.md` (checked-set semantics + facet AND + partial-failure copy), `preset-envelope.md` (urlsafe/raw + kind + proxy-alias/warp-guidance).
- Validation: `.planning/quickstart-complete.md` — runnable scenarios A–J covering dead-row elimination, crash recovery, restore atomicity, quarantine, off-UI ticks, DPAPI/lifecycle tests, keyboard/focus, multi+facets, history/sparkline, share/QR, logs export.

## Post-design Constitution Check

- No new capabilities vs spec; no silent behavior added; every network/registry/file-perm action stays user-initiated; testability preserved at highest seams (`generate()` JSON, Store tx paths, delegate filter, TcpStream poll, background pattern). Verdict: PASS.
