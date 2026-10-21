# T-20 — Diff report text (named `[+]/[~]/[-]` lists)

- **id**: T-20
- **title**: Diff report in manual refresh toasts — Added/Updated/Deleted **named** lists with `[+] [~] [-]` lines + "Still in use, so kept:" (running rows)
- **epic**: E5 — Subscription & exchange pack
- **size**: S–M
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E5 T-20 line ("Data already computed by the diff algorithm — display-only"); gap §4.4 S1 (Throne §I.2 report format `Added %1 profiles:\n%2…` + `Still in use, so kept:`; ManyProxy "counts only: `added/updated/tombstones`") — **with a correction found during verification**: ManyProxy is *not* counts-only — `refresh_summary_text` (main.rs:8687) already renders `[+]Added N / [~]Updated N / [-]Deleted N / [=] Still in use N` with counts; the missing piece is the **named** identity lists Throne shows. `RefreshOutcome` (store.rs:2934 — `added/updated/removed/duplicates/skipped/conflicts/in_use` counts + `failures: Vec<ImportFailure>`), diff algorithmic engine = `refresh_with_body_running` (store.rs:5207), batch aggregation state exists (`refresh_batch_*` fields, main.rs:11393–11400).

## Why

"4 updated" answers the accountant; "which 4" answers the actual question — was my node renamed, swapped, or removed? The diff algorithm already computes every adopt/tombstone identity decision inside `refresh_with_body_running`; only the display layer narrows it to counts. This is one store-field extension + one render extension, display-only.

## What

1. **Payload extension** (`RefreshOutcome`, store.rs:2934): add named-list fields — `added_names: Vec<String>`, `updated_names: Vec<String>`, `removed_names: Vec<String>`, `kept_in_use_names: Vec<String>` (labels collected exactly where each count increments inside `refresh_with_body_running`; no new diff logic, no second pass). Batch refreshes aggregate the lists on the `refresh_batch_*` fields in main.rs (parallel to the existing count aggregators at 11393–11400).
2. **Render** (`refresh_summary_text`, main.rs:8687 — extend, don't replace): keep the count line; when the named lists are non-empty (manual refreshes only, per brief), append a compact report block in the same toast notification:
   ```
   Added 3 profiles:
   [+] alpha
   [+] beta
   [+] gamma
   Updated 2 profiles:
   [~] node-x
   [~] node-y
   Deleted 1 profile:
   [-] old-node
   Still in use, so kept:
   [=] ws-relay
   ```
   (ManyProxy's own header wording, Throne's structural pattern — the brief pins `[+] [~] [-]` line markers and the "Still in use, so kept:" line verbatim.)
3. **Streaming cap**: toasts truncate — cap each list at 12 names rendered + "+ N more…" line (toast min-width/truncation utilities exist for the status bar; the toast uses the same Label constraints). Data beyond the cap still exists for dialogs (optional: the Subscriptions dialog can render the full named report — implementer may reuse the same renderer cheaply; not required).
4. **Manual refreshes only** (brief: "manual refreshes: named report", auto updates keep the count summary — due-tick refreshes (main.rs `tick_due_subscriptions` 19976) render today's count text unchanged; also acceptable to render named there too only if the implementer keeps the toast economics — default: manual-only, matching brief).

## Files to touch

- `src/store.rs` — `RefreshOutcome` fields + collection points in `refresh_with_body_running` + tests
- `src/main.rs` — `refresh_summary_text` extension + batch aggregation fields + render tests

## Acceptance criteria

- [ ] Counts unchanged in every scenario (existing refresh tests pass); the named lists match the counts exactly (property: `added_names.len() == added` etc. — asserted in the store tests).
- [ ] Report block renders per the format above with `[+] [~] [-]` markers and the "Still in use, so kept:" line; 12-name cap + "+ N more…" overflow marker.
- [ ] Auto (due-tick) refreshes keep the old count-only line.
- [ ] In-use semantics: running rows kept appear in the kept list — not deleted (existing keep-running logic untouched).
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t70_`, verified unused): `t70_outcome_named_lists_match_counts`, `t70_named_report_renders_markers_and_kept_line`, `t70_report_caps_at_twelve_names_with_more_line`, `t70_auto_refresh_keeps_count_only_text`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData / No-migration / Plaintext / Hijack / Sidecars: N/A.
- [x] No system mutation: display-only.
- [x] Clipboard `bind:port` discipline: N/A (report lives in a toast, not clipboard).
- [x] Redaction: **labels only** — every list entry is a row label (already non-secret by design); never params/url fragments.
- [x] Cancel-writes-nothing and dry-run restore flows untouched.

## Out of scope

- Full refresh geometry UI redesign or a dedicated diff-dialog.
- Tombstone-management UI changes (exists at store level).
- Cross-subscription diff views.
