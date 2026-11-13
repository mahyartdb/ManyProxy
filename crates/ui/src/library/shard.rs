use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShardCaption {
    NotRunning,
    SamplingUnavailable,
    Collecting,
}

impl ShardCaption {
    pub fn label(self) -> String {
        match self {
            ShardCaption::NotRunning => t!("shard-not-running"),
            ShardCaption::SamplingUnavailable => t!("shard-sampling-unavailable"),
            ShardCaption::Collecting => t!("shard-collecting"),
        }
    }
}

/// One row of the Runtime-stats per-shard section: either two plottable series
/// or a caption saying why there is nothing to plot.
#[derive(Debug, Clone, PartialEq)]
pub struct ShardStatRow {
    pub label: String,
    pub cpu: Vec<f32>,
    pub ram: Vec<f32>,
    pub caption: Option<ShardCaption>,
}

pub fn shard_stat_rows(
    shards: &[(String, Option<u32>)],
    rings: &BTreeMap<String, ShardRing>,
    denied: &HashSet<String>,
) -> Vec<ShardStatRow> {
    shards
        .iter()
        .map(|(label, pid)| {
            let ring = rings.get(label);
            let caption = if pid.is_none() {
                Some(ShardCaption::NotRunning)
            } else if denied.contains(label) {
                Some(ShardCaption::SamplingUnavailable)
            } else if ring.is_none_or(|ring| ring.cpu.len() < 2) {
                Some(ShardCaption::Collecting)
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
pub fn shard_sparkline(series: &[f32]) -> impl IntoElement {
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

pub fn shard_stat_row_view(row: &ShardStatRow) -> AnyElement {
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
                    .child(format!("{}: {}", row.label, caption.label())),
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
                        .child(t!(
                            "shard-stat-line",
                            label = row.label.as_str(),
                            cpu = format!("{latest_cpu:.1}"),
                            ram = format!("{latest_ram:.1}")
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
