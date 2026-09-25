# 20: Release packaging layout (parallel-safe: new files only)

**What to build:** A reproducible Windows release layout: portable zip structure (manyproxy.exe + sidecar engine binaries + README), version stamping source, and a packaging checklist. No src/ changes; do not edit Cargo.toml (owned by feature work).

**Blocked by:** none.

**Owns:** none (docs/scripts only)

**Status:** done (commit a66763e; PACKAGING.md verified against code)

- [ ] Scenario: clean checkout → Action: follow packaging doc → Expected: documented steps produce a runnable portable dir. Verification: manual UX review. Priority: important.
- [ ] Scenario: sidecar matrix → Action: read doc → Expected: exact binary names/versions/placements for stock, fork, xray sidecars. Verification: manual UX review. Priority: important.
- [ ] Prohibited side effect: no changes under src/; no Cargo.toml edits; no network downloads wired into the app.
