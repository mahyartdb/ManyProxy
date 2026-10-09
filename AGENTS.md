# AGENTS.md

Windows 10/11-only Rust GPUI desktop app (single crate, edition 2021). No workspace, no `opencode.json` — `formatter:true` equivalent is `cargo fmt`.

## Commands (pwsh, repo root)

- Run dev: `cargo run` — no admin needed for loopback defaults.
- Verify (same as `ci.yml`): `cargo fmt --check`, then `cargo clippy --all-targets --locked -- -D warnings`, then `cargo test --locked`.
- Fix formatting: `cargo fmt` (OpenCode V2 has no LSP — never rely on `lsp` config, run the cargo commands).
- Single test: `cargo test <substring> --locked` (e.g. `cargo test refresh_with_body --locked`).
- Live proofs are opt-in only: default `cargo test` skips them. Run via `.github/workflows/live.yml` (`workflow_dispatch`, pinned `sing-box 1.14.2`) or locally with `$env:MANYPROXY_SINGBOX_BIN="C:\path\to\sing-box.exe"; $env:MANYPROXY_LIVE_CHECK="1"; $env:MANYPROXY_LIVE_TRAFFIC="1"; cargo test`.

## Architecture

- Entry: `src/main.rs` (~950 KB monolith — all UI/dialogs/ticks/shortcuts). Do not split it speculatively.
- `src/store.rs` — SQLite library, auto ports primary `10801–10999` + spillover `18081–18380`, export/restore, `MANYPROXY_DB` / `MANYPROXY_EXPORT` overrides.
- `src/engine.rs` — sidecar config generate + `check` + spawn/restart; binary lookup is app-dir ONLY, never `PATH`.
- `src/import.rs` — share-link/file/clipboard grammar pack (vless/vmess/trojan/ss/hysteria/tuic/wg/awg/socks/http/anytls/naive/throne/sing-box/Xray/SIP008/Clash YAML/WG `.conf`).
- `src/health.rs` — URL test + TCP ping against `DEFAULT_TEST_URL`, session-only logs with secret redaction; `MANYPROXY_LOGS` overrides visible-logs path.
- `src/dpapi.rs` — Windows DPAPI user-scope sealing (`dpapi1:`); off-Windows builds must fail loudly, never store plaintext.
- `src/preset.rs` + `src/tray.rs` — throne routing presets; tray (Start all/Stop all/Show/Quit, HKCU Run autostart).
- `build.rs` — embeds `manyproxy.rc` + `assets/icon.ico`, generates `tray_icon_rgba.rs` in `OUT_DIR` (missing/undecodable icon panics the build), stamps `MANYPROXY_GIT_HASH` best-effort (`unknown` fallback).

## Constraints (do not break)

- Local-only: no system proxy, TUN, DNS rewrite, routing editor, elevation, services/drivers, telemetry, or silent binary downloads. Covered by `assert_no_system_mutation`-family tests — keep them passing.
- Sidecars (`sing-box`, `sing-box-awg`, `xray`, max one extra-core exe) must sit next to the app exe; never consult `PATH`, never download.
- Secrets: DPAPI at rest, redacted logs/URLs/labels, clipboard carries `bind:port` only. Export is explicit-confirm JSON with verbatim plaintext warning; cancel writes nothing.
- Tests are hermetic: TEMP files only, isolated `MANYPROXY_DB`, no spawning real sidecars unless the live gates above are set.
