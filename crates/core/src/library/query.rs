use std::collections::{BTreeSet, HashMap, HashSet};

use crate::*;

/// The library scope. The nav is the only place a group or source is chosen
/// (R1); the query composes it with the facets instead of mirroring it into
/// `FilterQuery`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum LibraryScope {
    #[default]
    All,
    Group(String),
    /// The store's default group, labelled `Ungrouped` in the nav.
    Ungrouped,
    Source(String),
}

impl LibraryScope {
    pub fn contains(&self, row: &ProxyConfig) -> bool {
        match self {
            Self::All => true,
            Self::Group(name) => &row.group == name,
            Self::Ungrouped => row.group == DEFAULT_GROUP,
            Self::Source(id) => row.subscription_id.as_deref() == Some(id.as_str()),
        }
    }
}

/// The scope predicate as a free function, for callers that keep the scope
/// beside the row list rather than inside the query.
pub fn scope_matches(scope: &LibraryScope, row: &ProxyConfig) -> bool {
    scope.contains(row)
}

/// The five values the status multi-select offers. This is the pure spelling
/// of the same lamp vocabulary the row paints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StatusFacet {
    Enabled,
    Disabled,
    Running,
    Dead,
    Stopped,
}

impl StatusFacet {
    pub const ALL: [StatusFacet; 5] = [
        StatusFacet::Enabled,
        StatusFacet::Disabled,
        StatusFacet::Running,
        StatusFacet::Dead,
        StatusFacet::Stopped,
    ];

    pub fn matches(self, row: &ProxyConfig, facts: &QueryFacts<'_>) -> bool {
        let running = facts.running.contains(&row.id);
        let dead = facts.crashed.contains(&row.id)
            || facts.health.get(&row.id).map(|text| text.trim()) == Some("dead");
        match self {
            Self::Enabled => row.enabled,
            Self::Disabled => !row.enabled,
            Self::Running => running && !dead,
            Self::Dead => running && dead,
            Self::Stopped => !running,
        }
    }
}

/// The read-only mirrors matching and counting need. The app keeps these
/// fresh on its normal ticks; the query never polls and never writes.
pub struct QueryFacts<'a> {
    pub running: &'a HashSet<String>,
    pub health: &'a HashMap<String, String>,
    pub crashed: &'a HashSet<String>,
    pub latency: &'a HashMap<String, u64>,
}

/// The one query. OR inside a set, AND across sets and search.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FilterQuery {
    pub search: String,
    pub statuses: BTreeSet<StatusFacet>,
    pub protocols: BTreeSet<String>,
    pub latency: LatencyFacet,
    pub binds: BTreeSet<String>,
    pub port: Option<PortPredicate>,
    pub custom_endpoint: bool,
}

/// One facet a count excludes. `without` is the one place self-exclusion is
/// spelled, so a count can never accidentally include its own selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Facet {
    Status,
    Protocol,
    Latency,
    Bind,
    Port,
    CustomEndpoint,
}

impl FilterQuery {
    /// Write the search text. A `port=` token is the compatibility shortcut:
    /// it seeds the port facet and stays in the field bytes. Typing a name
    /// while a popover-set port is live leaves that port alone; removing the
    /// token (or clearing the field) releases it.
    pub fn set_search(&mut self, text: impl Into<String>) {
        let text = text.into();
        let (next_port, _) = split_port_query(&text);
        let (current_port, _) = split_port_query(&self.search);
        if next_port.is_some() || current_port.is_some() {
            self.port = next_port;
        }
        self.search = text;
    }

    /// The free-text part of the search, with a valid `port=` token removed.
    pub fn search_text(&self) -> String {
        split_port_query(&self.search).1
    }

    pub fn is_active(&self) -> bool {
        !self.search.trim().is_empty()
            || !self.statuses.is_empty()
            || !self.protocols.is_empty()
            || self.latency != LatencyFacet::default()
            || !self.binds.is_empty()
            || self.port.is_some()
            || self.custom_endpoint
    }

    /// R1/R14: exactly the four long-tail controls, one each.
    pub fn long_tail_count(&self) -> usize {
        let latency = self.latency != LatencyFacet::default();
        latency as usize
            + (!self.binds.is_empty()) as usize
            + self.port.is_some() as usize
            + self.custom_endpoint as usize
    }

    pub fn without(&self, facet: Facet) -> Self {
        let mut query = self.clone();
        match facet {
            Facet::Status => query.statuses.clear(),
            Facet::Protocol => query.protocols.clear(),
            Facet::Latency => query.latency = LatencyFacet::default(),
            Facet::Bind => query.binds.clear(),
            Facet::Port => query.port = None,
            Facet::CustomEndpoint => query.custom_endpoint = false,
        }
        query
    }
}

/// The rows the query keeps, in library order. Search is one substring pass
/// over `RowSearchIndex`; the `port=` token never participates in free text.
pub fn filter(
    rows: &[ProxyConfig],
    search_index: &[RowSearchIndex],
    facts: &QueryFacts<'_>,
    scope: &LibraryScope,
    query: &FilterQuery,
) -> Vec<usize> {
    let lowered = query.search_text().trim().to_lowercase();
    let mut visible = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if !scope.contains(row) {
            continue;
        }
        let search_hit = search_index
            .get(index)
            .is_some_and(|blob| blob.matches_lowered(&lowered));
        if !search_hit {
            continue;
        }
        if !query.statuses.is_empty()
            && !query
                .statuses
                .iter()
                .any(|status| status.matches(row, facts))
        {
            continue;
        }
        if !query.protocols.is_empty() && !query.protocols.contains(&row.protocol) {
            continue;
        }
        if !query.latency.matches(facts.latency.get(&row.id).copied()) {
            continue;
        }
        if !query.binds.is_empty() && !query.binds.contains(&row.bind_ip) {
            continue;
        }
        if let Some(port) = query.port {
            if !port.matches(row.port) {
                continue;
            }
        }
        if query.custom_endpoint && row.bind_ip.trim() == DEFAULT_BIND {
            continue;
        }
        visible.push(index);
    }
    visible
}

pub fn visible_count(
    rows: &[ProxyConfig],
    search_index: &[RowSearchIndex],
    facts: &QueryFacts<'_>,
    scope: &LibraryScope,
    query: &FilterQuery,
) -> usize {
    filter(rows, search_index, facts, scope, query).len()
}

/// M: the rows in the current scope before search and facets.
pub fn scope_total(rows: &[ProxyConfig], scope: &LibraryScope) -> usize {
    rows.iter().filter(|row| scope.contains(row)).count()
}

/// Every scope's total, measured before search and facets. The nav stores
/// this on the row-load path so a repaint never recounts the library.
pub fn scope_counts(rows: &[ProxyConfig], scopes: &[LibraryScope]) -> Vec<(LibraryScope, usize)> {
    scopes
        .iter()
        .cloned()
        .map(|scope| {
            let count = scope_total(rows, &scope);
            (scope, count)
        })
        .collect()
}

/// How many selected ids the current visible set hides. The selection is not
/// a visible-row set, so a hidden row still counts and still acts.
pub fn selection_hidden_count(
    selected: &HashSet<String>,
    visible: &[usize],
    rows: &[ProxyConfig],
) -> usize {
    let visible_ids: HashSet<&str> = visible
        .iter()
        .filter_map(|&index| rows.get(index))
        .map(|row| row.id.as_str())
        .collect();
    selected
        .iter()
        .filter(|id| !visible_ids.contains(id.as_str()))
        .count()
}

/// Five options, each counted with the other facets applied and the status
/// set released.
pub fn status_counts(
    rows: &[ProxyConfig],
    search_index: &[RowSearchIndex],
    facts: &QueryFacts<'_>,
    scope: &LibraryScope,
    query: &FilterQuery,
) -> Vec<(StatusFacet, usize)> {
    let base = query.without(Facet::Status);
    StatusFacet::ALL
        .into_iter()
        .map(|status| {
            let mut narrowed = base.clone();
            narrowed.statuses.insert(status);
            (
                status,
                filter(rows, search_index, facts, scope, &narrowed).len(),
            )
        })
        .collect()
}

/// Protocol options present in the scope, plus any selected protocol that
/// survived a row change, sorted by count descending then name.
pub fn protocol_counts(
    rows: &[ProxyConfig],
    search_index: &[RowSearchIndex],
    facts: &QueryFacts<'_>,
    scope: &LibraryScope,
    query: &FilterQuery,
) -> Vec<(String, usize)> {
    let base = query.without(Facet::Protocol);
    let mut names: BTreeSet<String> = rows
        .iter()
        .filter(|row| scope.contains(row))
        .map(|row| row.protocol.clone())
        .collect();
    names.extend(query.protocols.iter().cloned());
    let mut options: Vec<(String, usize)> = names
        .into_iter()
        .map(|protocol| {
            let mut narrowed = base.clone();
            narrowed.protocols.insert(protocol.clone());
            (
                protocol,
                filter(rows, search_index, facts, scope, &narrowed).len(),
            )
        })
        .collect();
    options.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    options
}

/// The four latency buckets, counted with the latency facet released and the
/// current unprobed toggle preserved.
pub fn latency_counts(
    rows: &[ProxyConfig],
    search_index: &[RowSearchIndex],
    facts: &QueryFacts<'_>,
    scope: &LibraryScope,
    query: &FilterQuery,
) -> Vec<(LatencyBucket, usize)> {
    let base = query.without(Facet::Latency);
    LatencyBucket::ALL
        .into_iter()
        .map(|bucket| {
            let mut narrowed = base.clone();
            narrowed.latency = LatencyFacet {
                bucket,
                include_unprobed: query.latency.include_unprobed,
            };
            (
                bucket,
                filter(rows, search_index, facts, scope, &narrowed).len(),
            )
        })
        .collect()
}

/// Bind values present in the scope, plus any selected bind, counted with the
/// bind facet released.
pub fn bind_counts(
    rows: &[ProxyConfig],
    search_index: &[RowSearchIndex],
    facts: &QueryFacts<'_>,
    scope: &LibraryScope,
    query: &FilterQuery,
) -> Vec<(String, usize)> {
    let base = query.without(Facet::Bind);
    let mut names: BTreeSet<String> = rows
        .iter()
        .filter(|row| scope.contains(row))
        .map(|row| row.bind_ip.clone())
        .collect();
    names.extend(query.binds.iter().cloned());
    let mut options: Vec<(String, usize)> = names
        .into_iter()
        .map(|bind| {
            let mut narrowed = base.clone();
            narrowed.binds.insert(bind.clone());
            (
                bind,
                filter(rows, search_index, facts, scope, &narrowed).len(),
            )
        })
        .collect();
    options.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(
        id: &str,
        name: &str,
        protocol: &str,
        group: &str,
        bind: &str,
        port: u16,
    ) -> ProxyConfig {
        ProxyConfig {
            id: id.to_string(),
            name: name.to_string(),
            protocol: protocol.to_string(),
            params: "{}".to_string().into(),
            group: group.to_string(),
            enabled: true,
            bind_ip: bind.to_string(),
            port,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        }
    }

    fn facts<'a>(
        running: &'a HashSet<String>,
        health: &'a HashMap<String, String>,
        crashed: &'a HashSet<String>,
        latency: &'a HashMap<String, u64>,
    ) -> QueryFacts<'a> {
        QueryFacts {
            running,
            health,
            crashed,
            latency,
        }
    }

    fn indexes(rows: &[ProxyConfig]) -> Vec<RowSearchIndex> {
        rows.iter().map(RowSearchIndex::of).collect()
    }

    type EmptyFacts = (
        HashSet<String>,
        HashMap<String, String>,
        HashSet<String>,
        HashMap<String, u64>,
    );

    fn empty_facts() -> EmptyFacts {
        (
            HashSet::new(),
            HashMap::new(),
            HashSet::new(),
            HashMap::new(),
        )
    }

    #[test]
    fn or_within_a_facet_and_across_facets() {
        let mut rows = vec![
            row("1", "alpha", "vless", "work", "127.0.0.1", 10801),
            row("2", "beta", "vmess", "work", "127.0.0.1", 10802),
            row("3", "gamma", "vless", "home", "127.0.0.1", 10803),
        ];
        rows[1].enabled = false;
        let index = indexes(&rows);
        let (running, health, crashed, latency) = empty_facts();
        let facts = facts(&running, &health, &crashed, &latency);
        let mut query = FilterQuery::default();

        query.statuses.insert(StatusFacet::Enabled);
        query.statuses.insert(StatusFacet::Disabled);
        assert_eq!(
            filter(&rows, &index, &facts, &LibraryScope::All, &query),
            vec![0, 1, 2],
            "two statuses match either"
        );

        query.protocols.insert("vless".to_string());
        assert_eq!(
            filter(&rows, &index, &facts, &LibraryScope::All, &query),
            vec![0, 2],
            "status OR inside, protocol AND across"
        );
    }

    #[test]
    fn search_matches_every_documented_field_and_never_secrets() {
        let mut rows = vec![row("1", "Alpha", "vless", "work", "127.0.0.1", 10801)];
        rows[0].params = "{\"server\":\"edge.example.net\",\"password\":\"hunter2\"}"
            .to_string()
            .into();
        rows[0].subscription_id = Some("sub-1".to_string());
        rows[0].preset_id = Some("preset-1".to_string());
        let mut index = indexes(&rows);
        let running: HashSet<String> = ["1".to_string()].into_iter().collect();
        let health: HashMap<String, String> = [("1".to_string(), "live".to_string())]
            .into_iter()
            .collect();
        let crashed = HashSet::new();
        let latency = HashMap::new();
        index[0].enrich(
            &rows[0],
            &HashMap::from([("sub-1".to_string(), "Team feed".to_string())]),
            &HashMap::from([("preset-1".to_string(), "Fast preset".to_string())]),
            &running,
            &health,
        );
        let facts = facts(&running, &health, &crashed, &latency);

        for needle in [
            "alpha",
            "edge.example.net",
            "127.0.0.1:10801",
            "127.0.0.1",
            "10801",
            "work",
            "vless",
            "team feed",
            "fast preset",
            "running",
            "live",
        ] {
            let mut query = FilterQuery::default();
            query.set_search(needle);
            assert_eq!(
                filter(&rows, &index, &facts, &LibraryScope::All, &query),
                vec![0],
                "{needle} matches a documented field"
            );
        }

        let mut query = FilterQuery::default();
        query.set_search("hunter2");
        assert!(
            filter(&rows, &index, &facts, &LibraryScope::All, &query).is_empty(),
            "a secret never matches"
        );
    }

    #[test]
    fn port_token_parses_all_four_forms_and_leaves_text() {
        let rows = vec![
            row("1", "one", "vless", "", "127.0.0.1", 1080),
            row("2", "two", "vless", "", "127.0.0.1", 1081),
            row("3", "three", "vless", "", "127.0.0.1", 1099),
            row("4", "four", "vless", "", "127.0.0.1", 2000),
        ];
        let index = indexes(&rows);
        let (running, health, crashed, latency) = empty_facts();
        let facts = facts(&running, &health, &crashed, &latency);

        for (text, expected) in [
            ("port=1081", vec![1]),
            ("port=1080:1099", vec![0, 1, 2]),
            ("port=1099:", vec![2, 3]),
            ("port=:1081", vec![0, 1]),
        ] {
            let mut query = FilterQuery::default();
            query.set_search(text);
            assert_eq!(
                filter(&rows, &index, &facts, &LibraryScope::All, &query),
                expected,
                "{text} narrows by port"
            );
            assert_eq!(query.search_text(), "", "{text} leaves no free text");
        }

        let mut query = FilterQuery::default();
        query.set_search("port=5:1");
        assert_eq!(query.port, None, "reversed bounds stay literal");
        assert_eq!(query.search_text(), "port=5:1");
        assert!(
            filter(&rows, &index, &facts, &LibraryScope::All, &query).is_empty(),
            "the malformed token is a literal substring"
        );

        let mut query = FilterQuery::default();
        query.set_search("port=1080:1099 alpha");
        assert_eq!(query.search_text(), "alpha");
        assert_eq!(
            filter(&rows, &index, &facts, &LibraryScope::All, &query),
            Vec::<usize>::new(),
            "the remaining text still ANDs with the port"
        );
    }

    #[test]
    fn status_counts_are_self_excluded() {
        let mut rows = vec![
            row("1", "alpha", "vless", "", "127.0.0.1", 10801),
            row("2", "beta", "vmess", "", "127.0.0.1", 10802),
            row("3", "gamma", "vless", "", "127.0.0.1", 10803),
        ];
        rows[1].enabled = false;
        let index = indexes(&rows);
        let running: HashSet<String> = ["1".to_string()].into_iter().collect();
        let health = HashMap::new();
        let crashed = HashSet::new();
        let latency = HashMap::new();
        let facts = facts(&running, &health, &crashed, &latency);
        let mut query = FilterQuery::default();
        query.statuses.insert(StatusFacet::Running);

        let counts = status_counts(&rows, &index, &facts, &LibraryScope::All, &query);
        let count = |status: StatusFacet| {
            counts
                .iter()
                .find(|(candidate, _)| *candidate == status)
                .map(|(_, count)| *count)
                .unwrap_or(0)
        };
        assert_eq!(
            count(StatusFacet::Running),
            1,
            "the Running count ignores Running"
        );
        assert_eq!(count(StatusFacet::Enabled), 2);
        assert_eq!(count(StatusFacet::Disabled), 1);

        let protocols = protocol_counts(&rows, &index, &facts, &LibraryScope::All, &query);
        assert_eq!(
            protocols,
            vec![("vless".to_string(), 1), ("vmess".to_string(), 0)],
            "protocol counts still respect the status selection"
        );
    }

    #[test]
    fn protocol_counts_sort_by_count_then_name_with_zeroes() {
        let mut rows = vec![
            row("1", "a", "vmess", "", "127.0.0.1", 10801),
            row("2", "b", "vmess", "", "127.0.0.1", 10802),
            row("3", "c", "vless", "", "127.0.0.1", 10803),
            row("4", "d", "trojan", "", "127.0.0.1", 10804),
        ];
        rows[3].enabled = false;
        let index = indexes(&rows);
        let (running, health, crashed, latency) = empty_facts();
        let facts = facts(&running, &health, &crashed, &latency);
        let mut query = FilterQuery::default();
        query.statuses.insert(StatusFacet::Enabled);
        let options = protocol_counts(&rows, &index, &facts, &LibraryScope::All, &query);
        assert_eq!(
            options,
            vec![
                ("vmess".to_string(), 2),
                ("vless".to_string(), 1),
                ("trojan".to_string(), 0),
            ],
            "count descending, then name, zero kept"
        );
    }

    #[test]
    fn latency_counts_use_inclusive_boundaries_and_unprobed() {
        let rows = vec![
            row("1", "a", "vless", "", "127.0.0.1", 10801),
            row("2", "b", "vless", "", "127.0.0.1", 10802),
            row("3", "c", "vless", "", "127.0.0.1", 10803),
            row("4", "d", "vless", "", "127.0.0.1", 10804),
        ];
        let index = indexes(&rows);
        let (running, health, crashed) = (HashSet::new(), HashMap::new(), HashSet::new());
        let latency: HashMap<String, u64> = [
            ("1".to_string(), 100),
            ("2".to_string(), 101),
            ("3".to_string(), 1000),
        ]
        .into_iter()
        .collect();
        let facts = facts(&running, &health, &crashed, &latency);
        let query = FilterQuery::default();

        let counts = latency_counts(&rows, &index, &facts, &LibraryScope::All, &query);
        let count = |bucket: LatencyBucket| {
            counts
                .iter()
                .find(|(candidate, _)| *candidate == bucket)
                .map(|(_, count)| *count)
                .unwrap_or(0)
        };
        assert_eq!(count(LatencyBucket::Any), 4);
        assert_eq!(
            count(LatencyBucket::Ms100),
            2,
            "100 is inclusive; the default toggle carries the unprobed row"
        );
        assert_eq!(count(LatencyBucket::Ms300), 3);
        assert_eq!(count(LatencyBucket::Ms1000), 4);

        let probed_only = FilterQuery {
            latency: LatencyFacet {
                bucket: LatencyBucket::Any,
                include_unprobed: false,
            },
            ..FilterQuery::default()
        };
        let counts = latency_counts(&rows, &index, &facts, &LibraryScope::All, &probed_only);
        let probed = |bucket: LatencyBucket| {
            counts
                .iter()
                .find(|(candidate, _)| *candidate == bucket)
                .map(|(_, count)| *count)
                .unwrap_or(0)
        };
        assert_eq!(
            (
                probed(LatencyBucket::Any),
                probed(LatencyBucket::Ms100),
                probed(LatencyBucket::Ms300),
                probed(LatencyBucket::Ms1000),
            ),
            (3, 1, 2, 3),
            "the unprobed toggle carries into the counts; boundaries stay inclusive"
        );
    }

    #[test]
    fn bind_counts_exclude_their_own_facet() {
        let rows = vec![
            row("1", "a", "vless", "", "127.0.0.1", 10801),
            row("2", "b", "vless", "", "0.0.0.0", 10802),
            row("3", "c", "vless", "", "0.0.0.0", 10803),
        ];
        let index = indexes(&rows);
        let (running, health, crashed, latency) = empty_facts();
        let facts = facts(&running, &health, &crashed, &latency);
        let mut query = FilterQuery::default();
        query.binds.insert("127.0.0.1".to_string());
        let options = bind_counts(&rows, &index, &facts, &LibraryScope::All, &query);
        assert_eq!(
            options,
            vec![("0.0.0.0".to_string(), 2), ("127.0.0.1".to_string(), 1)],
            "the selected bind still sees the whole pool"
        );
    }

    #[test]
    fn visible_count_and_scope_total_are_distinct() {
        let rows = vec![
            row("1", "alpha", "vless", "work", "127.0.0.1", 10801),
            row("2", "beta", "vless", "work", "127.0.0.1", 10802),
            row("3", "gamma", "vless", "home", "127.0.0.1", 10803),
        ];
        let index = indexes(&rows);
        let (running, health, crashed, latency) = empty_facts();
        let facts = facts(&running, &health, &crashed, &latency);
        let scope = LibraryScope::Group("work".to_string());
        assert_eq!(scope_total(&rows, &scope), 2);

        let mut query = FilterQuery::default();
        query.set_search("alpha");
        assert_eq!(visible_count(&rows, &index, &facts, &scope, &query), 1);
        assert_eq!(scope_total(&rows, &scope), 2);
    }

    #[test]
    fn scope_matches_all_group_ungrouped_and_source() {
        let mut rows = vec![
            row("1", "alpha", "vless", "work", "127.0.0.1", 10801),
            row("2", "beta", "vless", DEFAULT_GROUP, "127.0.0.1", 10802),
            row("3", "gamma", "vless", "home", "127.0.0.1", 10803),
        ];
        rows[2].subscription_id = Some("sub-1".to_string());

        assert!(scope_matches(&LibraryScope::All, &rows[0]));
        assert!(scope_matches(
            &LibraryScope::Group("work".to_string()),
            &rows[0]
        ));
        assert!(!scope_matches(
            &LibraryScope::Group("work".to_string()),
            &rows[1]
        ));
        assert!(scope_matches(&LibraryScope::Ungrouped, &rows[1]));
        assert!(!scope_matches(&LibraryScope::Ungrouped, &rows[0]));
        assert!(scope_matches(
            &LibraryScope::Source("sub-1".to_string()),
            &rows[2]
        ));
        assert!(!scope_matches(
            &LibraryScope::Source("sub-1".to_string()),
            &rows[0]
        ));
    }

    #[test]
    fn ungrouped_is_the_default_group_bucket() {
        let rows = vec![
            row("1", "alpha", "vless", DEFAULT_GROUP, "127.0.0.1", 10801),
            row("2", "beta", "vless", "Home", "127.0.0.1", 10802),
        ];
        assert_eq!(
            filter(
                &rows,
                &indexes(&rows),
                &facts(
                    &HashSet::new(),
                    &HashMap::new(),
                    &HashSet::new(),
                    &HashMap::new()
                ),
                &LibraryScope::Ungrouped,
                &FilterQuery::default()
            ),
            vec![0],
            "the default bucket holds only the store's default group"
        );
        assert_eq!(
            scope_total(&rows, &LibraryScope::Group("Home".to_string())),
            1
        );
    }

    #[test]
    fn scope_counts_ignore_the_query() {
        let rows = vec![
            row("1", "alpha", "vless", "work", "127.0.0.1", 10801),
            row("2", "beta", "vless", "work", "127.0.0.1", 10802),
            row("3", "gamma", "vless", DEFAULT_GROUP, "127.0.0.1", 10803),
        ];
        let scopes = vec![
            LibraryScope::All,
            LibraryScope::Group("work".to_string()),
            LibraryScope::Ungrouped,
        ];
        let counts = scope_counts(&rows, &scopes);
        assert_eq!(
            counts,
            vec![
                (LibraryScope::All, 3),
                (scopes[1].clone(), 2),
                (LibraryScope::Ungrouped, 1)
            ]
        );

        let mut query = FilterQuery::default();
        query.set_search("alpha");
        assert_eq!(
            filter(
                &rows,
                &indexes(&rows),
                &facts(
                    &HashSet::new(),
                    &HashMap::new(),
                    &HashSet::new(),
                    &HashMap::new()
                ),
                &LibraryScope::All,
                &query
            )
            .len(),
            1,
            "the query narrows the table"
        );
        assert_eq!(
            scope_counts(&rows, &scopes),
            counts,
            "the nav counts do not move with a search"
        );
    }

    #[test]
    fn scope_total_is_the_rows_in_the_scope_before_the_query() {
        let rows = vec![
            row("1", "alpha", "vless", "work", "127.0.0.1", 10801),
            row("2", "beta", "vless", "work", "127.0.0.1", 10802),
        ];
        let mut query = FilterQuery::default();
        query.set_search("alpha");
        assert_eq!(
            filter(
                &rows,
                &indexes(&rows),
                &facts(
                    &HashSet::new(),
                    &HashMap::new(),
                    &HashSet::new(),
                    &HashMap::new()
                ),
                &LibraryScope::Group("work".to_string()),
                &query
            )
            .len(),
            1
        );
        assert_eq!(
            scope_total(&rows, &LibraryScope::Group("work".to_string())),
            2,
            "M ignores the query"
        );
    }

    #[test]
    fn custom_endpoint_means_non_default_bind() {
        let rows = vec![
            row("1", "loopback", "vless", "", "127.0.0.1", 10801),
            row("2", "lan", "vless", "", "0.0.0.0", 10802),
        ];
        let index = indexes(&rows);
        let (running, health, crashed, latency) = empty_facts();
        let facts = facts(&running, &health, &crashed, &latency);
        let query = FilterQuery {
            custom_endpoint: true,
            ..FilterQuery::default()
        };
        assert_eq!(
            filter(&rows, &index, &facts, &LibraryScope::All, &query),
            vec![1]
        );
    }

    #[test]
    fn selection_hidden_count_counts_selected_ids_not_in_visible() {
        let rows = vec![
            row("1", "a", "vless", "", "127.0.0.1", 10801),
            row("2", "b", "vless", "", "127.0.0.1", 10802),
            row("3", "c", "vless", "", "127.0.0.1", 10803),
        ];
        let selected: HashSet<String> = ["1".to_string(), "3".to_string()].into_iter().collect();
        assert_eq!(selection_hidden_count(&selected, &[0], &rows), 1);
        assert_eq!(selection_hidden_count(&selected, &[0, 2], &rows), 0);
    }
}
