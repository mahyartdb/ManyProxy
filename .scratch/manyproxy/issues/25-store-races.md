# 25: Store races, transactions, identity, off-thread work (store.rs)

**What to build:** Refresh/edit/remove that can't silently revert user work, resurrect deletes, or freeze the UI; restorable export.

**Blocked by:** none (owns store.rs; main.rs consumers adapt after — no signature breaks except additive).

**Owns:** sqlite

**Status:** done (commit c203faf; `cargo test` 318 passed, verified)

Findings to fix (all src/store.rs unless noted, verify each in code before fixing):
- HIGH edit-revert race (889-902 × 1222-1233): update() leaves identity_key stale so refresh overwrites user param/protocol fixes. Fix: refresh identity_key on update + distinguish user-changed vs upstream-changed in merge (overlay covers params/protocol or versioned compare).
- HIGH export restorability (149-177): export full params + enabled + identity + subs metadata (last_refresh_at/last_error/failures); import reads library/subs sections; tombstones exported so deletes stay deleted. Round-trip test: export → wipe → import = same rows, same groups, same enabled, no resurrections.
- HIGH UI-thread blocking (683-685 single conn; main.rs:1524-1531, 2760-2773, 1505-1517, import.rs:1552-1589, main.rs:2404): move probes + batch inserts + refresh merge off the UI thread (spawn_blocking) with bounded time; batch transactions for commit_batch (one tx, not ~1000 statements) and health saves.
- MED remove() non-atomic (912-939): single transaction for delete + tombstone + snapshot prune.
- MED remove_subscription() window (1070-1091, bare TEXT 773-774): transaction; released rows get identity cleared so a second subscription CAN adopt them (fixes cross-sub shadowing).
- MED identity staleness (1295-1299 vs 1563-1567 vs 1170-1172): Op::Update refreshes identity_key; upstream identity rotation updates in place (keeps id/port/enabled) instead of delete+re-add.
- MED snapshot leak on refresh-delete (1301-1306): prune like remove() does.
- MED identity over-merge (import.rs:1642-1700 — implement here or in import, one place): widen keys to transport/security/flow/fp/pbk/alpn/plugin/method.
- MED migrations (759-828): user_version-gated migrations + minimum SQLite assert; repair path for unknown drift (fail loudly with actionable error, never silent empty).
- LOW L1 first-wins comment vs last-wins code (1149-1157): make code match comment.
- LOW L2 port determinism: free same-pass deletions; deterministic new-port order.
- LOW L3 exhaustion index (1250-1256): real per-item indices.
- LOW L4 atomic export (1380-1402): tmp + rename.
- RUST port cast (map_row:268): CHECK constraint or saturating reject on out-of-range.
- RUST latency cast (204): clamp negatives to 0/reject.
- RUST failure index overflow (653-656 + import.rs:178): saturating_add.

- [ ] Scenario: edit owned row params → Action: refresh → Expected: user fix kept (or explicit conflict surfaced), never silent revert. Verification: automated test. Priority: required.
- [ ] Scenario: export → wipe → import → Action: round-trip → Expected: identical rows/groups/enabled, deletes stay deleted. Verification: automated test. Priority: required.
- [ ] Scenario: 500-node import → Action: commit → Expected: completes without UI freeze (off-thread + single tx). Verification: automated test. Priority: required.
- [ ] Prohibited side effect: refresh merge tests stay green except listed verdict changes; no public API breaks for main.rs (additive only).
