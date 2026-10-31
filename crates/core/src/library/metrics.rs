use crate::*;

/// T-03: the one geometry spec for the library row.
///
/// The header strip and the row used to hardcode their widths twice and
/// disagree; now both read this struct, so a column's label always sits at
/// its value's first character — and the row never re-measures itself into a
/// column of stacked cards.
///
/// `RowColumns` stays the VISIBILITY flags (what the user toggled in
/// Tools → Row columns); `RowMetrics` is the GEOMETRY (how wide each cell
/// is and which optional cells the width budget still affords).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowMetrics {
    /// The occupancy lamp and the checkbox share this fixed leading cell.
    pub lamp_check: f32,
    /// The reserved slot after the lamp/checkbox cell. K4 removed the numeral
    /// chip, but the slot stays reserved so the column grid (and the header
    /// strip that mirrors it) never shifts.
    pub check_slot: f32,
    /// The name column's minimum width; it flexes into the leftover space.
    pub name_min: f32,
    pub group: f32,
    pub protocol: f32,
    pub upstream: f32,
    pub endpoint: f32,
    pub health: f32,
    pub traffic: f32,
    pub copy: f32,

    pub show_group: bool,
    pub show_protocol: bool,
    pub show_upstream: bool,
    pub show_health: bool,
    pub show_traffic: bool,
}

impl RowMetrics {
    /// The gap between two row cells.
    pub const GAP: f32 = 8.;
    /// One row's PITCH — the definite height of the row. The card carries
    /// the rest, and a hairline of breathing room separates two cards.
    pub const ROW_PITCH: f32 = 30.;
    /// The card's own height.
    pub const CARD_HEIGHT: f32 = 28.;
    /// The gap between two cards.
    pub const CARD_GAP: f32 = 1.;

    pub const LAMP_CHECK: f32 = 28.;
    /// The empty slot keeps the name column aligned with the header strip.
    /// The constant is geometry, not a chip width, since K4 dropped numerals.
    pub const CHECK_SLOT: f32 = 32.;
    pub const NAME_MIN: f32 = 160.;
    pub const GROUP: f32 = 100.;
    pub const PROTOCOL: f32 = 90.;
    pub const UPSTREAM: f32 = 140.;
    pub const ENDPOINT: f32 = 120.;
    pub const HEALTH: f32 = 110.;
    pub const TRAFFIC: f32 = 120.;
    pub const COPY: f32 = 72.;

    /// The table never falls under this width: below the drawer's window
    /// width the inspector stops taking space from the list.
    pub const TABLE_MIN: f32 = 640.;

    /// The geometry both readers share. `show_group` and `show_protocol` are
    /// the width budget's decisions; the rest are the user's column toggles.
    pub fn widths(show_group: bool, show_protocol: bool, columns: RowColumns) -> RowMetrics {
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

    /// The width budget: which optional columns fit the table. The group
    /// column goes first, then the protocol column, then upstream; the name,
    /// endpoint, health and copy columns never go. An optional column also
    /// honours the user's own toggle — the budget can only take a column
    /// away, never add one.
    pub fn for_table_width(table_width: f32, columns: RowColumns) -> RowMetrics {
        let both = RowMetrics::row_width(columns, true, true);
        let protocol_only = RowMetrics::row_width(columns, false, true);
        let upstream_only = RowMetrics::row_width(columns, false, false);
        let (budget_group, budget_protocol, budget_upstream) = if table_width >= both {
            (true, true, true)
        } else if table_width >= protocol_only {
            (false, true, true)
        } else if table_width >= upstream_only {
            (false, false, true)
        } else {
            (false, false, false)
        };
        let mut metrics =
            RowMetrics::widths(columns.group && budget_group, budget_protocol, columns);
        metrics.show_upstream = columns.upstream && budget_upstream;
        metrics
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
