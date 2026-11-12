use std::cmp::Ordering;
use std::collections::HashMap;

use crate::*;

/// T-27: the per-row maps the sort keys read, borrowed for one `apply_filter`
/// pass. Everything here is a mirror the app already keeps fresh on its normal
/// ticks, so sorting is a pure render-time computation.
pub struct SortContext<'a> {
    /// Latest successful URL-test sample per row, in milliseconds.
    pub latency: &'a HashMap<String, u64>,
    /// Per-row current (session) data-plane counters, rx then tx.
    pub data_traffic: &'a HashMap<String, (u64, u64)>,
    /// Per-row accumulated counters in the store's counting window, rx then tx.
    pub totals: &'a HashMap<String, (u64, u64)>,
    /// Resolved egress IP per row, as text.
    pub out_ips: &'a HashMap<String, String>,
}

impl SortContext<'_> {
    pub fn numeric(&self, key: LibrarySortKey, row_id: &str) -> Option<u64> {
        match key {
            LibrarySortKey::Latency => self.latency.get(row_id).copied(),
            LibrarySortKey::Download => self.data_traffic.get(row_id).map(|(rx, _)| *rx),
            LibrarySortKey::Upload => self.data_traffic.get(row_id).map(|(_, tx)| *tx),
            LibrarySortKey::TrafficTotal => self
                .totals
                .get(row_id)
                .map(|(rx, tx)| rx.saturating_add(*tx)),
            LibrarySortKey::TrafficDown => self.totals.get(row_id).map(|(rx, _)| *rx),
            LibrarySortKey::TrafficUp => self.totals.get(row_id).map(|(_, tx)| *tx),
            _ => None,
        }
    }

    pub fn out_ip(&self, row_id: &str) -> Option<&str> {
        self.out_ips
            .get(row_id)
            .map(String::as_str)
            .filter(|text| !text.trim().is_empty())
    }
}

/// T-27: the secondary order that keeps unknown/missing values stable across
/// ticks. Row ids are numeric strings; compare them as numbers so "2" precedes
/// "10", and fall back to bytes for anything else.
pub fn row_id_order(a: &str, b: &str) -> Ordering {
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
pub fn compare_library_rows(
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
pub fn sort_header_keys(column: &str) -> &'static [LibrarySortKey] {
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

/// The movable columns, in their default order. The endpoint, health and
/// group columns lead, because the signed row reads Name, Endpoint, Health,
/// Group; the name stays pinned left and the trailing actions stay pinned
/// right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryColumn {
    Group,
    Protocol,
    Upstream,
    Endpoint,
    Health,
    Traffic,
}

impl LibraryColumn {
    pub const ALL: [LibraryColumn; 6] = [
        LibraryColumn::Endpoint,
        LibraryColumn::Health,
        LibraryColumn::Group,
        LibraryColumn::Protocol,
        LibraryColumn::Upstream,
        LibraryColumn::Traffic,
    ];

    /// The widest a dragged column may become. Wider than any cell needs and
    /// narrow enough that one column cannot swallow the whole table alone.
    pub const MAX_WIDTH: f32 = 480.;

    pub fn id(self) -> &'static str {
        match self {
            LibraryColumn::Group => "group",
            LibraryColumn::Protocol => "protocol",
            LibraryColumn::Upstream => "upstream",
            LibraryColumn::Endpoint => "endpoint",
            LibraryColumn::Health => "health",
            LibraryColumn::Traffic => "traffic",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LibraryColumn::Group => "Group",
            LibraryColumn::Protocol => "Protocol",
            LibraryColumn::Upstream => "Upstream",
            LibraryColumn::Endpoint => "Endpoint",
            LibraryColumn::Health => "Health",
            LibraryColumn::Traffic => "Traffic",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|column| column.id() == id)
    }

    pub fn index(self) -> usize {
        match self {
            LibraryColumn::Group => 0,
            LibraryColumn::Protocol => 1,
            LibraryColumn::Upstream => 2,
            LibraryColumn::Endpoint => 3,
            LibraryColumn::Health => 4,
            LibraryColumn::Traffic => 5,
        }
    }

    pub fn default_width(self) -> f32 {
        match self {
            LibraryColumn::Group => RowMetrics::GROUP,
            LibraryColumn::Protocol => RowMetrics::PROTOCOL,
            LibraryColumn::Upstream => RowMetrics::UPSTREAM,
            LibraryColumn::Endpoint => RowMetrics::ENDPOINT,
            LibraryColumn::Health => RowMetrics::HEALTH,
            LibraryColumn::Traffic => RowMetrics::TRAFFIC,
        }
    }

    /// A header must still show its own name at the smallest size.
    pub fn min_width(self) -> f32 {
        match self {
            LibraryColumn::Group => 64.,
            LibraryColumn::Protocol => 72.,
            LibraryColumn::Upstream => 90.,
            LibraryColumn::Endpoint => 96.,
            LibraryColumn::Health => 84.,
            LibraryColumn::Traffic => 84.,
        }
    }
}

/// T-03: persisted geometry for the movable columns. Visibility stays in
/// [`RowColumns`]; this is the user's width and order choice layered over the
/// computed defaults, stored in its own meta row so a corrupt payload can only
/// reset geometry, never hide a column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LibraryColumns {
    widths: [Option<f32>; 6],
    order: [LibraryColumn; 6],
}

impl Default for LibraryColumns {
    fn default() -> Self {
        Self {
            widths: [None; 6],
            order: LibraryColumn::ALL,
        }
    }
}

impl LibraryColumns {
    pub fn order(&self) -> &[LibraryColumn; 6] {
        &self.order
    }

    /// The effective width: the user's override clamped to the column's own
    /// floor, or the computed default when untouched.
    pub fn width(&self, column: LibraryColumn) -> f32 {
        let raw = self.widths[column.index()].unwrap_or(column.default_width());
        if raw.is_finite() {
            raw.clamp(column.min_width(), LibraryColumn::MAX_WIDTH)
        } else {
            column.default_width()
        }
    }

    pub fn is_custom(&self, column: LibraryColumn) -> bool {
        self.widths[column.index()].is_some()
    }

    pub fn set_width(&mut self, column: LibraryColumn, width: f32) {
        if !width.is_finite() {
            return;
        }
        self.widths[column.index()] = Some(
            width
                .clamp(column.min_width(), LibraryColumn::MAX_WIDTH)
                .round(),
        );
    }

    pub fn reset_width(&mut self, column: LibraryColumn) {
        self.widths[column.index()] = None;
    }

    pub fn reset_widths(&mut self) {
        self.widths = [None; 6];
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Move `source` next to `target`. The array only ever holds the six
    /// movable columns, so the fixed name/copy ends can never be crossed.
    pub fn move_column(
        &mut self,
        source: LibraryColumn,
        target: LibraryColumn,
        after: bool,
    ) -> bool {
        if source == target {
            return false;
        }
        let mut rest: Vec<LibraryColumn> = self
            .order
            .iter()
            .copied()
            .filter(|column| *column != source)
            .collect();
        let Some(target_index) = rest.iter().position(|column| *column == target) else {
            return false;
        };
        rest.insert(
            if after {
                target_index + 1
            } else {
                target_index
            },
            source,
        );
        if rest.as_slice() == self.order.as_slice() {
            return false;
        }
        self.order.copy_from_slice(&rest);
        true
    }

    /// Canonical meta payload. Widths are keyed by id so a later column
    /// insertion cannot shift an existing override onto its neighbour.
    pub fn to_json(&self) -> String {
        let widths: serde_json::Map<String, serde_json::Value> = LibraryColumn::ALL
            .iter()
            .filter_map(|column| {
                self.widths[column.index()]
                    .map(|width| (column.id().to_string(), serde_json::json!(width)))
            })
            .collect();
        let order: Vec<&str> = self.order.iter().map(|column| column.id()).collect();
        serde_json::json!({ "widths": widths, "order": order }).to_string()
    }

    /// Tolerant read: unknown ids, duplicates, and malformed numbers are
    /// ignored; any column the payload omits keeps its default position.
    pub fn from_json(raw: &str) -> Self {
        let Ok(serde_json::Value::Object(map)) = serde_json::from_str(raw.trim()) else {
            return Self::default();
        };
        let mut layout = Self::default();
        if let Some(entries) = map.get("order").and_then(|value| value.as_array()) {
            let mut seen = [false; 6];
            let mut order = Vec::with_capacity(6);
            for entry in entries {
                let Some(column) = entry.as_str().and_then(LibraryColumn::from_id) else {
                    continue;
                };
                if !seen[column.index()] {
                    seen[column.index()] = true;
                    order.push(column);
                }
            }
            for column in LibraryColumn::ALL {
                if !seen[column.index()] {
                    order.push(column);
                }
            }
            layout.order.copy_from_slice(&order);
        }
        if let Some(widths) = map.get("widths").and_then(|value| value.as_object()) {
            for (id, value) in widths {
                let Some(column) = LibraryColumn::from_id(id) else {
                    continue;
                };
                if let Some(width) = value.as_f64() {
                    layout.set_width(column, width as f32);
                }
            }
        }
        layout
    }
}

/// T-03: a labeled header cell. The id is the column's anchor, so a test can
/// measure the label against the row cell it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowColumns {
    pub protocol: bool,
    pub upstream: bool,
    pub health: bool,
    pub group: bool,
    pub traffic: bool,
}

impl Default for RowColumns {
    fn default() -> Self {
        Self {
            protocol: false,
            upstream: false,
            health: true,
            group: true,
            traffic: false,
        }
    }
}

impl RowColumns {
    pub const ORDER: [&'static str; 5] = ["protocol", "upstream", "health", "group", "traffic"];

    pub fn parse(raw: &str) -> Self {
        if raw.trim().is_empty() {
            return Self::default();
        }
        let mut columns = Self {
            protocol: false,
            upstream: false,
            health: false,
            group: false,
            traffic: false,
        };
        for token in raw.split(',').map(str::trim) {
            match token {
                "protocol" => columns.protocol = true,
                "upstream" => columns.upstream = true,
                "health" => columns.health = true,
                "group" => columns.group = true,
                "traffic" => columns.traffic = true,
                _ => {}
            }
        }
        columns
    }

    pub fn display(self) -> String {
        if !self.protocol && !self.upstream && !self.health && !self.group && !self.traffic {
            return "none".to_string();
        }
        let mut names = Vec::with_capacity(5);
        for name in Self::ORDER {
            let enabled = match name {
                "protocol" => self.protocol,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str) -> ProxyConfig {
        ProxyConfig {
            id: id.to_string(),
            name: format!("row {id}"),
            protocol: "vless".to_string(),
            params: Secret::new("{}"),
            group: String::new(),
            enabled: true,
            bind_ip: "127.0.0.1".to_string(),
            port: 10801,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        }
    }

    fn sort(key: LibrarySortKey, direction: SortDirection) -> LibrarySort {
        LibrarySort::new(key, direction)
    }

    fn context<'a>(
        latency: &'a HashMap<String, u64>,
        out_ips: &'a HashMap<String, String>,
    ) -> SortContext<'a> {
        SortContext {
            latency,
            data_traffic: &EMPTY_PAIRS,
            totals: &EMPTY_PAIRS,
            out_ips,
        }
    }

    static EMPTY_PAIRS: std::sync::LazyLock<HashMap<String, (u64, u64)>> =
        std::sync::LazyLock::new(HashMap::new);

    /// Row ids are numeric strings, so "2" must precede "10" or the tie-break
    /// reorders a list between ticks.
    #[test]
    fn row_ids_order_numerically_then_by_bytes() {
        assert_eq!(row_id_order("2", "10"), Ordering::Less);
        assert_eq!(row_id_order("10", "2"), Ordering::Greater);
        assert_eq!(row_id_order("7", "7"), Ordering::Equal);
        assert_eq!(row_id_order("alpha", "beta"), Ordering::Less);
        assert_eq!(row_id_order("2", "alpha"), Ordering::Less, "numeric wins");
    }

    /// Missing values stay last in both directions, so toggling the direction
    /// never floats unmeasured rows to the top.
    #[test]
    fn missing_values_pin_last_in_both_directions() {
        let latency = HashMap::from([("1".to_string(), 40u64), ("2".to_string(), 90u64)]);
        let out_ips = HashMap::new();
        let context = context(&latency, &out_ips);
        let (fast, slow, unknown) = (row("1"), row("2"), row("3"));

        for direction in [SortDirection::Ascending, SortDirection::Descending] {
            let sort = sort(LibrarySortKey::Latency, direction);
            assert_eq!(
                compare_library_rows(&context, sort, &unknown, &fast),
                Ordering::Greater,
                "unmeasured sorts after measured ({direction:?})"
            );
            assert_eq!(
                compare_library_rows(&context, sort, &fast, &unknown),
                Ordering::Less
            );
        }
        assert_eq!(
            compare_library_rows(
                &context,
                sort(LibrarySortKey::Latency, SortDirection::Ascending),
                &fast,
                &slow
            ),
            Ordering::Less,
            "ascending ranks the lower latency first"
        );
        assert_eq!(
            compare_library_rows(
                &context,
                sort(LibrarySortKey::Latency, SortDirection::Descending),
                &fast,
                &slow
            ),
            Ordering::Greater,
            "descending flips the measured order"
        );
        assert_eq!(
            compare_library_rows(
                &context,
                sort(LibrarySortKey::Latency, SortDirection::Ascending),
                &row("3"),
                &row("12")
            ),
            Ordering::Less,
            "two unmeasured rows fall back to numeric id order"
        );
    }

    /// The egress-IP key ranks text and treats blank as unmeasured, matching
    /// the numeric keys' pinned-last rule.
    #[test]
    fn out_ip_key_ranks_text_and_pins_blanks() {
        let latency = HashMap::new();
        let out_ips = HashMap::from([
            ("1".to_string(), "203.0.113.9".to_string()),
            ("2".to_string(), "198.51.100.1".to_string()),
            ("3".to_string(), "   ".to_string()),
        ]);
        let context = context(&latency, &out_ips);
        let ascending = sort(LibrarySortKey::OutIp, SortDirection::Ascending);
        assert_eq!(
            compare_library_rows(&context, ascending, &row("2"), &row("1")),
            Ordering::Less,
            "text order, not numeric order"
        );
        assert_eq!(
            compare_library_rows(&context, ascending, &row("3"), &row("1")),
            Ordering::Greater,
            "blank egress counts as unmeasured"
        );
        assert_eq!(
            compare_library_rows(
                &context,
                sort(LibrarySortKey::OutIp, SortDirection::Descending),
                &row("2"),
                &row("1")
            ),
            Ordering::Greater,
            "descending flips the text order"
        );
    }

    /// Each sortable header offers its own key set, and `None` always leads so
    /// the menu can reset the sort.
    #[test]
    fn header_keys_split_between_traffic_and_health() {
        assert_eq!(
            sort_header_keys("traffic").first(),
            Some(&LibrarySortKey::None)
        );
        assert_eq!(
            sort_header_keys("health").first(),
            Some(&LibrarySortKey::None)
        );
        assert!(sort_header_keys("traffic").contains(&LibrarySortKey::TrafficTotal));
        assert!(!sort_header_keys("traffic").contains(&LibrarySortKey::Latency));
        assert!(sort_header_keys("health").contains(&LibrarySortKey::TypeSecurity));
        assert!(!sort_header_keys("health").contains(&LibrarySortKey::TrafficDown));
        assert_eq!(
            sort_header_keys("unknown-column"),
            sort_header_keys("health"),
            "only the traffic header owns the counters"
        );
    }

    /// The stored column set round-trips through its text form, and an empty
    /// value means the defaults rather than "no columns".
    #[test]
    fn row_columns_round_trip() {
        assert_eq!(RowColumns::parse(""), RowColumns::default());
        assert_eq!(RowColumns::parse("  "), RowColumns::default());
        let parsed = RowColumns::parse("traffic, group ,bogus");
        assert!(parsed.traffic && parsed.group && !parsed.upstream && !parsed.health);
        assert_eq!(parsed.display(), "group,traffic", "display follows ORDER");
        assert_eq!(RowColumns::parse(&parsed.display()), parsed, "round trip");
        let none = RowColumns::parse("none");
        assert_eq!(none.display(), "none", "no known token reads as none");
        assert_eq!(RowColumns::parse(&none.display()), none);
    }

    #[test]
    fn library_columns_widths_clamp_and_reset() {
        let mut layout = LibraryColumns::default();
        assert_eq!(layout.width(LibraryColumn::Health), RowMetrics::HEALTH);
        assert!(!layout.is_custom(LibraryColumn::Health));

        layout.set_width(LibraryColumn::Health, 1.);
        assert_eq!(
            layout.width(LibraryColumn::Health),
            LibraryColumn::Health.min_width(),
            "a drag below the floor stops at the column minimum"
        );
        assert!(layout.is_custom(LibraryColumn::Health));

        layout.set_width(LibraryColumn::Health, 10_000.);
        assert_eq!(
            layout.width(LibraryColumn::Health),
            LibraryColumn::MAX_WIDTH,
            "a drag past the ceiling stops at the shared maximum"
        );

        layout.set_width(LibraryColumn::Health, 132.4);
        assert_eq!(layout.width(LibraryColumn::Health), 132.);
        layout.reset_width(LibraryColumn::Health);
        assert_eq!(layout.width(LibraryColumn::Health), RowMetrics::HEALTH);
        assert!(!layout.is_custom(LibraryColumn::Health));
    }

    #[test]
    fn library_columns_round_trip_and_tolerate_junk() {
        let mut layout = LibraryColumns::default();
        layout.set_width(LibraryColumn::Endpoint, 180.);
        assert!(layout.move_column(LibraryColumn::Traffic, LibraryColumn::Group, false));
        let parsed = LibraryColumns::from_json(&layout.to_json());
        assert_eq!(parsed, layout, "widths and order round-trip");
        assert_eq!(parsed.order()[2], LibraryColumn::Traffic);

        assert_eq!(
            LibraryColumns::from_json("not json"),
            LibraryColumns::default()
        );
        let partial = LibraryColumns::from_json(
            r#"{"widths":{"bogus":120,"health":90},"order":["traffic","traffic","bogus"]}"#,
        );
        assert_eq!(partial.width(LibraryColumn::Health), 90.);
        assert_eq!(partial.order()[0], LibraryColumn::Traffic);
        assert_eq!(
            partial.order()[1],
            LibraryColumn::Endpoint,
            "omitted columns append in canonical order"
        );
        assert_eq!(
            partial.order().len(),
            LibraryColumn::ALL.len(),
            "the order always holds every movable column once"
        );
    }

    #[test]
    fn library_columns_reorder_keeps_every_column() {
        let mut layout = LibraryColumns::default();
        let original = *layout.order();
        assert!(layout.move_column(LibraryColumn::Protocol, LibraryColumn::Endpoint, true));
        assert_eq!(
            layout.order(),
            &[
                LibraryColumn::Endpoint,
                LibraryColumn::Protocol,
                LibraryColumn::Health,
                LibraryColumn::Group,
                LibraryColumn::Upstream,
                LibraryColumn::Traffic,
            ],
            "move after the target"
        );
        assert!(!layout.move_column(LibraryColumn::Protocol, LibraryColumn::Protocol, false));
        layout.reset();
        assert_eq!(layout.order(), &original);
    }
}
