# 09: Hardening + release proof

**What to build:** The release gate: quickstart scenarios A–D green, OS-mutation proof, LAN-bind warning, secrets audit, packaging/AV/quarantine notes. Nothing new — proves the system is shippable.

**Blocked by:** 04 (Subscriptions + refresh), 06 (Health + logs + stats), 07 (Fork-core exotic), 08 (Xray sidecar + custom + chaining).

**Owns:** security-and-hardening

**Status:** done (commit 760bb14; `cargo test` 160 passed, verified; live halves need real remotes per QUICKSTART-RUN.md)

- [ ] Scenario: quickstart A (3 egresses) → Action: run end-to-end → Expected: distinct egresses per endpoint, OS snapshot unchanged. Verification: operational check. Priority: required.
- [ ] Scenario: quickstart B/C/D → Action: run conflict, subscription-preservation, exotic+custom paths → Expected: all expectations in quickstart.md hold. Verification: operational check. Priority: required.
- [ ] Scenario: secrets audit → Action: export + logs + clipboard review → Expected: no plaintext private keys/tokens; export carries an explicit warning. Verification: security review. Priority: required.
- [ ] Scenario: non-loopback bind → Action: set LAN IP → Expected: explicit LAN-exposure warning with confirm required. Verification: security review. Priority: important.
- [ ] Prohibited side effect: no telemetry, no auto-update that replaces binaries silently, no admin requirement introduced.
