# 10: Status strip and trust wording

**What to build:** Status strip with fixed slots (counts, engine/health,
usage) that never shift layout, bulk/destructive actions confirming by
naming rows with per-item outcomes, and copy/redaction discipline verified
end to end.

**Blocked by:** 04 (Detail pane fit — shared status/detail text functions),
05 (Honest dialog footers — shared confirm-description wording).

**Owns:** rust-testing

**Status:** ready-for-agent

- [ ] Scenario: values change (ports serve, traffic accrues) → Action: render
  status strip → Expected: fixed slots, no layout shift. Verification: manual UX review. Priority: important.
- [ ] Scenario: bulk delete of checked rows incl. running ones → Action:
  confirm → Expected: confirm names the count and stop-first promise; toast
  reports per-row outcomes. Verification: automated test. Priority: required.
- [ ] Scenario: any copy action → Action: paste clipboard → Expected: bind +
  port only, no credential material. Verification: automated test. Priority: required.
- [ ] Prohibited side effect: telemetry beyond user-initiated fetches/probes;
  secrets in logs/URLs/labels; nonzero usage shown when unreadable.

Retry cap: 2, then escalate to the user.
