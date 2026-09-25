# 11: Convergence — user-owned JSON export

**What to build:** A JSON export path for the library (configs + subscriptions, no health samples) behind the existing plaintext-secret warning: export dialog shows `export_warning_text` verbatim, requires explicit confirm, writes the file, and the import grammar pack round-trips it back without duplicates.

**Blocked by:** 09 (Hardening + release proof).

**Owns:** sqlite

**Status:** done (verified: `cargo test` 177 passed)

- [ ] Scenario: library with 3 configs → Action: export with confirm → Expected: file parses as JSON, re-import yields 0 added / N duplicates, warning shown verbatim before write. Verification: automated test. Priority: important.
- [ ] Scenario: export cancelled → Action: dismiss warning → Expected: no file created. Verification: automated test. Priority: important.
- [ ] Prohibited side effect: export never redacts-then-breaks (fields stay valid for re-import); no auto-export, no cloud destination.
