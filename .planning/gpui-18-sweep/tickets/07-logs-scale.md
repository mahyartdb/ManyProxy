# 07: Logs scale and stay redacted

**What to build:** Log lines in a virtualized/searchable list with single `RadioGroup` level, `Empty`/`Spinner` states, stable-width Pause/Resume, redacted copy/save with overwrite gate, and commit-vs-discard footer verbs.

**Blocked by:** 06-search-facets-tabs (shares RadioGroup + Empty + small language; reuses T03 group pattern).

**Owns:** gpui-kit composition.

**Status:** ready-for-agent

- [ ] Scenario: 10k-line tail seeded → Action: filter + change level + toggle tail → Expected: list stays virtualized/responsive, filter applies without reopen jank, level is the single grouped choice, no-lines shows `Empty`, tail-reopen shows `Spinner`, Pause/Resume keeps stable width. Verification: automated test (virtual list ids + filter/level interaction + Empty/loading states; perf by construction, no sleep-based asserts). Priority: required.
- [ ] Scenario: copy/save logs → Action: press Copy/Save → Expected: content redacted (uuid/password/token forms), visible redact note, Save overwrite gate or picker + `MANYPROXY_LOGS` disclosure, footer distinguishes committing filter change from discarding. Verification: automated test (redaction cross-test vs `health.rs`/`import` redactors; save-gate test). Priority: required.
- [ ] Prohibited side effect: no secret material in clipboard/files/logs; no unvirtualized 10k `flex_col`; no second level control.

Retry cap: 2, then escalate.
