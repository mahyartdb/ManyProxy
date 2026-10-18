use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};

#[cfg(test)]
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::engine::{inbound_tag, outbound_tag};
use crate::store::{now_unix, HealthHistoryEntry, HealthSnapshot, ProxyConfig};

pub const DEFAULT_TEST_URL: &str = "http://www.gstatic.com/generate_204";

pub const GROUP_TEST_BATCH_SIZE: usize = 100;
pub const GROUP_TEST_POLL_INTERVAL: Duration = Duration::from_millis(200);
pub const GROUP_TEST_CONCURRENCY: usize = 10;
pub const URL_TEST_TIMEOUT: Duration = Duration::from_secs(3);
pub const SPEED_TEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Host of a test `url`, path/query trimmed. Empty when the URL has no host.
pub fn probe_host_for(url: &str) -> String {
    split_http_url(url)
        .map(|(_, host)| host.to_string())
        .unwrap_or_default()
}

/// The set of probe endpoints whose own traffic must not be counted as row
/// usage. Destinations match by host name, or by a resolved address once
/// [`ProbeExclusions::resolve_ips`] has run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProbeExclusions {
    pub hosts: Vec<String>,
    pub ips: Vec<std::net::IpAddr>,
}

impl ProbeExclusions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add the probe host named by `url`. Blank/invalid URLs and hosts already
    /// present are ignored. Returns true when a new host was added.
    pub fn push_url(&mut self, url: &str) -> bool {
        let host = normalize_probe_host(&probe_host_for(url));
        if host.is_empty() || self.hosts.contains(&host) {
            return false;
        }
        self.hosts.push(host);
        true
    }

    pub fn add_resolved_ip(&mut self, ip: std::net::IpAddr) {
        if !self.ips.contains(&ip) {
            self.ips.push(ip);
        }
    }

    /// Resolve every host to its addresses so entries carrying a bare IP are
    /// matched too. Blocking; call from a worker thread. Failed resolves add
    /// nothing, mirroring the default-host behavior.
    pub fn resolve_ips(&mut self) {
        let mut resolved = Vec::new();
        for host in &self.hosts {
            resolved.extend(resolve_probe_ips(host));
        }
        for ip in resolved {
            self.add_resolved_ip(ip);
        }
    }

    /// True when `dest` is one of the excluded probe endpoints.
    pub fn excludes(&self, dest: &str) -> bool {
        self.hosts
            .iter()
            .any(|host| is_probe_destination_with_ips(dest, host, &self.ips))
    }
}

const MAX_SAMPLES: usize = 10_000;
const MAX_LOG_RECORDS: usize = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthKind {
    UrlTest,
    TcpPing,
    SpeedTest,
}

impl HealthKind {
    pub fn as_str(self) -> &'static str {
        match self {
            HealthKind::UrlTest => "url-test",
            HealthKind::TcpPing => "tcp-ping",
            HealthKind::SpeedTest => "speed-test",
        }
    }

    /// Map a persisted kind string back to a kind. Unknown/legacy values stay
    /// url-test so old rows keep their meaning.
    pub fn from_persisted(kind: &str) -> HealthKind {
        match kind {
            "tcp-ping" => HealthKind::TcpPing,
            "speed-test" => HealthKind::SpeedTest,
            _ => HealthKind::UrlTest,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthSample {
    pub config_id: String,
    pub at: i64,
    pub kind: HealthKind,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProbeSuccess {
    pub latency_ms: u64,
    pub bytes_rx: u64,
    pub bytes_tx: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestOutcome {
    pub sample: HealthSample,
    pub success: Option<ProbeSuccess>,
}

pub trait HealthProber {
    fn supports(&self, config: &ProxyConfig) -> bool {
        let _ = config;
        true
    }

    fn url_test(
        &self,
        config: &ProxyConfig,
        bind: &str,
        port: u16,
        url: &str,
    ) -> Result<ProbeSuccess, String>;

    fn tcp_ping(&self, config: &ProxyConfig, bind: &str, port: u16)
        -> Result<ProbeSuccess, String>;

    fn speed_test(
        &self,
        config: &ProxyConfig,
        bind: &str,
        port: u16,
        url: &str,
        mode: SpeedtestMode,
        timeout: Duration,
    ) -> Result<SpeedtestOutcome, String> {
        let _ = (bind, port, url, mode, timeout);
        Err(format!(
            "speed test not supported for \"{}\" by this driver",
            redact_secrets(&config.name)
        ))
    }

    fn out_ip(
        &self,
        config: &ProxyConfig,
        bind: &str,
        port: u16,
        url: &str,
    ) -> Result<String, String> {
        let _ = (bind, port, url);
        Err(format!(
            "out-ip resolve not supported for \"{}\" by this driver",
            redact_secrets(&config.name)
        ))
    }
}

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
        Self::new(URL_TEST_TIMEOUT)
    }
}

fn endpoint_addr(bind: &str, port: u16, timeout: Duration) -> Result<SocketAddr, String> {
    let trimmed = bind.trim();
    let addr = if let Ok(ip) = trimmed.parse::<std::net::IpAddr>() {
        let host = if ip.is_unspecified() {
            if ip.is_ipv4() {
                "127.0.0.1".to_string()
            } else {
                "::1".to_string()
            }
        } else {
            trimmed.to_string()
        };

        let socket = if host.contains(':') {
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        };
        socket
            .parse::<SocketAddr>()
            .map_err(|_| format!("bad endpoint {host}:{port}"))?
    } else {
        resolve_endpoint_host(trimmed, port, timeout)
            .ok_or_else(|| format!("cannot resolve endpoint {trimmed}:{port}"))?
    };
    if !addr.ip().is_loopback() {
        return Err(format!(
            "refusing non-loopback probe endpoint {addr} (probes dial the local loopback endpoint only)"
        ));
    }
    Ok(addr)
}

fn resolve_endpoint_host(host: &str, port: u16, timeout: Duration) -> Option<SocketAddr> {
    let text = format!("{host}:{port}");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let first = text
            .to_socket_addrs()
            .ok()
            .and_then(|mut addrs| addrs.next());
        let _ = tx.send(first);
    });
    rx.recv_timeout(timeout).ok().flatten()
}

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
        let endpoint = endpoint_addr(bind, port, self.timeout)?;
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
        let read_started = Instant::now();
        loop {
            let elapsed = read_started.elapsed();
            if elapsed >= self.timeout {
                return Err(format!(
                    "url test via {endpoint}: timed out waiting for response headers"
                ));
            }
            stream.set_read_timeout(Some(self.timeout - elapsed)).ok();
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    if buf.len() >= 65_536 || find_headers_end(&buf).is_some() {
                        break;
                    }
                }
                Err(error) => {
                    if error.kind() == std::io::ErrorKind::TimedOut {
                        return Err(format!(
                            "url test via {endpoint}: timed out waiting for response headers"
                        ));
                    }
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

        if (200..300).contains(&code) {
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
        let endpoint = endpoint_addr(bind, port, self.timeout)?;
        let started = Instant::now();
        TcpStream::connect_timeout(&endpoint, self.timeout)
            .map_err(|error| format!("tcp ping {endpoint}: {error}"))?;
        Ok(ProbeSuccess {
            latency_ms: started.elapsed().as_millis() as u64,
            bytes_rx: 0,
            bytes_tx: 0,
        })
    }

    fn speed_test(
        &self,
        config: &ProxyConfig,
        bind: &str,
        port: u16,
        url: &str,
        mode: SpeedtestMode,
        timeout: Duration,
    ) -> Result<SpeedtestOutcome, String> {
        let outcome =
            |latency_ms: Option<u64>, mbps: Option<f64>, error: Option<String>| SpeedtestOutcome {
                config_id: config.id.clone(),
                mode,
                latency_ms,
                mbps,
                error,
            };
        let endpoint =
            endpoint_addr(bind, port, timeout).map_err(|cause| named_speed_error(config, cause))?;
        let (scheme, host) =
            split_http_url(url).map_err(|cause| named_speed_error(config, cause))?;
        if !scheme.eq_ignore_ascii_case("http") {
            return Err(named_speed_error(
                config,
                format!(
                    "unsupported url scheme \"{scheme}\" for speed test (use an http:// test url)"
                ),
            ));
        }
        match mode {
            SpeedtestMode::Download => {
                let request = format!(
                    "GET {url} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nUser-Agent: ManyProxy-health\r\n\r\n"
                );
                let started = Instant::now();
                match exchange(endpoint, &request, timeout, 8 * 1024 * 1024) {
                    Ok((code, body)) if (200..300).contains(&code) => Ok(outcome(
                        Some(started.elapsed().as_millis() as u64),
                        Some(mbps_of(body.len() as u64, started.elapsed())),
                        None,
                    )),
                    Ok((code, _)) => Err(named_speed_error(
                        config,
                        format!("speed test via {endpoint}: unexpected status {code}"),
                    )),
                    Err(cause) => Err(named_speed_error(config, cause)),
                }
            }
            SpeedtestMode::Upload => {
                let payload = vec![b'M'; 256 * 1024];
                let request = format!(
                    "POST {url} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nUser-Agent: ManyProxy-health\r\nContent-Length: {}\r\n\r\n",
                    payload.len()
                );
                let started = Instant::now();
                match post_exchange(endpoint, &request, &payload, timeout) {
                    Ok(code) if (200..300).contains(&code) => Ok(outcome(
                        Some(started.elapsed().as_millis() as u64),
                        Some(mbps_of(payload.len() as u64, started.elapsed())),
                        None,
                    )),
                    Ok(code) => Err(named_speed_error(
                        config,
                        format!("speed test via {endpoint}: unexpected status {code}"),
                    )),
                    Err(cause) => Err(named_speed_error(config, cause)),
                }
            }
        }
    }

    fn out_ip(
        &self,
        config: &ProxyConfig,
        bind: &str,
        port: u16,
        url: &str,
    ) -> Result<String, String> {
        let named = |cause: String| {
            format!(
                "out-ip resolve failed for \"{}\": {}",
                redact_secrets(&config.name),
                redact_secrets(&cause)
            )
        };
        let endpoint = endpoint_addr(bind, port, self.timeout).map_err(named)?;
        let (scheme, host) = split_http_url(url).map_err(named)?;
        if !scheme.eq_ignore_ascii_case("http") {
            return Err(named(format!(
                "unsupported url scheme \"{scheme}\" for out-ip resolve (use an http:// url)"
            )));
        }
        let request = format!(
            "GET {url} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nUser-Agent: ManyProxy-health\r\n\r\n"
        );
        let (_, body) = exchange(endpoint, &request, self.timeout, 4096).map_err(named)?;
        let text = String::from_utf8_lossy(&body);

        let candidate = text
            .split("\r\n\r\n")
            .nth(1)
            .unwrap_or(text.as_ref())
            .trim();
        candidate
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.to_string())
            .map_err(|_| named("unexpected out-ip response (not an IP address)".to_string()))
    }
}

fn named_speed_error(config: &ProxyConfig, cause: impl Into<String>) -> String {
    format!(
        "speed test failed for \"{}\": {}",
        redact_secrets(&config.name),
        redact_secrets(&cause.into())
    )
}

fn exchange(
    endpoint: SocketAddr,
    request: &str,
    timeout: Duration,
    cap: usize,
) -> Result<(u16, Vec<u8>), String> {
    use std::io::{Read as _, Write as _};
    let started = Instant::now();
    let mut stream = TcpStream::connect_timeout(&endpoint, timeout)
        .map_err(|error| format!("speed test via {endpoint}: dial failed ({error})"))?;
    stream.set_write_timeout(Some(timeout)).ok();
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("speed test via {endpoint}: write failed ({error})"))?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let elapsed = started.elapsed();
        if elapsed >= timeout {
            return Err(format!("speed test via {endpoint}: timed out"));
        }
        stream.set_read_timeout(Some(timeout - elapsed)).ok();
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() >= cap {
                    break;
                }
            }
            Err(error) => {
                if error.kind() == std::io::ErrorKind::TimedOut {
                    return Err(format!("speed test via {endpoint}: timed out"));
                }
                return Err(format!("speed test via {endpoint}: read failed ({error})"));
            }
        }
    }
    let head = String::from_utf8_lossy(&buf);
    let code = head
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| format!("speed test via {endpoint}: bad proxy response"))?;
    Ok((code, buf))
}

fn post_exchange(
    endpoint: SocketAddr,
    headers: &str,
    payload: &[u8],
    timeout: Duration,
) -> Result<u16, String> {
    use std::io::{Read as _, Write as _};
    let started = Instant::now();
    let mut stream = TcpStream::connect_timeout(&endpoint, timeout)
        .map_err(|error| format!("speed test via {endpoint}: dial failed ({error})"))?;
    stream.set_write_timeout(Some(timeout)).ok();
    stream
        .write_all(headers.as_bytes())
        .map_err(|error| format!("speed test via {endpoint}: write failed ({error})"))?;
    stream
        .write_all(payload)
        .map_err(|error| format!("speed test via {endpoint}: write failed ({error})"))?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let elapsed = started.elapsed();
        if elapsed >= timeout {
            return Err(format!("speed test via {endpoint}: timed out"));
        }
        stream.set_read_timeout(Some(timeout - elapsed)).ok();
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if find_headers_end(&buf).is_some() || buf.len() >= 65_536 {
                    break;
                }
            }
            Err(error) => {
                if error.kind() == std::io::ErrorKind::TimedOut {
                    return Err(format!("speed test via {endpoint}: timed out"));
                }
                return Err(format!("speed test via {endpoint}: read failed ({error})"));
            }
        }
    }
    String::from_utf8_lossy(&buf)
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| format!("speed test via {endpoint}: bad proxy response"))
}

fn find_headers_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|pos| pos + 4)
}

type ProbeFn = fn(&dyn HealthProber, &ProxyConfig, &str, u16, &str) -> Result<ProbeSuccess, String>;

pub const MAX_PROBE_CONCURRENCY: usize = GROUP_TEST_CONCURRENCY;

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
                    config.protocol,
                    redact_secrets(&config.name)
                )),
            },
            success: None,
        };
    }

    let probed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        probe(prober, config, &config.bind_ip, config.port, url)
    }));
    match probed {
        Ok(Ok(success)) => TestOutcome {
            sample: HealthSample {
                config_id: config.id.clone(),
                at,
                kind,
                latency_ms: Some(success.latency_ms),
                error: None,
            },
            success: Some(success),
        },
        Ok(Err(cause)) => TestOutcome {
            sample: HealthSample {
                config_id: config.id.clone(),
                at,
                kind,
                latency_ms: None,
                error: Some(format!(
                    "{verb} failed for \"{}\": {}",
                    redact_secrets(&config.name),
                    redact_secrets(&cause)
                )),
            },
            success: None,
        },
        Err(_) => TestOutcome {
            sample: HealthSample {
                config_id: config.id.clone(),
                at,
                kind,
                latency_ms: None,
                error: Some(format!(
                    "{verb} failed for \"{}\": probe panicked",
                    redact_secrets(&config.name)
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

fn run_all_concurrent(
    configs: &[ProxyConfig],
    prober: &(dyn HealthProber + Sync),
    kind: HealthKind,
    verb: &str,
    probe: ProbeFn,
    url: &str,
) -> Vec<TestOutcome> {
    run_all_concurrent_stoppable(configs, prober, kind, verb, probe, url, None)
}

fn run_all_concurrent_stoppable(
    configs: &[ProxyConfig],
    prober: &(dyn HealthProber + Sync),
    kind: HealthKind,
    verb: &str,
    probe: ProbeFn,
    url: &str,
    stop: Option<&std::sync::atomic::AtomicBool>,
) -> Vec<TestOutcome> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    if configs.is_empty() {
        return Vec::new();
    }
    if stop.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
        return Vec::new();
    }
    if configs.len() == 1 {
        return run_all(configs, prober, kind, verb, probe, url);
    }
    let next = AtomicUsize::new(0);
    let done = std::sync::Mutex::new(Vec::with_capacity(configs.len()));
    let workers = configs.len().clamp(1, MAX_PROBE_CONCURRENCY);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                if stop.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
                    break;
                }
                let index = next.fetch_add(1, Ordering::SeqCst);
                let Some(config) = configs.get(index) else {
                    break;
                };
                let outcome = run_one(config, prober, kind, verb, probe, url);

                done.lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .push((index, outcome));
            });
        }
    });
    let mut done = done
        .into_inner()
        .unwrap_or_else(|poison| poison.into_inner());
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

pub fn test_all(
    configs: &[ProxyConfig],
    url: &str,
    prober: &(dyn HealthProber + Sync),
) -> Vec<TestOutcome> {
    test_all_concurrent(configs, url, prober)
}

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

pub fn ping_all(configs: &[ProxyConfig], prober: &(dyn HealthProber + Sync)) -> Vec<TestOutcome> {
    ping_all_concurrent(configs, prober)
}

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

pub fn test_all_stoppable(
    configs: &[ProxyConfig],
    url: &str,
    prober: &(dyn HealthProber + Sync),
    stop: &std::sync::atomic::AtomicBool,
) -> Vec<TestOutcome> {
    run_all_concurrent_stoppable(
        configs,
        prober,
        HealthKind::UrlTest,
        "url test",
        url_probe,
        url,
        Some(stop),
    )
}

#[cfg(test)]
pub fn ping_all_stoppable(
    configs: &[ProxyConfig],
    prober: &(dyn HealthProber + Sync),
    stop: &std::sync::atomic::AtomicBool,
) -> Vec<TestOutcome> {
    run_all_concurrent_stoppable(
        configs,
        prober,
        HealthKind::TcpPing,
        "tcp ping",
        ping_probe,
        "",
        Some(stop),
    )
}

pub fn test_group(
    configs: &[ProxyConfig],
    url: &str,
    prober: &(dyn HealthProber + Sync),
    stop: &std::sync::atomic::AtomicBool,
    mut on_progress: impl FnMut(usize, usize),
) -> Vec<TestOutcome> {
    run_group(configs, stop, &mut on_progress, |chunk| {
        test_all_stoppable(chunk, url, prober, stop)
    })
}

#[cfg(test)]
pub fn ping_group(
    configs: &[ProxyConfig],
    prober: &(dyn HealthProber + Sync),
    stop: &std::sync::atomic::AtomicBool,
    mut on_progress: impl FnMut(usize, usize),
) -> Vec<TestOutcome> {
    run_group(configs, stop, &mut on_progress, |chunk| {
        ping_all_stoppable(chunk, prober, stop)
    })
}

fn run_group(
    configs: &[ProxyConfig],
    stop: &std::sync::atomic::AtomicBool,
    on_progress: &mut impl FnMut(usize, usize),
    mut run_chunk: impl FnMut(&[ProxyConfig]) -> Vec<TestOutcome>,
) -> Vec<TestOutcome> {
    use std::sync::atomic::Ordering;
    let total = configs.len();
    let mut out = Vec::with_capacity(total);
    for chunk in configs.chunks(GROUP_TEST_BATCH_SIZE) {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        out.extend(run_chunk(chunk));
        on_progress(out.len().min(total), total);
        if out.len() < total && !stop.load(Ordering::SeqCst) {
            std::thread::sleep(GROUP_TEST_POLL_INTERVAL);
        }
    }
    out
}

pub fn speed_group(
    configs: &[ProxyConfig],
    url: &str,
    mode: SpeedtestMode,
    timeout: Duration,
    prober: &dyn HealthProber,
    stop: &std::sync::atomic::AtomicBool,
    mut on_progress: impl FnMut(usize, usize),
) -> Vec<SpeedtestOutcome> {
    use std::sync::atomic::Ordering;
    let total = configs.len();
    let mut out = Vec::with_capacity(total);
    for config in configs {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        if !prober.supports(config) {
            out.push(SpeedtestOutcome {
                config_id: config.id.clone(),
                mode,
                latency_ms: None,
                mbps: None,
                error: Some(format!(
                    "unsupported protocol \"{}\" for \"{}\" (no speed test for this protocol yet)",
                    config.protocol,
                    redact_secrets(&config.name)
                )),
            });
        } else {
            let probed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                prober.speed_test(config, &config.bind_ip, config.port, url, mode, timeout)
            }));
            out.push(match probed {
                Ok(Ok(measured)) => measured,
                Ok(Err(cause)) => SpeedtestOutcome {
                    config_id: config.id.clone(),
                    mode,
                    latency_ms: None,
                    mbps: None,
                    error: Some(cause),
                },
                Err(_) => SpeedtestOutcome {
                    config_id: config.id.clone(),
                    mode,
                    latency_ms: None,
                    mbps: None,
                    error: Some(format!(
                        "speed test failed for \"{}\": probe panicked",
                        redact_secrets(&config.name)
                    )),
                },
            });
        }
        on_progress(out.len(), total);
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpeedtestMode {
    #[default]
    Download,
    Upload,
}

impl SpeedtestMode {
    pub fn as_str(self) -> &'static str {
        match self {
            SpeedtestMode::Download => "download",
            SpeedtestMode::Upload => "upload",
        }
    }

    #[cfg(test)]
    pub fn from_label(label: &str) -> Option<SpeedtestMode> {
        match label.trim().to_ascii_lowercase().as_str() {
            "download" => Some(SpeedtestMode::Download),
            "upload" => Some(SpeedtestMode::Upload),
            _ => None,
        }
    }

    pub fn next(self) -> SpeedtestMode {
        match self {
            SpeedtestMode::Download => SpeedtestMode::Upload,
            SpeedtestMode::Upload => SpeedtestMode::Download,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeedtestConfig {
    pub mode: SpeedtestMode,
    pub timeout: Duration,
}

impl Default for SpeedtestConfig {
    fn default() -> Self {
        Self {
            mode: SpeedtestMode::Download,
            timeout: SPEED_TEST_TIMEOUT,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpeedtestOutcome {
    pub config_id: String,
    pub mode: SpeedtestMode,
    pub latency_ms: Option<u64>,
    pub mbps: Option<f64>,
    pub error: Option<String>,
}

pub fn mbps_of(bytes: u64, elapsed: Duration) -> f64 {
    (bytes as f64 * 8.0) / elapsed.as_secs_f64().max(1e-9) / 1e6
}

pub fn format_speed_result(outcome: &SpeedtestOutcome) -> String {
    match (outcome.mbps, outcome.error.as_deref()) {
        (Some(mbps), _) => format!("{} {mbps:.1} Mbps", outcome.mode.as_str()),
        (None, Some(cause)) => cause.to_string(),
        (None, None) => "untested".to_string(),
    }
}

pub const RESOLVE_IP_URL: &str = "http://api.ipify.org";

pub const RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);

pub fn resolve_hostname(host: &str, timeout: Duration) -> Option<std::net::IpAddr> {
    resolve_endpoint_host(host.trim(), 80, timeout).map(|addr| addr.ip())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowState {
    Unknown,
    Live,
    Dead,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowHealth {
    pub state: RowState,
    pub latency_ms: Option<u64>,
    pub cause: Option<String>,
    pub cause_kind: Option<HealthCause>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrafficStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub probes: u64,
}

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

/// Render a whole-transfer duration for speed samples. Deliberately not
/// `{ms}ms`: a speed sample measures a download, not a latency.
pub fn format_transfer_duration(ms: u64) -> String {
    if ms >= 1_000 {
        format!("{:.1}s", ms as f64 / 1_000.0)
    } else {
        format!("{ms}ms")
    }
}

pub fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut kept: String = text.chars().take(limit).collect();
    kept.push('…');
    kept
}

pub fn sparkline_latencies(samples: &[(i64, Option<u64>)], limit: usize) -> Vec<u64> {
    let mut latencies: Vec<u64> = samples.iter().filter_map(|(_, latency)| *latency).collect();
    if latencies.len() > limit.max(1) {
        latencies = latencies.split_off(latencies.len() - limit.max(1));
    }
    latencies
}

/// Latency series for the url-test sparkline: the url-test and tcp-ping history
/// mixed as before, minus speed-test durations so they never masquerade as
/// latency.
pub fn url_test_sparkline(entries: &[HealthHistoryEntry], limit: usize) -> Vec<u64> {
    let samples: Vec<(i64, Option<u64>)> = entries
        .iter()
        .filter(|entry| entry.kind != HealthKind::SpeedTest.as_str())
        .map(|entry| (entry.at, entry.latency_ms))
        .collect();
    sparkline_latencies(&samples, limit)
}

/// Tone for the sparkline caption, decided by the newest persisted url/ping
/// entry: an error reads [`LatencyTone::Failed`], a latency picks its Throne
/// band, and an empty history stays neutral `Unknown`. Speed-test durations
/// are skipped exactly like [`url_test_sparkline`] skips them, so a download
/// time can never pick a latency band; ties on `at` keep the later entry.
pub fn sparkline_caption_tone(entries: &[HealthHistoryEntry]) -> LatencyTone {
    let mut newest: Option<&HealthHistoryEntry> = None;
    for entry in entries
        .iter()
        .filter(|entry| entry.kind != HealthKind::SpeedTest.as_str())
    {
        if newest.is_none_or(|current| entry.at >= current.at) {
            newest = Some(entry);
        }
    }
    match newest {
        Some(entry) if entry.error.is_some() => LatencyTone::Failed,
        Some(entry) => latency_tone(entry.latency_ms),
        None => LatencyTone::Unknown,
    }
}

#[cfg(test)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClashConn {
    pub id: String,
    pub row_id: Option<String>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,

    pub dest: String,
}

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

#[cfg(test)]
pub fn is_probe_destination(dest: &str, probe_host: &str) -> bool {
    is_probe_destination_with_ips(dest, probe_host, &[])
}

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

    if let Some(inner) = host.strip_prefix('[') {
        if let Some(end) = inner.find(']') {
            return inner[..end].trim_end_matches('.').to_string();
        }
        return inner.trim_end_matches('.').to_string();
    }

    if host.chars().filter(|c| *c == ':').count() >= 2 {
        return host.trim_end_matches('.').to_string();
    }

    if let Some((head, tail)) = host.rsplit_once(':') {
        if !tail.is_empty() && tail.bytes().all(|byte| byte.is_ascii_digit()) {
            host = head.to_string();
        }
    }
    host.trim_end_matches('.').to_string()
}

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

    if let Ok(dest_ip) = dest.parse::<std::net::IpAddr>() {
        if probe_ips.contains(&dest_ip) {
            return true;
        }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthCause {
    Dns,
    Local,
    Egress,
}

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

#[cfg(test)]
pub fn attribute_entries(
    entries: &[ClashConn],
    probe_host: &str,
) -> std::collections::HashMap<String, (u64, u64)> {
    attribute_entries_with_ips(entries, probe_host, &[])
}

#[cfg(test)]
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

#[cfg(test)]
pub fn poll_clash_totals(
    port: u16,
    secret: &str,
    timeout: std::time::Duration,
) -> Result<(u64, u64), String> {
    parse_clash_totals(&fetch_clash_body(port, secret, timeout)?)
}

pub fn poll_clash_entries(
    port: u16,
    secret: &str,
    timeout: std::time::Duration,
) -> Result<Vec<ClashConn>, String> {
    parse_clash_entries(&fetch_clash_body(port, secret, timeout)?)
}

pub const CLASH_PER_CORE_TIMEOUT_SECS: u64 = 2;

pub const MAX_TRAFFIC_SEEN_ENTRIES: usize = 20_000;

pub const TRAFFIC_SEEN_TTL_SECS: i64 = 600;

#[derive(Debug, Clone)]
pub struct TrafficSeenMap {
    inner: HashMap<String, (u64, u64, i64)>,
    cap: usize,
    ttl_secs: i64,
}

impl Default for TrafficSeenMap {
    fn default() -> Self {
        Self::bounded()
    }
}

impl TrafficSeenMap {
    pub fn bounded() -> Self {
        Self {
            inner: HashMap::new(),
            cap: MAX_TRAFFIC_SEEN_ENTRIES,
            ttl_secs: TRAFFIC_SEEN_TTL_SECS,
        }
    }

    pub fn get(&self, key: &str) -> Option<(u64, u64)> {
        self.inner.get(key).map(|(rx, tx, _)| (*rx, *tx))
    }

    pub fn insert(&mut self, key: String, rx: u64, tx: u64, now: i64) {
        self.inner.insert(key, (rx, tx, now));
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn core_of(id: &str) -> &str {
        id.split('/').next().unwrap_or("")
    }

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
                return !ok_cores.contains(core);
            }

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

pub const CLASH_PER_CORE_TIMEOUT: Duration = Duration::from_secs(CLASH_PER_CORE_TIMEOUT_SECS);

pub const CLASH_TOTAL_DEADLINE_SECS: u64 = 8;

pub const CLASH_TOTAL_DEADLINE: Duration = Duration::from_secs(CLASH_TOTAL_DEADLINE_SECS);

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

#[derive(Debug, Default)]
pub struct HealthStore {
    samples: VecDeque<HealthSample>,

    latest_any: HashMap<String, HealthSample>,
    latest_kind: HashMap<String, [Option<HealthSample>; 3]>,
    traffic: HashMap<String, TrafficStats>,

    data_traffic: HashMap<String, (u64, u64)>,

    stale: HashSet<String>,
}

impl HealthStore {
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
        self.latest_kind.entry(id).or_insert([None, None, None])[slot] = Some(sample);
    }

    fn kind_slot(kind: HealthKind) -> usize {
        match kind {
            HealthKind::UrlTest => 0,
            HealthKind::TcpPing => 1,
            HealthKind::SpeedTest => 2,
        }
    }

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

    fn rebuild_latest_index(&mut self) {
        self.latest_any.clear();
        self.latest_kind.clear();
        for sample in &self.samples {
            self.latest_any
                .insert(sample.config_id.clone(), sample.clone());
            self.latest_kind
                .entry(sample.config_id.clone())
                .or_insert([None, None, None])[Self::kind_slot(sample.kind)] = Some(sample.clone());
        }
    }

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

    pub fn hydrate(&mut self, snapshots: &[HealthSnapshot]) {
        for snapshot in snapshots {
            let kind = HealthKind::from_persisted(&snapshot.kind);
            self.push_sample(HealthSample {
                config_id: snapshot.config_id.clone(),
                at: snapshot.at,
                kind,
                latency_ms: snapshot.latency_ms,
                error: snapshot.error.as_ref().map(|cause| redact_secrets(cause)),
            });
        }
    }

    #[cfg(test)]
    pub fn latest(&self, config_id: &str) -> Option<&HealthSample> {
        self.latest_filtered(config_id, None)
    }

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

    #[cfg(test)]
    pub fn prune_stale(&mut self, now: i64, max_age_secs: i64) {
        self.samples
            .retain(|sample| now.saturating_sub(sample.at) <= max_age_secs);
        self.rebuild_latest_index();
    }

    pub fn traffic(&self, config_id: &str) -> TrafficStats {
        self.traffic.get(config_id).copied().unwrap_or_default()
    }

    pub fn set_data_traffic(&mut self, config_id: &str, rx_bytes: u64, tx_bytes: u64) {
        self.data_traffic
            .insert(config_id.to_string(), (rx_bytes, tx_bytes));
    }

    pub fn data_traffic(&self, config_id: &str) -> Option<(u64, u64)> {
        self.data_traffic.get(config_id).copied()
    }

    pub fn prune_configs(&mut self, ids: &HashSet<String>) {
        self.samples
            .retain(|sample| !ids.contains(&sample.config_id));
        self.traffic.retain(|id, _| !ids.contains(id));
        self.data_traffic.retain(|id, _| !ids.contains(id));
        self.stale.retain(|id| !ids.contains(id));
        self.rebuild_latest_index();
    }

    #[cfg(test)]
    pub fn mark_stale(&mut self, config_id: &str) {
        self.stale.insert(config_id.to_string());
    }

    pub fn mark_stale_all(&mut self, config_ids: &[String]) {
        self.stale.extend(config_ids.iter().cloned());
    }

    pub fn clear_stale(&mut self, config_id: &str) {
        self.stale.remove(config_id);
    }

    pub fn is_stale(&self, config_id: &str) -> bool {
        self.stale.contains(config_id)
    }

    fn traffic_text(&self, config_id: &str) -> String {
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

    pub fn row_summary(&self, config_id: &str) -> String {
        match self.status(config_id) {
            RowHealth {
                state: RowState::Unknown,
                ..
            } => self.speed_summary(config_id).unwrap_or_default(),
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

    pub fn detail_line(&self, config_id: &str) -> String {
        let url_ms = self
            .latest_of(config_id, HealthKind::UrlTest)
            .and_then(|sample| sample.latency_ms);
        let ping_ms = self
            .latest_of(config_id, HealthKind::TcpPing)
            .and_then(|sample| sample.latency_ms);
        let traffic = self.traffic_text(config_id);
        let speed = self.speed_summary(config_id);
        match self.status(config_id) {
            RowHealth {
                state: RowState::Unknown,
                ..
            } => "health: never tested — press Test to check".to_string(),
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
                if let Some(speed) = speed {
                    parts.push(format!("speed {speed}"));
                }
                format!("health: {} · {traffic}", parts.join(" · "))
            }
            RowHealth {
                state: RowState::Live,
                latency_ms: None,
                ..
            } => match speed {
                Some(speed) => format!("health: live · speed {speed} · {traffic}"),
                None => format!("health: live · {traffic}"),
            },
            RowHealth {
                state: RowState::Dead,
                cause: Some(cause),
                ..
            } => format!("health: {cause} · {traffic}"),
            RowHealth {
                state: RowState::Dead,
                cause: None,
                ..
            } => format!("health: dead · {traffic}"),
        }
    }

    /// Presentation-tone of the latest samples: a dead verdict (the newest
    /// url/ping sample carries an error, no live latency) reads
    /// [`LatencyTone::Failed`]; a live latency picks its Throne band via
    /// [`latency_tone`]; everything else — untested rows, speed-only
    /// summaries, live rows without a latency — stays neutral `Unknown`.
    /// Pure view of stored samples: no probing is scheduled and no
    /// classification changes.
    pub fn health_tone(&self, config_id: &str) -> LatencyTone {
        match self.status(config_id) {
            RowHealth {
                state: RowState::Dead,
                ..
            } => LatencyTone::Failed,
            RowHealth { latency_ms, .. } => latency_tone(latency_ms),
        }
    }

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

    pub fn record_speed_sample(&mut self, outcome: &SpeedtestOutcome, at: i64) {
        self.push_sample(HealthSample {
            config_id: outcome.config_id.clone(),
            at,
            kind: HealthKind::SpeedTest,
            latency_ms: outcome.latency_ms,
            error: outcome.error.clone(),
        });
    }

    /// Duration of the latest successful speed sample, rendered as a transfer
    /// duration (e.g. `2.7s`) rather than a latency. None without one.
    pub fn speed_summary(&self, config_id: &str) -> Option<String> {
        let sample = self.latest_of(config_id, HealthKind::SpeedTest)?;
        if sample.error.is_some() {
            return None;
        }
        sample.latency_ms.map(format_transfer_duration)
    }
}

/// Health readout cap for the tray Select-exit dialog's monitor section.
pub const BEST_ROWS_MONITOR_TOP_N: usize = 5;

/// Rank rows best-first by their latest health sample: live rows with a known
/// latency first, fastest first; live rows without a latency next; rows with
/// no url/ping sample after those; dead rows last. Equal ranks keep the
/// caller's order, so unknown/dead/no-sample bundles render in a stable,
/// deterministic order. A pure view of existing samples — nothing is probed,
/// re-sampled, or restarted, and group-test stop flags stay untouched.
pub fn best_rows_by_health<'a>(
    rows: &'a [ProxyConfig],
    health: &HealthStore,
) -> Vec<&'a ProxyConfig> {
    let mut ordered: Vec<&'a ProxyConfig> = rows.iter().collect();
    ordered.sort_by_key(|row| {
        let row_health = health.status(&row.id);
        match row_health.state {
            RowState::Live => (0u8, row_health.latency_ms.unwrap_or(u64::MAX)),
            RowState::Unknown => (1u8, 0u64),
            RowState::Dead => (2u8, 0u64),
        }
    });
    ordered
}

/// Read-only monitor lines for the tray Select-exit dialog, computed per
/// open: a `Best by health in {group}:` header plus the top five rows ranked
/// by [`best_rows_by_health`], rendered as `N) {name} ({row_summary})` with
/// `(stale)` appended the way row traffic text marks staleness. `None`
/// without rows — the caller hides the section, leaving no empty block.
/// Names and health summaries only, the data row text already shows; no
/// secrets, params, or URLs ever enter the strings.
pub fn best_rows_monitor_lines(
    group: &str,
    rows: &[ProxyConfig],
    health: &HealthStore,
) -> Option<(String, Vec<String>)> {
    if rows.is_empty() {
        return None;
    }
    let lines = best_rows_by_health(rows, health)
        .into_iter()
        .take(BEST_ROWS_MONITOR_TOP_N)
        .enumerate()
        .map(|(index, row)| {
            let stale = if health.is_stale(&row.id) {
                " (stale)"
            } else {
                ""
            };
            let summary = health.row_summary(&row.id).trim().to_string();
            let summary = if summary.is_empty() {
                "no probes this session".to_string()
            } else {
                summary
            };
            format!("{}) {} ({summary}){stale}", index + 1, row.name)
        })
        .collect();
    Some((format!("Best by health in {group}:"), lines))
}

/// Presentation-only latency band for probe readouts: Throne-style `<=100 ms`
/// green, `<=300 ms` yellow, past that red; a failed probe (latest sample
/// carries an error) reads darker than a plain poor latency; a row without a
/// probe stays neutral. Pure presentation — bands never gate probing,
/// pruning, re-sampling, or classification (`classify_health_cause`
/// untouched), and a slow-but-alive sample keeps its place in the series.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatencyTone {
    Unknown,
    Failed,
    Fast,
    Slow,
    Poor,
}

/// Map a probed latency onto the Throne §F.1 bands: `None` stays
/// [`LatencyTone::Unknown`]; the 100 ms and 300 ms boundaries are inclusive
/// (`<=100` green, `<=300` yellow, `>300` red).
pub fn latency_tone(latency_ms: Option<u64>) -> LatencyTone {
    match latency_ms {
        None => LatencyTone::Unknown,
        Some(ms) if ms <= 100 => LatencyTone::Fast,
        Some(ms) if ms <= 300 => LatencyTone::Slow,
        Some(_) => LatencyTone::Poor,
    }
}

#[cfg(test)]
pub fn test_result_string(outcome: &TestOutcome) -> String {
    match (outcome.sample.latency_ms, outcome.sample.error.as_deref()) {
        (Some(ms), _) => format!("{ms}ms"),
        (None, Some(cause)) => cause.to_string(),
        (None, None) => "untested".to_string(),
    }
}

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

fn has_word(lower: &str, word: &str) -> bool {
    lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|token| token == word)
}

fn has_any_word(lower: &str, words: &[&str]) -> bool {
    words.iter().any(|word| has_word(lower, word))
}

fn parse_level(line: &str) -> LogLevel {
    let lower = line.to_ascii_lowercase();
    if has_any_word(&lower, &["error", "errors", "fatal", "panic", "critical"]) {
        LogLevel::Error
    } else if has_any_word(&lower, &["warn", "warning"]) {
        LogLevel::Warn
    } else if has_any_word(&lower, &["debug", "trace"]) {
        LogLevel::Debug
    } else {
        LogLevel::Info
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRecord {
    pub at: i64,
    pub config_id: Option<String>,
    pub level: LogLevel,
    pub message: String,
    pub search: String,
}

#[derive(Debug, Clone, Default)]
pub struct LogTagIndex {
    tags: Vec<(String, String)>,
}

impl LogTagIndex {
    pub fn build(active: &[ProxyConfig]) -> Self {
        let mut tags = Vec::with_capacity(active.len() * 2);
        for config in active {
            tags.push((inbound_tag(&config.id), config.id.clone()));
            tags.push((outbound_tag(&config.id), config.id.clone()));
        }
        Self { tags }
    }

    pub fn tag_for(&self, line: &str) -> Option<String> {
        for (tag, id) in &self.tags {
            if tag_matches_bounded(line, tag) {
                return Some(id.clone());
            }
        }
        None
    }
}

fn is_tag_cont_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

fn tag_matches_bounded(line: &str, tag: &str) -> bool {
    if tag.is_empty() || tag.len() > line.len() {
        return false;
    }
    let mut start = 0;
    while start + tag.len() <= line.len() {
        let Some(pos) = line[start..].find(tag) else {
            return false;
        };
        let found = start + pos;
        let end = found + tag.len();
        let prev_ok = line[..found]
            .chars()
            .next_back()
            .is_none_or(|c| !is_tag_cont_char(c));
        let next_ok = line[end..]
            .chars()
            .next()
            .is_none_or(|c| !is_tag_cont_char(c));
        if prev_ok && next_ok {
            return true;
        }
        start = found + 1;
    }
    false
}

#[derive(Debug, Default)]
pub struct LogStore {
    records: VecDeque<LogRecord>,
    push_seq: u64,
}

impl LogStore {
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
        self.push_seq = self.push_seq.wrapping_add(1);
        if self.records.len() >= MAX_LOG_RECORDS {
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

    /// Monotonic count of records pushed, so a tail view can notice new output
    /// even when the capped ring keeps `all().len()` pinned at `MAX_LOG_RECORDS`.
    pub fn push_seq(&self) -> u64 {
        self.push_seq
    }

    pub fn for_config(&self, config_id: &str) -> Vec<&LogRecord> {
        self.records
            .iter()
            .filter(|record| record.config_id.as_deref() == Some(config_id))
            .collect()
    }

    pub fn all(&self) -> &VecDeque<LogRecord> {
        &self.records
    }
}

fn is_crash_cause_line(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    has_any_word(
        &lower,
        &["panic", "panicked", "fatal", "crash", "crashed", "crashes"],
    )
}

#[cfg(test)]
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
    "private-key",
    "apikey",
    "api_key",
    "api-key",
    "auth",
    "authorization",
    "bearer",
    "basic",
    "access_token",
    "access-token",
    "refresh_token",
    "refresh-token",
    "cookie",
    "set-cookie",
    "set_cookie",
    "client_secret",
    "clientsecret",
    "client-secret",
    "psk",
    "key",
];

fn is_secret_key(token: &str) -> bool {
    let clean = token
        .trim_matches(|c| c == '"' || c == '\'')
        .to_ascii_lowercase();
    if SECRET_KEYS.contains(&clean.as_str()) {
        return true;
    }
    [
        "_key",
        "_token",
        "_secret",
        "_password",
        "_passwd",
        "-key",
        "-token",
        "-secret",
        "-password",
        "-passwd",
    ]
    .iter()
    .any(|suffix| clean.ends_with(suffix))
}

fn is_uuid_like(token: &str) -> bool {
    let bytes = token.as_bytes();
    if bytes.len() == 36
        && bytes[8] == b'-'
        && bytes[14] == b'-'
        && bytes[18] == b'-'
        && bytes[23] == b'-'
    {
        return bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| [8, 14, 18, 23].contains(&index) || byte.is_ascii_hexdigit());
    }
    bytes.len() == 32 && bytes.iter().all(|byte| byte.is_ascii_hexdigit())
}

fn is_bare_secret(token: &str) -> bool {
    if is_uuid_like(token) {
        return true;
    }
    if token.len() < 20 {
        return false;
    }
    token
        .bytes()
        .any(|byte| byte.is_ascii_digit() || byte == b'-' || byte == b'_')
        || token.len() >= 32
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

fn is_secret_value_end(c: char) -> bool {
    c != ' ' && c != '\t' && is_value_end(c)
}

fn is_cred_local_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '%' | '+')
}

fn is_cred_terminator(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '"' | '\'' | ',' | ';' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | '&'
        )
}

/// End index of a scheme-less `user:pass@host` credential starting at `start`, or `None`.
fn scheme_less_credential_end(chars: &[char], start: usize) -> Option<usize> {
    let mut separator = None;
    for (offset, c) in chars[start..].iter().enumerate() {
        let index = start + offset;
        match *c {
            '@' => {
                let separator = separator?;
                if separator == start || index == separator + 1 {
                    return None;
                }
                if separator + 2 < chars.len()
                    && chars[separator + 1] == '/'
                    && chars[separator + 2] == '/'
                {
                    return None;
                }
                return matches!(chars.get(index + 1), Some(c) if c.is_ascii_alphanumeric() || *c == '[')
                    .then_some(index);
            }
            ':' if separator.is_none() => separator = Some(index),
            c if is_cred_terminator(c) => return None,
            _ => {}
        }
    }
    None
}

fn starts_auth_scheme(chars: &[char], start: usize) -> bool {
    if start >= chars.len() {
        return false;
    }
    let end = chars[start..]
        .iter()
        .position(|c| !is_token_char(*c))
        .map_or(chars.len(), |offset| start + offset);
    let word: String = chars[start..end].iter().collect();
    matches!(word.to_ascii_lowercase().as_str(), "bearer" | "basic")
}

pub fn redact_secrets(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < chars.len() {
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
            let at = chars[j..end]
                .iter()
                .rposition(|c| *c == '@')
                .map(|position| j + position);
            if let Some(at) = at {
                let userinfo: String = chars[j..at].iter().collect();
                let shaped = userinfo.contains(':')
                    || (!userinfo.chars().any(|c| matches!(c, '/' | '?' | '#'))
                        && is_bare_secret(&userinfo));
                if shaped {
                    out.push_str("***@");
                    i = at + 1;
                    continue;
                }
            }

            i += 3;
            continue;
        }
        if is_cred_local_char(chars[i]) && (i == 0 || !is_cred_local_char(chars[i - 1])) {
            if let Some(at) = scheme_less_credential_end(&chars, i) {
                out.push_str("***@");
                i = at + 1;
                continue;
            }
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
                    while i <= k {
                        out.push(chars[i]);
                        i += 1;
                    }
                    j = i;
                    while j < chars.len()
                        && (chars[j] == ' ' || chars[j] == '"' || chars[j] == '\'')
                    {
                        out.push(chars[j]);
                        j += 1;
                    }
                    out.push_str("***");
                    let mut probe = j;
                    while probe < chars.len() && (chars[probe] == ' ' || chars[probe] == '\t') {
                        probe += 1;
                    }
                    if starts_auth_scheme(&chars, probe) {
                        j = probe;
                        while j < chars.len() && is_token_char(chars[j]) {
                            j += 1;
                        }
                        while j < chars.len() && !is_secret_value_end(chars[j]) {
                            j += 1;
                        }
                    } else {
                        while j < chars.len() && !is_value_end(chars[j]) {
                            j += 1;
                        }
                    }

                    if j < chars.len() && (chars[j] == '"' || chars[j] == '\'') {
                        out.push(chars[j]);
                        j += 1;
                    }
                    i = j;
                    continue;
                }
                if bearer && k < chars.len() && !is_value_end(chars[k]) {
                    while i < k {
                        out.push(chars[i]);
                        i += 1;
                    }
                    j = i;
                    out.push_str("***");
                    while j < chars.len() && !is_value_end(chars[j]) {
                        j += 1;
                    }
                    i = j;
                    continue;
                }
            }

            if is_bare_secret(&token) {
                out.push_str("***");
                i = j;
                continue;
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
    speed_mbps: HashMap<String, f64>,
    out_ips: HashMap<String, String>,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeCall {
    pub config_id: String,
    pub kind: HealthKind,
    pub bind: String,
    pub port: u16,
    pub url: String,
}

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

    pub fn script_speed(&self, config_id: &str, mbps: f64) {
        self.state
            .lock()
            .unwrap()
            .speed_mbps
            .insert(config_id.to_string(), mbps);
    }

    pub fn script_out_ip(&self, config_id: &str, ip: &str) {
        self.state
            .lock()
            .unwrap()
            .out_ips
            .insert(config_id.to_string(), ip.to_string());
    }
}

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

    fn speed_test(
        &self,
        config: &ProxyConfig,
        _bind: &str,
        _port: u16,
        _url: &str,
        mode: SpeedtestMode,
        _timeout: Duration,
    ) -> Result<SpeedtestOutcome, String> {
        let state = self.state.lock().unwrap();
        if let Some(cause) = state.failures.get(&config.id) {
            return Err(cause.clone());
        }
        let latency = state
            .latency
            .get(&config.id)
            .copied()
            .unwrap_or(self.default_latency);
        let mbps = state.speed_mbps.get(&config.id).copied().unwrap_or(10.0);
        Ok(SpeedtestOutcome {
            config_id: config.id.clone(),
            mode,
            latency_ms: Some(latency),
            mbps: Some(mbps),
            error: None,
        })
    }

    fn out_ip(
        &self,
        config: &ProxyConfig,
        _bind: &str,
        _port: u16,
        _url: &str,
    ) -> Result<String, String> {
        let state = self.state.lock().unwrap();
        if let Some(cause) = state.failures.get(&config.id) {
            return Err(cause.clone());
        }
        Ok(state
            .out_ips
            .get(&config.id)
            .cloned()
            .unwrap_or_else(|| "203.0.113.7".to_string()))
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

    #[test]
    fn t09_group_values_pinned() {
        assert_eq!(GROUP_TEST_BATCH_SIZE, 100, "batch pinned");
        assert_eq!(
            GROUP_TEST_POLL_INTERVAL,
            std::time::Duration::from_millis(200),
            "poll pinned"
        );
        assert_eq!(GROUP_TEST_CONCURRENCY, 10, "concurrency pinned");
        assert_eq!(MAX_PROBE_CONCURRENCY, 10, "runner matches");
        assert_eq!(URL_TEST_TIMEOUT, std::time::Duration::from_secs(3));
        assert_eq!(SPEED_TEST_TIMEOUT, std::time::Duration::from_secs(5));
    }

    #[test]
    fn t65_latency_bands_100_300_matrix() {
        assert_eq!(
            latency_tone(None),
            LatencyTone::Unknown,
            "no latency stays neutral"
        );
        assert_eq!(latency_tone(Some(0)), LatencyTone::Fast);
        assert_eq!(latency_tone(Some(99)), LatencyTone::Fast);
        assert_eq!(
            latency_tone(Some(100)),
            LatencyTone::Fast,
            "100 boundary inclusive"
        );
        assert_eq!(
            latency_tone(Some(101)),
            LatencyTone::Slow,
            "one past 100 turns yellow"
        );
        assert_eq!(latency_tone(Some(299)), LatencyTone::Slow);
        assert_eq!(
            latency_tone(Some(300)),
            LatencyTone::Slow,
            "300 boundary inclusive"
        );
        assert_eq!(
            latency_tone(Some(301)),
            LatencyTone::Poor,
            "one past 300 turns red"
        );
        assert_eq!(latency_tone(Some(u64::MAX)), LatencyTone::Poor);

        let mut store = HealthStore::default();
        let url_sample = |at: i64, latency: Option<u64>, error: Option<&str>| HealthSample {
            config_id: "row".to_string(),
            at,
            kind: HealthKind::UrlTest,
            latency_ms: latency,
            error: error.map(|cause| cause.to_string()),
        };
        let record = |store: &mut HealthStore, sample: HealthSample| {
            store.record_outcome(&TestOutcome {
                sample,
                success: None,
            });
        };

        record(&mut store, url_sample(1, Some(250), None));
        assert_eq!(
            store.health_tone("row"),
            LatencyTone::Slow,
            "250 ms sits in yellow"
        );

        record(&mut store, url_sample(2, Some(100), None));
        assert_eq!(
            store.health_tone("row"),
            LatencyTone::Fast,
            "boundary stays green"
        );

        record(&mut store, url_sample(3, Some(301), None));
        assert_eq!(store.health_tone("row"), LatencyTone::Poor, "301 turns red");

        assert_eq!(
            store.health_tone("never-probed"),
            LatencyTone::Unknown,
            "an unprobed row keeps the neutral tone"
        );

        record(&mut store, url_sample(4, None, Some("upstream refused")));
        assert_eq!(
            store.health_tone("row"),
            LatencyTone::Failed,
            "an error-only sample reads Failed, not a latency band"
        );

        record(
            &mut store,
            url_sample(5, Some(90), Some("upstream refused")),
        );
        assert_eq!(
            store.health_tone("row"),
            LatencyTone::Failed,
            "the error wins over a latency in the same sample"
        );

        record(&mut store, url_sample(6, None, None));
        assert_eq!(store.status("row").state, RowState::Live);
        assert_eq!(
            store.health_tone("row"),
            LatencyTone::Unknown,
            "live without a latency stays neutral"
        );

        let speedy = TestOutcome {
            sample: HealthSample {
                config_id: "speedy".to_string(),
                at: 1,
                kind: HealthKind::SpeedTest,
                latency_ms: Some(2700),
                error: Some("dial refused".to_string()),
            },
            success: None,
        };
        store.record_outcome(&speedy);
        assert_eq!(
            store.status("speedy").state,
            RowState::Unknown,
            "a speed sample never classifies the row"
        );
        assert_eq!(
            store.health_tone("speedy"),
            LatencyTone::Unknown,
            "a failed speed summary stays distinct from a failed probe"
        );
        assert_eq!(
            store.health_tone("row"),
            LatencyTone::Unknown,
            "the url row keeps its verdict beside the speed sample"
        );

        let caption = |at: i64, latency: Option<u64>, error: Option<&str>| HealthHistoryEntry {
            config_id: "row".to_string(),
            at,
            kind: "url-test".to_string(),
            latency_ms: latency,
            error: error.map(|cause| cause.to_string()),
        };
        assert_eq!(sparkline_caption_tone(&[]), LatencyTone::Unknown);
        assert_eq!(
            sparkline_caption_tone(&[caption(1, Some(300), None)]),
            LatencyTone::Slow,
            "the caption bands by its newest latency"
        );
        assert_eq!(
            sparkline_caption_tone(&[
                caption(1, Some(90), None),
                caption(2, None, Some("port in use")),
            ]),
            LatencyTone::Failed,
            "a failed newest entry darkens the caption"
        );
        assert_eq!(
            sparkline_caption_tone(&[
                caption(1, None, Some("port in use")),
                caption(2, Some(90), None),
            ]),
            LatencyTone::Fast,
            "a later success clears the failed tone"
        );
        let duration_caption = HealthHistoryEntry {
            config_id: "row".to_string(),
            at: 2,
            kind: "speed-test".to_string(),
            latency_ms: Some(5000),
            error: None,
        };
        assert_eq!(
            sparkline_caption_tone(&[caption(1, Some(90), None), duration_caption]),
            LatencyTone::Fast,
            "speed durations never pick a latency band for the caption"
        );
    }

    #[test]
    fn t09_test_group_chunks_with_progress() {
        let configs: Vec<ProxyConfig> = (0..205)
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
        let (prober, handle) = FakeHealthProber::shared();
        handle.script_failure("7", "upstream refused");
        let stop = std::sync::atomic::AtomicBool::new(false);
        let mut progress = Vec::new();
        let outcomes = test_group(&configs, DEFAULT_TEST_URL, &prober, &stop, |done, total| {
            progress.push((done, total));
        });
        assert_eq!(outcomes.len(), 205, "one outcome per row");
        assert_eq!(
            progress,
            vec![(100, 205), (200, 205), (205, 205)],
            "per-chunk progress"
        );
        for outcome in &outcomes {
            let text = test_result_string(outcome);
            if outcome.sample.config_id == "7" {
                assert!(outcome.sample.latency_ms.is_none());
                assert!(text.contains("upstream refused"), "named failure: {text}");
            } else {
                assert_eq!(text, "42ms", "latency string: {text}");
            }
        }

        assert_eq!(latency_tone(None), LatencyTone::Unknown);
        assert_eq!(latency_tone(Some(33)), LatencyTone::Fast);
        assert_eq!(latency_tone(Some(299)), LatencyTone::Slow);
        assert_eq!(latency_tone(Some(500)), LatencyTone::Poor);
        assert_eq!(latency_tone(Some(1499)), LatencyTone::Poor);
    }

    #[test]
    fn t09_test_group_stop_aborts() {
        use std::sync::atomic::Ordering;
        use std::sync::Arc;

        let configs: Vec<ProxyConfig> = (0..50)
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

        let stopped = std::sync::atomic::AtomicBool::new(true);
        let (prober, handle) = FakeHealthProber::shared();
        let mut progress = Vec::new();
        let outcomes = test_group(
            &configs,
            DEFAULT_TEST_URL,
            &prober,
            &stopped,
            |done, total| {
                progress.push((done, total));
            },
        );
        assert!(outcomes.is_empty(), "stopped run records nothing");
        assert!(progress.is_empty(), "no progress on abort");
        for config in &configs {
            assert!(
                handle.calls_for(&config.id).is_empty(),
                "no pulls after stop"
            );
        }

        struct StopOnFirst {
            stop: Arc<std::sync::atomic::AtomicBool>,
        }
        impl HealthProber for StopOnFirst {
            fn url_test(
                &self,
                _config: &ProxyConfig,
                _bind: &str,
                _port: u16,
                _url: &str,
            ) -> Result<ProbeSuccess, String> {
                self.stop.store(true, Ordering::SeqCst);
                Ok(ProbeSuccess {
                    latency_ms: 9,
                    bytes_rx: 0,
                    bytes_tx: 0,
                })
            }
            fn tcp_ping(
                &self,
                _config: &ProxyConfig,
                _bind: &str,
                _port: u16,
            ) -> Result<ProbeSuccess, String> {
                unreachable!("url path only");
            }
        }
        let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let prober = StopOnFirst { stop: flag.clone() };
        let outcomes = test_group(&configs, DEFAULT_TEST_URL, &prober, &flag, |_, _| {});
        assert!(!outcomes.is_empty(), "in-flight rows still record");
        assert!(
            outcomes.len() < configs.len(),
            "stop aborts the run ({} of {})",
            outcomes.len(),
            configs.len()
        );
        for outcome in &outcomes {
            assert_eq!(test_result_string(outcome), "9ms");
        }
    }

    #[test]
    fn t09_speed_runs_never_credit_probe_bytes() {
        let config = fixture("1", "Alpha", "vless", "127.0.0.1", 10801);
        let (prober, handle) = FakeHealthProber::shared();
        handle.script_speed("1", 24.5);
        let outcome = prober
            .speed_test(
                &config,
                "127.0.0.1",
                10801,
                DEFAULT_TEST_URL,
                SpeedtestMode::Download,
                SPEED_TEST_TIMEOUT,
            )
            .expect("fake measures");
        assert_eq!(outcome.mbps, Some(24.5));
        assert_eq!(format_speed_result(&outcome), "download 24.5 Mbps");
        let mut store = HealthStore::default();
        store.record_speed_sample(&outcome, 1_700_000_000);
        assert_eq!(
            store.traffic("1"),
            TrafficStats::default(),
            "no usage credit"
        );
        assert_eq!(
            store
                .latest_of("1", HealthKind::SpeedTest)
                .and_then(|sample| sample.latency_ms),
            Some(42),
            "speed duration kept in its own slot"
        );
        assert_eq!(
            store.status("1").state,
            RowState::Unknown,
            "a speed sample never sets row health"
        );
        assert!(
            store.latest_of("1", HealthKind::UrlTest).is_none(),
            "the url-test slot stays empty"
        );

        assert_eq!(
            SpeedtestMode::from_label("upload"),
            Some(SpeedtestMode::Upload)
        );
        assert_eq!(SpeedtestMode::from_label("bogus"), None);
        assert_eq!(SpeedtestMode::Download.next(), SpeedtestMode::Upload);
        let failed = SpeedtestOutcome {
            config_id: "1".to_string(),
            mode: SpeedtestMode::Upload,
            latency_ms: None,
            mbps: None,
            error: Some("dial refused".to_string()),
        };
        assert_eq!(format_speed_result(&failed), "dial refused");

        let probe = prober
            .url_test(&config, "127.0.0.1", 10801, DEFAULT_TEST_URL)
            .expect("fake probes");
        store.record_outcome(&TestOutcome {
            sample: HealthSample {
                config_id: "1".to_string(),
                at: 1_700_000_001,
                kind: HealthKind::UrlTest,
                latency_ms: Some(probe.latency_ms),
                error: None,
            },
            success: Some(probe),
        });
        assert_ne!(
            store.traffic("1"),
            TrafficStats::default(),
            "url probes credit"
        );

        assert!((mbps_of(1_250_000, std::time::Duration::from_secs(1)) - 10.0).abs() < 1e-9);
        assert_eq!(mbps_of(100, std::time::Duration::ZERO), 800.0 / 1e6 * 1e9);
    }

    #[test]
    fn t09_speed_group_measures_and_stops() {
        let configs = vec![
            fixture("1", "Alpha", "vless", "127.0.0.1", 10801),
            fixture("2", "Beta", "trojan", "127.0.0.1", 10802),
        ];
        let (prober, handle) = FakeHealthProber::shared();
        handle.script_speed("2", 50.0);
        handle.deny_protocol("vless");
        let stop = std::sync::atomic::AtomicBool::new(false);
        let mut progress = Vec::new();
        let outcomes = speed_group(
            &configs,
            DEFAULT_TEST_URL,
            SpeedtestMode::Download,
            SPEED_TEST_TIMEOUT,
            &prober,
            &stop,
            |done, total| progress.push((done, total)),
        );
        assert_eq!(outcomes.len(), 2, "denied rows record named skips");
        assert!(outcomes[0].error.as_deref().unwrap_or("").contains("vless"));
        assert_eq!(outcomes[1].mbps, Some(50.0));
        assert_eq!(progress, vec![(1, 2), (2, 2)], "per-row progress");
        let stopped = std::sync::atomic::AtomicBool::new(true);
        let outcomes = speed_group(
            &configs,
            DEFAULT_TEST_URL,
            SpeedtestMode::Download,
            SPEED_TEST_TIMEOUT,
            &prober,
            &stopped,
            |_, _| {},
        );
        assert!(outcomes.is_empty(), "stopped speed run records nothing");
    }

    #[test]
    fn t09_ping_group_chunks_and_stops() {
        let configs: Vec<ProxyConfig> = (0..105)
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
        let (prober, _) = FakeHealthProber::shared();
        let stop = std::sync::atomic::AtomicBool::new(false);
        let mut progress = Vec::new();
        let outcomes = ping_group(&configs, &prober, &stop, |done, total| {
            progress.push((done, total));
        });
        assert_eq!(outcomes.len(), 105);
        assert_eq!(progress, vec![(100, 105), (105, 105)]);
        let stopped = std::sync::atomic::AtomicBool::new(true);
        let outcomes = ping_group(&configs, &prober, &stopped, |_, _| {});
        assert!(outcomes.is_empty(), "stopped ping run records nothing");
    }

    #[test]
    fn t09_prune_configs_drops_session_state() {
        let configs = vec![
            fixture("1", "Alpha", "vless", "127.0.0.1", 10801),
            fixture("2", "Beta", "vless", "127.0.0.1", 10802),
        ];
        let (prober, _) = FakeHealthProber::shared();
        let mut store = HealthStore::default();
        for outcome in test_all(&configs, DEFAULT_TEST_URL, &prober) {
            store.record_outcome(&outcome);
        }
        assert!(store.latest("1").is_some());
        assert_ne!(store.traffic("1"), TrafficStats::default());
        store.prune_configs(&["1".to_string()].into_iter().collect());
        assert!(store.latest("1").is_none(), "samples pruned");
        assert_eq!(
            store.traffic("1"),
            TrafficStats::default(),
            "traffic pruned"
        );
        assert!(store.latest("2").is_some(), "other rows kept");
    }

    #[test]
    fn t09_out_ip_scripts_per_row() {
        let config = fixture("1", "Alpha", "vless", "127.0.0.1", 10801);
        let (prober, handle) = FakeHealthProber::shared();
        assert_eq!(
            prober.out_ip(&config, "127.0.0.1", 10801, RESOLVE_IP_URL),
            Ok("203.0.113.7".to_string()),
            "documented default"
        );
        handle.script_out_ip("1", "198.51.100.9");
        assert_eq!(
            prober.out_ip(&config, "127.0.0.1", 10801, RESOLVE_IP_URL),
            Ok("198.51.100.9".to_string())
        );
        handle.script_failure("1", "egress down");
        assert_eq!(
            prober.out_ip(&config, "127.0.0.1", 10801, RESOLVE_IP_URL),
            Err("egress down".to_string())
        );
    }

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

        let mut live = HealthStore::default();
        live.record_outcome(&outcome("1", HealthKind::TcpPing, Some(7), None));
        live.record_outcome(&outcome("1", HealthKind::UrlTest, Some(42), None));
        let status = live.status("1");
        assert!(matches!(status.state, RowState::Live));
        assert_eq!(status.latency_ms, Some(42));
    }

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
            HealthSnapshot {
                config_id: "4".to_string(),
                at: 1_700_000_003,
                kind: "speed-test".to_string(),
                latency_ms: Some(2700),
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

        assert_eq!(
            store.latest("4").map(|sample| sample.kind),
            Some(HealthKind::SpeedTest),
            "a persisted speed-test kind hydrates into its own slot"
        );
        assert_eq!(store.status("4").state, RowState::Unknown);
        assert_eq!(
            store.row_summary("4"),
            "2.7s",
            "the health column shows the transfer duration, never a latency"
        );
        assert_eq!(store.speed_summary("4").as_deref(), Some("2.7s"));

        assert_eq!(store.traffic("1"), TrafficStats::default());
        assert_eq!(store.status("missing").state, RowState::Unknown);
    }

    #[test]
    fn tcp_ping_records_timing_per_endpoint() {
        assert_eq!(HealthKind::UrlTest.as_str(), "url-test");
        assert_eq!(HealthKind::TcpPing.as_str(), "tcp-ping");
        assert_eq!(HealthKind::SpeedTest.as_str(), "speed-test");
        assert_eq!(
            HealthKind::from_persisted("speed-test"),
            HealthKind::SpeedTest
        );
        assert_eq!(HealthKind::from_persisted("mystery"), HealthKind::UrlTest);
        let configs = vec![fixture("1", "Alpha", "vless", "127.0.0.1", 10801)];
        let (prober, handle) = FakeHealthProber::shared();
        handle.script_latency("1", 7);
        let outcomes = ping_all(&configs, &prober);
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].sample.kind, HealthKind::TcpPing);
        assert_eq!(outcomes[0].sample.latency_ms, Some(7));
        assert_eq!(handle.calls_for("1")[0].port, 10801);
    }

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
        assert_eq!(
            store.detail_line("missing"),
            "health: never tested — press Test to check"
        );
    }

    #[test]
    fn traffic_text_is_labeled_probe() {
        let configs = vec![fixture("1", "Alpha", "vless", "127.0.0.1", 10801)];
        let (prober, _) = FakeHealthProber::shared();
        let mut store = HealthStore::default();
        assert!(
            store.detail_line("1") == "health: never tested — press Test to check",
            "untested row names the state plus its next step"
        );
        for outcome in test_all(&configs, DEFAULT_TEST_URL, &prober) {
            store.record_outcome(&outcome);
        }
        let line = store.detail_line("1");
        assert!(line.contains("probe:"), "probe bytes labeled, got {line}");
        assert!(line.contains("1 probe(s)"), "probe count shown, got {line}");
    }

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
        assert_eq!(untracked, "health: never tested — press Test to check");
    }

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

    #[test]
    fn format_bytes_and_truncate() {
        assert_eq!(format_bytes(0), "0B");
        assert_eq!(format_bytes(512), "512B");
        assert_eq!(format_bytes(2048), "2.0KB");
        assert_eq!(truncate("abcdef", 10), "abcdef");
        assert!(truncate(&"x".repeat(200), 140).ends_with('…'));
    }

    #[test]
    fn endpoint_addr_normalizes_wildcards() {
        use std::time::Duration;
        let timeout = Duration::from_secs(2);
        assert_eq!(
            endpoint_addr("0.0.0.0", 10801, timeout)
                .expect("parses")
                .ip()
                .to_string(),
            "127.0.0.1"
        );
        assert_eq!(
            endpoint_addr("::", 10801, timeout)
                .expect("parses")
                .ip()
                .to_string(),
            "::1"
        );
        assert_eq!(
            endpoint_addr("127.0.0.1", 10801, timeout)
                .expect("loopback parses")
                .ip()
                .to_string(),
            "127.0.0.1"
        );
        assert!(endpoint_addr("not a host!", 1, timeout).is_err());
        let refused =
            endpoint_addr("93.184.216.34", 80, timeout).expect_err("non-loopback refused");
        assert!(
            refused.contains("loopback"),
            "refusal names loopback (local cause): {refused}"
        );
        assert!(split_http_url("https://example.com/x").is_ok());
        assert!(split_http_url("notaurl").is_err());
        assert!(split_http_url(DEFAULT_TEST_URL).is_ok());
    }

    #[test]
    fn redact_secrets_covers_auth_pathed_bare_forms() {
        for (raw, leaked) in [
            ("Authorization: Bearer abc123def", "abc123def"),
            ("authorization: Basic dXNlcjpwYXNz", "dXNlcjpwYXNz"),
            ("password=hunter2 with spaces", "hunter2"),
            (
                "https://user:s3cret@host:8443/path?token=abc123#row",
                "s3cret",
            ),
            (
                "https://user:s3cret@host:8443/path?token=abc123#row",
                "abc123",
            ),
            (
                "vless://11111111-2222-4333-8444-555555555555@host:443?path=/ws&security=tls#x",
                "11111111-2222",
            ),
            (
                "peer 11111111-2222-4333-8444-555555555555 connected",
                "11111111-2222",
            ),
            (
                "key ab12cd34ef56ab78cd90ef12ab34cd56 seen",
                "ab12cd34ef56ab78cd90ef12ab34cd56",
            ),
        ] {
            let redacted = redact_secrets(raw);
            assert!(
                !redacted.contains(leaked),
                "leaked {leaked} in {redacted} (from {raw})"
            );
        }

        assert_eq!(
            redact_secrets("vless://host:443#my-row"),
            "vless://host:443#my-row"
        );
    }

    #[test]
    fn redact_secrets_preserves_keys_and_trailing_text() {
        for (raw, expected) in [
            ("password=hunter2", "password=***"),
            ("password = hunter2", "password = ***"),
            ("password=hunter2 with spaces", "password=*** with spaces"),
            ("\"password\": \"hunter2\"", "\"password\": \"***\""),
            (
                "{\"user\":{\"password\":\"hunter2\"}}",
                "{\"user\":{\"password\":\"***\"}}",
            ),
            ("token: abc123", "token: ***"),
            ("Authorization: Bearer abc123def", "Authorization: ***"),
            ("Bearer abc123def", "Bearer ***"),
        ] {
            assert_eq!(redact_secrets(raw), expected, "shape for {raw}");
        }
    }

    #[test]
    fn redact_secrets_covers_schemeless_and_pathed_url_credentials() {
        let active = vec![fixture("1", "Alpha", "vless", "127.0.0.1", 10801)];
        let tags = LogTagIndex::build(&active);
        let mut logs = LogStore::default();
        for raw in [
            "https://user:pa/ss@host:8443/path",
            "https://user:pa?ss@host:8443/path",
            "https://user:pa#ss@host:8443/path",
            "dial outbound user:hunter2@host.example.com:443 failed",
            "INFO [1234] inbound/vless[x]: user:password@example.com connected",
        ] {
            logs.push_raw(raw, &tags);
        }
        let messages: Vec<&str> = logs
            .all()
            .iter()
            .map(|record| record.message.as_str())
            .collect();
        assert_eq!(
            messages,
            vec![
                "https://***@host:8443/path",
                "https://***@host:8443/path",
                "https://***@host:8443/path",
                "dial outbound ***@host.example.com:443 failed",
                "INFO [1234] inbound/vless[x]: ***@example.com connected",
            ]
        );
    }

    #[test]
    fn redact_secrets_keeps_plain_emails_in_prose() {
        for benign in [
            "contact me at bob@example.com",
            "warn out-1: mail alice.smith@example.org bounced",
        ] {
            assert_eq!(redact_secrets(benign), benign, "email preserved: {benign}");
        }
    }

    #[test]
    fn a7_redaction_contract_holds_in_both_owners() {
        const UUID: &str = "11111111-2222-4333-8444-555555555555";
        const PASSWORD: &str = "hunter2-hunter2-hunter2-99";
        const TOKEN: &str = "tok-ABCDEF1234567890abcdef";
        let corpus = [
            format!("vless://{UUID}@host.example.com:443?security=tls#x"),
            format!("trojan://{PASSWORD}@host.example.com:443#x"),
            format!("https://host.example.com/sub?token={TOKEN}&other=1"),
            format!("ss://{PASSWORD}@host.example.com:8388#x"),
        ];
        for raw in &corpus {
            let via_health = redact_secrets(raw);
            for secret in [UUID, PASSWORD, TOKEN] {
                assert!(
                    !via_health.contains(secret),
                    "health leaked {secret} in {via_health} (from {raw})"
                );
            }
            let via_import = crate::import::redact_link(raw);
            for secret in [UUID, PASSWORD, TOKEN] {
                assert!(
                    !via_import.contains(secret),
                    "import leaked {secret} in {via_import} (from {raw})"
                );
            }
            assert!(
                via_import.contains("host.example.com"),
                "host kept for diagnostics: {via_import}"
            );
        }

        let long_secret = format!("https://host.example.com/sub?token={}", "s".repeat(200));
        let redacted = crate::import::redact_link(&long_secret);
        assert!(redacted.chars().count() <= 97, "truncated: {redacted}");
        assert!(
            !redacted.contains(&"s".repeat(5)),
            "no secret run past the cut: {redacted}"
        );
    }

    #[test]
    fn real_url_test_only_2xx_is_success() {
        use std::io::{Read as _, Write as _};
        use std::time::Duration;
        fn stub_once(reply: Vec<u8>) -> u16 {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("stub binds");
            let port = listener.local_addr().expect("addr works").port();
            std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept works");
                let mut head = [0u8; 4096];
                let _ = stream.read(&mut head);
                let _ = stream.write_all(&reply);
            });
            port
        }
        let config = fixture("p1", "Probe", "vless", "127.0.0.1", 10801);
        let prober = RealHealthProber::new(Duration::from_secs(2));
        let ok = stub_once(
            b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        );
        prober
            .url_test(&config, "127.0.0.1", ok, "http://example.com/")
            .expect("204 succeeds");
        let redirect = stub_once(
            b"HTTP/1.1 302 Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        );
        let error = prober
            .url_test(&config, "127.0.0.1", redirect, "http://example.com/")
            .expect_err("302 fails");
        assert!(error.contains("302"), "names the status: {error}");
        let broken = stub_once(
            b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec(),
        );
        let error = prober
            .url_test(&config, "127.0.0.1", broken, "http://example.com/")
            .expect_err("500 fails");
        assert!(error.contains("500"), "names the status: {error}");
    }

    #[test]
    fn real_url_test_slow_drip_hits_overall_deadline() {
        use std::io::{Read as _, Write as _};
        use std::time::{Duration, Instant};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("stub binds");
        let port = listener.local_addr().expect("addr works").port();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept works");
            let mut head = [0u8; 4096];
            let _ = stream.read(&mut head);

            for _ in 0..10_000 {
                if stream.write_all(b"x").is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let config = fixture("p1", "Probe", "vless", "127.0.0.1", 10801);
        let prober = RealHealthProber::new(Duration::from_millis(500));
        let started = Instant::now();
        let error = prober
            .url_test(&config, "127.0.0.1", port, "http://example.com/")
            .expect_err("drip must fail");
        assert!(error.contains("timed out"), "deadline cause: {error}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "bounded, took {:?}",
            started.elapsed()
        );
    }

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

        let ips = vec!["93.184.216.34".parse().expect("ip parses")];
        assert!(is_probe_destination_with_ips("93.184.216.34", probe, &ips));
        assert!(is_probe_destination_with_ips(
            "93.184.216.34:443",
            probe,
            &ips
        ));
        assert!(!is_probe_destination_with_ips("93.184.216.34", probe, &[]));
        assert!(!is_probe_destination_with_ips("203.0.113.7", probe, &ips));

        assert_eq!(resolve_probe_ips("93.184.216.34"), ips, "numeric fast path");
        assert!(resolve_probe_ips("").is_empty());
    }

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

        let hostname_only = attribute_entries(&entries[4..], probe);
        assert_eq!(hostname_only.get("7"), Some(&(1000, 100)));
    }

    #[test]
    fn bug18_exclusion_set_covers_default_global_and_overrides() {
        let mut exclusions = ProbeExclusions::new();
        assert!(exclusions.push_url(DEFAULT_TEST_URL));
        assert!(
            !exclusions.push_url("http://WWW.GSTATIC.COM:8080/other"),
            "same host after normalization is not re-added"
        );
        assert!(exclusions.push_url("http://203.0.113.9/health"));
        assert!(exclusions.push_url("https://speed.example:8443/generate_204?q=1"));
        assert!(!exclusions.push_url(""));
        assert!(!exclusions.push_url("notaurl"));
        assert_eq!(
            exclusions.hosts,
            vec![
                "www.gstatic.com".to_string(),
                "203.0.113.9".to_string(),
                "speed.example".to_string(),
            ]
        );

        assert!(exclusions.excludes("www.gstatic.com:443"));
        assert!(exclusions.excludes("WWW.GSTATIC.COM."));
        assert!(
            exclusions.excludes("203.0.113.9:80"),
            "bare-IP host matches"
        );
        assert!(exclusions.excludes("SPEED.EXAMPLE"));
        assert!(!exclusions.excludes("example.com"));
        assert!(!exclusions.excludes(""));

        let mut with_ip = ProbeExclusions::new();
        assert!(with_ip.push_url("http://probe.example/generate_204"));
        assert!(
            !with_ip.excludes("93.184.216.34"),
            "no address resolved yet"
        );
        with_ip.add_resolved_ip("93.184.216.34".parse().expect("ip parses"));
        assert!(with_ip.excludes("93.184.216.34:443"));
        assert!(with_ip.excludes("93.184.216.34"));

        let mut numeric = ProbeExclusions::new();
        assert!(numeric.push_url("http://93.184.216.34/generate_204"));
        numeric.resolve_ips();
        assert!(
            numeric.excludes("93.184.216.34"),
            "numeric resolve is local"
        );
    }

    #[test]
    fn bug21_speed_sample_is_a_duration_not_row_latency() {
        let mut store = HealthStore::default();
        let slow = SpeedtestOutcome {
            config_id: "1".to_string(),
            mode: SpeedtestMode::Download,
            latency_ms: Some(2700),
            mbps: Some(24.0),
            error: None,
        };
        store.record_speed_sample(&slow, now_unix());
        assert_eq!(
            store.status("1").state,
            RowState::Unknown,
            "a slow 8 MiB download is not a dead row"
        );
        assert_eq!(
            store.row_summary("1"),
            "2.7s",
            "a transfer duration, not a fake latency"
        );
        assert_eq!(store.speed_summary("1").as_deref(), Some("2.7s"));

        let failed = SpeedtestOutcome {
            config_id: "2".to_string(),
            mode: SpeedtestMode::Upload,
            latency_ms: None,
            mbps: None,
            error: Some("speed test failed for \"Beta\": dial refused".to_string()),
        };
        store.record_speed_sample(&failed, now_unix());
        assert_eq!(
            store.status("2").state,
            RowState::Unknown,
            "a failed speedtest never marks the row dead"
        );
        assert_eq!(
            store.speed_summary("2"),
            None,
            "failed speeds are not shown"
        );
        assert_eq!(
            store.row_summary("2"),
            "",
            "a failed speedtest leaves the health column blank, never dead"
        );

        assert_eq!(format_transfer_duration(2700), "2.7s");
        assert_eq!(format_transfer_duration(250), "250ms");
    }

    #[test]
    fn bug21_url_sparkline_drops_speed_samples() {
        fn entry(kind: &str, at: i64, ms: u64) -> HealthHistoryEntry {
            HealthHistoryEntry {
                config_id: "1".to_string(),
                at,
                kind: kind.to_string(),
                latency_ms: Some(ms),
                error: None,
            }
        }
        let entries = vec![
            entry("url-test", 10, 40),
            entry("speed-test", 20, 2700),
            entry("url-test", 30, 55),
            entry("tcp-ping", 40, 7),
        ];
        assert_eq!(
            url_test_sparkline(&entries, 120),
            vec![40, 55, 7],
            "tcp-ping history stays; only speed-test samples drop"
        );
        assert_eq!(
            url_test_sparkline(&[entry("speed-test", 1, 9000)], 120),
            Vec::<u64>::new()
        );
    }

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

        assert_eq!(MAX_PROBE_CONCURRENCY, 10, "bound pinned");
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
            "bounded at 10-way, peak was {peak}"
        );
        assert!(
            wall < std::time::Duration::from_millis(800),
            "16x100ms in ~2 waves, took {wall:?}"
        );
    }

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

    #[test]
    fn a8_clash_poll_errors_never_carry_secret() {
        use std::time::Duration;
        const SECRET: &str = "s3cret-hunter2-11111111-2222-4333-8444-555555555555";

        let refused = vec![crate::engine::ClashTarget {
            core: "down".to_string(),
            port: 1,
            secret: SECRET.to_string(),
        }];
        for (_, result) in
            poll_clash_targets(&refused, Duration::from_secs(2), Duration::from_secs(5))
        {
            let error = result.expect_err("refused port fails");
            assert!(!error.contains(SECRET), "secret in error: {error}");
            assert!(!error.contains("hunter2"), "secret part in error: {error}");
            assert!(error.contains("127.0.0.1"), "endpoint named: {error}");
        }

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("stub binds");
        let port = listener.local_addr().expect("addr works").port();
        let server = std::thread::spawn(move || {
            use std::io::{Read as _, Write as _};
            let (mut stream, _) = listener.accept().expect("accept works");
            let mut head = [0u8; 4096];
            let _ = stream.read(&mut head);
            let _ = stream.write_all(
                b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        });
        let wrong = vec![crate::engine::ClashTarget {
            core: "auth".to_string(),
            port,
            secret: SECRET.to_string(),
        }];
        for (_, result) in
            poll_clash_targets(&wrong, Duration::from_secs(5), Duration::from_secs(10))
        {
            let error = result.expect_err("wrong secret is unauthorized");
            assert!(error.contains("unauthorized"), "named: {error}");
            assert!(!error.contains(SECRET), "secret in error: {error}");
            assert!(!error.contains("hunter2"), "secret part in error: {error}");
        }
        server.join().expect("stub drains");
    }

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

        assert_eq!(index.tag_for("error out-12: y").as_deref(), Some("12"));
        assert_eq!(index.tag_for("info engine started"), None);

        let index12 = LogTagIndex::build(&active[1..]);
        assert_eq!(index12.tag_for("error out-12: y").as_deref(), Some("12"));
    }

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

    #[test]
    fn t47_latest_health_index_matches_scan() {
        let mut store = HealthStore::default();
        let mut model: Vec<HealthSample> = Vec::new();
        let kinds = [
            HealthKind::UrlTest,
            HealthKind::TcpPing,
            HealthKind::SpeedTest,
        ];
        for i in 0..600 {
            let config = format!("cfg{}", i % 3);
            let kind = kinds[(i / 3) % kinds.len()];
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

    #[test]
    fn t47_traffic_seen_map_bounds_and_sweep() {
        use std::collections::HashSet;
        let mut map = TrafficSeenMap::default();
        assert!(map.is_empty(), "fresh map tracks nothing");
        let now = 1_700_000_000i64;
        map.insert("core-a/c1".to_string(), 100, 10, now);
        assert!(!map.is_empty(), "insert tracks");
        assert_eq!(map.get("core-a/c1"), Some((100, 10)));

        let ok: HashSet<String> = ["core-a".to_string()].into_iter().collect();
        let live = ok.clone();
        map.sweep(now + 1, &ok, &HashSet::new(), &live);
        assert_eq!(map.get("core-a/c1"), None, "closed conn dropped");

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

        map.insert("dead/c9".to_string(), 7, 7, now);
        let live_only_a = live.clone();
        map.sweep(
            now + TRAFFIC_SEEN_TTL_SECS + 10,
            &ok_empty,
            &HashSet::new(),
            &live_only_a,
        );
        assert_eq!(map.get("dead/c9"), None, "dead-core entry TTL'd");

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

    fn stub_clash_raw(replies: Vec<Vec<u8>>) -> (u16, std::thread::JoinHandle<()>) {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("stub binds");
        let port = listener.local_addr().expect("addr works").port();
        let server = std::thread::spawn(move || {
            for reply in replies {
                let (mut stream, _) = listener.accept().expect("accept works");
                let mut head = [0u8; 4096];
                let _ = stream.read(&mut head);

                let _ = stream.write_all(&reply);
            }
        });
        (port, server)
    }

    fn http_reply(status: &str, body: &[u8]) -> Vec<u8> {
        let mut out = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn t48_clash_error_arm_table() {
        use std::time::Duration;
        let oversize = vec![b'x'; 2_200_000];
        let (port, server) = stub_clash_raw(vec![
            http_reply("200 OK", &oversize),
            http_reply("401 Unauthorized", br#"{"error":"unauthorized"}"#),
            http_reply("500 Internal Server Error", b"boom"),
            b"HTTP/1.1 200 OK\r\nConnection: close\r\nno-headers-end".to_vec(),
            b"not even a status line\r\n\r\n{}".to_vec(),
            http_reply("200 OK", b"not json"),
        ]);
        let timeout = Duration::from_secs(5);
        for (index, &needle) in [
            "response too large",
            "unauthorized",
            "unexpected status 500",
            "malformed response",
            "bad response",
            "not JSON",
        ]
        .iter()
        .enumerate()
        {
            let error = crate::health::poll_clash_totals(port, "s3cret", timeout)
                .expect_err(&format!("arm {index} must fail"));
            assert!(
                error.contains(needle),
                "arm {index}: typed cause ({needle}), got {error}"
            );
        }
        server.join().expect("stub drains");
    }

    #[test]
    fn t48_probe_destination_edges() {
        let probe = "example.com";

        assert!(is_probe_destination("www.example.com", probe));
        assert!(!is_probe_destination("example.com", "www.example.com"));
        assert!(is_probe_destination("WWW.EXAMPLE.COM.:443", probe));

        let ips = vec!["93.184.216.34".parse().expect("ip parses")];
        assert!(is_probe_destination_with_ips(
            "::ffff:93.184.216.34",
            probe,
            &ips
        ));
        assert!(!is_probe_destination_with_ips(
            "::ffff:203.0.113.7",
            probe,
            &ips
        ));

        assert!(!is_probe_destination("example.com:abc", probe));
        assert!(!is_probe_destination("example.com:", probe));

        assert!(!is_probe_destination("", probe));
        assert!(!is_probe_destination("example.com", ""));
        assert!(!is_probe_destination("", ""));

        assert!(is_probe_destination("EXAMPLE.COM.", probe));
    }

    #[test]
    fn health_store_counts_live_dead_and_untested() {
        fn outcome(id: &str, error: Option<&str>) -> TestOutcome {
            TestOutcome {
                sample: HealthSample {
                    config_id: id.to_string(),
                    at: now_unix(),
                    kind: HealthKind::UrlTest,
                    latency_ms: if error.is_none() { Some(11) } else { None },
                    error: error.map(str::to_string),
                },
                success: None,
            }
        }
        let mut store = HealthStore::default();
        store.record_outcome(&outcome("live", None));
        store.record_outcome(&outcome("dead", Some("connection refused")));
        assert_eq!(
            store.counts(&[
                "live".to_string(),
                "dead".to_string(),
                "missing".to_string()
            ]),
            (1, 1)
        );
        assert_eq!(store.counts(&["missing".to_string()]), (0, 0));
    }

    #[test]
    fn resolve_probe_ips_unresolvable_is_empty() {
        assert!(resolve_probe_ips("no-such-host.invalid").is_empty());
    }

    #[test]
    fn t64_best_rows_ordering_live_wins_wrong_dead_last() {
        let rows = vec![
            fixture("1", "Fast", "vless", "127.0.0.1", 10801),
            fixture("2", "Slow", "vless", "127.0.0.1", 10802),
            fixture("3", "FastTwin", "vless", "127.0.0.1", 10803),
            fixture("4", "Regressed", "vless", "127.0.0.1", 10804),
            fixture("5", "PingOnly", "vless", "127.0.0.1", 10805),
            fixture("6", "NoSample", "vless", "127.0.0.1", 10806),
            fixture("7", "Dead", "vless", "127.0.0.1", 10807),
        ];
        let url = |id: &str, at: i64, latency: Option<u64>, error: Option<&str>| -> TestOutcome {
            TestOutcome {
                success: None,
                sample: HealthSample {
                    config_id: id.to_string(),
                    at,
                    kind: HealthKind::UrlTest,
                    latency_ms: latency,
                    error: error.map(str::to_string),
                },
            }
        };
        let ping = |id: &str, at: i64, latency: u64| -> TestOutcome {
            TestOutcome {
                success: None,
                sample: HealthSample {
                    config_id: id.to_string(),
                    at,
                    kind: HealthKind::TcpPing,
                    latency_ms: Some(latency),
                    error: None,
                },
            }
        };
        let mut store = HealthStore::default();
        for probe in [
            url("1", 1, Some(45), None),
            url("2", 1, Some(220), None),
            url("3", 1, Some(45), None),
            url("4", 1, Some(45), None),
            url("4", 3, Some(999), None),
            ping("5", 1, 30),
            url(
                "7",
                1,
                None,
                Some("dial 203.0.113.7:8388: connection refused"),
            ),
        ] {
            store.record_outcome(&probe);
        }

        let ranked = |rows: &[ProxyConfig]| {
            best_rows_by_health(rows, &store)
                .iter()
                .map(|row| row.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ranked(&rows),
            vec!["5", "1", "3", "2", "4", "6", "7"],
            "latency ascending on the LATEST sample (Regressed keeps its 999 not its old 45), \
             equal 45ms ties keep input order, no-sample then dead last"
        );
        let reversed: Vec<ProxyConfig> = rows.iter().rev().cloned().collect();
        assert_eq!(
            ranked(&reversed),
            vec!["5", "3", "1", "2", "4", "6", "7"],
            "equal-latency ties follow input order; the live > unknown > dead tiers never flip"
        );
        assert!(
            best_rows_by_health(&[], &store).is_empty(),
            "empty group ranks to empty"
        );
        assert_eq!(ranked(&rows[6..7]), vec!["7"], "a lone row stays itself");

        let (title, lines) = best_rows_monitor_lines("Home", &rows, &store).expect("lines exist");
        assert_eq!(title, "Best by health in Home:");
        assert_eq!(
            lines,
            vec![
                "1) PingOnly (30ms)".to_string(),
                "2) Fast (45ms)".to_string(),
                "3) FastTwin (45ms)".to_string(),
                "4) Slow (220ms)".to_string(),
                "5) Regressed (999ms)".to_string(),
            ],
            "top five capped: NoSample (no probes) and Dead fall out of the cap"
        );

        let edge_rows = vec![
            fixture("s1", "Ghost", "vless", "127.0.0.1", 10811),
            fixture("s2", "Down", "vless", "127.0.0.1", 10812),
        ];
        let mut edge = HealthStore::default();
        edge.mark_stale("s1");
        edge.record_outcome(&url(
            "s2",
            1,
            None,
            Some("dial 203.0.113.7:8388: connection refused"),
        ));
        let (edge_title, edge_lines) =
            best_rows_monitor_lines("Home", &edge_rows, &edge).expect("edge lines exist");
        assert_eq!(edge_title, "Best by health in Home:");
        assert_eq!(
            edge_lines,
            vec![
                "1) Ghost (no probes this session) (stale)".to_string(),
                "2) Down (dead)".to_string(),
            ],
            "unknown sorts before dead in input order, stale suffix matches row text"
        );
        assert!(
            best_rows_monitor_lines("Home", &[], &edge).is_none(),
            "empty group hides the section"
        );
    }
}
