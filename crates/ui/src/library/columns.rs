use crate::*;

use gpui_kit::component::ThemeStyled as _;

/// T-03: one header cell. The wrapper owns the column width so the edge handle
/// can sit on the boundary; the label fills it, so every existing `header-*`
/// test id keeps the same first-character position.
fn resizable_header(
    id: String,
    label_id: String,
    column: LibraryColumn,
    width: f32,
    cx: &mut Context<AppView>,
    label: AnyElement,
) -> AnyElement {
    div()
        .id(id)
        .relative()
        .w(px(width))
        .h_full()
        .flex_shrink_0()
        .child(label)
        .child(column_resize_handle(label_id, column, width, cx))
        .into_any_element()
}

fn column_resize_handle(
    handle_id: String,
    column: LibraryColumn,
    width: f32,
    cx: &mut Context<AppView>,
) -> AnyElement {
    div()
        .id(handle_id)
        .test_support()
        .absolute()
        // Half the hit area hangs into the 8px gap, so the visible boundary and
        // the grab target agree without spending layout width.
        .right(px(-3.))
        .top_0()
        .bottom_0()
        .w(px(6.))
        .cursor(CursorStyle::ResizeLeftRight)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, event: &MouseDownEvent, window, cx| {
                // The header itself owns click-to-sort; a press on the edge is
                // a resize and must not also open the sort menu path.
                cx.stop_propagation();
                if event.click_count >= 2 {
                    view.reset_column_width(column, window, cx);
                } else {
                    view.begin_column_resize(column, event.position.x.as_f32(), width, cx);
                }
            }),
        )
        .into_any_element()
}

pub(crate) fn column_header_label(
    column: LibraryColumn,
    width: f32,
    view: Entity<AppView>,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let column_id = column.id();
    let menu_view = view.clone();
    let label = div()
        .id(format!("header-{column_id}"))
        .test_support()
        .w_full()
        .h_full()
        .truncate()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(column.label())
        .context_menu(move |menu, _, _| AppView::width_header_menu(menu, column, &menu_view))
        .into_any_element();
    resizable_header(
        format!("header-{column_id}-cell"),
        format!("column-resize-{column_id}"),
        column,
        width,
        cx,
        label,
    )
}

/// T-27: a sortable column header — a labeled cell in the header strip that
/// carries the sort menu on right-click (the same context-menu affordance the
/// row cards use) and sorts on click, Enter, or Space. The active column shows
/// its direction in the label.
pub(crate) fn sort_header_cell(
    column: LibraryColumn,
    sort: LibrarySort,
    width: f32,
    view: Entity<AppView>,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let column_id = column.id();
    let label = column.label();
    let focus_handle = window
        .use_keyed_state(format!("sort-header-focus-{column_id}"), cx, |_, cx| {
            cx.focus_handle()
        })
        .read(cx)
        .clone();
    let focused = focus_handle.is_focused(window);
    let active =
        sort.key != LibrarySortKey::None && sort_header_keys(column_id).contains(&sort.key);
    let marker = if active { sort_header_marker(sort) } else { "" };
    let tip = if active {
        format!(
            "{label} — sorted by {} ({}) — click to reverse, right-click to change",
            sort.key.label(),
            sort.direction.label()
        )
    } else {
        format!("{label} — click to sort, right-click to pick a sort key")
    };
    let tooltip_text = tip.clone();
    let menu_view = view.clone();
    let mut header = div()
        .id(format!("sort-header-{column_id}"))
        .test_support()
        .w_full()
        .h_full()
        .truncate()
        .text_sm()
        .cursor_pointer()
        .text_color(if active {
            cx.theme().foreground
        } else {
            cx.theme().muted_foreground
        })
        .track_focus(&focus_handle)
        .tab_index(0)
        .role(Role::Button)
        // K12/A5-1: the accessible name mirrors the visible text (label plus
        // the word marker), so the header can never read as `Health 1` to a
        // screen reader or a UI test. The longer grammar lives in the tooltip.
        .aria_label(format!("{label}{marker}"))
        .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tooltip_text.clone())).into())
        .on_click(cx.listener(move |view, _, window, cx| {
            view.sort_by_header(column_id, window, cx);
        }))
        .child(format!("{label}{marker}"));
    if focused {
        header = header.focus_ring_style(window, cx);
    }
    let label = header
        .context_menu(move |menu, _, _| {
            AppView::sort_header_menu(menu, column_id, sort, &menu_view)
        })
        .into_any_element();
    resizable_header(
        format!("sort-header-{column_id}-cell"),
        format!("column-resize-{column_id}"),
        column,
        width,
        cx,
        label,
    )
}
