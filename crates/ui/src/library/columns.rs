use crate::*;

use gpui_kit::component::ThemeStyled as _;

pub(crate) fn column_header_label(
    id: &str,
    label: &'static str,
    width: f32,
    cx: &App,
) -> AnyElement {
    div()
        .id(id.to_string())
        .test_support()
        .w(px(width))
        .flex_shrink_0()
        .truncate()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(label)
        .into_any_element()
}

/// T-27: a sortable column header — a labeled cell in the header strip that
/// carries the sort menu on right-click (the same context-menu affordance the
/// row cards use) and sorts on click, Enter, or Space. The active column shows
/// its direction in the label.
pub(crate) fn sort_header_cell(
    column: &'static str,
    label: &'static str,
    width: f32,
    sort: LibrarySort,
    view: Entity<AppView>,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let focus_handle = window
        .use_keyed_state(format!("sort-header-focus-{column}"), cx, |_, cx| {
            cx.focus_handle()
        })
        .read(cx)
        .clone();
    let focused = focus_handle.is_focused(window);
    let active = sort.key != LibrarySortKey::None && sort_header_keys(column).contains(&sort.key);
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
    let mut header = div()
        .id(format!("sort-header-{column}"))
        .test_support()
        .w(px(width))
        .flex_shrink_0()
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
            view.sort_by_header(column, window, cx);
        }))
        .child(format!("{label}{marker}"));
    if focused {
        header = header.focus_ring_style(window, cx);
    }
    header
        .context_menu(move |menu, _, _| AppView::sort_header_menu(menu, column, sort, &view))
        .into_any_element()
}
