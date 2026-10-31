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

/// T-03: a labeled header cell. The id is the column's anchor, so a test can
/// measure the label against the row cell it names.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowColumns {
    pub upstream: bool,
    pub health: bool,
    pub group: bool,
    pub traffic: bool,
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
    pub const ORDER: [&'static str; 4] = ["upstream", "health", "group", "traffic"];

    pub fn parse(raw: &str) -> Self {
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

    pub fn display(self) -> String {
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
}
