---
name: sqlite
description: Embedded SQLite guidance (WAL mode, PRAGMAs, pooling, migrations, bulk loads). Use when working with SQLite, libsql, Turso, better-sqlite3, or Durable Objects SQLite outside Cloudflare. Do NOT use for Postgres (use supabase-postgres-best-practices), Cloudflare DO storage (use durable-objects), or ORM migrations (use database-migrations plus the framework skill: fastapi-patterns, django-patterns, kotlin-exposed-patterns, rust-database).
---

# SQLite

Single-file database. Zero config, one writer at a time, copy the file to back it up.

## Setup (every connection)

```sql
PRAGMA journal_mode = WAL;
PRAGMA busy_timeout = 5000;
PRAGMA foreign_keys = ON;
PRAGMA synchronous = NORMAL;
PRAGMA temp_store = MEMORY;
```

WAL + busy_timeout is what makes concurrent readers and a writer coexist. Without them you get locking errors under any real load.

## Rules

1. **One writer**: serialize writes (single connection, queue, or `BEGIN IMMEDIATE`). Readers never block in WAL mode.
2. **Parameterize everything**: never interpolate values into SQL. `?` placeholders always.
3. **STRICT tables** for new schemas (`CREATE TABLE t (...) STRICT`), explicit types, `NOT NULL` + defaults where the domain allows.
4. **Bulk loads in one transaction**: wrap multi-row inserts in `BEGIN`/`COMMIT`; use prepared statements in a loop. 10-100x faster than autocommit per row.
5. **Migrations expand/contract**: `ADD COLUMN` is instant and non-destructive — prefer it. Renames/removals go add-new, backfill, switch readers, drop-old across deploys, with a tested down path.
6. **Indexes for the query shape**: foreign keys, WHERE predicates, and ORDER BY columns get indexes; verify with `EXPLAIN QUERY PLAN` before assuming.
7. **Test in memory**: `:memory:` databases for unit tests; file-backed fixtures only for migration tests.

## Drivers

- better-sqlite3 (sync, Node), libsql/Turso (edge + embedded replicas), `sqlite3` CLI for inspection (`.schema`, `.dump`, `.mode`).
- rusqlite (Rust, sync + bundled) and Python `sqlite3` (stdlib, check `sqlite3.sqlite_version`): same SQL rules; serialize writes in both.
- Durable Objects `ctx.storage.sql` follows the same SQL rules; DO lifecycle (alarms, RPC) belongs to `durable-objects`.

## Full-text search (FTS5)

```sql
CREATE VIRTUAL TABLE docs_fts USING fts5(title, body);
INSERT INTO docs_fts(title, body) VALUES (?, ?);
SELECT rowid FROM docs_fts WHERE docs_fts MATCH ? ORDER BY rank;
```

External-content tables (`content=`, `content_rowid=`) keep the index separate from source rows; rebuild with `INSERT INTO docs_fts(docs_fts) VALUES('rebuild')`.

## Maintenance

- `PRAGMA optimize;` after schema changes and periodically on close; `ANALYZE` once the dataset is representative.
- Backups: `VACUUM INTO 'backup.db'` for online copies, or the `.backup` dot-command offline. Never copy the WAL file by hand.
- Triggers: enforce cross-table invariants SQLite can't express as FKs (audit rows, denormalized counters); keep them small and tested.

## Locking pitfalls

- Network file systems (notably Windows SMB shares) break SQLite locking — keep the DB on a local disk or use a client/server engine instead.
- Multi-process writers queue on `busy_timeout`; sustained write contention means a server DB (Postgres) instead of a longer timeout.

## Done when

- [ ] WAL + timeouts set on every connection, writes serialized, inputs parameterized
- [ ] Migration has a down path; `EXPLAIN QUERY PLAN` checked for new queries

## Related

- Skills: `durable-objects` (DO SQLite lifecycle: alarms, RPC), `database-migrations` (expand/contract versioning)
