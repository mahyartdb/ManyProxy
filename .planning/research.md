# ManyProxy — Research (Phase 0)

All `NEEDS CLARIFICATION` resolved (none carried; assumptions in spec stand unless corrected).

## R1 — One local endpoint per outbound, N at once

- **Decision**: Generate one combined engine config: N local inbounds (one per active config) + N outbounds/endpoints + route table mapping each inbound tag to exactly one outbound tag; unmatched traffic goes to `direct`.
- **Rationale**: Engine docs define `inbounds[]`/`outbounds[]`/`endpoints[]` arrays with required unique tags and a `route.rules[]` action mapping inbound to outbound; no documented count cap; per-inbound listen address/port supported.
- **Alternatives considered**: One engine process per config (rejected: port/file sprawl, harder health/logs); single shared inbound with selector (rejected: breaks per-app distinct-egress requirement).
- **Owning skill**: none (engine docs + prior research brief).

## R2 — Single mixed port per config, loopback default

- **Decision**: Default one mixed port per active config (serves SOCKS5 + HTTP on the same port); bind defaults to loopback; custom IP/port allowed with LAN-exposure warning; auto-assign from fixed ranges; conflicts error naming both parties.
- **Rationale**: Fewest ports to manage for "as many as I want"; matches user custom-bind demand with safe default.
- **Alternatives considered**: Separate SOCKS + HTTP ports per config (deferred as advanced option; doubles port bookkeeping).
- **Owning skill**: none.

## R3 — Exotic protocols need fork/sidecar, not native schema

- **Decision**: Official engine for standard types; AmneziaWG/Noise via compatible fork/sidecar; Xray-only VLESS via Xray sidecar chained to its endpoint; custom outbound JSON validated then passed to the owning engine; user extra-core executable slot runs isolated per config.
- **Rationale**: Official schema has a closed outbound-type enum with no Amnezia/Noise fields and no custom-passthrough; upstream declined Amnezia support; community forks exist. This mirrors Throne's dual-core + extra-core architecture found in research.
- **Alternatives considered**: Drop exotic types v1 (rejected by user: explicitly demanded); embed third-party code in-process (rejected: isolation + licensing risk).
- **Owning skill**: none.

## R4 — Throne-compatible intake, not full rule engines

- **Decision**: Accept share links, JSON arrays of outbounds, v2rayN links, limited Shadowsocks/Clash imports; subscription URLs with manual + auto refresh; hand-edits never overwritten by refresh.
- **Rationale**: Matches Throne subscription-format behavior from research; full Clash rule-engine import is out of scope.
- **Alternatives considered**: Full Clash config import (rejected: rule-engine scope creep).
- **Owning skill**: none.

## R5 — Desktop UI patterns (Rust kit)

- **Decision**: One window shell with sidebar/master-detail; config collection as entity-backed virtualized list keyed by stable config ID; add/edit in Dialog/Sheet, delete in confirm dialog; TitleBar + StatusBar; theme tokens only; full keyboard model; tray (if any) behind a narrow platform seam with fallback, never an invented kit API.
- **Rationale**: Coding/Design guides require Entity ownership, stable identity, virtualized lists at this scale, token styling, focus/keyboard contracts, and overlay discipline; kit catalog has no tray component.
- **Alternatives considered**: Plain table without virtualization (rejected at 50 rows); per-page roots/dialogs remounted in views (rejected by Root ownership rule).
- **Owning skills**: `gpui-kit`, `gpui-kit-design-guides` (repo-bundled).

## R6 — Storage + secrets

- **Decision**: SQLite WAL as library store; secrets redacted in logs, exports, and clipboard; JSON export with explicit plaintext-secret warning.
- **Rationale**: Fits desktop scale, survives 50+ rows, supports subscriptions + health samples; matches security boundary (never log secrets).
- **Alternatives considered**: Plain JSON files only (rejected: query/filter + subscription refresh bookkeeping).
- **Owning skill**: `sqlite` (consult at implementation for WAL/PRAGMAs).

## Deep-research addendum (2026-09-24, 5 parallel briefs)

Full briefs: `research/01-singbox-scale.md`, `02-amnezia-xray.md`, `03-throne-parity.md`, `04-gpui-arch.md`, `05-import-grammars.md`.

## R7 — Route-action form + WireGuard endpoints (from 01)

Use `{"action":"route","outbound":"<tag>"}` (bare `outbound` deprecated 1.11); WireGuard clients as `endpoints[]`, referenced by tag; `route.final` catch-all; validate with `check`, normalize with `format`, compose with `merge`.

## R8 — Fork-core + sidecar topology (from 02)

Standard engine stays stock; AmneziaWG/Noise via compatible fork-core (version matrix 2.0 vs 3.x required); Xray-only (finalMask/noise, VLESS encryption, XHTTP) via Xray sidecar; loopback ingress bridge with tag-count bookkeeping; Extra/Custom/Xray chain nodes egress-only; guard wrong-layer obfuscation mismatches explicitly; extra exe runs unprivileged from app dir.

## R9 — UI build rules (from 04)

Bootstrap `application → init → open_window → Root` + all three overlay layers; `Entity` collections + `RenderOnce` rows; entities/handles created once outside `render`; domain stable IDs; mandatory virtualization (`List`+delegate); `VirtualList` size/scroll/bounded-parent contract; `WindowExt` dialogs/sheets/alerts; built-in focus trap; `TitleBar` via window options + `StatusBar` regions; no Tray in kit (platform seam + fallback); tokens/rem only; single-Action keyboard model; 4 test layers with `test-support` dev-only.

## R10 — Parser + refresh semantics (from 05)

Per-format field tables (VLESS/VMess-dual/Trojan/SS-triple/Hy2+realm+v1/TUIC/WireGuard-URI+conf/socks/anytls/http) + sing-box target schema + containers (SIP008/v2rayN/Clash-limited/V2Fly detection) + `throne://` command matrix + error taxonomy; refresh = identity-keyed upsert + hand-edit overlay + tombstones + atomic swap (proposal, upstream unspecified).
