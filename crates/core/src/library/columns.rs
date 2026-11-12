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
    /// The upstream host:port each row dials, as text.
    pub upstreams: &'a HashMap<String, String>,
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

    fn text<'a>(&'a self, key: LibrarySortKey, row: &'a ProxyConfig) -> Option<&'a str> {
        let text = match key {
            LibrarySortKey::Name => row.name.as_str(),
            LibrarySortKey::Group => row.group.as_str(),
            LibrarySortKey::Protocol => row.protocol.as_str(),
            LibrarySortKey::Upstream => self.upstreams.get(&row.id).map(String::as_str)?,
            _ => return None,
        };
        (!text.trim().is_empty()).then_some(text)
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

fn compare_optional_text(
    left: Option<&str>,
    right: Option<&str>,
    descending: bool,
    a: &str,
    b: &str,
) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => {
            let order = left.cmp(right);
            (if descending { order.reverse() } else { order }).then_with(|| row_id_order(a, b))
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => row_id_order(a, b),
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
        return compare_optional_text(
            context.out_ip(&a.id),
            context.out_ip(&b.id),
            descending,
            &a.id,
            &b.id,
        );
    }
    if sort.key == LibrarySortKey::Endpoint {
        let order = a.bind_ip.cmp(&b.bind_ip).then_with(|| a.port.cmp(&b.port));
        return (if descending { order.reverse() } else { order })
            .then_with(|| row_id_order(&a.id, &b.id));
    }
    if matches!(
        sort.key,
        LibrarySortKey::Name
            | LibrarySortKey::Group
            | LibrarySortKey::Protocol
            | LibrarySortKey::Upstream
    ) {
        return compare_optional_text(
            context.text(sort.key, a),
            context.text(sort.key, b),
            descending,
            &a.id,
            &b.id,
        );
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

/// The seven signed columns, in their default order. Locked means "cannot be
/// hidden"; order and width stay free for every column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnId {
    Name,
    Endpoint,
    Health,
    Group,
    Protocol,
    Upstream,
    Traffic,
}

impl ColumnId {
    pub const ALL: [ColumnId; 7] = [
        ColumnId::Name,
        ColumnId::Endpoint,
        ColumnId::Health,
        ColumnId::Group,
        ColumnId::Protocol,
        ColumnId::Upstream,
        ColumnId::Traffic,
    ];

    /// The widest a dragged column may become. Wider than any cell needs and
    /// narrow enough that one column cannot swallow the whole table alone.
    pub const MAX_WIDTH: f32 = 480.;

    pub fn id(self) -> &'static str {
        match self {
            ColumnId::Name => "name",
            ColumnId::Endpoint => "endpoint",
            ColumnId::Health => "health",
            ColumnId::Group => "group",
            ColumnId::Protocol => "protocol",
            ColumnId::Upstream => "upstream",
            ColumnId::Traffic => "traffic",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ColumnId::Name => "Name",
            ColumnId::Endpoint => "Endpoint",
            ColumnId::Health => "Health",
            ColumnId::Group => "Group",
            ColumnId::Protocol => "Protocol",
            ColumnId::Upstream => "Upstream",
            ColumnId::Traffic => "Traffic",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|column| column.id() == id)
    }

    /// The canonical slot used by the fixed-size layout arrays.
    pub fn index(self) -> usize {
        match self {
            ColumnId::Name => 0,
            ColumnId::Endpoint => 1,
            ColumnId::Health => 2,
            ColumnId::Group => 3,
            ColumnId::Protocol => 4,
            ColumnId::Upstream => 5,
            ColumnId::Traffic => 6,
        }
    }

    pub fn default_width(self) -> f32 {
        match self {
            ColumnId::Name => RowMetrics::NAME_MIN,
            ColumnId::Endpoint => RowMetrics::ENDPOINT,
            ColumnId::Health => RowMetrics::HEALTH,
            ColumnId::Group => RowMetrics::GROUP,
            ColumnId::Protocol => RowMetrics::PROTOCOL,
            ColumnId::Upstream => RowMetrics::UPSTREAM,
            ColumnId::Traffic => RowMetrics::TRAFFIC,
        }
    }

    /// A header must still show its own name at the smallest size.
    pub fn min_width(self) -> f32 {
        match self {
            ColumnId::Name => RowMetrics::NAME_MIN,
            ColumnId::Endpoint => 96.,
            ColumnId::Health => 84.,
            ColumnId::Group => 64.,
            ColumnId::Protocol => 72.,
            ColumnId::Upstream => 90.,
            ColumnId::Traffic => 84.,
        }
    }

    pub fn locked(self) -> bool {
        matches!(self, ColumnId::Name | ColumnId::Endpoint)
    }

    /// The key a header click sorts by. Health reads the probe sample and
    /// Traffic the accumulated counters; the rest rank their own text.
    pub fn sort_key(self) -> LibrarySortKey {
        match self {
            ColumnId::Name => LibrarySortKey::Name,
            ColumnId::Endpoint => LibrarySortKey::Endpoint,
            ColumnId::Health => LibrarySortKey::Latency,
            ColumnId::Group => LibrarySortKey::Group,
            ColumnId::Protocol => LibrarySortKey::Protocol,
            ColumnId::Upstream => LibrarySortKey::Upstream,
            ColumnId::Traffic => LibrarySortKey::TrafficTotal,
        }
    }
}

/// The one column layout: order, visibility and widths, with Name and Endpoint
/// locked visible. `widths` holds the user's explicit overrides; `None` means
/// the column's computed default (and, for Name, the flex behaviour).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnLayout {
    order: [ColumnId; 7],
    hidden: [bool; 7],
    widths: [Option<f32>; 7],
}

impl Default for ColumnLayout {
    fn default() -> Self {
        let mut hidden = [false; 7];
        hidden[ColumnId::Protocol.index()] = true;
        hidden[ColumnId::Upstream.index()] = true;
        hidden[ColumnId::Traffic.index()] = true;
        Self {
            order: ColumnId::ALL,
            hidden,
            widths: [None; 7],
        }
    }
}

impl ColumnLayout {
    pub fn order(&self) -> &[ColumnId; 7] {
        &self.order
    }

    pub fn is_visible(&self, column: ColumnId) -> bool {
        !self.hidden[column.index()]
    }

    pub fn visible_count(&self) -> usize {
        self.hidden.iter().filter(|hidden| !**hidden).count()
    }

    /// The effective width: the user's override clamped to the column's floor,
    /// or the computed default when untouched.
    pub fn width(&self, column: ColumnId) -> f32 {
        let raw = self.widths[column.index()].unwrap_or(column.default_width());
        if raw.is_finite() {
            raw.clamp(column.min_width(), ColumnId::MAX_WIDTH)
        } else {
            column.default_width()
        }
    }

    pub fn is_custom(&self, column: ColumnId) -> bool {
        self.widths[column.index()].is_some()
    }

    pub fn set_width(&mut self, column: ColumnId, width: f32) {
        if !width.is_finite() {
            return;
        }
        self.widths[column.index()] =
            Some(width.clamp(column.min_width(), ColumnId::MAX_WIDTH).round());
    }

    pub fn reset_width(&mut self, column: ColumnId) {
        self.widths[column.index()] = None;
    }

    pub fn reset_widths(&mut self) {
        self.widths = [None; 7];
    }

    pub fn reset_order(&mut self) {
        self.order = ColumnId::ALL;
    }

    pub fn reset_visibility(&mut self) {
        self.hidden = Self::default().hidden;
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Locked columns cannot be hidden. Returns whether the state changed.
    pub fn set_hidden(&mut self, column: ColumnId, hidden: bool) -> bool {
        if hidden && column.locked() {
            return false;
        }
        if self.hidden[column.index()] == hidden {
            return false;
        }
        self.hidden[column.index()] = hidden;
        true
    }

    /// Move `source` next to `target`. Locked columns participate normally.
    pub fn move_column(&mut self, source: ColumnId, target: ColumnId, after: bool) -> bool {
        if source == target {
            return false;
        }
        let mut rest: Vec<ColumnId> = self
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

    /// Size one column to its measured content. The explicit width stops Name
    /// flexing, which is the signed behaviour.
    pub fn fit(&mut self, column: ColumnId, measured_width: f32) {
        if !measured_width.is_finite() {
            return;
        }
        self.set_width(column, measured_width);
    }

    /// Size every visible column in one operation. Hidden columns keep their
    /// previous widths.
    pub fn fit_all(&mut self, mut measure: impl FnMut(ColumnId) -> f32) {
        for column in self.order {
            if self.hidden[column.index()] {
                continue;
            }
            let width = measure(column);
            self.fit(column, width);
        }
    }

    /// The widths the table paints. Name (when unset) absorbs the leftover
    /// after the fixed columns, the gaps and both reserves. It never goes below
    /// its minimum: a narrow table scrolls instead.
    pub fn effective_widths(&self, table_width: f32) -> [f32; 7] {
        let mut widths = [0.; 7];
        for column in ColumnId::ALL {
            widths[column.index()] = self.width(column);
        }
        if self.widths[ColumnId::Name.index()].is_none() {
            let mut fixed = 0.;
            let mut other_visible = 0usize;
            for column in ColumnId::ALL {
                if column == ColumnId::Name || self.hidden[column.index()] {
                    continue;
                }
                fixed += widths[column.index()];
                other_visible += 1;
            }
            let gaps = other_visible as f32 * RowMetrics::GAP;
            let available = table_width - RowMetrics::LEAD - RowMetrics::ACTIONS - fixed - gaps;
            widths[ColumnId::Name.index()] = available.max(ColumnId::Name.min_width());
        }
        widths
    }

    /// The signed table content width: both reserves, every visible width and
    /// the gaps between them.
    pub fn content_width(&self, table_width: f32) -> f32 {
        let widths = self.effective_widths(table_width);
        let visible = self.visible_count();
        let sum: f32 = ColumnId::ALL
            .iter()
            .filter(|column| !self.hidden[column.index()])
            .map(|column| widths[column.index()])
            .sum();
        let gaps = visible.saturating_sub(1) as f32 * RowMetrics::GAP;
        RowMetrics::LEAD + RowMetrics::ACTIONS + sum + gaps
    }

    /// Canonical meta payload. Widths are keyed by id so a later column
    /// insertion cannot shift an existing override onto its neighbour.
    pub fn to_json(&self) -> String {
        let widths: serde_json::Map<String, serde_json::Value> = ColumnId::ALL
            .iter()
            .map(|column| {
                let value = self.widths[column.index()]
                    .map(|width| serde_json::json!(width))
                    .unwrap_or(serde_json::Value::Null);
                (column.id().to_string(), value)
            })
            .collect();
        let order: Vec<&str> = self.order.iter().map(|column| column.id()).collect();
        let hidden: Vec<&str> = ColumnId::ALL
            .iter()
            .filter(|column| self.hidden[column.index()])
            .map(|column| column.id())
            .collect();
        serde_json::json!({
            "version": 2,
            "order": order,
            "hidden": hidden,
            "widths": widths,
        })
        .to_string()
    }

    /// Tolerant read: unknown ids, duplicates, malformed numbers and future
    /// versions are ignored or fall back to defaults; the lock is enforced
    /// after parsing.
    pub fn from_json(raw: &str) -> Self {
        Self::from_json_with_legacy(raw, None)
    }

    /// Read the persisted layout, using the legacy `library_columns` string
    /// only when the layout key is absent or is a version-1 payload.
    pub fn from_store(store: &Store) -> Self {
        let raw = store
            .get_meta(Store::META_LIBRARY_COLUMN_LAYOUT)
            .ok()
            .flatten();
        let legacy = store.library_columns_raw();
        match raw {
            Some(raw) => Self::from_json_with_legacy(&raw, legacy.as_deref()),
            None => match legacy {
                Some(raw) => Self::seed_from_legacy(&raw),
                None => Self::default(),
            },
        }
    }

    /// Seed visibility from the retired comma token list. Name and Endpoint
    /// stay visible; order and widths keep their defaults.
    pub fn seed_from_legacy(raw: &str) -> Self {
        let mut layout = Self::default();
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return layout;
        }
        let tokens: Vec<&str> = trimmed
            .split(',')
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .collect();
        let known = tokens
            .iter()
            .any(|token| ColumnId::from_id(token).is_some());
        for column in [
            ColumnId::Health,
            ColumnId::Group,
            ColumnId::Protocol,
            ColumnId::Upstream,
            ColumnId::Traffic,
        ] {
            layout.hidden[column.index()] = if known {
                !tokens.contains(&column.id())
            } else {
                true
            };
        }
        layout
    }

    fn from_json_with_legacy(raw: &str, legacy: Option<&str>) -> Self {
        let Ok(serde_json::Value::Object(map)) = serde_json::from_str(raw.trim()) else {
            return Self::default();
        };
        let version = match map.get("version") {
            Some(value) => match value.as_u64() {
                Some(version) => Some(version),
                None => return Self::default(),
            },
            None if map.contains_key("hidden") => Some(2),
            None => Some(1),
        };
        let mut layout = match version {
            Some(2) => Self::parse_v2(&map),
            Some(1) => Self::parse_v1(&map, legacy),
            _ => return Self::default(),
        };
        layout.enforce_locks();
        layout
    }

    fn parse_v2(map: &serde_json::Map<String, serde_json::Value>) -> Self {
        let mut hidden = [false; 7];
        if let Some(entries) = map.get("hidden").and_then(|value| value.as_array()) {
            for entry in entries {
                if let Some(column) = entry.as_str().and_then(ColumnId::from_id) {
                    hidden[column.index()] = true;
                }
            }
        } else {
            hidden = Self::default().hidden;
        }
        let mut layout = Self {
            order: Self::parse_order(map.get("order")),
            hidden,
            widths: [None; 7],
        };
        layout.parse_widths(map);
        layout
    }

    fn parse_v1(map: &serde_json::Map<String, serde_json::Value>, legacy: Option<&str>) -> Self {
        let mut layout = Self::default();
        if let Some(entries) = map.get("order").and_then(|value| value.as_array()) {
            let mut seen = [false; 7];
            let mut rest = Vec::with_capacity(6);
            for entry in entries {
                let Some(column) = entry.as_str().and_then(ColumnId::from_id) else {
                    continue;
                };
                if column == ColumnId::Name || seen[column.index()] {
                    continue;
                }
                seen[column.index()] = true;
                rest.push(column);
            }
            for column in ColumnId::ALL {
                if column != ColumnId::Name && !seen[column.index()] {
                    rest.push(column);
                }
            }
            layout.order[0] = ColumnId::Name;
            layout.order[1..].copy_from_slice(&rest);
        }
        layout.hidden = match legacy {
            Some(raw) => Self::seed_from_legacy(raw).hidden,
            None => Self::default().hidden,
        };
        layout.parse_widths(map);
        layout
    }

    fn parse_order(value: Option<&serde_json::Value>) -> [ColumnId; 7] {
        let mut seen = [false; 7];
        let mut order = Vec::with_capacity(7);
        if let Some(entries) = value.and_then(|value| value.as_array()) {
            for entry in entries {
                let Some(column) = entry.as_str().and_then(ColumnId::from_id) else {
                    continue;
                };
                if !seen[column.index()] {
                    seen[column.index()] = true;
                    order.push(column);
                }
            }
        }
        for column in ColumnId::ALL {
            if !seen[column.index()] {
                order.push(column);
            }
        }
        let mut result = ColumnId::ALL;
        result.copy_from_slice(&order);
        result
    }

    fn parse_widths(&mut self, map: &serde_json::Map<String, serde_json::Value>) {
        let Some(widths) = map.get("widths").and_then(|value| value.as_object()) else {
            return;
        };
        for (id, value) in widths {
            let Some(column) = ColumnId::from_id(id) else {
                continue;
            };
            if value.is_null() {
                self.widths[column.index()] = None;
            } else if let Some(width) = value.as_f64() {
                self.set_width(column, width as f32);
            }
        }
    }

    fn enforce_locks(&mut self) {
        self.hidden[ColumnId::Name.index()] = false;
        self.hidden[ColumnId::Endpoint.index()] = false;
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
        upstreams: &'a HashMap<String, String>,
    ) -> SortContext<'a> {
        SortContext {
            latency,
            data_traffic: &EMPTY_PAIRS,
            totals: &EMPTY_PAIRS,
            out_ips,
            upstreams,
        }
    }

    static EMPTY_PAIRS: std::sync::LazyLock<HashMap<String, (u64, u64)>> =
        std::sync::LazyLock::new(HashMap::new);

    #[test]
    fn defaults_are_name_endpoint_health_group_visible() {
        let layout = ColumnLayout::default();
        assert_eq!(layout.order(), &ColumnId::ALL);
        assert!(layout.is_visible(ColumnId::Name));
        assert!(layout.is_visible(ColumnId::Endpoint));
        assert!(layout.is_visible(ColumnId::Health));
        assert!(layout.is_visible(ColumnId::Group));
        assert!(!layout.is_visible(ColumnId::Protocol));
        assert!(!layout.is_visible(ColumnId::Upstream));
        assert!(!layout.is_visible(ColumnId::Traffic));
        assert_eq!(layout.visible_count(), 4);
        assert!(ColumnId::Name.locked());
        assert!(ColumnId::Endpoint.locked());
        assert!(!ColumnId::Health.locked());
    }

    #[test]
    fn locked_columns_cannot_be_hidden() {
        let mut layout = ColumnLayout::default();
        assert!(!layout.set_hidden(ColumnId::Name, true));
        assert!(!layout.set_hidden(ColumnId::Endpoint, true));
        assert!(layout.is_visible(ColumnId::Name));
        assert!(layout.is_visible(ColumnId::Endpoint));
    }

    #[test]
    fn locked_columns_can_move() {
        let mut layout = ColumnLayout::default();
        assert!(layout.move_column(ColumnId::Endpoint, ColumnId::Name, false));
        assert_eq!(layout.order()[0], ColumnId::Endpoint);
        assert_eq!(layout.order()[1], ColumnId::Name);
        assert!(layout.is_visible(ColumnId::Name));
        assert!(layout.is_visible(ColumnId::Endpoint));
    }

    #[test]
    fn move_column_handles_first_last_and_unknown_targets() {
        let mut layout = ColumnLayout::default();
        assert!(layout.move_column(ColumnId::Traffic, ColumnId::Name, false));
        assert_eq!(layout.order()[0], ColumnId::Traffic);
        assert!(layout.move_column(ColumnId::Traffic, ColumnId::Name, true));
        assert_eq!(layout.order()[1], ColumnId::Traffic);
        assert!(!layout.move_column(ColumnId::Traffic, ColumnId::Traffic, false));
        let original = *layout.order();
        assert!(!layout.move_column(ColumnId::Traffic, ColumnId::Traffic, true));
        assert_eq!(layout.order(), &original);
    }

    #[test]
    fn size_to_fit_clamps_to_min_and_max_and_ignores_non_finite() {
        let mut layout = ColumnLayout::default();
        layout.fit(ColumnId::Health, 1.);
        assert_eq!(layout.width(ColumnId::Health), ColumnId::Health.min_width());
        layout.fit(ColumnId::Health, 10_000.);
        assert_eq!(layout.width(ColumnId::Health), ColumnId::MAX_WIDTH);
        layout.fit(ColumnId::Health, f32::NAN);
        assert_eq!(layout.width(ColumnId::Health), ColumnId::MAX_WIDTH);
        layout.fit(ColumnId::Health, f32::INFINITY);
        assert_eq!(layout.width(ColumnId::Health), ColumnId::MAX_WIDTH);
        layout.fit(ColumnId::Health, 132.4);
        assert_eq!(layout.width(ColumnId::Health), 132.);
    }

    #[test]
    fn name_flexes_only_when_its_width_is_unset() {
        let layout = ColumnLayout::default();
        let table = 900.;
        let widths = layout.effective_widths(table);
        assert_eq!(layout.content_width(table), table);
        assert!(widths[ColumnId::Name.index()] > ColumnId::Name.min_width());

        let mut fixed = layout;
        fixed.set_width(ColumnId::Name, 220.);
        let widths = fixed.effective_widths(table);
        assert_eq!(widths[ColumnId::Name.index()], 220.);
        let content = fixed.content_width(table);
        assert!(content < table);
        assert!(content > 220.);
    }

    #[test]
    fn content_width_adds_lead_gap_and_trailing_reserve() {
        let layout = ColumnLayout::default();
        let table = 700.;
        let widths = layout.effective_widths(table);
        let visible = layout.visible_count();
        let sum: f32 = ColumnId::ALL
            .iter()
            .filter(|column| layout.is_visible(**column))
            .map(|column| widths[column.index()])
            .sum();
        assert_eq!(
            layout.content_width(table),
            RowMetrics::LEAD + RowMetrics::ACTIONS + sum + RowMetrics::GAP * (visible - 1) as f32
        );
    }

    #[test]
    fn reset_verbs_touch_only_their_dimension() {
        let mut layout = ColumnLayout::default();
        layout.set_width(ColumnId::Health, 140.);
        assert!(layout.move_column(ColumnId::Traffic, ColumnId::Name, false));
        assert!(layout.set_hidden(ColumnId::Group, true));
        assert!(layout.set_hidden(ColumnId::Traffic, false));

        let mut widths_only = layout;
        widths_only.reset_widths();
        assert_eq!(
            widths_only.width(ColumnId::Health),
            ColumnId::Health.default_width()
        );
        assert_eq!(widths_only.order(), layout.order());
        assert!(!widths_only.is_visible(ColumnId::Group));
        assert!(widths_only.is_visible(ColumnId::Traffic));

        let mut order_only = layout;
        order_only.reset_order();
        assert_eq!(order_only.order(), &ColumnId::ALL);
        assert_eq!(order_only.width(ColumnId::Health), 140.);
        assert!(!order_only.is_visible(ColumnId::Group));

        let mut visibility_only = layout;
        visibility_only.reset_visibility();
        assert_eq!(visibility_only.order(), layout.order());
        assert_eq!(visibility_only.width(ColumnId::Health), 140.);
        assert!(visibility_only.is_visible(ColumnId::Group));
        assert!(!visibility_only.is_visible(ColumnId::Traffic));

        let mut all = layout;
        all.reset();
        assert_eq!(all, ColumnLayout::default());
    }

    #[test]
    fn json_round_trip_preserves_order_hidden_and_widths() {
        let mut layout = ColumnLayout::default();
        layout.set_width(ColumnId::Endpoint, 176.);
        layout.set_width(ColumnId::Name, 240.);
        assert!(layout.move_column(ColumnId::Traffic, ColumnId::Group, false));
        assert!(layout.set_hidden(ColumnId::Group, true));
        let parsed = ColumnLayout::from_json(&layout.to_json());
        assert_eq!(parsed, layout);
    }

    #[test]
    fn corrupt_json_falls_back_to_defaults_and_never_hides_locked_columns() {
        assert_eq!(ColumnLayout::from_json("not json"), ColumnLayout::default());
        assert_eq!(
            ColumnLayout::from_json(r#"{"version":99,"hidden":["name","endpoint"]}"#),
            ColumnLayout::default()
        );
        let parsed = ColumnLayout::from_json(
            r#"{"version":2,"order":["endpoint","name"],"hidden":["name","endpoint","group"],"widths":{"name":10,"endpoint":9999}}"#,
        );
        assert!(parsed.is_visible(ColumnId::Name));
        assert!(parsed.is_visible(ColumnId::Endpoint));
        assert!(!parsed.is_visible(ColumnId::Group));
        assert_eq!(parsed.width(ColumnId::Name), ColumnId::Name.min_width());
        assert_eq!(parsed.width(ColumnId::Endpoint), ColumnId::MAX_WIDTH);
    }

    #[test]
    fn version_one_order_migrates_with_name_first() {
        let parsed = ColumnLayout::from_json(
            r#"{"widths":{"health":120},"order":["traffic","endpoint","health","group","protocol","upstream"]}"#,
        );
        assert_eq!(parsed.order()[0], ColumnId::Name);
        assert_eq!(parsed.order()[1], ColumnId::Traffic);
        assert_eq!(parsed.order()[2], ColumnId::Endpoint);
        assert_eq!(parsed.width(ColumnId::Health), 120.);
        assert!(parsed.is_visible(ColumnId::Name));
        assert!(parsed.is_visible(ColumnId::Endpoint));
    }

    #[test]
    fn legacy_columns_string_seeds_visibility_only_when_layout_is_absent() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store
            .set_meta(Store::META_LIBRARY_COLUMNS, "protocol,traffic")
            .expect("legacy write");
        let layout = ColumnLayout::from_store(&store);
        assert!(layout.is_visible(ColumnId::Protocol));
        assert!(layout.is_visible(ColumnId::Traffic));
        assert!(!layout.is_visible(ColumnId::Health));
        assert!(!layout.is_visible(ColumnId::Group));
        assert!(!layout.is_visible(ColumnId::Upstream));
        assert_eq!(layout.order(), &ColumnId::ALL);
        assert!(!layout.is_custom(ColumnId::Name));

        store
            .set_meta(Store::META_LIBRARY_COLUMN_LAYOUT, "not json")
            .expect("corrupt layout write");
        assert_eq!(
            ColumnLayout::from_store(&store),
            ColumnLayout::default(),
            "a corrupt new layout ignores the legacy seed"
        );
    }

    #[test]
    fn unknown_and_duplicate_ids_are_ignored_and_missing_ids_appended() {
        let parsed = ColumnLayout::from_json(
            r#"{"version":2,"order":["traffic","traffic","bogus","name"],"hidden":["bogus","group","group"],"widths":{"bogus":120}}"#,
        );
        assert_eq!(parsed.order()[0], ColumnId::Traffic);
        assert_eq!(parsed.order()[1], ColumnId::Name);
        assert_eq!(parsed.order().len(), ColumnId::ALL.len());
        assert!(!parsed.is_visible(ColumnId::Group));
        assert!(parsed.is_visible(ColumnId::Endpoint));
    }

    #[test]
    fn text_columns_sort_with_blanks_last() {
        let latency = HashMap::new();
        let out_ips = HashMap::new();
        let upstreams = HashMap::from([
            ("1".to_string(), "z.example:443".to_string()),
            ("2".to_string(), "a.example:443".to_string()),
        ]);
        let context = context(&latency, &out_ips, &upstreams);
        let mut fast = row("1");
        fast.name = "Zulu".to_string();
        let mut slow = row("2");
        slow.name = "Alpha".to_string();
        assert_eq!(
            compare_library_rows(
                &context,
                sort(LibrarySortKey::Name, SortDirection::Ascending),
                &slow,
                &fast
            ),
            Ordering::Less,
            "names sort as text"
        );
        assert_eq!(
            compare_library_rows(
                &context,
                sort(LibrarySortKey::Upstream, SortDirection::Ascending),
                &slow,
                &fast
            ),
            Ordering::Less,
            "upstreams read the resolved cache"
        );
        let mut blank = row("3");
        blank.name.clear();
        assert_eq!(
            compare_library_rows(
                &context,
                sort(LibrarySortKey::Name, SortDirection::Descending),
                &blank,
                &fast
            ),
            Ordering::Greater,
            "blank names stay pinned last"
        );
    }

    #[test]
    fn endpoint_sort_ranks_bind_then_port() {
        let latency = HashMap::new();
        let out_ips = HashMap::new();
        let upstreams = HashMap::new();
        let context = context(&latency, &out_ips, &upstreams);
        let mut first = row("1");
        first.bind_ip = "127.0.0.1".to_string();
        first.port = 10801;
        let mut second = row("2");
        second.bind_ip = "127.0.0.1".to_string();
        second.port = 10802;
        assert_eq!(
            compare_library_rows(
                &context,
                sort(LibrarySortKey::Endpoint, SortDirection::Ascending),
                &first,
                &second
            ),
            Ordering::Less
        );
    }

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
        let upstreams = HashMap::new();
        let context = context(&latency, &out_ips, &upstreams);
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
        let upstreams = HashMap::new();
        let context = context(&latency, &out_ips, &upstreams);
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
}
