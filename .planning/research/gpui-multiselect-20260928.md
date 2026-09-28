# Research 06 — gpui-kit 0.6 List multi-select + facet/filter patterns, background executor

**Decision this informs:** multi-select library list with bulk bar, group/status/protocol facets, keyboard handling, search perf, and moving blocking engine/clash work off the UI thread.
**Confidence:** high for kit API facts (verified against vendored sources); medium for facet/bulk UX composition (kit provides primitives, composition is app design).
**Repo state:** `src/main.rs` `LibraryDelegate` is single-select (`selected: Option<IndexPath>`), search is `library_row_matches` per-keystroke, ticks run blocking work on the UI thread.

## TL;DR

1. `ListState` / `ListDelegate` / `TableState` are all **single-select only** (`Option<IndexPath>` / `Option<usize>`). Multi-select must be app-modeled as an id-keyed `HashSet` in the delegate. Only `ComboboxState` has native multi (`.multiple(true)`), and it is a dropdown, not a list.
2. Bulk bar has no kit component: render a conditional toolbar row when the set is non-empty, reusing existing commit paths + the `is_row_local_start_error` partial-failure copy pattern.
3. Facets compose as delegate predicate fields ANDed in `apply_filter` (`visible: Vec<usize>` unchanged); facet controls are `Select` (single), `Combobox multiple` (multi-value), `Checkbox` (booleans), `TabBar` segmented (status). Group headers can alternatively use `sections_count` / `render_section_header`.
4. Keyboard: `List` hardcodes key context `"List"` with `escape`/`enter`/`up`/`down`. App keys follow the existing `actions!` + `bind_app_keys` + `.key_context(APP_KEY_CONTEXT)` + `.on_action` + `has_active_dialog` guard pattern. Do NOT rebind `Enter` at app level — consume `ListEvent::Confirm` instead. `Delete` and `Ctrl-F` are new app actions.
5. Search perf: precompute one lowercase blob per row in `set_rows` (kills per-keystroke `serde_json` parse + N×`to_lowercase`). `perform_search` stays synchronous + `Task::ready(())`; `RowsCache::prepare_if_needed` only rebuilds on section/size change so filter-via-`items_count` is already cheap.
6. Background executor: the repo already has the pattern (`spawn_probe`, `commit_import_dialog`): `cx.spawn` + `cx.background_executor().spawn(blocking).await` + `cx.update`/`handle.update` + `.detach()`, weak downgrade, in-flight guard flag, fail-closed. Apply it to engine apply (`Command::output`, check+spawn) and clash polling (`TcpStream::connect_timeout`, 5s/core sequential on the UI thread today).

## 1. ListState multi-selection support: NO (verified)

Vendored source `gpui-component-0.6.6/src/list/list.rs`:

- `ListState` holds `selected_index: Option<IndexPath>` — one anchor, no set. `_set_selected_index` / `set_selected_index` / `select_item` all take `Option<IndexPath>`.
- `ListDelegate::set_selected_index(ix: Option<IndexPath>, …)` — singular by signature (`delegate.rs:105`).
- `ListEvent` is `Select(IndexPath)` / `Confirm(IndexPath)` / `Cancel` — singular.
- `RowsCache::prev`/`next` navigate a single cursor.
- `TableState` (`table/state.rs:240-245`) is likewise `selected_row: Option<usize>` (+ col/cell) — single-select only.
- Only multi-select in the kit: `ComboboxState::multiple(true)` (`combobox.rs:286` — "clicking an item toggles it in the selection and the popover stays open"), a dropdown primitive, not a list.

### Recommended model (app-level, id-keyed)

```rust
struct LibraryDelegate {
    rows: Vec<ProxyConfig>,
    visible: Vec<usize>,
    query: String,
    selected: Option<IndexPath>,       // existing single anchor (keyboard/Enter/detail)
    checked: HashSet<String>,          // NEW: bulk set, keyed by config id
    // ... health, running
}
```

- Key by `config.id`, never by row index — same rule as the existing `set_rows`/`apply_filter` selection re-derivation (`main.rs:219-252`). Prune `checked` against live ids in `set_rows`; keep members filtered-out by query/facets (bulk acts on the set, count line says "N selected (M visible)" so hidden membership is never a surprise).
- Render per-row affordance via `ListItem::confirmed(bool)` + `check_icon`, or a leading `Checkbox::new(id).checked(member).on_click(...)` child (`checkbox.rs`: `new`/`checked`/`on_click`/`on_change`). Anchor-follows-click is acceptable: the row `div` in `render_list_item` (`list.rs`) owns `on_click` → select+confirm, so an inner checkbox click will bubble and move the anchor too. Toggle still lands; `Enter`/detail keep working on the anchor.
- Open verification item: confirm bubbling behavior with a spike + `VisualTestContext` click test before committing to inner-checkbox vs Space-toggles-anchor vs dedicated toggle column. Do not claim propagation semantics without the spike.

## 2. Bulk bar pattern (no kit component — app composition)

- Conditional `div` row between toolbar and list, rendered only when `!checked.is_empty()`: `"N selected"` + `Start` / `Stop` / `Test` / `Delete` / `Clear` buttons reusing the existing single-row commit fns in a loop.
- Partial-failure copy already exists: `start_all_partial_message` (`main.rs:1032`) + `is_row_local_start_error` (`main.rs:1070`) — row-local errors (bad params, unsupported protocol, chain violations) fail per-row; union-level failures (port conflicts, missing binaries, LAN gate) stay atomic. Reuse both verbatim for bulk start.
- Delete reuses the detach-then-delete confirm path (`open_preset_detach_delete_confirm`, `main.rs:4908`); bulk delete names all affected rows, never a bare count.
- Clear = `checked.clear()` + notify; Esc clears set only when no dialog is open (see §4).

## 3. Facet/filter composition (group / status / protocol)

Delegate holds one field per facet, ANDed in `apply_filter`; `visible: Vec<usize>` pipeline is unchanged:

```rust
struct LibraryDelegate {
    group_filter: Option<String>,      // None = All
    protocol_filter: Option<String>,   // None = All
    status_filter: StatusFacet,        // All | Running | Stopped (+ Enabled/Disabled optional)
    // query stays as-is; all four predicates AND
}
```

- Facet value lists derive from `rows` (distinct groups/protocols); "All" is always `None`, never a magic string.
- Selection re-derivation by id already survives narrowing (`apply_filter`, `main.rs:231`); extend the same treatment to `checked` (prune dead ids, keep filtered-out members, show visible count).
- Facet controls (all retained entities created once outside `render`, stored on owner — kit rule from research 04):
  - Protocol (single): `Select` — `SelectState::new(delegate, selected_index, window, cx)` (`select.rs:149`).
  - Group (multi-value): `Combobox` with `.multiple(true)` (`combobox.rs:144` ctor takes `selected_indices: Vec<IndexPath>`; `selected_values()` reads the set).
  - Status/running booleans: `Checkbox` (`checkbox.rs`) — checked = facet on.
  - Status alternative: `TabBar` segmented/pill/underline variants (`tab/tab_bar.rs`: `segmented()`, `selected_index()`, `on_click`) for All/Running/Stopped.
- Optional group headers instead of / alongside a group facet: `sections_count` = number of groups, `items_count(section)` per group, `render_section_header` per group (`delegate.rs`). Constraint: uniform item height and uniform header height (measurement in `prepare_items_if_needed` measures one item + one header). Sections and facets compose (facet narrows sections; empty sections are skipped by `RowsCache`).

## 4. Keyboard: Delete / Ctrl-F / Enter

Kit facts (`list.rs:init`, `gpui-base-0.6.6/src/actions.rs`):

- `List` binds `escape`→`Cancel`, `enter`→`Confirm{secondary:false}`, `secondary-enter`→`Confirm{secondary:true}`, `up`→`SelectUp`, `down`→`SelectDown` under the hardcoded key context `"List"`.
- Shared actions available: `Cancel, SelectUp/Down/Left/Right, SelectFirst/Last, SelectPrevColumn/NextColumn, SelectPageUp/Down, Confirm{secondary}`.

App pattern (already in `main.rs:56-82`, `6118-6157`):

```rust
actions!(manyproxy, [DeleteSelected, FocusSearch, ClearSelection /* + existing five */]);

cx.bind_keys([
    KeyBinding::new("delete", DeleteSelected, Some(APP_KEY_CONTEXT)),
    KeyBinding::new("ctrl-f", FocusSearch, Some(APP_KEY_CONTEXT)),
    // NO enter binding here — List owns Enter under "List" context.
]);
```

- Shell `div`: `.key_context(APP_KEY_CONTEXT)` + `.on_action(...)` per action, each handler returning early when `window.has_active_dialog(cx)` (dialog-input protection, ticket 26 rule). Modifier-only chords for text-safe keys; `delete` is non-printable so list search typing is unaffected.
- `Delete` → bulk-aware delete: non-empty `checked` set → bulk detach-delete confirm; else anchor row → existing single delete confirm; neither → `no_selection_message()` toast.
- `Ctrl-F` → `libraries.update(cx, |list, cx| list.focus(window, cx))` — `ListState::focus` focuses the search input when `searchable(true)` (already set at `main.rs:1808`), so no new focus plumbing.
- `Enter` → do NOT add an app-level binding (double-fire with `List`'s `Confirm` + focus-context overlap). Consume the existing `_libraries_subscription` on `ListEvent::Confirm(ix)` (`main.rs:1842`) — Enter already routes anchor → `confirm()` → event. Bulk-Enter (start all checked) is a deliberate non-goal: Enter stays single-anchor; bulk acts only via bulk-bar buttons.
- `Esc` → `List`'s `Cancel` clears the anchor (`reset_on_cancel` default true); clear `checked` in the `ListEvent::Cancel` subscription arm, not via a second Esc binding.

## 5. Search blob caching for perf

Current cost (`library_row_matches`, `main.rs:1211` + `upstream_host_port`, `main.rs:1153`): per keystroke × per row = 5×`to_lowercase` on owned `String`s + full `serde_json::from_str::<Value>` parse of `params` + second parse inside `upstream_host_port` at render. Two parses per row per keystroke, plus identical work at render time.

Fix (app-level, no kit change — kit has no search index; `perform_search(query) -> Task<()>` is a dumb hook, currently `Task::ready(())` at `main.rs:318`):

```rust
struct LibraryDelegate {
    rows: Vec<ProxyConfig>,
    search_blobs: Vec<String>,  // NEW: lowercase "name protocol group bind port server" per row, built in set_rows
}
// apply_filter: blobs[i].contains(&q) — one contains per row, zero parses, zero allocs per keystroke
// render path: upstream_host_port stays (render-only, visible rows only)
```

- Build blobs in `set_rows` only (row data changes are rare; keystrokes are frequent). Lowercase once with `to_lowercase`; trim+lowercase the query once per `perform_search`.
- `upstream_host_port` parse stays for the visible-rows render path (bounded by viewport via virtualization) — do not hoist it into the filter loop.
- `RowsCache::prepare_if_needed` (`cache.rs`) already skips rebuilds when sections + measured size are unchanged, so filter-via-`items_count` is O(sections) per keystroke — the blob cache removes the only remaining per-keystroke linear cost.
- Debounce is available if needed: `start_search` (`list.rs`) already round-trips through `_search_task` + 100ms anti-flicker timer; the delegate can add its own debounce by returning a real `Task` from `perform_search`, but with blobs the sync path is fast enough that debounce is a fallback, not the plan.

## 6. Background executor pattern (engine apply + clash polling off UI thread)

### The established repo pattern (copy this)

`spawn_probe` (`main.rs:3224-3295`) and `commit_import_dialog` (`main.rs:3966-3992`):

```rust
self.op_in_flight = true;                       // 1. guard flag (second request toasts/ignored)
let weak = cx.entity().downgrade();             // 2. weak handle, never bare Entity capture
let owned_inputs = inputs.clone();              // 3. move OWNED data into the future (prober swapped via mem::replace)
cx.spawn(async move |_entity, cx| {              // 4. main-async wrapper
    let (outcomes, prober) = cx
        .background_executor()                   // 5. blocking work on the background executor
        .spawn(async move { blocking_work(owned_inputs) })
        .await;
    cx.update(|cx| {                             // 6. main-thread commit: restore state, clear flag, persist, reload
        weak.update(cx, |view, cx| view.finish_op(outcomes, cx));
        // window-captured delivery with headless fallback (import path):
        // handle.update(...) .is_ok() else weak.update headless commit
    });
})
.detach();                                      // 7. detach; loop exits / completion drops on released view (fail-closed)
```

Rules: blocking primitives (`TcpStream::connect_timeout`, `Command::output`, `test_all`/`ping_all`) never run inside `cx.update`/`weak.update` closures; `Box<dyn Trait + Send>` is moved (prober uses `mem::replace` with a default stand-in behind the guard flag); completion re-enters via `cx.update` with split borrows (record half, then toast half) to avoid update-while-updating.

### What to move (both block the UI thread today)

1. **Engine apply** (`commit_start_selected`, `main.rs:2327`; `start_all` variants): `EngineController::start_one_confirmed` → config render + `sing-box check` (`Command::output`, `engine.rs:4344`) + child spawn + bounded reaps (`wait_child_bounded`, 1-3s, `engine.rs:4058`). Runs synchronously inside the click/F5 handler. Move render+check+spawn into `background_executor().spawn`, keep the LAN-confirm gate + `start_in_flight` guard + toast/commit on the main thread. Reuse `start_busy_message()` for the second-click toast.
2. **Clash traffic poll** (`tick_traffic_poll`, `main.rs:5770`): `poll_clash_entries(port, secret, 5s)` per serving core, sequential loop — up to N×5s of `TcpStream::connect_timeout` inside `spawn_tick`'s `tick(view, cx)`, which executes inside `cx.update` on the main thread (`main.rs:1685`). Move the per-core poll loop into one `background_executor().spawn`, commit deltas (`add_traffic`, `traffic_last_seen`, `reload_quiet`) in `cx.update`. Same treatment applies to `tick_engine_poll` if `poll()`/`poll_shards` ever gains blocking calls (currently `try_wait`-based, cheap — leave it, note the asymmetry).
3. **Subscription refresh fetch** already follows the split (fetch off-thread, `apply` on main) — keep as the third instance of the pattern, not a new design.

`spawn_tick` itself stays (1s-slice sleep + stop/release check, `main.rs:1665`) — only the tick *body* moves; the tick still fires on schedule and no-ops when `ticks_enabled` is false.

## Sources

Vendored (workspace-pinned, versions from `Cargo.lock` / registry dir listing):

- `gpui-kit 0.6.6` re-export surface: `~/.cargo/registry/src/*/gpui-kit-0.6.6/src/lib.rs`, `test.rs`
- `gpui-component 0.6.6`: `src/list/list.rs` (ListState, key bindings, search task, render/measure), `src/list/delegate.rs` (ListDelegate trait), `src/list/list_item.rs` (ListItem selected/confirmed/check_icon), `src/list/cache.rs` (RowsCache, prepare_if_needed, prev/next), `src/combobox.rs:144,286` (multi), `src/select.rs:149` (single facet), `src/checkbox.rs`, `src/tab/tab_bar.rs` (segmented/pill), `src/table/state.rs:36,240` (single-select), `src/searchable_list/` (adapter/delegate/state/vec)
- `gpui-base 0.6.6`: `src/actions.rs` (Confirm/Cancel/Select*), `src/index_path.rs` (IndexPath::new/section/eq_row)
- App: `E:\Code\ManyProxy\src\main.rs` (§1 delegate ~188-404, actions ~52-82, spawn_tick ~1665-1699, spawn_probe ~3224-3295, import commit ~3940-4027, traffic poll ~5770-5846, shell render ~6115+), `src/engine.rs` (check/spawn/reap, clash_targets), `src/health.rs` (blocking dials, poll_clash_entries), prior brief `.planning/research/04-gpui-arch.md` (constraints 4, 6, 7, 13, 14)

Negative results (searched, absent): no `multi`/`checked_set`/`selected_indices` in `list/` or `table/state.rs`; no bulk/bar component in kit catalog; no search-index/blob helper in kit; no `spawn_blocking`/`block_in_place` in app (pattern is exclusively `background_executor().spawn`).

## Gaps / open spikes

1. Inner-checkbox click bubbling vs List row `on_click` (select+confirm) — spike + `VisualTestContext` click test (§1).
2. `delete` / `ctrl-f` keystroke string acceptance by `KeyBinding::new` on all targets — trivial compile check, but verify rather than assume.
3. Whether `Combobox multiple` trigger rendering fits a facet bar (chip/count summary) or needs a custom `render_trigger` — read `combobox.rs` trigger API during implementation.
4. Clash-poll move: per-core parallelism (one task per core vs one task for the loop) — decide at implementation; sequential-preserve first, parallelize only with a bound.
