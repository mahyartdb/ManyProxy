# T-01 — Doc hard-rule sweep (AGENTS.md + README)

- **id**: T-01
- **title**: Doc hard-rule sweep (AGENTS.md + README)
- **epic**: E1 — Release pipeline & portable storage
- **size**: S
- **depends-on**: — (first ticket; branch gate `cargo fmt --check && cargo clippy --all-targets --locked -- -D warnings && cargo test --locked` still runs)
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief hard rules 1–5; Q30 (no-migration); Q28 (ZIP hard rule); Q33 (multi-OS secrets = DPAPI now, keyring later); gap §1 honest-doc note ("AGENTS.md's '~950 KB monolith' understates `src/main.rs` — 44,545 lines, ~1,710 KB"); supported-matrix constraint note (ratified in the grilling session).

## Why

Before the first public beta the docs must state the five settled hard rules, because every later ticket's constraint checklist references them and outside contributors will read only AGENTS.md/README:

1. **Portability rule** — ManyProxy is always a ZIP; self-contained in one folder; no AppData, no Program Files, no installers; the folder can be moved anywhere; engines and data live beside the exe (Q28).
2. **No-migration rule** — beta app, no users; **no user-data migration path ever**. Legacy stores (e.g. `%APPDATA%\ManyProxy\library.db`) are never read and never copied. Fresh installs only; `MANYPROXY_DB` / `MANYPROXY_EXPORT` / `MANYPROXY_LOGS` env overrides remain as test/escape hatch (Q30).
3. **Secrets rule (multi-OS)** — plaintext banned on every OS. Windows = DPAPI user-scope `dpapi1:` exactly as today (zero behavior change); future non-Windows backends (Keychain / Secret Service via keyring) behind the same sealed-container semantics; off-Windows builds fail loudly rather than store plaintext (Q33 = (a); `dpapi.rs` as-built).
4. **The DNS-hijack non-concept** — stated as a *generator* note without using the word "hijack" beyond one factual sentence: presets containing Throne's DNS-mapping action simply reach the existing generic unknown-action intake error (`preset.rs:104 validate_preset_rules` + `preset.rs:6 VALID_PRESET_ACTIONS`) and are not special-cased. No machinery is built, named, mapped, or documented. (Q14 rule 4 — the brief **supersedes** the gap doc's R1 proposal of mapping to `direct`; there is no mapping, no annotation, no mention.)
5. **No system mutation** (existing, restated) — no system proxy, no TUN, no DNS-rewrite machinery, no routing *editor*, no elevation, no services/drivers, no telemetry, no silent binary downloads; sidecars app-dir only.
6. **Supported matrix statement** — supported-now: windows-amd64 (`windows-arm64` lands gated via T-04); tracked-as aspiration: linux/macOS via the dedicated port epic (outline: `EPIC-linux-macos-port.md`). ManyProxy stays Windows 10/11-only for beta — rewrite the current "Windows 10/11-only" opening line to reflect the now/aspiration split instead of deleting it.
7. **Stale monolith fact** — AGENTS.md line "Entry: `src/main.rs` (~950 KB monolith …)" is wrong by ~2×: actual is **44,545 lines (~1,710 KB)** [C measured]. Update to the real number plus "(measured 2026-10; re-measure before citing)". No navigation change advised (inventory stays single-crate; do not split speculatively).

## What

- Edit `E:\Code\ManyProxy\AGENTS.md`:
  - Rewrite the first line per (6).
  - Under **Constraints (do not break)**, add rules 1–4 verbatim-spirited: portability (ZIP/folder, exe-relative data), no-migration (no legacy store reads/copies; overrides intact), secrets-across-OS wording (DPAPI-sealed per OS backend; plaintext banned), and the unknown-action note for Throne's DNS-mapping action word (one sentence, generic phrasing, no feature naming).
  - Fix the `src/main.rs` size line to "44,545 lines (~1.7 MB monolith — all UI/dialogs/ticks/shortcuts)".
- Edit `E:\Code\ManyProxy\README.md`:
  - A short "Design posture" section: ZIP/portable-folder model; never consults `PATH`, never downloads binaries (`engine.rs` binary lookup is app-dir ONLY); opt-in-only autostart (HKCU Run) and session restore, no auto-update; DPAPI-sealed secrets; redacted logs; clipboard carries `bind:port` only; supported matrix line.
  - One sentence pointing route-profile importers at the presets manager (`throne://route` links accepted; unknown actions fail loudly at intake).
- Do **not** touch `PACKAGING.md`, `PACKAGING-NOTES.md`, `PRODUCT.md`, `DESIGN.md`, `QUICKSTART-RUN.md` in this ticket (they may be swept by a later doc pass; keep the diff minimal).

## Files to touch

- `E:\Code\ManyProxy\AGENTS.md` (verified stale line: "~950 KB monolith")
- `E:\Code\ManyProxy\README.md`

## Acceptance criteria

- [ ] AGENTS.md states the portability, no-migration, secrets, unknown-action, and system-non-mutation rules; the Windows-only line is replaced by supported-now (windows-amd64; arm64 tracked via T-04) vs tracked-aspiration (linux/macOS port epic).
- [ ] The `src/main.rs` line says 44,545 lines / ~1.7 MB (or updated real number at merge time, re-measured).
- [ ] README.md carries the design-posture section and matrix line; no "hijack" feature naming beyond the one factual unknown-action sentence.
- [ ] No code changes — `git diff --stat` shows only the two .md files.
- [ ] AGENTS gate green: `cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`.
- [ ] New hermetic tests: none required (docs-only ticket). The existing test suite proves the statements are still true (e.g. binary-lookup/PATH tests, hermeticity). If any doc statement is falsified by an existing test, fix the statement, not the test.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData: rule *stated*; no code path added (T-02 implements it).
- [x] No-migration: rule *stated* verbatim-spirited; no legacy read/copy code added anywhere.
- [x] Plaintext-always-sealed: rule *stated*; `dpapi1:` behavior untouched.
- [x] Hijack non-concept: no mapping/special-casing/guidance/docs feature mention; the only sentence is the generic unknown-action fact.
- [x] No system mutation: rule restated; assert_no_system_mutation family untouched.
- [x] Clipboard `bind:port` discipline: untouched.
- [x] Redaction: untouched.
- [x] Sidecars app-dir only: rule restated verbatim.

## Out of scope

- Implementing the storage relocation (T-02), the release workflow (T-03), or the ARM64 row (T-04).
- Editing any `src/` file, any test, or any other .md file.
- Rewriting the supported matrix into the port epic's detail (that lives in `EPIC-linux-macos-port.md`, which needs its own grilling session).
