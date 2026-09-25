# ManyProxy — Spec (source of truth)

## Why

Users juggle many proxy configurations (personal, work, regional, test accounts) but VPN apps force one active tunnel or rewrite system proxy/DNS. As a desktop user, I want to switch on any subset of my configs and get one local proxy address per config, so each app/browser profile can use a different egress without touching system network settings.

## Capabilities

- **CAP-1 — Config library**: system stores an unlimited list of proxy configs with name, protocol parameters, group/tags, enabled flag, bind address/port. Intent: one place for everything. Success: 50 configs stored, listed, searched.
- **CAP-2 — Multi-activation**: user can start/stop each config independently plus start-all/stop-all; each active config gets a unique local bind. Intent: N egresses at once. Success: 20 active at once, each independently stoppable, no port collisions.
- **CAP-3 — Local proxy per config**: each active config exposes one local endpoint usable as SOCKS5 and HTTP proxy on the configured bind (default loopback). Intent: drop-in proxy URL per config. Success: `socks5://bind:port` and `http://bind:port` both route through that config only.
- **CAP-4 — Protocol parity (Throne-like)**: supports Shadowsocks, Trojan, VMess, VLESS, TUIC, Hysteria, Hysteria2, AnyTLS, WireGuard, AmneziaWG variants + Noise obfuscation, SSH, custom outbound JSON, custom full config, chained outbounds, extra-core executable slot, Xray-only VLESS. Intent: no config left behind. Success: each listed type imports and connects.
- **CAP-5 — Import & subscriptions**: paste share links, drag-drop, clipboard import, JSON array import, subscription URL with refresh. Intent: Throne-compatible intake. Success: link, JSON, subscription all land in library with group preserved.
- **CAP-6 — Health & insight**: per-config URL test/latency, TCP ping, traffic counters, process/file logs. Intent: know what works. Success: dead config flagged, live latency shown, logs identify failing config. Note: v1 counters measure probe bytes; data-plane byte counts are deferred.
- **CAP-7 — Desktop management UX**: searchable virtualized list, add/edit dialogs, copy-proxy-URL action, status bar totals, keyboard operable. Intent: fast at 50+ rows. Success: keyboard-only add→start→copy flow; list stays smooth at 50 rows.

### User Story 1 — Many local proxies, no system change (Priority: P1)

As a desktop user, I want to start 3 configs and point 3 apps at 3 local proxies, so that traffic exits differently per app with no system VPN.
**Why this priority**: core promise; without it nothing else matters.
**Independent Test**: import 3 configs, start all, curl via each local endpoint, confirm distinct egress IPs, confirm OS proxy/DNS unchanged.
**Acceptance Scenarios**:

1. **Given** 3 stopped configs, **When** I start all, **Then** each shows its own reachable local endpoint and stays running independently.
2. **Given** 2 running and 1 stopped, **When** I stop one running config, **Then** the other keeps serving and the stopped endpoint refuses connections.
3. **Given** main window open, **When** rendered in headless window test, **Then** list, start/stop, and copy-URL controls are visible and operable.

### User Story 2 — Custom bind + on/off (Priority: P1)

As a power user, I want per-config on/off plus custom bind address/port, so that I control conflicts and LAN sharing explicitly.
**Why this priority**: user explicitly demanded unlimited + custom bind + toggle.
**Independent Test**: set custom port, restart, endpoint moves; toggle off, endpoint closes.
**Acceptance Scenarios**:

1. **Given** a config on auto port, **When** I set bind to loopback + port 10821 and start, **Then** that endpoint serves and the old port is closed.
2. **Given** two configs with the same port, **When** I try to start the second, **Then** I get a port-in-use error Naming the conflicting config and nothing crashes.

### User Story 3 — Throne-compatible import (Priority: P1)

As a Throne user, I want my links and subscriptions to just work, so that migration is paste-and-go.
**Why this priority**: intake is the adoption gate.
**Independent Test**: paste 5 mixed share links + 1 subscription URL; all appear grouped.
**Acceptance Scenarios**:

1. **Given** clipboard holds a proxy share link, **When** I paste-import, **Then** a new disabled config appears with correct host/port/protocol shown.
2. **Given** a subscription URL, **When** I add it and refresh, **Then** group members update without duplicating hand-edited configs.

### User Story 4 — Exotic protocols + custom cores (Priority: P2)

As an operator with AmneziaWG/Noise and Xray-only configs, I want them to run side-by-side with standard configs, so that I keep one manager.
**Why this priority**: differentiator over plain managers; phased after P1.
**Independent Test**: one AmneziaWG-variant, one Xray-only VLESS, one custom-JSON config all active with distinct endpoints.
**Acceptance Scenarios**:

1. **Given** an AmneziaWG-variant link, **When** I start it, **Then** its endpoint routes traffic (not “unsupported type”).
2. **Given** a custom outbound JSON snippet, **When** I save and start, **Then** it runs isolated to its own endpoint; a JSON error blocks start with the offending field named.

### User Story 5 — Health at a glance (Priority: P2)

As a user with 20+ actives, I want latency and traffic per row plus logs, so that I kill the slow/dead one fast.
**Why this priority**: operates the “many” without guessing.
**Independent Test**: run URL test on 5 actives; latencies appear; kill network for one; its failure is flagged.
**Acceptance Scenarios**:

1. **Given** 5 running configs, **When** I run URL test, **Then** each row shows latency or a named failure within 30s.
2. **Given** an active config, **When** I open logs filtered to it, **Then** only its lines show.

## Constraints

1. Desktop app: Rust, single UI dependency (`gpui-kit` layers only), follow Coding Guides + Design Guides (Entity list, virtualized rows, Dialog/Sheet editing, theme tokens, `gpui_kit::test` coverage).
2. Engine: primary proxy engine + compatible fork/sidecar for AmneziaWG/Noise + sidecar process for Xray-only + user extra-core executable slot; one generated routing table mapping each local endpoint to exactly one outbound.
3. Network: default bind loopback only; custom bind allowed with explicit user action + LAN-exposure warning; unique port per endpoint; no system proxy, no virtual interface/TUN, no OS DNS rewrite in v1; no admin required for default use.
4. Platform v1: Windows 10/11 64-bit first; data stored locally (JSON/SQLite) with user-owned export.

## Non-goals

- System proxy mode, TUN/virtual-interface mode, OS DNS rewriting.
- Routing-rule editor / remote routing profiles (v1 uses default direct + block; per-endpoint isolation only).
- Mobile apps, browser extensions, cloud sync, team management.
- OS-keychain/DPAPI credential encryption (backlog; v1 relies on OS file ACLs + permission hardening).
- Automatic protocol detection for malformed links beyond Throne-compatible formats.

## Success signal

- A user imports 20 mixed configs and activates all of them in under 5 minutes, each reachable on its own local endpoint with the correct egress.
- Stopping any one config never disturbs the others; a port conflict names the offender.
- Zero OS network mutations after a session (proxy/DNS/routes byte-identical).

## Seams (highest possible, fewest)

1. Import parsers (links/JSON/subscription → library records) — tested with fixture corpora.
2. Activation engine (library records → generated per-endpoint routing table → engine processes) — tested via loopback curl per endpoint.
3. Library window (list + dialogs + status) — tested headless for render/keys/actions.

## Boundaries

- **Always**: per-config enable/start/stop; unique loopback-default bind; port-conflict error naming both parties; secrets redacted in logs/clipboard; LAN bind behind explicit warning.
- **Ask first**: any OS network mutation; new protocol/first sidecar; changing default port ranges; telemetry of any kind.
- **Never**: require admin for default loopback use; silently fall back to a different egress; export secrets without an explicit verbatim plaintext warning plus confirm; invent tray/backend APIs (capability seam + fallback only).

## Assumptions

ASSUMPTIONS I'M MAKING:

1. One mixed local port per config (serves both SOCKS5 and HTTP) with auto ports from 10801/18081 ranges, not two ports per config.
2. Subscription formats: share links + JSON arrays + v2rayN + limited Clash/SS, same as Throne (not full Clash rule engine).
3. AmneziaWG/Noise runs via compatible fork/sidecar, Xray-only via sidecar — not native schema fields.
4. Tested scale target 50 stored / 20 concurrent; design imposes no hard cap.
5. Extra-core slots are N named slots (not one); auto-port assignment spans an expanded range with spillover and honest exhaustion instead of a 199-port cap.
5. Windows-first; Linux/macOS later.
   → Correct me now or I'll proceed with these.

## Verification

- No implementation names in capabilities/stories; every scenario measurable without code knowledge; zero `[NEEDS CLARIFICATION]` markers.
- Two-pass self-check: coherence (Why→CAPs→stories→boundaries agree: no system VPN anywhere) then preservation (every user demand + research fact landed: unlimited, custom bind, loopback, protocols, custom cores, import, health, UX).
- Grades: deterministic asserts for ports/routing/egress; schema checks for import; manual adjudication for latency UX.
- Evals: core activation pass@3 ≥ 0.90; release-critical no-system-mutation pass^3 = 1.00.

## Sources

- User: fresh manager (not Qt fork); Rust + gpui-kit; pure local SOCKS+HTTP; vless/vmess/wireguard + Noise/Amnezia variants; custom cores + xray/v2ray parity; unlimited + custom IP/port + on/off; loopback yes; Throne parity for scope.
- Research: throneproj/Throne README + docs (protocols, dual+extra cores, inbounds, routing profiles, DNS, subs, hotkeys/tray, per-config controls); sing-box configuration/inbound/outbound/route docs + wireguard-endpoint migration + issue #4045 (no native Amnezia); gpui-kit Coding/Design guides + usage (bootstrap, Entity vs RenderOnce, virtualized List, Dialog/Sheet/AlertDialog, TitleBar/StatusBar-no-Tray, tokens, focus/keys, test layers).
