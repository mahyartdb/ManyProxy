//! The console's pure model: tabs, scope, log rows and the poll cadence.
//!
//! Belongs: everything the console computes that the UI only paints.
//! Not here: gpui, the engine, or time formatting — the UI renders the clock.

use std::collections::{HashMap, HashSet};

use crate::*;

/// The console's three evidence tabs. Logs are history, connections are live
/// flows, traffic is session aggregates — one scope switch governs all three.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConsoleTab {
    #[default]
    Logs,
    Connections,
    Traffic,
}

impl ConsoleTab {
    pub fn label(self) -> &'static str {
        match self {
            ConsoleTab::Logs => "Logs",
            ConsoleTab::Connections => "Connections",
            ConsoleTab::Traffic => "Traffic",
        }
    }
}

/// What the console reads: everything, or only the selection. Global engine
/// lines (`config_id: None`) are `All`-only by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConsoleScope {
    #[default]
    All,
    Selected,
}

impl ConsoleScope {
    pub fn label(self) -> &'static str {
        match self {
            ConsoleScope::All => "All",
            ConsoleScope::Selected => "Selected",
        }
    }
}

/// One painted log row. `at` stays raw epoch seconds; the UI owns the clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleLogRow {
    pub at: i64,
    pub config_id: Option<String>,
    pub level: LogLevel,
    pub message: String,
}

impl ConsoleLogRow {
    /// The line the committed include/exclude/regex filters match, the same
    /// shape the retired inspector pane and the log sheet have always used.
    pub fn line(&self) -> String {
        format!("[{}] {}", self.level.as_str(), self.message)
    }
}

/// The log sheet's committed filters. The regex arrives pre-compiled: an
/// invalid pattern fails open in the UI, which keeps the old honest behavior.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConsoleLogFilters<'a> {
    pub include: &'a str,
    pub exclude: &'a str,
    pub regex: Option<&'a regex::Regex>,
    pub level: Option<LogLevel>,
}

/// How many log rows the console keeps on screen. The ring holds more; the
/// console is evidence, not an export surface — the sheet still saves it all.
pub const CONSOLE_LOG_ROWS: usize = 200;

/// The console's log rows for one scope and filter set, oldest first.
///
/// Global engine lines (`config_id: None`) survive only under `All`: under
/// `Selected` a line must belong to one of the selected ids. The quick filter
/// is one case-insensitive substring over level and message. The cap keeps
/// the NEWEST rows — the console tails.
pub fn log_rows(
    records: &[LogRecord],
    selected: &HashSet<String>,
    scope: ConsoleScope,
    filters: ConsoleLogFilters<'_>,
    quick: &str,
) -> Vec<ConsoleLogRow> {
    let include = filters.include.trim().to_lowercase();
    let exclude = filters.exclude.trim().to_lowercase();
    let quick = quick.trim().to_lowercase();
    let mut rows: Vec<ConsoleLogRow> = records
        .iter()
        .filter(|record| scope_allows(selected, scope, record))
        .filter(|record| filters.level.is_none_or(|want| record.level == want))
        .map(|record| ConsoleLogRow {
            at: record.at,
            config_id: record.config_id.clone(),
            level: record.level,
            // The ring already redacted at push; a second pass keeps a raw
            // push path from ever leaking through the console.
            message: redact_secrets(&strip_ansi_codes(&record.message)),
        })
        .filter(|row| {
            let line = row.line().to_lowercase();
            (include.is_empty() || line.contains(&include))
                && (exclude.is_empty() || !line.contains(&exclude))
                && filters.regex.is_none_or(|re| re.is_match(&row.line()))
                && (quick.is_empty()
                    || format!("{} {}", row.level.as_str(), row.message)
                        .to_lowercase()
                        .contains(&quick))
        })
        .collect();
    if rows.len() > CONSOLE_LOG_ROWS {
        rows.drain(..rows.len() - CONSOLE_LOG_ROWS);
    }
    rows
}

fn scope_allows(selected: &HashSet<String>, scope: ConsoleScope, record: &LogRecord) -> bool {
    match scope {
        ConsoleScope::All => true,
        ConsoleScope::Selected => match record.config_id.as_deref() {
            Some(id) => selected.contains(id),
            None => false,
        },
    }
}

/// How many connections the console shows for one scope. `All` keeps every
/// entry, including unclaimed ones; `Selected` keeps only flows claimed by a
/// selected row.
pub fn conn_rows_in_scope<'a>(
    snapshot: &'a [(String, crate::health::ClashConn)],
    selected: &HashSet<String>,
    scope: ConsoleScope,
) -> Vec<&'a (String, crate::health::ClashConn)> {
    snapshot
        .iter()
        .filter(|(_, entry)| match scope {
            ConsoleScope::All => true,
            ConsoleScope::Selected => entry
                .row_id
                .as_deref()
                .is_some_and(|row| selected.contains(row)),
        })
        .collect()
}

/// Whether a traffic poll is due. A console open on Connections or Traffic
/// follows the 1 Hz live cadence; every other state waits the base tick.
pub fn traffic_poll_due(open: bool, tab: ConsoleTab, now: i64, last_poll_at: i64) -> bool {
    if open && matches!(tab, ConsoleTab::Connections | ConsoleTab::Traffic) {
        return true;
    }
    now.saturating_sub(last_poll_at) >= TRAFFIC_TICK_SECS as i64
}

/// Session RX/TX summed over the connections a scope shows, plus the deltas
/// since the console opened tracking. These are session numbers, never the
/// stored totals the engine bar reads.
pub fn session_traffic(
    snapshot: &[(String, crate::health::ClashConn)],
    selected: &HashSet<String>,
    scope: ConsoleScope,
) -> (u64, u64) {
    conn_rows_in_scope(snapshot, selected, scope)
        .into_iter()
        .fold((0, 0), |(rx, tx), (_, entry)| {
            (
                rx.saturating_add(entry.rx_bytes),
                tx.saturating_add(entry.tx_bytes),
            )
        })
}

/// One exit bar for the Traffic tab. `limit` is the console's eight; the
/// retired dialog's nine is gone.
pub fn traffic_top_split(exits: &[ExitTraffic], limit: usize) -> Vec<ExitTraffic> {
    let mut ranked: Vec<ExitTraffic> = exits.to_vec();
    ranked.sort_by(exit_traffic_order);
    ranked.truncate(limit.max(1));
    ranked
}

/// The traffic tab's bar denominator over the exits it shows, so proportions
/// stay truthful once the tail is cut.
pub fn console_traffic_bar_max(exits: &[ExitTraffic]) -> u64 {
    exits
        .iter()
        .map(ExitTraffic::total)
        .max()
        .unwrap_or(1)
        .max(1)
}

/// Aggregated per-exit traffic for one scope, the bars' source. `delta`
/// carries the session's per-connection `core/id` counters; exits the scope
/// never shows are absent.
pub fn scope_exits(
    snapshot: &[(String, crate::health::ClashConn)],
    deltas: &HashMap<String, (u64, u64)>,
    selected: &HashSet<String>,
    scope: ConsoleScope,
) -> Vec<ExitTraffic> {
    let mut per_row: HashMap<String, (u64, u64)> = HashMap::new();
    for (_, entry) in conn_rows_in_scope(snapshot, selected, scope) {
        let Some(row_id) = entry.row_id.clone() else {
            continue;
        };
        let (delta_rx, delta_tx) = deltas.get(&entry.id).copied().unwrap_or((0, 0));
        let slot = per_row.entry(row_id).or_insert((0, 0));
        slot.0 = slot.0.saturating_add(entry.rx_bytes + delta_rx);
        slot.1 = slot.1.saturating_add(entry.tx_bytes + delta_tx);
    }
    per_row
        .into_iter()
        .map(|(row_id, (rx, tx))| ExitTraffic {
            row_id,
            name: String::new(),
            bind: String::new(),
            rx_bytes: rx,
            tx_bytes: tx,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(config_id: Option<&str>, level: LogLevel, message: &str) -> LogRecord {
        LogRecord {
            at: 1,
            config_id: config_id.map(str::to_string),
            level,
            message: message.to_string(),
            search: String::new(),
        }
    }

    fn set(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn console_log_scope_all_includes_global_lines_and_selected_does_not() {
        let records = vec![
            record(None, LogLevel::Info, "engine up"),
            record(Some("a"), LogLevel::Info, "row a line"),
        ];
        let selected = set(&["a"]);
        let filters = ConsoleLogFilters::default();
        let all = log_rows(&records, &selected, ConsoleScope::All, filters, "");
        assert_eq!(all.len(), 2, "All shows global and scoped lines: {all:?}");
        let scoped = log_rows(&records, &selected, ConsoleScope::Selected, filters, "");
        assert_eq!(
            scoped.len(),
            1,
            "Selected drops the global engine line: {scoped:?}"
        );
        assert_eq!(scoped[0].message, "row a line");
    }

    #[test]
    fn console_log_scope_selected_uses_every_selected_id() {
        let records = vec![
            record(Some("a"), LogLevel::Info, "alpha"),
            record(Some("b"), LogLevel::Warn, "beta"),
            record(Some("c"), LogLevel::Error, "gamma"),
        ];
        let selected = set(&["a", "c"]);
        let rows = log_rows(
            &records,
            &selected,
            ConsoleScope::Selected,
            ConsoleLogFilters::default(),
            "",
        );
        assert_eq!(rows.len(), 2, "every selected id's lines show: {rows:?}");
        assert!(rows.iter().all(|row| row.message != "beta"));
    }

    #[test]
    fn console_quick_filter_matches_level_and_message() {
        let records = vec![
            record(Some("a"), LogLevel::Error, "disk full"),
            record(Some("a"), LogLevel::Info, "all good"),
        ];
        let selected = set(&["a"]);
        let by_level = log_rows(
            &records,
            &selected,
            ConsoleScope::All,
            ConsoleLogFilters::default(),
            "error",
        );
        assert_eq!(by_level.len(), 1, "the level matches: {by_level:?}");
        let by_message = log_rows(
            &records,
            &selected,
            ConsoleScope::All,
            ConsoleLogFilters::default(),
            "DISK",
        );
        assert_eq!(by_message.len(), 1, "the message matches, any case");
    }

    #[test]
    fn console_pause_snapshot_survives_new_records_and_resume_recomputes() {
        let records = vec![record(Some("a"), LogLevel::Info, "one")];
        let selected = set(&["a"]);
        let paused = log_rows(
            &records,
            &selected,
            ConsoleScope::All,
            ConsoleLogFilters::default(),
            "",
        );
        let mut grown = records.clone();
        grown.push(record(Some("a"), LogLevel::Info, "two"));
        let resumed = log_rows(
            &grown,
            &selected,
            ConsoleScope::All,
            ConsoleLogFilters::default(),
            "",
        );
        assert_eq!(paused.len(), 1, "the snapshot froze at one row");
        assert_eq!(resumed.len(), 2, "resume recomputes over new records");
        assert_eq!(
            paused,
            log_rows(
                &records,
                &selected,
                ConsoleScope::All,
                ConsoleLogFilters::default(),
                ""
            )
        );
    }

    #[test]
    fn console_traffic_poll_is_due_on_connections_or_traffic() {
        let now = 1_000_000i64;
        assert!(
            traffic_poll_due(true, ConsoleTab::Connections, now, now),
            "open on Connections polls immediately"
        );
        assert!(
            traffic_poll_due(true, ConsoleTab::Traffic, now, now),
            "open on Traffic polls immediately"
        );
        assert!(
            !traffic_poll_due(true, ConsoleTab::Logs, now, now),
            "open on Logs waits the base cadence"
        );
        assert!(
            !traffic_poll_due(false, ConsoleTab::Traffic, now, now),
            "closed console waits the base cadence"
        );
        assert!(
            traffic_poll_due(false, ConsoleTab::Logs, now, now - 5),
            "base cadence fires once elapsed"
        );
    }

    #[test]
    fn conn_rows_in_scope_matches_a_set_not_one_row() {
        let conn = |id: &str, row: Option<&str>| crate::health::ClashConn {
            id: id.to_string(),
            row_id: row.map(str::to_string),
            rx_bytes: 1,
            tx_bytes: 2,
            dest: "d".to_string(),
            network: "tcp".to_string(),
        };
        let snapshot = vec![
            ("stock".to_string(), conn("c0", Some("a"))),
            ("stock".to_string(), conn("c1", Some("b"))),
            ("stock".to_string(), conn("c2", None)),
        ];
        let selected = set(&["a", "b"]);
        let all = conn_rows_in_scope(&snapshot, &selected, ConsoleScope::All);
        assert_eq!(all.len(), 3, "All keeps unclaimed entries too");
        let scoped = conn_rows_in_scope(&snapshot, &selected, ConsoleScope::Selected);
        assert_eq!(scoped.len(), 2, "Selected matches the whole set");
    }
}
