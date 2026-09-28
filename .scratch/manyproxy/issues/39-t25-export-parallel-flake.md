# 39: t25 export atomicity test flakes under parallel runs

**What to fix:** `store::tests::t25_export_is_atomic_no_tmp_litter` fails intermittently under default parallel `cargo test` but passes in isolation and under `--test-threads=1`.

**Blocked by:** None. Independent of ticket 38.

**Owns:** sqlite

**Status:** ready-for-agent

- [ ] Scenario: full suite in parallel → Action: `cargo test` → Expected: 100% green, no flakes across 5 consecutive runs. Verification: repeated automated runs. Priority: important.
- [ ] Suspected cause (from tickets 36/37 agents): the test scans the whole shared temp dir for `.tmp-` litter while ticket-32's export test holds an in-flight `commit_export` tmp file — a cross-test race, not a product bug. Confirm or refute with evidence.
- [ ] Suggested fix direction: scope the litter scan to the test's own artifact (own temp subdir / unique prefix), or serialize the two tests. Smallest change that kills the race.
- [ ] Prohibited side effect: no product behavior change; no new dependencies; no live DB writes.
