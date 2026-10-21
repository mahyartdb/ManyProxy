# T-30 — CPU/RAM mini-charts per sidecar shard

- **id**: T-30
- **title**: Runtime-stats CPU/RAM mini-charts per sidecar shard — local process sampling (GetProcessTimes / GetProcessMemoryInfo), 60-sample ring per shard
- **epic**: E6 — UI affordance pack
- **size**: M
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E6 T-30 line ("local process sampling … per-shard 60-sample ring, gpui LineChart; no per-app attribution (stock clash API has none — do not fake)"); gap §4.3 U8 (Throne §F.2 pairs core CPU/RAM with a 2 GiB watchdog; "a visible CPU/RAM view is the observability slice that stays telemetry-free — it is the user's own process"); verified: backend child handles exist (`RealEngineBackend.child: Option<std::process::Child>`, engine.rs:5299; `WaitTarget for std::process::Child` 5265), EngineController backends (engine.rs:5771–5778: stock/fork/xray/extra vec), `EngineStatus` enum (engine.rs:5765), runtime-stats dialog content today = uptime + traffic totals text (`format_uptime` main.rs:11081, region 11040–11107), gpui-kit `LineChart` sparkline precedent (`url_test_sparkline`, health.rs:1109).

## Why

Sidecar pain is invisible: a hung core eating 90% CPU or a leaking fork looks like normal operation until ports die. The processes are *the user's own app's children* — sampling them locally is the honest, telemetry-free observability slice (U8), reusing componentry proven by the sparkline (LineChart draws health history today). No network, no per-app pretense.

## What

1. **PID cohort exposure** (engine.rs): new public accessor on `EngineController`, e.g. `pub fn shard_processes(&self) -> Vec<(String, Option<u32>)>` returning one `(label, pid)` per live shard (`"stock"`, `"fork"`, `"xray"`, `"extra-<id>"` mirroring `poll_shards` naming at engine.rs:7080) — backed by each backend's `child.id()`. `None` = not running (skip/out-dose the sample).
2. **Sampler** (main.rs on the existing 5 s engine tick): for each live shard, `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` by pid then:
   - CPU: `GetProcessTimes` delta over the tick window → % of one core (normalize by elapsed wall time; single-core normalization documented),
   - RAM: `GetProcessMemoryInfo.WorkingSetSize` (psapi).
   Hand-rolled `extern "system"` declarations (kernel32!OpenProcess/GetProcessTimes, psapi!GetProcessMemoryInfo) — zero new crates (consistent with the crate's existing raw Win32 posture). Dead pids (process exited) drop silently from the ring.
3. **Ring**: per-shard label keyed 60-sample ring (label → `(Vec<f32>, Vec<f32>)` cpu/ram in a plain BTreeMap in AppView state; evict on shard death/rename).
4. **Render**: Runtime-stats dialog (region 11040–11107) adds per-shard mini-charts via gpui-kit `LineChart` same component family as the health sparkline — one chart per shard with its label; 0-probe/no-live → "not running" caption matching dialog tone.
5. **No watchdog, no kills, no auto-mitigation** — displayOnly (Throne's 2 GiB forced-GC watchdog is explicitly not adopted; if the owner wants it later that's a new grilling, not this ticket).

## Files to touch

- `src/engine.rs` — `shard_processes()` accessor + tests (fake backends can feed known pids)
- `src/main.rs` — sampler fns (FFI helpers), ring state, tick wiring, dialog charts + tests

## Acceptance criteria

- [ ] Each running shard's CPU/RAM mini-chart populates within two engine ticks and follows the shard label; stopped shards' charts show "not running"/stale-free removal.
- [ ] Sampler reads only the app's own child pids (no process enumeration of others; refuse-to-sample if pid mismatch — bounded to spawned children).
- [ ] Charts are stable under shard restart (fresh ring after a respawn; no data pollution across generations).
- [ ] Zero new network traffic and zero new crates for the FFI; permission failures (rare UWP-shaped contexts) degrade to a "(sampling unavailable)" caption — no crash, no retry storm.
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t80_`, verified unused): `t80_shard_processes_lists_live_pids` (fake backend), `t80_sampler_delta_calculations` (pure math — inject fake raw counts), `t80_ring_evicts_dead_shards`, `t80_runtime_stats_render_per_shard`, `t80_no_sampling_race_after_restart`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData / No-migration / Hijack: N/A.
- [x] **No system mutation**: reading own-children process stats = the user's own information; scope-closed to spawned shards (helper guards: pid must belong to a currently-tracked backend — enforced by construction, tests pin it). No telemetry anywhere — data stays in-session memory.
- [x] Plaintext / Redaction: N/A (numbers only).
- [x] Clipboard discipline: N/A.
- [x] Sidecars app-dir only: N/A (read-only observation, no sidecar change).

## Out of scope

- Memory watchdog / forced restarts / GC tripping (Throne §E pairing — not adopted).
- Per-outbound or per-app attribution (impossible from the stock clash API — dead; **do not fake it**).
- History beyond the 60-sample in-session ring (no DB columns).
- Windows service/container metrics.
