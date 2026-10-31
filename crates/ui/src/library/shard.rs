use crate::*;

pub(crate) const SHARD_NOT_RUNNING: &str = "not running";
pub(crate) const SHARD_SAMPLING_UNAVAILABLE: &str = "(sampling unavailable)";
pub(crate) const SHARD_COLLECTING: &str = "collecting first sample";

/// One row of the Runtime-stats per-shard section: either two plottable series
/// or a caption saying why there is nothing to plot.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ShardStatRow {
    pub(crate) label: String,
    pub(crate) cpu: Vec<f32>,
    pub(crate) ram: Vec<f32>,
    pub(crate) caption: Option<&'static str>,
}

pub(crate) fn shard_stat_rows(
    shards: &[(String, Option<u32>)],
    rings: &BTreeMap<String, ShardRing>,
    denied: &HashSet<String>,
) -> Vec<ShardStatRow> {
    shards
        .iter()
        .map(|(label, pid)| {
            let ring = rings.get(label);
            let caption = if pid.is_none() {
                Some(SHARD_NOT_RUNNING)
            } else if denied.contains(label) {
                Some(SHARD_SAMPLING_UNAVAILABLE)
            } else if ring.is_none_or(|ring| ring.cpu.len() < 2) {
                Some(SHARD_COLLECTING)
            } else {
                None
            };
            ShardStatRow {
                label: label.clone(),
                cpu: ring.map(|ring| ring.cpu.clone()).unwrap_or_default(),
                ram: ring.map(|ring| ring.ram.clone()).unwrap_or_default(),
                caption,
            }
        })
        .collect()
}

/// Sparkline-shaped line for one series: same `LineChart` family and sizing as
/// the health sparkline, one per series so CPU and RAM never share a scale.
pub(crate) fn shard_sparkline(series: &[f32]) -> impl IntoElement {
    struct SeriesPoint {
        x: String,
        y: f64,
    }
    let points: Vec<SeriesPoint> = series
        .iter()
        .enumerate()
        .map(|(index, value)| SeriesPoint {
            x: index.to_string(),
            y: f64::from(*value),
        })
        .collect();
    div().w(px(120.)).h(px(24.)).child(
        LineChart::new(points)
            .x(|point: &SeriesPoint| point.x.clone())
            .y(|point: &SeriesPoint| point.y)
            .x_axis(false)
            .grid(false),
    )
}

pub(crate) fn shard_stat_row_view(row: &ShardStatRow) -> AnyElement {
    let mut entry = div()
        .id(format!("shard-stat-{}", row.label))
        .test_support()
        .flex()
        .flex_col()
        .gap_1();
    match row.caption {
        Some(caption) => {
            entry = entry.child(
                div()
                    .id(format!("shard-caption-{}", row.label))
                    .test_support()
                    .text_xs()
                    .child(format!("{}: {caption}", row.label)),
            );
        }
        None => {
            let latest_cpu = row.cpu.last().copied().unwrap_or(0.0);
            let latest_ram = row.ram.last().copied().unwrap_or(0.0);
            entry = entry
                .child(
                    div()
                        .id(format!("shard-label-{}", row.label))
                        .test_support()
                        .text_xs()
                        .child(format!(
                            "{} · CPU {latest_cpu:.1}% of one core · RAM {latest_ram:.1} MB",
                            row.label
                        )),
                )
                .child(
                    div()
                        .id(format!("shard-chart-{}", row.label))
                        .test_support()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(shard_sparkline(&row.cpu))
                        .child(shard_sparkline(&row.ram)),
                );
        }
    }
    entry.into_any_element()
}
