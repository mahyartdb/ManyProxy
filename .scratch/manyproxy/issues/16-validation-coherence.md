# 16: Validation coherence + dedupe/group consistency

**What to build:** Manual add/edit validates like import does; edit-blank preserves current values; dedupe uses one key everywhere; dialog group/hints behave consistently.

**Blocked by:** 13 (Convergence — live sidecar log pipe).

**Owns:** sqlite

**Status:** ready-for-agent

- [ ] Scenario: manual add with unknown protocol or non-JSON params → Action: save → Expected: field error naming the problem, row not created. Verification: automated test. Priority: required.
- [ ] Scenario: edit with a field cleared → Action: save → Expected: current value preserved (blank = keep), except port where blank = auto is explicit in UI. Verification: automated test. Priority: required.
- [ ] Scenario: import same server twice with different case/whitespace → Action: commit → Expected: single row (normalized identity dedupe, same as subscription refresh). Verification: automated test. Priority: required.
- [ ] Scenario: single addsub line with dialog group edited → Action: commit → Expected: subscription lands in the edited group, not the payload group. Verification: automated test. Priority: important.
- [ ] Scenario: drop .conf then edit box to links → Action: commit → Expected: parsed as edited text (stale file_hint cleared on edit). Verification: automated test. Priority: important.
- [ ] Scenario: multi-file drop of two .conf files → Action: commit → Expected: per-file stems and keys preserved (no cross-file PrivateKey bleed). Verification: automated test. Priority: important.
- [ ] Scenario: export with hand-made rows → Action: export then check count → Expected: count toast equals re-importable rows, or toast states both numbers. Verification: automated test. Priority: optional.
- [ ] Prohibited side effect: no behavior change for subscription refresh merge (its tests stay green untouched).
