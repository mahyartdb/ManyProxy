# Data Model delta: v4 → v5 (plan-complete.md)

Base: `.planning/data-model.md` + live schema v4 (`SCHEMA_VERSION = 4` in `src/store.rs`).

## v5 deltas (all online, idempotent, bump-last)

### subscriptions.interval_secs
- DDL: `ALTER TABLE subscriptions ADD COLUMN interval_secs INTEGER NOT NULL DEFAULT 86400;`
- Semantics: per-sub auto-refresh interval; missing/NULL → 86400 (`DEFAULT_AUTO_REFRESH_SECS`); `<= 0` → treated as due (matches `due_for_refresh`). No PK/UNIQUE/rebuild needed.
- UI: picker in SubForm + row display of `last_refresh_at` (already loaded, newly rendered) + global kill-switch + Refresh-all-due (derived action, no stored state).

### health_history (new, append-only)
- DDL: `id INTEGER PRIMARY KEY AUTOINCREMENT, config_id TEXT NOT NULL, at INTEGER NOT NULL, kind TEXT NOT NULL DEFAULT 'url-test', latency_ms INTEGER, error TEXT` STRICT + `CREATE INDEX idx_health_history_config_at ON health_history(config_id, at)`.
- Rule: `health_snapshots(config_id PK)` keeps latest-fast-path untouched; history appends per probe round; retention = time-based delete (`DELETE WHERE at < ?`, 30–90d) in one tx at startup + daily, never VACUUM.
- Validation: `latency_ms` null XOR `error` non-null; `kind` in {url-test, tcp-ping}.

### route_presets extension (additive only)
- New nullable columns via `ensure_column` (e.g. `description TEXT`, `is_builtin INTEGER NOT NULL DEFAULT 0`, `last_used_at INTEGER`); `name UNIQUE` + existing index kept; **no FK** on `proxy_configs.preset_id` (attach-time validation + orphan-sweep banner stand).
- Validation: `rules_json` parses to array; action in {route, direct, reject} served (`block`→`reject` at serve; `proxy` accepted as alias of `route` at intake); `warp` → guidance error.

### traffic_rx/tx (existing v4, batching rule change only)
- No DDL. Write rule: one `Transaction` + prepared `UPDATE … SET rx = min(rx+?, MAX)` loop + single commit per tick; deltas collected before tx opens; errors logged not swallowed. Drift-check loop extended with both columns.

### meta (existing, typed access)
- Keys: `restore_running`, `running_ids` (+ allow-list for future). Snapshot saves filter to known ids; write errors surfaced; missing vs corrupt distinguished in telemetry.

### In-memory only (never persisted)
- `LibraryDelegate.checked: HashSet<String>` keyed by config id; pruned to live ids in `set_rows`; hidden-by-filter members kept with `N selected (M visible)` count. Facet fields (`group_filter`, `protocol_filter`, `status_filter`) are view state ANDed in `apply_filter`; `visible: Vec<usize>` pipeline unchanged.

## State transitions (unchanged topology)

- ProxyConfig → LocalEndpoint 1:1 while running; crash sets shard Crashed, drops its `served[]`/clash target, ticks skip it, totals mark stale; Restart re-applies.
- Restore: temp sibling DB → validate → `quick_check` → atomic rename; backup `library.db.bak-<ts>` taken before any wipe; mid-kill leaves old DB intact.
- List reads: total functions returning `(rows, quarantined_ids)`; write paths stay strict.
