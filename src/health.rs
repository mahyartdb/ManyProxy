//! Ticket 06: per-config health, traffic stats, and sidecar logs.
//!
//! One [`HealthStore`] per window holds append-only [`HealthSample`]s (latest
//! wins per row), accumulated [`TrafficStats`], and a [`LogStore`] of
//! redacted [`LogRecord`]s tagged by config where possible.
//!
//! The measure seam is [`HealthProber`]: the real driver dials each config's
//! own local endpoint (so a URL test can only ever exit via that config's
//! egress), the fake driver scripts latencies/failures for tests. No test
//! touches a real remote server. Ticket 08 flips Xray/custom/extra-core/
//! chained rows on: they are probeable through their own endpoints exactly
//! like stock rows (extra-core slot: active-profile tests only — the runner
//! only ever probes running endpoints, never group bulk sets; there is no
//! group-test path in v1). Ticket 07 flips AmneziaWG variants on: AWG rows
//! are probeable through their own endpoints exactly like stock rows.
//!
//! Hard prohibitions: health checks never route through another config's
//! egress (each probe dials its own `bind:port`); secrets never reach stored
//! or displayed log lines ([`redact_secrets`] runs on every ingest).

use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
/// Ticket 26: the fake prober crosses threads (health probes run on the
/// background executor via `spawn_probe`), so its shared script state is
/// `Arc<Mutex<…>>` instead of `Rc<RefCell<…>>`. The scripting API
/// (`shared`, latencies, failures, deny-list, call log) is unchanged.
#[cfg(test)]
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::engine::{inbound_tag, outbound_tag};
use crate::store::{now_unix, HealthSnapshot, ProxyConfig};

/// Default URL-test target: plain `http` (the local mixed inbound serves an
/// HTTP-proxy GET directly; `https` would need a CONNECT+TLS stack v1 does
/// not carry) returning 204.
pub const DEFAULT_TEST_URL: &str = "http://www.gstatic.com/generate_204";
/// Per-test ceiling so 5 rows finish inside the 30s scenario budget.
pub const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
/// Ticket 36: host of the health-test URL — clash flows to this host are
/// probes, never data-plane bytes (single source of truth derived from
/// [`DEFAULT_TEST_URL`], never a second literal to drift).
pub fn probe_host() -> String {
    split_http_url(DEFAULT_TEST_URL)
        .map(|(_, host)| host.to_string())
        .unwrap_or_default()
}
/// In-memory bounds (durable persistence defers to a later ticket).
const MAX_SAMPLES: usize = 10_000;
#[cfg_attr(not(test), allow(dead_code))]
const MAX_LOG_RECORDS: usize = 5_000;

/// What kind of measurement a sample holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthKind {
    UrlTest,
    TcpPing,
}

impl HealthKind {
    pub fn as_str(self) -> &'static str {
        match self {
            HealthKind::UrlTest => "url-test",
            HealthKind::TcpPing => "tcp-ping",
        }
    }
}

/// One measurement for one config (data-model `HealthSample`). Success has
/// `latency_ms` set and `error` clear; failure is the reverse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthSample {
    pub config_id: String,
    pub at: i64,
    pub kind: HealthKind,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

/// Bytes moved by successful probes (accumulated per config).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProbeSuccess {
    pub latency_ms: u64,
    pub bytes_rx: u64,
    pub bytes_tx: u64,
}

/// One probe result: the append-only sample plus the success payload (if
/// any) that feeds traffic counters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestOutcome {
    pub sample: HealthSample,
    pub success: Option<ProbeSuccess>,
}

/// Mock-friendly measure seam. Every method receives the config's own local
/// endpoint (`bind`/`port`); implementations must dial exactly that address
/// so results are per-egress. Object-safe so the UI holds a boxed driver.
pub trait HealthProber {
    /// False for rows this driver cannot measure (extra-core/custom until
    /// ticket 08). The runner records a named skip without calling the
    /// probe methods.
    /// Ticket 28: the permissive default stays — both in-tree drivers
    /// override it, and it keeps future test doubles total.
    fn supports(&self, config: &ProxyConfig) -> bool {
        let _ = config;
        true
    }
    /// GET `url` through the HTTP-proxy face of `bind:port`.
    /// Ticket 47: `&self` (was `&mut self`) — probes run concurrently on
    /// scoped threads behind [`test_all`]/[`ping_all`], so drivers share
    /// through the reference (`RealHealthProber` is stateless; the fake
    /// already synchronizes through `Arc<Mutex<…>>`). Outcomes (order,
    /// content, per-egress routing) are unchanged.
    fn url_test(
        &self,
        config: &ProxyConfig,
        bind: &str,
        port: u16,
        url: &str,
    ) -> Result<ProbeSuccess, String>;
    /// Time one TCP dial to `bind:port` (endpoint liveness).
    /// Ticket 47: `&self` like [`HealthProber::url_test`] (shared across
    /// probe threads; semantics unchanged).
    fn tcp_ping(&self, config: &ProxyConfig, bind: &str, port: u16)
        -> Result<ProbeSuccess, String>;
}

/// Protocols measurable in this slice. Mirrors the ticket-05 standard set
/// plus ticket-07 AmneziaWG variants plus ticket-08 Xray/custom/extra-core/
/// chained rows plus ticket-27 SSH/AnyTLS/Noise natives (each probe still
/// dials the row's own endpoint, so every row is measured through its own
/// egress). Anything else (socks, naive, …) is a named skip until its owning
/// ticket.
/// Ticket 28: kept parallel to the engine/store/import tables — probeability
/// is a measurement capability list, not a parse or routing taxonomy.
pub fn is_probeable_protocol(protocol: &str) -> bool {
    matches!(
        protocol.trim().to_ascii_lowercase().as_str(),
        "shadowsocks"
            | "ss"
            | "trojan"
            | "vmess"
            | "vless"
            | "tuic"
            | "hysteria"
            | "hysteria2"
            | "hy2"
            | "wireguard"
            | "wg"
            | "amneziawg"
            | "awg"
            | "amnezia-wg"
            | "anytls"
            | "ssh"
            | "noise"
            | "xray"
            | "xray-vless"
            | "xray_vless"
            | "vless-xray"
            | "custom"
            | "custom-out"
            | "custom-outbound"
            | "custom_outbound"
            | "custom-outbound-json"
            | "custom-config"
            | "custom_config"
            | "custom-full"
            | "custom_full"
            | "custom-full-config"
            | "extra"
            | "extra-core"
            | "extra_core"
            | "extracore"
            | "chain"
            | "chained"
            | "chain-outbound"
            | "chain_outbound"
    )
}

/// Production driver: blocking TCP dials with a per-test timeout. Ticket 26:
/// the blocking calls run on the background executor via
/// `AppView::spawn_probe` — never on the UI thread.
pub struct RealHealthProber {
    timeout: Duration,
}

impl RealHealthProber {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl Default for RealHealthProber {
    fn default() -> Self {
        Self::new(DEFAULT_PROBE_TIMEOUT)
    }
}

/// Resolve a local endpoint for dialing. Wildcard binds (`0.0.0.0`/`::`)
/// are served on loopback, so dial loopback instead of the unspecified
/// address (which can never connect).
fn endpoint_addr(bind: &str, port: u16) -> Result<SocketAddr, String> {
    let trimmed = bind.trim();
    if let Ok(ip) = trimmed.parse::<std::net::IpAddr>() {
        let host = if ip.is_unspecified() {
            if ip.is_ipv4() {
                "127.0.0.1".to_string()
            } else {
                "::1".to_string()
            }
        } else {
            trimmed.to_string()
        };
        // IPv6 literals need brackets in `ip:port` form.
        let socket = if host.contains(':') {
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        };
        socket
            .parse::<SocketAddr>()
            .map_err(|_| format!("bad endpoint {host}:{port}"))
    } else {
        format!("{trimmed}:{port}")
            .to_socket_addrs()
            .ok()
            .and_then(|mut addrs| addrs.next())
            .ok_or_else(|| format!("cannot resolve endpoint {trimmed}:{port}"))
    }
}

/// Split `scheme://host/rest`. Errors never echo the URL (queries carry
/// tokens).
fn split_http_url(url: &str) -> Result<(&str, &str), String> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| "bad test url (want http://host/...)".to_string())?;
    let host = rest.split('/').next().unwrap_or("");
    if host.trim().is_empty() {
        return Err("bad test url (want http://host/...)".to_string());
    }
    Ok((scheme, host))
}

impl HealthProber for RealHealthProber {
    fn supports(&self, config: &ProxyConfig) -> bool {
        is_probeable_protocol(&config.protocol)
    }

    fn url_test(
        &self,
        _config: &ProxyConfig,
        bind: &str,
        port: u16,
        url: &str,
    ) -> Result<ProbeSuccess, String> {
        let endpoint = endpoint_addr(bind, port)?;
        let (scheme, host) = split_http_url(url)?;
        if !scheme.eq_ignore_ascii_case("http") {
            return Err(format!(
                "unsupported url scheme \"{scheme}\" for url test (use an http:// test url)"
            ));
        }
        let started = Instant::now();
        let mut stream = TcpStream::connect_timeout(&endpoint, self.timeout)
            .map_err(|error| format!("dial {endpoint}: {error}"))?;
        stream.set_read_timeout(Some(self.timeout)).ok();
        stream.set_write_timeout(Some(self.timeout)).ok();
        let request = format!(
            "GET {url} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nUser-Agent: ManyProxy-health\r\n\r\n"
        );
        let tx_bytes = request.len() as u64;
        stream
            .write_all(request.as_bytes())
            .map_err(|error| format!("url test via {endpoint}: write failed ({error})"))?;
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    if buf.len() >= 65_536 || find_headers_end(&buf).is_some() {
                        break;
                    }
                }
                Err(error) => {
                    return Err(format!("url test via {endpoint}: read failed ({error})"));
                }
            }
        }
        let rx_bytes = buf.len() as u64;
        let head = String::from_utf8_lossy(&buf);
        let status_line = head.lines().next().unwrap_or("");
        let code = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse::<u16>().ok())
            .ok_or_else(|| format!("url test via {endpoint}: bad proxy response"))?;
        if (200..400).contains(&code) {
            Ok(ProbeSuccess {
                latency_ms: started.elapsed().as_millis() as u64,
                bytes_rx: rx_bytes,
                bytes_tx: tx_bytes,
            })
        } else {
            Err(format!("url test via {endpoint}: unexpected status {code}"))
        }
    }

    fn tcp_ping(
        &self,
        _config: &ProxyConfig,
        bind: &str,
        port: u16,
    ) -> Result<ProbeSuccess, String> {
        let endpoint = endpoint_addr(bind, port)?;
        let started = Instant::now();
        TcpStream::connect_timeout(&endpoint, self.timeout)
            .map_err(|error| format!("tcp ping {endpoint}: {error}"))?;
        Ok(ProbeSuccess {
            latency_ms: started.elapsed().as_millis() as u64,
            bytes_rx: 0,
            bytes_tx: 0,
        })
    }
}

fn find_headers_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|pos| pos + 4)
}

/// Ticket 28: shared runner behind [`test_all`] and [`ping_all`] (one
/// skip/probe/failure shape; only the kind, verb, and probe call differ).
/// Ticket 47: shared through `&` (probes run concurrently).
type ProbeFn = fn(&dyn HealthProber, &ProxyConfig, &str, u16, &str) -> Result<ProbeSuccess, String>;

/// Ticket 47: max probe threads per round. Test-all on N rows overlaps at
/// most this many blocking dials; outcomes still land in input order, so
/// probe semantics (per-egress routing, skip/failure shapes) are unchanged.
pub const MAX_PROBE_CONCURRENCY: usize = 8;

/// Ticket 47: one probe of one config (the per-row body shared by the
/// sequential and concurrent runners — one skip/probe/failure shape).
fn run_one(
    config: &ProxyConfig,
    prober: &dyn HealthProber,
    kind: HealthKind,
    verb: &str,
    probe: ProbeFn,
    url: &str,
) -> TestOutcome {
    let at = now_unix();
    if !prober.supports(config) {
        return TestOutcome {
            sample: HealthSample {
                config_id: config.id.clone(),
                at,
                kind,
                latency_ms: None,
                error: Some(format!(
                    "unsupported protocol \"{}\" for \"{}\" (no health probe for this protocol yet)",
                    config.protocol, config.name
                )),
            },
            success: None,
        };
    }
    match probe(prober, config, &config.bind_ip, config.port, url) {
        Ok(success) => TestOutcome {
            sample: HealthSample {
                config_id: config.id.clone(),
                at,
                kind,
                latency_ms: Some(success.latency_ms),
                error: None,
            },
            success: Some(success),
        },
        Err(cause) => TestOutcome {
            sample: HealthSample {
                config_id: config.id.clone(),
                at,
                kind,
                latency_ms: None,
                error: Some(format!(
                    "{verb} failed for \"{}\": {}",
                    config.name,
                    redact_secrets(&cause)
                )),
            },
            success: None,
        },
    }
}

fn run_all(
    configs: &[ProxyConfig],
    prober: &dyn HealthProber,
    kind: HealthKind,
    verb: &str,
    probe: ProbeFn,
    url: &str,
) -> Vec<TestOutcome> {
    configs
        .iter()
        .map(|config| run_one(config, prober, kind, verb, probe, url))
        .collect()
}

/// Ticket 47: bounded concurrent runner — at most [`MAX_PROBE_CONCURRENCY`]
/// workers pull row indices off an atomic counter (scoped threads, joined
/// before return; no tasks leak past the round). Outcomes are re-sorted
/// into input order, so callers observe exactly the sequential contract.
/// `prober` is shared (`&(dyn HealthProber + Sync)`); drivers must be
/// `Sync` (the real driver is stateless, the fake is `Arc<Mutex>`).
fn run_all_concurrent(
    configs: &[ProxyConfig],
    prober: &(dyn HealthProber + Sync),
    kind: HealthKind,
    verb: &str,
    probe: ProbeFn,
    url: &str,
) -> Vec<TestOutcome> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    if configs.is_empty() {
        return Vec::new();
    }
    if configs.len() == 1 {
        // Single-row fast path: no thread overhead for one blocking dial.
        return run_all(configs, prober, kind, verb, probe, url);
    }
    let next = AtomicUsize::new(0);
    let done = std::sync::Mutex::new(Vec::with_capacity(configs.len()));
    let workers = configs.len().clamp(1, MAX_PROBE_CONCURRENCY);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::SeqCst);
                let Some(config) = configs.get(index) else {
                    break;
                };
                let outcome = run_one(config, prober, kind, verb, probe, url);
                done.lock()
                    .expect("probe slot lock held")
                    .push((index, outcome));
            });
        }
    });
    let mut done = done.into_inner().expect("probe slots readable");
    done.sort_by_key(|(index, _)| *index);
    done.into_iter().map(|(_, outcome)| outcome).collect()
}

fn url_probe(
    prober: &dyn HealthProber,
    config: &ProxyConfig,
    bind: &str,
    port: u16,
    url: &str,
) -> Result<ProbeSuccess, String> {
    prober.url_test(config, bind, port, url)
}

fn ping_probe(
    prober: &dyn HealthProber,
    config: &ProxyConfig,
    bind: &str,
    port: u16,
    _url: &str,
) -> Result<ProbeSuccess, String> {
    prober.tcp_ping(config, bind, port)
}

/// Run one URL test per config through each config's own endpoint. Order of
/// the returned outcomes matches `configs`.
/// Ticket 47: probes overlap (bounded [`MAX_PROBE_CONCURRENCY`]-way via
/// [`test_all_concurrent`]) instead of serializing one 5s bound per row.
pub fn test_all(
    configs: &[ProxyConfig],
    url: &str,
    prober: &(dyn HealthProber + Sync),
) -> Vec<TestOutcome> {
    test_all_concurrent(configs, url, prober)
}

/// Ticket 47: the concurrent URL-test runner (also the `test_all` engine).
pub fn test_all_concurrent(
    configs: &[ProxyConfig],
    url: &str,
    prober: &(dyn HealthProber + Sync),
) -> Vec<TestOutcome> {
    run_all_concurrent(
        configs,
        prober,
        HealthKind::UrlTest,
        "url test",
        url_probe,
        url,
    )
}

/// Time one TCP dial per config to its own endpoint.
/// Ticket 47: bounded-concurrent like [`test_all`].
pub fn ping_all(configs: &[ProxyConfig], prober: &(dyn HealthProber + Sync)) -> Vec<TestOutcome> {
    ping_all_concurrent(configs, prober)
}

/// Ticket 47: the concurrent TCP-ping runner (also the `ping_all` engine).
pub fn ping_all_concurrent(
    configs: &[ProxyConfig],
    prober: &(dyn HealthProber + Sync),
) -> Vec<TestOutcome> {
    run_all_concurrent(
        configs,
        prober,
        HealthKind::TcpPing,
        "tcp ping",
        ping_probe,
        "",
    )
}

/// Liveness derived from the latest sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowState {
    Unknown,
    Live,
    Dead,
}

/// Latest-sample view for one row (dead carries the named cause).
/// Ticket 41: dead rows also carry the classified kind (DNS vs local vs
/// egress) so health tells resolution failure apart from reachability
/// failure without changing the displayed cause copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowHealth {
    pub state: RowState,
    pub latency_ms: Option<u64>,
    pub cause: Option<String>,
    pub cause_kind: Option<HealthCause>,
}

/// Accumulated traffic for one config.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrafficStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub probes: u64,
}

/// Short human byte count (`0B`, `512B`, `1.2KB`, `3.0MB`).
pub fn format_bytes(n: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n}B")
    } else {
        format!("{value:.1}{}", UNITS[unit])
    }
}

/// Truncate display text to `limit` chars (dead causes can be long).
pub fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut kept: String = text.chars().take(limit).collect();
    kept.push('…');
    kept
}

/// Ticket 45: sparkline series from oldest-first `(at, latency)` history —
/// the newest `limit` successful samples, oldest-first for the chart.
/// Error samples never plot as zero (they are dropped); empty in means
/// empty out (the detail pane shows a text fallback instead).
pub fn sparkline_latencies(samples: &[(i64, Option<u64>)], limit: usize) -> Vec<u64> {
    let mut latencies: Vec<u64> = samples.iter().filter_map(|(_, latency)| *latency).collect();
    if latencies.len() > limit.max(1) {
        latencies = latencies.split_off(latencies.len() - limit.max(1));
    }
    latencies
}

/// Ticket 36: parse a clash-API `/connections` body into
/// `(download_total, upload_total)`. Live shape (stock sing-box 1.14.2):
/// `{"connections":[],"downloadTotal":1048782,"memory":4497408,
/// "uploadTotal":88}`. Missing/non-integer totals fail named (never zero).
#[cfg_attr(not(test), allow(dead_code))]
pub fn parse_clash_totals(body: &str) -> Result<(u64, u64), String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|_| "clash totals are not JSON".to_string())?;
    let total = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| format!("clash totals lack integer {key}"))
    };
    Ok((total("downloadTotal")?, total("uploadTotal")?))
}

/// Ticket 36: one live clash connection with cumulative bytes and its
/// owning row (`metadata.type` is `<inbound-type>/<inbound-tag>`; our
/// tags are `in-<rowid>`, so the suffix is the row id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClashConn {
    pub id: String,
    pub row_id: Option<String>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    /// Destination host/IP for probe exclusion.
    pub dest: String,
}

/// Ticket 36: parse the `connections` array. Malformed entries are
/// skipped (never a whole-poll failure); a non-JSON body fails named.
pub fn parse_clash_entries(body: &str) -> Result<Vec<ClashConn>, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|_| "clash connections are not JSON".to_string())?;
    let items = value
        .get("connections")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "clash connections lack a connections array".to_string())?;
    let mut out = Vec::new();
    for item in items {
        let id = item
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let rx = item.get("download").and_then(serde_json::Value::as_u64);
        let tx = item.get("upload").and_then(serde_json::Value::as_u64);
        let meta = item.get("metadata");
        let inbound = meta
            .and_then(|m| m.get("type"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let dest = meta
            .and_then(|m| {
                m.get("host")
                    .and_then(serde_json::Value::as_str)
                    .filter(|host| !host.is_empty())
                    .or_else(|| m.get("destinationIP").and_then(serde_json::Value::as_str))
            })
            .unwrap_or("");
        let (Some(rx), Some(tx)) = (rx, tx) else {
            continue;
        };
        if id.is_empty() {
            continue;
        }
        let row_id = inbound
            .rsplit('/')
            .next()
            .and_then(|tag| tag.strip_prefix("in-"))
            .filter(|row| !row.is_empty())
            .map(str::to_string);
        out.push(ClashConn {
            id: id.to_string(),
            row_id,
            rx_bytes: rx,
            tx_bytes: tx,
            dest: dest.to_string(),
        });
    }
    Ok(out)
}

/// Ticket 36: true when a clash entry destination is the health-test
/// host (probe flows must never pollute data-plane counters). Shared by
/// [`attribute_entries`] and the traffic tick so the exclusion can never
/// drift between the two.
/// Ticket 41: both sides normalize (lowercase, no port, no trailing dot —
/// see [`normalize_probe_host`]) and resolved probe IPs match too (see
/// [`is_probe_destination_with_ips`]).
#[cfg_attr(not(test), allow(dead_code))]
pub fn is_probe_destination(dest: &str, probe_host: &str) -> bool {
    is_probe_destination_with_ips(dest, probe_host, &[])
}

/// Ticket 41: normalize one clash-entry destination / probe host for
/// attribution: lowercase, strip scheme + path/query, strip one `:port`
/// suffix (single-colon hosts only — bare IPv6 keeps its colons), strip
/// brackets, strip one trailing dot. `www.` folding stays in the caller
/// (probe hosts are single-sourced from [`DEFAULT_TEST_URL`]).
pub fn normalize_probe_host(raw: &str) -> String {
    let mut host = raw.trim().to_ascii_lowercase();
    if let Some(rest) = host
        .strip_prefix("http://")
        .or_else(|| host.strip_prefix("https://"))
    {
        host = rest.to_string();
    }
    if let Some(end) = host.find(['/', '?', '#']) {
        host.truncate(end);
    }
    // Bracketed IPv6 (`[::1]` or `[::1]:port`).
    if let Some(inner) = host.strip_prefix('[') {
        if let Some(end) = inner.find(']') {
            return inner[..end].trim_end_matches('.').to_string();
        }
        return inner.trim_end_matches('.').to_string();
    }
    // Bare IPv6 (two or more colons) never carries a port suffix here.
    if host.chars().filter(|c| *c == ':').count() >= 2 {
        return host.trim_end_matches('.').to_string();
    }
    // One optional `:port` suffix (numeric tail only — a junk suffix is
    // part of the name, never a port).
    if let Some((head, tail)) = host.rsplit_once(':') {
        if !tail.is_empty() && tail.bytes().all(|byte| byte.is_ascii_digit()) {
            host = head.to_string();
        }
    }
    host.trim_end_matches('.').to_string()
}

/// Ticket 41: probe-destination match over normalized forms, plus resolved
/// probe IPs: a clash entry whose destination is the probe host's resolved
/// IP (the common case — clash reports `destinationIP` while health dials
/// the name) is still a probe flow, in every casing/port/dot variant.
pub fn is_probe_destination_with_ips(
    dest: &str,
    probe_host: &str,
    probe_ips: &[std::net::IpAddr],
) -> bool {
    if probe_host.is_empty() {
        return false;
    }
    let (dest, probe) = (normalize_probe_host(dest), normalize_probe_host(probe_host));
    if dest.is_empty() || probe.is_empty() {
        return false;
    }
    if dest == probe
        || dest
            .strip_prefix("www.")
            .is_some_and(|rest| probe.strip_prefix("www.").unwrap_or(&probe) == rest)
    {
        return true;
    }
    // Resolved-IP leg: compare as addresses (string forms can spell one IP
    // several ways — `::ffff:1.2.3.4` vs `1.2.3.4`).
    if let Ok(dest_ip) = dest.parse::<std::net::IpAddr>() {
        if probe_ips.contains(&dest_ip) {
            return true;
        }
        // IPv4-mapped IPv6 normalization for mixed reporters.
        if let std::net::IpAddr::V6(v6) = dest_ip {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                let mapped = std::net::IpAddr::V4(mapped);
                if probe_ips.contains(&mapped) {
                    return true;
                }
            }
        }
    }
    false
}

/// Ticket 41: best-effort resolution of the probe host to its IPs (numeric
/// fast path, real DNS otherwise). Empty on failure (fail-open: the
/// hostname leg above still applies — resolution must never turn a probe
/// into data-plane bytes, nor data-plane bytes into a probe).
pub fn resolve_probe_ips(host: &str) -> Vec<std::net::IpAddr> {
    let normalized = normalize_probe_host(host);
    if normalized.is_empty() {
        return Vec::new();
    }
    if let Ok(ip) = normalized.parse::<std::net::IpAddr>() {
        return vec![ip];
    }
    use std::net::ToSocketAddrs as _;
    let mut ips: Vec<std::net::IpAddr> = (normalized.as_str(), 80)
        .to_socket_addrs()
        .map(|addrs| {
            let mut out: Vec<std::net::IpAddr> = addrs.map(|addr| addr.ip()).collect();
            out.sort();
            out.dedup();
            out
        })
        .unwrap_or_default();
    ips.truncate(16);
    ips
}

/// Ticket 41: health-cause classifier — DNS failures (the name never
/// resolved) vs local failures (bind/permission/reserved/port) vs egress
/// failures (the far end unreachable/refused/timed out). The validation
/// gate fails fast with [`crate::engine::EngineError::DnsFailure`]; this
/// is the probe-time twin so health rows read DNS-vs-egress honestly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthCause {
    Dns,
    Local,
    Egress,
}

/// Ticket 41: classify one health/probe cause string. Matching is
/// case-insensitive substring over the cause text (causes are already
/// redacted upstream — no secrets reach this match).
pub fn classify_health_cause(cause: &str) -> HealthCause {
    let lowered = cause.to_ascii_lowercase();
    const DNS_MARKERS: &[&str] = &[
        "dns",
        "resolv",
        "lookup",
        "nodename",
        "nxdomain",
        "no such host",
        "name or service not known",
        "getaddrinfo",
        "eai_",
        "temporary failure in name resolution",
    ];
    const LOCAL_MARKERS: &[&str] = &[
        "loopback",
        "lan confirm",
        "privileged",
        "permission",
        "reserved",
        "port in use",
        "already in use",
        "cannot assign",
        "bind",
        "bad local port",
        "bad bind",
    ];
    if DNS_MARKERS.iter().any(|marker| lowered.contains(marker)) {
        return HealthCause::Dns;
    }
    if LOCAL_MARKERS.iter().any(|marker| lowered.contains(marker)) {
        return HealthCause::Local;
    }
    HealthCause::Egress
}

/// Ticket 36: sum entry bytes per row, excluding flows to the health-test
/// host (probes must never pollute data-plane counters) and entries no
/// row claims. Returns `row_id -> (rx, tx)`.
/// Ticket 41: hostname-leg only; use [`attribute_entries_with_ips`] when
/// the probe host's resolved IPs are known.
#[cfg_attr(not(test), allow(dead_code))]
pub fn attribute_entries(
    entries: &[ClashConn],
    probe_host: &str,
) -> std::collections::HashMap<String, (u64, u64)> {
    attribute_entries_with_ips(entries, probe_host, &[])
}

/// Ticket 41: [`attribute_entries`] plus the resolved-IP leg — probe flows
/// addressed by resolved IP (case/port/dot variants included) are excluded
/// from usage counters in all variants.
#[cfg_attr(not(test), allow(dead_code))]
pub fn attribute_entries_with_ips(
    entries: &[ClashConn],
    probe_host: &str,
    probe_ips: &[std::net::IpAddr],
) -> std::collections::HashMap<String, (u64, u64)> {
    let mut totals = std::collections::HashMap::new();
    for entry in entries {
        let Some(row) = entry.row_id.as_deref() else {
            continue;
        };
        if is_probe_destination_with_ips(&entry.dest, probe_host, probe_ips) {
            continue;
        }
        let slot = totals.entry(row.to_string()).or_insert((0u64, 0u64));
        slot.0 = slot.0.saturating_add(entry.rx_bytes);
        slot.1 = slot.1.saturating_add(entry.tx_bytes);
    }
    totals
}

/// Ticket 36: raw `/connections` body off one clash controller.
fn fetch_clash_body(
    port: u16,
    secret: &str,
    timeout: std::time::Duration,
) -> Result<String, String> {
    use std::io::{Read as _, Write as _};
    let endpoint = format!("127.0.0.1:{port}");
    let addr: std::net::SocketAddr = endpoint
        .parse()
        .map_err(|_| format!("clash poll: bad controller port {port}"))?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout)
        .map_err(|error| format!("clash dial {endpoint}: {error}"))?;
    stream.set_read_timeout(Some(timeout)).ok();
    stream.set_write_timeout(Some(timeout)).ok();
    let request = format!(
        "GET /connections HTTP/1.1\r\nHost: {endpoint}\r\nAuthorization: Bearer {secret}\r\nConnection: close\r\nUser-Agent: ManyProxy-traffic\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("clash poll {endpoint}: write failed ({error})"))?;
    // Bodies stay small (single-core sidecars hold few connections); the
    // cap keeps a pathological peer from growing us without bound.
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() >= 2_097_152 {
                    return Err(format!("clash poll {endpoint}: response too large"));
                }
            }
            Err(error) => {
                return Err(format!("clash poll {endpoint}: read failed ({error})"));
            }
        }
    }
    let head = String::from_utf8_lossy(&buf);
    let status = head
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| format!("clash poll {endpoint}: bad response"))?;
    if status == 401 {
        return Err(format!("clash poll {endpoint}: unauthorized (bad secret)"));
    }
    if status != 200 {
        return Err(format!("clash poll {endpoint}: unexpected status {status}"));
    }
    let end = find_headers_end(&buf)
        .ok_or_else(|| format!("clash poll {endpoint}: malformed response"))?;
    Ok(String::from_utf8_lossy(&buf[end..]).into_owned())
}

/// Ticket 36: poll one clash controller's `/connections` totals over
/// loopback. Any failure is a named `Err` (the tick skips that row,
/// keeping its last totals — never zeroing on a missed poll).
#[cfg_attr(not(test), allow(dead_code))]
pub fn poll_clash_totals(
    port: u16,
    secret: &str,
    timeout: std::time::Duration,
) -> Result<(u64, u64), String> {
    parse_clash_totals(&fetch_clash_body(port, secret, timeout)?)
}

/// Ticket 36: poll one clash controller's live connection entries (for
/// per-row attribution by inbound tag).
pub fn poll_clash_entries(
    port: u16,
    secret: &str,
    timeout: std::time::Duration,
) -> Result<Vec<ClashConn>, String> {
    parse_clash_entries(&fetch_clash_body(port, secret, timeout)?)
}

/// Ticket 47: per-core clash poll ceiling (1–2s — one slow controller never
/// stalls the traffic tick past this).
pub const CLASH_PER_CORE_TIMEOUT_SECS: u64 = 2;
/// Ticket 47: cap for the traffic last-seen map (`core/conn-id` entries).
/// Churny short connections never grow it past this.
pub const MAX_TRAFFIC_SEEN_ENTRIES: usize = 20_000;
/// Ticket 47: time-to-live for last-seen entries of dead/removed cores.
/// Entries of live targets with failed polls never expire (dropping them
/// would double-count on recovery); entries whose core is gone entirely
/// expire so crash churn never grows the map.
pub const TRAFFIC_SEEN_TTL_SECS: i64 = 600;

/// Ticket 47: bounded last-seen clash connection bytes (`core/conn-id` →
/// cumulative rx/tx + last-refresh stamp). The traffic tick diffs live
/// readings against this map to persist per-row deltas; the sweep drops
/// closed connections on healthy cores and TTLs dead-core entries while
/// keeping failed-live-core ids (a failed poll must never look like a mass
/// close — that would double-count on recovery).
#[derive(Debug, Clone)]
pub struct TrafficSeenMap {
    inner: HashMap<String, (u64, u64, i64)>,
    cap: usize,
    ttl_secs: i64,
}

impl Default for TrafficSeenMap {
    /// Production bound ([`MAX_TRAFFIC_SEEN_ENTRIES`] + [`TRAFFIC_SEEN_TTL_SECS`]).
    fn default() -> Self {
        Self::bounded()
    }
}

impl TrafficSeenMap {
    /// Production bound ([`MAX_TRAFFIC_SEEN_ENTRIES`] + [`TRAFFIC_SEEN_TTL_SECS`]).
    pub fn bounded() -> Self {
        Self {
            inner: HashMap::new(),
            cap: MAX_TRAFFIC_SEEN_ENTRIES,
            ttl_secs: TRAFFIC_SEEN_TTL_SECS,
        }
    }

    /// Last reading for one connection, if tracked.
    pub fn get(&self, key: &str) -> Option<(u64, u64)> {
        self.inner.get(key).map(|(rx, tx, _)| (*rx, *tx))
    }

    /// Record one live reading. Cap enforcement happens in [`sweep`] (with
    /// failed-live protection), never here, so an insert burst mid-tick
    /// cannot evict the high-water marks the sweep still needs.
    pub fn insert(&mut self, key: String, rx: u64, tx: u64, now: i64) {
        self.inner.insert(key, (rx, tx, now));
    }

    /// Drop everything (clear-traffic paths reset the epoch deliberately).
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Tracked connection count (bounded by [`sweep`]).
    /// Ticket 47: test seam (production bounds via [`sweep`]).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// True when nothing is tracked.
    /// Ticket 47: test seam (production bounds via [`sweep`]).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn core_of(id: &str) -> &str {
        id.split('/').next().unwrap_or("")
    }

    /// One tick-end sweep: drop closed connections on healthy cores, TTL
    /// dead-core entries, keep failed-live-core ids and everything `seen`
    /// this round, then enforce the cap (oldest first, never evicting
    /// failed-live holds except when the whole map is holds).
    pub fn sweep(
        &mut self,
        now: i64,
        ok_cores: &HashSet<String>,
        seen: &HashSet<String>,
        live_cores: &HashSet<String>,
    ) {
        self.inner.retain(|id, (_, _, at)| {
            if seen.contains(id) {
                return true;
            }
            let core = Self::core_of(id);
            if live_cores.contains(core) {
                // Live target: a failed poll keeps ids (never a mass
                // close); a healthy poll drops unseen ids (closed).
                return !ok_cores.contains(core);
            }
            // Dead/removed core: TTL'd so crash churn stays bounded. A
            // restarted core resets its counters, so treating its next
            // reading as a fresh epoch is correct, never a double count.
            now.saturating_sub(*at) <= self.ttl_secs
        });
        let protected: HashSet<String> = self
            .inner
            .keys()
            .filter(|id| {
                let core = Self::core_of(id);
                live_cores.contains(core) && !ok_cores.contains(core)
            })
            .cloned()
            .collect();
        while self.inner.len() > self.cap {
            let victim = self
                .inner
                .iter()
                .filter(|(id, _)| !protected.contains(id.as_str()))
                .min_by_key(|(_, (_, _, at))| *at)
                .map(|(id, _)| id.clone())
                .or_else(|| {
                    self.inner
                        .iter()
                        .min_by_key(|(_, (_, _, at))| *at)
                        .map(|(id, _)| id.clone())
                });
            let Some(victim) = victim else {
                break;
            };
            self.inner.remove(&victim);
        }
    }
}
/// Ticket 47: per-core timeout as a [`Duration`] (single source; the tick
/// passes this, never a second literal).
pub const CLASH_PER_CORE_TIMEOUT: Duration = Duration::from_secs(CLASH_PER_CORE_TIMEOUT_SECS);
/// Ticket 47: total deadline for one traffic-tick poll round across all
/// cores (bounds the tick even when every controller is slow).
pub const CLASH_TOTAL_DEADLINE_SECS: u64 = 8;
/// Ticket 47: total deadline as a [`Duration`].
pub const CLASH_TOTAL_DEADLINE: Duration = Duration::from_secs(CLASH_TOTAL_DEADLINE_SECS);

/// Ticket 47: poll every clash target concurrently (one thread per live
/// core — bounded by the live-core count, which is bounded by the running
/// sidecar count), each poll capped by `per_core`. Results landing after
/// `total` are reported as deadline errors instead of stalling the tick;
/// late threads are detached and exit on their own within `per_core`
/// (bounded, never leaked past one timeout). Secrets travel only into the
/// polling threads (never into error strings or logs — error text names
/// core/port only). Result order matches `targets` order.
pub fn poll_clash_targets(
    targets: &[crate::engine::ClashTarget],
    per_core: Duration,
    total: Duration,
) -> Vec<(crate::engine::ClashTarget, Result<Vec<ClashConn>, String>)> {
    if targets.is_empty() {
        return Vec::new();
    }
    let (done_tx, done_rx) = std::sync::mpsc::channel::<(usize, Result<Vec<ClashConn>, String>)>();
    for (index, target) in targets.iter().enumerate() {
        let done_tx = done_tx.clone();
        let port = target.port;
        let secret = target.secret.clone();
        std::thread::spawn(move || {
            let result = poll_clash_entries(port, &secret, per_core);
            let _ = done_tx.send((index, result));
        });
    }
    drop(done_tx);
    let mut slots: Vec<Option<Result<Vec<ClashConn>, String>>> =
        (0..targets.len()).map(|_| None).collect();
    let deadline = Instant::now() + total;
    for _ in 0..targets.len() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match done_rx.recv_timeout(remaining) {
            Ok((index, result)) => slots[index] = Some(result),
            Err(_) => break,
        }
    }
    targets
        .iter()
        .zip(slots)
        .map(|(target, slot)| {
            let result = slot.unwrap_or_else(|| {
                Err(format!(
                    "clash poll {} (127.0.0.1:{}): total deadline exceeded",
                    target.core, target.port
                ))
            });
            (target.clone(), result)
        })
        .collect()
}

/// Append-only samples + per-config traffic accumulation.
#[derive(Debug, Default)]
pub struct HealthStore {
    samples: VecDeque<HealthSample>,
    /// Ticket 47: O(1) latest-sample index — the last-pushed sample of any
    /// kind per config, plus the last-pushed sample per (config, kind).
    /// Maintained on every push, ring-cap eviction, and stale prune, so
    /// [`HealthStore::latest`]/[`latest_of`] never scan the ring. Clones are
    /// bounded (one per config per slot), never per sample.
    latest_any: HashMap<String, HealthSample>,
    latest_kind: HashMap<String, [Option<HealthSample>; 2]>,
    traffic: HashMap<String, TrafficStats>,
    /// Ticket 36: data-plane totals per config, mirrored from the store
    /// (persisted) after every clash poll. Present means the row is
    /// clash-tracked; absent rows keep the probe-byte display.
    data_traffic: HashMap<String, (u64, u64)>,
    /// Ticket 41: per-row stale-totals markers — set when a crash drops the
    /// row's core (dead sockets are skipped, last totals kept but flagged),
    /// cleared on the next successful poll of the row.
    stale: HashSet<String>,
}

impl HealthStore {
    /// Ticket 28: one bounded push for [`record_outcome`] and [`hydrate`]
    /// (the ring cap is not per-caller policy).
    /// Ticket 47: the ring is a [`VecDeque`] (O(1) front eviction) and every
    /// push refreshes the O(1) latest-sample index.
    fn push_sample(&mut self, sample: HealthSample) {
        if self.samples.len() >= MAX_SAMPLES {
            if let Some(evicted) = self.samples.pop_front() {
                self.reindex_evicted(&evicted);
            }
        }
        let slot = Self::kind_slot(sample.kind);
        let id = sample.config_id.clone();
        self.samples.push_back(sample.clone());
        self.latest_any.insert(id.clone(), sample.clone());
        self.latest_kind.entry(id).or_insert([None, None])[slot] = Some(sample);
    }

    /// Ticket 47: index slot for one probe kind.
    fn kind_slot(kind: HealthKind) -> usize {
        match kind {
            HealthKind::UrlTest => 0,
            HealthKind::TcpPing => 1,
        }
    }

    /// Latest sample of one kind for one config.
    pub fn latest_of(&self, config_id: &str, kind: HealthKind) -> Option<&HealthSample> {
        self.latest_filtered(config_id, Some(kind))
    }

    fn latest_filtered(&self, config_id: &str, kind: Option<HealthKind>) -> Option<&HealthSample> {
        match kind {
            None => self.latest_any.get(config_id),
            Some(kind) => self
                .latest_kind
                .get(config_id)
                .and_then(|slots| slots[Self::kind_slot(kind)].as_ref()),
        }
    }

    /// Ticket 47: repair the index after a ring-cap eviction. Only the
    /// evicted config is ever rescanned (and only when the evicted sample
    /// was its indexed latest); every other config stays O(1).
    fn reindex_evicted(&mut self, evicted: &HealthSample) {
        let id = &evicted.config_id;
        if self.latest_any.get(id) == Some(evicted) {
            match self
                .samples
                .iter()
                .rev()
                .find(|sample| &sample.config_id == id)
                .cloned()
            {
                Some(newest) => {
                    self.latest_any.insert(id.clone(), newest);
                }
                None => {
                    self.latest_any.remove(id);
                }
            }
        }
        let slot = Self::kind_slot(evicted.kind);
        let indexed = self
            .latest_kind
            .get(id)
            .and_then(|slots| slots[slot].as_ref())
            == Some(evicted);
        if indexed {
            let newest = self
                .samples
                .iter()
                .rev()
                .find(|sample| &sample.config_id == id && sample.kind == evicted.kind)
                .cloned();
            match newest {
                Some(newest) => {
                    if let Some(slots) = self.latest_kind.get_mut(id) {
                        slots[slot] = Some(newest);
                    }
                }
                None => {
                    let mut drop_row = false;
                    if let Some(slots) = self.latest_kind.get_mut(id) {
                        slots[slot] = None;
                        drop_row = slots.iter().all(Option::is_none);
                    }
                    if drop_row {
                        self.latest_kind.remove(id);
                    }
                }
            }
        }
    }

    /// Ticket 47: rebuild the whole index front-to-back (later pushes win —
    /// the same answer a reverse scan gives). Only the prune path pays O(n).
    fn rebuild_latest_index(&mut self) {
        self.latest_any.clear();
        self.latest_kind.clear();
        for sample in &self.samples {
            self.latest_any
                .insert(sample.config_id.clone(), sample.clone());
            self.latest_kind
                .entry(sample.config_id.clone())
                .or_insert([None, None])[Self::kind_slot(sample.kind)] = Some(sample.clone());
        }
    }

    /// Append one outcome: sample always, traffic only on success.
    pub fn record_outcome(&mut self, outcome: &TestOutcome) {
        self.push_sample(outcome.sample.clone());
        if let Some(success) = &outcome.success {
            let entry = self
                .traffic
                .entry(outcome.sample.config_id.clone())
                .or_default();
            entry.rx_bytes = entry.rx_bytes.saturating_add(success.bytes_rx);
            entry.tx_bytes = entry.tx_bytes.saturating_add(success.bytes_tx);
            entry.probes += 1;
        }
    }

    /// Ticket 12: restore persisted latest-per-config samples after restart.
    /// Samples only — traffic counters stay session-only (a fresh session
    /// has moved no bytes yet). Unknown kinds default to URL tests; callers
    /// pass only snapshots of known rows so removed configs never resurrect.
    pub fn hydrate(&mut self, snapshots: &[HealthSnapshot]) {
        for snapshot in snapshots {
            let kind = if snapshot.kind == HealthKind::TcpPing.as_str() {
                HealthKind::TcpPing
            } else {
                HealthKind::UrlTest
            };
            self.push_sample(HealthSample {
                config_id: snapshot.config_id.clone(),
                at: snapshot.at,
                kind,
                latency_ms: snapshot.latency_ms,
                error: snapshot.error.clone(),
            });
        }
    }

    /// Latest sample of any kind for one config.
    /// Ticket 28: shares the reverse scan with [`latest_of`].
    /// Ticket 32: no longer behind [`HealthStore::status`] (worst-kind-wins
    /// reads per-kind); kept as the public latest-sample accessor.
    /// Ticket 47: served O(1) from the latest-sample index (see
    /// [`HealthStore::push_sample`]).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn latest(&self, config_id: &str) -> Option<&HealthSample> {
        self.latest_filtered(config_id, None)
    }

    /// Liveness across kinds (ticket 32: worst-kind-wins): the latest sample
    /// of EACH kind is considered — any dead kind means dead, so a later
    /// ping success never masks a live URL failure (and vice versa). The
    /// URL leg names the cause when it is dead (end-to-end signal beats
    /// dial timing); latency prefers the URL leg. Nothing recorded of any
    /// kind is unknown.
    pub fn status(&self, config_id: &str) -> RowHealth {
        let url = self.latest_of(config_id, HealthKind::UrlTest);
        let ping = self.latest_of(config_id, HealthKind::TcpPing);
        if url.is_none() && ping.is_none() {
            return RowHealth {
                state: RowState::Unknown,
                latency_ms: None,
                cause: None,
                cause_kind: None,
            };
        }
        let dead_cause = url
            .and_then(|sample| sample.error.as_deref())
            .or_else(|| ping.and_then(|sample| sample.error.as_deref()));
        match dead_cause {
            Some(cause) => RowHealth {
                state: RowState::Dead,
                latency_ms: None,
                cause: Some(cause.to_string()),
                cause_kind: Some(classify_health_cause(cause)),
            },
            None => RowHealth {
                state: RowState::Live,
                latency_ms: url
                    .and_then(|sample| sample.latency_ms)
                    .or_else(|| ping.and_then(|sample| sample.latency_ms)),
                cause: None,
                cause_kind: None,
            },
        }
    }

    /// Ticket 32: drop samples older than `max_age_secs` relative to `now`
    /// (stale samples expire instead of haunting a row forever).
    /// Future-dated samples (clock skew) are kept, never pruned.
    /// Ticket 47: the latest-sample index rebuilds from the survivors.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn prune_stale(&mut self, now: i64, max_age_secs: i64) {
        self.samples
            .retain(|sample| now.saturating_sub(sample.at) <= max_age_secs);
        self.rebuild_latest_index();
    }

    /// Accumulated traffic (zeroed when nothing succeeded yet).
    pub fn traffic(&self, config_id: &str) -> TrafficStats {
        self.traffic.get(config_id).copied().unwrap_or_default()
    }

    /// Ticket 36: mirror persisted data-plane totals for display.
    pub fn set_data_traffic(&mut self, config_id: &str, rx_bytes: u64, tx_bytes: u64) {
        self.data_traffic
            .insert(config_id.to_string(), (rx_bytes, tx_bytes));
    }

    /// Ticket 36: pass-through for the poll tick (untracked rows stay None).
    pub fn data_traffic(&self, config_id: &str) -> Option<(u64, u64)> {
        self.data_traffic.get(config_id).copied()
    }

    /// Ticket 41: mark one row's totals stale (its core crashed and ticks
    /// skip it — last totals are kept but flagged, never trusted fresh).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn mark_stale(&mut self, config_id: &str) {
        self.stale.insert(config_id.to_string());
    }

    /// Ticket 41: mark several rows stale at once (one crash drops a whole
    /// core's rows).
    pub fn mark_stale_all(&mut self, config_ids: &[String]) {
        self.stale.extend(config_ids.iter().cloned());
    }

    /// Ticket 41: clear one row's stale marker (its next successful poll
    /// landed after recovery).
    pub fn clear_stale(&mut self, config_id: &str) {
        self.stale.remove(config_id);
    }

    /// Ticket 41: true when the row's totals are stale (crashed, skipped by
    /// ticks until recovery).
    pub fn is_stale(&self, config_id: &str) -> bool {
        self.stale.contains(config_id)
    }

    /// Ticket 33: probe-byte counts are labeled `probe:` — they measure
    /// health-probe request/response bytes through the row's own endpoint,
    /// never data-plane traffic (there is no data-plane meter in v1).
    /// Ticket 36: clash-tracked rows report data-plane totals instead; the
    /// probe count rides along as context. Untracked rows keep the v1
    /// probe display.
    fn traffic_text(&self, config_id: &str) -> String {
        // Ticket 41: stale totals read as stale (crashed core, ticks
        // skipping) — never as fresh.
        let stale = if self.is_stale(config_id) {
            " (stale)"
        } else {
            ""
        };
        let stats = self.traffic(config_id);
        if let Some((rx, tx)) = self.data_traffic(config_id) {
            if stats.probes == 0 {
                return format!("↑{} ↓{}{stale}", format_bytes(tx), format_bytes(rx));
            }
            return format!(
                "↑{} ↓{} · {} probe(s){stale}",
                format_bytes(tx),
                format_bytes(rx),
                stats.probes
            );
        }
        if stats.probes == 0 {
            "probe: no traffic yet".to_string()
        } else {
            format!(
                "probe: ↑{} ↓{} · {} probe(s)",
                format_bytes(stats.tx_bytes),
                format_bytes(stats.rx_bytes),
                stats.probes
            )
        }
    }

    /// One-line row summary for the library list: `87ms`, `dead`, or empty
    /// when untested (the row then shows no health child).
    pub fn row_summary(&self, config_id: &str) -> String {
        match self.status(config_id) {
            RowHealth {
                state: RowState::Unknown,
                ..
            } => String::new(),
            RowHealth {
                state: RowState::Live,
                latency_ms: Some(ms),
                ..
            } => format!("{ms}ms"),
            RowHealth {
                state: RowState::Live,
                latency_ms: None,
                ..
            } => "live".to_string(),
            RowHealth {
                state: RowState::Dead,
                ..
            } => "dead".to_string(),
        }
    }

    /// Detail-bar line: url latency + tcp-ping latency + traffic, or the
    /// named dead cause.
    pub fn detail_line(&self, config_id: &str) -> String {
        let url_ms = self
            .latest_of(config_id, HealthKind::UrlTest)
            .and_then(|sample| sample.latency_ms);
        let ping_ms = self
            .latest_of(config_id, HealthKind::TcpPing)
            .and_then(|sample| sample.latency_ms);
        let traffic = self.traffic_text(config_id);
        match self.status(config_id) {
            RowHealth {
                state: RowState::Unknown,
                ..
            } => "health: not tested".to_string(),
            RowHealth {
                state: RowState::Live,
                latency_ms: Some(ms),
                ..
            } => {
                let _ = ms;
                let mut parts = vec![format!(
                    "url {}",
                    url_ms.map(|ms| format!("{ms}ms")).unwrap_or_default()
                )];
                if let Some(ping) = ping_ms {
                    parts.push(format!("ping {ping}ms"));
                }
                format!("health: {} · {traffic}", parts.join(" · "))
            }
            RowHealth {
                state: RowState::Live,
                latency_ms: None,
                ..
            } => format!("health: live · {traffic}"),
            RowHealth {
                state: RowState::Dead,
                cause: Some(cause),
                ..
            } => format!("health: {} · {traffic}", truncate(&cause, 140)),
            RowHealth {
                state: RowState::Dead,
                cause: None,
                ..
            } => format!("health: dead · {traffic}"),
        }
    }

    /// `(live, dead)` over the given ids (untested ids count as neither).
    pub fn counts(&self, ids: &[String]) -> (usize, usize) {
        let mut live = 0;
        let mut dead = 0;
        for id in ids {
            match self.status(id) {
                RowHealth {
                    state: RowState::Live,
                    ..
                } => live += 1,
                RowHealth {
                    state: RowState::Dead,
                    ..
                } => dead += 1,
                _ => {}
            }
        }
        (live, dead)
    }
}

/// Sidecar log severity.
/// Ticket 45: production type behind the logs level filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }

    /// Ticket 45: parse a level-filter label (`None` = the `all` position,
    /// unknown labels also read as `all` so a corrupt pick never hides
    /// everything silently — the UI only offers known labels anyway).
    pub fn from_label(label: &str) -> Option<LogLevel> {
        match label.trim().to_ascii_lowercase().as_str() {
            "debug" => Some(LogLevel::Debug),
            "info" => Some(LogLevel::Info),
            "warn" => Some(LogLevel::Warn),
            "error" => Some(LogLevel::Error),
            _ => None,
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn parse_level(line: &str) -> LogLevel {
    let lower = line.to_ascii_lowercase();
    if lower.contains("error") || lower.contains("fatal") || lower.contains("panic") {
        LogLevel::Error
    } else if lower.contains("warn") {
        LogLevel::Warn
    } else if lower.contains("debug") || lower.contains("trace") {
        LogLevel::Debug
    } else {
        LogLevel::Info
    }
}

/// One redacted sidecar line (data-model `LogRecord`). `config_id` is `None`
/// for engine-global lines no tag claimed.
/// Ticket 47: `message` is ANSI-stripped at ingest (render paths never strip
/// again) and `search` is the cached lowercase `"[level] message"` blob the
/// Logs dialog filters on (zero per-render allocs, zero per-keystroke case
/// folding).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRecord {
    pub at: i64,
    pub config_id: Option<String>,
    pub level: LogLevel,
    pub message: String,
    pub search: String,
}

/// Ticket 47: precomputed log-tag index — the `in-{id}`/`out-{id}` tag
/// strings built ONCE per ingest batch (not once per config per line), in
/// config order so first-config-wins matches the per-line scan exactly.
/// (A real multi-pattern automaton like aho-corasick would only pay off
/// past hundreds of patterns; at ≤2×rows needles the precomputed strings
/// remove the per-line allocs that dominated. No new dep.)
#[derive(Debug, Clone, Default)]
pub struct LogTagIndex {
    tags: Vec<(String, String)>,
}

impl LogTagIndex {
    /// Build the index for one ingest batch (one build per batch, not per line).
    pub fn build(active: &[ProxyConfig]) -> Self {
        let mut tags = Vec::with_capacity(active.len() * 2);
        for config in active {
            tags.push((inbound_tag(&config.id), config.id.clone()));
            tags.push((outbound_tag(&config.id), config.id.clone()));
        }
        Self { tags }
    }

    /// Claim a line for the first config whose tag it contains.
    pub fn tag_for(&self, line: &str) -> Option<String> {
        for (tag, id) in &self.tags {
            if line.contains(tag) {
                return Some(id.clone());
            }
        }
        None
    }
}

/// Redacted, tagged sidecar lines with a per-config filter.
/// Ticket 47: the ring is a [`VecDeque`] (O(1) front eviction) with
/// crash-cause lines pinned past normal-line eviction.
#[derive(Debug, Default)]
pub struct LogStore {
    records: VecDeque<LogRecord>,
}

impl LogStore {
    /// Ingest one raw sidecar line: strip ANSI, tag by config via the
    /// precomputed index, redact secrets, classify severity, cache the
    /// search blob.
    pub fn push_raw(&mut self, line: &str, tags: &LogTagIndex) {
        let stripped = crate::engine::strip_ansi_codes(line);
        let message = redact_secrets(&stripped);
        let level = parse_level(&stripped);
        let search = format!("[{}] {}", level.as_str(), message).to_lowercase();
        self.push(LogRecord {
            at: now_unix(),
            config_id: tags.tag_for(&stripped),
            level,
            search,
            message,
        });
    }

    fn push(&mut self, record: LogRecord) {
        if self.records.len() >= MAX_LOG_RECORDS {
            // Pin crash-cause lines: evict the oldest NORMAL line so a
            // sustained spam burst never flushes the evidence; when every
            // line is pinned, evict the oldest anyway (the ring stays
            // bounded no matter what).
            if let Some(position) = self
                .records
                .iter()
                .position(|record| !is_crash_cause_line(&record.message))
            {
                self.records.remove(position);
            } else {
                self.records.pop_front();
            }
        }
        self.records.push_back(record);
    }

    /// Only lines tagged to this config (per-config filter).
    pub fn for_config(&self, config_id: &str) -> Vec<&LogRecord> {
        self.records
            .iter()
            .filter(|record| record.config_id.as_deref() == Some(config_id))
            .collect()
    }

    /// All stored lines, oldest first.
    pub fn all(&self) -> &VecDeque<LogRecord> {
        &self.records
    }
}

/// Ticket 47: crash-cause lines survive ring eviction (case-insensitive
/// `panic`/`fatal`/`crash` — sing-box fatal errors, Go panics, and our own
/// crash vocabulary).
fn is_crash_cause_line(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    ["panic", "fatal", "crash"]
        .iter()
        .any(|marker| lower.contains(marker))
}

/// Claim a sidecar line for a config when it names that config's endpoint.
/// Ticket 47: one-shot build + tag (same first-config-wins answer as
/// [`LogTagIndex::tag_for`]); batch ingest builds the index once via
/// [`LogTagIndex::build`] instead. Kept as the parity oracle for the index.
#[cfg_attr(not(test), allow(dead_code))]
pub fn tag_for_line(line: &str, active: &[ProxyConfig]) -> Option<String> {
    LogTagIndex::build(active).tag_for(line)
}

const SECRET_KEYS: &[&str] = &[
    "password",
    "passwd",
    "pass",
    "pwd",
    "token",
    "secret",
    "uuid",
    "privatekey",
    "private_key",
    "apikey",
    "api_key",
    "auth",
    "authorization",
    "bearer",
    "basic",
    "access_token",
    "refresh_token",
    "cookie",
    "set-cookie",
    "set_cookie",
    "client_secret",
    "clientsecret",
];

fn is_secret_key(token: &str) -> bool {
    let clean = token
        .trim_matches(|c| c == '"' || c == '\'')
        .to_ascii_lowercase();
    if SECRET_KEYS.contains(&clean.as_str()) {
        return true;
    }
    ["_key", "_token", "_secret", "_password", "_passwd"]
        .iter()
        .any(|suffix| clean.ends_with(suffix))
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn is_value_end(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t'
            | '\r'
            | '\n'
            | ','
            | ';'
            | '"'
            | '\''
            | '}'
            | '{'
            | ']'
            | '['
            | ')'
            | '('
            | '&'
            | '<'
            | '>'
    )
}

/// Redact credential values in `key=value`, `key: value`, `"key": "value"`,
/// `Bearer <token>`, `scheme://userinfo@host`, and `?key=value` forms.
/// Non-secret text (hosts, tags, config names) passes through untouched.
pub fn redact_secrets(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < chars.len() {
        // URL userinfo: scheme://credentials@ → scheme://***@.
        if i + 2 < chars.len() && chars[i] == ':' && chars[i + 1] == '/' && chars[i + 2] == '/' {
            out.push_str("://");
            let j = i + 3;
            let mut end = j;
            while end < chars.len()
                && !matches!(
                    chars[end],
                    ' ' | '\t' | '\r' | '\n' | '"' | '\'' | ',' | ';'
                )
            {
                end += 1;
            }
            let mut at = None;
            let mut slash = false;
            for (k, ch) in chars.iter().enumerate().take(end).skip(j) {
                if *ch == '/' {
                    slash = true;
                    break;
                }
                if *ch == '@' {
                    at = Some(k);
                }
            }
            if let (Some(at), false) = (at, slash) {
                out.push_str("***@");
                let mut j = at + 1;
                while j < end {
                    out.push(chars[j]);
                    j += 1;
                }
                i = end;
                continue;
            }
            // No userinfo: emit just the scheme separator and fall through
            // so the host/query still pass key=value redaction.
            i += 3;
            continue;
        }
        if is_token_char(chars[i])
            && (i == 0 || (!is_token_char(chars[i - 1]) && chars[i - 1] != '.'))
        {
            let mut j = i;
            while j < chars.len() && is_token_char(chars[j]) {
                j += 1;
            }
            let token: String = chars[i..j].iter().collect();
            if is_secret_key(&token) {
                let mut k = j;
                while k < chars.len() && (chars[k] == ' ' || chars[k] == '"' || chars[k] == '\'') {
                    k += 1;
                }
                let bearer = matches!(
                    token
                        .trim_matches(|c| c == '"' || c == '\'')
                        .to_ascii_lowercase()
                        .as_str(),
                    "bearer" | "basic"
                );
                if k < chars.len() && (chars[k] == '=' || chars[k] == ':') {
                    // Replay key + separator verbatim, then mask the value.
                    while j <= k {
                        out.push(chars[j]);
                        j += 1;
                    }
                    while j < chars.len()
                        && (chars[j] == ' ' || chars[j] == '"' || chars[j] == '\'')
                    {
                        out.push(chars[j]);
                        j += 1;
                    }
                    out.push_str("***");
                    while j < chars.len() && !is_value_end(chars[j]) {
                        j += 1;
                    }
                    // Replay a closing quote when we consumed an opening one.
                    if j < chars.len() && (chars[j] == '"' || chars[j] == '\'') {
                        out.push(chars[j]);
                        j += 1;
                    }
                    i = j;
                    continue;
                }
                if bearer && k < chars.len() && !is_value_end(chars[k]) {
                    while j < k {
                        out.push(chars[j]);
                        j += 1;
                    }
                    out.push_str("***");
                    while j < chars.len() && !is_value_end(chars[j]) {
                        j += 1;
                    }
                    i = j;
                    continue;
                }
            }
            out.push_str(&token);
            i = j;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
#[derive(Debug, Default)]
struct FakeProbeState {
    calls: Vec<ProbeCall>,
    latency: HashMap<String, u64>,
    failures: HashMap<String, String>,
    denied: HashSet<String>,
}

/// One recorded probe call (asserts per-egress routing: each call carries
/// the tested config's own endpoint).
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeCall {
    pub config_id: String,
    pub kind: HealthKind,
    pub bind: String,
    pub port: u16,
    pub url: String,
}

/// Read/write handle to a [`FakeHealthProber`]'s script (latencies,
/// failures, and the ticket-08 deny-list hook).
#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub struct FakeHealthHandle {
    state: Arc<Mutex<FakeProbeState>>,
}

#[cfg(test)]
impl FakeHealthHandle {
    pub fn script_latency(&self, config_id: &str, latency_ms: u64) {
        self.state
            .lock()
            .unwrap()
            .latency
            .insert(config_id.to_string(), latency_ms);
    }

    pub fn script_failure(&self, config_id: &str, cause: &str) {
        self.state
            .lock()
            .unwrap()
            .failures
            .insert(config_id.to_string(), cause.to_string());
    }

    /// Deny a protocol (test hook: simulates an unprobeable row without
    /// touching the runner; the runner records a named skip).
    pub fn deny_protocol(&self, protocol: &str) {
        self.state
            .lock()
            .unwrap()
            .denied
            .insert(protocol.trim().to_ascii_lowercase());
    }

    pub fn calls_for(&self, config_id: &str) -> Vec<ProbeCall> {
        self.state
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|call| call.config_id == config_id)
            .cloned()
            .collect()
    }
}

/// In-memory mock driver: records calls, never dials, never resolves.
/// All health tests run against this.
/// Ticket 28: kept mirrored with the engine fake on purpose — this one is
/// `Arc<Mutex>` (shared across probe threads), the engine fake is
/// `Rc<RefCell>` (single-threaded UI harness).
#[cfg(test)]
pub struct FakeHealthProber {
    state: Arc<Mutex<FakeProbeState>>,
    default_latency: u64,
}

#[cfg(test)]
impl FakeHealthProber {
    pub fn shared() -> (Self, FakeHealthHandle) {
        let state = Arc::new(Mutex::new(FakeProbeState::default()));
        (
            Self {
                state: state.clone(),
                default_latency: 42,
            },
            FakeHealthHandle { state },
        )
    }

    fn scripted(&self, config: &ProxyConfig) -> Result<ProbeSuccess, String> {
        let state = self.state.lock().unwrap();
        if let Some(cause) = state.failures.get(&config.id) {
            return Err(cause.clone());
        }
        let latency = state
            .latency
            .get(&config.id)
            .copied()
            .unwrap_or(self.default_latency);
        Ok(ProbeSuccess {
            latency_ms: latency,
            bytes_rx: 1200,
            bytes_tx: 300,
        })
    }
}

#[cfg(test)]
impl HealthProber for FakeHealthProber {
    fn supports(&self, config: &ProxyConfig) -> bool {
        !self
            .state
            .lock()
            .unwrap()
            .denied
            .contains(&config.protocol.trim().to_ascii_lowercase())
    }

    fn url_test(
        &self,
        config: &ProxyConfig,
        bind: &str,
        port: u16,
        url: &str,
    ) -> Result<ProbeSuccess, String> {
        self.state.lock().unwrap().calls.push(ProbeCall {
            config_id: config.id.clone(),
            kind: HealthKind::UrlTest,
            bind: bind.to_string(),
            port,
            url: url.to_string(),
        });
        self.scripted(config)
    }

    fn tcp_ping(
        &self,
        config: &ProxyConfig,
        bind: &str,
        port: u16,
    ) -> Result<ProbeSuccess, String> {
        self.state.lock().unwrap().calls.push(ProbeCall {
            config_id: config.id.clone(),
            kind: HealthKind::TcpPing,
            bind: bind.to_string(),
            port,
            url: String::new(),
        });
        self.scripted(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(id: &str, name: &str, protocol: &str, bind: &str, port: u16) -> ProxyConfig {
        ProxyConfig {
            id: id.to_string(),
            name: name.to_string(),
            protocol: protocol.to_string(),
            params: "{}".to_string(),
            group: "default".to_string(),
            enabled: true,
            bind_ip: bind.to_string(),
            port,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        }
    }

    /// Prohibited side effect: each URL test dials its own config's endpoint,
    /// never another row's egress.
    #[test]
    fn url_tests_use_each_configs_own_endpoint() {
        let configs = vec![
            fixture("1", "Alpha", "vless", "127.0.0.1", 10801),
            fixture("2", "Beta", "vless", "127.0.0.1", 10802),
        ];
        let (prober, handle) = FakeHealthProber::shared();
        let outcomes = test_all(&configs, DEFAULT_TEST_URL, &prober);
        assert_eq!(outcomes.len(), 2);
        for (config, outcome) in configs.iter().zip(outcomes.iter()) {
            let calls = handle.calls_for(&config.id);
            assert_eq!(calls.len(), 1, "one probe for {}", config.name);
            assert_eq!(calls[0].bind, config.bind_ip);
            assert_eq!(calls[0].port, config.port, "own endpoint, not a sibling's");
            assert_eq!(calls[0].url, DEFAULT_TEST_URL);
            assert!(outcome.sample.error.is_none());
        }
    }

    /// Scenario 1: 5 running rows each show a latency or a named failure.
    #[test]
    fn five_actives_each_show_latency_or_named_failure() {
        let configs: Vec<ProxyConfig> = (0..5)
            .map(|i| {
                fixture(
                    &format!("c{i}"),
                    &format!("Row-{i}"),
                    "vless",
                    "127.0.0.1",
                    10801 + i as u16,
                )
            })
            .collect();
        let (prober, handle) = FakeHealthProber::shared();
        for (i, config) in configs.iter().enumerate() {
            handle.script_latency(&config.id, 20 + i as u64 * 10);
        }
        handle.script_failure("c3", "upstream refused");
        let outcomes = test_all(&configs, DEFAULT_TEST_URL, &prober);
        assert_eq!(outcomes.len(), 5);
        let mut store = HealthStore::default();
        for outcome in &outcomes {
            store.record_outcome(outcome);
        }
        for config in &configs {
            let sample = store.latest(&config.id).expect("sample per row");
            if config.id == "c3" {
                assert!(sample.latency_ms.is_none());
                let cause = sample.error.as_deref().expect("named failure");
                assert!(cause.contains("Row-3"), "names config: {cause}");
                assert!(cause.contains("upstream refused"), "names cause: {cause}");
                assert!(matches!(store.status(&config.id).state, RowState::Dead));
            } else {
                assert!(sample.latency_ms.is_some());
                assert!(sample.error.is_none());
                assert!(matches!(store.status(&config.id).state, RowState::Live));
            }
        }
        assert_eq!(store.row_summary("c0"), "20ms");
        assert_eq!(store.row_summary("c3"), "dead");
        assert_eq!(store.row_summary("missing"), "");
    }

    /// Scenario 3: a killed upstream flags only its own row; the others keep
    /// their values.
    #[test]
    fn retest_flags_only_killed_row() {
        let configs = vec![
            fixture("1", "Alpha", "vless", "127.0.0.1", 10801),
            fixture("2", "Beta", "vless", "127.0.0.1", 10802),
        ];
        let (prober, handle) = FakeHealthProber::shared();
        handle.script_latency("1", 30);
        handle.script_latency("2", 40);
        let mut store = HealthStore::default();
        for outcome in test_all(&configs, DEFAULT_TEST_URL, &prober) {
            store.record_outcome(&outcome);
        }
        handle.script_failure("2", "connection reset");
        for outcome in test_all(&configs, DEFAULT_TEST_URL, &prober) {
            store.record_outcome(&outcome);
        }
        let alpha = store.status("1");
        assert!(matches!(alpha.state, RowState::Live));
        assert_eq!(alpha.latency_ms, Some(30), "kept value, not clobbered");
        let beta = store.status("2");
        assert!(matches!(beta.state, RowState::Dead));
        assert!(
            beta.cause
                .as_deref()
                .unwrap_or("")
                .contains("connection reset"),
            "named cause: {:?}",
            beta.cause
        );
    }

    // -- Ticket 32: truthful status (worst-kind-wins, stale expiry) --------

    /// Ticket 32: a URL failure after a ping success is dead — and a later
    /// ping success never masks a live URL failure (worst-kind-wins).
    #[test]
    fn t32_url_failure_after_ping_success_is_dead() {
        fn outcome(
            id: &str,
            kind: HealthKind,
            latency_ms: Option<u64>,
            error: Option<&str>,
        ) -> TestOutcome {
            TestOutcome {
                sample: HealthSample {
                    config_id: id.to_string(),
                    at: now_unix(),
                    kind,
                    latency_ms,
                    error: error.map(str::to_string),
                },
                success: latency_ms.map(|latency| ProbeSuccess {
                    latency_ms: latency,
                    bytes_rx: 1200,
                    bytes_tx: 300,
                }),
            }
        }
        // Ping success, then URL failure: dead.
        let mut store = HealthStore::default();
        store.record_outcome(&outcome("1", HealthKind::TcpPing, Some(7), None));
        store.record_outcome(&outcome(
            "1",
            HealthKind::UrlTest,
            None,
            Some("url test failed for \"A\": refused"),
        ));
        let status = store.status("1");
        assert!(
            matches!(status.state, RowState::Dead),
            "url failure is dead: {status:?}"
        );
        // URL failure, then ping success: still dead (no ping masking).
        let mut masked = HealthStore::default();
        masked.record_outcome(&outcome(
            "1",
            HealthKind::UrlTest,
            None,
            Some("url test failed for \"A\": refused"),
        ));
        masked.record_outcome(&outcome("1", HealthKind::TcpPing, Some(7), None));
        let status = masked.status("1");
        assert!(
            matches!(status.state, RowState::Dead),
            "ping must not mask url failure: {status:?}"
        );
        assert!(masked.row_summary("1") == "dead", "list shows dead");
        // Both kinds live: live with a latency.
        let mut live = HealthStore::default();
        live.record_outcome(&outcome("1", HealthKind::TcpPing, Some(7), None));
        live.record_outcome(&outcome("1", HealthKind::UrlTest, Some(42), None));
        let status = live.status("1");
        assert!(matches!(status.state, RowState::Live));
        assert_eq!(status.latency_ms, Some(42));
    }

    /// Ticket 32: stale samples expire — pruning drops samples older than
    /// the TTL so dead history cannot haunt a row forever (and fresh
    /// samples survive).
    #[test]
    fn t32_stale_samples_expire() {
        let now = now_unix();
        let mut store = HealthStore::default();
        store.record_outcome(&TestOutcome {
            sample: HealthSample {
                config_id: "old".to_string(),
                at: now - 10_000,
                kind: HealthKind::UrlTest,
                latency_ms: None,
                error: Some("url test failed for \"Old\": refused".to_string()),
            },
            success: None,
        });
        store.record_outcome(&TestOutcome {
            sample: HealthSample {
                config_id: "fresh".to_string(),
                at: now,
                kind: HealthKind::UrlTest,
                latency_ms: Some(42),
                error: None,
            },
            success: Some(ProbeSuccess {
                latency_ms: 42,
                bytes_rx: 1200,
                bytes_tx: 300,
            }),
        });
        store.prune_stale(now, 300);
        assert!(
            matches!(store.status("old").state, RowState::Unknown),
            "stale expires"
        );
        assert!(
            matches!(store.status("fresh").state, RowState::Live),
            "fresh survives"
        );
        assert_eq!(store.row_summary("fresh"), "42ms");
    }

    /// Ticket 12: hydrated snapshots restore latencies and dead flags;
    /// traffic stays zeroed (session-only) and unknown kinds default.
    #[test]
    fn hydrate_restores_latency_and_dead_flags_without_traffic() {
        use crate::store::HealthSnapshot;

        let snapshots = vec![
            HealthSnapshot {
                config_id: "1".to_string(),
                at: 1_700_000_000,
                kind: "url-test".to_string(),
                latency_ms: Some(87),
                error: None,
            },
            HealthSnapshot {
                config_id: "2".to_string(),
                at: 1_700_000_001,
                kind: "tcp-ping".to_string(),
                latency_ms: None,
                error: Some("tcp ping failed for \"Beta\": dial refused".to_string()),
            },
            HealthSnapshot {
                config_id: "3".to_string(),
                at: 1_700_000_002,
                kind: "mystery-kind".to_string(),
                latency_ms: Some(5),
                error: None,
            },
        ];
        let mut store = HealthStore::default();
        store.hydrate(&snapshots);

        let live = store.status("1");
        assert!(matches!(live.state, RowState::Live));
        assert_eq!(live.latency_ms, Some(87));
        assert_eq!(store.row_summary("1"), "87ms");
        let dead = store.status("2");
        assert!(matches!(dead.state, RowState::Dead));
        assert!(
            dead.cause.as_deref().unwrap_or("").contains("dial refused"),
            "dead flag keeps its named cause: {:?}",
            dead.cause
        );
        assert_eq!(store.row_summary("2"), "dead");
        // Unknown kinds default to URL tests, never panic.
        assert_eq!(
            store.latest("3").map(|sample| sample.kind),
            Some(HealthKind::UrlTest)
        );
        assert_eq!(
            store
                .latest_of("2", HealthKind::TcpPing)
                .map(|s| &s.config_id),
            Some(&"2".to_string())
        );
        // Traffic is session-only: hydration moves no bytes.
        assert_eq!(store.traffic("1"), TrafficStats::default());
        assert_eq!(store.status("missing").state, RowState::Unknown);
    }

    /// TCP pings record dial timing under their own kind.
    #[test]
    fn tcp_ping_records_timing_per_endpoint() {
        assert_eq!(HealthKind::UrlTest.as_str(), "url-test");
        assert_eq!(HealthKind::TcpPing.as_str(), "tcp-ping");
        let configs = vec![fixture("1", "Alpha", "vless", "127.0.0.1", 10801)];
        let (prober, handle) = FakeHealthProber::shared();
        handle.script_latency("1", 7);
        let outcomes = ping_all(&configs, &prober);
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].sample.kind, HealthKind::TcpPing);
        assert_eq!(outcomes[0].sample.latency_ms, Some(7));
        assert_eq!(handle.calls_for("1")[0].port, 10801);
    }

    /// Denied protocols get a named skip without probing; standard rows
    /// still run. The deny hook simulates unprobeable rows in tests.
    /// Ticket 27: AnyTLS/SSH/Noise are probeable now — the residual skip
    /// case is a genuinely unprobeable protocol (`naive`).
    #[test]
    fn unsupported_protocol_skipped_without_probing() {
        let configs = vec![
            fixture("1", "Alpha", "vless", "127.0.0.1", 10801),
            fixture("9", "Odd", "naive", "127.0.0.1", 10809),
        ];
        let (prober, handle) = FakeHealthProber::shared();
        handle.deny_protocol("naive");
        let outcomes = test_all(&configs, DEFAULT_TEST_URL, &prober);
        assert_eq!(outcomes.len(), 2);
        assert!(outcomes[0].sample.error.is_none());
        let skip = outcomes[1].sample.error.as_deref().expect("named skip");
        assert!(skip.contains("naive"), "names protocol: {skip}");
        assert!(skip.contains("\"Odd\""), "names config: {skip}");
        assert!(
            handle.calls_for("9").is_empty(),
            "denied rows are never probed"
        );
        assert!(!handle.calls_for("1").is_empty());

        let real = RealHealthProber::default();
        assert!(real.supports(&configs[0]));
        assert!(!real.supports(&configs[1]), "real driver denies naive");
        assert!(is_probeable_protocol("ss"));
        assert!(!is_probeable_protocol("naive"));
    }

    /// Ticket 07: AWG rows are probeable through their own endpoints (deny
    /// list flipped); rows with no probe stay denied.
    /// Ticket 27: the still-denied example is `naive` (AnyTLS probes now).
    #[test]
    fn amneziawg_rows_are_probeable_through_own_endpoints() {
        for protocol in ["amneziawg", "awg", "amnezia-wg"] {
            assert!(is_probeable_protocol(protocol), "{protocol} is probeable");
        }
        let configs = vec![
            fixture("7", "AwgOne", "amneziawg", "127.0.0.1", 10807),
            fixture("9", "Odd", "naive", "127.0.0.1", 10809),
        ];
        let (prober, handle) = FakeHealthProber::shared();
        handle.deny_protocol("naive");
        let outcomes = test_all(&configs, DEFAULT_TEST_URL, &prober);
        assert_eq!(outcomes.len(), 2);
        assert!(outcomes[0].sample.error.is_none());
        assert!(outcomes[1].sample.error.is_some());
        let calls = handle.calls_for("7");
        assert_eq!(calls.len(), 1, "AWG row is probed, not skipped");
        assert_eq!(calls[0].bind, "127.0.0.1");
        assert_eq!(calls[0].port, 10807, "own endpoint, not a sibling's");

        let real = RealHealthProber::default();
        assert!(real.supports(&configs[0]), "real driver probes AWG");
        assert!(
            !real.supports(&configs[1]),
            "real driver still denies naive"
        );
    }

    /// Ticket 08: Xray/custom/extra-core/chained rows are probeable through
    /// their own endpoints (deny-list hook flipped); each probe dials its
    /// own endpoint so results stay per-egress.
    #[test]
    fn ticket08_rows_are_probeable_through_own_endpoints() {
        for protocol in [
            "xray",
            "xray-vless",
            "custom",
            "custom-outbound",
            "custom-config",
            "extra-core",
            "extracore",
            "chain",
        ] {
            assert!(is_probeable_protocol(protocol), "{protocol} is probeable");
        }
        let configs = vec![
            fixture("2", "XrayOne", "xray", "127.0.0.1", 10802),
            fixture("9", "CustomOne", "custom-outbound", "127.0.0.1", 10809),
            fixture("5", "Slot", "extra-core", "127.0.0.1", 10805),
            fixture("8", "ChainAB", "chain", "127.0.0.1", 10808),
        ];
        let (prober, handle) = FakeHealthProber::shared();
        let outcomes = test_all(&configs, DEFAULT_TEST_URL, &prober);
        assert_eq!(outcomes.len(), 4);
        for (config, outcome) in configs.iter().zip(outcomes.iter()) {
            assert!(
                outcome.sample.error.is_none(),
                "{} probed: {:?}",
                config.name,
                outcome.sample.error
            );
            let calls = handle.calls_for(&config.id);
            assert_eq!(calls.len(), 1, "one probe for {}", config.name);
            assert_eq!(calls[0].bind, config.bind_ip);
            assert_eq!(calls[0].port, config.port, "own endpoint, not a sibling's");
        }
        let real = RealHealthProber::default();
        for config in &configs {
            assert!(real.supports(config), "real driver probes {}", config.name);
        }
    }

    /// Ticket 27: SSH/AnyTLS/Noise rows are probeable through their own
    /// endpoints (native engine support landed); each probe dials its own
    /// endpoint so results stay per-egress. Traffic counters stay
    /// probe-bytes per the spec note (data-plane deferred).
    #[test]
    fn ticket27_ssh_anytls_noise_probeable_through_own_endpoints() {
        for protocol in ["ssh", "anytls", "noise"] {
            assert!(is_probeable_protocol(protocol), "{protocol} is probeable");
        }
        let configs = vec![
            fixture("1", "SshOne", "ssh", "127.0.0.1", 10801),
            fixture("9", "AnyOne", "anytls", "127.0.0.1", 10809),
            fixture("3", "NoiseOne", "noise", "127.0.0.1", 10803),
        ];
        let (prober, handle) = FakeHealthProber::shared();
        let outcomes = test_all(&configs, DEFAULT_TEST_URL, &prober);
        assert_eq!(outcomes.len(), 3);
        let mut store = HealthStore::default();
        for (config, outcome) in configs.iter().zip(outcomes.iter()) {
            assert!(
                outcome.sample.error.is_none(),
                "{} probed: {:?}",
                config.name,
                outcome.sample.error
            );
            assert!(
                outcome.success.is_some(),
                "{} carries probe bytes",
                config.name
            );
            let calls = handle.calls_for(&config.id);
            assert_eq!(calls.len(), 1, "one probe for {}", config.name);
            assert_eq!(calls[0].bind, config.bind_ip);
            assert_eq!(calls[0].port, config.port, "own endpoint, not a sibling's");
            store.record_outcome(outcome);
        }
        // Probe-bytes only: no data-plane traffic is invented here.
        for config in &configs {
            let stats = store.traffic(&config.id);
            assert_eq!(stats.probes, 1);
            assert!(stats.rx_bytes > 0 && stats.tx_bytes > 0);
        }
        let real = RealHealthProber::default();
        for config in &configs {
            assert!(real.supports(config), "real driver probes {}", config.name);
        }
    }

    /// Traffic counters accumulate per config across probes.
    #[test]
    fn traffic_stats_accumulate_per_config() {
        let configs = vec![fixture("1", "Alpha", "vless", "127.0.0.1", 10801)];
        let (prober, _) = FakeHealthProber::shared();
        let mut store = HealthStore::default();
        for outcome in test_all(&configs, DEFAULT_TEST_URL, &prober) {
            store.record_outcome(&outcome);
        }
        for outcome in ping_all(&configs, &prober) {
            store.record_outcome(&outcome);
        }
        let stats = store.traffic("1");
        assert_eq!(stats.probes, 2);
        assert_eq!(stats.rx_bytes, 2400);
        assert_eq!(stats.tx_bytes, 600);
        assert_eq!(store.traffic("missing"), TrafficStats::default());
        assert!(store.detail_line("1").contains("↑"), "traffic shown");
        assert_eq!(store.detail_line("missing"), "health: not tested");
    }

    /// Ticket 33: probe-byte counters are labeled `probe:` (never bare
    /// `traffic` — v1 has no data-plane meter).
    #[test]
    fn traffic_text_is_labeled_probe() {
        let configs = vec![fixture("1", "Alpha", "vless", "127.0.0.1", 10801)];
        let (prober, _) = FakeHealthProber::shared();
        let mut store = HealthStore::default();
        assert!(
            store.detail_line("1") == "health: not tested",
            "untested row names the state"
        );
        for outcome in test_all(&configs, DEFAULT_TEST_URL, &prober) {
            store.record_outcome(&outcome);
        }
        let line = store.detail_line("1");
        assert!(line.contains("probe:"), "probe bytes labeled, got {line}");
        assert!(line.contains("1 probe(s)"), "probe count shown, got {line}");
    }

    /// Ticket 36: the clash `/connections` totals parse (live shape from
    /// stock sing-box 1.14.2); missing fields and garbage fail named.
    #[test]
    fn ticket36_parse_clash_totals() {
        let (rx, tx) = crate::health::parse_clash_totals(
            r#"{"connections":[],"downloadTotal":1048782,"memory":4497408,"uploadTotal":88}"#,
        )
        .expect("live shape parses");
        assert_eq!((rx, tx), (1048782, 88));
        assert!(
            crate::health::parse_clash_totals(r#"{"connections":[]}"#).is_err(),
            "missing totals fail"
        );
        assert!(
            crate::health::parse_clash_totals("not json").is_err(),
            "garbage fails"
        );
    }

    /// Ticket 36: once data-plane totals exist, the traffic line reports
    /// them (not probe bytes); untracked rows keep the probe label.
    #[test]
    fn ticket36_traffic_text_prefers_data_plane() {
        let configs = vec![fixture("1", "Alpha", "vless", "127.0.0.1", 10801)];
        let (prober, _) = FakeHealthProber::shared();
        let mut store = HealthStore::default();
        for outcome in test_all(&configs, DEFAULT_TEST_URL, &prober) {
            store.record_outcome(&outcome);
        }
        store.set_data_traffic("1", 1_048_782, 88);
        let line = store.detail_line("1");
        assert!(line.contains("↑"), "data bytes shown, got {line}");
        assert!(
            !line.contains("probe: ↑"),
            "probe bytes no longer the source, got {line}"
        );
        let untracked = store.detail_line("9");
        assert_eq!(untracked, "health: not tested");
    }

    /// Ticket 36: the clash poller reads totals off a live controller
    /// (stub TCP server speaking the live shape) and rejects bad auth.
    #[test]
    fn ticket36_poll_clash_totals() {
        use std::io::Write as _;
        use std::time::Duration;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("stub binds");
        let port = listener.local_addr().expect("addr works").port();
        let server = std::thread::spawn(move || {
            for stream in listener.incoming().take(2) {
                let mut stream = stream.expect("accept works");
                let mut head = [0u8; 1024];
                use std::io::Read as _;
                let n = stream.read(&mut head).unwrap_or(0);
                let authed =
                    String::from_utf8_lossy(&head[..n]).contains("Authorization: Bearer s3cret");
                let body = if authed {
                    r#"{"connections":[],"downloadTotal":1048782,"memory":1,"uploadTotal":88}"#
                } else {
                    r#"{"error":"unauthorized"}"#
                };
                let status = if authed { "200 OK" } else { "401 Unauthorized" };
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .expect("reply works");
            }
        });
        let (rx, tx) = crate::health::poll_clash_totals(port, "s3cret", Duration::from_secs(5))
            .expect("authed poll works");
        assert_eq!((rx, tx), (1048782, 88));
        assert!(
            crate::health::poll_clash_totals(port, "wrong", Duration::from_secs(5)).is_err(),
            "bad secret fails"
        );
        server.join().expect("stub drains");
        assert!(
            crate::health::poll_clash_totals(1, "s3cret", Duration::from_millis(200)).is_err(),
            "refused port fails"
        );
    }

    /// Ticket 36: connection entries parse with per-connection bytes and
    /// the owning row (from `metadata.type: "mixed/in-<rowid>"`); probe
    /// destinations filter out so health checks never pollute counters.
    #[test]
    fn ticket36_parse_clash_entries() {
        let body = r#"{"connections":[{"chains":["direct"],"download":2877275,"id":"a99cac52","metadata":{"destinationIP":"127.0.0.1","destinationPort":"19097","host":"","network":"tcp","sourceIP":"127.0.0.1","sourcePort":"51241","type":"mixed/in-7"},"rule":"final","upload":89},{"chains":["direct"],"download":500,"id":"probe-1","metadata":{"destinationIP":"93.184.216.34","destinationPort":"80","host":"www.gstatic.com","network":"tcp","sourceIP":"127.0.0.1","sourcePort":"51242","type":"mixed/in-7"},"rule":"final","upload":60}],"downloadTotal":2877775,"uploadTotal":149}"#;
        let entries = crate::health::parse_clash_entries(body).expect("entries parse");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].row_id.as_deref(), Some("7"));
        assert_eq!((entries[0].rx_bytes, entries[0].tx_bytes), (2877275, 89));
        let attributed = crate::health::attribute_entries(&entries, "www.gstatic.com");
        assert_eq!(
            attributed.get("7"),
            Some(&(2877275, 89)),
            "probe flow excluded"
        );
    }

    /// Scenario 2: per-config log filter shows only its lines, secrets
    /// redacted; engine-global lines stay out.
    #[test]
    fn logs_filter_to_config_and_redact_secrets() {
        let active = vec![
            fixture("1", "Alpha", "vless", "127.0.0.1", 10801),
            fixture("2", "Beta", "vless", "127.0.0.1", 10802),
        ];
        let mut logs = LogStore::default();
        let tags = LogTagIndex::build(&active);
        logs.push_raw(
            "warn out-1: dial failed password=hunter2 uuid=11111111-2222-4333-8444-555555555555",
            &tags,
        );
        logs.push_raw("error in-2: upstream reset", &tags);
        logs.push_raw("info engine listening on 127.0.0.1:10801", &tags);
        let alpha = logs.for_config("1");
        assert_eq!(alpha.len(), 1, "only its lines show");
        assert_eq!(alpha[0].config_id.as_deref(), Some("1"));
        assert_eq!(alpha[0].level, LogLevel::Warn);
        assert!(
            !alpha[0].message.contains("hunter2"),
            "no plaintext secrets: {}",
            alpha[0].message
        );
        assert!(
            !alpha[0].message.contains("11111111-2222"),
            "uuids are secrets: {}",
            alpha[0].message
        );
        assert!(
            alpha[0].message.contains("out-1"),
            "tag preserved: {}",
            alpha[0].message
        );
        assert_eq!(logs.for_config("2").len(), 1);
        assert!(
            logs.all().iter().any(|record| record.config_id.is_none()),
            "untagged lines stay engine-global"
        );
    }

    /// Ticket 45: sparkline latencies take the newest successful samples
    /// (oldest-first for the chart), capped — errors never plot as zero.
    #[test]
    fn t45_sparkline_latencies_newest_capped() {
        let samples: Vec<(i64, Option<u64>)> = vec![
            (1, Some(100)),
            (2, None),
            (3, Some(200)),
            (4, Some(300)),
            (5, None),
            (6, Some(400)),
        ];
        assert_eq!(sparkline_latencies(&samples, 10), vec![100, 200, 300, 400]);
        assert_eq!(sparkline_latencies(&samples, 2), vec![300, 400]);
        assert!(sparkline_latencies(&[], 10).is_empty());
        let errors: Vec<(i64, Option<u64>)> = vec![(1, None), (2, None)];
        assert!(sparkline_latencies(&errors, 10).is_empty());
    }

    /// Ticket 45: log levels round-trip through labels for the level filter.
    #[test]
    fn t45_log_level_labels() {
        assert_eq!(LogLevel::Debug.as_str(), "debug");
        assert_eq!(LogLevel::Info.as_str(), "info");
        assert_eq!(LogLevel::Warn.as_str(), "warn");
        assert_eq!(LogLevel::Error.as_str(), "error");
        assert_eq!(LogLevel::from_label("warn"), Some(LogLevel::Warn));
        assert_eq!(LogLevel::from_label("all"), None);
        assert_eq!(LogLevel::from_label("bogus"), None);
    }

    /// Redaction covers key forms, bearer tokens, userinfo, and queries.
    #[test]
    fn redact_secrets_covers_credential_forms() {
        for (raw, leaked) in [
            ("password=hunter2", "hunter2"),
            ("\"password\": \"hunter2\"", "hunter2"),
            ("token: abc123", "abc123"),
            ("Bearer abc123def", "abc123def"),
            (
                "vless://11111111-2222-4333-8444-555555555555@host:443#x",
                "11111111-2222",
            ),
            ("https://host/path?token=abc123", "abc123"),
            ("private_key=LOCAL-PRIVATE", "LOCAL-PRIVATE"),
        ] {
            let redacted = redact_secrets(raw);
            assert!(
                !redacted.contains(leaked),
                "leaked {leaked} in {redacted} (from {raw})"
            );
        }
        let benign = "warn out-1: dial 203.0.113.7:8388 via Alpha";
        assert_eq!(
            redact_secrets(benign),
            benign,
            "hosts, tags, and names pass through"
        );
    }

    /// Byte formatting and truncation stay readable in rows.
    #[test]
    fn format_bytes_and_truncate() {
        assert_eq!(format_bytes(0), "0B");
        assert_eq!(format_bytes(512), "512B");
        assert_eq!(format_bytes(2048), "2.0KB");
        assert_eq!(truncate("abcdef", 10), "abcdef");
        assert!(truncate(&"x".repeat(200), 140).ends_with('…'));
    }

    /// Endpoint normalization: wildcards dial loopback; bad binds name the
    /// endpoint without secrets.
    #[test]
    fn endpoint_addr_normalizes_wildcards() {
        assert_eq!(
            endpoint_addr("0.0.0.0", 10801)
                .expect("parses")
                .ip()
                .to_string(),
            "127.0.0.1"
        );
        assert_eq!(
            endpoint_addr("::", 10801).expect("parses").ip().to_string(),
            "::1"
        );
        assert!(endpoint_addr("not a host!", 1).is_err());
        assert!(split_http_url("https://example.com/x").is_ok());
        assert!(split_http_url("notaurl").is_err());
        assert!(split_http_url(DEFAULT_TEST_URL).is_ok());
    }

    // -- Ticket 41: probe attribution normalization + cause classifier ------

    /// Normalization strips casing, ports, schemes, paths, brackets, and
    /// one trailing dot; bare IPv6 keeps its colons.
    #[test]
    fn t41_normalize_probe_host_variants() {
        assert_eq!(normalize_probe_host("www.gstatic.com"), "www.gstatic.com");
        assert_eq!(
            normalize_probe_host("WWW.GStatic.COM.:443"),
            "www.gstatic.com"
        );
        assert_eq!(
            normalize_probe_host("https://Example.COM:8443/generate_204?q=1"),
            "example.com"
        );
        assert_eq!(normalize_probe_host("[::1]:8080"), "::1");
        assert_eq!(normalize_probe_host("[::1]"), "::1");
        assert_eq!(normalize_probe_host("2001:db8::1"), "2001:db8::1");
        assert_eq!(normalize_probe_host("93.184.216.34:80"), "93.184.216.34");
        assert_eq!(normalize_probe_host("  Example.COM. "), "example.com");
        assert_eq!(normalize_probe_host(""), "");
    }

    /// Probe matching holds across case/port/dot variants and resolved IPs;
    /// strangers and empty probes never match.
    #[test]
    fn t41_probe_match_variants_and_resolved_ips() {
        let probe = "www.gstatic.com";
        for dest in [
            "www.gstatic.com",
            "WWW.GSTATIC.COM:443",
            "www.gstatic.com.",
            "https://www.gstatic.com/generate_204",
        ] {
            assert!(is_probe_destination(dest, probe), "probe: {dest}");
        }
        assert!(!is_probe_destination("example.com", probe));
        assert!(!is_probe_destination("www.gstatic.com", ""));
        assert!(!is_probe_destination("", probe));
        // Resolved-IP leg: clash reports the IP while health dials the name.
        let ips = vec!["93.184.216.34".parse().expect("ip parses")];
        assert!(is_probe_destination_with_ips("93.184.216.34", probe, &ips));
        assert!(is_probe_destination_with_ips(
            "93.184.216.34:443",
            probe,
            &ips
        ));
        assert!(!is_probe_destination_with_ips("93.184.216.34", probe, &[]));
        assert!(!is_probe_destination_with_ips("203.0.113.7", probe, &ips));
        // Numeric probe hosts resolve without DNS.
        assert_eq!(resolve_probe_ips("93.184.216.34"), ips, "numeric fast path");
        assert!(resolve_probe_ips("").is_empty());
    }

    /// Attribution excludes probe flows in every variant; legit flows count.
    #[test]
    fn t41_attribute_excludes_all_probe_variants() {
        fn entry(id: &str, row: &str, dest: &str, rx: u64, tx: u64) -> ClashConn {
            ClashConn {
                id: id.to_string(),
                row_id: Some(row.to_string()),
                rx_bytes: rx,
                tx_bytes: tx,
                dest: dest.to_string(),
            }
        }
        let probe = "www.gstatic.com";
        let ips = vec!["93.184.216.34".parse().expect("ip parses")];
        let entries = vec![
            entry("a", "7", "WWW.GSTATIC.COM:443", 500, 60),
            entry("b", "7", "www.gstatic.com.", 500, 60),
            entry("c", "7", "93.184.216.34", 500, 60),
            entry("d", "7", "93.184.216.34:443", 500, 60),
            entry("e", "7", "203.0.113.7", 1000, 100),
            entry("f", "8", "198.51.100.9", 2000, 200),
        ];
        let attributed = attribute_entries_with_ips(&entries, probe, &ips);
        assert_eq!(attributed.get("7"), Some(&(1000, 100)), "{attributed:?}");
        assert_eq!(attributed.get("8"), Some(&(2000, 200)), "{attributed:?}");
        // Hostname-leg compat entry point still excludes exact probes.
        let hostname_only = attribute_entries(&entries[4..], probe);
        assert_eq!(hostname_only.get("7"), Some(&(1000, 100)));
    }

    /// The cause classifier tells DNS failure from local and egress failure.
    #[test]
    fn t41_classify_health_cause() {
        for cause in [
            "cannot resolve endpoint bad.invalid:443",
            "DNS lookup failed: nodename nor servname",
            "temporary failure in name resolution",
            "getaddrinfo EAI_NONAME",
            "no such host",
        ] {
            assert_eq!(classify_health_cause(cause), HealthCause::Dns, "{cause}");
        }
        for cause in [
            "port 18501 is inside the reserved clash-controller range",
            "bad local port (port 0 serves nothing)",
            "LAN confirm required",
            "bind permission denied",
        ] {
            assert_eq!(classify_health_cause(cause), HealthCause::Local, "{cause}");
        }
        for cause in [
            "dial 203.0.113.7:8388: connection refused",
            "url test timed out",
            "upstream reset by peer",
        ] {
            assert_eq!(classify_health_cause(cause), HealthCause::Egress, "{cause}");
        }
    }

    /// Dead rows carry the classified kind on the status seam; live and
    /// unknown rows carry none.
    #[test]
    fn t41_status_carries_cause_kind() {
        fn outcome(id: &str, error: Option<&str>) -> TestOutcome {
            TestOutcome {
                sample: HealthSample {
                    config_id: id.to_string(),
                    at: now_unix(),
                    kind: HealthKind::UrlTest,
                    latency_ms: if error.is_none() { Some(9) } else { None },
                    error: error.map(str::to_string),
                },
                success: None,
            }
        }
        let mut store = HealthStore::default();
        store.record_outcome(&outcome(
            "dns",
            Some("cannot resolve endpoint bad.invalid:443"),
        ));
        store.record_outcome(&outcome(
            "egress",
            Some("dial 203.0.113.7:8388: connection refused"),
        ));
        store.record_outcome(&outcome("live", None));
        let dns = store.status("dns");
        assert!(matches!(dns.state, RowState::Dead));
        assert_eq!(dns.cause_kind, Some(HealthCause::Dns));
        assert!(dns.cause.as_deref().unwrap_or("").contains("bad.invalid"));
        let egress = store.status("egress");
        assert_eq!(egress.cause_kind, Some(HealthCause::Egress));
        let live = store.status("live");
        assert!(matches!(live.state, RowState::Live));
        assert_eq!(live.cause_kind, None);
        assert_eq!(store.status("missing").cause_kind, None);
    }

    /// Stale markers flag crashed rows' totals and clear on fresh polls.
    #[test]
    fn t41_stale_markers_flag_and_clear() {
        let configs = vec![fixture("1", "Alpha", "vless", "127.0.0.1", 10801)];
        let (prober, _) = FakeHealthProber::shared();
        let mut store = HealthStore::default();
        for outcome in test_all(&configs, DEFAULT_TEST_URL, &prober) {
            store.record_outcome(&outcome);
        }
        store.set_data_traffic("1", 1_048_782, 88);
        assert!(!store.is_stale("1"));
        assert!(!store.detail_line("1").contains("stale"));
        store.mark_stale("1");
        assert!(store.is_stale("1"));
        let line = store.detail_line("1");
        assert!(line.contains("(stale)"), "stale reads stale: {line}");
        assert!(line.contains("↑"), "totals kept: {line}");
        store.mark_stale_all(&["1".to_string(), "2".to_string()]);
        assert!(store.is_stale("2"));
        store.clear_stale("1");
        assert!(!store.is_stale("1"));
        assert!(!store.detail_line("1").contains("stale"));
    }

    /// Ticket 47 (RED): bounded concurrent probes — 16 slow probes finish in
    /// ~2 waves (8-way) with at most 8 in flight, outcomes in input order.
    #[test]
    fn t47_concurrent_probes_bounded_and_ordered() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        struct CountingProber {
            delay: std::time::Duration,
            live: Arc<AtomicUsize>,
            max: Arc<AtomicUsize>,
        }
        impl HealthProber for CountingProber {
            fn url_test(
                &self,
                _config: &ProxyConfig,
                _bind: &str,
                _port: u16,
                _url: &str,
            ) -> Result<ProbeSuccess, String> {
                let now = self.live.fetch_add(1, Ordering::SeqCst) + 1;
                self.max.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(self.delay);
                self.live.fetch_sub(1, Ordering::SeqCst);
                Ok(ProbeSuccess {
                    latency_ms: 7,
                    bytes_rx: 1,
                    bytes_tx: 1,
                })
            }
            fn tcp_ping(
                &self,
                _config: &ProxyConfig,
                _bind: &str,
                _port: u16,
            ) -> Result<ProbeSuccess, String> {
                let now = self.live.fetch_add(1, Ordering::SeqCst) + 1;
                self.max.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(self.delay);
                self.live.fetch_sub(1, Ordering::SeqCst);
                Ok(ProbeSuccess {
                    latency_ms: 7,
                    bytes_rx: 0,
                    bytes_tx: 0,
                })
            }
        }

        assert_eq!(MAX_PROBE_CONCURRENCY, 8, "bound pinned");
        let configs: Vec<ProxyConfig> = (0..16)
            .map(|i| {
                fixture(
                    &i.to_string(),
                    &format!("Row{i}"),
                    "vless",
                    "127.0.0.1",
                    10801,
                )
            })
            .collect();
        let live = Arc::new(AtomicUsize::new(0));
        let max = Arc::new(AtomicUsize::new(0));
        let prober = CountingProber {
            delay: std::time::Duration::from_millis(100),
            live: live.clone(),
            max: max.clone(),
        };
        let started = std::time::Instant::now();
        let outcomes = test_all_concurrent(&configs, DEFAULT_TEST_URL, &prober);
        let wall = started.elapsed();
        assert_eq!(outcomes.len(), 16, "one outcome per row");
        for (i, outcome) in outcomes.iter().enumerate() {
            assert_eq!(outcome.sample.config_id, i.to_string(), "input order kept");
            assert_eq!(outcome.sample.latency_ms, Some(7));
        }
        let peak = max.load(Ordering::SeqCst);
        assert!(peak >= 2, "probes overlap, peak was {peak}");
        assert!(
            peak <= MAX_PROBE_CONCURRENCY,
            "bounded at 8-way, peak was {peak}"
        );
        assert!(
            wall < std::time::Duration::from_millis(800),
            "16x100ms in ~2 waves, took {wall:?}"
        );
    }

    /// Ticket 47 (RED): concurrent clash polls — fast cores return entries
    /// while a slow core fails closed on the per-core timeout, and the whole
    /// round lands well under the serial sum (parallel, not sequential).
    #[test]
    fn t47_clash_polls_concurrent_with_per_core_timeout() {
        use std::io::{Read as _, Write as _};
        use std::time::{Duration, Instant};

        assert!(
            (1..=2).contains(&CLASH_PER_CORE_TIMEOUT_SECS),
            "per-core timeout 1-2s"
        );
        assert!(
            CLASH_TOTAL_DEADLINE.as_secs() >= CLASH_PER_CORE_TIMEOUT.as_secs(),
            "total deadline covers one per-core timeout"
        );

        fn fast_stub(entries: &str) -> (u16, std::thread::JoinHandle<()>) {
            let body = entries.to_string();
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("stub binds");
            let port = listener.local_addr().expect("addr works").port();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept works");
                let mut head = [0u8; 2048];
                let _ = stream.read(&mut head);
                let payload = body.clone();
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                            payload.len()
                        )
                        .as_bytes(),
                    )
                    .expect("reply works");
            });
            (port, server)
        }
        fn slow_stub(delay: Duration) -> (u16, std::thread::JoinHandle<()>) {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("stub binds");
            let port = listener.local_addr().expect("addr works").port();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept works");
                let mut head = [0u8; 2048];
                let _ = stream.read(&mut head);
                std::thread::sleep(delay);
                let payload = r#"{"connections":[]}"#.to_string();
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                        payload.len()
                    )
                    .as_bytes(),
                );
            });
            (port, server)
        }

        let entry_body = r#"{"connections":[
            {"download": 100, "id": "c1",
             "metadata": {"host": "example.com", "type": "mixed/in-7"},
             "upload": 10}]}"#;
        let (fast1, srv1) = fast_stub(entry_body);
        let (fast2, srv2) = fast_stub(entry_body);
        let (slow, srv_slow) = slow_stub(Duration::from_secs(3));
        let targets = vec![
            crate::engine::ClashTarget {
                core: "a".to_string(),
                port: fast1,
                secret: "s".to_string(),
            },
            crate::engine::ClashTarget {
                core: "b".to_string(),
                port: fast2,
                secret: "s".to_string(),
            },
            crate::engine::ClashTarget {
                core: "slow".to_string(),
                port: slow,
                secret: "s".to_string(),
            },
        ];
        let started = Instant::now();
        let mut results =
            poll_clash_targets(&targets, Duration::from_secs(1), Duration::from_secs(4));
        results.sort_by(|a, b| a.0.core.cmp(&b.0.core));
        let wall = started.elapsed();
        assert_eq!(results.len(), 3, "one result per target");
        for (target, result) in &results {
            if target.core == "slow" {
                assert!(result.is_err(), "slow core fails closed, got {result:?}");
            } else {
                let entries = result.as_ref().expect("fast core answers");
                assert_eq!(entries.len(), 1, "entries parsed");
                assert_eq!(entries[0].row_id.as_deref(), Some("7"));
            }
        }
        assert!(
            wall < Duration::from_millis(2500),
            "parallel round beats the serial sum, took {wall:?}"
        );
        srv1.join().expect("fast stub drains");
        srv2.join().expect("fast stub drains");
        srv_slow.join().expect("slow stub drains");
    }

    /// Ticket 47 (RED): the total deadline cuts a round of all-slow cores —
    /// every result is a deadline error and the round returns promptly.
    #[test]
    fn t47_clash_polls_total_deadline_caps_all_slow() {
        use std::io::Read as _;
        use std::time::{Duration, Instant};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("stub binds");
        let port = listener.local_addr().expect("addr works").port();
        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept works");
                let mut head = [0u8; 2048];
                let _ = stream.read(&mut head);
                std::thread::sleep(Duration::from_secs(10));
            }
        });
        let targets = vec![
            crate::engine::ClashTarget {
                core: "s1".to_string(),
                port,
                secret: "s".to_string(),
            },
            crate::engine::ClashTarget {
                core: "s2".to_string(),
                port,
                secret: "s".to_string(),
            },
        ];
        let started = Instant::now();
        let results = poll_clash_targets(&targets, Duration::from_secs(10), Duration::from_secs(1));
        let wall = started.elapsed();
        assert_eq!(results.len(), 2);
        for (target, result) in &results {
            assert!(
                result.is_err(),
                "{} fails closed on the total deadline, got {result:?}",
                target.core
            );
        }
        assert!(
            wall < Duration::from_secs(3),
            "total deadline caps the round, took {wall:?}"
        );
        let _ = server;
    }

    /// Ticket 47 (RED): ingest strips ANSI once and caches the lowercase
    /// search blob — stored records carry no escapes.
    #[test]
    fn t47_log_ingest_strips_ansi_and_caches_search() {
        let tags = LogTagIndex::build(&[]);
        let mut logs = LogStore::default();
        let esc = char::from(27u8);
        logs.push_raw(&format!("{esc}[31mERROR{esc}[0m dial failed"), &tags);
        let all = logs.all();
        assert_eq!(all.len(), 1);
        assert!(
            !all[0].message.contains('\u{1b}'),
            "stripped at ingest: {:?}",
            all[0].message
        );
        assert!(all[0].message.contains("ERROR"), "text kept");
        assert_eq!(all[0].level, LogLevel::Error);
        assert_eq!(
            all[0].search,
            format!("[{}] {}", all[0].level.as_str(), all[0].message).to_lowercase(),
            "search blob cached at ingest"
        );
    }

    /// Ticket 47 (RED): the precomputed tag index answers exactly like the
    /// per-line scan — including overlapping numeric ids (first-config-wins).
    #[test]
    fn t47_log_tag_index_matches_scan() {
        let active = vec![
            fixture("1", "Alpha", "vless", "127.0.0.1", 10801),
            fixture("12", "Dozen", "vless", "127.0.0.1", 10802),
            fixture("2", "Beta", "vless", "127.0.0.1", 10803),
        ];
        let index = LogTagIndex::build(&active);
        for line in [
            "warn in-1: dial failed",
            "error out-12: upstream reset",
            "info in-12 trailing in-1: overlap",
            "debug out-2: closed",
            "info engine listening on 127.0.0.1:10801",
            "in- deeper prefix",
            "warn out-1 then out-12: first wins",
        ] {
            assert_eq!(
                index.tag_for(line),
                tag_for_line(line, &active),
                "index parity for {line:?}"
            );
        }
        assert_eq!(index.tag_for("warn in-1: x").as_deref(), Some("1"));
        // Substring-overlap quirk (pre-existing, preserved): "out-12"
        // contains "out-1", so first-config-wins claims it for "1" — parity,
        // never longest-match.
        assert_eq!(index.tag_for("error out-12: y").as_deref(), Some("1"));
        assert_eq!(index.tag_for("info engine started"), None);
        // Without the overlapping id, 12 wins its own tag.
        let index12 = LogTagIndex::build(&active[1..]);
        assert_eq!(index12.tag_for("error out-12: y").as_deref(), Some("12"));
    }

    /// Ticket 47 (RED): the log ring stays capped and crash-cause lines
    /// survive churn (pinned past normal-line eviction).
    #[test]
    fn t47_log_ring_pins_crash_lines() {
        let tags = LogTagIndex::build(&[]);
        let mut logs = LogStore::default();
        logs.push_raw("panic: runtime error in sidecar", &tags);
        logs.push_raw("FATAL: core dumped", &tags);
        logs.push_raw("warn sidecar crashed hard", &tags);
        for i in 0..(MAX_LOG_RECORDS + 500) {
            logs.push_raw(&format!("info heartbeat {i}"), &tags);
        }
        assert_eq!(logs.all().len(), MAX_LOG_RECORDS, "ring capped");
        let messages: Vec<&str> = logs
            .all()
            .iter()
            .map(|record| record.message.as_str())
            .collect();
        assert!(messages.iter().any(|m| m.contains("panic")), "panic pinned");
        assert!(messages.iter().any(|m| m.contains("FATAL")), "fatal pinned");
        assert!(
            messages.iter().any(|m| m.contains("crashed")),
            "crash pinned"
        );
    }

    /// Ticket 47 (RED): the latest-health index answers O(1) and always
    /// agrees with a reverse scan — across interleaved kinds, ring-cap
    /// eviction, and stale pruning.
    #[test]
    fn t47_latest_health_index_matches_scan() {
        let mut store = HealthStore::default();
        let mut model: Vec<HealthSample> = Vec::new();
        let kinds = [HealthKind::UrlTest, HealthKind::TcpPing];
        for i in 0..600 {
            let config = format!("cfg{}", i % 3);
            let kind = kinds[(i / 3) % 2];
            let sample = HealthSample {
                config_id: config.clone(),
                at: 1_700_000_000 + i as i64,
                kind,
                latency_ms: Some(i as u64),
                error: None,
            };
            store.push_sample(sample.clone());
            model.push(sample);
            if model.len() > MAX_SAMPLES {
                model.remove(0);
            }
        }
        for cfg in ["cfg0", "cfg1", "cfg2"] {
            let expect_any = model.iter().rev().find(|s| s.config_id == cfg);
            assert_eq!(store.latest(cfg), expect_any, "latest {cfg}");
            for kind in kinds {
                let expect = model
                    .iter()
                    .rev()
                    .find(|s| s.config_id == cfg && s.kind == kind);
                assert_eq!(
                    store.latest_of(cfg, kind),
                    expect,
                    "latest {cfg:?}/{kind:?}"
                );
            }
        }
        assert!(store.samples.len() <= MAX_SAMPLES, "ring capped");
        // Eviction under pressure: newest index entries survive a full churn.
        for i in 0..(MAX_SAMPLES + 50) {
            let sample = HealthSample {
                config_id: "churn".to_string(),
                at: 1_800_000_000 + i as i64,
                kind: HealthKind::UrlTest,
                latency_ms: Some(i as u64),
                error: None,
            };
            store.push_sample(sample.clone());
            model.push(sample);
            if model.len() > MAX_SAMPLES {
                model.remove(0);
            }
        }
        assert!(
            store.samples.len() <= MAX_SAMPLES,
            "ring capped after churn"
        );
        let expect = model.iter().rev().find(|s| s.config_id == "churn");
        assert_eq!(store.latest("churn"), expect, "newest survives eviction");
        // Prune rebuilds the index identically.
        let now = 1_800_000_000 + MAX_SAMPLES as i64 + 100;
        store.prune_stale(now, 90);
        model.retain(|s| now.saturating_sub(s.at) <= 90);
        for cfg in ["cfg0", "cfg1", "cfg2", "churn"] {
            let expect_any = model.iter().rev().find(|s| s.config_id == cfg);
            assert_eq!(store.latest(cfg), expect_any, "latest {cfg} after prune");
            for kind in kinds {
                let expect = model
                    .iter()
                    .rev()
                    .find(|s| s.config_id == cfg && s.kind == kind);
                assert_eq!(
                    store.latest_of(cfg, kind),
                    expect,
                    "latest {cfg:?}/{kind:?} after prune"
                );
            }
        }
    }

    /// Ticket 47 (RED): the traffic last-seen map stays bounded (cap + TTL)
    /// while never dropping failed-live-core ids (no double-count on
    /// recovery) and dropping closed connections on healthy cores.
    #[test]
    fn t47_traffic_seen_map_bounds_and_sweep() {
        use std::collections::HashSet;
        let mut map = TrafficSeenMap::default();
        assert!(map.is_empty(), "fresh map tracks nothing");
        let now = 1_700_000_000i64;
        map.insert("core-a/c1".to_string(), 100, 10, now);
        assert!(!map.is_empty(), "insert tracks");
        assert_eq!(map.get("core-a/c1"), Some((100, 10)));
        // Closed on a healthy core: dropped by the sweep.
        let ok: HashSet<String> = ["core-a".to_string()].into_iter().collect();
        let live = ok.clone();
        map.sweep(now + 1, &ok, &HashSet::new(), &live);
        assert_eq!(map.get("core-a/c1"), None, "closed conn dropped");
        // Failed poll on a live core: kept across sweeps (never a mass
        // close, never TTL'd while live).
        map.insert("core-a/c2".to_string(), 50, 5, now);
        let ok_empty: HashSet<String> = HashSet::new();
        map.sweep(now + 1, &ok_empty, &HashSet::new(), &live);
        assert_eq!(map.get("core-a/c2"), Some((50, 5)), "failed-live kept");
        map.sweep(
            now + TRAFFIC_SEEN_TTL_SECS + 10,
            &ok_empty,
            &HashSet::new(),
            &live,
        );
        assert_eq!(
            map.get("core-a/c2"),
            Some((50, 5)),
            "live entries ignore TTL"
        );
        // Dead core (crashed/removed): TTL'd out so the map never grows
        // without bound across restarts.
        map.insert("dead/c9".to_string(), 7, 7, now);
        let live_only_a = live.clone();
        map.sweep(
            now + TRAFFIC_SEEN_TTL_SECS + 10,
            &ok_empty,
            &HashSet::new(),
            &live_only_a,
        );
        assert_eq!(map.get("dead/c9"), None, "dead-core entry TTL'd");
        // Cap: overflow evicts oldest evictable, never failed-live ids.
        let mut big = TrafficSeenMap::default();
        big.insert("core-a/keep".to_string(), 1, 1, now);
        for i in 0..(MAX_TRAFFIC_SEEN_ENTRIES + 100) {
            big.insert(format!("gone-{i}/c"), i as u64, 0, now + 1 + i as i64);
        }
        big.sweep(now + 2, &ok_empty, &HashSet::new(), &HashSet::new());
        assert!(
            big.len() <= MAX_TRAFFIC_SEEN_ENTRIES,
            "map capped, len={}",
            big.len()
        );
        assert_eq!(
            big.get("core-a/keep"),
            None,
            "non-live overflow entries evict (core-a not live here)"
        );
    }

    /// Ticket 47: concurrent runner matches the sequential runner
    /// outcome-for-outcome (ignoring `at` stamps) for both kinds.
    #[test]
    fn t47_concurrent_matches_sequential() {
        let configs = vec![
            fixture("1", "Alpha", "vless", "127.0.0.1", 10801),
            fixture("2", "Beta", "vless", "127.0.0.1", 10802),
            fixture("3", "Gamma", "vless", "127.0.0.1", 10803),
        ];
        let (prober, handle) = FakeHealthProber::shared();
        handle.script_failure("2", "boom");
        let seq_url = run_all(
            &configs,
            &prober,
            HealthKind::UrlTest,
            "url test",
            url_probe,
            DEFAULT_TEST_URL,
        );
        let par_url = test_all_concurrent(&configs, DEFAULT_TEST_URL, &prober);
        assert_eq!(strip_at(seq_url), strip_at(par_url), "url-test parity");
        let seq_ping = run_all(
            &configs,
            &prober,
            HealthKind::TcpPing,
            "tcp ping",
            ping_probe,
            "",
        );
        let par_ping = ping_all_concurrent(&configs, &prober);
        assert_eq!(strip_at(seq_ping), strip_at(par_ping), "tcp-ping parity");

        fn strip_at(
            outcomes: Vec<TestOutcome>,
        ) -> Vec<(String, HealthKind, Option<u64>, Option<String>)> {
            outcomes
                .into_iter()
                .map(|o| {
                    (
                        o.sample.config_id,
                        o.sample.kind,
                        o.sample.latency_ms,
                        o.sample.error,
                    )
                })
                .collect()
        }
    }
}
