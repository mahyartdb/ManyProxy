# 23: Engine lifecycle hardening (engine.rs)

**What to build:** Sidecar lifecycle that cannot orphan processes, leak secret files, or split-brain on partial failure.

**Blocked by:** none (owns engine.rs).

**Owns:** rust-networking

**Status:** done (commit 425a4a6; `cargo test` 299 passed full-tree incl. 24, verified; clippy/fmt clean)

Findings to fix (all src/engine.rs, verify each in code before fixing):
- CRITICAL start-order: `start()` writes config (2816) → `stop()` deletes it (2870) → spawn reads deleted path (2824). Fix order: stop-child → write → spawn; split kill from file-removal.
- HIGH no Drop: no `impl Drop` anywhere → window-close/crash orphans sidecars + secret temp files. Add RAII teardown (kill + best-effort file removal); no `main()` close hook exists (main.rs:3065) — teardown must live in the backend.
- HIGH fixed shared temp paths (2701/2714/2727/2740) + plain fs::write (2772-2776): per-instance unique paths (pid suffix), restrictive permissions, cleanup on stop/crash/poll-exit; check_config writes (2797/2800) must delete too.
- HIGH split-brain start-all (3476-3511, active committed 3224/3251 pre-apply): validate all starts before destructive stops where possible; rollback on failure; status must agree with per-row is_running (3560).
- HIGH crash path (2877-2881, 3519-3534): transition the failed shard + clean its file, not just record a string.
- MED crash gap (2873-2875, 3516-3534): one-shot per-poll loses N-1 causes; liveness = handle presence. Report all crashed shards per poll.
- MED port pre-check (1474-1489, 1787-1799, bridges 1618-1623/1801-1802): probe-bind before spawn + normalize aliases (127.0.0.1/localhost/::1/::ffff).
- MED reader generation (2832-2848, buffer 2706/2867-2869): JoinHandle/epoch so restarts can't interleave stale lines; poisoned-mutex silent drop must surface.
- MED probe/spawn skew (2748-2759, 2788-2792, 2815, 3280-3308 vs 3483-3486): pin the probed path for spawn; exec-bit check; no PATH fallback for bundled cores (extra slot already app-dir-only — copy the pattern).
- MED unbounded wait (2862-2863): timeout + kill escalation, never hang the UI thread.
- LOW: VecDeque for 2000-line cap (2691); bridge-count preview/apply agreement (1801 vs 1618-1622); clear extra_exe when slot empties (3473-3475/2892-2894); dup-id start_all names the id (3243); remove double validation with swallowed context (3322-3360).
- SEC-HIGH custom-config bypass (843-893, apply 3361-3399): run recursive assert_no_system_mutation (3724-3754) on the REPLACEMENT path, not just tests; validate_structure (2262) must cover replacements.
- SEC-MED check-stderr unredacted (2801-2811 → main.rs:1438/1359 toasts): run CheckFailed detail through redact_secrets.
- SEC-MED controller LAN enforcement: start_one (3213-3226)/start_all (3242-3253) must enforce requires_lan_confirm independent of UI.
- SEC-LOW temp perms + PATH fallback (L4/L5): covered by temp fixes above + no-PATH rule above.

- [ ] Scenario: start with no binaries → Action: start → Expected: typed Missing* error, no crash, no file left. Verification: automated test. Priority: required.
- [ ] Scenario: kill sidecar externally → Action: poll → Expected: named shard reported + file cleaned, siblings keep running. Verification: automated test. Priority: required.
- [ ] Scenario: custom config with nested tun/dns → Action: start → Expected: rejected naming the path. Verification: automated test. Priority: required.
- [ ] Prohibited side effect: no behavior change for FakeEngineBackend consumers; no new deps.
