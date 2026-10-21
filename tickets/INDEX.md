# ManyProxy × Throne Clean-Room — Ticket Index

Source of truth: `C:\Users\qmahyar\AppData\Local\Temp\opencode\manyproxy-borrow-design-decisions.md` (grilled 2026-10-16/17), evidence in `manyproxy-vs-throne-gap.md` and `manyproxy-inventory.md`, constraints in `AGENTS.md`.

## Branch strategy (Q19b)

Every ticket lands as its **own PR off a tracking branch**. Draft tracking branch name: **`throne-borrow`** (branch cut from `master`). One ticket = one PR = one reviewable diff; no rebase-chain mixing across tickets.

## Ordering rules (brief)

1. **Beta ordering (hard)**: `T-01 → T-02 → T-03 → T-04 → T-05` precede **all** feature tickets.
2. `T-06` / `T-07` follow `T-05` (the release feed must exist before update-check work is honest).
3. `T-08` is first among feature tickets (E3 grammar unblocks interop), then `T-09..T-12`, remainder per size/risk.
4. Every ticket carries the AGENTS gate before merge: `cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`.

## Execution order

### Epic E1 — Release pipeline & portable storage (before first beta release)

| id | title | epic | size | depends-on | file |
|---|---|---|---|---|---|
| T-01 | Doc hard-rule sweep (AGENTS.md + README) | E1 | S | — | [T-01-doc-hard-rules.md](T-01-doc-hard-rules.md) |
| T-02 | Storage relocation to exe-relative `<appdir>\data\` | E1 | M | T-01 | [T-02-portable-storage.md](T-02-portable-storage.md) |
| T-03 | Release workflow (windows-amd64 ZIP bundle) | E1 | M | T-02 | [T-03-release-workflow.md](T-03-release-workflow.md) |
| T-04 | ARM64 release row (gated) | E1 | S–M | T-03 (+ public repo, human) | [T-04-arm64-release-row.md](T-04-arm64-release-row.md) |
| T-05 | First beta cut `v0.0.1-beta.1` (human tag step) | E1 | S | T-02, T-03, T-04 | [T-05-first-beta-cut.md](T-05-first-beta-cut.md) |

### Epic E2 — Update-check

| id | title | epic | size | depends-on | file |
|---|---|---|---|---|---|
| T-06 | Update-check client (JSON-only, never executes) | E2 | M | T-05 | [T-06-update-check.md](T-06-update-check.md) |
| T-07 | UA header `ManyProxy/<version>` (no HWID, ever) | E2 | S | T-05 (shares `fetch_subscription_body`; land after T-06) | [T-07-ua-header.md](T-07-ua-header.md) |

### Epic E3 — Routing & interop borrow

| id | title | epic | size | depends-on | file |
|---|---|---|---|---|---|
| T-08 | Action enum extension (NO hijack) + live.yml proof family | E3 | M | T-05 | [T-08-action-enum.md](T-08-action-enum.md) |
| T-09 | Rule-attribute enrichment (Source tab, invert, ip_version, process_path_regex) | E3 | S | T-08 | [T-09-rule-attributes.md](T-09-rule-attributes.md) |
| T-10 | Rule names + prevent-mods lock + completion | E3 | M | T-08, T-09 | [T-10-rule-names-lock-completion.md](T-10-rule-names-lock-completion.md) |
| T-11 | Simple-rules grammar boxes (3 boxes) | E3 | M | T-08 | [T-11-simple-rules-grammar.md](T-11-simple-rules-grammar.md) |
| T-12 | `.srs` rule-set local staging (`route.rule_set` + `local_path`) | E3 | M | T-08, T-09 (joins T-08's live gate family) | [T-12-srs-rule-set-staging.md](T-12-srs-rule-set-staging.md) |
| T-13 | Chain transition-order validation | E3 | S | T-05 | [T-13-chain-transition-validation.md](T-13-chain-transition-validation.md) |
| T-14 | Tray Select-exit health ordering (monitor, not switcher) | E3 | S | T-05 | [T-14-tray-exit-health-order.md](T-14-tray-exit-health-order.md) |

### Epic E4 — Health-test accuracy pack

| id | title | epic | size | depends-on | file |
|---|---|---|---|---|---|
| T-15 | Latency color bands 100/300 (+ failed/unknown distinct) | E4 | S | T-05 | [T-15-latency-bands.md](T-15-latency-bands.md) |
| T-16 | Keep-previous outcomes (0 / aborted never overwrite ring) | E4 | S | T-05 | [T-16-keep-previous-outcomes.md](T-16-keep-previous-outcomes.md) |
| T-17 | Stop Testing shortcut `Ctrl-.` (menu item already exists) | E4 | S | T-05 | [T-17-stop-testing-shortcut.md](T-17-stop-testing-shortcut.md) |
| T-18 | Mux warm-up double probe | E4 | S–M | T-05 | [T-18-mux-warmup-probe.md](T-18-mux-warmup-probe.md) |
| T-19 | Per-group auto-clear-unavailable (opt-in, protected rows) | E4 | M | T-05 | [T-19-group-auto-clear.md](T-19-group-auto-clear.md) |

### Epic E5 — Subscription & exchange pack

| id | title | epic | size | depends-on | file |
|---|---|---|---|---|---|
| T-20 | Diff report text (named `[+]/[~]/[-]` report in refresh toasts) | E5 | S–M | T-05 | [T-20-diff-report.md](T-20-diff-report.md) |
| T-21 | Body format parity (`json://` fragments + WireGuard INI in sub bodies) | E5 | S | T-05 | [T-21-body-format-parity.md](T-21-body-format-parity.md) |
| T-22 | Export envelope `{schema, version, created, checksum}` + restore-side negotiation | E5 | S | T-05 | [T-22-export-envelope.md](T-22-export-envelope.md) |

### Epic E6 — UI affordance pack

| id | title | epic | size | depends-on | file |
|---|---|---|---|---|---|
| T-23 | `port=N:M` search grammar + last-filter memory | E6 | S–M | T-05 | [T-23-port-filter.md](T-23-port-filter.md) |
| T-24 | Click-to-copy cells + `Copied!` transient tooltips (Connections tab) | E6 | S | T-05 | [T-24-click-to-copy.md](T-24-click-to-copy.md) |
| T-25 | Connections counters `N active · M TCP · K UDP` | E6 | S | T-05 | [T-25-conn-counters.md](T-25-conn-counters.md) |
| T-26 | Window title marker `· N running` | E6 | S | T-05 | [T-26-title-marker.md](T-26-title-marker.md) |
| T-27 | Header sort pickers (latency/DL/UL/out-IP; traffic total/down/up) | E6 | M | T-23 | [T-27-header-sort-pickers.md](T-27-header-sort-pickers.md) |
| T-28 | QR encode of listener URLs (both schemes; encode only) | E6 | M | T-05 | [T-28-qr-listener.md](T-28-qr-listener.md) |
| T-29 | Top-9 exits + Other in traffic dialog | E6 | S | T-05 | [T-29-top9-exits.md](T-29-top9-exits.md) |
| T-30 | CPU/RAM mini-charts per sidecar shard | E6 | M | T-05 | [T-30-shard-cpu-ram.md](T-30-shard-cpu-ram.md) |
| T-31 | Log polish: level→color spans + `log_max_lines` meta | E6 | S | T-05 | [T-31-log-polish.md](T-31-log-polish.md) |
| T-32 | Window geometry persistence | E6 | S | T-05 | [T-32-window-geometry.md](T-32-window-geometry.md) |
| T-33 | Bind picker + Settings address list | E6 | M | T-05 | [T-33-bind-picker.md](T-33-bind-picker.md) |

### Epic E7 — Ops marker

| id | title | epic | size | depends-on | file |
|---|---|---|---|---|---|
| T-34 | `last_clean_exit` marker + unclean-exit toast | E7 | S | T-05 | [T-34-last-clean-exit.md](T-34-last-clean-exit.md) |

## Future epics (outline files only — each requires its own grilling session)

- [`EPIC-fork-core.md`](EPIC-fork-core.md) — user-owned ThroneCore-lineage Go core, plain sing-box CLI contract proof gate, own repo (e.g. `QMahyar/ManyProxyCore`), Go cross-arch pipeline, 1-exe layout retirement of bridge `11801–11999` + 3-sidecar packing, MVP windows-amd64 first. **Mihomo path is DEAD (owner chose own fork — Q34a).**
- [`EPIC-linux-macos-port.md`](EPIC-linux-macos-port.md) — per-OS earned rows; keyring backends decided (DPAPI/Keychain/Secret Service, plaintext banned), Windows-only dep gating, gpui Linux X11/Wayland + tray libayatana caveats, full gate per OS before any asset row.

These are **not** ticketed here. Open questions are listed explicitly inside each outline; nothing is assumed.

## Parked (may later adopt — explicit names; INDEX is the single source)

- **Download Profiles ▾ country menu** (Throne's China/Iran/Russia fetch menu; Q5). The `.srs` staging rail (T-12) is built; the country-download *menu* is not.
- **Remote-preset auto-update + per-preset `last_update` stamps** (Q5). `throne://remoteroute` fetch-on-import only (as today; `start_preset_refresh` exists for manual refresh).
- **Traffic hour-bucket history DB** (unlock Throne-style period dashboards; U7 second half). Top-9 Exits (T-29) ships without it.
- **Beta-channel update setting** (while stable releases absent; Throne "allow beta" idea — revisit only when asked).
- **QR share-link variant behind the Share confirm** (Q21/Q24 — T-28 ships listener-URL QR only; the *share-link* QR variant is parked).

## DEAD (permanently confirmed — do not ticket, do not build, do not mention in docs)

- Adblock toggle (Q16).
- **hijack-dns anything** (Q14 rule 4): no mapping, no intake special-casing, no guidance strings, no docs mention. Presets containing it trip the existing generic unknown-action error at intake. Zero machinery built or named.
- Auto-selector pools (C3; `autoselector` intake refusal stays).
- Front/landing per-group chain hops (C2).
- QR screen scan (S4; the typed refusal at `import.rs` stays).
- Per-app process dashboard (stock clash API has no process attribution — never fake it).
- i18n / language packs.
- Installers / NSIS / debs (ZIP hard rule — Q28).
- Scheduler-task autostart (HKCU Run only).
- System proxy / TUN / DNS hijack / insecure fake-IP machinery.
- Geo-asset auto-download.
- WARP cloud registration.
- HWID / `x-device-*` headers (Q7-ratified: the capability must not ship).
- Full-config clipboard copies (clipboard carries `bind:port` only).
- Crash-time stderr-to-disk stencil (K1 rejected — "session-only logs" constraint owned by the user; K1b marker adopted as T-34 instead).
- `running.marker` file (K2 rejected — SQLite running snapshot covers it).
- Mihomo consolidation (Q34a — own ThroneCore fork instead).

## Dependency edges (summary)

- `T-01 → T-02 → T-03 → T-04(and T-05) → T-05` (E1 chain).
- `T-06, T-07 ← T-05`.
- All feature tickets (`T-08..T-34`) require `T-01..T-05` merged first (ordering rule above).
- `T-09 ← T-08`; `T-10 ← T-08, T-09`; `T-11 ← T-08`; `T-12 ← T-08, T-09`; `T-13, T-14 ← T-05`; E4/E5/E6/E7 items ← `T-05` (independent pairs unless stated: `T-27 ← T-23`).
- Live `yml` edits: **only T-08 extends `.github/workflows/live.yml`**; T-12's `.srs` proof joins the same gated family without further workflow edits.

## Verification gate (verbatim, every ticket)

```
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

Single test: `cargo test <substring> --locked`. Live proofs are opt-in only (`MANYPROXY_SINGBOX_BIN` + `MANYPROXY_LIVE_CHECK=1` / `MANYPROXY_LIVE_TRAFFIC=1`, pinned sing-box 1.14.2 via `.github/workflows/live.yml`) — default `cargo test` skips them and must stay hermetic (TEMP files, isolated `MANYPROXY_DB`, no sidecar spawn).
