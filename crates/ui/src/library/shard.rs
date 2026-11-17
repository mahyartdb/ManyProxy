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

/// The fingerprint the Runtime group compares before rebuilding its rows:
/// every input `shard_stat_rows` reads, cheap to hash and exact.
pub fn shard_stat_fingerprint(
    shards: &[(String, Option<u32>)],
    rings: &BTreeMap<String, ShardRing>,
    denied: &HashSet<String>,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    0u8.hash(&mut hasher);
    for (label, pid) in shards {
        label.hash(&mut hasher);
        pid.hash(&mut hasher);
    }
    1u8.hash(&mut hasher);
    for (label, ring) in rings {
        label.hash(&mut hasher);
        ring.cpu.len().hash(&mut hasher);
        for value in &ring.cpu {
            value.to_bits().hash(&mut hasher);
        }
        ring.ram.len().hash(&mut hasher);
        for value in &ring.ram {
            value.to_bits().hash(&mut hasher);
        }
    }
    2u8.hash(&mut hasher);
    // Hash each label into its own hasher and XOR the results: a HashSet
    // iterates in an unstable order, and a straight fold would give identical
    // contents a different fingerprint every tick, defeating the cache while
    // any shard is sampling-denied. XOR is order-independent. The combined
    // value is finished once and fed as a single u64 after the section tag,
    // so the XOR cannot interact with the sections' hash stream; keep it
    // that way — splicing the per-label hashes into the outer hasher would
    // reintroduce the churn this exists to prevent.
    let mut denied_hash: u64 = 0;
    for label in denied {
        let mut label_hasher = std::collections::hash_map::DefaultHasher::new();
        label.hash(&mut label_hasher);
        denied_hash ^= label_hasher.finish();
    }
    denied_hash.hash(&mut hasher);
    hasher.finish()
}

/// Sparkline-shaped line for one series. 120×24, smaller than the health
/// sparkline's 160×32 (`AppView::latency_sparkline`); one chart per series so
/// CPU and RAM never share a scale, and the caller picks the stroke so the two
/// series read apart.
pub fn shard_sparkline(series: &[f32], stroke: Hsla) -> impl IntoElement {
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
            .grid(false)
            .stroke(stroke),
    )
}

pub fn shard_stat_row_view(
    row: &ShardStatRow,
    muted: Hsla,
    cpu_stroke: Hsla,
    ram_stroke: Hsla,
) -> AnyElement {
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
                    .text_color(muted)
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
                        .child(
                            div()
                                .id(format!("shard-cpu-{}-wrap", row.label))
                                .test_support()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1()
                                .child(
                                    div()
                                        .id(format!("shard-cpu-key-{}", row.label))
                                        .test_support()
                                        .text_xs()
                                        .text_color(cpu_stroke)
                                        .child(t!("shard-cpu-key")),
                                )
                                .child(shard_sparkline(&row.cpu, cpu_stroke)),
                        )
                        .child(
                            div()
                                .id(format!("shard-ram-{}-wrap", row.label))
                                .test_support()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1()
                                .child(
                                    div()
                                        .id(format!("shard-ram-key-{}", row.label))
                                        .test_support()
                                        .text_xs()
                                        .text_color(ram_stroke)
                                        .child(t!("shard-ram-key")),
                                )
                                .child(shard_sparkline(&row.ram, ram_stroke)),
                        ),
                );
        }
    }
    entry.into_any_element()
}
