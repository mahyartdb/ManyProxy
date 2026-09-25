# 14: Import-safety — error surfacing + credential leak

**What to build:** Every failure in add/remove/import/subscribe/export paths surfaces a truthful message; no silent no-ops; no credentials on screen or in mislabeled errors.

**Blocked by:** 13 (Convergence — live sidecar log pipe).

**Owns:** gpui-kit

**Status:** done (commit ec10469; `cargo test` 196 passed, verified)

- [ ] Scenario: subscription URL with userinfo → Action: view subscription row → Expected: userinfo never displayed (host only). Verification: automated test. Priority: required.
- [ ] Scenario: DB error on list/toggle/refresh → Action: trigger with failing store → Expected: error notification, never a silent empty list or dead click. Verification: automated test. Priority: required.
- [ ] Scenario: paste with all-invalid items → Action: commit import → Expected: dialog stays open, all failures listed with index/label/reason (not just the first). Verification: automated test. Priority: required.
- [ ] Scenario: bad addsub link → Action: import single line → Expected: original BadBase64/BadHost reason shown, not generic unknown-command. Verification: automated test. Priority: important.
- [ ] Scenario: export write fails → Action: export to bad path → Expected: IO error naming the path, never "database error". Verification: automated test. Priority: important.
- [ ] Scenario: remove subscription fails with Sql → Action: remove → Expected: truthful error, never "Already removed". Verification: automated test. Priority: important.
- [ ] Scenario: background refresh fires for a deleted sub → Action: delete then let tick run → Expected: silently ignored, no failure toast. Verification: automated test. Priority: important.
- [ ] Prohibited side effect: no credential (password, token, userinfo) in any notification, row, dialog, or log line.
