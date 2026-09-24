# Research 04 — gpui-kit architecture constraints (primary: gpui-kit docs)

**Decision this informs:** UI structure, identity, lists, overlays, theme, keys, tests.
**Confidence:** high. Negative: no repo-bundled guides present (workspace has only `.opencode/` + `.planning/`).

## Constraints

1. Bootstrap: `application().with_assets()` → `.run(|cx| { init(cx); open_window(opts → cx.new(|cx| Root::new(view…))) })`; `init` first; one `Root` per window, first-level child. Sources: getting-started, component/root, coding-guides.
2. Overlay layers must render: `render_dialog_layer` + `render_sheet_layer` + `render_notification_layer` in content view. Source: component/root, getting-started, test docs.
3. `RenderOnce`/`IntoElement` for value-like presentational only; `Entity<T>` + `Render` for spanning/observed/focused/async/measured behavior. Source: coding-guides.
4. Retained entities (`InputState/ListState/TableState`/scroll/`FocusHandle`) created once outside `render`, stored on owner; never `cx.new` in render; never unconditional mutate/notify in render. Sources: getting-started, coding-guides, base/virtual-list.
5. Stable `ElementId`: domain IDs namespaced by parent; never translated labels, reorderable indices, or per-render randoms. Sources: coding-guides, element_id docs.
6. Unbounded list = virtualization mandatory: `List` (+`ListState/ListDelegate/IndexPath/render_item/search/selection/load_more`) or `DataTable` or raw `v/h_virtual_list`; separate source/filter/selection/viewport/render. Sources: component/list, data-table, virtual-list, base/virtual-list, coding-guides.
7. `VirtualList` size contract: `item_sizes: Rc<Vec<Size<Pixels>>>` (len == count); vertical reads height only; cross axis measured from one item; hold sizes + scroll handle in entity; `.track_scroll`; bounded parent for Scrollbar; hot render closure (no I/O/sort/filter, elements not entities). Sources: base/component virtual-list.
8. `DataTable` selection flags + keyboard semantics + programmatic selection/scroll + `perform_sort` + infinite load. Source: component/data-table.
9. Overlays only via `WindowExt` (`open_dialog/close_dialog/open_sheet[_at]/close_sheet/open_alert_dialog`); Dialog defaults overlay/closable/keyboard/close-button true; AlertDialog flips closable+button off, centered footer, width 420; `on_ok/on_cancel -> bool`; `Dialog::new(cx)` (window-arg deprecated). Sources: component/dialog, sheet, alert-dialog.
10. Focus trap built into Dialog/Sheet; manual trap for custom modals only; Tab cycles in trap; top-first dismiss + focus restore. Sources: component/focus-trap, coding-guides.
11. `TitleBar` (`::new()`, `TITLE_BAR_HEIGHT 34px`, open with `TitleBar::window_options()` or titlebar options + `app_owns_titlebar_drag`); `StatusBar` left/center/right, ghost-xsmall buttons, `Separator::vertical()`, status tokens. TRAY ABSENT from `llms.txt` catalog (60+ components, no tray); OS tray would be an outside-kit platform seam. Sources: component/title-bar, status-bar, llms.txt, coding-guides platform boundaries.
12. Theme: `cx.theme()` roles + rem helpers; no hex/rgb/px in app UI except hairlines/insets/raster/measured/theme-def; `semantic_tokens` for new presentation; `sync_base` after global mutation; base `font_size` = rem zoom. Sources: coding-guides, design-guides, component/theme.
13. Keyboard: pointer callbacks for pointer; `Action`s for bound/menued/multi-input commands (one command modeled once); `key_context` + `on_action` on same focused region; retained `FocusHandle`; visible `focus_visible`; `Button` for commands (ghost/outline quiet), `Link` for browser/mail only. Sources: coding-guides, design-guides.
14. Tests (4 layers): `#[test]` pure; `#[gpui_kit::test]` + `TestAppContext` entities/async; `VisualTestContext` UI-integration (`find/click/press/input/scroll/drag`, `render_frame` first, `wait_for` outside `update_window`, re-query after actions); offscreen pixels macOS-Metal only. `test-support` dev-deps only; import test types explicitly. Source: test docs, coding-guides.
15. Agent gate: read impl + tests + re-export seam + component doc before editing; never translate React/CSS/old-GPUI by analogy; name owner/presentation/identity/keyboard/layout/tokens/regression test per change. Source: coding-guides.
