use crate::*;

/// T-03: the one geometry spec for the library row.
///
/// The header strip and the row used to hardcode their widths twice and
/// disagree; now both read this struct, so a column's label always sits at
/// its value's first character — and the row never re-measures itself into a
/// column of stacked cards.
///
/// `RowColumns` stays the VISIBILITY flags (what the user toggled in
/// Tools → Row columns…); `RowMetrics` is the GEOMETRY (how wide each cell
/// is and which optional cells the width budget still affords).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RowMetrics {
    /// The occupancy lamp and the checkbox share this fixed leading cell.
    pub(crate) lamp_check: f32,
    /// The numeral check chip's slot. It is always reserved, so a checked
    /// row never shifts its name out of the header's alignment.
    pub(crate) check_slot: f32,
    /// The name column's minimum width; it flexes into the leftover space.
    pub(crate) name_min: f32,
    pub(crate) group: f32,
    pub(crate) protocol: f32,
    pub(crate) upstream: f32,
    pub(crate) endpoint: f32,
    pub(crate) health: f32,
    pub(crate) traffic: f32,
    pub(crate) copy: f32,

    pub(crate) show_group: bool,
    pub(crate) show_protocol: bool,
    pub(crate) show_upstream: bool,
    pub(crate) show_health: bool,
    pub(crate) show_traffic: bool,
}

impl RowMetrics {
    /// The gap between two row cells.
    pub(crate) const GAP: f32 = 8.;
    /// One row's PITCH — the definite height of the row. The card carries
    /// the rest, and a hairline of breathing room separates two cards.
    pub(crate) const ROW_PITCH: f32 = 30.;
    /// The card's own height.
    pub(crate) const CARD_HEIGHT: f32 = 28.;
    /// The gap between two cards.
    pub(crate) const CARD_GAP: f32 = 1.;

    pub(crate) const LAMP_CHECK: f32 = 28.;
    /// The chip shows the checked position; three digits need ~31px with
    /// `px_1` + a 1px border, so the slot is 32. It stays reserved whether or
    /// not a row is checked.
    pub(crate) const CHECK_SLOT: f32 = 32.;
    pub(crate) const NAME_MIN: f32 = 160.;
    pub(crate) const GROUP: f32 = 100.;
    pub(crate) const PROTOCOL: f32 = 90.;
    pub(crate) const UPSTREAM: f32 = 140.;
    pub(crate) const ENDPOINT: f32 = 120.;
    pub(crate) const HEALTH: f32 = 110.;
    pub(crate) const TRAFFIC: f32 = 120.;
    pub(crate) const COPY: f32 = 72.;

    /// The table never falls under this width: below the drawer's window
    /// width the inspector stops taking space from the list.
    pub(crate) const TABLE_MIN: f32 = 640.;

    /// The geometry both readers share. `show_group` and `show_protocol` are
    /// the width budget's decisions; the rest are the user's column toggles.
    pub(crate) fn widths(show_group: bool, show_protocol: bool, columns: RowColumns) -> RowMetrics {
        RowMetrics {
            lamp_check: RowMetrics::LAMP_CHECK,
            check_slot: RowMetrics::CHECK_SLOT,
            name_min: RowMetrics::NAME_MIN,
            group: RowMetrics::GROUP,
            protocol: RowMetrics::PROTOCOL,
            upstream: RowMetrics::UPSTREAM,
            endpoint: RowMetrics::ENDPOINT,
            health: RowMetrics::HEALTH,
            traffic: RowMetrics::TRAFFIC,
            copy: RowMetrics::COPY,
            show_group,
            show_protocol,
            show_upstream: columns.upstream,
            show_health: columns.health,
            show_traffic: columns.traffic,
        }
    }

    /// The width budget: which optional columns fit the table. The protocol
    /// column goes first, then the group column; the name, endpoint, health
    /// and copy columns never go. The group column also honours the user's
    /// own toggle — the budget can only take a column away, never add one.
    pub(crate) fn for_table_width(table_width: f32, columns: RowColumns) -> RowMetrics {
        let both = RowMetrics::row_width(columns, true, true);
        let protocol_only = RowMetrics::row_width(columns, false, true);
        let (budget_group, budget_protocol) = if table_width >= both {
            (true, true)
        } else if table_width >= protocol_only {
            (false, true)
        } else {
            (false, false)
        };
        RowMetrics::widths(columns.group && budget_group, budget_protocol, columns)
    }

    /// The row's minimum width: every cell at its width, the name at its
    /// minimum, and the gaps between them.
    fn row_width(columns: RowColumns, show_group: bool, show_protocol: bool) -> f32 {
        let mut width = RowMetrics::LAMP_CHECK
            + RowMetrics::CHECK_SLOT
            + RowMetrics::NAME_MIN
            + RowMetrics::ENDPOINT
            + RowMetrics::COPY;
        let mut cells = 5;
        if columns.upstream {
            width += RowMetrics::UPSTREAM;
            cells += 1;
        }
        if columns.health {
            width += RowMetrics::HEALTH;
            cells += 1;
        }
        if columns.traffic {
            width += RowMetrics::TRAFFIC;
            cells += 1;
        }
        if show_group {
            width += RowMetrics::GROUP;
            cells += 1;
        }
        if show_protocol {
            width += RowMetrics::PROTOCOL;
            cells += 1;
        }
        width + (cells - 1) as f32 * RowMetrics::GAP
    }
}
