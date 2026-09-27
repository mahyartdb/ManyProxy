# 35: DPAPI secrets at rest

**What to build:** stored proxy credentials encrypted with Windows DPAPI (user scope) with silent v2→v3 migration.

**Blocked by:** None (can start immediately).

**Owns:** security-and-hardening, sqlite

**Status:** ready-for-agent

- [ ] Scenario: library with credentials → Action: open `%APPDATA%\ManyProxy\library.db` bytes → Expected: no private key, password, token, or uuid plaintext anywhere in the file. Verification: automated test. Priority: required.
- [ ] Scenario: v2 plaintext DB → Action: app opens it → Expected: migrates in one transaction, all rows/flags/subscriptions intact, all params encrypted. Verification: automated test. Priority: required.
- [ ] Scenario: DPAPI encrypt→decrypt round trip → Action: store then read a row → Expected: identical params. Verification: automated test. Priority: required.
- [ ] Scenario: tampered ciphertext → Action: read the row → Expected: loud typed error, never silent plaintext. Verification: automated test. Priority: required.
- [ ] Scenario: export → Action: export library → Expected: succeeds with the unchanged verbatim plaintext warning (decrypt-then-write). Verification: automated test. Priority: important.
- [ ] Prohibited side effect: no plaintext fallback on DPAPI failure; no secrets in logs/errors/tests; no test writes to the live DB.

**Status:** done (verified 413 passed / 0 failed, fmt+clippy clean)
