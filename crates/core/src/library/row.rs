//! The pure row view model the library row paints.
//!
//! Belongs: the fields one row needs to draw itself, and nothing else.
//! Not here: gpui, the store, or any engine call.
//!
//! The model never clones a [`Secret`]. A row's params are sealed material
//! the row never reads; the view borrows the config and copies the handful of
//! plain fields it paints, so a scroll cannot leave a copy of a password in
//! the allocator.

use std::collections::{HashMap, HashSet};

use crate::*;

/// The ambient state one row reads: the engine's running set, the probe's
/// results, the session's counters.
pub struct RowViewInputs<'a> {
    /// The rows the engine is serving.
    pub running: &'a HashSet<String>,
    /// The rows whose engine shard crashed.
    pub crashed: &'a HashSet<String>,
    /// Whether a start is in flight. A row that is not serving yet reads as
    /// starting.
    pub starting: bool,
    /// The probe's last summary per row (`123ms`, `live`, `dead`).
    pub health: &'a HashMap<String, String>,
    /// The probe's last failure cause per row.
    pub health_errors: &'a HashMap<String, String>,
    /// The latest latency sample per row, in milliseconds.
    pub latency: &'a HashMap<String, u64>,
    /// The accumulated counters per row, rx then tx.
    pub traffic: &'a HashMap<String, (u64, u64)>,
    /// The subscription labels, keyed by subscription id.
    pub sub_labels: &'a HashMap<String, String>,
    /// The preset names, keyed by preset id.
    pub preset_names: &'a HashMap<String, String>,
}

impl RowViewInputs<'_> {
    /// An empty world: nothing runs, nothing is probed.
    pub fn empty() -> Self {
        static EMPTY_SET: std::sync::LazyLock<HashSet<String>> =
            std::sync::LazyLock::new(HashSet::new);
        static EMPTY_MAP: std::sync::LazyLock<HashMap<String, String>> =
            std::sync::LazyLock::new(HashMap::new);
        static EMPTY_LATENCY: std::sync::LazyLock<HashMap<String, u64>> =
            std::sync::LazyLock::new(HashMap::new);
        static EMPTY_PAIRS: std::sync::LazyLock<HashMap<String, (u64, u64)>> =
            std::sync::LazyLock::new(HashMap::new);
        Self {
            running: &EMPTY_SET,
            crashed: &EMPTY_SET,
            starting: false,
            health: &EMPTY_MAP,
            health_errors: &EMPTY_MAP,
            latency: &EMPTY_LATENCY,
            traffic: &EMPTY_PAIRS,
            sub_labels: &EMPTY_MAP,
            preset_names: &EMPTY_MAP,
        }
    }
}

/// One row's paintable state.
///
/// Every field is plain data the renderer copies out of the store once per
/// frame. The model holds no secret and no engine handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowView {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub group: String,
    /// The subscription the row came from, or `Manual` when it has none.
    pub source: String,
    pub bind: String,
    pub port: u16,
    pub enabled: bool,
    /// The serving state the lead glyph paints.
    pub state: RowPhase,
    /// The latest latency sample in milliseconds, when the row has one.
    pub latency: Option<u64>,
    /// The health cell's text: a latency, `Unprobed`, `timeout`, `refused`
    /// or `Failed`. Empty when the row is stopped and nothing was probed.
    pub health_text: String,
    /// The accumulated counters, rx then tx.
    pub traffic: Option<(u64, u64)>,
    /// The preset the row is bound to, by name.
    pub preset: Option<String>,
}

impl RowView {
    /// Read one row. The config is borrowed, so the params never leave the
    /// store's hands.
    pub fn of(config: &ProxyConfig, inputs: &RowViewInputs<'_>) -> Self {
        let id = config.id.clone();
        let running = inputs.running.contains(&id);
        let failed = inputs.crashed.contains(&id)
            || inputs
                .health
                .get(&id)
                .is_some_and(|sum| sum.trim() == "dead");
        let starting = inputs.starting && !running;
        let state = RowPhase::of(running, starting, failed);
        let latency = inputs.latency.get(&id).copied();
        let health_text = row_health_text(
            state,
            latency,
            inputs.health.get(&id).map(String::as_str),
            inputs.health_errors.get(&id).map(String::as_str),
        );
        let traffic = inputs.traffic.get(&id).copied();
        Self {
            id: id.clone(),
            name: config.name.clone(),
            protocol: config.protocol.clone(),
            group: config.group.clone(),
            source: row_source(config, inputs.sub_labels),
            bind: config.bind_ip.clone(),
            port: config.port,
            enabled: config.enabled,
            state,
            latency,
            health_text,
            traffic,
            preset: config
                .preset_id
                .as_deref()
                .and_then(|preset| inputs.preset_names.get(preset))
                .cloned(),
        }
    }

    /// The endpoint the chip shows: `:port` for the default bind, and
    /// `host:port` for anything else. An IPv6 host is bracketed.
    pub fn endpoint_display(&self) -> String {
        endpoint_display(&self.bind, self.port)
    }

    /// The endpoint the clipboard carries: `host:port`, IPv6 bracketed.
    pub fn endpoint_full(&self) -> String {
        endpoint_full(&self.bind, self.port)
    }
}

/// The chip's label. The default bind is the loopback every row starts with,
/// so the chip reads `:10801` there and `host:port` everywhere else.
pub fn endpoint_display(bind: &str, port: u16) -> String {
    if bind.trim() == DEFAULT_BIND {
        format!(":{port}")
    } else {
        endpoint_full(bind, port)
    }
}

/// The full `host:port`. An IPv6 literal is bracketed so the port stays
/// readable.
pub fn endpoint_full(bind: &str, port: u16) -> String {
    let trimmed = bind.trim();
    if trimmed.contains(':') && !trimmed.starts_with('[') {
        format!("[{trimmed}]:{port}")
    } else {
        format!("{trimmed}:{port}")
    }
}

/// The row's source. A row with no subscription is `Manual`; the word
/// `Local` is retired — it reads as a bind address.
pub fn row_source(config: &ProxyConfig, sub_labels: &HashMap<String, String>) -> String {
    config
        .subscription_id
        .as_deref()
        .and_then(|owner| sub_labels.get(owner))
        .map(String::as_str)
        .filter(|label| !label.trim().is_empty())
        .unwrap_or("Manual")
        .to_string()
}

/// The health cell's text. A running row shows its latency or `Unprobed`; a
/// failed row shows the probe's verdict; a stopped row that was never probed
/// says nothing.
fn row_health_text(
    state: RowPhase,
    latency: Option<u64>,
    summary: Option<&str>,
    cause: Option<&str>,
) -> String {
    match state {
        RowPhase::Failed => cause
            .map(failure_label)
            .unwrap_or_else(|| summary.and_then(failure_label_opt).unwrap_or("Failed"))
            .to_string(),
        RowPhase::Running => match latency {
            Some(ms) => format!("{ms} ms"),
            None => "Unprobed".to_string(),
        },
        RowPhase::Starting | RowPhase::Stopped => match summary {
            Some(sum) if !sum.trim().is_empty() && sum.trim() != "live" => sum.trim().to_string(),
            _ => String::new(),
        },
    }
}

/// The failure word the health cell carries. The probe's cause is free-form,
/// so it is folded into the two the cell knows: `timeout` and `refused`.
pub fn failure_label(cause: &str) -> &'static str {
    failure_label_opt(cause).unwrap_or("Failed")
}

fn failure_label_opt(cause: &str) -> Option<&'static str> {
    let lowered = cause.to_lowercase();
    if lowered.contains("timeout") || lowered.contains("timed out") {
        Some("timeout")
    } else if lowered.contains("refused") || lowered.contains("connection refused") {
        Some("refused")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(id: &str, bind: &str, port: u16) -> ProxyConfig {
        ProxyConfig {
            id: id.to_string(),
            name: format!("row {id}"),
            protocol: "vless".to_string(),
            params: Secret::new("{\"password\":\"hunter2\"}"),
            group: "work".to_string(),
            enabled: true,
            bind_ip: bind.to_string(),
            port,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        }
    }

    /// The chip reads `:port` for the default bind and `host:port` for the
    /// rest, with an IPv6 literal bracketed.
    #[test]
    fn endpoint_display_follows_the_default_bind() {
        assert_eq!(endpoint_display(DEFAULT_BIND, 10801), ":10801");
        assert_eq!(endpoint_display("0.0.0.0", 10801), "0.0.0.0:10801");
        assert_eq!(endpoint_display("::1", 10801), "[::1]:10801");
        assert_eq!(endpoint_display("[::1]", 10801), "[::1]:10801");
    }

    /// The model copies plain fields and never the params: the row's secret
    /// material is not part of what a row can carry.
    #[test]
    fn row_model_never_carries_secret_material() {
        let inputs = RowViewInputs::empty();
        let view = RowView::of(&config("1", DEFAULT_BIND, 10801), &inputs);
        let rendered = format!("{view:?}");
        assert!(
            !rendered.contains("hunter2"),
            "the params never reach the model"
        );
        assert!(
            !rendered.contains("password"),
            "the params never reach the model"
        );
    }

    /// A row with no subscription is `Manual`, never `Local`.
    #[test]
    fn a_row_without_a_subscription_is_manual() {
        let inputs = RowViewInputs::empty();
        let view = RowView::of(&config("1", DEFAULT_BIND, 10801), &inputs);
        assert_eq!(view.source, "Manual");

        let mut labels = HashMap::new();
        labels.insert("sub-1".to_string(), "Airport".to_string());
        let mut owned = config("2", DEFAULT_BIND, 10801);
        owned.subscription_id = Some("sub-1".to_string());
        let view = RowView::of(
            &owned,
            &RowViewInputs {
                sub_labels: &labels,
                ..RowViewInputs::empty()
            },
        );
        assert_eq!(view.source, "Airport");
    }

    /// The health cell names the latency, `Unprobed`, or the probe's verdict.
    #[test]
    fn health_cell_reads_latency_unprobed_and_failure() {
        let running = HashSet::from(["1".to_string()]);
        let latency = HashMap::from([("1".to_string(), 42u64)]);
        let inputs = RowViewInputs {
            running: &running,
            latency: &latency,
            ..RowViewInputs::empty()
        };
        let view = RowView::of(&config("1", DEFAULT_BIND, 10801), &inputs);
        assert_eq!(view.health_text, "42 ms");

        let inputs = RowViewInputs {
            running: &running,
            ..RowViewInputs::empty()
        };
        let view = RowView::of(&config("1", DEFAULT_BIND, 10801), &inputs);
        assert_eq!(view.health_text, "Unprobed");

        let crashed = HashSet::from(["1".to_string()]);
        let errors = HashMap::from([("1".to_string(), "dial refused".to_string())]);
        let inputs = RowViewInputs {
            crashed: &crashed,
            health_errors: &errors,
            ..RowViewInputs::empty()
        };
        let view = RowView::of(&config("1", DEFAULT_BIND, 10801), &inputs);
        assert_eq!(view.state, RowPhase::Failed);
        assert_eq!(view.health_text, "refused");
    }
}
