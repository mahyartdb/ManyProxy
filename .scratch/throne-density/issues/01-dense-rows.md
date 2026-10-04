# 01: Dense List rows

**What to build:** library rows render as compact 2-line `ListItem`s (header: checkbox + name + protocol + state dot; meta: upstream + bind:port + health + traffic) instead of 4-5 line cards, so 50 rows scan without opening rows.

**Blocked by:** None (can start immediately).

**Owns:** gpui-kit-design-guides (max 1-2 per ticket).

**Status:** ready-for-agent

- [ ] Scenario: seeded 50 stored / 20 running at 1000x700 → Action: open library → Expected: >=3 rows painted, each row shows name, protocol, upstream, bind:port, running state, latency-or-named-failure, usage on the row surface. Verification: automated test. Priority: required.
- [ ] Scenario: selection on row id X with filter narrowing → Action: apply filter → Expected: selection stays on X if still visible else clears, never lands on wrong row. Verification: automated test. Priority: required.
- [ ] Prohibited side effect: stable ids `library-row-{id}`, `check-{id}`, `row-dot-{id}` renamed or removed; secrets (keys/tokens/passwords) rendered in any cell.
