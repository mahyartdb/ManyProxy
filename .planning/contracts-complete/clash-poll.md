# Contract: clash poll (concurrency + staleness)

- Endpoint: `GET http://127.0.0.1:<ctrl>/connections` + `Authorization: Bearer <secret>`; parse `downloadTotal`/`uploadTotal` (u64); per-row delta = max(0, cur − last); probe flows excluded via normalized `is_probe_destination` (lowercase, strip port/trailing dot, match hostname + resolved IPs).
- Concurrency: cores polled concurrently off-UI (`background_executor`), per-core timeout 1–2s, total deadline per tick; in-flight tick skipped, never queued.
- Secrets: 128-bit `getrandom` per start, in-memory only, `Zeroizing` wrapper, cleared on stop; never persisted, never in errors/logs.
- Staleness: per-core last-success timestamp; UI marks totals stale after N missed polls (never zeroes silently).
- Persistence: one tx per tick via batched `add_traffic_batch`; saturating i64 math; errors logged.
