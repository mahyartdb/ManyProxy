# 07: Import live feedback

**What to build:** Import dialog with live `X valid · Y invalid` preview as
text is typed or files picked, inline first-failure (item number, label,
reason), stay-open with full failure list on total failure, and honest
`Imported N · M duplicates skipped · K failed` toast on partial success.

**Blocked by:** 05 (Honest dialog footers — Import footer settled first).

**Owns:** rust-testing

**Status:** ready-for-agent

- [ ] Scenario: paste 2 valid + 1 invalid items → Action: preview updates →
  Expected: `2 valid · 1 invalid` with failure's item number, label, reason
  inline. Verification: automated test. Priority: required.
- [ ] Scenario: fully invalid paste → Action: press Import → Expected: dialog
  stays open listing every failure, nothing added. Verification: automated test. Priority: required.
- [ ] Scenario: partial paste → Action: import completes → Expected: toast
  reports imported/skipped/failed counts; new rows arrive selected and
  checked. Verification: automated test. Priority: important.
- [ ] Prohibited side effect: import-grammar acceptance changed; full file
  paths leaked into the files line (basenames only); silent library-export
  restore path bypassed.

Retry cap: 2, then escalate to the user.
