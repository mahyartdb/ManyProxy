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
#[cfg(test)]
use std::collections::HashSet;
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
    fn url_test(
        &mut self,
        config: &ProxyConfig,
        bind: &str,
        port: u16,
        url: &str,
    ) -> Result<ProbeSuccess, String>;
    /// Time one TCP dial to `bind:port` (endpoint liveness).
    fn tcp_ping(
        &mut self,
        config: &ProxyConfig,
        bind: &str,
        port: u16,
    ) -> Result<ProbeSuccess, String>;
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
        &mut self,
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
        &mut self,
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
type ProbeFn =
    fn(&mut dyn HealthProber, &ProxyConfig, &str, u16, &str) -> Result<ProbeSuccess, String>;

fn run_all(
    configs: &[ProxyConfig],
    prober: &mut dyn HealthProber,
    kind: HealthKind,
    verb: &str,
    probe: ProbeFn,
    url: &str,
) -> Vec<TestOutcome> {
    configs
        .iter()
        .map(|config| {
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
        })
        .collect()
}

fn url_probe(
    prober: &mut dyn HealthProber,
    config: &ProxyConfig,
    bind: &str,
    port: u16,
    url: &str,
) -> Result<ProbeSuccess, String> {
    prober.url_test(config, bind, port, url)
}

fn ping_probe(
    prober: &mut dyn HealthProber,
    config: &ProxyConfig,
    bind: &str,
    port: u16,
    _url: &str,
) -> Result<ProbeSuccess, String> {
    prober.tcp_ping(config, bind, port)
}

/// Run one URL test per config through each config's own endpoint. Order of
/// the returned outcomes matches `configs`.
pub fn test_all(
    configs: &[ProxyConfig],
    url: &str,
    prober: &mut dyn HealthProber,
) -> Vec<TestOutcome> {
    run_all(
        configs,
        prober,
        HealthKind::UrlTest,
        "url test",
        url_probe,
        url,
    )
}

/// Time one TCP dial per config to its own endpoint.
pub fn ping_all(configs: &[ProxyConfig], prober: &mut dyn HealthProber) -> Vec<TestOutcome> {
    run_all(
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowHealth {
    pub state: RowState,
    pub latency_ms: Option<u64>,
    pub cause: Option<String>,
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
pub fn is_probe_destination(dest: &str, probe_host: &str) -> bool {
    !probe_host.is_empty()
        && (dest == probe_host
            || dest
                .strip_prefix("www.")
                .is_some_and(|rest| probe_host.strip_prefix("www.").unwrap_or(probe_host) == rest))
}

/// Ticket 36: sum entry bytes per row, excluding flows to the health-test
/// host (probes must never pollute data-plane counters) and entries no
/// row claims. Returns `row_id -> (rx, tx)`.
#[cfg_attr(not(test), allow(dead_code))]
pub fn attribute_entries(
    entries: &[ClashConn],
    probe_host: &str,
) -> std::collections::HashMap<String, (u64, u64)> {
    let mut totals = std::collections::HashMap::new();
    for entry in entries {
        let Some(row) = entry.row_id.as_deref() else {
            continue;
        };
        if is_probe_destination(&entry.dest, probe_host) {
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

/// Append-only samples + per-config traffic accumulation.
#[derive(Debug, Default)]
pub struct HealthStore {
    samples: Vec<HealthSample>,
    traffic: HashMap<String, TrafficStats>,
    /// Ticket 36: data-plane totals per config, mirrored from the store
    /// (persisted) after every clash poll. Present means the row is
    /// clash-tracked; absent rows keep the probe-byte display.
    data_traffic: HashMap<String, (u64, u64)>,
}

impl HealthStore {
    /// Ticket 28: one bounded push for [`record_outcome`] and [`hydrate`]
    /// (the ring cap is not per-caller policy).
    fn push_sample(&mut self, sample: HealthSample) {
        if self.samples.len() >= MAX_SAMPLES {
            self.samples.remove(0);
        }
        self.samples.push(sample);
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
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn latest(&self, config_id: &str) -> Option<&HealthSample> {
        self.latest_filtered(config_id, None)
    }

    /// Latest sample of one kind for one config.
    pub fn latest_of(&self, config_id: &str, kind: HealthKind) -> Option<&HealthSample> {
        self.latest_filtered(config_id, Some(kind))
    }

    fn latest_filtered(&self, config_id: &str, kind: Option<HealthKind>) -> Option<&HealthSample> {
        self.samples.iter().rev().find(|sample| {
            sample.config_id == config_id && kind.is_none_or(|kind| sample.kind == kind)
        })
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
            },
            None => RowHealth {
                state: RowState::Live,
                latency_ms: url
                    .and_then(|sample| sample.latency_ms)
                    .or_else(|| ping.and_then(|sample| sample.latency_ms)),
                cause: None,
            },
        }
    }

    /// Ticket 32: drop samples older than `max_age_secs` relative to `now`
    /// (stale samples expire instead of haunting a row forever).
    /// Future-dated samples (clock skew) are kept, never pruned.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn prune_stale(&mut self, now: i64, max_age_secs: i64) {
        self.samples
            .retain(|sample| now.saturating_sub(sample.at) <= max_age_secs);
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

    /// Ticket 33: probe-byte counts are labeled `probe:` — they measure
    /// health-probe request/response bytes through the row's own endpoint,
    /// never data-plane traffic (there is no data-plane meter in v1).
    /// Ticket 36: clash-tracked rows report data-plane totals instead; the
    /// probe count rides along as context. Untracked rows keep the v1
    /// probe display.
    fn traffic_text(&self, config_id: &str) -> String {
        let stats = self.traffic(config_id);
        if let Some((rx, tx)) = self.data_traffic(config_id) {
            if stats.probes == 0 {
                return format!("↑{} ↓{}", format_bytes(tx), format_bytes(rx));
            }
            return format!(
                "↑{} ↓{} · {} probe(s)",
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
#[cfg_attr(not(test), allow(dead_code))]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRecord {
    pub at: i64,
    pub config_id: Option<String>,
    pub level: LogLevel,
    pub message: String,
}

/// Redacted, tagged sidecar lines with a per-config filter.
#[derive(Debug, Default)]
pub struct LogStore {
    records: Vec<LogRecord>,
}

impl LogStore {
    /// Ingest one raw sidecar line: tag by config where possible, redact
    /// secrets, classify severity.
    pub fn push_raw(&mut self, line: &str, active: &[ProxyConfig]) {
        self.push(LogRecord {
            at: now_unix(),
            config_id: tag_for_line(line, active),
            level: parse_level(line),
            message: redact_secrets(line),
        });
    }

    fn push(&mut self, record: LogRecord) {
        if self.records.len() >= MAX_LOG_RECORDS {
            self.records.remove(0);
        }
        self.records.push(record);
    }

    /// Only lines tagged to this config (per-config filter).
    pub fn for_config(&self, config_id: &str) -> Vec<&LogRecord> {
        self.records
            .iter()
            .filter(|record| record.config_id.as_deref() == Some(config_id))
            .collect()
    }

    /// All stored lines, oldest first.
    pub fn all(&self) -> &[LogRecord] {
        &self.records
    }
}

/// Claim a sidecar line for a config when it names that config's endpoint:
/// either the `in-{id}`/`out-{id}` tag. Raw ids are deliberately NOT
/// substring-matched (numeric rowids like `1` would claim every line
/// containing the digit). Anything untagged stays engine-global (`None`).
pub fn tag_for_line(line: &str, active: &[ProxyConfig]) -> Option<String> {
    for config in active {
        if line.contains(&inbound_tag(&config.id)) || line.contains(&outbound_tag(&config.id)) {
            return Some(config.id.clone());
        }
    }
    None
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
        &mut self,
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
        &mut self,
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
        let (mut prober, handle) = FakeHealthProber::shared();
        let outcomes = test_all(&configs, DEFAULT_TEST_URL, &mut prober);
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
        let (mut prober, handle) = FakeHealthProber::shared();
        for (i, config) in configs.iter().enumerate() {
            handle.script_latency(&config.id, 20 + i as u64 * 10);
        }
        handle.script_failure("c3", "upstream refused");
        let outcomes = test_all(&configs, DEFAULT_TEST_URL, &mut prober);
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
        let (mut prober, handle) = FakeHealthProber::shared();
        handle.script_latency("1", 30);
        handle.script_latency("2", 40);
        let mut store = HealthStore::default();
        for outcome in test_all(&configs, DEFAULT_TEST_URL, &mut prober) {
            store.record_outcome(&outcome);
        }
        handle.script_failure("2", "connection reset");
        for outcome in test_all(&configs, DEFAULT_TEST_URL, &mut prober) {
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
        let (mut prober, handle) = FakeHealthProber::shared();
        handle.script_latency("1", 7);
        let outcomes = ping_all(&configs, &mut prober);
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
        let (mut prober, handle) = FakeHealthProber::shared();
        handle.deny_protocol("naive");
        let outcomes = test_all(&configs, DEFAULT_TEST_URL, &mut prober);
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
        let (mut prober, handle) = FakeHealthProber::shared();
        handle.deny_protocol("naive");
        let outcomes = test_all(&configs, DEFAULT_TEST_URL, &mut prober);
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
        let (mut prober, handle) = FakeHealthProber::shared();
        let outcomes = test_all(&configs, DEFAULT_TEST_URL, &mut prober);
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
        let (mut prober, handle) = FakeHealthProber::shared();
        let outcomes = test_all(&configs, DEFAULT_TEST_URL, &mut prober);
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
        let (mut prober, _) = FakeHealthProber::shared();
        let mut store = HealthStore::default();
        for outcome in test_all(&configs, DEFAULT_TEST_URL, &mut prober) {
            store.record_outcome(&outcome);
        }
        for outcome in ping_all(&configs, &mut prober) {
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
        let (mut prober, _) = FakeHealthProber::shared();
        let mut store = HealthStore::default();
        assert!(
            store.detail_line("1") == "health: not tested",
            "untested row names the state"
        );
        for outcome in test_all(&configs, DEFAULT_TEST_URL, &mut prober) {
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
        let (mut prober, _) = FakeHealthProber::shared();
        let mut store = HealthStore::default();
        for outcome in test_all(&configs, DEFAULT_TEST_URL, &mut prober) {
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
        logs.push_raw(
            "warn out-1: dial failed password=hunter2 uuid=11111111-2222-4333-8444-555555555555",
            &active,
        );
        logs.push_raw("error in-2: upstream reset", &active);
        logs.push_raw("info engine listening on 127.0.0.1:10801", &active);
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
}
