use crate::app::columns::ColumnDragPreview;
use crate::*;

use gpui_kit::component::ThemeStyled as _;

/// T-03/T-04: one header cell. The wrapper owns the column width so the edge
/// handle can sit on the boundary; it is also the drag source and the drop
/// target. Every visible column uses this shape, Name included.
pub fn resizable_header(
    column: ColumnId,
    width: f32,
    drop_side: Option<DropSide>,
    view: Entity<AppView>,
    cx: &mut Context<AppView>,
    label: AnyElement,
) -> AnyElement {
    let column_id = column.id();
    let wrap_id = format!("header-{column_id}-cell");
    let handle_id = format!("column-resize-{column_id}");
    let drag_view = view.clone();
    let indicator = drop_side.map(|side| {
        let bar = div()
            .id(format!("column-drop-{}", column.id()))
            .test_support()
            .absolute()
            .top_0()
            .bottom_0()
            .w(px(2.))
            .bg(cx.theme().accent);
        if side == DropSide::Before {
            bar.left(px(-4.))
        } else {
            bar.right(px(-4.))
        }
    });
    div()
        .id(wrap_id)
        .relative()
        .w(px(width))
        .h_full()
        .flex_shrink_0()
        .on_drag(ColumnDrag { column }, move |drag: &ColumnDrag, _, _, cx| {
            drag_view.update(cx, |view, cx| view.clear_column_drop(cx));
            cx.new(|_| ColumnDragPreview {
                label: library_column_label(drag.column),
            })
        })
        .on_drag_move(
            cx.listener(move |view, event: &DragMoveEvent<ColumnDrag>, _, cx| {
                if !event.bounds.contains(&event.event.position) {
                    return;
                }
                let side = if event.event.position.x >= event.bounds.center().x {
                    DropSide::After
                } else {
                    DropSide::Before
                };
                view.hover_column_drop(column, side, cx);
            }),
        )
        .on_drop(cx.listener(move |view, drag: &ColumnDrag, window, cx| {
            view.drop_column(*drag, column, window, cx);
        }))
        .child(label)
        .child(column_resize_handle(handle_id, column, width, cx))
        .children(indicator)
        .into_any_element()
}

pub fn column_resize_handle(
    handle_id: String,
    column: ColumnId,
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
                // a resize and must not also sort.
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

/// T-27/R7: one sortable header. Every visible column sorts on click, a second
/// click reverses the direction, and the label carries the direction word. The
/// right-click menu is the same Explorer menu for every column.
pub fn column_header_cell(
    column: ColumnId,
    sort: LibrarySort,
    metrics: RowMetrics,
    drop_side: Option<DropSide>,
    view: Entity<AppView>,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let column_id = column.id();
    let label = library_column_label(column);
    let layout = metrics.layout;
    let width = metrics.column_width(column);
    let focus_handle = window
        .use_keyed_state(format!("sort-header-focus-{column_id}"), cx, |_, cx| {
            cx.focus_handle()
        })
        .read(cx)
        .clone();
    let focused = focus_handle.is_focused(window);
    let active = sort.key == column.sort_key();
    let marker = if active {
        sort_header_marker(sort)
    } else {
        String::new()
    };
    let tip = if active {
        t!(
            "sort-header-active-tip",
            label = &label,
            key = library_sort_key_label(sort.key),
            direction = sort_direction_label(sort.direction)
        )
    } else {
        t!("sort-header-idle-tip", label = &label)
    };
    let tooltip_text = tip.clone();
    let menu_view = view.clone();
    let menu_layout = layout;
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
        // screen reader or a UI test.
        .aria_label(format!("{label}{marker}"))
        .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tooltip_text.clone())).into())
        .on_click(cx.listener(move |view, _, window, cx| {
            view.sort_by_header(column, window, cx);
        }))
        .child(format!("{label}{marker}"));
    if focused {
        header = header.focus_ring_style(window, cx);
    }
    let label = header
        .context_menu(move |menu, _, _| {
            AppView::column_header_menu(menu, column, menu_layout, &menu_view)
        })
        .into_any_element();
    resizable_header(column, width, drop_side, view, cx, label)
}
