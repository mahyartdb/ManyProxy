# 40: Store v5 foundation + integrity hardening

**What to build:** The v4→v5 SQLite migration (per-sub interval, health-history table, preset extension columns) plus the integrity hardening the rest of the program stands on: backup-before-restore with atomic temp-DB rebuild, corrupt-row quarantine with tolerant reads, version-bump-last crash safety, batched traffic writes, tombstone + attach + snapshot fixes.

**Blocked by:** None (can start immediately).

**Owns:** sqlite, database-migrations

**Status:** ready-for-agent

- [ ] Scenario: v4 library with rows, subs, presets, traffic → Action: open with new build → Expected: migrates idempotently to `user_version = 5` with zero data loss; `interval_secs` defaults 86400; fresh and migrated DBs converge to identical DDL per drift check. Verification: automated test (v0–v4 fixture opens). Priority: required.
- [ ] Scenario: restore payload confirmed → Action: commit restore, kill mid-restore in harness → Expected: timestamped backup exists before any wipe; old library intact after mid-kill; success path rebuilds in one transaction after validate + quick_check + rename. Verification: automated test. Priority: required.
- [ ] Scenario: one row with corrupt params blob → Action: open, list, export → Expected: good rows load with banner naming quarantined ids; export offers skip-or-flag; write paths still reject strictly. Verification: automated test. Priority: required.
- [ ] Scenario: DPAPI down during migration → Action: open v4 plaintext DB → Expected: migration fails loudly, version stays (next open retries), never strands plaintext as migrated. Verification: automated test. Priority: required.
- [ ] Scenario: corrupt/foreign DB file → Action: app open → Expected: bad file preserved as `library.db.corrupt-<ts>`, integrity check run, restore-from-export offered; never silent in-memory loss. Verification: automated test. Priority: required.
- [ ] Scenario: export → Action: export then restore elsewhere → Expected: route presets + per-row preset linkage by name + traffic + meta survive, or dialog copy names exclusions. Verification: automated test. Priority: important.
- [ ] Scenario: traffic flowing → Action: concurrent ticks persist → Expected: one transaction per tick, saturating single-statement updates, errors logged not swallowed. Verification: automated test. Priority: important.
- [ ] Scenario: deleted sub member → Action: re-add same sub URL → Expected: delete stays honored (URL-keyed tombstones with rotation-core match + GC). Verification: automated test. Priority: important.
- [ ] Scenario: dangling preset id → Action: attach → Expected: refuses loudly at attach (re-validated in-tx), plus orphan-sweep banner path. Verification: automated test. Priority: important.
- [ ] Prohibited side effect: no live user DB writes from tests; no plaintext fallback; no downgrade support claims.
