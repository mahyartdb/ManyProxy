# T-28 — QR encode of listener URLs (both schemes; encode only)

- **id**: T-28
- **title**: Share-dialog QR for listener URLs — `socks5://host:port` and `http://host:port`, two schemes on one inbound, encode-only
- **epic**: E6 — UI affordance pack
- **size**: M
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E6 T-28 line (`socks5://host:port` + `http://host:port`, two lines, both schemes — mixed inbound serves both on the same port; Q24a; **encode only; no scanner ever**), Q21/24 in the grilling ledger; gap §4.3 U9 (encode-only; "PNG render not needed — gpui can draw modules as a grid"); verified: `store.rs:497 proxy_urls(bind_ip, port) -> (String, String)` produces exactly the two URLs; Share dialog = `open_share_for_row` (main.rs:12506) rendering link text + `SHARE_WARNING` alert (main.rs:1812); QR scan intentionally refused today (import.rs:2992 — stays).

## Why

Phones and LAN devices point-scan at a listener. The two URLs a mixed inbound serves (`proxy_urls`, store.rs:497) are the credential-free contact strings already in the Share dialog — the QR is a *transcription* of them, not a new share format. What stays dead by decision: any QR of share links/configs (PARKED variant behind Share confirm; can be revisited), and any QR **scanner** (permanently dead).

## What

1. **Encoder**: pure QR byte-mode encoder rendering a module grid for a given UTF-8 payload. Implementation choice: hand-rolled minimalist encoder (byte mode, error-correction L/M fixed, versions auto up to cover ≤ 255 bytes) or the smallest pure-Rust zero-dep crate (e.g. `qrcodegen` — nayuki, no deps, no IO, ~600 lines, MPL-2 compatible practice). **Prefer the dependency-light path** (`qrcodegen`) — hand-rolling QR is notoriously error-prone; the repo carries no QR crate today, so this ticket adds exactly one zero-dep dev-light dependency, noted in the PR with the license/emptiness claims (no network, no exec, no filesystem).
2. **Content (fixed)**: the row's two listener URLs via `proxy_urls(config.bind_ip, config.port)` — rendered as TWO QR patterns or ONE QR of a two-line payload? Brief: "two lines, both schemes". Render **two QR images side by side**, one per URL line (each line still scannable on phones), labeled `socks` / `http` beneath — matches "two lines, both schemes"; single payload QR would smuggle whitespace fidelity risk into scanners — two QRs avoid it.
   - **bind:port discipline**: each URL already carries only scheme://host:port — no params, no creds — compatible with the discipline by construction.
3. **Placement**: inside `open_share_for_row` (main.rs:12506) — the dialog grows a QR region below the link text and the warning block (gpui canvas/element drawing modules as a grid monochrome — dark/light theme aware: black-on-white module + white quiet zone minimum 4 modules per QR spec; small-cell `rs` grid).
4. **No scanner, no camera intake, no export-to-PNG** in this ticket (render-in-dialog only; import.rs:2992's typed scan refusal stays byte-identical).
5. Wildcard/`0.0.0.0` binds render their literal URL exactly as the text does today (no IP normalization in the QR — same string as the sibling text, or the summary line unchanged).

## Files to touch

- `Cargo.toml` (+ `Cargo.lock`) — one pure zero-dep encoder crate (`qrcodegen`), pinned
- `src/main.rs` — Share-dialog QR region + render helper + tests

## Acceptance criteria

- [ ] Share dialog for a row renders two QRs (socks + http), each encoding exactly `proxy_urls`' line for that row (byte-equal strings — property test comparing QR payload re-decode impossible → compare pre-encode strings + module grid hash of a fixed known payload as regression).
- [ ] Modswise monotonic on window resize (no clipped quiet zone; min size respected; dark/light themes both legible).
- [ ] No scanner intake; no camera capture; `import.rs` scan refusal untouched; no image files written.
- [ ] Large bind IPs / namespace characters don't corrupt rendering (all RFC-safe payloads encode).
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t78_`, verified unused): `t78_share_dialog_renders_two_qrs`, `t78_qr_content_equals_proxy_urls_lines`, `t78_qr_grid_known_payload_shape`, `t78_no_scanner_refusal_untouched`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData / No-migration / Hijack / Sidecars: N/A.
- [x] **No system mutation / supply chain**: the added crate is pure computation, no network/IO; noted in PR. Nothing executes at runtime from it.
- [x] **Clipboard discipline**: the QR pixels carry exactly the two contact URLs — the same `bind:port`-class data the dialog text shows; **no share-link QR** in this ticket (PARKED variant); nothing touches the clipboard.
- [x] Plaintext-always-sealed: listener URLs are contact strings, already displayed in plaintext in the same dialog.
- [x] Redaction: N/A.

## Out of scope

- QR of share links (`manyproxy://add/…`Parking behind Share confirm per decision), deep-link QR variants.
- QR scanning (permanently dead — typed refusal stays).
- PNG/file/photo export, print layouts.
- WiFi-style credential QRs (listener has no creds — nothing to encode).
