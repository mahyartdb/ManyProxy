use crate::*;

/// T-27: the secondary order that keeps unknown/missing values stable across
/// ticks. Row ids are numeric strings; compare them as numbers so "2" precedes
/// "10", and fall back to bytes for anything else.
pub(crate) fn row_id_order(a: &str, b: &str) -> Ordering {
    match (a.parse::<u64>(), b.parse::<u64>()) {
        (Ok(left), Ok(right)) => left.cmp(&right),
        _ => a.cmp(b),
    }
}

/// T-27: one key, one direction. Known values rank by the key (direction
/// applied); unknown or missing values are pinned last regardless of direction
/// and ordered by row id among themselves, so a list never jumps between ticks.
/// `TypeSecurity` carries its own fixed order (the retired toggle) and ignores
/// the direction; `None` never reaches here.
pub(crate) fn compare_library_rows(
    context: &SortContext,
    sort: LibrarySort,
    a: &ProxyConfig,
    b: &ProxyConfig,
) -> Ordering {
    if sort.key == LibrarySortKey::TypeSecurity {
        return compare_type_security(a, b);
    }
    let descending = sort.direction == SortDirection::Descending;
    if sort.key == LibrarySortKey::OutIp {
        return match (context.out_ip(&a.id), context.out_ip(&b.id)) {
            (Some(left), Some(right)) => {
                let order = left.cmp(right);
                (if descending { order.reverse() } else { order })
                    .then_with(|| row_id_order(&a.id, &b.id))
            }
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => row_id_order(&a.id, &b.id),
        };
    }
    match (
        context.numeric(sort.key, &a.id),
        context.numeric(sort.key, &b.id),
    ) {
        (Some(left), Some(right)) => {
            let order = left.cmp(&right);
            (if descending { order.reverse() } else { order })
                .then_with(|| row_id_order(&a.id, &b.id))
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => row_id_order(&a.id, &b.id),
    }
}

/// T-27: which keys each sortable header offers. The health/status column owns
/// the probe and identification keys, including the retired toggle's
/// Type|Security; the traffic column owns the accumulated counters.
pub(crate) fn sort_header_keys(column: &str) -> &'static [LibrarySortKey] {
    static HEALTH: [LibrarySortKey; 6] = [
        LibrarySortKey::None,
        LibrarySortKey::Latency,
        LibrarySortKey::Download,
        LibrarySortKey::Upload,
        LibrarySortKey::OutIp,
        LibrarySortKey::TypeSecurity,
    ];
    static TRAFFIC: [LibrarySortKey; 4] = [
        LibrarySortKey::None,
        LibrarySortKey::TrafficTotal,
        LibrarySortKey::TrafficDown,
        LibrarySortKey::TrafficUp,
    ];
    match column {
        "traffic" => &TRAFFIC,
        _ => &HEALTH,
    }
}

/// T-03: a labeled header cell. The id is the column's anchor, so a test can
/// measure the label against the row cell it names.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RowColumns {
    pub(crate) upstream: bool,
    pub(crate) health: bool,
    pub(crate) group: bool,
    pub(crate) traffic: bool,
}

impl Default for RowColumns {
    fn default() -> Self {
        Self {
            upstream: true,
            health: true,
            group: false,
            traffic: false,
        }
    }
}

impl RowColumns {
    pub(crate) const ORDER: [&'static str; 4] = ["upstream", "health", "group", "traffic"];

    pub(crate) fn parse(raw: &str) -> Self {
        if raw.trim().is_empty() {
            return Self::default();
        }
        let mut columns = Self {
            upstream: false,
            health: false,
            group: false,
            traffic: false,
        };
        for token in raw.split(',').map(str::trim) {
            match token {
                "upstream" => columns.upstream = true,
                "health" => columns.health = true,
                "group" => columns.group = true,
                "traffic" => columns.traffic = true,
                _ => {}
            }
        }
        columns
    }

    pub(crate) fn display(self) -> String {
        if !self.upstream && !self.health && !self.group && !self.traffic {
            return "none".to_string();
        }
        let mut names = Vec::with_capacity(4);
        for name in Self::ORDER {
            let enabled = match name {
                "upstream" => self.upstream,
                "health" => self.health,
                "group" => self.group,
                _ => self.traffic,
            };
            if enabled {
                names.push(name);
            }
        }
        names.join(",")
    }
}
