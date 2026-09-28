# 51: Packaging + ship (Phase 3)

**What to build:** Installable, diagnosable, documented release: portable-zip script + optional per-user installer with clean uninstall (Run value + data residues) and stale-autostart repair, stock/xray min-version preflight with versions in About, VERSIONINFO in the exe, first-run missing-sidecar banner, reconciled docs, hash/sign provenance with pinned sidecar table and release-build record, manual-update pointer with no auto-update.

**Blocked by:** 50-security-pass (ships only on security-green tree).

**Owns:** deployment-patterns

**Status:** ready-for-agent

- [ ] Scenario: moved exe with stale HKCU Run value → Action: launch → Expected: repair offered (stored path vs current exe); uninstall removes Run value + data file + documents every residue path. Verification: operational check. Priority: required.
- [ ] Scenario: portable zip on clean machine → Action: unzip, run with MANYPROXY_DB override documented → Expected: runs without registry writes unless opted in; data location documented in README/PACKAGING. Verification: operational check. Priority: required.
- [ ] Scenario: old stock/xray binary beside exe → Action: Start / open About → Expected: typed need-≥X error; About shows parsed version lines per slot, not just found/missing. Verification: operational check. Priority: required.
- [ ] Scenario: built exe → Action: inspect properties → Expected: VERSIONINFO present (Cargo version + short hash). Verification: operational check. Priority: important.
- [ ] Scenario: launch with zero sidecars → Action: observe → Expected: startup banner naming expected app-dir paths with Copy-path + Recheck (never silent). Verification: manual UX review. Priority: important.
- [ ] Scenario: docs read-through → Action: compare README / PACKAGING / PACKAGING-NOTES / QUICKSTART against app → Expected: install, tray, version, shortcuts, update statements all match; no contradicted claims remain. Verification: manual UX review. Priority: important.
- [ ] Scenario: release build → Action: run provenance script → Expected: SHA-256 hashes + sidecar source URLs recorded in a pinned table; full release build wall-time recorded; signing command documented. Verification: operational check. Priority: required.
- [ ] Scenario: About / docs update section → Action: read → Expected: static release URL + "updates are manual installs; the app never replaces binaries" pointer; no version ping. Verification: manual UX review. Priority: important.
- [ ] Prohibited side effect: no silent downloads; never PATH resolution; never admin elevation for sidecars; installer never installs machine-wide without explicit choice.
