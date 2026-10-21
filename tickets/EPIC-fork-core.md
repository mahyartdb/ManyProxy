# EPIC-fork-core.md — Future epic outline (requires its own grilling session)

> **Status**: outline only — NOT ticketed. Every open question below needs the owner's grilling before any spec/ticket exists. Nothing here is committed as scope.
> **Provenance**: brief "FUTURE EPICS (own grilling session each)" first bullet — Fork-core epic (Q26/34).

## One-line intent

Give ManyProxy a **user-owned single Go core** derived from the **ThroneCore source lineage** (already embedding Xray + AmneziaWG), retiring the 3-sidecar packing (incl. the `11801–11999` Xray bridge machinery), while proving — before any swap — that the core satisfies the plain `sing-box` CLI contract that everything in this repo assumes.

## Shape of the idea (settled in the brief)

1. **Base**: ThroneCore source lineage. Q34a settled "own fork" — the **Mihomo path is DEAD** (owner chose the ThroneCore lineage over consolidating on Mihomo).
2. **Strip list (Throne-specific surface to remove)** — from the brief:
   - IPC RPC plumbing,
   - system-proxy/DNS services (whole-system mutation machinery — permanently out of posture for ManyProxy too),
   - parent-identity checks + `-flag` argument handling (Throne's launcher identity passthrough).
3. **Proof gate before swap**: MUST prove the **plain sing-box CLI contract** `check -c` / `run -c` and the clash API surface ManyProxy consumes (`inject_clash_api` contract: loopback `external_controller:127.0.0.1:<port>` with a secret; per-connection rows attribution by inbound tag `in-<rowid>`; `connections` totals) — replicate today's gates (`sing-box check -c` vestibule pre-spawn, engine's config-build shape) against the fork binary.
4. **Repo**: e.g. `QMahyar/ManyProxyCore` with its **own Go release pipeline** cross-building: windows amd64/arm64 (windows-gnu toolchain question below), linux amd64/arm64, darwin amd64/arm64 — Go cross-compiles all; each release asset statically pinned by ManyProxy's release chain (ManyProxy release CI pins the core **from its own releases only** — never a moving main).
5. **Engine gains a 1-exe layout**: retire, per directed tickets (to be drafted in that session):
   - the `11801–11999` Xray bridge machinery (`engine.rs:175–176 XRAY_BRIDGE_PORT_START/END`, bridge/tag-count signatures, `XRAY_BRIDGE_COUNT_SIGNATURE`),
   - the 3-sidecar packing model (`sing-box` / `sing-box-awg` / `xray` app-dir layout, `ENGINE/FORK/XRAY_BINARY_BASE` at engine.rs:15–21),
   - with replacement: one core exe handling AWG/xray-lineage protocols natively (needs_fork/needs_xray routing collapses into owning-core composition).
6. **MVP swap order**: windows-amd64 **first**, then per-arch earned rows (mirrors how E1's release chain earned arm64 gating).

## Open questions for the grilling session (never assume)

- Exact ThroneCore lineage pin/commit and its license posture — what artifacts are redistributable under QMahyar's name; what must stay "private build" vs "published core".
- Which AWG/Xray **features** the current app actually consumes (AWG_V2/v3 key sets, XHTTP transport, vless-encryption) and which the lineage core still serves — a feature audit must precede the strip list.
- **windows-gnu vs msvc cross-build matrix** — Go builds windows amd64/arm64 with ANSI/toolchain quirks; which targets produce a clean single-exe release; CGO posture for anything needing sqlite-like embeds (forbidden in the core — it should be pure-Go config/dialer only).
- Clash-API **compat ceiling**: which fields ManyProxy's `poll_clash_entries`/connections parse actually require (traffic attribution via `in-<id>` tags, download/upload totals, `network` field) and whether the lineage core's clash API answers all of them identically — a contract test suite to freeze.
- Config-compat ceiling: how much of the generated sing-box config shape (per-row inbounds/mixed + isolation rules + route presets + rule_set local paths) the fork accepts verbatim — every delta here becomes engine.rs delta work in the swap tickets.
- Release supply chain: pin discipline (per-release checksums the app-side workflow verifies at bundle time), rotation policy when a fork regression appears (rollback flow — pin the previous core release?).
- Retirement sequencing: during the transition, do the two layouts coexist per-arch (3-sidecar for already-earned arches, 1-exe for MVP-target arch), and what gates that bridge shut-off per arch.
- Testing strategy parity: the current hermetic `assert_no_system_mutation`-family plus engine contract tests must hold unchanged — what the fork-core repo owes ManyProxy (contract tests + fixture configs) vs what ManyProxy keeps.
- Naming/branding of the core repo and its binaries; whether the app presents it as a vendor ("engine: ManyProxyCore vX") in the About sidecar lines.

## Non-goals recorded now

- Mihomo consolidation (dead — Q34a).
- Any system-proxy/DNS/TUN/dns-hijack service in the core (permanently out of posture).
- LD-scripts / dynamic plugin marketplaces / auto-updating core from within the app (feeds the thrown posture: the app pins core **releases**; updates are user-driven via releases).

## Sequencing

Runs **after** the borrow tickets (`throne-borrow` thread) in parallel-adjacency with the linux/macOS port epic (`EPIC-linux-macos-port.md` — the core's cross-arch releases are that epic's precondition for per-OS asset rows).
