# Contract — Local endpoints

## Behavior

- Each running config owns exactly one ready endpoint; traffic arriving on endpoint E exits only via config E's remote (isolation).
- Endpoint serves SOCKS5 and HTTP on the same port; bind defaults to loopback.
- Non-loopback bind requires explicit confirm; UI shows a LAN-exposure warning.
- Unmatched/internal traffic resolves to direct-without-proxy; engines never rewrite OS proxy, DNS, or routes.
- Stop/failed closes the port promptly; start with a taken port fails with the conflicting holder named.

## Verification

- Loopback HTTP + SOCKS requests per endpoint return the egress of their own config (distinct-egress check with 2+ configs).
- OS proxy/DNS/route snapshot identical before and after a session.
