use crate::*;

/// The one density switch. It owns every chrome height and the row pitch, so
/// the row, the header strip and the four bars can never disagree about how
/// tall a thing is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}

impl Density {
    /// The persisted token. `row_density` holds this string.
    pub const COMFORTABLE: &'static str = "comfortable";
    pub const COMPACT: &'static str = "compact";

    pub fn token(self) -> &'static str {
        match self {
            Self::Comfortable => Self::COMFORTABLE,
            Self::Compact => Self::COMPACT,
        }
    }

    /// A missing or unknown token reads as comfortable.
    pub fn from_token(token: &str) -> Option<Self> {
        match token.trim() {
            Self::COMFORTABLE => Some(Self::Comfortable),
            Self::COMPACT => Some(Self::Compact),
            _ => None,
        }
    }

    /// The row's pitch: the distance one row's top edge sits from the next.
    pub fn pitch(self) -> f32 {
        match self {
            Self::Comfortable => 36.,
            Self::Compact => 30.,
        }
    }

    /// The row's own height. The pitch and the row height are the same number:
    /// the rows are flat and separated by a hairline, never by a gap.
    pub fn row_height(self) -> f32 {
        self.pitch()
    }

    /// The action bar above the table.
    pub fn action_bar_height(self) -> f32 {
        match self {
            Self::Comfortable => 44.,
            Self::Compact => 40.,
        }
    }

    /// The filter row — the slot that holds search or the batch bar.
    pub fn filter_row_height(self) -> f32 {
        match self {
            Self::Comfortable => 36.,
            Self::Compact => 34.,
        }
    }

    /// The column header strip.
    pub fn header_height(self) -> f32 {
        match self {
            Self::Comfortable => 32.,
            Self::Compact => 30.,
        }
    }

    /// The engine bar under the content row.
    pub fn engine_bar_height(self) -> f32 {
        match self {
            Self::Comfortable => 40.,
            Self::Compact => 36.,
        }
    }
}

/// T-03: the one geometry spec for the library row.
///
/// The header strip and the row used to hardcode their widths twice and
/// disagree; now both read this struct, so a column's label always sits at
/// its value's first character.
///
/// `RowColumns` stays the VISIBILITY flags (what the user toggled in
/// Tools → Row columns); `RowMetrics` is the GEOMETRY (how wide each cell
/// is and which optional cells the width budget still affords).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowMetrics {
    /// The leading gutter. It holds one state glyph and nothing else.
    pub lead: f32,
    /// The name column's minimum width; it flexes into the leftover space.
    pub name_min: f32,
    pub group: f32,
    pub protocol: f32,
    pub upstream: f32,
    pub endpoint: f32,
    pub health: f32,
    pub traffic: f32,
    /// The trailing reserve. It is empty until the row is hovered or focused,
    /// but it is always there, so the grid never shifts.
    pub actions: f32,

    pub show_group: bool,
    pub show_protocol: bool,
    pub show_upstream: bool,
    pub show_health: bool,
    pub show_traffic: bool,

    /// The user's width and order overrides; the row and the header both
    /// iterate this, so a reorder cannot desynchronise their grids.
    pub layout: LibraryColumns,
}

impl RowMetrics {
    /// The gap between two row cells.
    pub const GAP: f32 = 8.;
    /// The leading gutter: one state glyph, and nothing else. It is not a
    /// button, so it carries no hit target.
    pub const LEAD: f32 = 24.;
    /// The state glyph inside the lead.
    pub const GLYPH: f32 = 14.;
    /// The trailing reserve that holds Start/Stop and More on hover.
    pub const ACTIONS: f32 = 56.;
    pub const NAME_MIN: f32 = 160.;
    pub const GROUP: f32 = 100.;
    pub const PROTOCOL: f32 = 90.;
    pub const UPSTREAM: f32 = 140.;
    pub const ENDPOINT: f32 = 120.;
    pub const HEALTH: f32 = 110.;
    pub const TRAFFIC: f32 = 120.;

    /// The table never falls under this width: below the drawer's window
    /// width the inspector stops taking space from the list.
    pub const TABLE_MIN: f32 = 640.;

    /// The geometry both readers share. `show_group` and `show_protocol` are
    /// the width budget's decisions; the rest are the user's column toggles.
    pub fn widths(
        show_group: bool,
        show_protocol: bool,
        columns: RowColumns,
        layout: LibraryColumns,
    ) -> RowMetrics {
        RowMetrics {
            lead: RowMetrics::LEAD,
            name_min: RowMetrics::NAME_MIN,
            group: layout.width(LibraryColumn::Group),
            protocol: layout.width(LibraryColumn::Protocol),
            upstream: layout.width(LibraryColumn::Upstream),
            endpoint: layout.width(LibraryColumn::Endpoint),
            health: layout.width(LibraryColumn::Health),
            traffic: layout.width(LibraryColumn::Traffic),
            actions: RowMetrics::ACTIONS,
            show_group,
            show_protocol,
            show_upstream: columns.upstream,
            show_health: columns.health,
            show_traffic: columns.traffic,
            layout,
        }
    }

    /// The width budget: which optional columns fit the table. The upstream
    /// column is the widest optional cell, so it is the one the budget gives
    /// up when the table is narrow; every other column is the user's own
    /// choice.
    pub fn for_table_width(
        table_width: f32,
        columns: RowColumns,
        layout: LibraryColumns,
    ) -> RowMetrics {
        let base = RowMetrics::row_width(columns, layout);
        let upstream = layout.width(LibraryColumn::Upstream);
        let show_upstream = columns.upstream && table_width >= base + upstream;
        let mut metrics = RowMetrics::widths(columns.group, columns.protocol, columns, layout);
        metrics.show_upstream = show_upstream;
        metrics
    }

    /// The effective width for one movable column.
    pub fn column_width(&self, column: LibraryColumn) -> f32 {
        match column {
            LibraryColumn::Group => self.group,
            LibraryColumn::Protocol => self.protocol,
            LibraryColumn::Upstream => self.upstream,
            LibraryColumn::Endpoint => self.endpoint,
            LibraryColumn::Health => self.health,
            LibraryColumn::Traffic => self.traffic,
        }
    }

    /// Whether the budget kept this column visible.
    pub fn column_visible(&self, column: LibraryColumn) -> bool {
        match column {
            LibraryColumn::Group => self.show_group,
            LibraryColumn::Protocol => self.show_protocol,
            LibraryColumn::Upstream => self.show_upstream,
            LibraryColumn::Endpoint | LibraryColumn::Health => true,
            LibraryColumn::Traffic => self.show_traffic,
        }
    }

    /// The row's minimum width: every cell at its width, the name at its
    /// minimum, and the gaps between them.
    fn row_width(columns: RowColumns, layout: LibraryColumns) -> f32 {
        let mut width = RowMetrics::LEAD
            + RowMetrics::NAME_MIN
            + layout.width(LibraryColumn::Endpoint)
            + RowMetrics::ACTIONS;
        let mut cells = 4;
        if columns.health {
            width += layout.width(LibraryColumn::Health);
            cells += 1;
        }
        if columns.group {
            width += layout.width(LibraryColumn::Group);
            cells += 1;
        }
        if columns.protocol {
            width += layout.width(LibraryColumn::Protocol);
            cells += 1;
        }
        if columns.traffic {
            width += layout.width(LibraryColumn::Traffic);
            cells += 1;
        }
        width + (cells - 1) as f32 * RowMetrics::GAP
    }
}
