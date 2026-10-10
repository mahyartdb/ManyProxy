# 09: Screenshots, verify trio, design audit

**What to build:** Fresh captures if surfaces visibly changed + full verify trio + hardcoded-color grep + pinned id/string audit + ledger.

**Blocked by:** 08-sheets-status-loading (integrate-and-verify after all build tickets; green promised here if any batch needed a shared branch).

**Owns:** verification (no product-code changes except test pin updates with rationale).

**Status:** done (screenshots deferred — see Ruling 4)

- [ ] Scenario: all T01–T08 merged → Action: run `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, color grep (`rg` → 0 paint hits), pinned id/string audit vs `contracts/ui.md` → Expected: all green; every deviation ledgered with `Ruling:` + commit range + command→result. Verification: operational check (command outputs pasted in ledger). Priority: required.
- [ ] Scenario: surfaces visibly changed → Action: recapture library/add/import/subscriptions at 1000x700 on isolated `MANYPROXY_DB` demo seed (`*.example.com`, no secrets) → Expected: `assets/screenshots/*.png` replaced only if changed, README count/captions updated only if surfaces added. Verification: manual UX review (human layout grader). Priority: important.
- [ ] Prohibited side effect: no product-code drive-bys; no screenshots with real hosts/credentials/tokens; no `--force` pushes.

Retry cap: 1, then escalate (verify tickets must not grind).

## T09 outcome (run by implementer on final tree)
- `cargo fmt --check` clean; `cargo clippy --all-targets --locked -- -D warnings` clean; `cargo test --locked` → 824 passed, 0 failed (run by Regen on final tree).
- Hardcoded-color grep over `src/*.rs` → 0 paint hits (only tray icon RGBA bytes, pre-existing non-paint).
- Pinned audit: all 41 contract ids present in `src/main.rs`; suite green covers `assert_painted`.
