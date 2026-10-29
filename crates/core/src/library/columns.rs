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
