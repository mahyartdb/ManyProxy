# 05: Ruled train-order dialogs

**What to build:** Add as a protocol-first mini-form (server, port, identity fields per protocol) with raw JSON as an Advanced disclosure and inline validation; a plain-words Bind guard that says leaving loopback exposes the machine; import keeps its live valid/invalid preview; export stays a red-bordered caution order with the verbatim secret warning. Every safety confirm keeps naming affected rows and every dialog keeps the single shared commit path for mouse and keyboard.

**Blocked by:** None (can start immediately).

**Owns:** impeccable

**Status:** ready-for-agent

- [ ] Scenario: new user adds a first config → Action: complete the Add form without touching raw JSON → Expected: protocol scaffolds the fields, validation runs inline, invalid input keeps the dialog open with the error. Verification: automated test. Priority: required.
- [ ] Scenario: non-loopback bind entered → Action: attempt Start → Expected: explicit confirm naming the config and bind before anything starts, plus the post-start warning. Verification: automated test. Priority: required.
- [ ] Scenario: export with secrets → Action: open Export → Expected: the verbatim plaintext-secret warning with a visible safer path, and cancel writes nothing. Verification: stakeholder acceptance. Priority: required.
- [ ] Prohibited side effect: import must never start endpoints; validation must never close over a failure.

Retry cap: 2 attempts, then escalate to the user.
