#[cfg(any(test, feature = "test-support"))]
use std::collections::{HashMap, HashSet};

use crate::*;

/// T-27: what a header-menu row does — pick a key, or reverse the direction.
#[derive(Debug, Clone, Copy)]
pub enum SortMenuAction {
    Key(LibrarySortKey),
    Direction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowPhase {
    Stopped,
    Starting,
    Running,
    Failed,
}

impl RowPhase {
    pub fn of(is_running: bool, starting: bool, failed: bool) -> Self {
        if starting {
            Self::Starting
        } else if failed {
            Self::Failed
        } else if is_running {
            Self::Running
        } else {
            Self::Stopped
        }
    }

    pub fn start_label(self) -> &'static str {
        match self {
            Self::Starting => "Starting…",
            Self::Stopped | Self::Running | Self::Failed => "Start",
        }
    }
}

pub fn upstream_host_port(row: &ProxyConfig) -> Option<String> {
    let map = serde_json::from_str::<serde_json::Value>(&row.params)
        .ok()?
        .as_object()?
        .clone();
    let host = ["server", "address"]
        .iter()
        .filter_map(|key| map.get(*key))
        .filter_map(serde_json::Value::as_str)
        .map(str::trim)
        .find(|host| !host.is_empty())?;
    let port = ["server_port", "port"]
        .iter()
        .filter_map(|key| map.get(*key))
        .find_map(|value| {
            if let Some(port) = value.as_u64() {
                (port > 0 && port <= 65535).then(|| port.to_string())
            } else {
                value.as_str().and_then(|text| {
                    text.trim()
                        .parse::<u64>()
                        .ok()
                        .filter(|port| *port > 0 && *port <= 65535)
                        .map(|port| port.to_string())
                })
            }
        });
    Some(match port {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    })
}

#[cfg(any(test, feature = "test-support"))]
pub fn library_row_lines(
    row: &ProxyConfig,
    health: Option<&str>,
    is_running: bool,
    traffic: Option<(u64, u64)>,
) -> Vec<String> {
    let upstream = upstream_host_port(row).unwrap_or_else(|| "no upstream".to_string());
    let state = if row.enabled { "enabled" } else { "disabled" };
    let mut lines = vec![
        row.name.clone(),
        format!("{} · {upstream}", row.protocol),
        format!(
            "{} · bind {}:{} · {state}",
            row.group, row.bind_ip, row.port
        ),
        serving_badge(is_running, &row.bind_ip, row.port, health),
    ];
    if let Some((rx, tx)) = traffic {
        if let Some(summary) = traffic_summary_text(rx, tx) {
            lines.push(summary);
        }
    }
    lines
}

pub fn library_row_cells(
    row: &ProxyConfig,
    upstream: &str,
    health: Option<&str>,
    is_running: bool,
    traffic: Option<(u64, u64)>,
    show_security: bool,
) -> [String; 4] {
    let mut upstream_cell = upstream.to_string();
    if show_security {
        apply_show_security(&mut upstream_cell, row);
    }
    let health_cell = if is_running {
        serving_badge(true, &row.bind_ip, row.port, health)
    } else {
        String::new()
    };
    let traffic_cell = if is_running {
        traffic
            .and_then(|(rx, tx)| traffic_summary_text(rx, tx))
            .unwrap_or_default()
    } else {
        String::new()
    };
    [upstream_cell, health_cell, row.group.clone(), traffic_cell]
}

pub fn library_row_full_route(row: &ProxyConfig, upstream: &str) -> String {
    let state = if row.enabled { "enabled" } else { "disabled" };
    format!(
        "{} · {upstream} · {} · bind {}:{} · {state}",
        row.protocol, row.group, row.bind_ip, row.port
    )
}

#[cfg(any(test, feature = "test-support"))]
pub fn library_row_matches(row: &ProxyConfig, query: &str) -> bool {
    library_row_matches_full(row, query, &MatchContext::empty())
}

#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Default)]
pub struct MatchContext {
    pub sub_labels: HashMap<String, String>,
    pub preset_names: HashMap<String, String>,
    pub running: HashSet<String>,
    pub health: HashMap<String, String>,
}

#[cfg(any(test, feature = "test-support"))]
impl MatchContext {
    #[cfg(any(test, feature = "test-support"))]
    pub fn empty() -> Self {
        Self::default()
    }
}

#[cfg(any(test, feature = "test-support"))]
pub fn library_row_matches_full(row: &ProxyConfig, query: &str, ctx: &MatchContext) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    if library_row_matches_base(row, &q) {
        return true;
    }
    if let Some(owner) = row.subscription_id.as_deref() {
        if let Some(label) = ctx.sub_labels.get(owner) {
            if label.to_lowercase().contains(&q) {
                return true;
            }
        }
    }
    if let Some(preset) = row.preset_id.as_deref() {
        if let Some(name) = ctx.preset_names.get(preset) {
            if name.to_lowercase().contains(&q) {
                return true;
            }
        }
    }
    let state_word = if row.enabled { "enabled" } else { "disabled" };
    if state_word.contains(&q) {
        return true;
    }
    let serve_word = if ctx.running.contains(&row.id) {
        "running"
    } else {
        "stopped"
    };
    if serve_word.contains(&q) {
        return true;
    }
    let summary = ctx
        .health
        .get(&row.id)
        .map(|summary| summary.trim().to_lowercase())
        .unwrap_or_default();
    if summary.is_empty() {
        return "untested".contains(&q);
    }
    if summary.contains(&q) {
        return true;
    }
    let health_word = if summary == "dead" { "dead" } else { "live" };
    health_word.contains(&q)
}

#[cfg(any(test, feature = "test-support"))]
pub fn library_row_matches_base(row: &ProxyConfig, q: &str) -> bool {
    if row.name.to_lowercase().contains(q)
        || row.protocol.to_lowercase().contains(q)
        || row.group.to_lowercase().contains(q)
        || row.bind_ip.to_lowercase().contains(q)
        || row.port.to_string().contains(q)
    {
        return true;
    }
    if let Ok(serde_json::Value::Object(map)) =
        serde_json::from_str::<serde_json::Value>(&row.params)
    {
        for key in ["server", "address"] {
            if let Some(serde_json::Value::String(host)) = map.get(key) {
                if host.to_lowercase().contains(q) {
                    return true;
                }
            }
        }
    }
    false
}
