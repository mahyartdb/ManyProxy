# T-23 — `port=N:M` search grammar + last-filter memory

- **id**: T-23
- **title**: Search-filter grammar `port=N`, `port=N:M`, `port=N:`, `port=:M` + last-filter column memory (meta)
- **epic**: E6 — UI affordance pack
- **size**: S–M
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E6 T-23 line; gap §4.3 U3 (Throne §B.2 item 23 port filters; "in a port-farm app, 'show me rows serving 1080x' is a first-class need; `port=108` prefix-range would be even better but start with Throne's grammar"); verified: `apply_filter` (main.rs:798) substring match over `search_index` blob (`blob.matches_lowered(&lowered)` at 803) + `row_matches_facets` (824) + `active_filter_text` (850); `RowColumns::parse` meta pattern (main.rs:11320) for persistence.

## Why

Rows ARE ports here. "Which listener serves 1080x?" as a substring guess ("108") lured the old UX into hash-noise; the settled grammar is exact/partial **range** matching over the row's listen port, plus persisting the last filter column memory so sessions resume where they stopped.

## What

1. **Grammar into `apply_filter`** (main.rs:798): parse the query for `port=` tokens (case-insensitive key; comma-space separated multiple `port=` tokens OR'd… decision: **multiple port tokens AND'd with each other and the text remainder? Throne: single token in the Address column; keep it small — single `port=` predicate per query; document that a second `port=` token renders as literal substring in the text part). Rules:
   - `port=N` → exactly N;
   - `port=N:M` → inclusive range N..M;
   - `port=N:` → N..=65535; `port=:M` → 0..=M;
   - the remaining query text (outside the token) keeps today's substring behavior against the blob, so `port=10801 group: api` composes with facets as facet logic already AND separately (row_matches_facets chain).
   - predicate applied over `row.port` (u16) — the same data `save_listen_gate`/endpoint edits mutate.
2. **Status-bar filter text** (`active_filter_text`, main.rs:850): token renders as `port: N..M` line in the existing "query" phrasing (mechanical mapping of the parsed form, no new UI control).
3. **Last-filter memory (meta)**: persist the search **query string** (not facets — the facets are not per-column text) as meta `library_last_query`; restored at `with_store` (alongside `RowColumns::parse` hydration, main.rs:11320–11324) so the search input opens with last session's filter already applied. Store+load via the existing meta const family; a "clear search" button clears both — session restore is visible state, not hidden state.
4. No new components: the grammar is invisible until used; the search placeholder can mention `port=N:M` (one tooltip-level hint, keep the static placeholder short).

## Files to touch

- `src/main.rs` — parser + `apply_filter` predicate, `active_filter_text` mapping, search-input persistence (meta), tests
- `src/store.rs` — `META_LIBRARY_LAST_QUERY` const + getter/setter (meta family, store.rs:6777–6982) + tests

## Acceptance criteria

- [ ] `port=10801` matches only that port; `port=10801:10910` matches the inclusive range; open-ended forms work; malformed (`port=`, `port=x`, `port=5:1` reverse) renders as literal substring (no error state, no crash) — document in the ticket the graceful-fallback rule above.
- [ ] Composition with group/protocol/status facets ANDs correctly; multiple `port=` tokens degrade to text (pinned).
- [ ] Filter text shows the canonical `port:` form; sort/type toggle (`type_security_sort`, main.rs:812) unaffected.
- [ ] Last query restores on launch and persists through clean restarts; empty default never writes a new meta.
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t73_`, verified unused): `t73_port_filter_exact_and_ranges`, `t73_port_filter_open_ended_forms`, `t73_malformed_port_token_renders_literal`, `t73_port_composes_with_facets`, `t73_last_query_meta_restores_on_launch`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData / No-migration / Plaintext / Hijack / Sidecars: N/A.
- [x] No system mutation: pure filter logic; meta key additive.
- [x] Clipboard `bind:port` discipline: N/A.
- [x] Redaction: N/A.

## Out of scope

- `port=108` prefix-range shorthand (noted as plausible later — not adopted).
- Per-column typed filters beyond the port facet of Throne's design (Type/Address/Name ones not asked here).
- FacetPersistence changes (existing `library_columns`-adjacent fields untouched).
