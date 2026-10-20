# T-03 — Release workflow (windows-amd64 ZIP bundle)

- **id**: T-03
- **title**: Release workflow: tag-push triggered windows-amd64 ZIP bundle (app + sidecars + notices, SHA256SUMS, prerelease)
- **epic**: E1 — Release pipeline & portable storage
- **size**: M
- **depends-on**: T-02 (the bundle must be self-contained *with exe-relative data*, which T-02 delivers)
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E1 decisions (Q25 public repo; Q26 matrix; Q27 ZIP = app + 3 pinned sidecars + THIRD-PARTY-NOTICES.md; Q28 ZIP-only, no installer ever); existing `.github/workflows/ci.yml` + `.github/workflows/live.yml` patterns (verified).

## Why

After going public (T-05's human step) the first beta cannot ship from a developer's machine by hand: the ZIP hard rule (Q28) needs a repeatable, auditable bundle — exactly the app exe, exactly 3 pinned sidecar binaries, exactly the third-party notices, with hashes. A GitHub Actions workflow makes the bundle reproducible on every tag and keeps the app's "never downloads binaries" promise intact at the app layer (the *workflow*, not the app, fetches pins).

## What

New workflow `.github/workflows/release.yml` (separate file, does not touch `ci.yml`/`live.yml`):

1. **Trigger**: `on: push: tags: ['v*']` (plus `workflow_dispatch` for rehearsal with a dry-run tag input).
2. **Job `windows-amd64`** on `runs-on: windows-latest`, `timeout-minutes: 60`:
   - checkout `@v4`; `dtolnay/rust-toolchain@stable`; `Swatinem/rust-cache@v2` (mirror `ci.yml` steps 13–17).
   - Build: `cargo build --release --locked` producing `target\release\manyproxy.exe`.
   - Optionally stamp the version/hash env (`MANYPROXY_GIT_HASH` comes from `build.rs`'s best-effort `git rev-parse`; release builds get the real tag naturally).
3. **—or the human gate on sidecars (PWB note, verbatim intent):**
   the workflow fetches the three sidecars **at pinned versions, only from their official upstream release URLs**, **on the maintainer's behalf** — it is the *packaging step* loading files, never ManyProxy-the-app downloading: the app keeps all never-download/never-PATH constraints (assert_no_system_mutation family untouched). Pin exactly:
   - `sing-box` — `https://github.com/SagerNet/sing-box/releases/download/v1.14.2/sing-box-1.14.2-windows-amd64.zip` (matches live.yml's pin at `.github/workflows/live.yml:30–31`),
   - `sing-box-awg` (AmneziaWG fork binary) — pinned version + official upstream release/fork URL recorded as a workflow-level constant with a comment, and a workflow step that prints the resolved URL before download,
   - `xray` — pinned version + official upstream release URL (Xray-core windows-amd64 zip), same treatment.
   Implementer must look up the current maintainer-canonical URLs for the awg fork and xray pins at ticket execution time and record them inline next to the constant (the repo README/QUICKSTART may already document the maintainer's sidecar list — reuse those pins if present). If a pin URL cannot be verified to be the official upstream release, the job **fails loudly** with that message rather than substituting an unofficial mirror.
4. **Bundle assembly (the ZIP hard rule)**
   - staging dir: `manyproxy-v<tag>-windows-amd64\`
   - contents: `manyproxy.exe`, `sing-box.exe`, `sing-box-awg.exe`, `xray.exe`, `THIRD-PARTY-NOTICES.md`, plus nothing else (no `.git`, no configs — the data dir is created at first run by T-02's defaults).
   - `THIRD-PARTY-NOTICES.md`: new file in repo root (Q27 — xray-only arithmetic killed 22/27 candidates; sing-box is mandatory and the notices file names it plus the AWG fork). Generated once in-repo (committed), not at bundle time; the workflow merely copies it. Content: license texts/URLs for gpui-kit, rusqlite (bundled SQLite), and the three sidecar engines as shipped; plain names, no marketing.
5. **Artifacts + release**:
   - `Compress-Archive` → `manyproxy-v<tag>-windows-amd64.zip`
   - `SHA256SUMS.txt` built with `Get-FileHash` (zip + each staged binary), signed-not (no signing story this beta — hashes are the honesty floor).
   - Create the GitHub release with `softprops/action-gh-release` or `gh release create` (choose the least-machinery; propose `gh` since `actions/checkout@v4` already provides git auth) with `--prerelease` **when the tag contains `-`** (prerelease flag; `v0.0.1-beta.1` qualifies) and a body summarizing: first beta, ZIP layout, sidecar pins (versions), hash file, no-auto-update statement.
   - Upload the zip + SHA256SUMS.txt as release assets (no installer, no msi, no deb — Q28).

## Files to touch

- `.github/workflows/release.yml` (new)
- `THIRD-PARTY-NOTICES.md` (new, committed)
- `README.md` — one line in the install/download section pointing at release assets (added only if T-01's README section already exists; otherwise hold)
- no `src/` changes

## Acceptance criteria

- [ ] Pushing a rehearsal tag (e.g. `v0.0.0-test.1`) on a fork/branch yields a green run producing the zip with the five files above and a matching SHA256SUMS.txt; unzip → `cargo`-free folder where `manyproxy.exe` launches with no admin and creates `data\` beside it (manual smoke on the maintainer's machine).
- [ ] The workflow prints the exact pinned sidecar versions and URLs used, and fails loudly on any unverifiable pin rather than substituting mirrors.
- [ ] Prerelease tag → release marked prerelease automatically.
- [ ] The app itself still never downloads sidecars or consults PATH — existing app-level tests untouched and green.
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New hermetic tests: none required (workflow + committed notices file; no Rust behavior change). Note explicitly in the PR body that verification of the workflow is by rehearsal run, not unit test.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData: bundle is a self-contained folder; nothing writes outside it; `data\` created at first run (T-02).
- [x] No-migration: bundle contains no legacy-store logic; fresh installs only.
- [x] Plaintext-always-sealed: no secrets in the workflow (no tokens beyond the default `GITHUB_TOKEN`); sidecar URLs are public constants.
- [x] Hijack non-concept: no guidance, no mention, nothing.
- [x] No system mutation (app code): untouched — **and the PWB note holds**: only the CI job (a maintainer-acting machine) fetches pinned sidecars from official upstream URLs; the app itself has zero download capability (assert_no_system_mutation family must stay green).
- [x] Clipboard `bind:port` discipline: N/A.
- [x] Redaction: N/A.
- [x] Sidecars app-dir only: the bundle *is* the app-dir layout; the engines sit next to `manyproxy.exe` exactly as `engine.rs` resolution requires (`ENGINE_BINARY_BASE`/`FORK_BINARY_BASE`/`XRAY_BINARY_BASE`, engine.rs:15–21).

## Out of scope

- The ARM64 runner row (T-04 extends this file).
- Making the repo public / pushing the tag (T-05, human steps).
- Update-check feed consumption (T-06; the feed URL only works once the repo is public and a release exists).
- Code signing, build attestations, winget/scoop distribution.
- Any Windows release customization beyond a plain zip (no installer ever — Q28).
