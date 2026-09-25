# 19: Library list renders stored rows + complete row UI

**What to build:** The middle list provably paints what the store holds. Root-cause and fix the blank-list defect (live evidence: status bar reads "2 configurations · 0 enabled" while the list paints neither rows nor the empty state). Prime suspect from code reading: `LibraryDelegate::set_rows` refreshes `rows`/`visible` but nothing notifies the owning `ListState`, so its `rows_cache` stays as first prepared (empty at startup) — the virtual list then paints nothing while `items_count > 0` also suppresses `render_empty`. Verify the true cause in code; fix it at the seam (notify / refresh the rows cache on every `set_rows`, including startup, reload, import commit, search, and health updates).

**Blocked by:** 18 (Dialog footers).

**Owns:** gpui-kit

**Status:** done (commit b0952fe; `cargo test` 249 passed, verified; root causes: stale rows-cache notify + zero-height list container)

- [ ] Scenario: store with 2 configs, fresh window → Action: open shell → Expected: 2 painted rows with stable ids (`library-row-{id}`), findable in a headless frame. Verification: automated test. Priority: required.
- [ ] Scenario: empty store → Action: open shell → Expected: "No proxy configurations yet" text actually painted (assert via bounds/snapshot, not just element presence). Verification: automated test. Priority: required.
- [ ] Scenario: add/import/remove/search/health tick → Action: each mutation → Expected: painted rows match store contents with no restart, no stale rows, selection preserved by id where possible. Verification: automated test. Priority: required.
- [ ] Scenario: complete row content → Action: view any row → Expected: name, protocol, host:port, bind endpoint, enabled state, health summary all visible; selected row drives a complete detail pane (endpoint URLs + actions). Verification: manual UX review. Priority: important.
- [ ] Prohibited side effect: no full-list rebuild per keystroke (filter stays incremental); no entity-per-row; no behavior change to store/parsers/engine.
