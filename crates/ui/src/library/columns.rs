use crate::*;

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
/// row cards use). The active column shows its direction in the label.
pub(crate) fn sort_header_cell(
    column: &'static str,
    label: &'static str,
    width: f32,
    sort: LibrarySort,
    view: Entity<AppView>,
    cx: &App,
) -> AnyElement {
    let active = sort.key != LibrarySortKey::None && sort_header_keys(column).contains(&sort.key);
    let marker = if active { sort_header_marker(sort) } else { "" };
    let tip = if active {
        format!(
            "{label} — sorted by {} ({}) — right-click to change",
            sort.key.label(),
            sort.direction.label()
        )
    } else {
        format!("{label} — right-click to pick a sort key")
    };
    let tooltip_text = tip.clone();
    div()
        .id(format!("sort-header-{column}"))
        .test_support()
        .w(px(width))
        .flex_shrink_0()
        .truncate()
        .text_sm()
        .text_color(if active {
            cx.theme().foreground
        } else {
            cx.theme().muted_foreground
        })
        // K12/A5-1: the accessible name mirrors the visible text (label plus
        // the word marker), so the header can never read as `Health 1` to a
        // screen reader or a UI test. The longer grammar lives in the tooltip.
        .aria_label(format!("{label}{marker}"))
        .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tooltip_text.clone())).into())
        .child(format!("{label}{marker}"))
        .context_menu(move |menu, _, _| AppView::sort_header_menu(menu, column, sort, &view))
        .into_any_element()
}
