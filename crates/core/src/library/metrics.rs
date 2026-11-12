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

/// T-03/R11: the one geometry spec for the library row.
///
/// The header strip and the row both read this struct, so a column's label
/// always sits at its value's first character. `layout` is the visibility and
/// order the user chose; `widths` is that layout resolved against the table's
/// width once per frame. The table never hides a column to make the widths
/// fit: a too-wide layout scrolls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowMetrics {
    /// The leading gutter. It holds one state glyph and nothing else.
    pub lead: f32,
    /// The trailing reserve. It is empty until the row is hovered or focused,
    /// but it is always there, so the grid never shifts.
    pub actions: f32,
    /// The effective width per column, in `ColumnId::index` order.
    pub widths: [f32; 7],
    /// The user's order, visibility and width overrides.
    pub layout: ColumnLayout,
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

    /// Resolve the layout against the table width. Every visible column keeps
    /// a width; the caller scrolls when the sum exceeds the table.
    pub fn for_table_width(table_width: f32, layout: ColumnLayout) -> RowMetrics {
        RowMetrics {
            lead: RowMetrics::LEAD,
            actions: RowMetrics::ACTIONS,
            widths: layout.effective_widths(table_width),
            layout,
        }
    }

    /// The effective width for one column.
    pub fn column_width(&self, column: ColumnId) -> f32 {
        self.widths[column.index()]
    }

    /// Whether the user kept this column visible.
    pub fn column_visible(&self, column: ColumnId) -> bool {
        self.layout.is_visible(column)
    }
}
