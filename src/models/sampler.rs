use crate::*;

// --- T-30: per-shard CPU/RAM sampling (read-only, own children only) ---------
//
// The sidecars are this app's own child processes, so reading their kernel
// counters is the user's own information: no enumeration, no network, no
// telemetry. Everything below is bounded by the pid list the controller hands
// over; nothing here can name a process the app did not spawn.

/// Samples kept per shard, one per engine tick (`ENGINE_POLL_TICK_SECS`), so
/// the ring spans five minutes of history and never grows.
pub(crate) const SHARD_SAMPLE_RING: usize = 60;

/// `PROCESS_QUERY_LIMITED_INFORMATION` — the least right that still answers
/// `GetProcessTimes` / `GetProcessMemoryInfo`.
#[cfg(all(windows, not(test)))]
pub(crate) const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

/// `OpenProcess` says this when the pid exists but our token may not query it.
#[cfg(all(windows, not(test)))]
pub(crate) const ERROR_ACCESS_DENIED: u32 = 5;

/// One tick's raw counters for one shard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ProcessSample {
    /// Kernel + user CPU time in 100 ns units (`GetProcessTimes`).
    pub(crate) cpu_100ns: u64,
    /// `PROCESS_MEMORY_COUNTERS.WorkingSetSize` in bytes.
    pub(crate) ram_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SampleRead {
    Sampled(ProcessSample),
    /// Pid is gone (exited, or exited between ticks): drop it silently.
    Gone,
    /// The OS refused the handle (permission): say so instead of guessing.
    Denied,
}

/// The single seam between the sampler and Win32. Production points it at
/// [`win32_process_sample`]; tests inject canned readings, so `cargo test`
/// never opens a process handle.
pub(crate) type ProcessSampler = Box<dyn Fn(u32) -> SampleRead>;

/// CPU as a percentage of **one core** over `window`: `cpu delta / wall
/// time`. Both sides are in 100 ns units, so a sidecar that saturates one core
/// reads 100.0 and a multi-threaded one legitimately reads above it (each
/// extra core adds 100). `None` when there is no honest number to plot: an
/// empty window, or a counter that went backwards (a different process
/// generation took the pid).
pub(crate) fn cpu_percent_of_one_core(
    prev_cpu_100ns: u64,
    now_cpu_100ns: u64,
    window: Duration,
) -> Option<f32> {
    let window_100ns = window.as_nanos() / 100;
    if window_100ns == 0 {
        return None;
    }
    let delta = now_cpu_100ns.checked_sub(prev_cpu_100ns)?;
    Some((delta as f64 / window_100ns as f64 * 100.0) as f32)
}

/// CPU (% of one core) and RAM (MB) series for one shard, oldest first.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ShardRing {
    pub(crate) cpu: Vec<f32>,
    pub(crate) ram: Vec<f32>,
}

pub(crate) fn push_shard_sample(series: &mut Vec<f32>, value: f32) {
    if series.len() >= SHARD_SAMPLE_RING {
        series.remove(0);
    }
    series.push(value);
}

/// Fold one tick of readings into the rings.
///
/// - labels that are no longer live are evicted (shard died or was renamed);
/// - a changed pid is a respawn: its ring starts empty, and the old baseline
///   is gone, so no sample ever mixes two process generations;
/// - the first sample of a generation only primes the baseline (there is no
///   delta yet), so charts start on the second tick;
/// - a gone pid drops the shard's ring without a trace.
///
/// Returns the labels whose handle the OS refused on this tick.
pub(crate) fn fold_shard_samples(
    rings: &mut BTreeMap<String, ShardRing>,
    baselines: &mut BTreeMap<String, (u32, ProcessSample, Instant)>,
    live: &[(String, u32)],
    sampler: &dyn Fn(u32) -> SampleRead,
    at: Instant,
) -> HashSet<String> {
    rings.retain(|label, _| live.iter().any(|(name, _)| name == label));
    baselines.retain(|label, _| live.iter().any(|(name, _)| name == label));

    let mut denied = HashSet::new();
    for (label, pid) in live {
        let previous = baselines
            .get(label)
            .and_then(|(prev_pid, sample, prev_at)| {
                (*prev_pid == *pid).then_some((*sample, *prev_at))
            });
        if baselines.contains_key(label) && previous.is_none() {
            rings.insert(label.clone(), ShardRing::default());
            baselines.remove(label);
        }
        match sampler(*pid) {
            SampleRead::Sampled(sample) => {
                let ring = rings.entry(label.clone()).or_default();
                if let Some((prev_sample, prev_at)) = previous {
                    let window = at.saturating_duration_since(prev_at);
                    if let Some(cpu) =
                        cpu_percent_of_one_core(prev_sample.cpu_100ns, sample.cpu_100ns, window)
                    {
                        push_shard_sample(&mut ring.cpu, cpu);
                        push_shard_sample(&mut ring.ram, sample.ram_bytes as f32 / 1_048_576.0);
                    }
                }
                baselines.insert(label.clone(), (*pid, sample, at));
            }
            SampleRead::Gone => {
                rings.remove(label);
                baselines.remove(label);
            }
            SampleRead::Denied => {
                denied.insert(label.clone());
            }
        }
    }
    denied
}
