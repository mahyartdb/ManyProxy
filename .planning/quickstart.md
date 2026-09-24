# ManyProxy — Quickstart (validation guide, no code)

## Prerequisites

- Built desktop app, 3 test proxy configs on distinct egresses, loopback networking allowed, a port range free from 10801.

## Scenario A — Three distinct egresses, no system change

1. Snapshot OS proxy/DNS/routes.
2. Import 3 configs (paste links), start all.
3. Request a public-IP reflector through each endpoint's SOCKS5 and HTTP forms.
4. Expected: 3 distinct egress IPs, each tied to its config; OS snapshot unchanged after stop-all.

## Scenario B — Custom bind + conflict

1. Set config A to loopback port 10821, start; request through it succeeds.
2. Set config B to the same port, start.
3. Expected: B fails with a port-in-use error naming config A; A unaffected.

## Scenario C — Subscription refresh preserves edits

1. Add subscription, refresh, rename one member.
2. Refresh again.
3. Expected: members updated, no duplicates, renamed member keeps its name.

## Scenario D — Exotic + custom

1. Add one AmneziaWG-variant, one Xray-only VLESS, one custom-JSON config; start all.
2. Expected: each endpoint serves its own egress; broken JSON blocks start naming the field.
