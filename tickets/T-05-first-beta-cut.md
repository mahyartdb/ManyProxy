# T-05 — First beta cut `v0.0.1-beta.1`

- **id**: T-05
- **title**: First beta cut — Cargo version `0.0.1`, prerelease tag `v0.0.1-beta.1`, repo public
- **epic**: E1 — Release pipeline & portable storage
- **size**: S
- **depends-on**: T-02, T-03, T-04 (the brief: "depends T-02..T-04")
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b) — then the tag is cut from `master` **after** the tracking branch merges
- **provenance**: brief E1 (Q31: tag `v0.0.1-beta.1`, Cargo version set to `0.0.1` to keep About/version-check honest; Q35a beta-first; Q25 public); gap §5 item 6 ("no auto-update" as in-app promise pairs with user-supplied tag cuts).

## Why

The release channel's honesty starts here: the app reports `APP_VERSION` from `env!("CARGO_PKG_VERSION")` (main.rs:184), and the future update-check (T-06) compares that value against the feed. Shipping a tag that disagrees with the in-app version breaks both the About dialog and the comparison — so the Cargo version moves to `0.0.1` (plain semver, no prerelease suffix — the *tag* carries `-beta.1`), and the tag lives only on a public repo.

## What (agent steps)

1. Edit `Cargo.toml`: `version = "0.1.0"` → `version = "0.0.1"` (downgrade deliberate — declared, not accidental).
2. Regenerate `Cargo.lock` for the version question if the locked entry changes (run `cargo update --workspace --precise manyproxy@0.0.1` or simply `cargo build --locked` once to refresh the `manyproxy` package entry; no dependency bumps otherwise).
3. Confirm About-dialog text derives from `APP_VERSION`/`GIT_HASH` automatically (main.rs `open_about_dialog`/About region ~14212) — no code edit expected; verify the About shows `0.0.1` after the build.
4. Add a format-check-style hermetic test pinning that `APP_VERSION` parses as plain `major.minor.patch` semver (no prerelease/build suffixes in the Cargo version — the tag carries them): `t55_app_version_is_plain_semver` in main.rs tests (greps the constant at main.rs:184 through `CARGO_PKG_VERSION`).
5. Update the version line anywhere release-drift-prone docs state `0.1.0` (grep `0.1.0` across `*.md`; only touch files T-01 already owns or a single release-notes line; otherwise leave).

## HUMAN STEPS (the owner's actions — never agent actions, never scripted here)

> These are the owner's steps, phrased as they perform them. The agent must not attempt, script, draft, or hint at executing them.

1. **Make the repository public** — the owner opens `https://github.com/QMahyar/ManyProxy/settings`, chooses "Danger Zone → Change visibility → Make public", and confirms. The repo must be public before tagging so (a) the release is reachable, and (b) T-04's `windows-11-arm` runner is available.
2. **Cut the tag** — the owner runs, from a clean `master` at the merge commit of `throne-borrow`:
   ```
   git tag -a v0.0.1-beta.1 -m "First beta"
   git push origin v0.0.1-beta.1
   ```
   The tag push triggers T-03's release workflow, which builds, gates, bundles, and publishes the prerelease assets automatically. The owner verifies the run is green and that the release shows as **prerelease** with `manyproxy-v0.0.1-beta.1-windows-amd64.zip` + `SHA256SUMS.txt` (and the arm64 row if T-04 went green).

## Files to touch

- `Cargo.toml` (version line)
- `Cargo.lock` (package version entry only)
- `src/main.rs` tests (the one `t55_` pinning test)
- at most a one-line release-notes/readme version mention

## Acceptance criteria

- [ ] `Cargo.toml` says `version = "0.0.1"`; About dialog renders `0.0.1` (manual smoke) and `APP_VERSION` parses as plain semver.
- [ ] `cargo test --locked` green including `t55_app_version_is_plain_semver`; full AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] The human steps (public repo + tag push) were performed by the **owner**; the release workflow run they trigger is green and published the prerelease.
- [ ] No other code change rides in this PR (single-purpose cut).

## Constraints checklist (hard rules)

- [x] Portability / no-AppData: N/A (no code path change).
- [x] No-migration: N/A.
- [x] Plaintext-always-sealed: N/A; no secrets introduced by the tag/release.
- [x] Hijack non-concept: N/A.
- [x] No system mutation: N/A. (PWB rules do not apply — this ticket is manifest-only; release tooling was settled in T-03/T-04.)
- [x] Clipboard `bind:port` discipline: N/A.
- [x] Redaction: N/A.
- [x] Sidecars app-dir only: N/A.

## Out of scope

- Publishing the release itself (that is T-03's workflow reacting to the owner's tag push).
- Update checks or any feed consumption (T-06 — only honest now that the tag exists).
- Version-ping/telemetry of any kind (permanently dead).
- Bumping to any later version, changelog machinery, or release-notes automation.
