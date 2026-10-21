use crate::*;

pub(crate) struct RowSearchIndex {
    pub(crate) name: String,
    pub(crate) protocol: String,
    pub(crate) group: String,
    pub(crate) bind: String,
    pub(crate) port: String,
    pub(crate) host_server: String,
    pub(crate) host_address: String,
    pub(crate) sub_label: String,
    pub(crate) preset_name: String,
    pub(crate) state_word: String,
    pub(crate) serve_word: String,
    pub(crate) health_text: String,
}

impl RowSearchIndex {
    pub(crate) fn of(row: &ProxyConfig) -> Self {
        let (host_server, host_address) = search_hosts(&row.params);
        Self {
            name: row.name.to_lowercase(),
            protocol: row.protocol.to_lowercase(),
            group: row.group.to_lowercase(),
            bind: row.bind_ip.to_lowercase(),
            port: row.port.to_string(),
            host_server,
            host_address,
            sub_label: String::new(),
            preset_name: String::new(),
            state_word: if row.enabled {
                "enabled".to_string()
            } else {
                "disabled".to_string()
            },
            serve_word: "stopped".to_string(),
            health_text: String::new(),
        }
    }

    pub(crate) fn enrich(
        &mut self,
        row: &ProxyConfig,
        sub_labels: &HashMap<String, String>,
        preset_names: &HashMap<String, String>,
        running: &HashSet<String>,
        health: &HashMap<String, String>,
    ) {
        self.sub_label = row
            .subscription_id
            .as_deref()
            .and_then(|id| sub_labels.get(id))
            .map(|label| label.to_lowercase())
            .unwrap_or_default();
        self.preset_name = row
            .preset_id
            .as_deref()
            .and_then(|id| preset_names.get(id))
            .map(|name| name.to_lowercase())
            .unwrap_or_default();
        self.serve_word = if running.contains(&row.id) {
            "running".to_string()
        } else {
            "stopped".to_string()
        };
        self.health_text = health
            .get(&row.id)
            .map(|summary| summary.trim().to_lowercase())
            .unwrap_or_default();
    }

    pub(crate) fn matches_lowered(&self, lowered: &str) -> bool {
        if lowered.is_empty() {
            return true;
        }
        if self.name.contains(lowered)
            || self.protocol.contains(lowered)
            || self.group.contains(lowered)
            || self.bind.contains(lowered)
            || self.port.contains(lowered)
            || self.host_server.contains(lowered)
            || self.host_address.contains(lowered)
            || self.sub_label.contains(lowered)
            || self.preset_name.contains(lowered)
            || self.state_word.contains(lowered)
            || self.serve_word.contains(lowered)
        {
            return true;
        }
        if self.health_text.is_empty() {
            return "untested".contains(lowered);
        }
        if self.health_text.contains(lowered) {
            return true;
        }
        let health_word = if self.health_text == "dead" {
            "dead"
        } else {
            "live"
        };
        health_word.contains(lowered)
    }
}

pub(crate) fn search_hosts(params: &str) -> (String, String) {
    let mut server = String::new();
    let mut address = String::new();
    if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(params) {
        if let Some(serde_json::Value::String(host)) = map.get("server") {
            server = host.to_lowercase();
        }
        if let Some(serde_json::Value::String(host)) = map.get("address") {
            address = host.to_lowercase();
        }
    }
    (server, address)
}

/// T-23: the port facet of the search grammar — one `port=` token per query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PortPredicate {
    pub(crate) lo: u16,
    pub(crate) hi: u16,
}

impl PortPredicate {
    pub(crate) fn matches(self, port: u16) -> bool {
        port >= self.lo && port <= self.hi
    }

    /// Canonical form shown in the status-bar filter text.
    pub(crate) fn label(self) -> String {
        format!("port: {}..{}", self.lo, self.hi)
    }
}

/// Parse a single `port=` token: `port=N`, `port=N:M`, `port=N:` or `port=:M`.
/// Returns `None` for anything else — including the malformed forms (`port=`,
/// `port=x`, `port=5:1`, `port=:`, out-of-`u16` values) which stay literal
/// substring text instead of raising an error state.
pub(crate) fn parse_port_predicate(token: &str) -> Option<PortPredicate> {
    let (key, value) = token.split_once('=')?;
    if !key.eq_ignore_ascii_case("port") {
        return None;
    }
    let (lo, hi) = match value.split_once(':') {
        // port=N — exact port.
        None => {
            let exact = value.parse::<u16>().ok()?;
            (exact, exact)
        }
        // port=N: — open-ended range up to the last u16.
        Some((lo_raw, "")) => (lo_raw.parse::<u16>().ok()?, u16::MAX),
        // port=:M — open-ended range from zero.
        Some(("", hi_raw)) => (0, hi_raw.parse::<u16>().ok()?),
        // port=N:M — inclusive range, reversed bounds are malformed.
        Some((lo_raw, hi_raw)) => {
            let lo = lo_raw.parse::<u16>().ok()?;
            let hi = hi_raw.parse::<u16>().ok()?;
            if lo > hi {
                return None;
            }
            (lo, hi)
        }
    };
    Some(PortPredicate { lo, hi })
}

/// Split a query into its single `port=` predicate (if any) and the remaining
/// substring text. The text part is token-rejoined, so `port=10801 group: api`
/// keeps matching the blob for `group: api`. A second `port=` token is left in
/// the text part — it then degrades to a literal substring (documented above).
pub(crate) fn split_port_query(query: &str) -> (Option<PortPredicate>, String) {
    let mut predicate = None;
    let mut kept: Vec<&str> = Vec::new();
    for token in query.split_whitespace() {
        if predicate.is_none() {
            if let Some(parsed) = parse_port_predicate(token) {
                predicate = Some(parsed);
                continue;
            }
        }
        kept.push(token);
    }
    if predicate.is_none() {
        // No port token: keep today's query bytes untouched.
        return (None, query.to_string());
    }
    (predicate, kept.join(" "))
}
