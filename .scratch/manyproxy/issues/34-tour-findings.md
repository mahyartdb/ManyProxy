# 34: Tour findings — log ANSI + About hash freshness

**What to build:** live tour (2026-09-26, computer-use) proved everything else; two blemishes remain.
**Owns:** src/main.rs log render + build.rs (no schema, no behavior change).

- [ ] Scenario: sidecar stderr with ANSI colors → Action: open Logs → Expected: plain readable text, no `[31m`/`[38;5;` escapes (reuse engine `strip_ansi_codes` at render; stored records untouched). Verification: automated test. Priority: required.
- [ ] Scenario: commit after build → Action: rebuild + open About → Expected: hash names the current HEAD (build.rs follows `.git/HEAD` to the ref file for `rerun-if-changed`). Verification: manual About screenshot. Priority: important.
- [ ] Prohibited: no change to stored log records; build never fails when git is absent.

**Status:** done (verified 409 passed / 0 failed, fmt+clippy clean)
