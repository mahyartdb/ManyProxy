use std::collections::{BTreeMap, HashSet};
use std::time::{Duration, Instant};

// --- T-30: per-shard CPU/RAM sampling (read-only, own children only) ---------
//
// The sidecars are this app's own child processes, so reading their kernel
// counters is the user's own information: no enumeration, no network, no
// telemetry. Everything below is bounded by the pid list the controller hands
// over; nothing here can name a process the app did not spawn.

/// Samples kept per shard, one per engine tick (`ENGINE_POLL_TICK_SECS`), so
/// the ring spans five minutes of history and never grows.
pub const SHARD_SAMPLE_RING: usize = 60;

/// `PROCESS_QUERY_LIMITED_INFORMATION` — the least right that still answers
/// `GetProcessTimes` / `GetProcessMemoryInfo`.
#[cfg(all(windows, not(test)))]
pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

/// `OpenProcess` says this when the pid exists but our token may not query it.
#[cfg(all(windows, not(test)))]
pub const ERROR_ACCESS_DENIED: u32 = 5;

/// One tick's raw counters for one shard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProcessSample {
    /// Kernel + user CPU time in 100 ns units (`GetProcessTimes`).
    pub cpu_100ns: u64,
    /// `PROCESS_MEMORY_COUNTERS.WorkingSetSize` in bytes.
    pub ram_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleRead {
    Sampled(ProcessSample),
    /// Pid is gone (exited, or exited between ticks): drop it silently.
    Gone,
    /// The OS refused the handle (permission): say so instead of guessing.
    Denied,
}

/// The single seam between the sampler and Win32. Production points it at
/// [`win32_process_sample`]; tests inject canned readings, so `cargo test`
/// never opens a process handle.
pub type ProcessSampler = Box<dyn Fn(u32) -> SampleRead>;

/// CPU as a percentage of **one core** over `window`: `cpu delta / wall
/// time`. Both sides are in 100 ns units, so a sidecar that saturates one core
/// reads 100.0 and a multi-threaded one legitimately reads above it (each
/// extra core adds 100). `None` when there is no honest number to plot: an
/// empty window, or a counter that went backwards (a different process
/// generation took the pid).
pub fn cpu_percent_of_one_core(
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
pub struct ShardRing {
    pub cpu: Vec<f32>,
    pub ram: Vec<f32>,
}

pub fn push_shard_sample(series: &mut Vec<f32>, value: f32) {
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
pub fn fold_shard_samples(
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

/// Read one child's counters. Windows-only, same posture as `dpapi`: raw
/// `extern "system"` declarations, no crate, and the handle is closed on every
/// path.
#[cfg(all(windows, not(test)))]
mod win32_process {
    use std::ffi::c_void;

    use super::{
        ProcessSample, SampleRead, ERROR_ACCESS_DENIED, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    impl FileTime {
        /// `FILETIME` counts 100 ns ticks in two halves.
        fn ticks(&self) -> u64 {
            (u64::from(self.high) << 32) | u64::from(self.low)
        }
    }

    /// `PROCESS_MEMORY_COUNTERS` — only `WorkingSetSize` is read; the rest
    /// keeps the layout the callee fills.
    #[repr(C)]
    #[derive(Default)]
    struct ProcessMemoryCounters {
        cb: u32,
        _page_fault_count: u32,
        _peak_working_set_size: usize,
        working_set_size: usize,
        _quota_peak_paged_pool_usage: usize,
        _quota_paged_pool_usage: usize,
        _quota_peak_non_paged_pool_usage: usize,
        _quota_non_paged_pool_usage: usize,
        _pagefile_usage: usize,
        _peak_pagefile_usage: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> *mut c_void;
        fn GetProcessTimes(
            process: *mut c_void,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn GetLastError() -> u32;
    }

    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    /// Owns the query handle; dropping it is the only side effect in here.
    struct OwnedHandle(*mut c_void);

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    pub(super) fn sample(pid: u32) -> SampleRead {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return if GetLastError() == ERROR_ACCESS_DENIED {
                    SampleRead::Denied
                } else {
                    SampleRead::Gone
                };
            }
            let handle = OwnedHandle(handle);

            let mut creation = FileTime::default();
            let mut exit = FileTime::default();
            let mut kernel = FileTime::default();
            let mut user = FileTime::default();
            if GetProcessTimes(handle.0, &mut creation, &mut exit, &mut kernel, &mut user) == 0 {
                return SampleRead::Gone;
            }

            let cb = std::mem::size_of::<ProcessMemoryCounters>() as u32;
            let mut counters = ProcessMemoryCounters {
                cb,
                ..Default::default()
            };
            if GetProcessMemoryInfo(handle.0, &mut counters, cb) == 0 {
                return SampleRead::Gone;
            }

            SampleRead::Sampled(ProcessSample {
                cpu_100ns: kernel.ticks().saturating_add(user.ticks()),
                ram_bytes: counters.working_set_size as u64,
            })
        }
    }
}

#[cfg(all(windows, not(test)))]
pub fn win32_process_sample(pid: u32) -> SampleRead {
    win32_process::sample(pid)
}

/// Tests inject their own readings (no handle ever opens under `cargo test`),
/// and non-Windows builds have no Win32 to ask — both degrade to the caption
/// instead of pretending a number exists.
#[cfg(any(test, not(windows)))]
pub fn win32_process_sample(_pid: u32) -> SampleRead {
    SampleRead::Denied
}
