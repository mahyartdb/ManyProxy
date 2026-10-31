use std::collections::HashMap;

use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitTraffic {
    pub row_id: String,
    pub name: String,
    pub bind: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

impl ExitTraffic {
    pub fn total(&self) -> u64 {
        self.rx_bytes.saturating_add(self.tx_bytes)
    }
}

/// How many per-exit rows the traffic dialog lists before rolling the tail up.
pub const TRAFFIC_TOP_EXITS: usize = 9;

/// One ranking for every place exits are shown: heaviest first, with totals
/// ties broken by stable identity so the same data always paints identically.
pub fn exit_traffic_order(a: &ExitTraffic, b: &ExitTraffic) -> std::cmp::Ordering {
    b.total()
        .cmp(&a.total())
        .then_with(|| a.row_id.cmp(&b.row_id))
        .then_with(|| a.name.cmp(&b.name))
        .then_with(|| a.bind.cmp(&b.bind))
}

/// What the dialog lists: the top [`TRAFFIC_TOP_EXITS`] exits plus the count
/// and exact byte totals of every exit ranked below them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrafficExitSplit {
    pub top: Vec<ExitTraffic>,
    pub other_exits: usize,
    pub other_rx: u64,
    pub other_tx: u64,
}

impl TrafficExitSplit {
    pub fn other_line(&self) -> Option<String> {
        (self.other_exits > 0)
            .then(|| traffic_other_line_text(self.other_exits, self.other_rx, self.other_tx))
    }
}

/// Ranks the exits, keeps the top nine and sums the rest. Recomputed from the
/// rows handed in, so every period paints its own split and nothing is stored.
pub fn traffic_top9_split(exits: &[ExitTraffic]) -> TrafficExitSplit {
    let mut ranked: Vec<ExitTraffic> = exits.to_vec();
    ranked.sort_by(exit_traffic_order);
    let split_at = ranked.len().min(TRAFFIC_TOP_EXITS);
    let rest = ranked.split_off(split_at);
    let mut other_rx = 0u64;
    let mut other_tx = 0u64;
    for exit in &rest {
        other_rx = other_rx.saturating_add(exit.rx_bytes);
        other_tx = other_tx.saturating_add(exit.tx_bytes);
    }
    TrafficExitSplit {
        top: ranked,
        other_exits: rest.len(),
        other_rx,
        other_tx,
    }
}

/// Bar denominator taken over every row (never the "Other" aggregate), so the
/// stacked bars keep truthful proportions once the tail is rolled up.
pub fn traffic_bar_max(exits: &[ExitTraffic]) -> u64 {
    exits
        .iter()
        .map(ExitTraffic::total)
        .max()
        .unwrap_or(1)
        .max(1)
}

pub fn conn_exit_of(rows: &[ProxyConfig], row_id: Option<&str>) -> (String, String, String) {
    match row_id.and_then(|id| rows.iter().find(|row| row.id == id)) {
        Some(row) => (
            row.name.clone(),
            format!("{}:{}", row.bind_ip, row.port),
            row.protocol.clone(),
        ),
        None => ("unclaimed".to_string(), "—".to_string(), "—".to_string()),
    }
}

pub fn aggregate_exit_traffic(
    rows: &[ProxyConfig],
    totals: &HashMap<String, (u64, u64)>,
) -> Vec<ExitTraffic> {
    let mut exits: Vec<ExitTraffic> = rows
        .iter()
        .filter_map(|row| {
            let (rx, tx) = totals.get(&row.id).copied().unwrap_or((0, 0));
            if rx == 0 && tx == 0 {
                return None;
            }
            Some(ExitTraffic {
                row_id: row.id.clone(),
                name: row.name.clone(),
                bind: format!("{}:{}", row.bind_ip, row.port),
                rx_bytes: rx,
                tx_bytes: tx,
            })
        })
        .collect();
    exits.sort_by(exit_traffic_order);
    exits
}

pub fn traffic_poll_due(tab: InspectorTab, now: i64, last_poll_at: i64) -> bool {
    if matches!(tab, InspectorTab::Links | InspectorTab::Traffic) {
        return true;
    }
    now.saturating_sub(last_poll_at) >= TRAFFIC_TICK_SECS as i64
}
