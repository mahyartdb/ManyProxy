# Plan: parity batch (spec-parity2.md)

## Technical Context
- Rust 2021, gpui-kit 0.6 UI, rusqlite bundled (SQLite), serde_json, futures, base64. No HTTP client, no windows crate, no tokio.
- Sidecars: stock sing-box 1.14.2 + lx fork + xray, one process per active row, loopback mixed inbounds, per-row secret JSON in temp dir.
- Store: SQLite schema v2 (`SCHEMA_VERSION=2`), WAL, file hardening, `MANYPROXY_DB` override, `:memory:` in tests.

## Constitution Check
- Spec is truth; TDD red-run per ticket; no silent fallbacks; live DB never touched by tests. No violations.

## Phase 0 — research.md (decisions)
1. **Traffic source (owns: rust-networking).** Decision: enable `experimental.clash_api` per sidecar (`external_controller 127.0.0.1:<port>`, random `secret`), poll `GET /connections` with the existing hand-rolled `TcpStream` HTTP client, diff `downloadTotal`/`uploadTotal` per poll, persist deltas. Rationale: zero new deps; one-process-per-row makes process totals exactly per-row usage; `/connections` totals are cumulative counters (Clash API compat). Alternatives: v2ray API (needs gRPC client — rejected, heavy dep); log scraping (no byte data — rejected). MUST verify live against 1.14.2 before building (totals fields present, auth header accepted).
2. **Clash ports (owns: rust-networking).** Decision: allocate per-sidecar controller ports from a dedicated loopback range above the endpoint spillover (e.g. 18401+), tracked in-memory only (never persisted; regenerated per start). Rationale: avoids endpoint port conflicts; secrets already handled via temp secret files.
3. **DPAPI shape (owns: security-and-hardening, sqlite).** Decision: hand-rolled `crypt32.dll` FFI (`CryptProtectData`/`CryptUnprotectData`, user scope, no UI), `params` column stored as `dpapi1:<base64>`; migration v2→v3 encrypts in one transaction; reads decrypt transparently. Rationale: no `windows`-crate weight; user scope binds to the Windows user profile. Alternatives: `windows` crate (rejected: enormous for two calls); plaintext fallback (rejected: violates spec Never). Open question resolved: export decrypts with the existing verbatim plaintext warning (unchanged UX).
4. **Preset application point (owns: api-and-interface-design).** Decision: preset = ordered sing-box route-rule objects; on start, rules are prepended to the row sidecar's `route.rules` (before the per-row isolation rule), validated by `sing-box check`. Rationale: single merge point, engine-verified, no route-final changes. Rule editor: matcher (domain/ipcidr/process_name/port range subset) + action (proxy→row outbound / direct / block) form; stored as JSON, validated field-by-field.
5. **Tray integration (owns: gpui-kit).** Decision: `tray-icon` 0.25 (new dep, authorized), icon from `assets/icon.ico` (decode via bundled means — tray-icon `Icon::from_rgba` needs RGBA bytes; decode ICO without new deps: reuse existing asset pipeline? `embed-resource` only embeds. Options: add `image` dep (dev-only in tray-icon) or ship a tiny RGBA blob generated once at build via a build-script PNG→RGBA step. Decision: build-script converts `assets/icon.ico`→ RGBA byte array include file using a minimal ICO parser in build.rs (no new runtime dep). MUST spike first: tray create + hide window + restore from tray event inside the GPUI main thread. If the spike fails, fallback: close-confirm stays, tray ships menu-less with tooltip only… no — fallback is explicit: tray without hide (window close still confirms) is dishonest; spike gates the whole ticket.
6. **Autostart (owns: workers-best-practices equivalent = OS integration).** Decision: `winreg` micro-dep for `HKCU\...\Run` value + opt-in checkbox in About/Settings surface + `restore_running_on_launch` flag persisted in a new `meta` table; on launch with the flag, start the flagged set (no LAN rows without confirm — confirm gate preserved by reusing `start_all_confirmed`).

## Phase 1 — data-model.md (delta)
- `route_presets(id TEXT PK, name TEXT UNIQUE, source TEXT, source_url TEXT, rules_json TEXT, updated_at INT)`; `proxy_configs.preset_id TEXT NULL REFERENCES route_presets(id)` (validate on attach; null = none).
- `proxy_configs.traffic_rx INT DEFAULT 0, traffic_tx INT DEFAULT 0` (cumulative data-plane bytes; probe bytes never written here).
- `meta(key TEXT PK, value TEXT)` (`dpapi_migrated`, `restore_running`, `autostart` mirror of registry for display).
- Schema v2→v3 migration: add columns/tables + encrypt `params` in-transaction.

## Contracts
- Clash poller: `GET http://127.0.0.1:<ctrl>/connections` + `Authorization: Bearer <secret>` → parse `downloadTotal`/`uploadTotal` (u64) → per-row delta → `store.add_traffic(id, rx, tx)`.
- Preset JSON: `{"name": str, "rules": [sing-box route-rule objects with action in {route→row-outbound, direct, block}]}`; `throne://route` envelope decoded same as addsub (standard-b64, byte-safe scheme split).
- Tray menu: Show / Start all / Stop all / Quit; tooltip `ManyProxy · N running`; left-click toggles window.
- Registry: value `ManyProxy = "<exe path>"` under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.

## quickstart.md (validation guide)
1. Import preset link → preset listed → attach to row → start → `sing-box check` EXIT 0 + rule present in served config.
2. Pass 1 MB through a running endpoint → row counters grow ≈1 MB → restart app → counters persist.
3. `strings library.db` shows no `private_key`/password/token material; app still starts all rows.
4. Start endpoint → close window → tray icon present, endpoint still serves → Quit from tray.
5. Enable autostart + restore → reboot-equivalent (kill + launch with flag) → running set restored.

## Post-design Constitution Check
- No new silent behavior; every network/registry action is user-initiated; DPAPI loud-fail. Pass.
