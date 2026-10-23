# AGENTS.md

Windows 10/11 GPUI desktop app (single crate, edition 2021). Supported now: windows-amd64 (windows-arm64 lands gated via T-04). Tracked-aspiration (not supported): linux/macOS via the port epic in EPIC-linux-macos-port.md. No workspace, no `opencode.json` — `formatter:true` equivalent is `cargo fmt`.

## Commands (pwsh, repo root)

- Run dev: `cargo run` — no admin needed for loopback defaults.
- Verify (same as `ci.yml`): `cargo fmt --check`, then `cargo clippy --all-targets --locked -- -D warnings`, then `cargo test --locked`.
- Fix formatting: `cargo fmt` (OpenCode V2 has no LSP — never rely on `lsp` config, run the cargo commands).
- Single test: `cargo test <substring> --locked` (e.g. `cargo test refresh_with_body --locked`).
- Live proofs are opt-in only: default `cargo test` skips them. Run via `.github/workflows/live.yml` (`workflow_dispatch`, pinned `sing-box 1.14.2`) or locally with `$env:MANYPROXY_SINGBOX_BIN="C:\path\to\sing-box.exe"; $env:MANYPROXY_LIVE_CHECK="1"; $env:MANYPROXY_LIVE_TRAFFIC="1"; cargo test`.

## Architecture

- Root: `src/main.rs` (543 lines — inner attributes, the `mod` declarations, the crate-root `use …::*;` seams, `actions!`, the `AppView` struct with its field block, `fn main`, and the crash hook; re-measure before citing). **`struct AppView` must stay at the crate root**: field privacy follows the declaring module, which is exactly what lets 20+ `impl AppView` blocks live in `app/*.rs` without widening a field. Moving it forces every field to `pub(crate)` and buys nothing.
- `src/app/` — the UI: `shell.rs` (root layout, key context, action handlers), `toolbar.rs`, `render.rs`, `detail.rs` (inspector panes), `settings.rs` (six-section modal), `bulk.rs` (bulk bar + checked set), `search.rs`, `open.rs` (dialog openers), `commit_{forms,async,engine}.rs` (dialog commits and finish handlers), `engine_ops.rs` (start/stop/test/tick), `io_ops.rs` (clipboard and file io), `state.rs` (selection, filters, subscription wiring), `focus.rs` (focus reclaim + one-layer Esc), `tray.rs`, `bootstrap.rs` + `window.rs` (the boot path), `bits.rs`/`kit_bits.rs`.
- `src/store.rs` — SQLite library, auto ports primary `10801–10999` + spillover `18081–18380`, export/restore, `MANYPROXY_DB` / `MANYPROXY_EXPORT` / `MANYPROXY_RULE_SETS_DIR` overrides; `write_probe` is the only honest writability test (it writes a real temp file).
- `src/engine.rs` — sidecar config generate + `check` + spawn/restart; binary lookup is app-dir ONLY, never `PATH`. `start_many_additive` is the BATCH path (additive union into `active`, ONE `apply`); `start_all_subset` is subtractive and must never be used for a batch.
- `src/import.rs` — share-link/file/clipboard grammar pack (vless/vmess/trojan/ss/hysteria/tuic/wg/awg/socks/http/anytls/naive/throne/sing-box/Xray/SIP008/Clash YAML/WG `.conf`); `src/import_flow/` — resolve + preview.
- `src/update/` — feed fetch, release parsing, semver compare (button-only; release assets are never fetched automatically).
- `src/forms/` — `ConfigForm`, `ImportForm`, `SubForm`, `PresetForm`, rule boxes; `src/dialog/` — the dialog kit + QR.
- `src/library/` — row grid, `metrics.rs` (the single geometry spec), tone, shard strip; `src/models/` — rows, traffic, connections, inspector tab, palette ops, chains, sampler.
- `src/preset/` (mod/groups/rules/sets) + `src/tray.rs` — throne routing presets; tray (Start all/Stop all/Show/Quit, HKCU Run autostart).
- `src/health.rs` — URL test + TCP ping against `DEFAULT_TEST_URL`, session-only logs with secret redaction; `MANYPROXY_LOGS` overrides visible-logs path.
- `src/dpapi.rs` — Windows DPAPI user-scope sealing (`dpapi1:`); off-Windows builds must fail loudly, never store plaintext.
- `src/tests.rs` — one flat file on purpose: it uses `include_str!("main.rs")` and `include_str!("../tests/fixtures/uia-*.json")`, and the boot-path source scan asserts each of its nine markers matches exactly once across `main.rs` + `app/bootstrap.rs`. Do not split it, and never weaken that scan.
- `build.rs` — embeds `manyproxy.rc` + `assets/icon.ico`, generates `tray_icon_rgba.rs` in `OUT_DIR` (missing/undecodable icon panics the build), stamps `MANYPROXY_GIT_HASH` best-effort (`unknown` fallback).

## Constraints (do not break)

- Local-only: no system proxy, TUN, DNS rewrite, routing editor, elevation, services/drivers, telemetry, or silent binary downloads. Covered by `assert_no_system_mutation`-family tests — keep them passing.
- Sidecars (`sing-box`, `sing-box-awg`, `xray`, max one extra-core exe) must sit next to the app exe; never consult `PATH`, never download.
- Secrets: DPAPI at rest, redacted logs/URLs/labels, clipboard carries `bind:port` only. Export is explicit-confirm JSON with verbatim plaintext warning; cancel writes nothing.
- Tests are hermetic: TEMP files only, isolated `MANYPROXY_DB`, no spawning real sidecars unless the live gates above are set.

### Hard rules (settled)

- **Portability (ZIP):** ManyProxy ships and runs as a single self-contained folder (ZIP). No AppData, no Program Files, no installers. Engines and data live beside the exe; the folder can be moved anywhere. Binary lookup is app-dir only; no silent downloads.
- **No-migration:** beta app, no users — no user-data migration path ever. Legacy stores (e.g. `%APPDATA%\ManyProxy\library.db`) are never read or copied. Fresh installs only. `MANYPROXY_DB` / `MANYPROXY_EXPORT` / `MANYPROXY_LOGS` env overrides remain as test/escape hatch.
- **Secrets-across-OS:** plaintext banned on every OS. Windows = DPAPI user-scope `dpapi1:` exactly as today (zero behavior change); future non-Windows backends (Keychain / Secret Service via keyring) behind the same sealed-container semantics; off-Windows builds fail loudly rather than store plaintext.
- **DNS-mapping action (non-concept):** presets containing Throne's DNS-mapping action reach the existing generic unknown-action intake error and are not special-cased. No machinery is built, named, mapped, or documented.
- **Supported matrix:** supported now: windows-amd64 (`windows-arm64` lands gated via T-04). Tracked-aspiration (not supported): linux/macOS via the port epic (`EPIC-linux-macos-port.md`). ManyProxy stays Windows 10/11-only for beta.
