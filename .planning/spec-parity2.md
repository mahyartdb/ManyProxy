# Spec: finish-the-product parity batch (tickets 35–38)

## Kernel

### Why
Four reviewer-flagged gaps keep ManyProxy below Throne/v2rayNG production grade: routing presets are import-dead, traffic counters measure probes instead of usage, secrets sit plaintext in SQLite, and closing the window kills every proxy with no tray or autostart.

### Capabilities
- CAP-1 — Routing presets: import, manage, and apply per-row routing rule presets.
  - US-1: As a migrant Throne user, I want to paste a `throne://route` link or preset URL, so that the preset lands in my library. Given a route link, when imported, then a named preset with its rules is stored and listed.
  - US-2: As a user, I want to attach a preset to a config, so that its sidecar routes accordingly. Given an attached preset, when the endpoint starts, then `sing-box check` passes and the rules are in the served config.
  - US-3: As a user, I want to add/remove rules and create presets by hand, so that I am not limited to imports. Given the preset manager, when I add a rule (matcher + action), then it validates and persists.
- CAP-2 — Data-plane traffic: per-config usage counters measuring real proxied bytes, surviving restart.
  - US-4: As a user, I want per-row ↑/↓ byte counters, so that I know real usage. Given a running endpoint passing traffic, when I look at the row, then counters grow with data-plane bytes only (probes excluded from the count source).
  - US-5: As a user, I want counters to survive restart, so that usage history is not wiped. Given persisted totals, when the app restarts, then counters resume from stored values.
- CAP-3 — Secrets at rest: stored credentials encrypted with Windows DPAPI (user scope).
  - US-6: As a user, I want my proxy credentials encrypted on disk, so that a stolen DB file is useless. Given a library with credentials, when I inspect `library.db` bytes, then no private key/password/token appears in plaintext.
  - US-7: As an existing user, I want my library migrated silently, so that nothing breaks. Given a v2 plaintext DB, when the app opens it, then it migrates to encrypted storage with zero data loss.
- CAP-4 — Tray + autostart: background presence and opt-in login behavior.
  - US-8: As a user, I want closing the window to hide to tray instead of killing proxies, so that my connectivity survives. Given running endpoints, when I close the window, then endpoints keep serving and a tray icon shows counts.
  - US-9: As a user, I want opt-in autostart with session restore, so that my proxies return after login. Given opt-in enabled, when Windows starts, then the app launches and restores the previously running set.

### Constraints
- No silent network writes: preset fetch only on explicit user action; no auto-update without consent.
- No new silent persistence: autostart/registry writes only behind explicit opt-in checkboxes, default off.
- DPAPI failures fail loudly; never silently fall back to plaintext.
- Live user DB (`%APPDATA%\ManyProxy\library.db`) never touched by tests (isolated stores only).
- Full suite green + fmt + clippy before every commit.

### Non-goals
- TUN/VPN mode, per-process connection trees, QR sharing, auto-selector/failover, i18n/RTL.
- Remote preset auto-update subscriptions (explicit refresh only).

### Success signal
- A Throne `route` preset imports, attaches to a row, and the row starts with the rules live (`sing-box check` EXIT 0).
- Row counters increase only when data-plane bytes flow and persist across restart.
- `strings library.db` shows no credential material.
- Window close with running endpoints hides to tray; endpoints keep serving; reboot with opt-in restores them.

## Seams
- Highest: existing `generate()` output JSON (presets merge into `route.rules`; clash API into `experimental`), `Store` CRUD + new tables, existing dialog/commit/test seams in `main.rs`, existing `TcpStream` HTTP probe machinery for clash polling.
- New: `route_presets` table, `traffic_rx/tx` columns, DPAPI FFI module, tray module.

## Boundaries
- Always: TDD with red-run evidence; synthetic data in tests; secrets never in logs/errors/tests.
- Ask first: none remaining (user authorized full implementation).
- Never: plaintext fallback on DPAPI failure; tray/autostart without consent; test writes to the live DB; placeholder/skeleton code.

## Assumptions
1. One sidecar process per row (established architecture) — per-process clash totals equal per-row usage.
2. sing-box 1.14.2 clash API exposes Clash-compatible `/connections` totals (verified live before building).
3. Throne route payloads are JSON rule arrays compatible with sing-box route-rule objects (validated, never trusted blindly).
4. `tray-icon` 0.25 builds on this toolchain with only the app icon asset (no new art).
5. GPUI can hide/restore the single window from the main thread on a tray event (spiked before committing to the design).

## Verification
- Every scenario has an automated test; live-binary checks for clash shape and generated-config `check`; live computer-use tour for tray/close/autostart UI.
- Two-pass self-check done at review: coherence across spec/plan/tickets, preservation of every source claim.

## Sources
- 10-reviewer consolidated findings (`.scratch/manyproxy/review-10x/consolidated.md`), tickets 10 (parked), R10 parity report.
- sing-box clash-api docs (fetched 2026), tray-icon 0.25 docs (fetched 2026).
- Codebase: `src/engine.rs` generation, `src/store.rs` schema v2, `src/health.rs` probes, `src/main.rs` dialogs/ticks.

## Maintain
- Constitution: spec is truth; smallest scope that solves it; out-of-scope written down.
- Map: this spec + `.planning/plan-parity2.md` own the batch; tickets 35–38 own execution slices.
- Status: open until all four ship green with live proof.
- History: ledger lines 35+ record completion contracts.
