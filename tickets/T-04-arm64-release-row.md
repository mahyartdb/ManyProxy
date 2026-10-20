# T-04 — ARM64 release row (gated)

- **id**: T-04
- **title**: ARM64 release row — `windows-11-arm` runner job, full gate, asset only when green
- **epic**: E1 — Release pipeline & portable storage
- **size**: S–M
- **depends-on**: T-03 (extends `.github/workflows/release.yml`) **and** the repo being public (human step in T-05) — the free `windows-11-arm` runner is only available to public repos
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E1 (Q26 facts: ARM64 Go — only the C dependency is bundled SQLite, so the toolchain wins are real; Q29: arm64 asset **only if the full gate is green**); Q26 capability audit (windowslegacy/32-bit capability-impossible: GPUI x64-only + no 386 sidecars; linux/mac not rows).

## Why

An arm64 Windows beta is nearly free (single C-dependency = bundled SQLite; everything else pure Rust including gpui), but only worth shipping if the **full gate** proves it — a half-green arm64 asset betrays the project's hermetic-test posture. The row is also the template for any future per-OS earned asset (see `EPIC-linux-macos-port.md`).

## What

Extend `.github/workflows/release.yml` (created by T-03) with a second job:

1. **Job `windows-arm64`**, `runs-on: windows-11-arm`, `timeout-minutes: 60`:
   - checkout + stable toolchain + rust-cache (same as amd64 job).
   - Target: `--target aarch64-pc-windows-msvc` — or native arm64 toolchain if the runner defaults to it; pick the form the runner's pre-installed toolchain supports (verify with a `rustc -vV` echo step; do not install a second toolchain if one exists).
   - **Full gate in the arm64 job, un-skippable** (Q29): `cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked` (all three against the arm64 target). Live proofs stay opt-in and out of the release path (`live.yml` is amd64-only; that asymmetry is accepted — record it in the release body).
   - Release build only after the gate: `cargo build --release --locked --target aarch64-pc-windows-msvc` (or native form above).
2. **—Sidecar pins (PWB note, same discipline as T-03):** the workflow fetches **windows-arm64** sidecars at pinned versions **only from their official upstream release URLs, on the maintainer's behalf** — packaging-time loading, never app-level downloading; the unpinned/absent case must fail loudly, and any pin that does not exist for arm64 (e.g. a fork binary without an arm64 release) must **skip the asset row** with an explicit step summary line, never a silent partial bundle:
   - sing-box `windows-arm64` official asset for v1.14.2;
   - sing-box-awg arm64 asset — verify existence at execution time; if absent, the arm64 job emits **no** arm64 zip and explains why in the step summary (record the finding for the owner; do not substitute stock sing-box, do not fake the fork).
   - xray `windows-arm64` official asset for the pinned version.
3. **Artifacts**: `manyproxy-v<tag>-windows-arm64.zip` + its hashes **appended as its own block** into a per-run `SHA256SUMS.txt` (two files total: one per arch, not one mixed file — keep each arch's sums unambiguous).
   - Release body gains one line per arch with the sidecar pins and a green/red gate statement.
4. **Honest gating**: if the arm64 gate is red, the job fails and **no arm64 asset is published** — amd64 is unaffected; the release still ships.

## Files to touch

- `.github/workflows/release.yml` — extend (does not need `ci.yml`/`live.yml` edits)
- no `src/` changes expected; if the toolchain needs a `rust-toolchain.toml`, treat it as a behavior-neutral addition and note it in the PR

## Acceptance criteria

- [ ] Rehearsal tag run shows the arm64 job's gate (fmt → clippy → test) passing on `windows-11-arm`, then producing the arm64 zip + sums.
- [ ] If any arm64 sidecar pin is unverifiable, the job produces no arm64 asset and states the reason in the step summary (loud refusal, no substitution).
- [ ] The gate is never skipped, shortened, or wired behind `continue-on-error` for the asset path.
- [ ] amd64 job unchanged from T-03 except the release body line.
- [ ] AGENTS gate green locally on amd64 (the arm64 gate runs in CI).
- [ ] New hermetic tests: none (workflow-only; verified by rehearsal run as in T-03).

## Constraints checklist (hard rules)

- [x] Portability / no-AppData: bundle layout identical to amd64; `data\` at first run.
- [x] No-migration: nothing added.
- [x] Plaintext-always-sealed: pinned URLs public; `dpapi.rs` compiles and is used as-is on arm64 (sealing is user-scope DPAPI on any Windows arch; off-Windows loud-fail behavior untouched).
- [x] Hijack non-concept: N/A.
- [x] No system mutation: **PWB note** — only the CI job loads pinned sidecars from official upstream URLs on the maintainer's behalf; the app keeps zero download capability.
- [x] Clipboard `bind:port` discipline: N/A.
- [x] Redaction: N/A.
- [x] Sidecars app-dir only: arm64 engines bundled next to the app exe; no PATH, no download at runtime.

## Out of scope

- Making the repo public (T-05 human step; this row *requires* public to get the free arm runner — if still private, the job is drafted but rehearses nowhere; note that in the PR).
- linux/macOS rows (never in this ticket; `EPIC-linux-macos-port.md` owns them after its grilling session).
- Arm64-native gpui quirks beyond the gate's honest verdict (a red gate is a legitimate squash of this row).
