# ManyProxy — Packaging Notes (ticket 09, release gate)

## Windows 10/11 64-bit (v1 platform)

- **Portable first**: the app is one exe plus sidecar binaries **alongside
  the exe in the app directory** (never in system paths, never downloaded):
  `sing-box` (stock multi-engine), `sing-box-awg` (compatible fork for
  AmneziaWG/Noise/XHTTP rows only), `xray` (Xray-only VLESS bridge only),
  plus at most one user-supplied extra-core executable (single slot, app dir
  only, unprivileged).
- **Resolution order**: app directory first, then `PATH`
  (`resolve_engine_binary*` in `src/engine.rs`). A missing binary is a typed
  error naming the rows + the expected file + every directory searched — the
  app never fetches an executable silently (asserted by
  `missing_binaries_state_never_downloads`).
- **Installer vs portable**: either shape works as long as the sidecars ship
  next to the exe. No registry keys, no services, no drivers, no system
  proxy/DNS/route writes — so there is nothing to roll back on uninstall
  beyond the app folder + the local data file.
- **No admin for default use**: loopback binds need no elevation. LAN binds
  (`0.0.0.0`, `::`, any non-loopback literal) need no elevation either on
  Windows for high ports, but they pass an explicit confirm gate
  (`requires_lan_confirm` + `LAN_CONFIRM_TITLE` dialog) plus a post-start
  warning. Firewall prompts for LAN binds are the OS's own and are expected.
- **Data**: local SQLite (user-owned file). v1 ships **no export UI**; when
  an export path lands it must surface `export_warning_text()` verbatim
  (asserted by `export_warning_names_secret_classes`).

## AV / heuristic expectations

- Unsigned fresh binaries + process-spawning (sidecars) + loopback listeners
  routinely trip heuristics (generic "trojan"/"hacktool" labels,
  SmartScreen reputation warnings). Mitigations: code-sign the exe when a
  cert exists, publish SHA-256 hashes per release, keep sidecar spawning
  visible (named child processes, one per core), never pack/obfuscate.
- A quarantine hit on a sidecar surfaces as a typed missing/crash error
  naming the binary — never a silent fallback to a different egress
  (boundary: **Never** silently fall back).

## Non-goals with rationale (spec)

- **macOS quarantine / setuid**: not a v1 target (Windows-first). If a
  macOS build lands later, expect Gatekeeper quarantine on unsigned sidecars
  (`xattr -d com.apple.quarantine` / notarization story then). No setuid is
  ever needed: loopback + high-port LAN binds require no privilege.
- **Linux SUID-for-TUN**: v1 has no TUN/virtual-interface mode and no OS DNS
  rewrite (spec non-goals), so no privileged helper, no SUID, no capabilities
  (`cap_net_admin`, …) are required. If TUN ever lands, it needs its own
  privilege design + ticket — this release must not grow one by accident
  (asserted by `binary_resolution_never_elevates_or_downloads` and the
  `assert_no_system_mutation` scan over every generated config).
- **Auto-update that replaces binaries**: prohibited. Updates are manual
  installs; the app never replaces its own or sidecar executables
  (boundary: **Ask first** for anything new; **Never** silent replacement).
- **Telemetry**: none. The only network calls are user-initiated
  subscription refreshes (user-supplied URLs, generic errors that never echo
  the URL) and health probes through the user's own running endpoints to the
  user-visible test URL. No analytics, no crash reporting, no version pings.
