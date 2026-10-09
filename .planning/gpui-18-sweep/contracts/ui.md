# UI contract: gpui-18-sweep

Canonical owner for ids/strings/overlay/focus/secrets in this sweep. Tests assert against this; restyle freely, never rename/drop without same-ticket test updates + ledger rationale.

## Pinned ids (must paint)

`manyproxy-shell`, `manyproxy-toolbar`, `add-config`, `start-selected`, `stop-selected`, `test-selected`, `import-config`, `library-menu`, `config-list`, `library-row-<id>`, `row-card-<id>`, `row-dot-<id>`, `row-name-<id>`, `row-meta-<id>`, `row-check-chip-<id>`, `check-<id>`, `search-count`, `clear-search`, `facet-bar`, `facet-group/protocol/status`, `groups-dialog`, `clear-facets`, `bulk-bar`, `bulk-*` (8), `config-detail`, `inspector-tabs`, `detail-acting-on`, `detail-serving`, `config-endpoints/health/usage/history`, `endpoint-value-*`, `copy-upstream/socks/http`, `detail-operate`, `engine-start/stop`, `health-test/ping`, `row-menu`, `status-left/center/right`, `status-count`, `engine-status`, `health-status`, `traffic-status`, dialog footer `ok/cancel` ids per dialog, `export-warning-box`, `port-budget`, `log-lines`, `log-level-row`.

New ids (only if unavoidable): `preset-*` inline-detach (kept), log `RadioGroup` id, virtual-log list id, sheet ids for Logs/Subscriptions/Groups. Ledger each.

## Pinned strings (exact-copy asserts stay or update same-ticket)

`serving_badge`, `library_row_lines*`, `search_count_text` (`N of M`), `bulk_label` (`N checked (of M visible)`), `Acting on: <name>`, `health: never tested — press Test`, `Endpoint failed`, footer verbs (`Add/Save/Import/Export/Move/Rename/Delete/Overwrite/Reset/Clear/Restore/Start/Close/Cancel` — never bare `OK`), honest toasts (`Nothing to start — …`, `"<name>" is already …`), `Copied SOCKS/HTTP URL`, export plaintext warning verbatim, `Close the open dialog first — dropped files were not imported`.

## Overlay / focus rules

- Commits → `Dialog` + shared `Rc<DialogCommit>` footer (mouse = keyboard); destructive → `ok_variant(Danger)`.
- Reference → `Sheet` via `WindowExt::open_sheet` (no `Sheet::open`); `overlay_closable`, Escape closes topmost, focus returns to trigger.
- Menus → `DropdownMenu` + `PopupMenuItem::element(div role=MenuItem aria_label keyshortcuts tooltip)` (no id field — keep pattern).
- `Select` tooltip → outer wrapper (no `Select::tooltip`); `Tab` per-tab paint → wrapper+markers (no `Tab::id`); `StatusBar` center → `children()` (no `.center()`).
- Every key-bound action + menu-only destination also in `Ctrl-K` palette via same handlers; destructive commits return focus to shell; inspector header `Role::Status` announces selection.

## Secrets / honesty rules

Redaction in logs/URLs/labels; clipboard bind:port only; DPAPI at rest; export warning verbatim + safer path (`Copy URLs never carries secrets`); no `.disabled()` styling (honest-toast); no per-row start spinners; no fabricated stopped-row latency; Kbd only where bound; flat/hairline/6px/two-sizes/signal-only.
