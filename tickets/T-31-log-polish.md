# T-31 — Log polish: level→color spans + `log_max_lines` meta

- **id**: T-31
- **title**: Log view polish — per-level color spans + configurable `log_max_lines` (default 5000) replacing the fixed ring constant
- **epic**: E6 — UI affordance pack
- **size**: S
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E6 T-31 line; gap §4.3 U10 (Throne regex highlighting + cap-accessor settable, §B.1/§F.2 "fixed 5,000-record ring (`health.rs:82`) vs Throne's cap-200 settable"); verified: `const MAX_LOG_RECORDS: usize = 5_000` (health.rs:82) inside `LogStore::push` cap logic (health.rs:2098–2127 with crash-cause-line eviction protection), `log_lines` view state (`LogLinesDelegate`, main.rs:1067), filters block `apply_log_filters` (main.rs:15765), `LogLevel` parse (health.rs:1980).

## Why

10,000-line core dumps screech through a 5k ring — and level scannability paints the story: errors red at a glance, warnings amber, debug muted. Cap configurability matches Throne's editable retention, and the crash-cause protection already built into `push` (eviction keeps crash lines; health.rs:2120–2127) must stay intact at ANY cap.

## What

1. **Color spans** (`LogLinesDelegate` render, main.rs:1067): lines parse to `LogLevel` already at ingest (`health.rs:2107 parse_level`); render the level segment (and thus the whole line's tone) per level: Error → theme error color, Warn → amber, Info → default, Debug → muted. Dark/light theme both verified legible (theme tokens from gpui-kit `Theme`).
   - Redaction/ANSI-strip unchanged (upstream `redact_secrets` + `strip_ansi_codes` at health.rs:2105–2106 untouched).
2. **`log_max_lines` meta**: new `META` const (store.rs family, store.rs:6777–6982) + Settings UI field (Maintenance/Engine section adopter's choice — Logs dialog header fits; place near "Save logs"; label `Maximum kept log lines`), clamp 500..50_000 (default/absent = 5000 = today's shape).
   - `LogStore` gains `pub fn with_capacity(cap: usize) -> LogStore` (default stays `MAX_LOG_RECORDS`); `LogStore::push` uses the instance cap instead of the const (the const remains the default constant).
   - Appliy: `AppView` constructs `LogStore::default()` today (main.rs:11374); hydrate cap from meta at `with_store` and on Settings save (recreate the store's ring is acceptable: recreate with cap = new value, preserving nothing across the change beyond what's loaded — document that changing cap mid-session truncates for the fresh ring).
   - Crash-cause protection: the eviction picker (`is_crash_cause_line`) works at any cap (tests).
3. Filter/tail behaviors (`apply_log_filters` main.rs:15765) byte-identical (they operate over present records only).

## Files to touch

- `src/health.rs` — `with_capacity` + instance-cap push + tests
- `src/main.rs` — delegate Level spans, Settings field + save-contact, tests

## Acceptance criteria

- [ ] Level tones render distinct for all four levels in both themes.
- [ ] Cap: 500-line env ross-drop path rings at 500; 50_000 cap respected; default stays 5,000; cap survives restart (meta round trip); crash-cause lines protected at every cap (pinned).
- [ ] Filter/exclude/regex/level/tail behaviors unchanged with any cap.
- [ ] Changing the cap mid-session rebuilds the ring to the new cap (documented truncation — no partial growth states).
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t81_`, verified unused): `t81_log_level_tones_render_all_levels`, `t81_log_caps_round_trip_reset_and_default`, `t81_crash_cause_lines_protected_at_any_cap`, `t81_filters_unaffected_by_cap`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData: metas file path behavior unchanged (`MANYPROXY_LOGS` untouched).
- [x] No-migration: additive meta; default equals today's cap.
- [x] Plaintext-always-sealed / **Redaction**: redaction-on-ingest untouched (color spans decorate the *already-redacted* text — no new display pipeline); session-only logs constraint preserved (this bid does NOT create persistent log files; the "Save logs" explicit path unchanged).
- [x] Hijack non-concept: N/A.
- [x] No system mutation / Sidecars: N/A.
- [x] Clipboard discipline: N/A.

## Out of scope

- Persistent diagnostic file logs / rotation (session-only stays the posture; the "Save logs" user action unchanged).
- Regex syntax highlighting of arbitrary patterns (Throne §B.1 nice-to-have; not adopted).
- Log volume toasts or log analytics.
