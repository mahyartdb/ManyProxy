use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConnRow {
    pub(crate) core: String,
    pub(crate) id: String,
    pub(crate) dest: String,
    pub(crate) row_id: Option<String>,
    pub(crate) rx_bytes: u64,
    pub(crate) tx_bytes: u64,
    pub(crate) delta_rx: u64,
    pub(crate) delta_tx: u64,
}

impl ConnRow {
    pub(crate) fn total(&self) -> u64 {
        self.rx_bytes.saturating_add(self.tx_bytes)
    }
}

/// The only two Connections-tab cells that click-to-copy: the destination cell
/// and the per-row exit endpoint (`bind:port`). Indexed by painted row order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnCopyTarget {
    Dest(usize),
    Endpoint(usize),
}

pub(crate) fn build_conn_rows(
    snapshot: &[(String, crate::health::ClashConn)],
    deltas: &HashMap<String, (u64, u64)>,
    selected_row: Option<&str>,
    selected_only: bool,
    desc: bool,
) -> Vec<ConnRow> {
    let mut rows: Vec<ConnRow> = snapshot
        .iter()
        .filter(
            |(_, entry)| match (selected_only, selected_row, entry.row_id.as_deref()) {
                (true, Some(selected), Some(row)) => row == selected,
                (true, Some(_), None) => false,
                _ => true,
            },
        )
        .map(|(core, entry)| {
            let key = format!("{core}/{}", entry.id);
            let (delta_rx, delta_tx) = deltas.get(&key).copied().unwrap_or((0, 0));
            ConnRow {
                core: core.clone(),
                id: entry.id.clone(),
                dest: entry.dest.clone(),
                row_id: entry.row_id.clone(),
                rx_bytes: entry.rx_bytes,
                tx_bytes: entry.tx_bytes,
                delta_rx,
                delta_tx,
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        if desc {
            b.total().cmp(&a.total())
        } else {
            a.total().cmp(&b.total())
        }
    });
    rows
}
