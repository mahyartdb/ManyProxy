#[cfg(test)]
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use crate::store::ProxyConfig;

pub const ENGINE_BINARY_BASE: &str = "sing-box";

pub const FORK_BINARY_BASE: &str = "sing-box-awg";

pub const XHTTP_KNOWN_BUGS: &str = "known fork issue: XHTTP stream-one/auto modes are unreliable (research 02); if streams stall, pin a working mode or Xray-sidecar version";

pub const XRAY_BINARY_BASE: &str = "xray";

pub const CLASH_PORT_START: u16 = 18501;
pub const CLASH_PORT_END: u16 = 18999;

pub const CLASH_RESERVED_NAME: &str = "clash-controller range 18501-18999";
pub const XRAY_RESERVED_NAME: &str = "xray-bridge range 11801-11999";

pub fn reserved_range_of(port: u16) -> Option<&'static str> {
    if (CLASH_PORT_START..=CLASH_PORT_END).contains(&port) {
        Some(CLASH_RESERVED_NAME)
    } else if (XRAY_BRIDGE_PORT_START..=XRAY_BRIDGE_PORT_END).contains(&port) {
        Some(XRAY_RESERVED_NAME)
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClashTarget {
    pub core: String,
    pub port: u16,
    pub secret: String,
}

fn clash_secret(core: &str) -> String {
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        static NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = std::time::SystemTime::now();
        let mut first = DefaultHasher::new();
        ("manyproxy-clash-v1", std::process::id(), now, n, core).hash(&mut first);
        let mut second = DefaultHasher::new();
        ("manyproxy-clash-v2", core, n, now, std::process::id()).hash(&mut second);
        let (a, b) = (first.finish().to_be_bytes(), second.finish().to_be_bytes());
        bytes[..8].copy_from_slice(&a);
        bytes[8..].copy_from_slice(&b);
    }
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(32);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn reserve_clash_port(holds: &mut Vec<std::net::TcpListener>) -> Option<u16> {
    reserve_port_in_range(holds, CLASH_PORT_START, CLASH_PORT_END)
}

/// Serializes reservations from the clash controller window: a config
/// generation holds it for its whole reserve-and-inject section. Module scope
/// (not a function-local `static`) so tests can take the same lock and hold the
/// window shut while they rebind a released port — otherwise a parallel test's
/// generation can grab the port between release and re-bind.
fn port_window_lock() -> std::sync::MutexGuard<'static, ()> {
    static PORT_WINDOW_LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();
    PORT_WINDOW_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

fn reserve_port_in_range(
    holds: &mut Vec<std::net::TcpListener>,
    start: u16,
    end: u16,
) -> Option<u16> {
    (start..=end).find(
        |port| match std::net::TcpListener::bind(format!("127.0.0.1:{port}")) {
            Ok(listener) => {
                holds.push(listener);
                true
            }
            Err(_) => false,
        },
    )
}

fn inject_clash_api(json: &str, port: u16, secret: &str) -> Result<String, EngineError> {
    let invalid = |detail: &str| EngineError::InvalidGenerated {
        detail: detail.to_string(),
    };
    let mut document: serde_json::Value = serde_json::from_str(json)
        .map_err(|_| invalid("sidecar config is not JSON (clash injection refused)"))?;
    let obj = document
        .as_object_mut()
        .ok_or_else(|| invalid("sidecar config is not an object (clash injection refused)"))?;
    let api = serde_json::json!({"clash_api": {
        "external_controller": format!("127.0.0.1:{port}"),
        "secret": secret,
    }});
    match obj.get_mut("experimental") {
        Some(existing) if existing.is_object() => {
            existing
                .as_object_mut()
                .expect("experimental is an object")
                .insert(
                    "clash_api".to_string(),
                    api.get("clash_api").expect("clash_api present").clone(),
                );
        }
        _ => {
            obj.insert("experimental".to_string(), api);
        }
    }
    serde_json::to_string(&document)
        .map_err(|_| invalid("clash-injected config does not serialize"))
}

fn served_listen_ports(json: &str) -> Vec<(String, u16)> {
    custom_config_listens(
        &serde_json::from_str::<serde_json::Value>(json).unwrap_or(serde_json::Value::Null),
    )
}

pub fn custom_config_listens(document: &serde_json::Value) -> Vec<(String, u16)> {
    let mut out = Vec::new();
    let Some(inbounds) = document
        .get("inbounds")
        .and_then(serde_json::Value::as_array)
    else {
        return out;
    };
    for node in inbounds {
        let port = node
            .get("listen_port")
            .or_else(|| node.get("port"))
            .and_then(serde_json::Value::as_u64)
            .filter(|port| (1..=65_535).contains(port))
            .map(|port| port as u16);
        let bind = match node.get("listen").and_then(serde_json::Value::as_str) {
            Some(text) if !text.trim().is_empty() => text.to_string(),
            _ => WILDCARD_LISTEN.to_string(),
        };
        if let Some(port) = port {
            out.push((bind, port));
        }
    }
    out
}

fn clash_target_from_served(core: &str, json: &str) -> Option<ClashTarget> {
    let document: serde_json::Value = serde_json::from_str(json).ok()?;
    let api = document.get("experimental")?.get("clash_api")?;
    let controller = api.get("external_controller")?.as_str()?;
    let port: u16 = controller.rsplit(':').next()?.parse().ok()?;
    if !controller.starts_with("127.0.0.1:") && !controller.starts_with("localhost:") {
        return None;
    }
    let secret = api.get("secret")?.as_str()?.to_string();
    if secret.is_empty() {
        return None;
    }
    Some(ClashTarget {
        core: core.to_string(),
        port,
        secret,
    })
}

const WILDCARD_LISTEN: &str = "0.0.0.0";

pub const XRAY_BRIDGE_PORT_START: u16 = 11801;
pub const XRAY_BRIDGE_PORT_END: u16 = 11999;

pub const CHAIN_EGRESS_RULE: &str = "Cannot use Extracore or Custom or Xray configs in a chain";
pub const XRAY_BRIDGE_COUNT_SIGNATURE: &str = "xray ingress tags size does not match bridge count!";

fn matches_protocol_aliases(protocol: &str, aliases: &[&str]) -> bool {
    aliases.contains(&protocol.trim().to_ascii_lowercase().as_str())
}

pub fn is_xray_protocol(protocol: &str) -> bool {
    matches_protocol_aliases(
        protocol,
        &["xray", "xray-vless", "xray_vless", "vless-xray"],
    )
}

pub fn is_custom_outbound_protocol(protocol: &str) -> bool {
    matches_protocol_aliases(
        protocol,
        &[
            "custom",
            "custom-out",
            "custom-outbound",
            "custom_outbound",
            "custom-outbound-json",
        ],
    )
}

pub fn is_custom_config_protocol(protocol: &str) -> bool {
    matches_protocol_aliases(
        protocol,
        &[
            "custom-config",
            "custom_config",
            "custom-full",
            "custom_full",
            "custom-full-config",
        ],
    )
}

pub fn is_extra_core_protocol(protocol: &str) -> bool {
    matches_protocol_aliases(
        protocol,
        &["extra", "extra-core", "extra_core", "extracore"],
    )
}

pub fn is_chain_protocol(protocol: &str) -> bool {
    matches_protocol_aliases(
        protocol,
        &["chain", "chained", "chain-outbound", "chain_outbound"],
    )
}

pub fn is_egress_only_protocol(protocol: &str) -> bool {
    is_extra_core_protocol(protocol)
        || is_custom_outbound_protocol(protocol)
        || is_custom_config_protocol(protocol)
        || is_xray_protocol(protocol)
}

pub fn is_direct_protocol(protocol: &str) -> bool {
    matches_protocol_aliases(protocol, &["direct"])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwningCore {
    Stock,
    Fork,
    Xray,
}

impl OwningCore {
    pub fn as_str(self) -> &'static str {
        match self {
            OwningCore::Stock => "stock",
            OwningCore::Fork => "fork",
            OwningCore::Xray => "xray",
        }
    }
}

fn owning_core_of(
    name: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<OwningCore, EngineError> {
    let raw = params
        .get("core")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|core| !core.is_empty());
    match raw.map(str::to_ascii_lowercase).as_deref() {
        None => Ok(OwningCore::Stock),
        Some("stock" | "sing-box" | "singbox") => Ok(OwningCore::Stock),
        Some("fork" | "awg" | "sing-box-awg" | "singbox-awg") => Ok(OwningCore::Fork),
        Some("xray" | "xray-core" | "xray_core") => Ok(OwningCore::Xray),
        Some(other) => Err(EngineError::BadCustom {
            name: name.to_string(),
            field: "core".to_string(),
            detail: format!(
                "unknown owning core \"{other}\" (want \"stock\", \"fork\", or \"xray\")"
            ),
        }),
    }
}

fn canonical_type(protocol: &str) -> Option<&'static str> {
    match protocol.trim().to_ascii_lowercase().as_str() {
        "shadowsocks" | "ss" => Some("shadowsocks"),
        "trojan" => Some("trojan"),
        "vmess" => Some("vmess"),
        "vless" => Some("vless"),
        "hysteria" => Some("hysteria"),
        "hysteria2" | "hy2" => Some("hysteria2"),
        "tuic" => Some("tuic"),
        "wireguard" | "wg" => Some("wireguard"),

        "anytls" => Some("anytls"),
        "ssh" => Some("ssh"),

        "socks" | "socks5" => Some("socks"),
        "http" | "https" => Some("http"),
        "naive" => Some("naive"),
        "snell" => Some("snell"),
        "shadowtls" | "shadow-tls" | "shadow_tls" => Some("shadowtls"),
        _ => None,
    }
}

#[cfg(test)]
pub fn is_ssh_protocol(protocol: &str) -> bool {
    matches_protocol_aliases(protocol, &["ssh"])
}

pub fn is_noise_protocol(protocol: &str) -> bool {
    matches_protocol_aliases(protocol, &["noise"])
}

pub fn inbound_tag(id: &str) -> String {
    tagged("in", id)
}

pub fn outbound_tag(id: &str) -> String {
    tagged("out", id)
}

fn tagged(prefix: &str, id: &str) -> String {
    format!("{prefix}-{}", sanitize_tag(id))
}

pub(crate) fn sanitize_tag(raw: &str) -> String {
    raw.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    PortConflict {
        bind: String,
        port: u16,
        first: String,
        second: String,
    },

    UnsupportedProtocol {
        name: String,
        protocol: String,
    },

    InvalidParamsJson {
        name: String,
    },

    BadParams {
        name: String,
        field: &'static str,
    },

    NoActiveConfigs,

    InvalidGenerated {
        detail: String,
    },

    CheckFailed {
        detail: String,
    },

    MissingBinary {
        binary: String,
        searched: String,
    },

    MissingForkBinary {
        rows: String,
        binary: String,
        searched: String,
        xhttp: bool,
    },

    BadAwg {
        name: String,
        field: String,
        detail: String,
    },

    WrongLayer {
        name: String,
        field: String,
        detail: String,
    },

    BadCustom {
        name: String,
        field: String,
        detail: String,
    },

    ChainViolation {
        name: String,
        detail: String,
    },

    XrayBridgeMismatch {
        detail: String,
    },

    XrayLoopback {
        name: String,
        detail: String,
    },

    MissingXrayBinary {
        rows: String,
        binary: String,
        searched: String,
    },

    MissingExtraBinary {
        rows: String,
        binary: String,
        searched: String,
    },

    BadExtra {
        name: String,
        field: String,
        detail: String,
    },

    BadPreset {
        name: String,
        detail: String,
    },

    SpawnFailed {
        detail: String,
    },

    PortUnavailable {
        bind: String,
        port: u16,
        name: String,
    },

    LanConfirmRequired {
        names: String,
    },

    ReservedPort {
        name: String,
        port: u16,
        range: &'static str,
    },

    BadBind {
        name: String,
        bind: String,
    },

    BadPort {
        name: String,
        detail: String,
    },

    DnsFailure {
        name: String,
        server: String,
    },
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::PortConflict {
                bind,
                port,
                first,
                second,
            } => write!(
                f,
                "port in use: {bind}:{port} already serves \"{first}\"; cannot start \"{second}\""
            ),
            EngineError::UnsupportedProtocol { name, protocol } => write!(
                f,
                "unsupported protocol for \"{name}\": {protocol} (standard set only)"
            ),
            EngineError::InvalidParamsJson { name } => {
                write!(f, "bad stored params for \"{name}\": params are not a JSON object")
            }
            EngineError::BadParams { name, field } => {
                write!(f, "bad stored params for \"{name}\": missing \"{field}\"")
            }
            EngineError::NoActiveConfigs => write!(f, "no active configurations"),
            EngineError::InvalidGenerated { detail } => {
                write!(f, "invalid generated engine config ({detail})")
            }
            EngineError::CheckFailed { detail } => {






                write!(
                    f,
                    "engine config check failed ({})",
                    crate::health::redact_secrets(&strip_ansi_codes(detail))
                )
            }
            EngineError::MissingBinary { binary, searched } => write!(
                f,
                "engine binary not found (looked for {binary} in {searched}); install sing-box — ManyProxy never downloads executables silently"
            ),
            EngineError::MissingForkBinary {
                rows,
                binary,
                searched,
                xhttp,
            } => {
                let rows = if rows.trim().is_empty() {
                    "exotic rows".to_string()
                } else {
                    rows.clone()
                };
                write!(
                    f,
                    "fork engine binary not found for {rows} (looked for {binary} in {searched}); install the AWG-capable fork — ManyProxy never downloads executables silently; stock configs keep working, only AmneziaWG/Noise/XHTTP rows need it"
                )?;
                if *xhttp {
                    write!(f, " ({XHTTP_KNOWN_BUGS})")?;
                }
                Ok(())
            }
            EngineError::BadAwg { name, field, detail } => write!(
                f,
                "bad AmneziaWG params for \"{name}\": \"{field}\" ({detail})"
            ),
            EngineError::WrongLayer { name, field, detail } => write!(
                f,
                "wrong-layer config for \"{name}\": \"{field}\" ({detail})"
            ),
            EngineError::BadCustom { name, field, detail } => write!(
                f,
                "bad custom JSON for \"{name}\": \"{field}\" ({detail})"
            ),
            EngineError::ChainViolation { name, detail } => {
                write!(f, "{CHAIN_EGRESS_RULE} (chain \"{name}\": {detail})")
            }
            EngineError::XrayBridgeMismatch { detail } => {
                write!(f, "{XRAY_BRIDGE_COUNT_SIGNATURE} ({detail})")
            }
            EngineError::XrayLoopback { name, detail } => write!(
                f,
                "xray loopback-hygiene failure for \"{name}\" ({detail})"
            ),
            EngineError::MissingXrayBinary {
                rows,
                binary,
                searched,
            } => {
                let rows = if rows.trim().is_empty() {
                    "xray rows".to_string()
                } else {
                    rows.clone()
                };
                write!(
                    f,
                    "xray engine binary not found for {rows} (looked for {binary} in {searched}); install xray-core — ManyProxy never downloads executables silently; stock configs keep working, only Xray-only rows need it"
                )
            }
            EngineError::MissingExtraBinary {
                rows,
                binary,
                searched,
            } => {
                let rows = if rows.trim().is_empty() {
                    "extra-core rows".to_string()
                } else {
                    rows.clone()
                };
                write!(
                    f,
                    "extra-core executable not found for {rows} (looked for {binary} in {searched}); place the user executable in the app directory — ManyProxy never downloads executables silently and never runs it privileged"
                )
            }
            EngineError::BadExtra { name, field, detail } => write!(
                f,
                "bad extra-core slot for \"{name}\": \"{field}\" ({detail})"
            ),
            EngineError::BadPreset { name, detail } => write!(
                f,
                "bad routing preset for \"{name}\": {detail}"
            ),
            EngineError::SpawnFailed { detail } => {
                write!(f, "engine failed to start ({detail})")
            }
            EngineError::PortUnavailable { bind, port, name } => write!(
                f,
                "port unavailable: {bind}:{port} is already in use (\"{name}\" cannot start there; another process may hold it)"
            ),
            EngineError::LanConfirmRequired { names } => write!(
                f,
                "LAN confirm required for {names} (confirm LAN exposure first)"
            ),
            EngineError::ReservedPort { name, port, range } => write!(
                f,
                "port {port} for \"{name}\" is inside the reserved {range} (pick a port outside it)"
            ),
            EngineError::BadBind { name, bind } => write!(
                f,
                "bad bind address for \"{name}\": \"{bind}\" (want an IP address or localhost)"
            ),
            EngineError::BadPort { name, detail } => {
                write!(f, "bad local port for \"{name}\" ({detail})")
            }
            EngineError::DnsFailure { name, server } => write!(
                f,
                "cannot resolve egress server for \"{name}\": \"{server}\" (DNS failure — the name does not resolve, the egress was never reached)"
            ),
        }
    }
}

pub fn is_awg_protocol(protocol: &str) -> bool {
    matches!(
        protocol.trim().to_ascii_lowercase().as_str(),
        "amneziawg" | "awg" | "amnezia-wg"
    )
}

const AWG_V2_KEYS: &[&str] = &[
    "jc", "jmin", "jmax", "s1", "s2", "s3", "s4", "h1", "h2", "h3", "h4", "i1", "i2", "i3", "i4",
    "i5",
];

const AWG_V3_ONLY_KEYS: &[&str] = &[
    "headerprotectionkey",
    "contentpaddingaddition",
    "rekeyaftertime",
    "rekeytimeout",
    "rejectaftertime",
    "keepalivetimeout",
    "maxhandshakeattempts",
    "randomtrailers",
    "disablecookies",
];

const AWG_BASE_KEYS: &[&str] = &[
    "type",
    "tag",
    "server",
    "server_port",
    "private_key",
    "public_key",
    "peer_public_key",
    "pre_shared_key",
    "allowed_ips",
    "persistent_keepalive",
    "mtu",
    "address",
    "dns",
    "reserved",
    "workers",
    "system",
    "udp_timeout",
    "local_address",
    "peers",
    "awg_version",
    "version",
    "enable_amnezia",
];

const XRAY_NOISE_KEYS: &[&str] = &["noise", "finalmask", "final_mask", "xray_noise"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AwgVersion {
    V2,
    V3,
}

impl AwgVersion {
    pub fn as_str(self) -> &'static str {
        match self {
            AwgVersion::V2 => "2.0",
            AwgVersion::V3 => "3.x",
        }
    }
}

fn is_awg_layer_key(lower: &str) -> bool {
    lower == "awg_version" || AWG_V2_KEYS.contains(&lower) || AWG_V3_ONLY_KEYS.contains(&lower)
}

fn awg_version_of(
    name: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<AwgVersion, EngineError> {
    const FIELD: &str = "awg_version";
    let raw = params.get("awg_version").or_else(|| params.get("version"));
    let text = match raw {
        Some(serde_json::Value::String(text)) => text.trim().to_string(),
        Some(serde_json::Value::Number(number)) => number.to_string(),
        Some(_) => {
            return Err(EngineError::BadAwg {
                name: name.to_string(),
                field: FIELD.to_string(),
                detail: "version must be \"2.0\" or \"3.x\"".to_string(),
            });
        }
        None => {
            return Err(EngineError::BadAwg {
                name: name.to_string(),
                field: FIELD.to_string(),
                detail: "missing AmneziaWG version (want \"awg_version\": \"2.0\" or \"3.x\")"
                    .to_string(),
            });
        }
    };
    match text.to_ascii_lowercase().as_str() {
        "2" | "2.0" => Ok(AwgVersion::V2),
        "3" | "3.0" | "3.1" | "3.x" => Ok(AwgVersion::V3),
        _ => Err(EngineError::BadAwg {
            name: name.to_string(),
            field: FIELD.to_string(),
            detail: format!("unsupported AmneziaWG version \"{text}\" (want \"2.0\" or \"3.x\")"),
        }),
    }
}

fn check_wrong_layer(
    name: &str,
    protocol: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), EngineError> {
    let awg = is_awg_protocol(protocol);
    let noise = is_noise_protocol(protocol);
    let xray = is_xray_protocol(protocol);
    for key in params.keys() {
        let lower = key.to_ascii_lowercase();
        if !awg && !noise && is_awg_layer_key(&lower) {
            return Err(EngineError::WrongLayer {
                name: name.to_string(),
                field: key.clone(),
                detail: format!(
                    "\"{key}\" is an AmneziaWG field but protocol is \"{protocol}\" \
                     (Amnezia Noise ≠ Xray Noise: match obfuscation to its server; research 02)"
                ),
            });
        }
        if noise && is_awg_layer_key(&lower) {
            return Err(EngineError::WrongLayer {
                name: name.to_string(),
                field: key.clone(),
                detail: format!(
                    "\"{key}\" is an AmneziaWG field on a standalone Noise row \
                     (AWG-vs-Noise confusion: AWG obfuscation must match the AWG server, Noise rows carry their own key; research 02)"
                ),
            });
        }
        if awg && XRAY_NOISE_KEYS.contains(&lower.as_str()) {
            return Err(EngineError::WrongLayer {
                name: name.to_string(),
                field: key.clone(),
                detail: format!(
                    "\"{key}\" is Xray-noise semantics on an AmneziaWG row \
                     (Amnezia Noise ≠ Xray Noise: AWG obfuscation must match the AWG server; research 02)"
                ),
            });
        }
        if noise && XRAY_NOISE_KEYS.contains(&lower.as_str()) {
            return Err(EngineError::WrongLayer {
                name: name.to_string(),
                field: key.clone(),
                detail: format!(
                    "\"{key}\" is Xray-noise semantics on a standalone Noise row \
                     (AWG-vs-Noise confusion: Noise rows carry their own key, Xray noise belongs on Xray rows; research 02)"
                ),
            });
        }
        if !awg && !noise && !xray && XRAY_NOISE_KEYS.contains(&lower.as_str()) {
            return Err(EngineError::WrongLayer {
                name: name.to_string(),
                field: key.clone(),
                detail: format!(
                    "\"{key}\" is Xray-noise semantics on a \"{protocol}\" row \
                     (Amnezia Noise ≠ Xray Noise: match obfuscation to its server; research 02)"
                ),
            });
        }
    }
    Ok(())
}

fn validate_awg_row(
    name: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<AwgVersion, EngineError> {
    let version = awg_version_of(name, params)?;
    for field in ["private_key", "public_key"] {
        if str_field(params, field).is_none() {
            return Err(EngineError::BadParams {
                name: name.to_string(),
                field,
            });
        }
    }
    for key in params.keys() {
        let lower = key.to_ascii_lowercase();
        if AWG_BASE_KEYS.contains(&lower.as_str()) || AWG_V2_KEYS.contains(&lower.as_str()) {
            continue;
        }
        if AWG_V3_ONLY_KEYS.contains(&lower.as_str()) {
            if version == AwgVersion::V3 {
                continue;
            }
            return Err(EngineError::BadAwg {
                name: name.to_string(),
                field: key.clone(),
                detail: format!(
                    "\"{key}\" is an AmneziaWG 3.x field but this row is version \"2.0\""
                ),
            });
        }
        return Err(EngineError::BadAwg {
            name: name.to_string(),
            field: key.clone(),
            detail: format!(
                "unknown AmneziaWG field for version \"{}\" \
                 (2.0 wants jc/jmin/jmax/s1-s4/h1-h4/i1-i5; 3.x adds header-protection/padding/trailers/ranged-timings)",
                version.as_str()
            ),
        });
    }
    Ok(version)
}

fn validate_noise_row(
    name: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), EngineError> {
    check_wrong_layer(name, "noise", params)?;
    if str_field(params, "server").is_none() {
        return Err(EngineError::BadParams {
            name: name.to_string(),
            field: "server",
        });
    }
    if !any_field(params, &["password", "key"]) {
        return Err(EngineError::BadParams {
            name: name.to_string(),
            field: "password",
        });
    }
    Ok(())
}

fn validate_ssh_row(
    name: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), EngineError> {
    if !any_field(params, &["user", "username"]) {
        return Err(EngineError::BadParams {
            name: name.to_string(),
            field: "user",
        });
    }
    let secret = ["password", "private_key", "privatekey"].iter().any(|key| {
        params
            .get(*key)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.is_empty())
    });
    if !secret {
        return Err(EngineError::BadParams {
            name: name.to_string(),
            field: "password",
        });
    }
    Ok(())
}

fn validate_anytls_row(
    name: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), EngineError> {
    let password = params
        .get("password")
        .and_then(serde_json::Value::as_str)
        .filter(|password| !password.is_empty());
    if password.is_none() {
        return Err(EngineError::BadParams {
            name: name.to_string(),
            field: "password",
        });
    }
    Ok(())
}

fn transport_of(params: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    if let Some(network) = params.get("network").and_then(serde_json::Value::as_str) {
        return Some(network.to_string());
    }
    for holder in ["transport", "streamSettings"] {
        let key = if holder == "transport" {
            "type"
        } else {
            "network"
        };
        if let Some(marker) = params
            .get(holder)
            .and_then(|holder| holder.get(key))
            .and_then(serde_json::Value::as_str)
        {
            return Some(marker.to_string());
        }
    }
    None
}

pub fn config_is_xhttp(config: &ProxyConfig) -> bool {
    let Ok(serde_json::Value::Object(map)) =
        serde_json::from_str::<serde_json::Value>(&config.params)
    else {
        return false;
    };
    transport_of(&map).is_some_and(|transport| transport.eq_ignore_ascii_case("xhttp"))
}

pub fn needs_fork(config: &ProxyConfig) -> bool {
    if is_awg_protocol(&config.protocol) || is_noise_protocol(&config.protocol) {
        return true;
    }
    if config_is_xhttp(config) {
        return matches!(
            config.protocol.trim().to_ascii_lowercase().as_str(),
            "vless" | "vmess" | "trojan"
        );
    }
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VlessEncryption {
    Stock,

    Xray,

    BadType,
}

fn vless_encryption_of(params: &serde_json::Map<String, serde_json::Value>) -> VlessEncryption {
    match params.get("encryption") {
        None | Some(serde_json::Value::Null) => VlessEncryption::Stock,
        Some(serde_json::Value::String(value)) => {
            if value.trim().is_empty() || value.trim().eq_ignore_ascii_case("none") {
                VlessEncryption::Stock
            } else {
                VlessEncryption::Xray
            }
        }
        Some(_) => VlessEncryption::BadType,
    }
}

fn vless_needs_xray(protocol: &str, params: &serde_json::Map<String, serde_json::Value>) -> bool {
    canonical_type(protocol) == Some("vless")
        && vless_encryption_of(params) == VlessEncryption::Xray
}

fn config_vless_needs_xray(config: &ProxyConfig) -> bool {
    serde_json::from_str::<serde_json::Value>(&config.params)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .is_some_and(|map| vless_needs_xray(&config.protocol, &map))
}

fn bad_vless_encryption(name: &str) -> EngineError {
    EngineError::BadCustom {
        name: name.to_string(),
        field: "encryption".to_string(),
        detail: "vless \"encryption\" must be \"none\" (stock, key dropped) or a string value served via the xray sidecar".to_string(),
    }
}

pub fn needs_xray(config: &ProxyConfig) -> bool {
    if is_xray_protocol(&config.protocol) {
        return true;
    }
    if config_vless_needs_xray(config) {
        return true;
    }
    if is_custom_outbound_protocol(&config.protocol) {
        if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(&config.params) {
            return owning_core_of(&config.name, &map).is_ok_and(|core| core == OwningCore::Xray);
        }
    }
    false
}

fn params_object(
    config: &ProxyConfig,
) -> Result<serde_json::Map<String, serde_json::Value>, EngineError> {
    serde_json::from_str::<serde_json::Value>(&config.params)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or_else(|| EngineError::InvalidParamsJson {
            name: config.name.clone(),
        })
}

fn json_port_from_value(value: &serde_json::Value) -> Option<u16> {
    match value {
        serde_json::Value::Number(number) => number
            .as_u64()
            .and_then(|port| u16::try_from(port).ok())
            .filter(|port| *port > 0),
        serde_json::Value::String(text) => text.trim().parse::<u16>().ok().filter(|port| *port > 0),
        _ => None,
    }
}

fn json_port_valid(value: &serde_json::Value) -> bool {
    json_port_from_value(value).is_some()
}

fn str_field<'a>(
    params: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    params
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn any_field(params: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> bool {
    keys.iter().any(|key| str_field(params, key).is_some())
}

fn validate_xray_row(
    name: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), EngineError> {
    str_field(params, "server").ok_or(EngineError::BadParams {
        name: name.to_string(),
        field: "server",
    })?;
    match params.get("server_port") {
        Some(value) if json_port_valid(value) => {}
        Some(_) => {
            return Err(EngineError::BadCustom {
                name: name.to_string(),
                field: "server_port".to_string(),
                detail: "xray server_port must be 1-65535".to_string(),
            });
        }
        _ => {
            return Err(EngineError::BadParams {
                name: name.to_string(),
                field: "server_port",
            });
        }
    }
    str_field(params, "uuid").ok_or(EngineError::BadParams {
        name: name.to_string(),
        field: "uuid",
    })?;
    Ok(())
}

fn validate_custom_outbound(
    name: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<(OwningCore, serde_json::Map<String, serde_json::Value>), EngineError> {
    let core = owning_core_of(name, params)?;
    let mut map = params.clone();
    map.remove("core");
    if core == OwningCore::Xray {
        if str_field(&map, "protocol").is_none() {
            return Err(EngineError::BadCustom {
                name: name.to_string(),
                field: "protocol".to_string(),
                detail: "xray-core custom outbound needs an Xray \"protocol\" (e.g. \"vless\")"
                    .to_string(),
            });
        }
    } else {
        if str_field(&map, "type").is_none() {
            return Err(EngineError::BadCustom {
                name: name.to_string(),
                field: "type".to_string(),
                detail: "custom outbound needs a sing-box \"type\" (e.g. \"vless\")".to_string(),
            });
        }
        if let Some(port) = map.get("server_port") {
            if !json_port_valid(port) {
                return Err(EngineError::BadCustom {
                    name: name.to_string(),
                    field: "server_port".to_string(),
                    detail: "custom server_port must be 1-65535".to_string(),
                });
            }
        }
    }
    Ok((core, map))
}

fn custom_full_config(
    name: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<(OwningCore, serde_json::Value), EngineError> {
    let core = owning_core_of(name, params)?;
    let document = match params.get("config") {
        Some(serde_json::Value::Object(_)) => {
            params.get("config").expect("config object present").clone()
        }
        Some(_) => {
            return Err(EngineError::BadCustom {
                name: name.to_string(),
                field: "config".to_string(),
                detail: "custom full config \"config\" must be a JSON object".to_string(),
            });
        }
        None => serde_json::Value::Object(params.clone()),
    };
    let top = document.as_object().expect("config is an object");

    if let Some((key, path)) = find_system_mutation_key(&document) {
        return Err(EngineError::BadCustom {
            name: name.to_string(),
            field: key.clone(),
            detail: format!(
                "custom full config must not carry \"{key}\" at {path} (no system network mutation)"
            ),
        });
    }
    match top.get("outbounds") {
        Some(serde_json::Value::Array(_)) => {}
        _ => {
            return Err(EngineError::BadCustom {
                name: name.to_string(),
                field: "outbounds".to_string(),
                detail: "custom full config needs an \"outbounds\" array (or wrap it in {\"config\": {...}})".to_string(),
            });
        }
    }
    Ok((core, document))
}

fn is_bad_extra_exe_name(name: &str) -> bool {
    if name.is_empty() || name == "." || name == ".." {
        return true;
    }
    if name.contains(['/', '\\', ':']) {
        return true;
    }
    if name.bytes().any(|b| b < 0x20 || b == 0x7f) {
        return true;
    }

    if name.ends_with(['.', ' ']) {
        return true;
    }
    let stem = name.split('.').next().unwrap_or(name);
    let upper = stem.to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    if (upper.starts_with("COM") || upper.starts_with("LPT"))
        && upper.len() == 4
        && upper[3..].bytes().all(|b| b.is_ascii_digit())
    {
        return true;
    }
    false
}

fn validate_extra_row(config: &ProxyConfig) -> Result<(String, String), EngineError> {
    let params = params_object(config)?;
    let exe = params
        .get("exe")
        .or_else(|| params.get("binary"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|exe| !exe.is_empty())
        .ok_or_else(|| EngineError::BadExtra {
            name: config.name.clone(),
            field: "exe".to_string(),
            detail: "extra-core rows need an \"exe\" file name (resolved in the app directory)"
                .to_string(),
        })?;
    if is_bad_extra_exe_name(exe) {
        return Err(EngineError::BadExtra {
            name: config.name.clone(),
            field: "exe".to_string(),
            detail: "extra-core \"exe\" must be a plain file name in the app directory (no paths, streams, device names, or trailing dots/spaces)"
                .to_string(),
        });
    }
    let raw = params.get("config").ok_or_else(|| EngineError::BadExtra {
        name: config.name.clone(),
        field: "config".to_string(),
        detail: "extra-core rows need a \"config\" value passed through to the executable"
            .to_string(),
    })?;
    let config_json = match raw {
        serde_json::Value::String(text) => {
            serde_json::from_str::<serde_json::Value>(text).map_err(|_| EngineError::BadExtra {
                name: config.name.clone(),
                field: "config".to_string(),
                detail: "extra-core \"config\" string must be valid JSON".to_string(),
            })?;
            text.clone()
        }
        serde_json::Value::Object(_) | serde_json::Value::Array(_) => raw.to_string(),
        _ => {
            return Err(EngineError::BadExtra {
                name: config.name.clone(),
                field: "config".to_string(),
                detail: "extra-core \"config\" must be a JSON object or a JSON string".to_string(),
            });
        }
    };
    Ok((exe.to_string(), config_json))
}

pub fn extra_config_json(config: &ProxyConfig) -> Result<String, EngineError> {
    validate_extra_row(config).map(|(_, config_json)| config_json)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainHopKind {
    Standard,
    Xray,
    Custom,
    Extra,
}

#[derive(Debug, Clone)]
pub struct ChainHop {
    pub kind: ChainHopKind,
    pub ref_id: Option<String>,
    pub node: Option<serde_json::Map<String, serde_json::Value>>,

    pub label: String,
}

fn truthy(map: &serde_json::Map<String, serde_json::Value>, key: &str) -> bool {
    map.get(key).is_some_and(|value| match value {
        serde_json::Value::Bool(flag) => *flag,
        serde_json::Value::String(text) => {
            matches!(
                text.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        }
        serde_json::Value::Number(number) => number.as_u64().is_some_and(|n| n != 0),
        _ => false,
    })
}

fn inline_hop_kind(
    name: &str,
    label: &str,
    map: &serde_json::Map<String, serde_json::Value>,
) -> Result<ChainHopKind, EngineError> {
    let marker = map
        .get("type")
        .or_else(|| map.get("protocol"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if is_chain_protocol(&marker) {
        return Err(EngineError::BadCustom {
            name: name.to_string(),
            field: "nodes".to_string(),
            detail: format!("{label} nests a chain (v1 chains are flat)"),
        });
    }

    if canonical_type(&marker) == Some("vless") {
        match vless_encryption_of(map) {
            VlessEncryption::Xray => return Ok(ChainHopKind::Xray),
            VlessEncryption::BadType => {
                return Err(EngineError::BadCustom {
                    name: name.to_string(),
                    field: "nodes".to_string(),
                    detail: format!(
                        "{label} carries a non-string \"encryption\" (want \"none\" or an xray-served string value)"
                    ),
                });
            }
            VlessEncryption::Stock => {}
        }
    }
    if is_extra_core_protocol(&marker) || truthy(map, "extra") || truthy(map, "extracore") {
        return Ok(ChainHopKind::Extra);
    }
    if is_custom_outbound_protocol(&marker)
        || is_custom_config_protocol(&marker)
        || truthy(map, "custom")
    {
        return Ok(ChainHopKind::Custom);
    }
    if is_xray_protocol(&marker) || truthy(map, "xray") {
        return Ok(ChainHopKind::Xray);
    }
    Ok(ChainHopKind::Standard)
}

pub fn validate_chain_nodes(
    name: &str,
    params: &serde_json::Map<String, serde_json::Value>,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<ChainHop>, EngineError> {
    let nodes = match params.get("nodes") {
        Some(serde_json::Value::Array(nodes)) if !nodes.is_empty() => nodes,
        _ => {
            return Err(EngineError::BadCustom {
                name: name.to_string(),
                field: "nodes".to_string(),
                detail: "chain rows need a non-empty \"nodes\" array (id references or inline outbound objects)".to_string(),
            });
        }
    };
    let mut hops = Vec::with_capacity(nodes.len());
    for (index, node) in nodes.iter().enumerate() {
        let label = format!("node {}", index + 1);
        match node {
            serde_json::Value::String(id) => {
                let id = id.trim();
                if id.is_empty() {
                    return Err(EngineError::BadCustom {
                        name: name.to_string(),
                        field: "nodes".to_string(),
                        detail: format!("{label} is an empty id reference"),
                    });
                }
                let protocol = lookup(id).ok_or_else(|| EngineError::BadCustom {
                    name: name.to_string(),
                    field: "nodes".to_string(),
                    detail: format!(
                        "{label} references unknown id \"{id}\" (chain hops reference rows in the same sidecar config or carry inline JSON)"
                    ),
                })?;
                if is_chain_protocol(&protocol) {
                    return Err(EngineError::BadCustom {
                        name: name.to_string(),
                        field: "nodes".to_string(),
                        detail: format!("{label} references chain \"{id}\" (v1 chains are flat)"),
                    });
                }
                let kind = if is_egress_only_protocol(&protocol) {
                    if is_xray_protocol(&protocol) {
                        ChainHopKind::Xray
                    } else if is_extra_core_protocol(&protocol) {
                        ChainHopKind::Extra
                    } else {
                        ChainHopKind::Custom
                    }
                } else {
                    ChainHopKind::Standard
                };
                hops.push(ChainHop {
                    kind,
                    ref_id: Some(id.to_string()),
                    node: None,
                    label: format!("{label} (\"{id}\")"),
                });
            }
            serde_json::Value::Object(map) => {
                if !map.contains_key("type") && !map.contains_key("protocol") {
                    return Err(EngineError::BadCustom {
                        name: name.to_string(),
                        field: "nodes".to_string(),
                        detail: format!(
                            "{label} is missing \"type\" (inline hops need an outbound type)"
                        ),
                    });
                }
                let kind = inline_hop_kind(name, &label, map)?;
                hops.push(ChainHop {
                    kind,
                    ref_id: None,
                    node: Some(map.clone()),
                    label,
                });
            }
            _ => {
                return Err(EngineError::BadCustom {
                    name: name.to_string(),
                    field: "nodes".to_string(),
                    detail: format!("{label} must be an id reference or an outbound object"),
                });
            }
        }
    }
    for (index, hop) in hops.iter().enumerate() {
        if index + 1 < hops.len() && hop.kind != ChainHopKind::Standard {
            return Err(EngineError::ChainViolation {
                name: name.to_string(),
                detail: format!(
                    "{} is an Extra/Custom/Xray hop mid-chain; egress-only nodes must be last",
                    hop.label
                ),
            });
        }
    }
    Ok(hops)
}

/// T-13: transition-order validation for user-built chains, one call over
/// every chain the apply path is about to plan. The core walk must stay
/// sing-box → xray → sing-box — the bridge shape every xray hop gives the
/// walk — so an xray hop never sits directly beside another non-standard
/// core hop, at most one extra-core hop exists per chain, and an autoselector
/// slot never returns (store.rs `chain_hop_rejection` refuses them at slot
/// insert; this re-checks what reaches this layer as defense-in-depth).
/// Pure: touches no config, sidecar, or OS state.
pub fn validate_chain_transition_order(chains: &[(&str, &[ChainHop])]) -> Result<(), EngineError> {
    for (name, hops) in chains {
        for pair in hops.windows(2) {
            let (earlier, later) = (&pair[0], &pair[1]);
            let pattern = match (earlier.kind, later.kind) {
                (ChainHopKind::Xray, ChainHopKind::Xray) => Some("two consecutive xray hops"),
                (ChainHopKind::Xray, ChainHopKind::Extra | ChainHopKind::Custom) => {
                    Some("an xray hop followed by an extra/custom hop")
                }
                (ChainHopKind::Extra | ChainHopKind::Custom, ChainHopKind::Xray) => {
                    Some("an extra/custom hop directly ahead of an xray hop")
                }
                _ => None,
            };
            let Some(pattern) = pattern else {
                continue;
            };
            return Err(EngineError::ChainViolation {
                name: (*name).to_string(),
                detail: format!(
                    "{} → {}: {pattern} — chain hop sequence must stay sing-box → xray → sing-box",
                    earlier.label, later.label
                ),
            });
        }
        let extra_positions: Vec<&str> = hops
            .iter()
            .filter(|hop| hop.kind == ChainHopKind::Extra)
            .map(|hop| hop.label.as_str())
            .collect();
        if extra_positions.len() > 1 {
            return Err(EngineError::ChainViolation {
                name: (*name).to_string(),
                detail: format!(
                    "{} — at most one extracore hop per chain",
                    extra_positions.join(" + ")
                ),
            });
        }
        for hop in hops.iter() {
            if hop.kind != ChainHopKind::Standard {
                continue;
            }
            let Some(node) = hop.node.as_ref() else {
                continue;
            };
            let autoselector = node
                .get("type")
                .or_else(|| node.get("protocol"))
                .and_then(serde_json::Value::as_str)
                .is_some_and(|marker| marker.trim().eq_ignore_ascii_case("autoselector"));
            if autoselector {
                return Err(EngineError::ChainViolation {
                    name: (*name).to_string(),
                    detail: format!(
                        "{} is an autoselector hop — chain hops cannot take an autoselector (v1 chains are flat — pick a concrete row)",
                        hop.label
                    ),
                });
            }
        }
    }
    Ok(())
}

pub fn xray_ingress_tag(id: &str) -> String {
    format!("x-in-{}", sanitize_tag(id))
}

pub fn xray_egress_tag(id: &str) -> String {
    format!("x-out-{}", sanitize_tag(id))
}

#[derive(Debug, Clone)]
pub struct XrayEgress {
    pub owner: String,

    pub params: serde_json::Map<String, serde_json::Value>,
    pub xray_shaped: bool,
    pub bridge_port: u16,

    pub bridge_key: String,

    pub main_tag: String,
    pub ingress_tag: String,
    pub egress_tag: String,
}

pub fn assign_bridge_ports(
    keys: &[String],
    taken: &HashSet<u16>,
) -> Result<HashMap<String, u16>, EngineError> {
    let mut assigned: HashMap<String, u16> = HashMap::new();
    let mut used: HashSet<u16> = taken.clone();
    for key in keys {
        if assigned.contains_key(key) {
            continue;
        }
        let port = (XRAY_BRIDGE_PORT_START..=XRAY_BRIDGE_PORT_END)
            .find(|port| !used.contains(port))
            .ok_or_else(|| EngineError::InvalidGenerated {
                detail: format!(
                    "no loopback bridge port left in {XRAY_BRIDGE_PORT_START}-{XRAY_BRIDGE_PORT_END} ({} bridges)",
                    keys.len()
                ),
            })?;
        used.insert(port);
        assigned.insert(key.clone(), port);
    }
    Ok(assigned)
}

pub fn validate_bridge_counts(
    ingress: usize,
    egress: usize,
    bridges: usize,
    detail_owner: &str,
) -> Result<(), EngineError> {
    if ingress != bridges || egress != bridges {
        return Err(EngineError::XrayBridgeMismatch {
            detail: format!(
                "{detail_owner}: {ingress} ingress tags / {egress} egress tags for {bridges} bridges"
            ),
        });
    }
    Ok(())
}

fn check_egress_not_loopback(owner: &str, server: &str, kind: &str) -> Result<(), EngineError> {
    let loopback = server
        .trim()
        .parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback() || ip.is_unspecified())
        || server.trim().eq_ignore_ascii_case("localhost");
    if loopback {
        return Err(EngineError::XrayLoopback {
            name: owner.to_string(),
            detail: format!(
                "{kind} points at loopback \"{server}\" (sidecar traffic must exit remotely)"
            ),
        });
    }
    Ok(())
}

pub fn generate_xray_config(egresses: &[XrayEgress]) -> Result<GeneratedXrayConfig, EngineError> {
    let mut inbounds = Vec::with_capacity(egresses.len());
    let mut outbounds: Vec<serde_json::Value> = Vec::with_capacity(egresses.len() + 1);
    let mut rules = Vec::with_capacity(egresses.len());
    for egress in egresses {
        if egress.bridge_port < XRAY_BRIDGE_PORT_START || egress.bridge_port > XRAY_BRIDGE_PORT_END
        {
            return Err(EngineError::XrayLoopback {
                name: egress.owner.clone(),
                detail: format!(
                    "bridge port {} is outside {XRAY_BRIDGE_PORT_START}-{XRAY_BRIDGE_PORT_END}",
                    egress.bridge_port
                ),
            });
        }
        let node = if egress.xray_shaped {
            let server = egress
                .params
                .get("address")
                .or_else(|| egress.params.get("server"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            if !server.trim().is_empty() {
                check_egress_not_loopback(&egress.owner, server, "xray egress")?;
            }
            let mut map = egress.params.clone();
            map.insert(
                "tag".to_string(),
                serde_json::Value::String(egress.egress_tag.clone()),
            );
            serde_json::Value::Object(map)
        } else {
            let server =
                str_field(&egress.params, "server").ok_or_else(|| EngineError::BadParams {
                    name: egress.owner.clone(),
                    field: "server",
                })?;
            check_egress_not_loopback(&egress.owner, server, "xray egress")?;
            let server_port = egress
                .params
                .get("server_port")
                .and_then(json_port_from_value)
                .unwrap_or(0);
            let uuid = egress
                .params
                .get("uuid")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .unwrap_or_default();
            let mut user = serde_json::Map::new();
            user.insert("id".to_string(), serde_json::Value::String(uuid));
            for key in ["encryption", "flow"] {
                if let Some(value) = egress.params.get(key) {
                    user.insert(key.to_string(), value.clone());
                }
            }
            let mut vnext = serde_json::Map::new();
            vnext.insert(
                "address".to_string(),
                serde_json::Value::String(server.to_string()),
            );
            vnext.insert(
                "port".to_string(),
                serde_json::Value::Number(server_port.into()),
            );
            vnext.insert(
                "users".to_string(),
                serde_json::Value::Array(vec![serde_json::Value::Object(user)]),
            );
            let mut settings = serde_json::Map::new();
            settings.insert(
                "vnext".to_string(),
                serde_json::Value::Array(vec![serde_json::Value::Object(vnext)]),
            );
            let mut stream = serde_json::Map::new();

            for key in [
                "network",
                "security",
                "sni",
                "pbk",
                "sid",
                "spiderx",
                "path",
                "host",
                "mode",
                "fp",
                "alpn",
                "packetEncoding",
            ] {
                if let Some(value) = egress.params.get(key) {
                    stream.insert(key.to_string(), value.clone());
                }
            }

            if stream.get("fp").is_none() {
                if let Some(fp) = egress.params.get("fingerprint") {
                    stream.insert("fp".to_string(), fp.clone());
                }
            }
            if stream.get("serviceName").is_none() {
                if let Some(svc) = egress
                    .params
                    .get("serviceName")
                    .or_else(|| egress.params.get("service_name"))
                {
                    stream.insert("serviceName".to_string(), svc.clone());
                }
            }

            const XRAY_FLAT_KEYS: &[&str] = &[
                "type",
                "address",
                "server",
                "server_port",
                "port",
                "uuid",
                "id",
                "encryption",
                "flow",
                "network",
                "security",
                "sni",
                "pbk",
                "sid",
                "spiderx",
                "path",
                "host",
                "mode",
                "fp",
                "fingerprint",
                "alpn",
                "packetEncoding",
                "serviceName",
                "service_name",
                "xray_mux",
            ];
            if let Some(unknown) = egress
                .params
                .keys()
                .find(|key| !XRAY_FLAT_KEYS.contains(&key.as_str()))
            {
                return Err(bad_option(
                    &egress.owner,
                    unknown,
                    "has no Xray bridge mapping (the Ph2 XHTTP/xmux matrix is not served; use custom-config for full Xray control)",
                ));
            }
            let mut node = serde_json::Map::new();
            node.insert(
                "protocol".to_string(),
                serde_json::Value::String("vless".to_string()),
            );
            node.insert(
                "tag".to_string(),
                serde_json::Value::String(egress.egress_tag.clone()),
            );
            node.insert("settings".to_string(), serde_json::Value::Object(settings));

            match egress
                .params
                .get("xray_mux")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
            {
                None | Some("") => {}
                Some("enabled") => {
                    node.insert("mux".to_string(), serde_json::json!({"enabled": true}));
                }
                Some("disabled") => {
                    node.insert("mux".to_string(), serde_json::json!({"enabled": false}));
                }
                Some(_) => {
                    return Err(EngineError::BadCustom {
                        name: egress.owner.clone(),
                        field: "xray_mux".to_string(),
                        detail: "xray_mux must be \"\" (default), \"enabled\" or \"disabled\""
                            .to_string(),
                    });
                }
            }
            if !stream.is_empty() {
                node.insert(
                    "streamSettings".to_string(),
                    serde_json::Value::Object(stream),
                );
            }
            serde_json::Value::Object(node)
        };
        inbounds.push(serde_json::json!({
            "protocol": "socks",
            "listen": "127.0.0.1",
            "port": egress.bridge_port,
            "tag": egress.ingress_tag,
            "settings": {"auth": "noauth", "udp": true},
        }));
        outbounds.push(node);
        rules.push(serde_json::json!({
            "type": "field",
            "inboundTag": [egress.ingress_tag],
            "outboundTag": egress.egress_tag,
        }));
    }
    outbounds.push(serde_json::json!({"protocol": "freedom", "tag": "direct"}));
    validate_bridge_counts(
        inbounds.len(),
        egresses.len(),
        egresses.len(),
        "xray sidecar",
    )?;
    let document = serde_json::json!({
        "log": {"level": "warning"},
        "inbounds": inbounds,
        "outbounds": outbounds,
        "routing": {"rules": rules},
    });
    Ok(GeneratedXrayConfig {
        json: serde_json::to_string(&document).expect("xray config serializes"),
    })
}

#[derive(Debug, Clone)]
pub struct GeneratedXrayConfig {
    pub json: String,
}

impl GeneratedXrayConfig {
    #[cfg(test)]
    pub fn value(&self) -> serde_json::Value {
        serde_json::from_str(&self.json).expect("xray config is valid JSON")
    }
}

pub fn validate_bridges_present(
    main_json: &str,
    egresses: &[XrayEgress],
) -> Result<(), EngineError> {
    let main: serde_json::Value =
        serde_json::from_str(main_json).map_err(|_| EngineError::XrayBridgeMismatch {
            detail: "main config is not JSON".to_string(),
        })?;
    let outbounds = main
        .get("outbounds")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| EngineError::XrayBridgeMismatch {
            detail: "main config has no outbounds".to_string(),
        })?;
    for egress in egresses {
        let found = outbounds.iter().any(|node| {
            node.get("tag").and_then(serde_json::Value::as_str) == Some(egress.main_tag.as_str())
                && node.get("server_port").and_then(serde_json::Value::as_u64)
                    == Some(egress.bridge_port as u64)
        });
        if !found {
            return Err(EngineError::XrayBridgeMismatch {
                detail: format!(
                    "bridge pointer \"{}\" (127.0.0.1:{}) missing from the main config for \"{}\"",
                    egress.main_tag, egress.bridge_port, egress.owner
                ),
            });
        }
    }
    Ok(())
}

fn bridge_pointer(main_tag: &str, bridge_port: u16) -> serde_json::Value {
    serde_json::json!({
        "type": "socks",
        "tag": main_tag,
        "server": "127.0.0.1",
        "server_port": bridge_port,
        "version": "5",
    })
}

#[derive(Debug, Clone)]
pub struct ActivationPlan {
    pub main_rows: Vec<ProxyConfig>,

    pub fork_rows: Vec<ProxyConfig>,

    pub egresses: Vec<XrayEgress>,

    pub extras: Vec<ProxyConfig>,

    pub replacements: Vec<(OwningCore, String)>,

    pub replacement_listens: Vec<(String, String, u16)>,

    pub warnings: Vec<String>,

    pub fork_names: String,
    pub fork_xhttp: bool,
    pub xray_names: String,
    pub extra_names: String,
}

pub fn validate_activation(configs: &[ProxyConfig]) -> Result<ActivationPlan, EngineError> {
    if configs.is_empty() {
        return Err(EngineError::NoActiveConfigs);
    }

    let mut taken: HashMap<(String, u16), &str> = HashMap::new();
    let mut seen_ids: HashMap<&str, &str> = HashMap::new();
    for config in configs {
        if let Some(first) = seen_ids.get(config.id.as_str()) {
            return Err(EngineError::InvalidGenerated {
                detail: format!(
                    "duplicate id \"{}\" (\"{first}\" and \"{}\")",
                    config.id, config.name
                ),
            });
        }
        seen_ids.insert(config.id.as_str(), config.name.as_str());
        if is_custom_config_protocol(&config.protocol) {
            continue;
        }

        let key = (normalize_bind_key(&config.bind_ip), config.port);
        if let Some(first) = taken.get(&key) {
            return Err(EngineError::PortConflict {
                bind: config.bind_ip.clone(),
                port: config.port,
                first: (*first).to_string(),
                second: config.name.clone(),
            });
        }
        taken.insert(key, config.name.as_str());
    }

    let mut main_rows = Vec::new();
    let mut fork_rows = Vec::new();
    let mut egresses: Vec<XrayEgress> = Vec::new();

    let mut extras: Vec<ProxyConfig> = Vec::new();
    let mut replacements: Vec<(OwningCore, String)> = Vec::new();
    let mut replacement_owner: Vec<(OwningCore, String)> = Vec::new();
    let mut bridge_keys: Vec<String> = Vec::new();
    for config in configs {
        let params = params_object(config)?;

        if !is_custom_config_protocol(&config.protocol) {
            validate_listen_for_start(&config.name, &config.bind_ip, config.port)?;
        }
        if is_extra_core_protocol(&config.protocol) {
            validate_extra_row(config)?;
            extras.push(config.clone());
        } else if is_custom_config_protocol(&config.protocol) {
            let (core, document) = custom_full_config(&config.name, &params)?;
            if replacements.iter().any(|(owned, _)| *owned == core) {
                return Err(EngineError::BadCustom {
                    name: config.name.clone(),
                    field: "core".to_string(),
                    detail: format!(
                        "only one custom full-config per core (\"{}\" already replaces {})",
                        replacements
                            .iter()
                            .filter(|(owned, _)| *owned == core)
                            .count(),
                        core.as_str()
                    ),
                });
            }
            replacements.push((core, document.to_string()));
            replacement_owner.push((core, config.name.clone()));
        } else if is_xray_protocol(&config.protocol) || vless_needs_xray(&config.protocol, &params)
        {
            check_wrong_layer(&config.name, &config.protocol, &params)?;
            validate_xray_row(&config.name, &params)?;
            bridge_keys.push(config.id.clone());
            main_rows.push(config.clone());
        } else if is_custom_outbound_protocol(&config.protocol) {
            let (core, _) = validate_custom_outbound(&config.name, &params)?;
            match core {
                OwningCore::Stock => main_rows.push(config.clone()),
                OwningCore::Fork => fork_rows.push(config.clone()),
                OwningCore::Xray => {
                    bridge_keys.push(config.id.clone());
                    main_rows.push(config.clone());
                }
            }
        } else if is_chain_protocol(&config.protocol) {
            main_rows.push(config.clone());
        } else if needs_fork(config) {
            check_wrong_layer(&config.name, &config.protocol, &params)?;

            let mut owned = params.clone();
            build_standard_node(config, &mut owned, "validate")?;
            fork_rows.push(config.clone());
        } else {
            check_wrong_layer(&config.name, &config.protocol, &params)?;

            if canonical_type(&config.protocol).is_none()
                && !is_awg_protocol(&config.protocol)
                && !is_noise_protocol(&config.protocol)
                && !is_direct_protocol(&config.protocol)
            {
                return Err(EngineError::UnsupportedProtocol {
                    name: config.name.clone(),
                    protocol: config.protocol.clone(),
                });
            }
            let mut owned = params.clone();
            build_standard_node(config, &mut owned, "validate")?;
            main_rows.push(config.clone());
        }
    }

    let main_ids: HashSet<&str> = main_rows.iter().map(|row| row.id.as_str()).collect();
    let by_id: HashMap<&str, &ProxyConfig> =
        configs.iter().map(|row| (row.id.as_str(), row)).collect();
    let mut chain_transitions: Vec<(&str, Vec<ChainHop>)> = Vec::new();
    for config in configs {
        if !is_chain_protocol(&config.protocol) {
            continue;
        }
        let params = params_object(config)?;
        let hops = validate_chain_nodes(&config.name, &params, &|id| {
            by_id.get(id).map(|row| row.protocol.clone())
        })?;
        chain_transitions.push((config.name.as_str(), hops.clone()));
        for (index, hop) in hops.iter().enumerate() {
            if let Some(id) = hop.ref_id.as_deref() {
                if !main_ids.contains(id) {
                    return Err(EngineError::BadCustom {
                        name: config.name.clone(),
                        field: "nodes".to_string(),
                        detail: format!(
                            "{} references \"{id}\" outside the main sidecar (v1 chains stay within one sidecar; Extra and full-config rows are never chain hops)",
                            hop.label
                        ),
                    });
                }
                if matches!(hop.kind, ChainHopKind::Extra | ChainHopKind::Custom)
                    && by_id.get(id).is_some_and(|row| {
                        is_extra_core_protocol(&row.protocol)
                            || is_custom_config_protocol(&row.protocol)
                    })
                {
                    return Err(EngineError::BadCustom {
                        name: config.name.clone(),
                        field: "nodes".to_string(),
                        detail: format!(
                            "{} references \"{id}\", which runs outside the main sidecar (never a chain hop)",
                            hop.label
                        ),
                    });
                }

                if index + 1 < hops.len()
                    && by_id
                        .get(id)
                        .is_some_and(|row| config_vless_needs_xray(row))
                {
                    return Err(EngineError::ChainViolation {
                        name: config.name.clone(),
                        detail: format!(
                            "{} references a VLESS-`encryption` row mid-chain; xray-bridged nodes must be last",
                            hop.label
                        ),
                    });
                }
            }
            if hop.kind == ChainHopKind::Xray && hop.ref_id.is_none() {
                bridge_keys.push(format!("{}#hop{}", config.id, index));
            }
        }
    }

    let chain_views: Vec<(&str, &[ChainHop])> = chain_transitions
        .iter()
        .map(|(name, hops)| (*name, hops.as_slice()))
        .collect();
    validate_chain_transition_order(&chain_views)?;

    let taken_ports: HashSet<u16> = configs
        .iter()
        .filter(|row| !is_custom_config_protocol(&row.protocol))
        .map(|row| row.port)
        .collect();
    let ports = assign_bridge_ports(&bridge_keys, &taken_ports)?;
    for config in configs {
        if is_xray_protocol(&config.protocol) || config_vless_needs_xray(config) {
            let params = params_object(config)?;
            egresses.push(XrayEgress {
                owner: config.name.clone(),
                params,
                xray_shaped: false,
                bridge_port: ports[&config.id],
                bridge_key: config.id.clone(),
                main_tag: outbound_tag(&config.id),
                ingress_tag: xray_ingress_tag(&config.id),
                egress_tag: xray_egress_tag(&config.id),
            });
        } else if is_custom_outbound_protocol(&config.protocol) {
            let params = params_object(config)?;
            if owning_core_of(&config.name, &params)? == OwningCore::Xray {
                let (_, map) = validate_custom_outbound(&config.name, &params)?;
                egresses.push(XrayEgress {
                    owner: config.name.clone(),
                    params: map,
                    xray_shaped: true,
                    bridge_port: ports[&config.id],
                    bridge_key: config.id.clone(),
                    main_tag: outbound_tag(&config.id),
                    ingress_tag: xray_ingress_tag(&config.id),
                    egress_tag: xray_egress_tag(&config.id),
                });
            }
        } else if is_chain_protocol(&config.protocol) {
            let params = params_object(config)?;
            let hops = validate_chain_nodes(&config.name, &params, &|id| {
                by_id.get(id).map(|row| row.protocol.clone())
            })?;
            for (index, hop) in hops.iter().enumerate() {
                if hop.kind != ChainHopKind::Xray || hop.ref_id.is_some() {
                    continue;
                }
                let map = hop.node.clone().expect("inline xray hop has a node");
                let xray_shaped = map.contains_key("protocol");
                if !xray_shaped {
                    validate_xray_hop(&config.name, &hop.label, &map)?;
                }
                egresses.push(XrayEgress {
                    owner: format!("{} {}", config.name, hop.label),
                    params: map,
                    xray_shaped,
                    bridge_port: ports[&format!("{}#hop{}", config.id, index)],
                    bridge_key: format!("{}#hop{}", config.id, index),
                    main_tag: format!("{}-hop-{index}", outbound_tag(&config.id)),
                    ingress_tag: format!("{}-hop-{index}", xray_ingress_tag(&config.id)),
                    egress_tag: format!("{}-hop-{index}", xray_egress_tag(&config.id)),
                });
            }
        }
    }

    for (core, name) in &replacement_owner {
        let siblings = match core {
            OwningCore::Stock => main_rows.len(),
            OwningCore::Fork => fork_rows.len(),
            OwningCore::Xray => egresses.len(),
        };
        if siblings > 0 {
            return Err(EngineError::BadCustom {
                name: name.clone(),
                field: "core".to_string(),
                detail: format!(
                    "custom config for {} also claims {siblings} row(s); stop or move them",
                    core.as_str()
                ),
            });
        }
    }

    let bridge_owner_by_port: HashMap<u16, String> = egresses
        .iter()
        .map(|egress| (egress.bridge_port, egress.owner.clone()))
        .collect();
    let mut replacement_listens: Vec<(String, String, u16)> = Vec::new();
    for config in configs {
        if !is_custom_config_protocol(&config.protocol) {
            continue;
        }
        let Ok(params) = params_object(config) else {
            continue;
        };
        let Ok((_, document)) = custom_full_config(&config.name, &params) else {
            continue;
        };
        for (bind, port) in custom_config_listens(&document) {
            if normalize_bind_key(&bind) == "loopback" {
                if let Some(owner) = bridge_owner_by_port.get(&port) {
                    return Err(EngineError::PortConflict {
                        bind,
                        port,
                        first: owner.clone(),
                        second: config.name.clone(),
                    });
                }
            }
            let key = (normalize_bind_key(&bind), port);
            if let Some(first) = taken.get(&key) {
                return Err(EngineError::PortConflict {
                    bind,
                    port,
                    first: (*first).to_string(),
                    second: config.name.clone(),
                });
            }
            taken.insert(key, config.name.as_str());
            replacement_listens.push((config.name.clone(), bind, port));
        }
    }

    let mut warnings: Vec<String> = Vec::new();
    for config in configs {
        if let Ok(params) = params_object(config) {
            warnings.extend(typo_hints_for(&config.name, &config.protocol, &params));
        }
    }
    warnings.sort();
    warnings.dedup();
    let quoted = |rows: &[ProxyConfig]| {
        rows.iter()
            .map(|row| format!("\"{}\"", row.name))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let fork_names = quoted(&fork_rows);
    let fork_xhttp = fork_rows.iter().any(config_is_xhttp);
    let xray_names = egresses
        .iter()
        .map(|egress| format!("\"{}\"", egress.owner))
        .collect::<Vec<_>>()
        .join(", ");
    let extra_names = extras
        .iter()
        .map(|row| format!("\"{}\"", row.name))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(ActivationPlan {
        main_rows,
        fork_rows,
        egresses,
        extras,
        replacements,
        replacement_listens,
        warnings,
        fork_names,
        fork_xhttp,
        xray_names,
        extra_names,
    })
}

fn validate_xray_hop(
    name: &str,
    label: &str,
    map: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), EngineError> {
    if str_field(map, "server").is_none() {
        return Err(EngineError::BadCustom {
            name: name.to_string(),
            field: "nodes".to_string(),
            detail: format!("{label} xray hop is missing \"server\""),
        });
    }
    if str_field(map, "uuid").is_none() {
        return Err(EngineError::BadCustom {
            name: name.to_string(),
            field: "nodes".to_string(),
            detail: format!("{label} xray hop is missing \"uuid\""),
        });
    }

    if map
        .get("encryption")
        .is_some_and(|value| !value.is_string())
    {
        return Err(EngineError::BadCustom {
            name: name.to_string(),
            field: "nodes".to_string(),
            detail: format!("{label} xray hop carries a non-string \"encryption\""),
        });
    }
    Ok(())
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsolationEntry {
    pub config_id: String,
    pub inbound_tag: String,
    pub outbound_tag: String,
}

#[derive(Debug, Clone)]
pub struct GeneratedConfig {
    pub json: String,
}

impl GeneratedConfig {
    #[cfg(test)]
    pub fn value(&self) -> serde_json::Value {
        serde_json::from_str(&self.json).expect("generated config is valid JSON")
    }
}

#[cfg(test)]
pub fn generate(configs: &[ProxyConfig]) -> Result<GeneratedConfig, EngineError> {
    generate_with_presets(configs, &HashMap::new())
}

/// T-12 variant: like `generate_with_presets`, but staged `.srs` rule sets are
/// resolvable by name (absolute file paths, cached beside the app). Preset
/// rules carrying `rule_set` references fail loud when a name is not in the
/// map or its file vanished from disk — never skip, never substitute.
pub fn generate_with_rule_sets(
    configs: &[ProxyConfig],
    presets: &HashMap<String, Vec<serde_json::Value>>,
    rule_set_files: &HashMap<String, PathBuf>,
) -> Result<GeneratedConfig, EngineError> {
    if configs.is_empty() {
        return Err(EngineError::NoActiveConfigs);
    }
    let mut taken: HashMap<(String, u16), &str> = HashMap::new();
    for config in configs {
        if is_custom_config_protocol(&config.protocol) {
            continue;
        }

        let key = (normalize_bind_key(&config.bind_ip), config.port);
        if let Some(first) = taken.get(&key) {
            return Err(EngineError::PortConflict {
                bind: config.bind_ip.clone(),
                port: config.port,
                first: (*first).to_string(),
                second: config.name.clone(),
            });
        }
        taken.insert(key, config.name.as_str());
    }
    let keys = bridge_keys_for(configs)?;

    let taken_ports: HashSet<u16> = configs
        .iter()
        .filter(|row| !is_custom_config_protocol(&row.protocol))
        .map(|row| row.port)
        .collect();
    let bridges = assign_bridge_ports(&keys, &taken_ports)?;
    build_main_config(configs, &bridges, presets, rule_set_files)
}

/// Test-only alias of `generate_with_rule_sets` with no staged rule sets: any
/// preset carrying `rule_set` references fails loud — the "unstaged sets fail
/// loud" contract T-11 promised in its help text. Production code calls
/// `generate_with_rule_sets` with the actual staged map.
#[cfg(test)]
pub fn generate_with_presets(
    configs: &[ProxyConfig],
    presets: &HashMap<String, Vec<serde_json::Value>>,
) -> Result<GeneratedConfig, EngineError> {
    generate_with_rule_sets(configs, presets, &HashMap::new())
}

/// Per-config-build rule-set staging state: the staged name→file map synced
/// from the store, plus the entries accumulated for `route.rule_set`. One
/// instance per config build, shared across every preset's rules so identical
/// references stay deduped.
struct RuleSetStage<'a> {
    files: &'a HashMap<String, PathBuf>,
    tags: HashMap<String, String>,
    entries: Vec<serde_json::Value>,
}

impl<'a> RuleSetStage<'a> {
    fn new(files: &'a HashMap<String, PathBuf>) -> Self {
        Self {
            files,
            tags: HashMap::new(),
            entries: Vec::new(),
        }
    }

    /// Resolves `name` to the `rs-<sanitized>` tag, emitting its
    /// `route.rule_set` entry on first use. Missing staged file at configgen →
    /// loud detail naming the set and the path: never skip, never substitute.
    fn resolve(&mut self, name: &str, rule_index: usize) -> Result<String, String> {
        if let Some(tag) = self.tags.get(name) {
            return Ok(tag.clone());
        }
        let path = match self.files.get(name) {
            Some(path) => path.clone(),
            None => crate::store::staged_rule_set_path(&crate::store::rule_set_file_name(name)),
        };
        if !path.is_file() {
            return Err(format!(
                "rule {rule_index}: field \"rule_set\" names staged set \"{name}\" whose staged file is missing: {} — re-download it under Rule sets",
                path.display()
            ));
        }
        let tag = format!("rs-{}", sanitize_tag(name));
        if self
            .entries
            .iter()
            .any(|entry| entry.get("tag").and_then(serde_json::Value::as_str) == Some(tag.as_str()))
        {
            return Err(format!(
                "rule {rule_index}: staged set \"{name}\" sanitizes to tag \"{tag}\" that another staged set already uses — rename one"
            ));
        }
        self.entries.push(serde_json::json!({
            "type": "local",
            "tag": tag,
            "format": "binary",
            "path": path.display().to_string(),
        }));
        self.tags.insert(name.to_string(), tag.clone());
        Ok(tag)
    }
}

fn preset_rules_for(
    config: &ProxyConfig,
    serving_tag: &str,
    inbound_tag: &str,
    presets: &HashMap<String, Vec<serde_json::Value>>,
    staged: &mut RuleSetStage,
) -> Result<Vec<serde_json::Value>, EngineError> {
    let bad = |detail: String| EngineError::BadPreset {
        name: config.name.clone(),
        detail,
    };
    let Some(preset_id) = config.preset_id.as_deref() else {
        return Ok(Vec::new());
    };
    let Some(stored) = presets.get(preset_id) else {
        return Err(bad(format!(
            "attached routing preset (id {preset_id}) is missing — detach it or re-import it"
        )));
    };
    let mut out = Vec::with_capacity(stored.len());
    for (index, rule) in stored.iter().enumerate() {
        let checked = crate::preset::check_preset_rule(index, rule)
            .map_err(|failure| bad(format!("rule {}: {}", failure.index, failure.detail)))?;
        let obj = checked.obj;
        let action = checked.action;
        let route_like = crate::preset::action_carries_outbound(action);
        let mut merged = obj.clone();
        // Rule names are an authoring artifact (Throne shares per-rule
        // labels); they stay out of the generated config.
        merged.remove("name");

        // T-12: references fold from staged names to `rs-*` tags during the
        // merge — `route.rule_set` entries carry the absolute staged paths,
        // rules reference the tags.
        if let Some(serde_json::Value::Array(names)) = merged.get("rule_set") {
            let mut tags = Vec::with_capacity(names.len());
            for name in names {
                let name = name.as_str().ok_or_else(|| {
                    bad("rule's \"rule_set\" must be a list of staged set names".to_string())
                })?;
                let tag = staged.resolve(name, index).map_err(bad)?;
                tags.push(serde_json::Value::String(tag));
            }
            merged.insert("rule_set".to_string(), serde_json::Value::Array(tags));
        }

        merged.insert(
            "action".to_string(),
            serde_json::Value::String(crate::preset::served_action(action).to_string()),
        );
        if route_like {
            match obj.get("outbound").and_then(serde_json::Value::as_str) {
                None => {
                    merged.insert(
                        "outbound".to_string(),
                        serde_json::Value::String(serving_tag.to_string()),
                    );
                }

                Some(explicit) if explicit == serving_tag || explicit == "direct" => {}
                Some(_) => {
                    return Err(bad(format!(
                        "rule {index}: field \"outbound\" names an unknown exit (want the row's outbound or \"direct\")"
                    )));
                }
            }
        }

        match merged.get("inbound") {
            None => {
                merged.insert(
                    "inbound".to_string(),
                    serde_json::Value::Array(vec![serde_json::Value::String(
                        inbound_tag.to_string(),
                    )]),
                );
            }
            Some(serde_json::Value::Array(items))
                if items.len() == 1 && items[0].as_str() == Some(inbound_tag) => {}
            Some(_) => {
                return Err(bad(format!(
                    "rule {index}: field \"inbound\" must scope to this row (foreign inbounds are never kept)"
                )));
            }
        }
        out.push(serde_json::Value::Object(merged));
    }
    Ok(out)
}

fn bridge_keys_for(configs: &[ProxyConfig]) -> Result<Vec<String>, EngineError> {
    let mut keys = Vec::new();
    for config in configs {
        if is_xray_protocol(&config.protocol) || config_vless_needs_xray(config) {
            keys.push(config.id.clone());
            continue;
        }
        if is_custom_outbound_protocol(&config.protocol) {
            let params = params_object(config)?;
            if owning_core_of(&config.name, &params)? == OwningCore::Xray {
                keys.push(config.id.clone());
            }
            continue;
        }
        if is_chain_protocol(&config.protocol) {
            let params = params_object(config)?;
            let hops = validate_chain_nodes(&config.name, &params, &|id| {
                configs
                    .iter()
                    .find(|row| row.id == id)
                    .map(|row| row.protocol.clone())
            })?;
            for (index, hop) in hops.iter().enumerate() {
                if hop.kind == ChainHopKind::Xray && hop.ref_id.is_none() {
                    keys.push(format!("{}#hop{}", config.id, index));
                }
            }
        }
    }
    Ok(keys)
}

fn build_standard_node(
    config: &ProxyConfig,
    params: &mut serde_json::Map<String, serde_json::Value>,
    out_tag: &str,
) -> Result<(serde_json::Value, bool, bool), EngineError> {
    if is_direct_protocol(&config.protocol) {
        let node = serde_json::json!({"type": "direct", "tag": out_tag});
        return Ok((node, false, false));
    }
    str_field(params, "server").ok_or(EngineError::BadParams {
        name: config.name.clone(),
        field: "server",
    })?;

    check_wrong_layer(&config.name, &config.protocol, params)?;
    let xhttp =
        transport_of(params).is_some_and(|transport| transport.eq_ignore_ascii_case("xhttp"));

    let (canonical, fork_row) = if is_awg_protocol(&config.protocol) {
        validate_awg_row(&config.name, params)?;
        ("wireguard", true)
    } else if is_noise_protocol(&config.protocol) {
        validate_noise_row(&config.name, params)?;
        ("noise", true)
    } else if let Some(canonical) = canonical_type(&config.protocol) {
        if xhttp && !matches!(canonical, "vless" | "vmess" | "trojan") {
            return Err(EngineError::WrongLayer {
                name: config.name.clone(),
                field: "network".to_string(),
                detail: format!(
                    "xhttp is a VLESS-family transport, not a \"{canonical}\" transport (wrong layer; research 02)"
                ),
            });
        }
        if canonical == "ssh" {
            validate_ssh_row(&config.name, params)?;
        } else if canonical == "anytls" {
            validate_anytls_row(&config.name, params)?;
        }
        (canonical, xhttp)
    } else {
        return Err(EngineError::UnsupportedProtocol {
            name: config.name.clone(),
            protocol: config.protocol.clone(),
        });
    };

    if canonical == "http"
        && config.protocol.trim().eq_ignore_ascii_case("https")
        && !params.contains_key("security")
        && !params.contains_key("tls")
    {
        params.insert(
            "security".to_string(),
            serde_json::Value::String("tls".to_string()),
        );
    }

    if canonical == "vless" {
        match vless_encryption_of(params) {
            VlessEncryption::Stock => {
                params.remove("encryption");
            }
            VlessEncryption::BadType => return Err(bad_vless_encryption(&config.name)),
            VlessEncryption::Xray => {
                return Err(EngineError::BadCustom {
                    name: config.name.clone(),
                    field: "encryption".to_string(),
                    detail: "vless rows carrying a real \"encryption\" value run in the xray sidecar, not the stock config".to_string(),
                });
            }
        }
    }

    translate_outbound(canonical, &config.name, params)?;
    let server_port = match params.get("server_port") {
        Some(serde_json::Value::Number(number)) => number
            .as_u64()
            .filter(|port| (1..=65_535).contains(port))
            .map(|port| port as u16),
        Some(serde_json::Value::String(text)) => {
            text.trim().parse::<u16>().ok().filter(|port| *port > 0)
        }
        _ => None,
    }
    .ok_or(EngineError::BadParams {
        name: config.name.clone(),
        field: "server_port",
    })?;
    params.insert(
        "server_port".to_string(),
        serde_json::Value::Number(server_port.into()),
    );
    if canonical == "wireguard" {
        reshape_wireguard_endpoint(&config.name, params, server_port)?;
    }
    params.insert(
        "type".to_string(),
        serde_json::Value::String(canonical.to_string()),
    );
    params.insert(
        "tag".to_string(),
        serde_json::Value::String(out_tag.to_string()),
    );
    let node = serde_json::Value::Object(params.clone());
    Ok((node, canonical == "wireguard", fork_row))
}

fn take_flat(
    params: &mut serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    let mut found = None;
    for key in keys {
        if let Some(serde_json::Value::String(text)) = params.remove(*key) {
            if found.is_none() && !text.trim().is_empty() {
                found = Some(text.trim().to_string());
            }
        } else {
            params.remove(*key);
        }
    }
    found
}

fn take_truthy(params: &mut serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> bool {
    let mut on = false;
    for key in keys {
        match params.remove(*key) {
            Some(serde_json::Value::String(text)) => {
                if matches!(
                    text.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on" | "enabled"
                ) {
                    on = true;
                }
            }
            Some(serde_json::Value::Bool(flag)) => on |= flag,
            Some(serde_json::Value::Number(number)) => {
                on |= number.as_u64().is_some_and(|n| n != 0);
            }
            _ => {}
        }
    }
    on
}

fn ech_block(
    mark: Option<String>,
    config: Option<String>,
    config_path: Option<String>,
) -> Option<serde_json::Value> {
    fn truthy(value: &str) -> bool {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on" | "enabled"
        )
    }
    fn falsy(value: &str) -> bool {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off" | "disabled"
        )
    }
    let mut block = serde_json::Map::new();
    let mut enabled = false;
    if let Some(mark) = mark {
        if truthy(&mark) {
            enabled = true;
        } else if !mark.trim().is_empty() && !falsy(&mark) {
            enabled = true;
            block.insert(
                "config".to_string(),
                serde_json::Value::String(mark.trim().to_string()),
            );
        }
    }
    if let Some(config) = config {
        if !config.trim().is_empty() {
            enabled = true;
            block
                .entry("config".to_string())
                .or_insert(serde_json::Value::String(config.trim().to_string()));
        }
    }
    if let Some(path) = config_path {
        if !path.trim().is_empty() {
            enabled = true;
            block.insert(
                "config_path".to_string(),
                serde_json::Value::String(path.trim().to_string()),
            );
        }
    }
    if !enabled {
        return None;
    }
    block.insert("enabled".to_string(), serde_json::Value::Bool(true));
    Some(serde_json::Value::Object(block))
}

fn bad_option(name: &str, field: &str, detail: &str) -> EngineError {
    EngineError::BadCustom {
        name: name.to_string(),
        field: field.to_string(),
        detail: detail.to_string(),
    }
}

fn split_alpn(raw: Option<String>) -> Vec<serde_json::Value> {
    raw.map(|text| {
        text.split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(|entry| serde_json::Value::String(entry.to_string()))
            .collect()
    })
    .unwrap_or_default()
}

fn tls_object(
    server: &str,
    server_name: Option<String>,
    fingerprint: Option<String>,
    alpn: Vec<serde_json::Value>,
    reality: Option<(String, Option<String>)>,
    insecure: bool,
    ech: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut tls = serde_json::Map::with_capacity(7);
    tls.insert("enabled".to_string(), serde_json::Value::Bool(true));
    tls.insert(
        "server_name".to_string(),
        serde_json::Value::String(server_name.unwrap_or_else(|| server.to_string())),
    );
    if let Some(fingerprint) = fingerprint {
        tls.insert(
            "utls".to_string(),
            serde_json::json!({"enabled": true, "fingerprint": fingerprint}),
        );
    }
    if !alpn.is_empty() {
        tls.insert("alpn".to_string(), serde_json::Value::Array(alpn));
    }
    if let Some((public_key, short_id)) = reality {
        let mut real = serde_json::Map::with_capacity(3);
        real.insert("enabled".to_string(), serde_json::Value::Bool(true));
        real.insert(
            "public_key".to_string(),
            serde_json::Value::String(public_key),
        );
        if let Some(short_id) = short_id {
            real.insert("short_id".to_string(), serde_json::Value::String(short_id));
        }
        tls.insert("reality".to_string(), serde_json::Value::Object(real));
    }
    if insecure {
        tls.insert("insecure".to_string(), serde_json::Value::Bool(true));
    }
    if let Some(ech) = ech {
        tls.insert("ech".to_string(), ech);
    }
    serde_json::Value::Object(tls)
}

enum TlsIntent {
    Off,
    On { reality: bool },
}

fn apply_tls(
    name: &str,
    params: &mut serde_json::Map<String, serde_json::Value>,
    server: &str,
    default_on: bool,
    mandatory: bool,
) -> Result<(), EngineError> {
    let security = take_flat(params, &["security"]);
    let intent = match security.as_deref().map(str::to_ascii_lowercase).as_deref() {
        None if default_on => TlsIntent::On { reality: false },
        None => TlsIntent::Off,
        Some("tls" | "xtls") => TlsIntent::On { reality: false },
        Some("reality") => TlsIntent::On { reality: true },
        Some("none" | "") => {
            if mandatory {
                return Err(bad_option(
                    name,
                    "security",
                    "this protocol requires TLS (security=none cannot work here)",
                ));
            }
            TlsIntent::Off
        }
        Some(other) => {
            return Err(bad_option(
                name,
                "security",
                &format!("unknown security \"{other}\" (want \"tls\", \"reality\", or \"none\")"),
            ));
        }
    };
    let server_name = take_flat(params, &["sni"]).or_else(|| take_flat(params, &["peer"]));
    let fingerprint = take_flat(params, &["fp", "fingerprint"]);
    let alpn = split_alpn(take_flat(params, &["alpn"]));
    let public_key = take_flat(params, &["pbk"]);
    let short_id = take_flat(params, &["sid"]);

    let insecure = take_truthy(
        params,
        &[
            "allowInsecure",
            "allow_insecure",
            "allowinsecure",
            "insecure",
        ],
    );

    let (native_ech, ech_mark) = match params.remove("ech") {
        Some(serde_json::Value::Object(map)) => (Some(serde_json::Value::Object(map)), None),
        Some(serde_json::Value::String(text)) => (None, Some(text)),
        Some(serde_json::Value::Bool(true)) => (None, Some("true".to_string())),
        _ => (None, None),
    };
    let ech_config = take_flat(params, &["ech_config", "echconfig"]);
    let ech_config_path = take_flat(params, &["ech_config_path", "echconfigpath"]);
    let ech = native_ech.or_else(|| ech_block(ech_mark, ech_config, ech_config_path));
    match intent {
        TlsIntent::Off => Ok(()),
        TlsIntent::On { reality: false } => {
            params.insert(
                "tls".to_string(),
                tls_object(server, server_name, fingerprint, alpn, None, insecure, ech),
            );
            Ok(())
        }
        TlsIntent::On { reality: true } => {
            let public_key = public_key.ok_or_else(|| EngineError::BadParams {
                name: name.to_string(),
                field: "pbk",
            })?;
            params.insert(
                "tls".to_string(),
                tls_object(
                    server,
                    server_name,
                    fingerprint,
                    alpn,
                    Some((public_key, short_id)),
                    insecure,
                    ech,
                ),
            );
            Ok(())
        }
    }
}

fn reject_115_fields(
    name: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), EngineError> {
    if params.contains_key("certificate_sha256") {
        return Err(bad_option(
            name,
            "certificate_sha256",
            "certificate pinning is sing-box 1.15.0+ (1.14.2 pins via \"certificate_public_key_sha256\")",
        ));
    }
    Ok(())
}

const DIAL_KEYS: &[&str] = &[
    "bind_interface",
    "routing_mark",
    "reuse_addr",
    "connect_timeout",
    "tcp_fast_open",
    "tcp_multi_path",
    "udp_fragment",
    "netns",
    "domain_resolver",
];

fn reject_unknown_params(
    name: &str,
    canonical: &str,
    params: &serde_json::Map<String, serde_json::Value>,
    allowed: &[&str],
    tolerate_near_miss: bool,
) -> Result<(), EngineError> {
    let mut keys: Vec<&String> = params.keys().collect();
    keys.sort();
    for key in keys {
        if ["type", "tag", "server", "server_port"].contains(&key.as_str()) {
            continue;
        }
        if allowed.contains(&key.as_str()) || DIAL_KEYS.contains(&key.as_str()) {
            continue;
        }
        // vless/vmess/trojan tolerate near-miss keys (the typo hint warns) so a
        // legitimate schema the importer accepts is never a hard failure; strict
        // protocols keep the pre-existing hard rejection.
        if tolerate_near_miss && suggest_param_key(key).is_some() {
            continue;
        }
        return Err(bad_option(
            name,
            key,
            &format!("\"{key}\" has no sing-box 1.14.2 mapping on {canonical}"),
        ));
    }
    Ok(())
}

fn take_speed(
    name: &str,
    params: &mut serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
    field: &str,
) -> Result<Option<u64>, EngineError> {
    let bad = || {
        bad_option(
            name,
            field,
            &format!("hysteria2 \"{field}\" must be a number (Mbps)"),
        )
    };
    let mut numeric: Option<Option<u64>> = None;
    for key in keys {
        if let Some(serde_json::Value::Number(number)) = params.get(*key) {
            if numeric.is_none() {
                numeric = Some(number.as_u64());
            }
        }
    }
    if let Some(number) = numeric {
        take_flat(params, keys);
        return number.map(Some).ok_or_else(bad);
    }
    let raw = take_flat(params, keys);
    match raw {
        None => Ok(None),
        Some(text) => {
            let digits: String = text
                .trim()
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if digits.is_empty() {
                return Err(bad());
            }
            digits.parse::<u64>().map(Some).map_err(|_| bad())
        }
    }
}

fn apply_transport(
    name: &str,
    params: &mut serde_json::Map<String, serde_json::Value>,
) -> Result<(), EngineError> {
    if let Some(native) = params.remove("transport") {
        take_flat(
            params,
            &[
                "network",
                "path",
                "host",
                "headers",
                "serviceName",
                "service_name",
            ],
        );
        take_flat(params, &["method"]);
        if native.is_object() {
            params.insert("transport".to_string(), native);
        } else {
            return Err(bad_option(
                name,
                "transport",
                "transport must be a JSON object",
            ));
        }
        return Ok(());
    }
    let network = take_flat(params, &["network"]).unwrap_or_default();
    let path = take_flat(params, &["path"]);
    let host = take_flat(params, &["host"]);

    let headers_trivial = match params.get("headers") {
        None | Some(serde_json::Value::Null) => true,
        Some(serde_json::Value::String(text)) => text.trim().is_empty(),
        Some(serde_json::Value::Object(map)) => map.is_empty(),
        Some(serde_json::Value::Array(items)) => items.is_empty(),
        Some(_) => false,
    };
    if !headers_trivial {
        return Err(bad_option(
            name,
            "headers",
            "custom transport headers have no sing-box 1.14.2 mapping on this protocol (only `host` maps to the Host header)",
        ));
    }
    take_flat(params, &["headers"]);
    let service_name = take_flat(params, &["serviceName", "service_name"]).or(path.clone());
    let method = take_flat(params, &["method"]);
    let transport = match network.to_ascii_lowercase().as_str() {
        "" | "tcp" | "raw" | "none" => None,

        "quic" => {
            let quic = serde_json::Map::with_capacity(1);
            Some((serde_json::Value::String("quic".to_string()), quic))
        }
        "ws" | "websocket" => {
            let mut ws = serde_json::Map::with_capacity(3);
            if let Some(path) = path {
                ws.insert("path".to_string(), serde_json::Value::String(path));
            }
            if let Some(host) = host {
                ws.insert("headers".to_string(), serde_json::json!({"Host": host}));
            }
            Some((serde_json::Value::String("ws".to_string()), ws))
        }
        "grpc" | "gun" => {
            let mut grpc = serde_json::Map::with_capacity(1);

            let service = service_name.or(path).or(host);
            if let Some(service) = service {
                grpc.insert(
                    "service_name".to_string(),
                    serde_json::Value::String(service),
                );
            }
            Some((serde_json::Value::String("grpc".to_string()), grpc))
        }
        "http" => {
            let mut http = serde_json::Map::with_capacity(3);
            if let Some(host) = host {
                http.insert("host".to_string(), serde_json::Value::String(host));
            }
            if let Some(path) = path {
                http.insert("path".to_string(), serde_json::Value::String(path));
            }
            if let Some(method) = method {
                http.insert("method".to_string(), serde_json::Value::String(method));
            }
            Some((serde_json::Value::String("http".to_string()), http))
        }
        "httpupgrade" => {
            let mut upgrade = serde_json::Map::with_capacity(2);
            if let Some(host) = host {
                upgrade.insert("host".to_string(), serde_json::Value::String(host));
            }
            if let Some(path) = path {
                upgrade.insert("path".to_string(), serde_json::Value::String(path));
            }
            Some((
                serde_json::Value::String("httpupgrade".to_string()),
                upgrade,
            ))
        }
        "xhttp" => {
            let mut xhttp = serde_json::Map::with_capacity(3);
            if let Some(host) = host {
                xhttp.insert("host".to_string(), serde_json::Value::String(host));
            }
            if let Some(path) = path {
                xhttp.insert("path".to_string(), serde_json::Value::String(path));
            }
            if let Some(mode) = take_flat(params, &["mode"]) {
                xhttp.insert("mode".to_string(), serde_json::Value::String(mode));
            }
            Some((serde_json::Value::String("xhttp".to_string()), xhttp))
        }
        other => {
            return Err(bad_option(
                name,
                "network",
                &format!("transport \"{other}\" has no stock sing-box mapping (want tcp, ws, grpc, http, httpupgrade, or xhttp)"),
            ));
        }
    };
    if let Some((kind, mut body)) = transport {
        body.insert("type".to_string(), kind);
        params.insert("transport".to_string(), serde_json::Value::Object(body));
    }
    Ok(())
}

/// Whether the stored params declare multiplex — the exact params-key rule
/// [`apply_mux`] consumes, so UI/health consumers cannot drift from what the
/// generated config really says. Gated by the `mux` key: only a string value
/// counts, a trimmed `"on"` (any ASCII case) enables the row, `"off"` keeps it
/// disabled, blank or non-string values declare nothing, and any other
/// spelling fails config generation in `apply_mux`. Malformed or non-object
/// JSON — which `params_object` balks at — never declares.
pub fn params_declare_mux(params_raw: &str) -> bool {
    let Ok(serde_json::Value::Object(params)) =
        serde_json::from_str::<serde_json::Value>(params_raw)
    else {
        return false;
    };
    let Some(serde_json::Value::String(mode)) = params.get("mux") else {
        return false;
    };
    let mode = mode.trim();
    !mode.is_empty() && mode.eq_ignore_ascii_case("on")
}

fn apply_mux(
    name: &str,
    params: &mut serde_json::Map<String, serde_json::Value>,
) -> Result<(), EngineError> {
    let mode = take_flat(params, &["mux"]);

    take_truthy(params, &["brutal_enable"]);
    take_flat(
        params,
        &[
            "brutal_up_mbps",
            "brutal_down_mbps",
            "brutal_up",
            "brutal_down",
            "brutal_upmbps",
            "brutal_downmbps",
        ],
    );
    let mode = mode.unwrap_or_default();
    if mode.trim().is_empty() {
        return Ok(());
    }
    let mut multiplex = serde_json::Map::new();
    if mode.eq_ignore_ascii_case("on") {
        multiplex.insert("enabled".to_string(), serde_json::Value::Bool(true));
    } else if mode.eq_ignore_ascii_case("off") {
        multiplex.insert("enabled".to_string(), serde_json::Value::Bool(false));
    } else {
        return Err(bad_option(
            name,
            "mux",
            "\"mux\" must be \"\" (default), \"on\" or \"off\"",
        ));
    }
    params.insert(
        "multiplex".to_string(),
        serde_json::Value::Object(multiplex),
    );
    Ok(())
}

fn drop_foreign_flats(params: &mut serde_json::Map<String, serde_json::Value>, keep: &[&str]) {
    for key in [
        "security",
        "sni",
        "peer",
        "fp",
        "fingerprint",
        "alpn",
        "pbk",
        "sid",
        "network",
        "path",
        "host",
        "headers",
        "serviceName",
        "service_name",
        "flow",
        "tls",
        "encryption",
        "alter_id",
        "header_type",
        "mode",
        "method",
        "transport",
        "variant",
        "allowInsecure",
        "allow_insecure",
        "allowinsecure",
        "insecure",
        "mux",
        "brutal_enable",
        "brutal_up_mbps",
        "brutal_down_mbps",
        "brutal_up",
        "brutal_down",
        "brutal_upmbps",
        "brutal_downmbps",
        "ech",
        "ech_config",
        "echconfig",
        "ech_config_path",
        "echconfigpath",
        "up_mbps",
        "down_mbps",
        "upmbps",
        "downmbps",
        "up",
        "down",
        "obfs",
        "obfs_password",
        "obfs-password",
        "obfs_host",
        "obfs-host",
        "psk",
    ] {
        if !keep.contains(&key) {
            params.remove(key);
        }
    }
}

fn translate_outbound(
    canonical: &str,
    name: &str,
    params: &mut serde_json::Map<String, serde_json::Value>,
) -> Result<(), EngineError> {
    reject_115_fields(name, params)?;
    let server = params
        .get("server")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|server| !server.is_empty())
        .unwrap_or_default()
        .to_string();
    match canonical {
        "vless" | "trojan" => {
            apply_tls(name, params, &server, canonical == "trojan", false)?;
            apply_transport(name, params)?;

            apply_mux(name, params)?;

            let allowed: &[&str] = if canonical == "vless" {
                // `password`/`token` are importer credential keys the library tolerates
                // on vless rows; every other key must map to a sing-box 1.14.2 field.
                &[
                    "uuid",
                    "flow",
                    "tls",
                    "transport",
                    "multiplex",
                    "packet_encoding",
                    "password",
                    "token",
                ]
            } else {
                &[
                    "password",
                    "tls",
                    "transport",
                    "multiplex",
                    "packet_encoding",
                ]
            };
            reject_unknown_params(name, canonical, params, allowed, true)?;
        }
        "vmess" => {
            if params.get("tls").is_some_and(|tls| tls.is_boolean()) {
                let on = params
                    .remove("tls")
                    .and_then(|tls| tls.as_bool())
                    .unwrap_or(false);
                if on {
                    let server_name = take_flat(params, &["sni"]);
                    let fingerprint = take_flat(params, &["fp", "fingerprint"]);
                    let alpn = split_alpn(take_flat(params, &["alpn"]));

                    let insecure = take_truthy(
                        params,
                        &[
                            "allowInsecure",
                            "allow_insecure",
                            "allowinsecure",
                            "insecure",
                        ],
                    );
                    params.insert(
                        "tls".to_string(),
                        tls_object(
                            &server,
                            server_name,
                            fingerprint,
                            alpn,
                            None,
                            insecure,
                            None,
                        ),
                    );
                }

                take_flat(params, &["pbk", "sid", "peer"]);
            } else {
                let cipher =
                    take_flat(params, &["encryption"]).unwrap_or_else(|| "auto".to_string());
                apply_tls(name, params, &server, false, false)?;
                params.insert("security".to_string(), serde_json::Value::String(cipher));
            }

            let aid = take_flat(params, &["aid"]);
            if params.get("alter_id").is_none() {
                if let Some(aid) = aid {
                    params.insert("alter_id".to_string(), serde_json::Value::String(aid));
                }
            }
            match params.remove("alter_id") {
                None => {}
                Some(serde_json::Value::Number(number)) => {
                    params.insert("alter_id".to_string(), serde_json::Value::Number(number));
                }
                Some(serde_json::Value::String(text)) => {
                    let number: u64 = text.trim().parse().map_err(|_| {
                        bad_option(name, "alter_id", "vmess \"alter_id\" must be a number")
                    })?;
                    params.insert(
                        "alter_id".to_string(),
                        serde_json::Value::Number(number.into()),
                    );
                }
                Some(_) => {
                    return Err(bad_option(
                        name,
                        "alter_id",
                        "vmess \"alter_id\" must be a number",
                    ));
                }
            }
            let header = take_flat(params, &["header_type"]);
            if header.is_some_and(|header| !header.eq_ignore_ascii_case("none")) {
                return Err(bad_option(
                    name,
                    "header_type",
                    "vmess TCP header obfuscation has no stock sing-box mapping (use a stream transport)",
                ));
            }
            apply_transport(name, params)?;

            apply_mux(name, params)?;

            reject_unknown_params(
                name,
                "vmess",
                params,
                &[
                    "uuid",
                    "alter_id",
                    "security",
                    "tls",
                    "transport",
                    "multiplex",
                    "packet_encoding",
                    "encryption",
                ],
                true,
            )?;
        }
        "tuic" | "hysteria" | "anytls" => {
            apply_tls(name, params, &server, true, true)?;

            if canonical == "tuic" {
                take_flat(
                    params,
                    &[
                        "path",
                        "host",
                        "headers",
                        "serviceName",
                        "service_name",
                        "method",
                    ],
                );
            } else {
                take_flat(
                    params,
                    &[
                        "network",
                        "path",
                        "host",
                        "headers",
                        "serviceName",
                        "service_name",
                        "method",
                    ],
                );
            }

            if canonical == "anytls" {
                if params.get("idle_session_check_interval").is_none() {
                    if let Some(interval) = take_flat(params, &["interval"]) {
                        params.insert(
                            "idle_session_check_interval".to_string(),
                            serde_json::Value::String(interval),
                        );
                    }
                } else {
                    params.remove("interval");
                }
                if params.get("idle_session_timeout").is_none() {
                    if let Some(timeout) = take_flat(params, &["timeout"]) {
                        params.insert(
                            "idle_session_timeout".to_string(),
                            serde_json::Value::String(timeout),
                        );
                    }
                } else {
                    params.remove("timeout");
                }
                if params.get("min_idle_session").is_none() {
                    if let Some(min) = take_flat(params, &["min"]) {
                        let coerced = match min.trim().parse::<u64>() {
                            Ok(number) => serde_json::Value::Number(number.into()),
                            Err(_) => serde_json::Value::String(min),
                        };
                        params.insert("min_idle_session".to_string(), coerced);
                    }
                } else {
                    params.remove("min");
                }
            }
            if canonical == "hysteria" {
                let auth = take_flat(params, &["auth"]);
                let password = take_flat(params, &["password"]);
                if params.get("auth_str").is_none() {
                    if let Some(auth) = auth {
                        params.insert("auth_str".to_string(), serde_json::Value::String(auth));
                    } else if let Some(password) = password {
                        params.insert("auth_str".to_string(), serde_json::Value::String(password));
                    }
                }

                for field in ["up", "down"] {
                    let present = params
                        .get(field)
                        .and_then(|value| match value {
                            serde_json::Value::String(text) => Some(!text.trim().is_empty()),
                            serde_json::Value::Number(_) => Some(true),
                            _ => None,
                        })
                        .unwrap_or(false);
                    if !present {
                        return Err(EngineError::BadParams {
                            name: name.to_string(),
                            field: if field == "up" { "up" } else { "down" },
                        });
                    }
                }
            }
            if canonical == "tuic" {
                if take_truthy(params, &["zero_rtt_handshake", "zero_rtt"]) {
                    params.insert(
                        "zero_rtt_handshake".to_string(),
                        serde_json::Value::Bool(true),
                    );
                }
                reject_unknown_params(
                    name,
                    "tuic",
                    params,
                    &[
                        "uuid",
                        "password",
                        "network",
                        "congestion_control",
                        "udp_relay_mode",
                        "udp_over_stream",
                        "zero_rtt_handshake",
                        "heartbeat",
                        "tls",
                    ],
                    false,
                )?;
            }
        }
        "hysteria2" => {
            apply_tls(name, params, &server, true, true)?;
            if let Some(variant) = take_flat(params, &["variant"]) {
                return Err(bad_option(
                    name,
                    "variant",
                    &format!("hysteria2 variant \"{variant}\" has no stock sing-box mapping"),
                ));
            }

            let auth = take_flat(params, &["auth"]);
            if params.get("password").is_none() {
                if let Some(auth) = auth {
                    params.insert("password".to_string(), serde_json::Value::String(auth));
                }
            }
            if str_field(params, "password").is_none() {
                return Err(EngineError::BadParams {
                    name: name.to_string(),
                    field: "password",
                });
            }
            if let Some(up) = take_speed(name, params, &["up_mbps", "upmbps", "up"], "up_mbps")? {
                params.insert("up_mbps".to_string(), serde_json::Value::Number(up.into()));
            }
            if let Some(down) = take_speed(
                name,
                params,
                &["down_mbps", "downmbps", "down"],
                "down_mbps",
            )? {
                params.insert(
                    "down_mbps".to_string(),
                    serde_json::Value::Number(down.into()),
                );
            }
            let obfs_raw = params.remove("obfs");
            let obfs_pw = take_flat(params, &["obfs_password", "obfs-password"]);
            match obfs_raw {
                None => {
                    if obfs_pw.is_some() {
                        return Err(bad_option(
                            name,
                            "obfs",
                            "hysteria2 obfs password without an obfs type (want \"salamander\" or \"gecko\")",
                        ));
                    }
                }
                Some(serde_json::Value::Object(map)) => {
                    params.insert("obfs".to_string(), serde_json::Value::Object(map));
                }
                Some(serde_json::Value::String(kind)) => {
                    match kind.trim().to_ascii_lowercase().as_str() {
                        "" | "none" | "off" => {
                            if obfs_pw.is_some() {
                                return Err(bad_option(
                                    name,
                                    "obfs",
                                    "hysteria2 obfs password without an obfs type (want \"salamander\" or \"gecko\")",
                                ));
                            }
                        }
                        "salamander" | "gecko" => {
                            let mut obfs = serde_json::Map::with_capacity(2);
                            obfs.insert(
                                "type".to_string(),
                                serde_json::Value::String(kind.trim().to_ascii_lowercase()),
                            );
                            if let Some(password) = obfs_pw {
                                obfs.insert(
                                    "password".to_string(),
                                    serde_json::Value::String(password),
                                );
                            }
                            params.insert("obfs".to_string(), serde_json::Value::Object(obfs));
                        }
                        other => {
                            return Err(bad_option(
                                name,
                                "obfs",
                                &format!(
                                    "unknown hysteria2 obfs \"{other}\" (want \"salamander\" or \"gecko\")"
                                ),
                            ));
                        }
                    }
                }
                Some(_) => {
                    return Err(bad_option(
                        name,
                        "obfs",
                        "hysteria2 obfs must be an object or a type name",
                    ));
                }
            }

            take_flat(
                params,
                &[
                    "path",
                    "host",
                    "headers",
                    "serviceName",
                    "service_name",
                    "method",
                ],
            );
            reject_unknown_params(
                name,
                "hysteria2",
                params,
                &[
                    "password",
                    "up_mbps",
                    "down_mbps",
                    "obfs",
                    "network",
                    "server_ports",
                    "hop_interval",
                    "hop_interval_max",
                    "bbr_profile",
                    "disable_chrome_parrot",
                    "realm",
                    "tls",
                ],
                false,
            )?;
        }
        "socks" => {
            match params.remove("version") {
                None => {}
                Some(serde_json::Value::Number(number)) if number.as_u64() == Some(5) => {}
                Some(serde_json::Value::String(version)) if version.trim() == "5" => {}
                Some(serde_json::Value::String(version)) => {
                    return Err(bad_option(
                        name,
                        "version",
                        &format!(
                            "socks version \"{}\" has no verified 1.14.2 mapping (stock serves version 5)",
                            version.trim()
                        ),
                    ));
                }
                Some(_) => {
                    return Err(bad_option(
                        name,
                        "version",
                        "socks version must be 5 (stock serves version 5)",
                    ));
                }
            }
            if let Some(serde_json::Value::String(network)) = params.get("network") {
                if !["tcp", "udp"].contains(&network.trim().to_ascii_lowercase().as_str()) {
                    return Err(bad_option(
                        name,
                        "network",
                        "socks network must be \"tcp\" or \"udp\"",
                    ));
                }
            }
            drop_foreign_flats(params, &["username", "password", "network", "udp_over_tcp"]);
            reject_unknown_params(
                name,
                "socks",
                params,
                &["username", "password", "network", "udp_over_tcp"],
                false,
            )?;
        }
        "http" => {
            if params.contains_key("version") {
                return Err(bad_option(
                    name,
                    "version",
                    "http version selection is sing-box 1.15.0+ (1.14.2 serves its default)",
                ));
            }
            match params.remove("tls") {
                None => apply_tls(name, params, &server, false, false)?,
                Some(serde_json::Value::Object(map)) => {
                    take_flat(
                        params,
                        &[
                            "security",
                            "sni",
                            "peer",
                            "fp",
                            "fingerprint",
                            "alpn",
                            "pbk",
                            "sid",
                        ],
                    );
                    take_truthy(
                        params,
                        &[
                            "allowInsecure",
                            "allow_insecure",
                            "allowinsecure",
                            "insecure",
                        ],
                    );
                    take_flat(params, &["ech", "ech_config", "echconfig"]);
                    take_flat(params, &["ech_config_path", "echconfigpath"]);
                    params.insert("tls".to_string(), serde_json::Value::Object(map));
                }
                Some(_) => {
                    return Err(bad_option(name, "tls", "http tls must be an object"));
                }
            }
            drop_foreign_flats(params, &["username", "password", "path", "headers", "tls"]);
            reject_unknown_params(
                name,
                "http",
                params,
                &["username", "password", "path", "headers", "tls"],
                false,
            )?;
        }
        "naive" => {
            for field in ["username", "password"] {
                if str_field(params, field).is_none() {
                    return Err(EngineError::BadParams {
                        name: name.to_string(),
                        field,
                    });
                }
            }
            apply_tls(name, params, &server, true, true)?;

            if take_truthy(params, &["uot"]) && params.get("udp_over_tcp").is_none() {
                params.insert("udp_over_tcp".to_string(), serde_json::Value::Bool(true));
            }
            if params.get("quic_congestion_control").is_none() {
                if let Some(congestion) = take_flat(params, &["congestion_control"]) {
                    params.insert(
                        "quic_congestion_control".to_string(),
                        serde_json::Value::String(congestion),
                    );
                }
            } else {
                params.remove("congestion_control");
            }
            drop_foreign_flats(
                params,
                &[
                    "username",
                    "password",
                    "network",
                    "tls",
                    "udp_over_tcp",
                    "quic",
                    "quic_congestion_control",
                ],
            );
            reject_unknown_params(
                name,
                "naive",
                params,
                &[
                    "username",
                    "password",
                    "network",
                    "tls",
                    "udp_over_tcp",
                    "quic",
                    "quic_congestion_control",
                ],
                false,
            )?;
        }
        "snell" => {
            if str_field(params, "psk").is_none() {
                return Err(EngineError::BadParams {
                    name: name.to_string(),
                    field: "psk",
                });
            }
            let version = match params.remove("version") {
                None => 4u64,
                Some(serde_json::Value::Number(number)) => number
                    .as_u64()
                    .filter(|version| *version == 4 || *version == 6)
                    .ok_or_else(|| bad_option(name, "version", "snell version must be 4 or 6"))?,
                Some(serde_json::Value::String(text)) => match text.trim() {
                    "4" => 4,
                    "6" => 6,
                    _ => {
                        return Err(bad_option(name, "version", "snell version must be 4 or 6"));
                    }
                },
                Some(_) => {
                    return Err(bad_option(name, "version", "snell version must be 4 or 6"));
                }
            };
            params.insert(
                "version".to_string(),
                serde_json::Value::Number(version.into()),
            );
            drop_foreign_flats(
                params,
                &[
                    "psk",
                    "version",
                    "userkey",
                    "reuse",
                    "network",
                    "obfs_mode",
                    "obfs_host",
                    "mode",
                ],
            );
            reject_unknown_params(
                name,
                "snell",
                params,
                &[
                    "psk",
                    "version",
                    "userkey",
                    "reuse",
                    "network",
                    "obfs_mode",
                    "obfs_host",
                    "mode",
                ],
                false,
            )?;
        }
        "shadowtls" => {
            apply_tls(name, params, &server, true, true)?;
            match params.remove("version") {
                None => {}
                Some(serde_json::Value::Number(number)) => {
                    let valid = number
                        .as_u64()
                        .is_some_and(|version| (1..=3).contains(&version));
                    if !valid {
                        return Err(bad_option(
                            name,
                            "version",
                            "shadowtls version must be 1, 2, or 3",
                        ));
                    }
                    params.insert("version".to_string(), serde_json::Value::Number(number));
                }
                Some(serde_json::Value::String(text)) => match text.trim() {
                    "1" | "2" | "3" => {
                        let number: u64 = text.trim().parse().expect("1|2|3 parses");
                        params.insert(
                            "version".to_string(),
                            serde_json::Value::Number(number.into()),
                        );
                    }
                    _ => {
                        return Err(bad_option(
                            name,
                            "version",
                            "shadowtls version must be 1, 2, or 3",
                        ));
                    }
                },
                Some(_) => {
                    return Err(bad_option(
                        name,
                        "version",
                        "shadowtls version must be 1, 2, or 3",
                    ));
                }
            }
            drop_foreign_flats(params, &["version", "password", "tls"]);
            reject_unknown_params(
                name,
                "shadowtls",
                params,
                &["version", "password", "tls"],
                false,
            )?;
        }
        "ssh" => {
            if params.get("user").is_none() {
                if let Some(user) = take_flat(params, &["username"]) {
                    params.insert("user".to_string(), serde_json::Value::String(user));
                }
            } else {
                params.remove("username");
            }

            if params.get("private_key_passphrase").is_none() {
                if let Some(pass) = take_flat(params, &["private_key_pass"]) {
                    params.insert(
                        "private_key_passphrase".to_string(),
                        serde_json::Value::String(pass),
                    );
                }
            } else {
                params.remove("private_key_pass");
            }
            drop_foreign_flats(params, &["user"]);
        }
        "wireguard" => {
            drop_foreign_flats(params, &[]);
        }
        "shadowsocks" => {
            if take_flat(params, &["plugin"]).is_some() {
                return Err(bad_option(
                    name,
                    "plugin",
                    "shadowsocks plugins need their executable beside the engine (stock check fails without it)",
                ));
            }

            apply_mux(name, params)?;

            if take_truthy(params, &["uot"]) && params.get("udp_over_tcp").is_none() {
                params.insert("udp_over_tcp".to_string(), serde_json::Value::Bool(true));
            }
            drop_foreign_flats(params, &["method"]);
        }
        _ => {}
    }

    if canonical != "vless" {
        params.remove("flow");
    }
    Ok(())
}

fn reshape_wireguard_endpoint(
    name: &str,
    params: &mut serde_json::Map<String, serde_json::Value>,
    server_port: u16,
) -> Result<(), EngineError> {
    let missing = |field: &'static str| EngineError::BadParams {
        name: name.to_string(),
        field,
    };
    let take_key = |params: &mut serde_json::Map<String, serde_json::Value>,
                    keys: &[&str]|
     -> Option<serde_json::Value> {
        keys.iter()
            .find_map(|key| params.remove(*key))
            .filter(|value| value.as_str().is_none_or(|text| !text.trim().is_empty()))
    };
    let server = params
        .get("server")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|server| !server.is_empty())
        .ok_or_else(|| missing("server"))?
        .to_string();
    let public_key = take_key(params, &["public_key", "publickey"])
        .and_then(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|key| !key.is_empty())
                .map(str::to_string)
        })
        .ok_or_else(|| missing("public_key"))?;
    if let Some(private_key) = take_key(params, &["private_key", "privatekey"]) {
        params.insert("private_key".to_string(), private_key);
    } else {
        return Err(missing("private_key"));
    }
    let as_list = |value: serde_json::Value| -> serde_json::Value {
        match value {
            serde_json::Value::Array(_) => value,
            serde_json::Value::String(text) => serde_json::Value::Array(
                text.split([',', ' '])
                    .map(str::trim)
                    .filter(|entry| !entry.is_empty())
                    .map(|entry| serde_json::Value::String(entry.to_string()))
                    .collect(),
            ),
            other => serde_json::Value::Array(vec![other]),
        }
    };

    if let Some(address) = params.remove("address") {
        params.insert("address".to_string(), as_list(address));
    }
    let mut peer = serde_json::Map::with_capacity(6);
    peer.insert("address".to_string(), serde_json::Value::String(server));
    peer.insert(
        "port".to_string(),
        serde_json::Value::Number(server_port.into()),
    );
    peer.insert(
        "public_key".to_string(),
        serde_json::Value::String(public_key),
    );
    if let Some(pre_shared) = take_key(params, &["pre_shared_key", "presharedkey"]) {
        peer.insert("pre_shared_key".to_string(), pre_shared);
    }
    if let Some(allowed) = take_key(params, &["allowed_ips", "allowedips"]) {
        peer.insert("allowed_ips".to_string(), as_list(allowed));
    } else {
        peer.insert(
            "allowed_ips".to_string(),
            serde_json::Value::Array(vec![serde_json::Value::String("0.0.0.0/0".to_string())]),
        );
    }

    if let Some(keepalive) = take_key(
        params,
        &["persistent_keepalive", "persistent_keepalive_interval"],
    ) {
        let coerced = match keepalive {
            serde_json::Value::String(text) => text
                .trim()
                .parse::<u64>()
                .map(serde_json::Value::from)
                .unwrap_or_else(|_| serde_json::Value::String(text)),
            other => other,
        };
        peer.insert("persistent_keepalive_interval".to_string(), coerced);
    }
    params.remove("server");
    params.remove("server_port");

    params.remove("awg_version");

    params.remove("enable_amnezia");

    params.remove("dns");

    if let Some(reserved) = params.remove("reserved") {
        let coerced = match reserved {
            serde_json::Value::String(text) => text
                .split([',', ' '])
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(|entry| entry.parse::<u64>())
                .collect::<Result<Vec<_>, _>>()
                .map(|bytes| {
                    serde_json::Value::Array(
                        bytes.into_iter().map(serde_json::Value::from).collect(),
                    )
                })
                .unwrap_or_else(|_| serde_json::Value::String(text)),
            other => other,
        };
        params.insert("reserved".to_string(), coerced);
    }

    if let Some(number) = params
        .get("mtu")
        .and_then(serde_json::Value::as_str)
        .and_then(|text| text.trim().parse::<u64>().ok())
    {
        params.insert("mtu".to_string(), serde_json::Value::Number(number.into()));
    }

    for key in [
        "jc",
        "jmin",
        "jmax",
        "s1",
        "s2",
        "s3",
        "s4",
        "h1",
        "h2",
        "h3",
        "h4",
        "workers",
        "maxhandshakeattempts",
    ] {
        let number = params
            .get(key)
            .and_then(serde_json::Value::as_str)
            .and_then(|text| text.trim().parse::<u64>().ok());
        if let Some(number) = number {
            params.insert(key.to_string(), serde_json::Value::Number(number.into()));
        }
    }
    params.insert(
        "peers".to_string(),
        serde_json::Value::Array(vec![serde_json::Value::Object(peer)]),
    );
    Ok(())
}

struct RowBuilt {
    nodes: Vec<(serde_json::Value, bool)>,
    serving_tag: String,
}

fn build_ticket08_row(
    config: &ProxyConfig,
    params: &serde_json::Map<String, serde_json::Value>,
    bridges: &HashMap<String, u16>,
    configs: &[ProxyConfig],
) -> Result<Option<RowBuilt>, EngineError> {
    let out_tag = outbound_tag(&config.id);
    if is_extra_core_protocol(&config.protocol) {
        return Err(EngineError::BadExtra {
            name: config.name.clone(),
            field: "exe".to_string(),
            detail: "extra-core rows run in the user-executable sidecar, not the main config"
                .to_string(),
        });
    }
    if is_custom_config_protocol(&config.protocol) {
        return Err(EngineError::BadCustom {
            name: config.name.clone(),
            field: "config".to_string(),
            detail: "custom full-config rows replace their core's config instead of joining the main config".to_string(),
        });
    }

    if is_xray_protocol(&config.protocol) || vless_needs_xray(&config.protocol, params) {
        check_wrong_layer(&config.name, &config.protocol, params)?;
        validate_xray_row(&config.name, params)?;
        let port = *bridges
            .get(&config.id)
            .ok_or_else(|| EngineError::InvalidGenerated {
                detail: format!("no bridge port assigned for \"{}\"", config.name),
            })?;
        return Ok(Some(RowBuilt {
            nodes: vec![(bridge_pointer(&out_tag, port), false)],
            serving_tag: out_tag,
        }));
    }
    if is_custom_outbound_protocol(&config.protocol) {
        let (core, mut map) = validate_custom_outbound(&config.name, params)?;
        if core == OwningCore::Xray {
            let port = *bridges
                .get(&config.id)
                .ok_or_else(|| EngineError::InvalidGenerated {
                    detail: format!("no bridge port assigned for \"{}\"", config.name),
                })?;
            return Ok(Some(RowBuilt {
                nodes: vec![(bridge_pointer(&out_tag, port), false)],
                serving_tag: out_tag,
            }));
        }
        map.insert(
            "tag".to_string(),
            serde_json::Value::String(out_tag.clone()),
        );
        let is_endpoint = map
            .get("type")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|outbound_type| outbound_type.eq_ignore_ascii_case("wireguard"));
        return Ok(Some(RowBuilt {
            nodes: vec![(serde_json::Value::Object(map), is_endpoint)],
            serving_tag: out_tag,
        }));
    }
    if is_chain_protocol(&config.protocol) {
        check_wrong_layer(&config.name, &config.protocol, params)?;
        let hops = validate_chain_nodes(&config.name, params, &|id| {
            configs
                .iter()
                .find(|row| row.id == id)
                .map(|row| row.protocol.clone())
        })?;
        let mut nodes = Vec::with_capacity(hops.len());
        let mut hop_tags = Vec::with_capacity(hops.len());
        for (index, _) in hops.iter().enumerate() {
            let hop_tag = format!("{out_tag}-hop-{index}");
            hop_tags.push(hop_tag.clone());
        }
        for (index, hop) in hops.iter().enumerate() {
            let hop_tag = hop_tags[index].clone();
            let node = match (&hop.ref_id, &hop.node) {
                (Some(id), None) => {
                    let target = configs.iter().find(|row| row.id == *id).ok_or_else(|| {
                        EngineError::BadCustom {
                            name: config.name.clone(),
                            field: "nodes".to_string(),
                            detail: format!(
                                "{} references unknown id \"{id}\" in this sidecar config",
                                hop.label
                            ),
                        }
                    })?;
                    if is_extra_core_protocol(&target.protocol)
                        || is_custom_config_protocol(&target.protocol)
                        || is_chain_protocol(&target.protocol)
                    {
                        return Err(EngineError::BadCustom {
                            name: config.name.clone(),
                            field: "nodes".to_string(),
                            detail: format!(
                                "{} references \"{id}\", which runs outside the main sidecar (never a chain hop)",
                                hop.label
                            ),
                        });
                    }

                    let target_bridged = is_xray_protocol(&target.protocol)
                        || params_object(target)
                            .ok()
                            .is_some_and(|map| vless_needs_xray(&target.protocol, &map));
                    if needs_fork(target) && !target_bridged {
                        return Err(EngineError::BadCustom {
                            name: config.name.clone(),
                            field: "nodes".to_string(),
                            detail: format!(
                                "{} references fork-sidecar row \"{id}\" (v1 chains stay within one sidecar)",
                                hop.label
                            ),
                        });
                    }
                    if target_bridged {
                        let port = *bridges.get(&target.id).ok_or_else(|| {
                            EngineError::InvalidGenerated {
                                detail: format!("no bridge port assigned for \"{}\"", target.name),
                            }
                        })?;
                        bridge_pointer(&hop_tag, port)
                    } else if is_custom_outbound_protocol(&target.protocol) {
                        let target_params = params_object(target)?;
                        let (core, mut hop_map) =
                            validate_custom_outbound(&target.name, &target_params)?;
                        if core == OwningCore::Xray {
                            let port = *bridges.get(&target.id).ok_or_else(|| {
                                EngineError::InvalidGenerated {
                                    detail: format!(
                                        "no bridge port assigned for \"{}\"",
                                        target.name
                                    ),
                                }
                            })?;
                            bridge_pointer(&hop_tag, port)
                        } else {
                            hop_map.insert(
                                "tag".to_string(),
                                serde_json::Value::String(hop_tag.clone()),
                            );
                            if index + 1 < hops.len() {
                                hop_map.insert(
                                    "detour".to_string(),
                                    serde_json::Value::String(hop_tags[index + 1].clone()),
                                );
                            }
                            serde_json::Value::Object(hop_map)
                        }
                    } else {
                        let mut hop_params = params_object(target)?;
                        let (mut node, _, _) =
                            build_standard_node(target, &mut hop_params, &hop_tag)?;

                        if index + 1 < hops.len() {
                            node.as_object_mut().expect("hop node is an object").insert(
                                "detour".to_string(),
                                serde_json::Value::String(hop_tags[index + 1].clone()),
                            );
                        }
                        node
                    }
                }
                (None, Some(map)) => {
                    if hop.kind == ChainHopKind::Xray {
                        let port = *bridges
                            .get(&format!("{}#hop{}", config.id, index))
                            .ok_or_else(|| EngineError::InvalidGenerated {
                                detail: format!(
                                    "no bridge port assigned for \"{}\" {}",
                                    config.name, hop.label
                                ),
                            })?;
                        bridge_pointer(&hop_tag, port)
                    } else {
                        let mut hop_map = map.clone();
                        hop_map.insert(
                            "tag".to_string(),
                            serde_json::Value::String(hop_tag.clone()),
                        );
                        if index + 1 < hops.len() {
                            hop_map.insert(
                                "detour".to_string(),
                                serde_json::Value::String(hop_tags[index + 1].clone()),
                            );
                        }
                        serde_json::Value::Object(hop_map)
                    }
                }
                _ => {
                    return Err(EngineError::InvalidGenerated {
                        detail: format!("chain hop without a body (\"{}\")", config.name),
                    });
                }
            };
            let is_endpoint = node
                .get("type")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|outbound_type| outbound_type.eq_ignore_ascii_case("wireguard"));
            nodes.push((node, is_endpoint));
        }
        let serving_tag = hop_tags.first().cloned().expect("chain has hops");
        return Ok(Some(RowBuilt { nodes, serving_tag }));
    }
    Ok(None)
}

fn build_main_config(
    configs: &[ProxyConfig],
    bridges: &HashMap<String, u16>,
    presets: &HashMap<String, Vec<serde_json::Value>>,
    rule_set_files: &HashMap<String, PathBuf>,
) -> Result<GeneratedConfig, EngineError> {
    if configs.is_empty() {
        return Err(EngineError::NoActiveConfigs);
    }
    let mut inbounds = Vec::with_capacity(configs.len());
    let mut outbounds: Vec<serde_json::Value> = Vec::with_capacity(configs.len() + 1);
    let mut endpoints = Vec::new();
    let mut rules = Vec::with_capacity(configs.len());
    let mut staged = RuleSetStage::new(rule_set_files);

    for config in configs {
        let in_tag = inbound_tag(&config.id);
        let out_tag = outbound_tag(&config.id);
        let mut params = params_object(config)?;

        if let Some(built) = build_ticket08_row(config, &params, bridges, configs)? {
            for (node, is_endpoint) in built.nodes {
                if is_endpoint {
                    endpoints.push(node);
                } else {
                    outbounds.push(node);
                }
            }
            inbounds.push(serde_json::json!({
                "type": "mixed",
                "tag": in_tag,
                "listen": config.bind_ip,
                "listen_port": config.port,
            }));

            for preset_rule in
                preset_rules_for(config, &built.serving_tag, &in_tag, presets, &mut staged)?
            {
                rules.push(preset_rule);
            }
            rules.push(serde_json::json!({
                "inbound": [in_tag],
                "action": "route",
                "outbound": built.serving_tag,
            }));
            continue;
        }
        let (node, is_endpoint, _) = build_standard_node(config, &mut params, &out_tag)?;
        if is_endpoint {
            endpoints.push(node);
        } else {
            outbounds.push(node);
        }
        inbounds.push(serde_json::json!({
            "type": "mixed",
            "tag": in_tag,
            "listen": config.bind_ip,
            "listen_port": config.port,
        }));

        for preset_rule in preset_rules_for(config, &out_tag, &in_tag, presets, &mut staged)? {
            rules.push(preset_rule);
        }
        rules.push(serde_json::json!({
            "inbound": [in_tag],
            "action": "route",
            "outbound": out_tag,
        }));
    }
    outbounds.push(serde_json::json!({"type": "direct", "tag": "direct"}));

    let mut route = serde_json::Map::new();
    route.insert("rules".to_string(), serde_json::Value::Array(rules));
    route.insert(
        "final".to_string(),
        serde_json::Value::String("direct".to_string()),
    );
    // T-12: only when a preset rule references staged sets, `route.rule_set`
    // appears — local entries only, paths pointing at the staging location.
    if !staged.entries.is_empty() {
        route.insert(
            "rule_set".to_string(),
            serde_json::Value::Array(staged.entries.clone()),
        );
    }

    let document = serde_json::json!({
        "log": {"level": "warn"},
        "inbounds": inbounds,
        "outbounds": outbounds,
        "endpoints": endpoints,
        "route": route,
    });
    validate_structure(&document)?;
    Ok(GeneratedConfig {
        json: serde_json::to_string(&document).expect("engine config serializes"),
    })
}

fn validate_structure(document: &serde_json::Value) -> Result<(), EngineError> {
    let invalid = |detail: &str| EngineError::InvalidGenerated {
        detail: detail.to_string(),
    };
    if let Some(top) = document.as_object() {
        for forbidden in ["tun", "dns"] {
            if top.contains_key(forbidden) {
                return Err(invalid(&format!("forbidden section \"{forbidden}\"")));
            }
        }
    }
    let inbounds = document
        .get("inbounds")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| invalid("missing \"inbounds\""))?;
    let outbounds = document
        .get("outbounds")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| invalid("missing \"outbounds\""))?;
    let endpoints = document
        .get("endpoints")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| invalid("missing \"endpoints\""))?;
    let mut tags = HashSet::new();
    let mut binds = HashSet::new();
    for inbound in inbounds {
        let obj = inbound.as_object().ok_or_else(|| invalid("bad inbound"))?;
        if obj.get("type").and_then(serde_json::Value::as_str) != Some("mixed") {
            return Err(invalid("inbound is not \"mixed\""));
        }
        let tag = obj
            .get("tag")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| invalid("inbound without tag"))?;
        if !tags.insert(("in", tag.to_string())) {
            return Err(invalid(&format!("duplicate inbound tag \"{tag}\"")));
        }
        let listen = obj
            .get("listen")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| invalid("inbound without listen"))?;
        let port = obj
            .get("listen_port")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| invalid("inbound without listen_port"))?;

        if !binds.insert((normalize_bind_key(listen), port)) {
            return Err(invalid("duplicate inbound bind"));
        }
        if obj.contains_key("set_system_proxy") {
            return Err(invalid("forbidden key \"set_system_proxy\""));
        }
    }
    let mut exits = HashSet::new();
    for node in outbounds.iter().chain(endpoints.iter()) {
        let obj = node.as_object().ok_or_else(|| invalid("bad outbound"))?;
        let tag = obj
            .get("tag")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| invalid("outbound without tag"))?;
        if !tags.insert(("out", tag.to_string())) {
            return Err(invalid(&format!("duplicate outbound tag \"{tag}\"")));
        }
        exits.insert(tag.to_string());
        if obj.contains_key("set_system_proxy") {
            return Err(invalid("forbidden key \"set_system_proxy\""));
        }
    }
    let route = document
        .get("route")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| invalid("missing \"route\""))?;
    let inbound_names: HashSet<&str> = inbounds
        .iter()
        .filter_map(|node| node.get("tag").and_then(serde_json::Value::as_str))
        .collect();
    let rules = route
        .get("rules")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| invalid("missing \"route.rules\""))?;
    for rule in rules {
        let obj = rule.as_object().ok_or_else(|| invalid("bad route rule"))?;

        let inbound_names_list: Vec<&str> = match obj.get("inbound") {
            Some(serde_json::Value::Array(items)) if !items.is_empty() => {
                let mut names = Vec::with_capacity(items.len());
                for item in items {
                    names.push(
                        item.as_str()
                            .ok_or_else(|| invalid("rule inbound must be strings"))?,
                    );
                }
                names
            }
            _ => return Err(invalid("rule without inbound array")),
        };
        let outbound = obj.get("outbound").and_then(serde_json::Value::as_str);

        match obj.get("action").and_then(serde_json::Value::as_str) {
            Some("route") => {
                let outbound = outbound.ok_or_else(|| invalid("rule without outbound"))?;
                if !exits.contains(outbound) {
                    return Err(invalid(&format!("rule references unknown \"{outbound}\"")));
                }
            }
            Some("route-options") => {
                if let Some(outbound) = outbound {
                    if !exits.contains(outbound) {
                        return Err(invalid(&format!("rule references unknown \"{outbound}\"")));
                    }
                }
            }
            Some("direct") | Some("reject") | Some("sniff") | Some("resolve") => {
                if outbound.is_some() {
                    return Err(invalid(
                        "rule with direct/reject/sniff/resolve action carries outbound",
                    ));
                }
            }
            Some("block") => {
                return Err(invalid(
                    "rule action \"block\" must be normalized to \"reject\" before serving",
                ));
            }
            _ => return Err(invalid("rule without route action")),
        }
        for inbound in inbound_names_list {
            if !inbound_names.contains(inbound) {
                return Err(invalid(&format!("rule references unknown \"{inbound}\"")));
            }
        }
    }
    let final_outbound = route
        .get("final")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid("missing \"route.final\""))?;
    if final_outbound != "direct" || !exits.contains(final_outbound) {
        return Err(invalid("route.final must be the direct outbound"));
    }
    Ok(())
}

pub trait EngineBackend {
    fn check_config(&mut self, config_json: &str) -> Result<(), String>;

    fn start(&mut self, config_json: &str) -> Result<(), String>;

    fn stop(&mut self);

    fn is_running(&self) -> bool;

    /// OS pid of the spawned sidecar, when this backend owns one. `None` = not
    /// running, or a backend that never spawns a child.
    fn process_id(&self) -> Option<u32> {
        None
    }

    fn poll_exit(&mut self) -> Option<String>;

    fn drain_log_lines(&mut self) -> Vec<String> {
        Vec::new()
    }

    fn set_extra_exe(&mut self, _exe: PathBuf) {}

    fn clear_extra_exe(&mut self) {}

    fn pin_binary(&mut self, _path: PathBuf) {}

    fn remove_config_file(&self) {}
}

pub fn engine_binary_file_name() -> String {
    if cfg!(windows) {
        format!("{ENGINE_BINARY_BASE}.exe")
    } else {
        ENGINE_BINARY_BASE.to_string()
    }
}

pub(crate) fn current_app_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
}

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn spawn_hidden(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

fn resolve_bundled_in(app_dir: Option<&Path>, file: &str) -> Result<PathBuf, String> {
    match app_dir {
        Some(dir) => {
            let candidate = dir.join(file);
            if is_executable(&candidate) {
                Ok(candidate)
            } else {
                Err(dir.display().to_string())
            }
        }
        None => Err("no app directory".to_string()),
    }
}

pub fn resolve_engine_binary() -> Result<PathBuf, EngineError> {
    let app_dir = current_app_dir();
    let path_env = std::env::var("PATH").unwrap_or_default();
    resolve_engine_binary_in(app_dir.as_deref(), &path_env)
}

pub fn resolve_engine_binary_in(
    app_dir: Option<&Path>,
    _path_env: &str,
) -> Result<PathBuf, EngineError> {
    resolve_bundled_in(app_dir, &engine_binary_file_name()).map_err(|searched| {
        EngineError::MissingBinary {
            binary: engine_binary_file_name(),
            searched,
        }
    })
}

pub fn fork_binary_file_name() -> String {
    if cfg!(windows) {
        format!("{FORK_BINARY_BASE}.exe")
    } else {
        FORK_BINARY_BASE.to_string()
    }
}

pub fn resolve_fork_binary_in(app_dir: Option<&Path>, _path_env: &str) -> Result<PathBuf, String> {
    resolve_bundled_in(app_dir, &fork_binary_file_name())
}

pub fn resolve_fork_binary() -> Result<PathBuf, EngineError> {
    let app_dir = current_app_dir();
    let path_env = std::env::var("PATH").unwrap_or_default();
    resolve_fork_binary_in(app_dir.as_deref(), &path_env).map_err(|searched| {
        EngineError::MissingForkBinary {
            rows: String::new(),
            binary: fork_binary_file_name(),
            searched,
            xhttp: false,
        }
    })
}

pub fn probe_fork_version(binary: &Path) -> Result<String, String> {
    probe_fork_version_with_timeout(binary, FORK_PROBE_TIMEOUT)
}

pub const FORK_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

pub fn probe_fork_version_with_timeout(
    binary: &Path,
    timeout: std::time::Duration,
) -> Result<String, String> {
    use std::io::Read as _;
    let mut cmd = std::process::Command::new(binary);
    cmd.arg("version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    spawn_hidden(&mut cmd);
    let mut child = cmd.spawn().map_err(|error| {
        format!(
            "fork version probe failed for {} ({error})",
            binary.display()
        )
    })?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait().map_err(|error| {
            format!(
                "fork version probe failed for {} ({error})",
                binary.display()
            )
        })? {
            Some(status) => {
                if !status.success() {
                    return Err(format!(
                        "fork version probe failed for {} (non-zero exit)",
                        binary.display()
                    ));
                }
                let mut capped = Vec::new();
                if let Some(stdout) = child.stdout.as_mut() {
                    let _ = stdout.take(64 * 1024).read_to_end(&mut capped);
                }
                let _ = child.wait();
                let first = String::from_utf8_lossy(&capped)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if first.is_empty() {
                    return Err(format!(
                        "fork version probe failed for {} (empty output)",
                        binary.display()
                    ));
                }
                return Ok(truncate_detail(&first));
            }
            None => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "fork version probe timed out for {}",
                        binary.display()
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }
}

pub fn xray_binary_file_name() -> String {
    if cfg!(windows) {
        format!("{XRAY_BINARY_BASE}.exe")
    } else {
        XRAY_BINARY_BASE.to_string()
    }
}

pub fn resolve_xray_binary_in(app_dir: Option<&Path>, _path_env: &str) -> Result<PathBuf, String> {
    resolve_bundled_in(app_dir, &xray_binary_file_name())
}

pub fn resolve_xray_binary() -> Result<PathBuf, EngineError> {
    let app_dir = current_app_dir();
    let path_env = std::env::var("PATH").unwrap_or_default();
    resolve_xray_binary_in(app_dir.as_deref(), &path_env).map_err(|searched| {
        EngineError::MissingXrayBinary {
            rows: String::new(),
            binary: xray_binary_file_name(),
            searched,
        }
    })
}

pub fn resolve_extra_binary_in(app_dir: Option<&Path>, exe: &str) -> Result<PathBuf, String> {
    let name = exe.trim();
    if is_bad_extra_exe_name(name) {
        return Err(format!(
            "bad extra-core exe \"{name}\" (want a plain file name in the app directory)"
        ));
    }
    if std::path::Path::new(name).is_absolute() {
        return Err(format!(
            "bad extra-core exe \"{name}\" (want a plain file name in the app directory)"
        ));
    }
    let Some(dir) = app_dir else {
        return Err("no app directory to resolve the extra-core executable".to_string());
    };
    let candidate = dir.join(name);
    if is_executable(&candidate) {
        Ok(candidate)
    } else {
        Err(dir.display().to_string())
    }
}

pub fn resolve_extra_binary(exe: &str) -> Result<PathBuf, EngineError> {
    let app_dir = current_app_dir();
    resolve_extra_binary_in(app_dir.as_deref(), exe).map_err(|searched| {
        EngineError::MissingExtraBinary {
            rows: String::new(),
            binary: exe.trim().to_string(),
            searched,
        }
    })
}

fn truncate_detail(detail: &str) -> String {
    crate::health::truncate(detail, 1000)
}

pub fn strip_ansi_codes(text: &str) -> String {
    const CSI_FINAL: std::ops::RangeInclusive<char> = '@'..='~';
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(next) = chars.next() {
        if next != 27 as char {
            out.push(next);
            continue;
        }
        match chars.peek() {
            Some('[') => {
                chars.next();
                for code in chars.by_ref() {
                    if CSI_FINAL.contains(&code) {
                        break;
                    }
                }
            }
            Some(_) => {
                chars.next();
            }
            None => {}
        }
    }
    out
}

pub const STALE_TEMP_PREFIX: &str = "manyproxy-";

pub fn sidecar_binary_names() -> Vec<String> {
    let exe = |base: &str| {
        if cfg!(windows) {
            format!("{base}.exe")
        } else {
            base.to_string()
        }
    };
    vec![
        exe(ENGINE_BINARY_BASE),
        exe(FORK_BINARY_BASE),
        exe(XRAY_BINARY_BASE),
    ]
}

fn is_ours_binary_name(image: &str) -> bool {
    let base = image.rsplit(['/', '\\']).next().unwrap_or(image);
    sidecar_binary_names()
        .iter()
        .any(|ours| base.eq_ignore_ascii_case(ours))
}

pub fn should_reap_process(image: &str, port: u16, our_ports: &[u16]) -> bool {
    our_ports.contains(&port) && is_ours_binary_name(image)
}

pub fn stale_temp_files(dir: &Path) -> Vec<PathBuf> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            eprintln!(
                "manyproxy: stale-temp listing failed for {} ({error})",
                dir.display()
            );
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                eprintln!(
                    "manyproxy: stale-temp entry unreadable in {} ({error})",
                    dir.display()
                );
                continue;
            }
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(STALE_TEMP_PREFIX) && name.ends_with(".json") {
            out.push(path);
        }
    }
    out.sort();
    out
}

pub fn sweep_stale_temp_files(dir: &Path) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    for path in stale_temp_files(dir) {
        if std::fs::remove_file(&path).is_ok() {
            removed.push(path);
        }
    }
    removed
}

pub fn parse_netstat_tcp_owners(output: &str) -> Vec<(u16, u32)> {
    let mut out = Vec::new();
    for line in output.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [proto, local, _remote, _state, pid_raw] = fields.as_slice() else {
            continue;
        };
        if !proto.eq_ignore_ascii_case("TCP") {
            continue;
        }
        let Some(port) = local
            .rsplit(':')
            .next()
            .and_then(|port| port.parse::<u16>().ok())
        else {
            continue;
        };
        let Ok(pid) = pid_raw.parse::<u32>() else {
            continue;
        };
        if pid == 0 {
            continue;
        }
        out.push((port, pid));
    }
    out
}

pub fn parse_tasklist_table(output: &str) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    for line in output.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [image, pid_raw, ..] = fields.as_slice() else {
            continue;
        };
        if !image.contains('.') {
            continue;
        }
        let Ok(pid) = pid_raw.replace(',', "").parse::<u32>() else {
            continue;
        };
        out.push((image.to_string(), pid));
    }
    out
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReapReport {
    pub reaped_pids: Vec<u32>,

    pub swept_files: Vec<PathBuf>,

    pub errors: Vec<String>,
}

pub fn startup_reap_and_sweep(
    our_ports: &[u16],
    temp_dir: &Path,
    netstat_output: &str,
    image_of: &dyn Fn(u32) -> Option<String>,
    kill: &dyn Fn(u32) -> bool,
) -> ReapReport {
    let mut report = ReapReport::default();
    if !our_ports.is_empty() {
        let mut seen: HashSet<u32> = HashSet::new();
        for (port, pid) in parse_netstat_tcp_owners(netstat_output) {
            if !our_ports.contains(&port) || !seen.insert(pid) {
                continue;
            }
            match image_of(pid) {
                Some(image) if should_reap_process(&image, port, our_ports) => {
                    if kill(pid) {
                        report.reaped_pids.push(pid);
                    } else {
                        report
                            .errors
                            .push(format!("could not reap stale {image} (pid {pid})"));
                    }
                }
                _ => {}
            }
        }
    }
    report.swept_files = sweep_stale_temp_files(temp_dir);
    report.reaped_pids.sort_unstable();
    report
}

fn system_tool(name: &str) -> PathBuf {
    #[cfg(windows)]
    {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        Path::new(&root).join("System32").join(name)
    }
    #[cfg(not(windows))]
    {
        PathBuf::from(name)
    }
}

pub fn production_startup_reap(our_ports: &[u16]) -> ReapReport {
    let netstat_output = if cfg!(windows) {
        std::process::Command::new(system_tool("netstat.exe"))
            .args(["-ano", "-p", "TCP"])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let image_of = |pid: u32| -> Option<String> {
        if !cfg!(windows) {
            return None;
        }
        let output = std::process::Command::new(system_tool("tasklist.exe"))
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "TABLE", "/NH"])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        parse_tasklist_table(&text)
            .into_iter()
            .find(|(_, found)| *found == pid)
            .map(|(image, _)| image)
    };
    let kill = |pid: u32| -> bool {
        if !cfg!(windows) {
            return false;
        }

        std::process::Command::new(system_tool("taskkill.exe"))
            .args(["/PID", &pid.to_string(), "/F"])
            .output()
            .is_ok_and(|output| output.status.success())
    };
    startup_reap_and_sweep(
        our_ports,
        &std::env::temp_dir(),
        &netstat_output,
        &image_of,
        &kill,
    )
}

fn validate_listen(
    name: &str,
    bind_ip: &str,
    port: u16,
    allow_zero: bool,
) -> Result<(), EngineError> {
    let bind = bind_ip.trim();
    if !bind.is_empty()
        && bind.parse::<std::net::IpAddr>().is_err()
        && !bind.eq_ignore_ascii_case("localhost")
    {
        return Err(EngineError::BadBind {
            name: name.to_string(),
            bind: bind_ip.to_string(),
        });
    }
    if port == 0 {
        if allow_zero {
            return Ok(());
        }
        return Err(EngineError::BadPort {
            name: name.to_string(),
            detail: "port 0 serves nothing (pick a concrete port or auto-assign before start)"
                .to_string(),
        });
    }
    if port < 1024 {
        return Err(EngineError::BadPort {
            name: name.to_string(),
            detail: format!(
                "port {port} is privileged (needs elevation); prefer the 10801-10999 auto range"
            ),
        });
    }
    if let Some(range) = reserved_range_of(port) {
        return Err(EngineError::ReservedPort {
            name: name.to_string(),
            port,
            range,
        });
    }
    Ok(())
}

pub fn validate_listen_for_save(name: &str, bind_ip: &str, port: u16) -> Result<(), EngineError> {
    validate_listen(name, bind_ip, port, true)
}

pub fn validate_listen_for_start(name: &str, bind_ip: &str, port: u16) -> Result<(), EngineError> {
    validate_listen(name, bind_ip, port, false)
}

pub fn resolve_egress_host(
    server: &str,
    resolver: &dyn Fn(&str) -> Result<(), String>,
) -> Result<(), String> {
    let trimmed = server.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    if trimmed.parse::<std::net::IpAddr>().is_ok() {
        return Ok(());
    }

    if let Some((head, _)) = trimmed.split_once('/') {
        if head.trim().parse::<std::net::IpAddr>().is_ok() {
            return Ok(());
        }
    }
    resolver(trimmed)
}

pub fn default_egress_resolver(server: &str) -> Result<(), String> {
    use std::net::ToSocketAddrs as _;
    (server, 443)
        .to_socket_addrs()
        .map_err(|error| error.to_string())
        .and_then(|mut addrs| {
            addrs
                .next()
                .map(|_| ())
                .ok_or_else(|| "no addresses".to_string())
        })
}

fn egress_servers_of(params: &serde_json::Map<String, serde_json::Value>) -> Vec<String> {
    ["server", "address"]
        .into_iter()
        .filter_map(|key| params.get(key).and_then(serde_json::Value::as_str))
        .map(|server| server.trim().to_string())
        .filter(|server| !server.is_empty() && !server.contains('/'))
        .collect()
}

pub fn validate_egress_resolution_with(
    configs: &[ProxyConfig],
    resolver: &dyn Fn(&str) -> Result<(), String>,
) -> Result<(), EngineError> {
    for config in configs {
        if is_custom_config_protocol(&config.protocol)
            || is_extra_core_protocol(&config.protocol)
            || is_chain_protocol(&config.protocol)
        {
            continue;
        }

        let params = params_object(config)?;
        for server in egress_servers_of(&params) {
            if resolve_egress_host(&server, resolver).is_err() {
                return Err(EngineError::DnsFailure {
                    name: config.name.clone(),
                    server,
                });
            }
        }
    }
    Ok(())
}

pub fn validate_egress_resolution(configs: &[ProxyConfig]) -> Result<(), EngineError> {
    validate_egress_resolution_with(configs, &default_egress_resolver)
}

const KNOWN_PARAM_KEYS: &[&str] = &[
    "server",
    "server_port",
    "server_ports",
    "address",
    "port",
    "uuid",
    "password",
    "method",
    "type",
    "tag",
    "tls",
    "security",
    "encryption",
    "sni",
    "alpn",
    "fp",
    "fingerprint",
    "network",
    "transport",
    "path",
    "host",
    "headers",
    "header_type",
    "service_name",
    "servicename",
    "flow",
    "public_key",
    "short_id",
    "reality",
    "ech",
    "utls",
    "obfs",
    "obfs_host",
    "obfs_mode",
    "auth",
    "auth_str",
    "up",
    "down",
    "up_mbps",
    "down_mbps",
    "user",
    "username",
    "private_key",
    "publickey",
    "peer_public_key",
    "reserved",
    "keepalive",
    "keep_alive",
    "mtu",
    "dns",
    "version",
    "psk",
    "userkey",
    "reuse",
    "mode",
    "alter_id",
    "congestion",
    "hop_interval",
    "insecure",
    "allow_insecure",
    "allowinsecure",
    "socks",
    "http",
    "core",
    "config",
    "exe",
    "binary",
    "nodes",
    "name",
    "protocol",
    "local_address",
    "privatekey",
    "preshared_key",
    "pre_shared_key",
    "endpoint",
    "persistent_keepalive",
    "allowed_ips",
    "allowedips",
    "noise",
    "pattern",
    "payload",
];

fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, &ca) in a.iter().enumerate() {
        let mut current = vec![i + 1];
        for (j, &cb) in b.iter().enumerate() {
            current.push(
                (prev[j] + usize::from(ca != cb))
                    .min(prev[j + 1] + 1)
                    .min(current[j] + 1),
            );
        }
        prev = current;
    }
    prev[b.len()]
}

pub fn suggest_param_key(key: &str) -> Option<&'static str> {
    let lowered = key.to_ascii_lowercase();
    if lowered.len() < 4 {
        return None;
    }
    if KNOWN_PARAM_KEYS.contains(&lowered.as_str()) {
        return None;
    }
    KNOWN_PARAM_KEYS
        .iter()
        .filter(|known| known.len() >= 4)
        .map(|known| (*known, edit_distance(&lowered, known)))
        .filter(|(_, distance)| *distance <= 2)
        .min_by_key(|(known, distance)| (*distance, known.len()))
        .map(|(known, _)| known)
}

pub fn typo_hints_for(
    name: &str,
    protocol: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> Vec<String> {
    if is_custom_config_protocol(protocol)
        || is_custom_outbound_protocol(protocol)
        || is_extra_core_protocol(protocol)
        || is_chain_protocol(protocol)
    {
        return Vec::new();
    }
    let mut hints = Vec::new();
    let mut keys: Vec<&String> = params.keys().collect();
    keys.sort();
    for key in keys {
        if let Some(suggestion) = suggest_param_key(key) {
            hints.push(format!(
                "unknown param \"{key}\" for \"{name}\" (did you mean \"{suggestion}\"?)"
            ));
        }
    }
    hints
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoRestartConfig {
    pub enabled: bool,
    pub max_retries: u32,
    pub base_backoff_secs: u64,
}

impl Default for AutoRestartConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_retries: 3,
            base_backoff_secs: 5,
        }
    }
}

pub fn auto_restart_backoff_secs(base_secs: u64, attempt: u32) -> u64 {
    base_secs.saturating_mul(1u64 << attempt.min(6)).min(300)
}

pub fn is_row_local_error(error: &EngineError) -> bool {
    matches!(
        error,
        EngineError::InvalidParamsJson { .. }
            | EngineError::BadParams { .. }
            | EngineError::UnsupportedProtocol { .. }
            | EngineError::BadAwg { .. }
            | EngineError::WrongLayer { .. }
            | EngineError::BadCustom { .. }
            | EngineError::BadExtra { .. }
            | EngineError::BadPreset { .. }
            | EngineError::ChainViolation { .. }
            | EngineError::BadBind { .. }
            | EngineError::BadPort { .. }
            | EngineError::ReservedPort { .. }
            | EngineError::DnsFailure { .. }
    )
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StartAllOutcome {
    pub applied: Vec<String>,

    pub failed: Vec<(String, String)>,

    pub warnings: Vec<String>,

    pub error: Option<String>,
}

static CONFIG_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_config_path(stem: &str) -> PathBuf {
    let n = CONFIG_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut rand = [0u8; 8];
    if getrandom::fill(&mut rand).is_err() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        let mixed = nanos.wrapping_add((n as u128).wrapping_mul(0x9e3779b97f4a7c15));
        rand.copy_from_slice(&mixed.to_ne_bytes()[..8]);
    }
    let r = u64::from_ne_bytes(rand);
    std::env::temp_dir().join(format!(
        "manyproxy-{stem}-{}-{n}-{r:016x}.json",
        std::process::id()
    ))
}

const LOG_POISON_NOTICE: &str = "log pipe unreadable (log reader failed)";

fn write_config_restricted(path: &Path, config_json: &str) -> Result<(), String> {
    use std::io::Write as _;
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            std::fs::remove_file(path)
                .map_err(|error| format!("cannot write engine config ({error})"))?;
            options
                .open(path)
                .map_err(|error| format!("cannot write engine config ({error})"))?
        }
        Err(error) => return Err(format!("cannot write engine config ({error})")),
    };
    file.write_all(config_json.as_bytes())
        .map_err(|error| format!("cannot write engine config ({error})"))?;
    drop(file);

    crate::store::harden_file_permissions(path);
    Ok(())
}

fn is_executable(path: &Path) -> bool {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let meta = match file.metadata() {
        Ok(meta) => meta,
        Err(_) => return false,
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        return meta.permissions().mode() & 0o111 != 0;
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub(crate) fn normalize_bind_key(bind: &str) -> String {
    match bind.trim().to_ascii_lowercase().as_str() {
        "" | "localhost" | "127.0.0.1" | "::1" | "::ffff:127.0.0.1" => "loopback".to_string(),
        other => other.to_string(),
    }
}

fn probe_bind_available(bind: &str, port: u16) -> Result<std::net::TcpListener, String> {
    static PROBE_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    let _guard = PROBE_LOCK
        .get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let host = {
        let key = normalize_bind_key(bind);
        if key == "loopback" {
            "127.0.0.1".to_string()
        } else {
            bind.trim().to_string()
        }
    };
    let mut last = format!("probe failed for {host}:{port}");
    for _ in 0..10 {
        match std::net::TcpListener::bind(format!("{host}:{port}")) {
            Ok(listener) => return Ok(listener),
            Err(error) => {
                last = format!("{host}:{port} is already in use ({error})");
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
    Err(last)
}

fn find_system_mutation_key(document: &serde_json::Value) -> Option<(String, String)> {
    fn walk(node: &serde_json::Value, path: &str) -> Option<(String, String)> {
        match node {
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    let lowered = key.to_ascii_lowercase();
                    if lowered == "tun" || lowered == "dns" || lowered == "set_system_proxy" {
                        return Some((key.clone(), path.to_string()));
                    }

                    if lowered == "type" || lowered == "protocol" {
                        if let Some(kind) = value.as_str() {
                            let kind = kind.to_ascii_lowercase();
                            if kind == "tun" || kind == "dns" {
                                return Some((key.clone(), path.to_string()));
                            }
                        }
                    }
                    let child = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    if let Some(hit) = walk(value, &child) {
                        return Some(hit);
                    }
                }
                None
            }
            serde_json::Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    if let Some(hit) = walk(item, &format!("{path}[{index}]")) {
                        return Some(hit);
                    }
                }
                None
            }
            _ => None,
        }
    }
    walk(document, "$")
}

fn unconfirmed_lan(
    configs: &[ProxyConfig],
    confirmed: &HashSet<(String, String)>,
) -> Vec<(String, String, String)> {
    configs
        .iter()
        .filter(|row| requires_lan_confirm(&row.bind_ip))
        .filter(|row| !confirmed.contains(&(row.id.clone(), row.bind_ip.clone())))
        .map(|row| (row.id.clone(), row.name.clone(), row.bind_ip.clone()))
        .collect()
}

trait WaitTarget {
    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>>;
    fn kill(&mut self) -> std::io::Result<()>;
}

impl WaitTarget for std::process::Child {
    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        std::process::Child::try_wait(self)
    }

    fn kill(&mut self) -> std::io::Result<()> {
        std::process::Child::kill(self)
    }
}

fn wait_child_bounded(target: &mut impl WaitTarget, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        match target.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => {}
            Err(_) => return false,
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = target.kill();
    for _ in 0..20 {
        match target.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => {}
            Err(_) => return false,
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = target.kill();
    matches!(target.try_wait(), Ok(Some(_)))
}

pub struct RealEngineBackend {
    child: Option<std::process::Child>,
    config_path: PathBuf,
    fork: bool,

    xray: bool,

    extra_exe: Option<PathBuf>,

    pinned_binary: Option<PathBuf>,

    log_buffer: Arc<Mutex<(u64, VecDeque<String>)>>,

    reader_gen: u64,

    reader_handle: Option<std::thread::JoinHandle<()>>,
}

pub const LOG_PIPE_BUFFER: usize = 2000;

fn push_pipe_line(buffer: &Arc<Mutex<(u64, VecDeque<String>)>>, expected_gen: u64, line: String) {
    match buffer.lock() {
        Ok(mut slot) => {
            if slot.0 != expected_gen {
                return;
            }
            if slot.1.len() >= LOG_PIPE_BUFFER {
                slot.1.pop_front();
            }
            slot.1.push_back(line);
        }
        Err(poison) => {
            let mut slot = poison.into_inner();
            if slot.0 != expected_gen {
                return;
            }
            if !slot.1.back().is_some_and(|last| last == LOG_POISON_NOTICE) {
                if slot.1.len() >= LOG_PIPE_BUFFER {
                    slot.1.pop_front();
                }
                slot.1.push_back(LOG_POISON_NOTICE.to_string());
            }
            if slot.1.len() >= LOG_PIPE_BUFFER {
                slot.1.pop_front();
            }
            slot.1.push_back(line);
        }
    }
}

impl RealEngineBackend {
    pub fn new() -> Self {
        Self {
            child: None,

            config_path: unique_config_path("singbox"),
            fork: false,
            xray: false,
            extra_exe: None,
            pinned_binary: None,
            log_buffer: Arc::new(Mutex::new((0, VecDeque::new()))),
            reader_gen: 0,
            reader_handle: None,
        }
    }

    pub fn fork() -> Self {
        Self {
            child: None,
            config_path: unique_config_path("fork"),
            fork: true,
            xray: false,
            extra_exe: None,
            pinned_binary: None,
            log_buffer: Arc::new(Mutex::new((0, VecDeque::new()))),
            reader_gen: 0,
            reader_handle: None,
        }
    }

    pub fn xray() -> Self {
        Self {
            child: None,
            config_path: unique_config_path("xray"),
            fork: false,
            xray: true,
            extra_exe: None,
            pinned_binary: None,
            log_buffer: Arc::new(Mutex::new((0, VecDeque::new()))),
            reader_gen: 0,
            reader_handle: None,
        }
    }

    pub fn extra_slot() -> Self {
        Self {
            child: None,
            config_path: unique_config_path("extra"),
            fork: false,
            xray: false,
            extra_exe: None,
            pinned_binary: None,
            log_buffer: Arc::new(Mutex::new((0, VecDeque::new()))),
            reader_gen: 0,
            reader_handle: None,
        }
    }

    fn resolve_binary(&self) -> Result<PathBuf, String> {
        if let Some(exe) = &self.extra_exe {
            if !is_executable(exe) {
                return Err(format!(
                    "extra-core executable is not executable: {}",
                    exe.display()
                ));
            }
            return Ok(exe.clone());
        }

        if let Some(pinned) = &self.pinned_binary {
            if is_executable(pinned) {
                return Ok(pinned.clone());
            }
        }
        if self.xray {
            resolve_xray_binary().map_err(|error| error.to_string())
        } else if self.fork {
            resolve_fork_binary().map_err(|error| error.to_string())
        } else {
            resolve_engine_binary().map_err(|error| error.to_string())
        }
    }

    fn start_args(&self) -> Vec<String> {
        if self.extra_exe.is_some() {
            vec!["-c".to_string()]
        } else {
            vec!["run".to_string(), "-c".to_string()]
        }
    }

    fn write_config(&self, config_json: &str) -> Result<PathBuf, String> {
        write_config_restricted(&self.config_path, config_json)?;
        Ok(self.config_path.clone())
    }

    fn stop_child(&mut self) {
        self.reader_gen = self.reader_gen.wrapping_add(1);
        let gen = self.reader_gen;
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let reaped = wait_child_bounded(&mut child, Duration::from_secs(3));
            if !reaped {
                let _ = child.kill();
            }
        }

        match self.log_buffer.lock() {
            Ok(mut slot) => {
                slot.1.clear();
                slot.0 = gen;
            }
            Err(poison) => {
                let mut slot = poison.into_inner();
                slot.1.clear();
                slot.0 = gen;
            }
        }

        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            if self
                .reader_handle
                .as_ref()
                .is_some_and(|handle| handle.is_finished())
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if self
            .reader_handle
            .as_ref()
            .is_some_and(|handle| handle.is_finished())
        {
            if let Some(handle) = self.reader_handle.take() {
                let _ = handle.join();
            }
        }
    }
}

impl Default for RealEngineBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineBackend for RealEngineBackend {
    fn check_config(&mut self, config_json: &str) -> Result<(), String> {
        let binary = self.resolve_binary()?;
        if self.fork {
            probe_fork_version(&binary)?;
        }

        if self.xray || self.extra_exe.is_some() {
            self.write_config(config_json)?;

            self.remove_config_file();
            return Ok(());
        }
        let path = self.write_config(config_json)?;
        let mut cmd = std::process::Command::new(&binary);
        cmd.arg("check").arg("-c").arg(&path);
        spawn_hidden(&mut cmd);
        let output = cmd.output();
        self.remove_config_file();
        let output = output.map_err(|error| format!("engine check could not run ({error})"))?;

        if output.status.success() {
            Ok(())
        } else {
            Err(truncate_detail(&strip_ansi_codes(
                &String::from_utf8_lossy(&output.stderr),
            )))
        }
    }

    fn start(&mut self, config_json: &str) -> Result<(), String> {
        let binary = self.resolve_binary()?;

        self.stop_child();
        let path = self.write_config(config_json)?;

        let mut cmd = std::process::Command::new(&binary);
        cmd.args(self.start_args())
            .arg(&path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped());
        spawn_hidden(&mut cmd);
        let spawn = cmd.spawn();
        let mut child = match spawn {
            Ok(child) => child,

            Err(error) => {
                self.remove_config_file();
                return Err(format!("engine failed to start ({error})"));
            }
        };
        let gen = self.reader_gen;
        match child.stderr.take() {
            Some(pipe) => {
                let buffer = self.log_buffer.clone();
                let handle = std::thread::spawn(move || {
                    use std::io::BufRead as _;
                    let reader = std::io::BufReader::new(pipe);
                    for line in reader.lines() {
                        match line {
                            Ok(text) => push_pipe_line(&buffer, gen, text),
                            Err(error) => {
                                push_pipe_line(
                                    &buffer,
                                    gen,
                                    format!("log pipe unreadable ({error})"),
                                );
                                break;
                            }
                        }
                    }
                });
                self.reader_handle = Some(handle);
            }
            None => {
                push_pipe_line(
                    &self.log_buffer,
                    gen,
                    "log pipe unavailable (sidecar stderr was not piped)".to_string(),
                );
            }
        }
        self.child = Some(child);
        Ok(())
    }

    fn stop(&mut self) {
        self.stop_child();
        self.remove_config_file();
    }

    fn is_running(&self) -> bool {
        self.child.is_some()
    }

    fn process_id(&self) -> Option<u32> {
        self.child.as_ref().map(|child| child.id())
    }

    fn poll_exit(&mut self) -> Option<String> {
        let status = match self.child.as_mut()?.try_wait() {
            Ok(Some(status)) => status,
            Ok(None) => return None,
            Err(error) => {
                self.child = None;
                self.remove_config_file();
                return Some(format!("engine exited (unknown: {error})"));
            }
        };
        self.child = None;

        self.remove_config_file();
        Some(format!("engine exited ({status})"))
    }

    fn drain_log_lines(&mut self) -> Vec<String> {
        match self.log_buffer.lock() {
            Ok(mut slot) => std::mem::take(&mut slot.1).into_iter().collect(),
            Err(poison) => {
                let mut slot = poison.into_inner();
                let mut lines: Vec<String> = std::mem::take(&mut slot.1).into_iter().collect();
                lines.push(LOG_POISON_NOTICE.to_string());
                lines
            }
        }
    }

    fn set_extra_exe(&mut self, exe: PathBuf) {
        self.extra_exe = Some(exe);
    }

    fn clear_extra_exe(&mut self) {
        self.extra_exe = None;
    }

    fn pin_binary(&mut self, path: PathBuf) {
        self.pinned_binary = Some(path);
    }

    fn remove_config_file(&self) {
        let _ = std::fs::remove_file(&self.config_path);
    }
}

impl Drop for RealEngineBackend {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
#[derive(Debug, Default)]
struct FakeState {
    running: bool,
    last_config: Option<String>,
    check_calls: usize,
    fail_check: Option<String>,

    fail_start: Option<String>,
    exited: Option<String>,
    pending_logs: Vec<String>,

    /// T-30: the pid the fake shard pretends to have spawned.
    pid: Option<u32>,
}

#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub struct FakeHandle {
    state: Rc<RefCell<FakeState>>,
}

#[cfg(test)]
impl FakeHandle {
    pub fn last_config(&self) -> Option<String> {
        self.state.borrow().last_config.clone()
    }

    pub fn is_running(&self) -> bool {
        self.state.borrow().running
    }

    pub fn check_calls(&self) -> usize {
        self.state.borrow().check_calls
    }

    pub fn fail_check(&self, detail: &str) {
        self.state.borrow_mut().fail_check = Some(detail.to_string());
    }

    pub fn fail_start(&self, detail: &str) {
        self.state.borrow_mut().fail_start = Some(detail.to_string());
    }

    pub fn clear_failures(&self) {
        let mut state = self.state.borrow_mut();
        state.fail_check = None;
        state.fail_start = None;
    }

    pub fn inject_crash(&self, cause: &str) {
        let mut state = self.state.borrow_mut();
        state.running = false;
        state.pid = None;
        state.exited = Some(cause.to_string());
    }

    pub fn emit_log(&self, line: &str) {
        self.state.borrow_mut().pending_logs.push(line.to_string());
    }

    /// T-30: PRETEND this shard's child runs with `pid` (`None` = no child).
    pub fn set_pid(&self, pid: Option<u32>) {
        self.state.borrow_mut().pid = pid;
    }
}

#[cfg(test)]
pub struct FakeEngineBackend {
    state: Rc<RefCell<FakeState>>,
}

#[cfg(test)]
impl FakeEngineBackend {
    pub fn shared() -> (Self, FakeHandle) {
        let state = Rc::new(RefCell::new(FakeState::default()));
        (
            Self {
                state: state.clone(),
            },
            FakeHandle { state },
        )
    }
}

#[cfg(test)]
impl Default for FakeEngineBackend {
    fn default() -> Self {
        Self::shared().0
    }
}

#[cfg(test)]
impl EngineBackend for FakeEngineBackend {
    fn check_config(&mut self, _config_json: &str) -> Result<(), String> {
        let mut state = self.state.borrow_mut();
        state.check_calls += 1;
        state.fail_check.clone().map_or(Ok(()), Err)
    }

    fn start(&mut self, config_json: &str) -> Result<(), String> {
        let mut state = self.state.borrow_mut();
        if let Some(detail) = state.fail_start.take() {
            state.running = false;
            return Err(detail);
        }
        state.running = true;
        state.exited = None;
        state.last_config = Some(config_json.to_string());
        Ok(())
    }

    fn stop(&mut self) {
        let mut state = self.state.borrow_mut();
        state.running = false;
        state.pid = None;
    }

    fn is_running(&self) -> bool {
        self.state.borrow().running
    }

    fn process_id(&self) -> Option<u32> {
        self.state.borrow().pid
    }

    fn poll_exit(&mut self) -> Option<String> {
        self.state.borrow_mut().exited.take()
    }

    fn drain_log_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.state.borrow_mut().pending_logs)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineStatus {
    Stopped,
    Running,
    Crashed { cause: String },
}

pub struct EngineController {
    backend: Box<dyn EngineBackend>,

    fork_backend: Box<dyn EngineBackend>,

    xray_backend: Box<dyn EngineBackend>,

    extra_backends: Vec<Box<dyn EngineBackend>>,

    fork_probe: Box<dyn Fn() -> Result<PathBuf, EngineError>>,

    xray_probe: Box<dyn Fn() -> Result<PathBuf, EngineError>>,

    #[allow(clippy::type_complexity)]
    extra_probe: Box<dyn Fn(&str) -> Result<PathBuf, EngineError>>,

    stock_probe: Box<dyn Fn() -> Result<PathBuf, EngineError>>,

    lan_confirmed: HashSet<(String, String)>,

    served: [Option<String>; 3],

    served_extra: Vec<Option<String>>,

    preset_rules: HashMap<String, Vec<serde_json::Value>>,

    /// T-12 staged `.srs` name → absolute staged file path, synced from the
    /// store like `preset_rules`; config generation resolves references
    /// against it and fails loud on missing files.
    rule_set_files: HashMap<String, PathBuf>,
    active: Vec<ProxyConfig>,
    status: EngineStatus,
    last_error: Option<String>,
    last_config_json: Option<String>,

    clash: Vec<ClashTarget>,

    generation: u64,

    crashed: HashSet<String>,

    pending_crash: Option<String>,

    totals_stale: bool,

    auto_restart: AutoRestartConfig,

    restart_attempts: u32,

    last_restart_attempt: Option<Instant>,
}

fn missing_fork_for(error: EngineError, rows: &str, xhttp: bool) -> EngineError {
    match error {
        EngineError::MissingForkBinary {
            binary, searched, ..
        } => EngineError::MissingForkBinary {
            rows: rows.to_string(),
            binary,
            searched,
            xhttp,
        },
        other => other,
    }
}

fn missing_xray_for(error: EngineError, rows: &str) -> EngineError {
    match error {
        EngineError::MissingXrayBinary {
            binary, searched, ..
        } => EngineError::MissingXrayBinary {
            rows: rows.to_string(),
            binary,
            searched,
        },
        other => other,
    }
}

fn missing_extra_for(error: EngineError, rows: &str) -> EngineError {
    match error {
        EngineError::MissingExtraBinary {
            binary, searched, ..
        } => EngineError::MissingExtraBinary {
            rows: rows.to_string(),
            binary,
            searched,
        },
        other => other,
    }
}

impl EngineController {
    pub fn new(backend: Box<dyn EngineBackend>) -> Self {
        Self {
            backend,
            fork_backend: Box::new(RealEngineBackend::fork()),
            xray_backend: Box::new(RealEngineBackend::xray()),
            extra_backends: vec![Box::new(RealEngineBackend::extra_slot())],
            fork_probe: Box::new(resolve_fork_binary),
            xray_probe: Box::new(resolve_xray_binary),
            extra_probe: Box::new(resolve_extra_binary),
            stock_probe: Box::new(resolve_engine_binary),
            lan_confirmed: HashSet::new(),
            served: [None, None, None],
            served_extra: vec![None],
            preset_rules: HashMap::new(),
            rule_set_files: HashMap::new(),
            active: Vec::new(),
            status: EngineStatus::Stopped,
            last_error: None,
            last_config_json: None,
            clash: Vec::new(),
            generation: 0,
            crashed: HashSet::new(),
            pending_crash: None,
            totals_stale: false,
            auto_restart: AutoRestartConfig::default(),
            restart_attempts: 0,
            last_restart_attempt: None,
        }
    }

    pub fn production() -> Self {
        Self::new(Box::new(RealEngineBackend::new()))
    }

    #[cfg(test)]
    pub fn fake() -> (Self, FakeHandle) {
        let (controller, stock, _) = Self::fake_pair();
        (controller, stock)
    }

    #[cfg(test)]
    pub fn fake_pair() -> (Self, FakeHandle, FakeHandle) {
        let (controller, stock, fork, _, _) = Self::fake_quartet();
        (controller, stock, fork)
    }

    #[cfg(test)]
    pub fn fake_quartet() -> (Self, FakeHandle, FakeHandle, FakeHandle, FakeHandle) {
        let (backend, stock) = FakeEngineBackend::shared();
        let (fork_backend, fork) = FakeEngineBackend::shared();
        let (xray_backend, xray) = FakeEngineBackend::shared();
        let (extra_backend, extra) = FakeEngineBackend::shared();
        let controller = Self {
            backend: Box::new(backend),
            fork_backend: Box::new(fork_backend),
            xray_backend: Box::new(xray_backend),
            extra_backends: vec![Box::new(extra_backend)],
            fork_probe: Box::new(|| {
                Err(EngineError::MissingForkBinary {
                    rows: String::new(),
                    binary: fork_binary_file_name(),
                    searched: "test-seam (no fork binary)".to_string(),
                    xhttp: false,
                })
            }),
            xray_probe: Box::new(|| {
                Err(EngineError::MissingXrayBinary {
                    rows: String::new(),
                    binary: xray_binary_file_name(),
                    searched: "test-seam (no xray binary)".to_string(),
                })
            }),
            extra_probe: Box::new(|exe| {
                Err(EngineError::MissingExtraBinary {
                    rows: String::new(),
                    binary: exe.to_string(),
                    searched: "test-seam (no app dir)".to_string(),
                })
            }),

            stock_probe: Box::new(|| Ok(PathBuf::from("test-stock-bin"))),
            lan_confirmed: HashSet::new(),
            served: [None, None, None],
            served_extra: vec![None],
            preset_rules: HashMap::new(),
            rule_set_files: HashMap::new(),
            active: Vec::new(),
            status: EngineStatus::Stopped,
            last_error: None,
            last_config_json: None,
            clash: Vec::new(),
            generation: 0,
            crashed: HashSet::new(),
            pending_crash: None,
            totals_stale: false,
            auto_restart: AutoRestartConfig::default(),
            restart_attempts: 0,
            last_restart_attempt: None,
        };
        (controller, stock, fork, xray, extra)
    }

    #[cfg(test)]
    pub fn push_test_extra_backend(&mut self) -> FakeHandle {
        let (backend, handle) = FakeEngineBackend::shared();
        self.extra_backends.push(Box::new(backend));
        self.served_extra.push(None);
        handle
    }

    #[cfg(test)]
    pub fn set_fork_probe(&mut self, probe: impl Fn() -> Result<PathBuf, EngineError> + 'static) {
        self.fork_probe = Box::new(probe);
    }

    #[cfg(test)]
    pub fn set_xray_probe(&mut self, probe: impl Fn() -> Result<PathBuf, EngineError> + 'static) {
        self.xray_probe = Box::new(probe);
    }

    #[cfg(test)]
    pub fn set_extra_probe(
        &mut self,
        probe: impl Fn(&str) -> Result<PathBuf, EngineError> + 'static,
    ) {
        self.extra_probe = Box::new(probe);
    }

    #[cfg(test)]
    pub fn set_stock_probe(&mut self, probe: impl Fn() -> Result<PathBuf, EngineError> + 'static) {
        self.stock_probe = Box::new(probe);
    }

    pub fn set_preset_rules(&mut self, presets: HashMap<String, Vec<serde_json::Value>>) {
        self.preset_rules = presets;
    }

    /// T-12: staged rule-set name → file path map, re-derived at sync time so
    /// a moved app folder re-points at the right staged files.
    pub fn set_rule_set_files(&mut self, files: HashMap<String, PathBuf>) {
        self.rule_set_files = files;
    }

    #[cfg(test)]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    #[cfg(test)]
    pub fn crashed_shards(&self) -> Vec<String> {
        let mut shards: Vec<String> = self.crashed.iter().cloned().collect();
        shards.sort();
        shards
    }

    #[cfg(test)]
    pub fn is_shard_crashed(&self, core: &str) -> bool {
        self.crashed.contains(core)
    }

    fn custom_core_of(row: &ProxyConfig) -> Option<OwningCore> {
        params_object(row)
            .ok()
            .and_then(|params| owning_core_of(&row.name, &params).ok())
    }

    fn shard_of_row(&self, row: &ProxyConfig) -> String {
        if is_extra_core_protocol(&row.protocol) {
            return format!("extra-{}", row.id);
        }
        if is_custom_config_protocol(&row.protocol) {
            return Self::custom_core_of(row)
                .map(|core| core.as_str().to_string())
                .unwrap_or_else(|| "unknown".to_string());
        }
        if needs_xray(row) {
            return OwningCore::Xray.as_str().to_string();
        }
        if is_custom_outbound_protocol(&row.protocol) {
            return Self::custom_core_of(row)
                .map(|core| core.as_str().to_string())
                .unwrap_or_else(|| "unknown".to_string());
        }
        if needs_fork(row) {
            return OwningCore::Fork.as_str().to_string();
        }
        OwningCore::Stock.as_str().to_string()
    }

    pub fn ids_on_crashed_shards(&self) -> Vec<String> {
        if self.crashed.is_empty() {
            return Vec::new();
        }
        self.active
            .iter()
            .filter(|row| self.crashed.contains(&self.shard_of_row(row)))
            .map(|row| row.id.clone())
            .collect()
    }

    #[cfg(test)]
    pub fn totals_stale(&self) -> bool {
        self.totals_stale
    }

    #[cfg(test)]
    pub fn clear_totals_stale(&mut self) {
        self.totals_stale = false;
    }

    pub fn live_clash_targets(&self) -> Vec<ClashTarget> {
        self.clash
            .iter()
            .filter(|target| !self.crashed.contains(&target.core))
            .cloned()
            .collect()
    }

    pub fn set_auto_restart(&mut self, config: AutoRestartConfig) {
        if !config.enabled {
            self.restart_attempts = 0;
            self.last_restart_attempt = None;
        }
        self.auto_restart = config;
    }

    pub fn auto_restart(&self) -> &AutoRestartConfig {
        &self.auto_restart
    }

    #[cfg(test)]
    pub fn restart_attempts(&self) -> u32 {
        self.restart_attempts
    }

    /// The most recent crash cause, retained until a notification or restart
    /// handles it so a later `reload` can still surface it after the tick drained
    /// the backend's one-shot exit event.
    pub fn pending_crash(&self) -> Option<String> {
        if self.active.is_empty() {
            return None;
        }
        self.pending_crash.clone()
    }

    pub fn clear_pending_crash(&mut self) {
        self.pending_crash = None;
    }

    pub fn auto_restart_due(&self, now: Instant) -> bool {
        if !self.auto_restart.enabled || self.crashed.is_empty() {
            return false;
        }
        if self.restart_attempts >= self.auto_restart.max_retries {
            return false;
        }
        match self.last_restart_attempt {
            None => true,
            Some(last) => {
                now.duration_since(last)
                    >= Duration::from_secs(auto_restart_backoff_secs(
                        self.auto_restart.base_backoff_secs,
                        self.restart_attempts,
                    ))
            }
        }
    }

    pub fn note_auto_restart_attempt(&mut self, now: Instant) {
        self.restart_attempts = self.restart_attempts.saturating_add(1);
        self.last_restart_attempt = Some(now);
    }

    pub fn note_restart_success(&mut self) {
        self.crashed.clear();
        self.pending_crash = None;
        self.totals_stale = false;
    }

    /// Resets the retry budget only once a shard has stayed continuously up for
    /// at least `base_backoff_secs` after its last restart attempt. A successful
    /// `apply()` only proves a spawn, so a core that crashes again immediately
    /// keeps its attempts and the backoff escalates until `max_retries`.
    pub fn settle_restart_budget(&mut self, now: Instant) {
        if self.restart_attempts == 0 || !self.crashed.is_empty() {
            return;
        }
        let steady = Duration::from_secs(self.auto_restart.base_backoff_secs.max(1));
        if self
            .last_restart_attempt
            .is_some_and(|last| now.saturating_duration_since(last) >= steady)
        {
            self.restart_attempts = 0;
            self.last_restart_attempt = None;
        }
    }

    pub fn restart_crashed(&mut self) -> Result<(), EngineError> {
        self.generation = self.generation.wrapping_add(1);
        if self.crashed.is_empty() {
            return Ok(());
        }
        match self.apply() {
            Ok(()) => {
                self.note_restart_success();
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn clear_clash_secrets(&mut self) {
        use zeroize::Zeroize as _;
        for target in self.clash.iter_mut() {
            target.secret.zeroize();
        }
        self.clash.clear();
    }

    fn drop_crashed_served(&mut self, shard: &str) {
        const SHARDS: [&str; 3] = ["stock", "fork", "xray"];
        if let Some(index) = SHARDS.iter().position(|name| *name == shard) {
            self.served[index] = None;
            return;
        }
        if let Some(id) = shard.strip_prefix("extra-") {
            let extra_ids: Vec<String> = self
                .active
                .iter()
                .filter(|row| is_extra_core_protocol(&row.protocol))
                .map(|row| row.id.clone())
                .collect();

            let index = extra_ids
                .iter()
                .position(|known| known == id)
                .or_else(|| id.parse::<usize>().ok());
            if let Some(index) = index {
                if index < self.served_extra.len() {
                    self.served_extra[index] = None;
                }
            }
        }
    }

    fn shard_of_row_exists(&self, shard: &str) -> bool {
        self.active
            .iter()
            .any(|row| self.shard_of_row(row) == shard)
    }

    fn shard_backend_running(&self, shard: &str) -> bool {
        if let Some(id) = shard.strip_prefix("extra-") {
            let extra_ids: Vec<&String> = self
                .active
                .iter()
                .filter(|row| is_extra_core_protocol(&row.protocol))
                .map(|row| &row.id)
                .collect();
            let index = extra_ids
                .iter()
                .position(|known| known.as_str() == id)
                .or_else(|| id.parse::<usize>().ok());
            return index
                .and_then(|index| self.extra_backends.get(index))
                .is_some_and(|slot| slot.is_running());
        }
        match shard {
            "stock" => self.backend.is_running(),
            "fork" => self.fork_backend.is_running(),
            "xray" => self.xray_backend.is_running(),
            _ => false,
        }
    }

    /// Drops crash state for shards that a successful `apply()` has resolved:
    /// either the core is serving again, or no active row targets it anymore.
    fn reap_recovered_crashes(&mut self) {
        if self.crashed.is_empty() {
            return;
        }
        let recovered: Vec<String> = self
            .crashed
            .iter()
            .filter(|shard| !self.shard_of_row_exists(shard) || self.shard_backend_running(shard))
            .cloned()
            .collect();
        for shard in recovered {
            self.crashed.remove(&shard);
        }
        if self.crashed.is_empty() {
            self.pending_crash = None;
        }
    }

    pub fn stop_one_result(&mut self, id: &str) -> (bool, Option<EngineError>) {
        self.generation = self.generation.wrapping_add(1);
        let before = self.active.len();
        let prev = self.active.clone();
        let prev_status = self.status.clone();
        self.active.retain(|row| row.id != id);
        if self.active.len() == before {
            return (false, None);
        }
        if let Err(error) = self.apply() {
            self.active = prev;
            self.status = prev_status;
            self.prune_lan_confirmed();
            return (true, Some(error));
        }
        self.prune_lan_confirmed();
        (true, None)
    }

    pub fn stop_all_result(&mut self) -> Option<EngineError> {
        self.generation = self.generation.wrapping_add(1);
        let prev = self.active.clone();
        let prev_status = self.status.clone();
        self.active.clear();
        if let Err(error) = self.apply() {
            self.active = prev;
            self.status = prev_status;
            self.prune_lan_confirmed();
            return Some(error);
        }
        self.prune_lan_confirmed();
        None
    }

    pub fn start_all_subset(
        &mut self,
        configs: &[ProxyConfig],
        confirmed: bool,
    ) -> StartAllOutcome {
        let mut outcome = StartAllOutcome::default();
        if configs.is_empty() {
            self.active.clear();
            let result = self.apply();
            self.prune_lan_confirmed();
            if let Err(error) = result {
                outcome.error = Some(error.to_string());
            }
            return outcome;
        }

        let mut candidate: Vec<ProxyConfig> = Vec::with_capacity(configs.len());
        for row in configs {
            match validate_activation(std::slice::from_ref(row)) {
                Ok(_) => candidate.push(row.clone()),
                Err(error) => {
                    outcome.failed.push((row.name.clone(), error.to_string()));
                }
            }
        }

        let mut plan_warnings = Vec::new();
        let mut union_error: Option<EngineError> = None;
        match validate_activation(&candidate) {
            Ok(plan) => {
                plan_warnings = plan.warnings.clone();
            }
            Err(error) => {
                union_error = Some(error);
            }
        }
        if let Some(error) = union_error {
            let text = error.to_string();
            for row in &candidate {
                outcome.failed.push((row.name.clone(), text.clone()));
            }
            outcome.error = Some(text);
            return outcome;
        }
        if candidate.is_empty() {
            return outcome;
        }
        outcome.warnings = plan_warnings;

        let prev = self.active.clone();
        let prev_status = self.status.clone();
        self.generation = self.generation.wrapping_add(1);
        self.active = candidate.clone();
        if let Err(error) = self.enforce_lan_confirm(&candidate, confirmed) {
            self.active = prev;
            self.status = prev_status;
            self.prune_lan_confirmed();
            outcome.error = Some(error.to_string());
            for row in &candidate {
                outcome.failed.push((row.name.clone(), error.to_string()));
            }
            return outcome;
        }
        match self.apply() {
            Ok(()) => {
                self.prune_lan_confirmed();
                outcome.applied = candidate.iter().map(|row| row.id.clone()).collect();
            }
            Err(error) => {
                self.active = prev;
                self.status = prev_status;
                self.prune_lan_confirmed();
                outcome.error = Some(error.to_string());
                for row in &candidate {
                    outcome.failed.push((row.name.clone(), error.to_string()));
                }
            }
        }
        outcome
    }

    fn prune_lan_confirmed(&mut self) {
        let live: HashSet<(String, String)> = self
            .active
            .iter()
            .map(|row| (row.id.clone(), row.bind_ip.clone()))
            .collect();
        self.lan_confirmed.retain(|entry| live.contains(entry));
    }

    fn backend_at(&mut self, index: usize) -> &mut dyn EngineBackend {
        match index {
            0 => self.backend.as_mut(),
            1 => self.fork_backend.as_mut(),
            _ => self.xray_backend.as_mut(),
        }
    }

    fn any_backend_running(&self) -> bool {
        self.backend.is_running()
            || self.fork_backend.is_running()
            || self.xray_backend.is_running()
            || self.extra_backends.iter().any(|slot| slot.is_running())
    }

    fn extra_slot_index(&self, id: &str) -> Option<usize> {
        self.active
            .iter()
            .filter(|row| is_extra_core_protocol(&row.protocol))
            .position(|row| row.id == id)
    }

    #[cfg(test)]
    pub fn start_one(&mut self, config: &ProxyConfig) -> Result<(), EngineError> {
        self.start_one_inner(config, false)
    }

    pub fn start_one_confirmed(&mut self, config: &ProxyConfig) -> Result<(), EngineError> {
        self.start_one_inner(config, true)
    }

    fn start_one_inner(
        &mut self,
        config: &ProxyConfig,
        confirmed: bool,
    ) -> Result<(), EngineError> {
        let mut candidate = self.active.clone();
        if let Some(slot) = candidate.iter_mut().find(|row| row.id == config.id) {
            *slot = config.clone();
        } else {
            candidate.push(config.clone());
        }

        validate_activation(&candidate).inspect_err(|error| {
            self.last_error = Some(error.to_string());
        })?;
        self.enforce_lan_confirm(&candidate, confirmed)?;

        let prev = self.active.clone();
        let prev_status = self.status.clone();
        self.generation = self.generation.wrapping_add(1);
        self.active = candidate;
        match self.apply() {
            Ok(()) => {
                self.prune_lan_confirmed();
                Ok(())
            }
            Err(error) => {
                self.active = prev;
                self.status = prev_status;
                self.prune_lan_confirmed();
                Err(error)
            }
        }
    }

    fn enforce_lan_confirm(
        &mut self,
        candidate: &[ProxyConfig],
        confirmed: bool,
    ) -> Result<(), EngineError> {
        if confirmed {
            for row in candidate {
                if requires_lan_confirm(&row.bind_ip) {
                    self.lan_confirmed
                        .insert((row.id.clone(), row.bind_ip.clone()));
                }
            }
            return Ok(());
        }
        let missing = unconfirmed_lan(candidate, &self.lan_confirmed);
        if missing.is_empty() {
            return Ok(());
        }
        let mut names: Vec<String> = missing
            .iter()
            .map(|(_, name, bind)| format!("\"{name}\" on {bind}"))
            .collect();
        names.sort();
        let error = EngineError::LanConfirmRequired {
            names: names.join(", "),
        };
        self.last_error = Some(error.to_string());
        Err(error)
    }

    pub fn stop_one(&mut self, id: &str) -> bool {
        let (removed, error) = self.stop_one_result(id);
        if let Some(error) = error {
            self.last_error = Some(error.to_string());
        }
        removed
    }

    #[cfg(test)]
    pub fn start_all(&mut self, configs: &[ProxyConfig]) -> Result<(), EngineError> {
        self.start_all_inner(configs, false)
    }

    pub fn start_all_confirmed(&mut self, configs: &[ProxyConfig]) -> Result<(), EngineError> {
        self.start_all_inner(configs, true)
    }

    fn start_all_inner(
        &mut self,
        configs: &[ProxyConfig],
        confirmed: bool,
    ) -> Result<(), EngineError> {
        let candidate = configs.to_vec();
        if candidate.is_empty() {
            self.active.clear();
            let result = self.apply();
            self.prune_lan_confirmed();
            return result;
        }
        validate_activation(&candidate).inspect_err(|error| {
            self.last_error = Some(error.to_string());
        })?;
        self.enforce_lan_confirm(&candidate, confirmed)?;

        let prev = self.active.clone();
        let prev_status = self.status.clone();
        self.generation = self.generation.wrapping_add(1);
        self.active = candidate;
        match self.apply() {
            Ok(()) => {
                self.prune_lan_confirmed();
                Ok(())
            }
            Err(error) => {
                self.active = prev;
                self.status = prev_status;
                self.prune_lan_confirmed();
                Err(error)
            }
        }
    }

    pub fn stop_all(&mut self) {
        if let Some(error) = self.stop_all_result() {
            self.last_error = Some(error.to_string());
        }
    }

    fn apply(&mut self) -> Result<(), EngineError> {
        if self.active.is_empty() {
            self.backend.stop();
            self.fork_backend.stop();
            self.xray_backend.stop();
            for slot in self.extra_backends.iter_mut() {
                slot.stop();

                slot.clear_extra_exe();
            }

            self.served = [None, None, None];
            for served in self.served_extra.iter_mut() {
                *served = None;
            }

            self.clear_clash_secrets();
            self.status = EngineStatus::Stopped;
            self.last_config_json = None;
            self.crashed.clear();
            self.pending_crash = None;
            self.restart_attempts = 0;
            self.last_restart_attempt = None;
            return Ok(());
        }

        let plan = validate_activation(&self.active).inspect_err(|error| {
            self.last_error = Some(error.to_string());
        })?;

        if let Err(error) = validate_egress_resolution(&self.active) {
            self.last_error = Some(error.to_string());
            return Err(error);
        }

        let needs_stock = !plan.main_rows.is_empty()
            || plan
                .replacements
                .iter()
                .any(|(owned, _)| *owned == OwningCore::Stock);
        if needs_stock {
            match (self.stock_probe)() {
                Ok(path) => self.backend.pin_binary(path),
                Err(error) => {
                    self.last_error = Some(error.to_string());
                    return Err(error);
                }
            }
        }
        if !plan.fork_rows.is_empty() {
            match (self.fork_probe)() {
                Ok(path) => self.fork_backend.pin_binary(path),
                Err(error) => {
                    let error = missing_fork_for(error, &plan.fork_names, plan.fork_xhttp);
                    self.last_error = Some(error.to_string());
                    return Err(error);
                }
            }
        }
        if !plan.egresses.is_empty() {
            match (self.xray_probe)() {
                Ok(path) => self.xray_backend.pin_binary(path),
                Err(error) => {
                    let error = missing_xray_for(error, &plan.xray_names);
                    self.last_error = Some(error.to_string());
                    return Err(error);
                }
            }
        }

        while self.extra_backends.len() < plan.extras.len() {
            self.extra_backends
                .push(Box::new(RealEngineBackend::extra_slot()));
            self.served_extra.push(None);
        }
        let mut extra_paths: Vec<Option<PathBuf>> = Vec::with_capacity(plan.extras.len());
        for row in &plan.extras {
            let (exe, _) = validate_extra_row(row).inspect_err(|error| {
                self.last_error = Some(error.to_string());
            })?;
            match (self.extra_probe)(&exe) {
                Ok(path) => extra_paths.push(Some(path)),
                Err(error) => {
                    let error = missing_extra_for(error, &plan.extra_names);
                    self.last_error = Some(error.to_string());
                    return Err(error);
                }
            }
        }

        let replacement = |core: OwningCore| {
            plan.replacements
                .iter()
                .find(|(owned, _)| *owned == core)
                .map(|(_, json)| json.clone())
        };

        let mut plan_bridges: HashMap<String, u16> = HashMap::with_capacity(plan.egresses.len());
        for egress in &plan.egresses {
            plan_bridges.insert(egress.bridge_key.clone(), egress.bridge_port);
        }
        let mut stock_gen: Option<String> = match replacement(OwningCore::Stock) {
            Some(json) => Some(json),
            None if plan.main_rows.is_empty() => None,
            None => Some(
                build_main_config(
                    &plan.main_rows,
                    &plan_bridges,
                    &self.preset_rules,
                    &self.rule_set_files,
                )
                .inspect_err(|error| {
                    self.last_error = Some(error.to_string());
                })?
                .json,
            ),
        };
        let mut fork_gen: Option<String> = match replacement(OwningCore::Fork) {
            Some(json) => Some(json),
            None if plan.fork_rows.is_empty() => None,
            None => Some(
                generate_with_rule_sets(&plan.fork_rows, &self.preset_rules, &self.rule_set_files)
                    .inspect_err(|error| {
                        self.last_error = Some(error.to_string());
                    })?
                    .json,
            ),
        };

        let port_window_guard = port_window_lock();
        let mut clash_holds: Vec<std::net::TcpListener> = Vec::new();
        let mut clash: Vec<ClashTarget> = Vec::new();
        for (core, gen, rows_present) in [
            ("stock", &mut stock_gen, !plan.main_rows.is_empty()),
            ("fork", &mut fork_gen, !plan.fork_rows.is_empty()),
        ] {
            let generated = match core {
                "stock" => replacement(OwningCore::Stock).is_none() && rows_present,
                _ => replacement(OwningCore::Fork).is_none() && rows_present,
            };
            let Some(json) = gen.as_mut() else {
                continue;
            };
            if !generated {
                continue;
            }
            let port = reserve_clash_port(&mut clash_holds).ok_or_else(|| {
                let error = EngineError::PortUnavailable {
                    bind: "127.0.0.1".to_string(),
                    port: CLASH_PORT_START,
                    name: format!("clash controller ({core}, range exhausted)"),
                };
                self.last_error = Some(error.to_string());
                error
            })?;
            let secret = clash_secret(core);
            *json = inject_clash_api(json, port, &secret).inspect_err(|error| {
                self.last_error = Some(error.to_string());
            })?;
            clash.push(ClashTarget {
                core: core.to_string(),
                port,
                secret,
            });
        }
        let xray_gen: Option<String> = match replacement(OwningCore::Xray) {
            Some(json) => Some(json),
            None if plan.egresses.is_empty() => None,
            None => Some(
                generate_xray_config(&plan.egresses)
                    .inspect_err(|error| {
                        self.last_error = Some(error.to_string());
                    })?
                    .json,
            ),
        };

        let mut extra_gens: Vec<Option<String>> = Vec::with_capacity(plan.extras.len());
        for row in &plan.extras {
            extra_gens.push(Some(extra_config_json(row).inspect_err(|error| {
                self.last_error = Some(error.to_string());
            })?));
        }

        if !plan.egresses.is_empty() {
            let xray_json = xray_gen
                .as_deref()
                .ok_or_else(|| EngineError::XrayBridgeMismatch {
                    detail: "xray sidecar has no config for active bridges".to_string(),
                })?;
            let parsed: serde_json::Value =
                serde_json::from_str(xray_json).map_err(|_| EngineError::XrayBridgeMismatch {
                    detail: "xray sidecar config is not JSON".to_string(),
                })?;
            let ingress = parsed
                .get("inbounds")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            let tags: HashSet<&str> = plan
                .egresses
                .iter()
                .map(|egress| egress.egress_tag.as_str())
                .collect();
            let egress_hits = parsed
                .get("outbounds")
                .and_then(serde_json::Value::as_array)
                .map(|nodes| {
                    nodes
                        .iter()
                        .filter(|node| {
                            node.get("tag")
                                .and_then(serde_json::Value::as_str)
                                .is_some_and(|tag| tags.contains(tag))
                        })
                        .count()
                })
                .unwrap_or(0);
            validate_bridge_counts(ingress, egress_hits, plan.egresses.len(), "xray sidecar")
                .inspect_err(|error| {
                    self.last_error = Some(error.to_string());
                })?;
            let main_json =
                stock_gen
                    .as_deref()
                    .ok_or_else(|| EngineError::XrayBridgeMismatch {
                        detail: "main sidecar has no config for active bridges".to_string(),
                    })?;
            validate_bridges_present(main_json, &plan.egresses).inspect_err(|error| {
                self.last_error = Some(error.to_string());
            })?;
        }

        for (core, json) in &plan.replacements {
            let document: serde_json::Value =
                serde_json::from_str(json).map_err(|_| EngineError::BadCustom {
                    name: format!("custom full-config ({})", core.as_str()),
                    field: "config".to_string(),
                    detail: "custom full config is not JSON".to_string(),
                })?;
            if let Some((key, path)) = find_system_mutation_key(&document) {
                let error = EngineError::BadCustom {
                    name: format!("custom full-config ({})", core.as_str()),
                    field: key.clone(),
                    detail: format!(
                        "custom full config must not carry \"{key}\" at {path} (no system network mutation)"
                    ),
                };
                self.last_error = Some(error.to_string());
                return Err(error);
            }
        }

        let mut bridge_holds: Vec<std::net::TcpListener> = Vec::new();
        {
            let mut seen: HashSet<u16> = HashSet::new();
            for egress in &plan.egresses {
                if !seen.insert(egress.bridge_port) {
                    continue;
                }
                match std::net::TcpListener::bind(format!("127.0.0.1:{}", egress.bridge_port)) {
                    Ok(listener) => bridge_holds.push(listener),
                    Err(error) => {
                        let failure = EngineError::PortUnavailable {
                            bind: "127.0.0.1".to_string(),
                            port: egress.bridge_port,
                            name: format!("xray bridge ({})", egress.owner),
                        };
                        self.last_error = Some(format!("{failure} (bridge hold failed: {error})"));
                        return Err(failure);
                    }
                }
            }
        }

        let mut probe_holds: Vec<std::net::TcpListener> = Vec::new();
        {
            let mut own: std::collections::HashSet<(String, u16)> =
                std::collections::HashSet::new();
            for served in self
                .served
                .iter()
                .flatten()
                .chain(self.served_extra.iter().flatten())
            {
                own.extend(
                    served_listen_ports(served)
                        .into_iter()
                        .map(|(bind, port)| (normalize_bind_key(&bind), port)),
                );
            }
            let mut endpoints: Vec<(String, String, u16)> = Vec::new();
            for row in plan.main_rows.iter().chain(plan.fork_rows.iter()) {
                endpoints.push((row.name.clone(), row.bind_ip.clone(), row.port));
            }
            for row in &plan.extras {
                endpoints.push((row.name.clone(), row.bind_ip.clone(), row.port));
            }
            for egress in &plan.egresses {
                endpoints.push((
                    egress.owner.clone(),
                    "127.0.0.1".to_string(),
                    egress.bridge_port,
                ));
            }

            let held: std::collections::HashSet<u16> =
                clash.iter().map(|target| target.port).collect();
            let bridge_ports: std::collections::HashSet<u16> = plan
                .egresses
                .iter()
                .map(|egress| egress.bridge_port)
                .collect();
            for target in &clash {
                endpoints.push((
                    format!("clash controller ({})", target.core),
                    "127.0.0.1".to_string(),
                    target.port,
                ));
            }

            for (owner, bind, port) in &plan.replacement_listens {
                endpoints.push((
                    format!("custom full-config (\"{owner}\")"),
                    bind.clone(),
                    *port,
                ));
            }
            for (name, bind, port) in &endpoints {
                if own.contains(&(normalize_bind_key(bind), *port)) {
                    continue;
                }
                if name.starts_with("clash controller (") && held.contains(port) {
                    continue;
                }

                if normalize_bind_key(bind) == "loopback" && bridge_ports.contains(port) {
                    continue;
                }
                match probe_bind_available(bind, *port) {
                    Ok(listener) => probe_holds.push(listener),
                    Err(_) => {
                        let error = EngineError::PortUnavailable {
                            bind: bind.clone(),
                            port: *port,
                            name: name.clone(),
                        };
                        self.last_error = Some(error.to_string());
                        return Err(error);
                    }
                }
            }
        }
        for (label, backend, gen) in [
            ("stock", &mut self.backend, &stock_gen),
            ("fork", &mut self.fork_backend, &fork_gen),
            ("xray", &mut self.xray_backend, &xray_gen),
        ] {
            if let Some(json) = gen {
                if let Err(mut detail) = backend.check_config(json) {
                    if label == "fork" && plan.fork_xhttp {
                        detail = format!("{detail} ({XHTTP_KNOWN_BUGS})");
                    }

                    detail = strip_ansi_codes(&detail);
                    let error = EngineError::CheckFailed { detail };
                    self.last_error = Some(error.to_string());
                    return Err(error);
                }
            }
        }

        for (slot, gen) in self.extra_backends.iter_mut().zip(extra_gens.iter()) {
            if let Some(json) = gen {
                if let Err(detail) = slot.check_config(json) {
                    let error = EngineError::CheckFailed {
                        detail: strip_ansi_codes(&detail),
                    };
                    self.last_error = Some(error.to_string());
                    return Err(error);
                }
            } else {
                continue;
            }
        }

        for (index, slot) in self.extra_backends.iter_mut().enumerate() {
            if let Some(path) = extra_paths.get(index).and_then(|path| path.as_ref()) {
                slot.set_extra_exe(path.clone());
            } else {
                slot.clear_extra_exe();
            }
        }

        drop(probe_holds);
        drop(bridge_holds);
        drop(clash_holds);
        drop(port_window_guard);

        const SHARDS: [&str; 3] = ["stock", "fork", "xray"];
        let gens: [Option<String>; 3] = [stock_gen, fork_gen, xray_gen];
        let prev_served = self.served.clone();
        let prev_served_extra = self.served_extra.clone();
        let mut failures: Vec<(String, EngineError)> = Vec::new();
        for index in 0..3 {
            let json = gens[index].clone();
            match json {
                Some(json) => {
                    let backend = self.backend_at(index);
                    backend.stop();
                    match backend.start(&json) {
                        Ok(()) => {
                            self.served[index] = Some(json);
                        }
                        Err(detail) => {
                            self.served[index] = None;
                            failures.push((
                                SHARDS[index].to_string(),
                                EngineError::SpawnFailed { detail },
                            ));
                        }
                    }
                }
                None => {
                    let backend = self.backend_at(index);
                    backend.stop();
                    self.served[index] = None;
                }
            }
        }

        for (index, gen) in extra_gens.iter().enumerate() {
            let label = format!(
                "extra-{}",
                plan.extras
                    .get(index)
                    .map(|row| row.id.as_str())
                    .unwrap_or("?")
            );
            match gen {
                Some(json) => {
                    let slot = &mut self.extra_backends[index];
                    slot.stop();
                    match slot.start(json) {
                        Ok(()) => {
                            if index < self.served_extra.len() {
                                self.served_extra[index] = Some(json.clone());
                            }
                        }
                        Err(detail) => {
                            if index < self.served_extra.len() {
                                self.served_extra[index] = None;
                            }
                            failures.push((label, EngineError::SpawnFailed { detail }));
                        }
                    }
                }
                None => {
                    if let Some(slot) = self.extra_backends.get_mut(index) {
                        slot.stop();
                    }
                    if index < self.served_extra.len() {
                        self.served_extra[index] = None;
                    }
                }
            }
        }
        for index in extra_gens.len()..self.extra_backends.len() {
            if let Some(slot) = self.extra_backends.get_mut(index) {
                slot.stop();
            }
            if index < self.served_extra.len() {
                self.served_extra[index] = None;
            }
        }
        if !failures.is_empty() {
            let mut rollback_errors: Vec<String> = Vec::new();
            for (index, prev) in prev_served.clone().into_iter().enumerate() {
                let backend = self.backend_at(index);
                backend.stop();
                match prev {
                    Some(prev) => match backend.start(&prev) {
                        Ok(()) => self.served[index] = Some(prev),
                        Err(error) => {
                            self.served[index] = None;
                            rollback_errors
                                .push(format!("rollback {} failed: {error}", SHARDS[index]));
                        }
                    },
                    None => self.served[index] = None,
                }
            }

            for (index, prev) in prev_served_extra.clone().into_iter().enumerate() {
                if let Some(slot) = self.extra_backends.get_mut(index) {
                    slot.stop();
                    match prev {
                        Some(prev) => match slot.start(&prev) {
                            Ok(()) => {
                                if index < self.served_extra.len() {
                                    self.served_extra[index] = Some(prev);
                                }
                            }
                            Err(error) => {
                                if index < self.served_extra.len() {
                                    self.served_extra[index] = None;
                                }
                                rollback_errors
                                    .push(format!("rollback extra-{index} failed: {error}"));
                            }
                        },
                        None => {
                            if index < self.served_extra.len() {
                                self.served_extra[index] = None;
                            }
                        }
                    }
                }
            }
            let mut detail = failures
                .iter()
                .map(|(shard, error)| format!("{shard}: {error}"))
                .collect::<Vec<_>>()
                .join("; ");
            if !rollback_errors.is_empty() {
                detail = format!("{detail}; {}", rollback_errors.join("; "));
            }
            self.last_error = Some(detail);
            let (_, first) = failures.into_iter().next().expect("failures non-empty");

            self.sync_clash_from_served();
            self.status = if self.any_backend_running() {
                EngineStatus::Running
            } else {
                EngineStatus::Stopped
            };
            return Err(first);
        }

        self.status = if self.any_backend_running() {
            EngineStatus::Running
        } else {
            EngineStatus::Stopped
        };
        self.last_config_json = gens[0]
            .clone()
            .or_else(|| gens[1].clone())
            .or_else(|| gens[2].clone())
            .or_else(|| extra_gens.iter().find_map(|gen| gen.clone()));

        self.sync_clash_from_served();
        self.last_error = None;
        self.reap_recovered_crashes();
        Ok(())
    }

    pub fn poll_shards(&mut self) -> Vec<(String, String)> {
        let mut crashes = Vec::new();
        for (shard, backend) in [
            ("stock", &mut self.backend),
            ("fork", &mut self.fork_backend),
            ("xray", &mut self.xray_backend),
        ] {
            if let Some(cause) = backend.poll_exit() {
                crashes.push((shard.to_string(), cause));
            }
        }
        let extra_ids: Vec<String> = self
            .active
            .iter()
            .filter(|row| is_extra_core_protocol(&row.protocol))
            .map(|row| row.id.clone())
            .collect();
        for (index, slot) in self.extra_backends.iter_mut().enumerate() {
            if let Some(cause) = slot.poll_exit() {
                let shard = extra_ids
                    .get(index)
                    .map(|id| format!("extra-{id}"))
                    .unwrap_or_else(|| format!("extra-{index}"));
                crashes.push((shard, cause));
            }
        }
        if crashes.is_empty() || self.active.is_empty() {
            return crashes;
        }

        for (shard, _) in &crashes {
            self.drop_crashed_served(shard);
            self.crashed.insert(shard.clone());
        }
        self.totals_stale = true;
        self.clear_clash_secrets();
        self.sync_clash_from_served();
        let cause = if crashes.len() == 1 {
            crashes.first().expect("one crash").1.clone()
        } else {
            crashes
                .iter()
                .map(|(shard, cause)| format!("{shard}: {cause}"))
                .collect::<Vec<_>>()
                .join("; ")
        };
        self.status = EngineStatus::Crashed {
            cause: cause.clone(),
        };
        self.last_error = Some(format!("engine crashed ({cause})"));
        self.pending_crash = Some(Self::crash_detail(&crashes));
        crashes
    }

    /// T-30: `(shard label, pid)` per shard slot, named exactly like
    /// [`Self::poll_shards`] — "stock", "fork", "xray", "extra-<row id>" (or
    /// "extra-<slot>" when no row names it). `None` = that shard has no live
    /// child. The caller only ever sees pids this controller spawned; nothing
    /// here enumerates foreign processes.
    pub fn shard_processes(&self) -> Vec<(String, Option<u32>)> {
        let mut out = vec![
            ("stock".to_string(), self.backend.process_id()),
            ("fork".to_string(), self.fork_backend.process_id()),
            ("xray".to_string(), self.xray_backend.process_id()),
        ];
        let extra_ids: Vec<String> = self
            .active
            .iter()
            .filter(|row| is_extra_core_protocol(&row.protocol))
            .map(|row| row.id.clone())
            .collect();
        for (index, slot) in self.extra_backends.iter().enumerate() {
            let shard = extra_ids
                .get(index)
                .map(|id| format!("extra-{id}"))
                .unwrap_or_else(|| format!("extra-{index}"));
            out.push((shard, slot.process_id()));
        }
        out
    }

    pub fn crash_detail(crashes: &[(String, String)]) -> String {
        crashes
            .iter()
            .map(|(shard, cause)| format!("{shard}: {cause}"))
            .collect::<Vec<_>>()
            .join("; ")
    }

    pub fn poll(&mut self) -> Option<String> {
        self.poll_shards();
        self.pending_crash()
    }

    pub fn drain_log_lines(&mut self) -> Vec<String> {
        let mut lines = self.backend.drain_log_lines();
        lines.extend(self.fork_backend.drain_log_lines());
        lines.extend(self.xray_backend.drain_log_lines());
        for slot in self.extra_backends.iter_mut() {
            lines.extend(slot.drain_log_lines());
        }
        lines
    }

    pub fn running_ids(&self) -> Vec<String> {
        self.active.iter().map(|row| row.id.clone()).collect()
    }

    pub fn running_count(&self) -> usize {
        self.active.len()
    }

    #[cfg(test)]
    pub fn clash_targets(&self) -> &[ClashTarget] {
        &self.clash
    }

    #[cfg(test)]
    pub fn set_clash_targets(&mut self, targets: Vec<ClashTarget>) {
        self.clash = targets;
    }

    fn sync_clash_from_served(&mut self) {
        const CORES: [&str; 2] = ["stock", "fork"];
        self.clash = CORES
            .into_iter()
            .zip(self.served.iter().take(2))
            .filter_map(|(core, served)| {
                served
                    .as_deref()
                    .and_then(|json| clash_target_from_served(core, json))
            })
            .collect();
    }

    pub fn is_running(&self, id: &str) -> bool {
        let Some(row) = self.active.iter().find(|row| row.id == id) else {
            return false;
        };

        if is_extra_core_protocol(&row.protocol) {
            return self
                .extra_slot_index(id)
                .and_then(|index| self.extra_backends.get(index))
                .is_some_and(|slot| slot.is_running());
        }
        if is_custom_config_protocol(&row.protocol) {
            let Some(core) = Self::custom_core_of(row) else {
                return false;
            };
            return match core {
                OwningCore::Stock => self.backend.is_running(),
                OwningCore::Fork => self.fork_backend.is_running(),
                OwningCore::Xray => self.xray_backend.is_running(),
            };
        }
        if needs_xray(row) {
            return self.xray_backend.is_running();
        }
        if is_custom_outbound_protocol(&row.protocol) {
            let Some(core) = Self::custom_core_of(row) else {
                return false;
            };
            return match core {
                OwningCore::Stock => self.backend.is_running(),
                OwningCore::Fork => self.fork_backend.is_running(),
                OwningCore::Xray => self.xray_backend.is_running(),
            };
        }
        if needs_fork(row) {
            return self.fork_backend.is_running();
        }
        self.backend.is_running()
    }

    #[cfg(test)]
    pub fn status(&self) -> &EngineStatus {
        &self.status
    }

    pub fn status_text(&self) -> String {
        match &self.status {
            EngineStatus::Stopped => "engine stopped".to_string(),
            EngineStatus::Running => {
                format!("engine running · {} endpoint(s)", self.active.len())
            }
            EngineStatus::Crashed { cause } => format!("engine crashed ({cause})"),
        }
    }

    #[cfg(test)]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    #[cfg(test)]
    pub fn isolation_map(&self) -> Vec<IsolationEntry> {
        self.active
            .iter()
            .map(|row| IsolationEntry {
                config_id: row.id.clone(),
                inbound_tag: inbound_tag(&row.id),
                outbound_tag: outbound_tag(&row.id),
            })
            .collect()
    }

    pub fn lan_warnings(&self) -> Vec<String> {
        self.active
            .iter()
            .filter(|row| !is_loopback_bind(&row.bind_ip))
            .map(|row| format!("\"{}\" serves on {} (LAN-visible)", row.name, row.bind_ip))
            .collect()
    }

    #[cfg(test)]
    pub fn session_fingerprint(&self) -> SessionFingerprint {
        let mut binds: Vec<String> = self
            .active
            .iter()
            .map(|row| format!("{}:{}", row.bind_ip.to_ascii_lowercase(), row.port))
            .collect();
        binds.sort();
        SessionFingerprint {
            binds,
            config_hash: fnv1a_64(self.last_config_json.as_deref().unwrap_or("")),
        }
    }
}

pub fn is_loopback_bind(bind: &str) -> bool {
    let trimmed = bind.trim();
    if trimmed.eq_ignore_ascii_case("localhost")
        || trimmed
            .strip_suffix('.')
            .is_some_and(|bare| bare.eq_ignore_ascii_case("localhost"))
    {
        return true;
    }
    trimmed
        .parse::<std::net::IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

pub fn lan_exposure_warning(name: &str, bind: &str) -> Option<String> {
    if is_loopback_bind(bind) {
        None
    } else {
        Some(format!(
            "\"{name}\" binds {bind}: LAN-visible; anyone on the network can use this proxy"
        ))
    }
}

pub const LAN_CONFIRM_TITLE: &str = "Expose proxy on the local network?";

pub fn requires_lan_confirm(bind: &str) -> bool {
    !is_loopback_bind(bind)
}

pub fn lan_confirm_text(name: &str, bind: &str) -> String {
    format!(
        "\"{name}\" will listen on {bind}, reachable by anyone on the local network. Start it anyway?"
    )
}

pub fn lan_confirm_description(entries: &[(String, String)]) -> String {
    let mut lines: Vec<String> = entries
        .iter()
        .map(|(name, bind)| format!("\"{name}\" on {bind}"))
        .collect();
    lines.sort();
    format!(
        "These endpoints will be LAN-visible (anyone on the local network can use them): {}. Start them anyway?",
        lines.join(", ")
    )
}

#[cfg(test)]
pub fn assert_no_system_mutation(config_json: &str) -> Result<(), String> {
    let document: serde_json::Value = serde_json::from_str(config_json)
        .map_err(|_| "generated config is not JSON".to_string())?;
    if let Some((key, path)) = find_system_mutation_key(&document) {
        return Err(format!("forbidden key \"{key}\" at {path}"));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(test)]
pub struct SessionFingerprint {
    pub binds: Vec<String>,
    pub config_hash: u64,
}

#[cfg(test)]
fn fnv1a_64(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture(
        id: &str,
        name: &str,
        protocol: &str,
        params: serde_json::Value,
        bind: &str,
        port: u16,
    ) -> ProxyConfig {
        ProxyConfig {
            id: id.to_string(),
            name: name.to_string(),
            protocol: protocol.to_string(),
            params: params.to_string(),
            group: "default".to_string(),
            enabled: true,
            bind_ip: bind.to_string(),
            port,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        }
    }

    fn ss_params(password: &str) -> serde_json::Value {
        json!({"type": "shadowsocks", "server": "203.0.113.7", "server_port": 8388,
               "method": "aes-128-gcm", "password": password})
    }

    fn vless_params() -> serde_json::Value {
        json!({"type": "vless", "server": "198.51.100.9", "server_port": 443,
               "uuid": "11111111-2222-4333-8444-555555555555", "encryption": "none",
               "security": "tls", "sni": "198.51.100.9", "network": "tcp"})
    }

    fn wg_params() -> serde_json::Value {
        json!({"type": "wireguard", "server": "192.0.2.44", "server_port": 51820,
               "private_key": "LOCAL-PRIVATE", "public_key": "REMOTE-PUBLIC"})
    }

    fn three_standard() -> Vec<ProxyConfig> {
        vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            ),
            fixture("2", "Beta", "vless", vless_params(), "127.0.0.1", 10802),
            fixture("3", "Gamma", "wireguard", wg_params(), "127.0.0.1", 10803),
        ]
    }

    #[test]
    fn three_configs_yield_three_mixed_inbounds() {
        let generated = generate(&three_standard()).expect("standard set generates");
        let value = generated.value();
        let inbounds = value["inbounds"].as_array().expect("inbounds array");
        assert_eq!(inbounds.len(), 3);
        let mut tags = HashSet::new();
        let mut ports = HashSet::new();
        for inbound in inbounds {
            assert_eq!(inbound["type"], json!("mixed"));
            assert_eq!(inbound["listen"], json!("127.0.0.1"));
            assert!(tags.insert(inbound["tag"].as_str().unwrap().to_string()));
            assert!(ports.insert(inbound["listen_port"].as_u64().unwrap()));
        }
    }

    #[test]
    fn route_rules_isolate_each_endpoint_with_direct_final() {
        let rows = three_standard();
        let generated = generate(&rows).expect("standard set generates");
        let value = generated.value();
        let rules = value["route"]["rules"].as_array().expect("rules array");
        assert_eq!(rules.len(), 3);
        for (row, rule) in rows.iter().zip(rules.iter()) {
            assert_eq!(rule["inbound"], json!([inbound_tag(&row.id)]));
            assert_eq!(rule["outbound"], json!(outbound_tag(&row.id)));
            assert_eq!(rule["action"], json!("route"));
        }
        assert_eq!(value["route"]["final"], json!("direct"));
        let outbound_tags: HashSet<String> = value["outbounds"]
            .as_array()
            .expect("outbounds array")
            .iter()
            .filter_map(|node| node["tag"].as_str().map(str::to_string))
            .collect();
        assert!(outbound_tags.contains("direct"));
        for row in &rows {
            let tag = outbound_tag(&row.id);
            if value["endpoints"]
                .as_array()
                .expect("endpoints array")
                .iter()
                .any(|node| node["tag"].as_str() == Some(tag.as_str()))
            {
                continue;
            }
            assert!(outbound_tags.contains(&tag), "outbound tag resolves: {tag}");
        }
    }

    #[test]
    fn wireguard_becomes_endpoint_not_outbound() {
        let generated = generate(&three_standard()).expect("standard set generates");
        let value = generated.value();
        let endpoints = value["endpoints"].as_array().expect("endpoints array");
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0]["type"], json!("wireguard"));
        assert_eq!(endpoints[0]["tag"], json!(outbound_tag("3")));
        let outbound_tags: Vec<String> = value["outbounds"]
            .as_array()
            .expect("outbounds array")
            .iter()
            .filter_map(|node| node["tag"].as_str().map(str::to_string))
            .collect();
        assert!(!outbound_tags.contains(&outbound_tag("3")));
        assert_eq!(outbound_tags.len(), 3);
        assert!(outbound_tags.contains(&outbound_tag("1")));
    }

    #[test]
    fn ticket36_inject_clash_api() {
        let doc = serde_json::json!({
            "log": {"level": "warn"},
            "inbounds": [], "outbounds": [],
            "endpoints": [], "route": {"rules": [], "final": "direct"},
        });
        let out = inject_clash_api(&doc.to_string(), 18501, "s3cret").expect("injects");
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("stays JSON");
        assert_eq!(
            parsed["experimental"]["clash_api"]["external_controller"],
            serde_json::json!("127.0.0.1:18501")
        );
        assert_eq!(
            parsed["experimental"]["clash_api"]["secret"],
            serde_json::json!("s3cret")
        );
        assert!(
            inject_clash_api("not json", 18501, "s3cret").is_err(),
            "garbage fails typed"
        );
    }

    #[test]
    fn ticket36_served_listen_ports() {
        let doc = serde_json::json!({
            "inbounds": [
                {"type": "mixed", "tag": "in-1", "listen": "127.0.0.1", "listen_port": 10801},
                {"type": "mixed", "tag": "in-2", "listen": "0.0.0.0", "listen_port": 10802},
                {"type": "mixed", "tag": "no-port"},
            ],
        });
        assert_eq!(
            served_listen_ports(&doc.to_string()),
            vec![
                ("127.0.0.1".to_string(), 10801),
                ("0.0.0.0".to_string(), 10802),
            ]
        );
        assert!(
            served_listen_ports("not json").is_empty(),
            "garbage fails open"
        );
    }

    #[test]
    fn ticket36_missing_or_blank_listen_defaults_to_wildcard() {
        let doc = serde_json::json!({
            "inbounds": [
                {"type": "mixed", "tag": "in-1", "listen_port": 10901},
                {"type": "mixed", "tag": "in-2", "listen": "", "listen_port": 10902},
                {"type": "mixed", "tag": "in-3", "listen": "   ", "listen_port": 10903},
                {"type": "mixed", "tag": "in-4", "listen": "127.0.0.1", "listen_port": 10904},
                {"type": "mixed", "tag": "no-port"},
            ],
        });
        assert_eq!(
            custom_config_listens(&doc),
            vec![
                ("0.0.0.0".to_string(), 10901),
                ("0.0.0.0".to_string(), 10902),
                ("0.0.0.0".to_string(), 10903),
                ("127.0.0.1".to_string(), 10904),
            ]
        );
    }

    #[test]
    fn ticket36_wildcard_custom_listen_still_conflicts_same_bind() {
        let row = fixture(
            "7",
            "CustomA",
            "custom-config",
            json!({
                "core": "stock",
                "config": {
                    "outbounds": [{"type": "direct", "tag": "direct"}],
                    "inbounds": [{"type": "mixed", "tag": "in-x", "listen_port": 10991}],
                }
            }),
            "127.0.0.1",
            10801,
        );
        let second = fixture(
            "8",
            "CustomB",
            "custom-config",
            json!({
                "core": "fork",
                "config": {
                    "outbounds": [{"type": "direct", "tag": "direct"}],
                    "inbounds": [{"type": "mixed", "tag": "in-y", "listen_port": 10991}],
                }
            }),
            "127.0.0.1",
            10802,
        );
        let rows = vec![row, second];
        let error = validate_activation(&rows).expect_err("shared wildcard port conflicts");
        assert!(
            matches!(error, EngineError::PortConflict { .. }),
            "got {error}"
        );
        let text = error.to_string();
        assert!(text.contains("CustomA"), "names both: {text}");
        assert!(text.contains("CustomB"), "names both: {text}");
        assert!(text.contains("10991"), "names port: {text}");
    }

    #[test]
    fn ticket36_reapply_skips_own_served_ports() {
        fn free_port() -> u16 {
            std::net::TcpListener::bind("127.0.0.1:0")
                .expect("probe bind works")
                .local_addr()
                .expect("addr works")
                .port()
        }
        let (mut controller, _stock, _fork) = EngineController::fake_pair();
        let first_port = free_port();
        let rows = vec![fixture(
            "1",
            "Alpha",
            "shadowsocks",
            ss_params("pass1"),
            "127.0.0.1",
            first_port,
        )];
        controller.start_all(&rows).expect("first start works");

        let held =
            std::net::TcpListener::bind(format!("127.0.0.1:{first_port}")).expect("holds port");
        controller
            .start_all(&rows)
            .expect("re-apply skips own ports");
        let second_port = free_port();
        let two = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                first_port,
            ),
            fixture(
                "2",
                "Beta",
                "shadowsocks",
                ss_params("pass2"),
                "127.0.0.1",
                second_port,
            ),
        ];
        controller.start_all(&two).expect("second row starts");
        assert!(controller.stop_one("1"), "stop one of two works");
        assert!(controller.is_running("2"), "other row keeps serving");

        let foreign = std::net::TcpListener::bind("127.0.0.1:0").expect("holds a port");
        let blocked_port = foreign.local_addr().expect("addr works").port();
        let blocked = vec![fixture(
            "9",
            "Blocked",
            "shadowsocks",
            ss_params("pass9"),
            "127.0.0.1",
            blocked_port,
        )];
        let error = controller
            .start_all(&blocked)
            .expect_err("foreign hold fails");
        assert!(
            error.to_string().contains(&blocked_port.to_string()),
            "names the port, got {error}"
        );
        drop(held);
        drop(foreign);
    }

    #[test]
    fn ticket36_live_clash_totals_move() {
        if std::env::var("MANYPROXY_LIVE_TRAFFIC").unwrap_or_default() != "1" {
            return;
        }
        use std::time::{Duration, Instant};
        let binary = std::env::var("MANYPROXY_SINGBOX_BIN").map_or_else(
            |_| {
                if cfg!(windows) {
                    PathBuf::from("target/debug/sing-box.exe")
                } else {
                    PathBuf::from("target/debug/sing-box")
                }
            },
            PathBuf::from,
        );
        fn free_port() -> u16 {
            std::net::TcpListener::bind("127.0.0.1:0")
                .expect("probe bind works")
                .local_addr()
                .expect("addr works")
                .port()
        }

        let upstream = std::net::TcpListener::bind("127.0.0.1:0").expect("upstream binds");
        let upstream_port = upstream.local_addr().expect("addr works").port();
        upstream.set_nonblocking(true).expect("nonblocking works");
        let (count_tx, count_rx) = std::sync::mpsc::channel::<u32>();
        let upstream_handle = std::thread::spawn(move || {
            let want = count_rx.recv().unwrap_or(0);
            let deadline = Instant::now() + Duration::from_secs(15);
            let mut taken = 0u32;
            while taken < want && Instant::now() < deadline {
                match upstream.accept() {
                    Ok((peer, _)) => {
                        taken += 1;
                        drop(peer);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    Err(_) => break,
                }
            }
        });
        let inbound_port = free_port();
        let clash_port = free_port();
        let secret = "live-traffic-s3cret";
        let row = fixture(
            "7",
            "TrafficLive",
            "shadowsocks",
            json!({"type": "shadowsocks", "server": "127.0.0.1", "server_port": upstream_port,
                   "method": "aes-128-gcm", "password": "live-pass-1"}),
            "127.0.0.1",
            inbound_port,
        );
        let generated = generate(std::slice::from_ref(&row)).expect("single row generates");
        let injected =
            inject_clash_api(&generated.json, clash_port, secret).expect("clash injects");
        let path = std::env::temp_dir().join(format!(
            "manyproxy-t36-live-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, &injected).expect("temp config writes");
        let mut cmd = std::process::Command::new(&binary);
        cmd.arg("check").arg("-c").arg(&path);
        spawn_hidden(&mut cmd);
        let check = cmd.output().expect("check runs");
        let _ = std::fs::remove_file(&path);
        assert!(
            check.status.success(),
            "PROBE live-traffic: sing-box check EXIT 0, stderr: {}",
            String::from_utf8_lossy(&check.stderr)
        );
        std::fs::write(&path, &injected).expect("temp config writes");
        let mut cmd = std::process::Command::new(&binary);
        cmd.arg("run")
            .arg("-c")
            .arg(&path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        spawn_hidden(&mut cmd);
        let mut child = cmd.spawn().expect("sidecar runs");
        let result = (|| -> Result<String, String> {
            use std::io::{Read as _, Write as _};

            let deadline = Instant::now() + Duration::from_secs(20);
            let (rx0, tx0) = loop {
                match crate::health::poll_clash_totals(clash_port, secret, Duration::from_secs(2)) {
                    Ok(totals) => break totals,
                    Err(error) if error.contains("unauthorized") => {
                        return Err(format!("live clash auth broken: {error}"));
                    }
                    Err(_) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(200));
                    }
                    Err(error) => {
                        return Err(format!("clash controller never came up: {error}"));
                    }
                }
            };

            if crate::health::poll_clash_totals(clash_port, "wrong", Duration::from_secs(5)).is_ok()
            {
                return Err("clash accepted a bad secret".to_string());
            }

            let request = "GET http://192.0.2.1/ HTTP/1.1\r\nHost: 192.0.2.1\r\n\
                 Connection: close\r\nUser-Agent: ManyProxy-live\r\n\r\n";
            let mut sent = 0u32;
            for _ in 0..8 {
                let endpoint = format!("127.0.0.1:{inbound_port}");
                let addr: std::net::SocketAddr = endpoint
                    .parse()
                    .map_err(|_| "bad inbound port".to_string())?;
                let mut stream =
                    std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(5))
                        .map_err(|error| format!("inbound dial failed ({error})"))?;
                stream.set_write_timeout(Some(Duration::from_secs(3))).ok();
                stream.set_read_timeout(Some(Duration::from_secs(3))).ok();
                stream
                    .write_all(request.as_bytes())
                    .map_err(|error| format!("inbound write failed ({error})"))?;
                sent += 1;
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
            }
            let _ = count_tx.send(sent);

            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let (rx1, tx1) =
                    crate::health::poll_clash_totals(clash_port, secret, Duration::from_secs(5))
                        .map_err(|error| format!("totals re-poll failed ({error})"))?;
                let delta_up = tx1.saturating_sub(tx0);
                if delta_up >= 10 * sent as u64 {
                    return Ok(format!(
                        "upload {tx0}->{tx1} (+{delta_up}B over {sent} requests), \
                         download {rx0}->{rx1}"
                    ));
                }
                if Instant::now() >= deadline {
                    return Err(format!(
                        "totals never moved: upload {tx0}->{tx1}, download {rx0}->{rx1} \
                         after {sent} requests"
                    ));
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        })();
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_file(&path);
        let _ = upstream_handle.join();
        assert!(
            std::env::temp_dir()
                .read_dir()
                .map(|entries| {
                    entries.filter_map(Result::ok).all(|entry| {
                        !entry
                            .file_name()
                            .to_string_lossy()
                            .starts_with("manyproxy-t36-live-")
                    })
                })
                .unwrap_or(false),
            "no temp litter left"
        );
        result.expect("PROBE live-traffic: clash totals move");
    }

    #[test]
    fn wireguard_endpoint_schema_matches_fork() {
        let row = fixture(
            "7",
            "AwgLive",
            "amneziawg",
            json!({"type": "wireguard", "server": "192.0.2.44", "server_port": 51820,
                   "private_key": "LOCAL", "public_key": "REMOTE",
                   "address": "100.64.134.1/32", "dns": "198.18.0.1",
                   "allowed_ips": "0.0.0.0/0", "awg_version": "2.0",
                   "jc": "5", "jmin": "30", "jmax": "1000"}),
            "127.0.0.1",
            10807,
        );
        let generated = generate(std::slice::from_ref(&row)).expect("wg generates");
        let value = generated.value();
        let endpoint = value["endpoints"]
            .as_array()
            .expect("endpoints array")
            .iter()
            .find(|node| node["tag"] == json!(outbound_tag("7")))
            .expect("wg endpoint present");
        assert!(endpoint.get("server").is_none(), "no outbound key");
        assert!(endpoint.get("server_port").is_none(), "no outbound key");
        assert!(endpoint.get("awg_version").is_none(), "no internal marker");
        assert!(endpoint.get("dns").is_none(), "no client setting");
        assert_eq!(endpoint["private_key"], json!("LOCAL"));
        assert_eq!(endpoint["address"], json!(["100.64.134.1/32"]));
        assert_eq!(endpoint["jc"], json!(5));
        assert_eq!(endpoint["jmin"], json!(30));
        assert_eq!(endpoint["jmax"], json!(1000));
        let peer = endpoint["peers"]
            .as_array()
            .expect("peers array")
            .first()
            .expect("one peer");
        assert_eq!(peer["address"], json!("192.0.2.44"));
        assert_eq!(peer["port"], json!(51820));
        assert_eq!(peer["public_key"], json!("REMOTE"));
        assert_eq!(peer["allowed_ips"], json!(["0.0.0.0/0"]));
    }

    #[test]
    fn unsupported_protocol_names_config_without_secrets() {
        let configs = vec![fixture(
            "9",
            "Odd",
            "mieru",
            json!({"server": "10.0.0.9", "server_port": 443, "username": "u", "password": "TOP-SECRET"}),
            "127.0.0.1",
            10809,
        )];
        let error = generate(&configs).expect_err("mieru is not standard");
        let text = error.to_string();
        assert!(text.contains("\"Odd\""), "names config: {text}");
        assert!(text.contains("mieru"), "names protocol: {text}");
        assert!(!text.contains("TOP-SECRET"), "no secrets: {text}");
    }

    #[test]
    fn duplicate_listen_port_names_both_configs() {
        let configs = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10821,
            ),
            fixture("2", "Beta", "vless", vless_params(), "127.0.0.1", 10821),
        ];
        let error = generate(&configs).expect_err("collision must fail");
        let text = error.to_string();
        assert!(text.contains("\"Alpha\""), "names holder: {text}");
        assert!(text.contains("\"Beta\""), "names newcomer: {text}");
        assert!(text.contains("10821"), "names port: {text}");
    }

    #[test]
    fn generated_json_has_no_tun_dns_or_system_proxy() {
        let generated = generate(&three_standard()).expect("standard set generates");
        for key in ["\"tun\"", "\"dns\"", "\"set_system_proxy\""] {
            assert!(
                !generated.json.contains(key),
                "forbidden key {key} in generated config"
            );
        }
        let top = generated.value();
        assert!(top.get("tun").is_none());
        assert!(top.get("dns").is_none());
    }

    #[test]
    fn controller_start_all_stop_one_restart_via_fake() {
        let (mut controller, handle) = EngineController::fake();
        let rows = three_standard();
        controller.start_all(&rows).expect("start-all works");
        assert_eq!(controller.running_count(), 3);
        assert_eq!(controller.status(), &EngineStatus::Running);
        assert!(handle.is_running());
        assert_eq!(handle.check_calls(), 1);

        assert!(controller.stop_one("2"));
        assert_eq!(controller.running_count(), 2);
        assert_eq!(
            controller.running_ids(),
            vec!["1".to_string(), "3".to_string()]
        );
        let live: serde_json::Value =
            serde_json::from_str(&handle.last_config().expect("regenerated")).expect("JSON");
        assert_eq!(live["inbounds"].as_array().expect("inbounds").len(), 2);
        assert_eq!(live["route"]["rules"].as_array().expect("rules").len(), 2);
        let isolation = controller.isolation_map();
        assert_eq!(isolation.len(), 2);
        assert!(isolation.iter().all(|entry| entry.config_id != "2"));

        assert!(!controller.stop_one("missing"));
        controller.stop_all();
        assert_eq!(controller.running_count(), 0);
        assert!(!handle.is_running());
        assert_eq!(controller.status(), &EngineStatus::Stopped);
    }

    #[test]
    fn check_failure_blocks_start() {
        let (mut controller, handle) = EngineController::fake();
        handle.fail_check("boom: bad field");
        let error = controller
            .start_all(&three_standard())
            .expect_err("check failure blocks start");
        assert!(matches!(error, EngineError::CheckFailed { .. }));
        assert!(!handle.is_running());
        assert!(controller.last_error().is_some());
    }

    #[test]
    fn crash_poll_records_named_cause() {
        let (mut controller, handle) = EngineController::fake();
        controller.start_all(&three_standard()).expect("starts");
        handle.inject_crash("signal: segmentation fault");
        assert_eq!(
            controller.poll().as_deref(),
            Some("stock: signal: segmentation fault")
        );
        assert!(matches!(controller.status(), EngineStatus::Crashed { .. }));
        assert!(controller.status_text().contains("segmentation fault"));
        assert_eq!(
            controller.poll().as_deref(),
            Some("stock: signal: segmentation fault"),
            "cause stays pending until handled"
        );
        controller.clear_pending_crash();
        assert!(controller.poll().is_none(), "cleared after handling");
    }

    #[test]
    fn log_pipe_drains_all_sidecars_and_closes_cleanly_on_exit() {
        let (mut controller, stock, fork, _xray, _extra) = EngineController::fake_quartet();
        controller.start_all(&three_standard()).expect("starts");
        stock.emit_log("warn out-1: dial failed");
        fork.emit_log("info fork sidecar ready");

        let lines = controller.drain_log_lines();
        assert_eq!(lines.len(), 2, "every sidecar drains, got {lines:?}");
        assert!(lines.iter().any(|line| line.contains("out-1")));
        assert!(lines.iter().any(|line| line.contains("fork sidecar")));
        assert!(
            controller.drain_log_lines().is_empty(),
            "drain takes, never replays"
        );

        stock.inject_crash("signal: segmentation fault");
        assert_eq!(
            controller.poll().as_deref(),
            Some("stock: signal: segmentation fault"),
            "exit noted once"
        );
        controller.clear_pending_crash();
        assert!(controller.poll().is_none(), "cleared after handling");
        assert!(
            controller.drain_log_lines().is_empty(),
            "pipe closed cleanly, no panic"
        );
    }

    #[test]
    fn log_pipe_buffer_stays_bounded() {
        let buffer = Arc::new(Mutex::new((0u64, VecDeque::new())));
        for index in 0..(LOG_PIPE_BUFFER + 50) {
            push_pipe_line(&buffer, 0, format!("line {index}"));
        }
        let slot = buffer.lock().expect("lock works");
        assert_eq!(
            slot.1.len(),
            LOG_PIPE_BUFFER,
            "capped, got {}",
            slot.1.len()
        );
        assert!(
            slot.1.front().is_some_and(|line| line == "line 50"),
            "oldest dropped first"
        );
    }

    #[test]
    fn log_pipe_stale_generation_dropped() {
        let buffer = Arc::new(Mutex::new((1u64, VecDeque::new())));
        push_pipe_line(&buffer, 0, "stale line".to_string());
        push_pipe_line(&buffer, 1, "fresh line".to_string());
        let slot = buffer.lock().expect("lock works");
        let lines: Vec<&String> = slot.1.iter().collect();
        assert_eq!(lines, vec!["fresh line"], "stale dropped, got {lines:?}");
    }

    #[test]
    fn log_pipe_poison_recovers_with_notice() {
        let buffer = Arc::new(Mutex::new((0u64, VecDeque::new())));
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = buffer.lock().expect("first lock works");
            panic!("poison the log lock");
        }));
        push_pipe_line(&buffer, 0, "after poison".to_string());
        let slot = buffer.lock().unwrap_or_else(|poison| poison.into_inner());
        let lines: Vec<&String> = slot.1.iter().collect();
        assert!(
            lines.iter().any(|line| line.as_str() == LOG_POISON_NOTICE),
            "notice surfaced, got {lines:?}"
        );
        assert!(
            lines.iter().any(|line| line.as_str() == "after poison"),
            "reader keeps serving, got {lines:?}"
        );
    }

    #[test]
    fn log_pipe_poison_respects_reader_generation() {
        let buffer = Arc::new(Mutex::new((1u64, VecDeque::new())));
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = buffer.lock().expect("first lock works");
            panic!("poison the log lock");
        }));
        push_pipe_line(&buffer, 0, "stale reader line".to_string());
        push_pipe_line(&buffer, 1, "fresh reader line".to_string());
        let slot = buffer.lock().unwrap_or_else(|poison| poison.into_inner());
        let lines: Vec<&String> = slot.1.iter().collect();
        assert!(
            !lines.iter().any(|line| line.contains("stale reader line")),
            "stale reader must not inject through the poison path, got {lines:?}"
        );
        assert!(
            lines.iter().any(|line| line.contains("fresh reader line")),
            "fresh reader still appends, got {lines:?}"
        );
    }

    #[test]
    fn missing_binary_error_lists_searched_dirs() {
        let error = resolve_engine_binary_in(Some(Path::new("/nonexistent-manyproxy-dir")), "")
            .expect_err("no binary anywhere");
        let text = error.to_string();
        assert!(text.contains("sing-box"), "names binary: {text}");
        assert!(
            text.contains("/nonexistent-manyproxy-dir"),
            "names searched dir: {text}"
        );
        assert!(!text.contains("password"), "no secrets: {text}");
    }

    #[test]
    fn bad_params_error_names_field_not_secrets() {
        let configs = vec![fixture(
            "7",
            "Leaky",
            "trojan",
            json!({"server": "10.1.1.7", "password": "HUNTER2"}),
            "127.0.0.1",
            10807,
        )];
        let error = generate(&configs).expect_err("server_port missing");
        let text = error.to_string();
        assert!(text.contains("\"Leaky\""), "names config: {text}");
        assert!(text.contains("server_port"), "names field: {text}");
        assert!(!text.contains("HUNTER2"), "no secrets: {text}");

        let broken = vec![fixture(
            "8",
            "Broken",
            "vless",
            json!({"server": "10.1.1.8", "server_port": 443, "uuid": "x"}),
            "127.0.0.1",
            10808,
        )];
        let mut tampered = broken;
        tampered[0].params = "{not json".to_string();
        let error = generate(&tampered).expect_err("invalid JSON rejected");
        assert!(!error.to_string().contains("not json"));
    }

    #[test]
    fn lan_warning_only_for_non_loopback() {
        assert!(is_loopback_bind("127.0.0.1"));
        assert!(is_loopback_bind("::1"));
        assert!(is_loopback_bind("localhost"));
        assert!(is_loopback_bind("LocalHost"));
        assert!(!is_loopback_bind("0.0.0.0"));
        assert!(!requires_lan_confirm("localhost"));
        assert!(!requires_lan_confirm("127.0.0.1"));
        assert!(requires_lan_confirm("0.0.0.0"));
        assert!(lan_exposure_warning("A", "127.0.0.1").is_none());
        let warning = lan_exposure_warning("B", "0.0.0.0").expect("LAN warns");
        assert!(warning.contains("\"B\""), "names config: {warning}");

        let (mut controller, _) = EngineController::fake();
        let mut rows = three_standard();
        rows[0].bind_ip = "0.0.0.0".to_string();

        let error = controller
            .start_all(&rows)
            .expect_err("unconfirmed LAN bind fails");
        assert!(
            matches!(error, EngineError::LanConfirmRequired { .. }),
            "typed gate, got {error}"
        );
        assert!(error.to_string().contains("\"Alpha\""), "got {error}");
        controller
            .start_all_confirmed(&rows)
            .expect("custom bind starts");
        let warnings = controller.lan_warnings();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("\"Alpha\""));
    }

    #[test]
    fn lan_confirm_gate_blocks_non_loopback_only() {
        assert!(!requires_lan_confirm("127.0.0.1"));
        assert!(!requires_lan_confirm("::1"));
        assert!(!requires_lan_confirm(" 127.0.0.2 "));
        assert!(requires_lan_confirm("0.0.0.0"));
        assert!(requires_lan_confirm("::"));
        assert!(requires_lan_confirm("192.168.1.20"));
        assert!(requires_lan_confirm("not-an-ip"), "garbage fails closed");
        assert!(requires_lan_confirm(""), "blank fails closed");
    }

    #[test]
    fn lan_confirm_text_names_config_without_secrets() {
        let single = lan_confirm_text("Home", "0.0.0.0");
        assert!(single.contains("\"Home\""), "names config: {single}");
        assert!(single.contains("0.0.0.0"), "names bind: {single}");
        for canary in ["hunter2", "LOCAL-PRIVATE", "SECRET-UUID"] {
            assert!(!single.contains(canary), "no secrets: {single}");
        }
        let multi = lan_confirm_description(&[
            ("Beta".to_string(), "192.168.1.20".to_string()),
            ("Alpha".to_string(), "0.0.0.0".to_string()),
        ]);
        assert!(
            multi.contains("\"Alpha\" on 0.0.0.0"),
            "names pair: {multi}"
        );
        assert!(
            multi.contains("\"Beta\" on 192.168.1.20"),
            "names pair: {multi}"
        );
        for canary in ["hunter2", "LOCAL-PRIVATE", "SECRET-UUID"] {
            assert!(!multi.contains(canary), "no secrets: {multi}");
        }
    }

    #[test]
    fn session_fingerprint_roundtrip_proves_no_residue() {
        let (mut controller, _) = EngineController::fake();
        let before = controller.session_fingerprint();
        let rows = three_standard();
        controller.start_all(&rows).expect("session starts");
        let during = controller.session_fingerprint();
        assert_ne!(before, during, "serving differs from idle");
        assert_eq!(during.binds.len(), 3);
        let generated = generate(&rows).expect("standard set generates");
        assert_no_system_mutation(&generated.value().to_string())
            .expect("generated config has no system-mutation keys");
        controller.stop_all();
        let after = controller.session_fingerprint();
        assert_eq!(before, after, "stop-all leaves no residue");
        let debug = format!("{after:?}");
        for canary in ["pass1", "TOP-SECRET", "LOCAL-PRIVATE", "11111111-2222"] {
            assert!(
                !debug.contains(canary),
                "fingerprint leaks no secrets: {debug}"
            );
        }
    }

    #[test]
    fn mutation_scan_rejects_forbidden_keys() {
        for (label, doc) in [
            (
                "top tun",
                r#"{"tun": {}, "inbounds": [], "outbounds": [], "endpoints": []}"#,
            ),
            (
                "top dns",
                r#"{"dns": {}, "inbounds": [], "outbounds": [], "endpoints": []}"#,
            ),
            (
                "nested set_system_proxy",
                r#"{"inbounds": [{"set_system_proxy": true}]}"#,
            ),
            ("deep dns", r#"{"route": {"rules": [{"dns": "x"}]}}"#),
        ] {
            let error = assert_no_system_mutation(doc).unwrap_err();
            assert!(
                error.contains("forbidden key"),
                "{label}: names the key class, got {error}"
            );
        }
        assert_no_system_mutation(r#"{"inbounds": [], "domain_strategy": "x"}"#)
            .expect("lookalike keys pass");
        assert!(assert_no_system_mutation("{not json}").is_err());
    }

    #[test]
    fn missing_binaries_state_never_downloads() {
        let stock = EngineError::MissingBinary {
            binary: engine_binary_file_name(),
            searched: "app-dir + PATH".to_string(),
        }
        .to_string();
        assert!(stock.contains("never downloads"), "contract text: {stock}");
        let fork = EngineError::MissingForkBinary {
            rows: "\"AWG\"".to_string(),
            binary: FORK_BINARY_BASE.to_string(),
            searched: "app-dir + PATH".to_string(),
            xhttp: false,
        }
        .to_string();
        assert!(fork.contains("never downloads"), "fork contract: {fork}");
        let xray = EngineError::MissingXrayBinary {
            rows: "\"VX\"".to_string(),
            binary: XRAY_BINARY_BASE.to_string(),
            searched: "app-dir + PATH".to_string(),
        }
        .to_string();
        assert!(xray.contains("never downloads"), "xray contract: {xray}");
        let extra = EngineError::MissingExtraBinary {
            rows: "\"EC\"".to_string(),
            binary: "extra-core".to_string(),
            searched: "app dir".to_string(),
        }
        .to_string();
        assert!(extra.contains("never downloads"), "extra contract: {extra}");
        assert!(
            extra.contains("never runs it privileged"),
            "no-admin text: {extra}"
        );
    }

    #[test]
    fn binary_resolution_never_elevates_or_downloads() {
        let error = resolve_engine_binary_in(None, "").expect_err("empty search roots fail");
        assert!(
            matches!(error, EngineError::MissingBinary { .. }),
            "typed failure, got {error}"
        );
        let text = error.to_string();
        assert!(
            text.contains(&engine_binary_file_name()),
            "names expected file, got {text}"
        );
        assert!(
            text.contains("no app directory"),
            "names searched dir, got {text}"
        );
    }

    #[test]
    fn a6_executable_bit_behavior_pinned() {
        assert!(!is_executable(Path::new(
            "C:\\definitely-not-here-manyproxy\\nope.exe"
        )));
        assert!(!is_executable(&std::env::temp_dir()));
        let probe =
            std::env::temp_dir().join(format!("manyproxy-a6-exec-{}.tmp", std::process::id()));
        std::fs::write(&probe, "probe").expect("probe writes");
        #[cfg(not(unix))]
        assert!(is_executable(&probe), "plain file is executable on Windows");
        #[cfg(unix)]
        assert!(!is_executable(&probe), "no exec bit on unix probe");
        let _ = std::fs::remove_file(&probe);
    }

    #[test]
    fn a6_fork_probe_never_hangs() {
        let missing = Path::new("C:\\definitely-not-here-manyproxy\\fork.exe");
        let error = probe_fork_version_with_timeout(missing, std::time::Duration::from_secs(5))
            .expect_err("missing binary fails");
        assert!(
            error.contains("fork version probe failed"),
            "named failure, got {error}"
        );
        assert!(
            !error.contains("timed out"),
            "missing is fast-fail, not timeout: {error}"
        );
    }

    #[test]
    fn empty_set_is_no_active_configs() {
        assert!(matches!(generate(&[]), Err(EngineError::NoActiveConfigs)));
    }

    #[test]
    fn t32_route_rules_use_inbound_array() {
        let rows = three_standard();
        let generated = generate(&rows).expect("standard set generates");
        let value = generated.value();
        let rules = value["route"]["rules"].as_array().expect("rules array");
        assert_eq!(rules.len(), 3);
        for (row, rule) in rows.iter().zip(rules.iter()) {
            assert_eq!(rule["inbound"], json!([inbound_tag(&row.id)]), "array form");
            assert_eq!(rule["outbound"], json!(outbound_tag(&row.id)));
            assert_eq!(rule["action"], json!("route"));
        }
    }

    #[test]
    fn t32_custom_full_config_never_conflicts() {
        fn custom_full(id: &str, name: &str, core: &str) -> ProxyConfig {
            fixture(
                id,
                name,
                "custom-config",
                json!({"core": core,
                       "config": {"outbounds": [{"type": "direct", "tag": "direct"}]}}),
                "127.0.0.1",
                10801,
            )
        }

        let rows = vec![
            custom_full("c1", "CustomA", "stock"),
            custom_full("c2", "CustomB", "fork"),
        ];
        validate_activation(&rows).expect("customs never conflict in validate");

        let error = generate(&rows).expect_err("customs join no bare config");
        assert!(
            !matches!(error, EngineError::PortConflict { .. }),
            "generate agrees (no conflict), got {error}"
        );

        let mixed = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            ),
            custom_full("c1", "CustomA", "stock"),
        ];
        let error = generate(&mixed).expect_err("custom joins no bare config");
        assert!(
            !matches!(error, EngineError::PortConflict { .. }),
            "mixed set agrees (no conflict), got {error}"
        );
    }

    #[test]
    fn t32_stop_with_failing_regen_restores_snapshot() {
        let rows = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            ),
            fixture("2", "Beta", "vless", vless_params(), "127.0.0.1", 10802),
        ];
        let (mut controller, stock, _) = EngineController::fake_pair();
        controller.start_all(&rows).expect("both start");
        assert!(stock.is_running());
        stock.fail_check("t32 boom");
        assert!(controller.stop_one("1"), "row was active");
        assert_eq!(
            controller.running_ids(),
            vec!["1".to_string(), "2".to_string()],
            "membership snapshot restored"
        );
        assert!(stock.is_running(), "sidecar still serves the snapshot");
        assert!(controller.is_running("1"), "row still served");
        let error = controller.last_error().unwrap_or("").to_string();
        assert!(error.contains("t32 boom"), "error recorded, got {error}");
        stock.clear_failures();
        assert!(controller.stop_one("1"), "stop succeeds once healthy");
        assert_eq!(controller.running_ids(), vec!["2".to_string()]);
    }

    #[test]
    fn t32_awg_string_numerics_coerce_ranges_verbatim() {
        let params = awg_params(
            "2.0",
            json!({"jc": "5", "jmin": "30", "jmax": "1000",
            "s1": "20", "s2": "21", "s3": "22", "s4": "23",
            "h1": "7", "h2": "8", "h3": "9", "h4": "1-100",
            "i1": "<b 0xd100><r 50>", "i2": "5"}),
        );
        let configs = vec![awg_fixture("7", "AwgNum", params, 10807)];
        let generated = generate(&configs).expect("AWG numerics generate");
        let endpoint = generated.value()["endpoints"][0].clone();
        for (key, number) in [
            ("jc", 5),
            ("jmin", 30),
            ("jmax", 1000),
            ("s1", 20),
            ("s2", 21),
            ("s3", 22),
            ("s4", 23),
            ("h1", 7),
            ("h2", 8),
            ("h3", 9),
        ] {
            assert_eq!(endpoint[key], json!(number), "{key} coerced");
        }
        assert_eq!(endpoint["h4"], json!("1-100"), "h-range verbatim");
        assert_eq!(
            endpoint["i1"],
            json!("<b 0xd100><r 50>"),
            "i-chain verbatim"
        );
        assert_eq!(endpoint["i2"], json!("5"), "i-chain never coerced");
    }

    #[test]
    fn t04_awg_markers_strip_and_matrix_passes_through() {
        let params = awg_params(
            "3.x",
            json!({"private_key": "LOCAL", "public_key": "REMOTE",
            "pre_shared_key": "PSK", "allowed_ips": "10.0.0.0/8",
            "persistent_keepalive": "25", "reserved": "1,2,3",
            "address": "10.0.0.2/32", "dns": "198.18.0.1", "mtu": "1280",
            "workers": "4", "system": true, "udp_timeout": "30",
            "enable_amnezia": true,
            "jc": "5", "jmin": "30", "jmax": "1000",
            "s1": "20", "h1": "1-100", "i1": "<b 0xd100><r 50>",
            "headerprotectionkey": "HPK", "contentpaddingaddition": "PAD",
            "rekeyaftertime": "120", "rekeytimeout": "30",
            "rejectaftertime": "180", "keepalivetimeout": "60",
            "maxhandshakeattempts": "3",
            "randomtrailers": true, "disablecookies": true}),
        );
        let configs = vec![awg_fixture("7", "AwgFull", params, 10807)];
        let generated = generate(&configs).expect("AWG v3 generates");
        let endpoint = generated.value()["endpoints"][0].clone();
        assert!(
            endpoint.get("enable_amnezia").is_none(),
            "UI marker stripped"
        );
        assert!(endpoint.get("awg_version").is_none(), "no internal marker");
        assert!(endpoint.get("dns").is_none(), "no client setting");
        assert_eq!(endpoint["private_key"], json!("LOCAL"));
        assert_eq!(endpoint["address"], json!(["10.0.0.2/32"]));
        assert_eq!(endpoint["mtu"], json!(1280), "mtu coerced");
        assert_eq!(endpoint["workers"], json!(4), "workers coerced");
        assert_eq!(endpoint["system"], json!(true), "system passes through");
        assert_eq!(
            endpoint["udp_timeout"],
            json!("30"),
            "import-only preserved, never stripped"
        );
        assert_eq!(endpoint["reserved"], json!([1, 2, 3]), "reserved coerced");
        assert_eq!(endpoint["jc"], json!(5));
        assert_eq!(endpoint["h1"], json!("1-100"), "h-range verbatim");
        assert_eq!(endpoint["headerprotectionkey"], json!("HPK"), "v3 verbatim");
        assert_eq!(
            endpoint["maxhandshakeattempts"],
            json!(3),
            "v3 numeric coerced"
        );
        assert_eq!(
            endpoint["randomtrailers"],
            json!(true),
            "trailers pass through for fork check"
        );
        assert_eq!(
            endpoint["disablecookies"],
            json!(true),
            "cookies pass through for fork check"
        );
        let peer = endpoint["peers"]
            .as_array()
            .expect("peers array")
            .first()
            .expect("one peer");
        assert_eq!(peer["public_key"], json!("REMOTE"));
        assert_eq!(peer["pre_shared_key"], json!("PSK"));
        assert_eq!(peer["allowed_ips"], json!(["10.0.0.0/8"]));
        assert_eq!(peer["persistent_keepalive_interval"], json!(25));

        let bad = awg_params("2.0", json!({"randomtrailers": true}));
        let error =
            generate(&[awg_fixture("8", "AwgOld", bad, 10808)]).expect_err("v3 key on 2.0 fails");
        let text = error.to_string();
        assert!(text.contains("randomtrailers"), "names the field: {text}");
    }

    #[test]
    fn t32_live_singbox_check_accepts_standard_rows() {
        if std::env::var("MANYPROXY_LIVE_CHECK").unwrap_or_default() != "1" {
            return;
        }
        use crate::import::parse_link;
        let binary = std::env::var("MANYPROXY_SINGBOX_BIN").map_or_else(
            |_| {
                if cfg!(windows) {
                    PathBuf::from("target/debug/sing-box.exe")
                } else {
                    PathBuf::from("target/debug/sing-box")
                }
            },
            PathBuf::from,
        );
        let uuid = "11111111-2222-4333-8444-555555555555";
        let vmess_json = serde_json::json!({
            "v": "2", "ps": "vmess-probe", "add": "192.0.2.11", "port": "443",
            "id": uuid, "aid": "0", "scy": "auto", "net": "ws",
            "type": "none", "host": "example.com", "path": "/ws", "tls": "tls", "sni": "example.com",
        })
        .to_string();
        use base64::Engine as _;
        let vmess_link = format!(
            "vmess://{}#vmess-probe",
            base64::engine::general_purpose::STANDARD.encode(vmess_json)
        );
        let links = [
            ("ss", "ss://YWVzLTEyOC1nY206cGFzczE@192.0.2.7:8388#ss-one"),
            ("vless-plain", &format!("vless://{uuid}@192.0.2.9:443?encryption=none#vless-plain")),
            ("vless-tls-ws", &format!("vless://{uuid}@192.0.2.9:443?encryption=none&security=tls&sni=example.com&fp=chrome&type=ws&path=%2Fws&host=example.com#vless-tls")),
            ("vless-grpc", &format!("vless://{uuid}@192.0.2.9:443?encryption=none&security=tls&sni=example.com&type=grpc&serviceName=exampleSvc#vless-grpc")),
            ("vless-http", &format!("vless://{uuid}@192.0.2.9:443?encryption=none&security=tls&sni=example.com&type=http&host=example.com&path=%2Fh2path#vless-http")),
            ("vless-reality", &format!("vless://{uuid}@192.0.2.9:443?encryption=none&security=reality&pbk=A3oRvoNAnsCtWItpFcR4nQ0VyJjEJhRq8yx6shbXSVo&sid=abcd&sni=example.com&fp=chrome&flow=xtls-rprx-vision#vless-reality")),
            ("vmess", &vmess_link),
            ("trojan", "trojan://cGFzczE@192.0.2.10:443?sni=example.com#trojan-one"),
            ("tuic", &format!("tuic://{uuid}:cGFzczE@192.0.2.12:443?congestion_control=bbr&alpn=h3#tuic-one")),
            ("hysteria2", "hysteria2://cGFzczE@192.0.2.13:443?sni=example.com#hy2-one"),
            ("hysteria", "hysteria://cGFzczE@192.0.2.14:443?peer=example.com&alpn=h3&up=100%20Mbps&down=100%20Mbps#hy-one"),
            ("anytls", "anytls://c2VjcmV0@192.0.2.15:8443?sni=example.com#anytls-one"),
            ("ssh", "ssh://testuser:testpass@192.0.2.16:22#ssh-one"),
            ("wg-uri", "wg://+OXqmRTv+Pv771vT7BlefIxQkBgLNEbmb+tqNcSR5FA=@192.0.2.44:51820?publickey=ovSNn7m3TBtLeRWIaGYAhpqTn3o1sF75Ot7LV0qULD0=#wg-uri"),
            ("wg-full", "wgfull"),
            ("wg-minimal", "wgminimal"),
        ];
        for (label, link) in links {
            if link == "wgfull" {
                let row = fixture(
                    "9",
                    "WgFull",
                    "wireguard",
                    serde_json::json!({"type": "wireguard", "server": "192.0.2.44",
                        "server_port": 51820,
                        "private_key": "+OXqmRTv+Pv771vT7BlefIxQkBgLNEbmb+tqNcSR5FA=",
                        "public_key": "ovSNn7m3TBtLeRWIaGYAhpqTn3o1sF75Ot7LV0qULD0=",
                        "address": "100.64.0.2/32", "mtu": "1280",
                        "persistent_keepalive": "25", "allowed_ips": "0.0.0.0/0"}),
                    "127.0.0.1",
                    10901,
                );
                check_one(&binary, label, &[row]);
                continue;
            }
            if link == "wgminimal" {
                let row = fixture(
                    "9",
                    "WgMinimal",
                    "wireguard",
                    serde_json::json!({"type": "wireguard", "server": "192.0.2.44",
                        "server_port": 51820,
                        "private_key": "+OXqmRTv+Pv771vT7BlefIxQkBgLNEbmb+tqNcSR5FA=",
                        "public_key": "ovSNn7m3TBtLeRWIaGYAhpqTn3o1sF75Ot7LV0qULD0="}),
                    "127.0.0.1",
                    10901,
                );
                check_one(&binary, label, &[row]);
                continue;
            }
            match parse_link(link) {
                Ok(imported) => {
                    let row = ProxyConfig {
                        id: format!("live-{label}"),
                        name: imported.name.clone(),
                        protocol: imported.protocol.clone(),
                        params: imported.params.clone(),
                        group: "live".to_string(),
                        enabled: true,
                        bind_ip: "127.0.0.1".to_string(),
                        port: 10901,
                        subscription_id: None,
                        identity_key: None,
                        preset_id: None,
                    };
                    check_one(&binary, label, std::slice::from_ref(&row));
                }
                Err(error) => panic!("PROBE {label}: link parses: {error}"),
            }
        }
        fn check_one(binary: &Path, label: &str, rows: &[ProxyConfig]) {
            let generated =
                generate(rows).unwrap_or_else(|error| panic!("PROBE {label}: generates: {error}"));
            let path = std::env::temp_dir().join(format!(
                "manyproxy-t32-live-{label}-{}-{}.json",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|elapsed| elapsed.as_nanos())
                    .unwrap_or(0)
            ));
            std::fs::write(&path, &generated.json).expect("temp config writes");
            let output = std::process::Command::new(binary)
                .arg("check")
                .arg("-c")
                .arg(&path)
                .output()
                .expect("check runs");
            let _ = std::fs::remove_file(&path);
            assert!(
                output.status.success(),
                "PROBE {label}: sing-box check EXIT 0, stderr: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    #[test]
    fn t32_translation_errors_name_field() {
        let plugin = fixture(
            "1",
            "SsPlugin",
            "shadowsocks",
            json!({"type": "shadowsocks", "server": "192.0.2.7", "server_port": 8388,
                   "method": "aes-128-gcm", "password": "pass1", "plugin": "obfs-local"}),
            "127.0.0.1",
            10801,
        );
        let error = generate(std::slice::from_ref(&plugin)).expect_err("plugin names its field");
        assert!(
            matches!(error, EngineError::BadCustom { .. }),
            "typed, got {error}"
        );
        assert!(
            error.to_string().contains("\"plugin\""),
            "names field: {error}"
        );
        assert!(
            !error.to_string().contains("obfs-local"),
            "no value echo: {error}"
        );

        let slow = fixture(
            "2",
            "HyBare",
            "hysteria",
            json!({"type": "hysteria", "server": "192.0.2.14", "server_port": 443, "auth": "pass1"}),
            "127.0.0.1",
            10802,
        );
        let error = generate(std::slice::from_ref(&slow)).expect_err("speeds required");
        assert!(
            matches!(error, EngineError::BadParams { field: "up", .. }),
            "typed, got {error}"
        );

        let realm = fixture(
            "3",
            "Hy2Realm",
            "hysteria2",
            json!({"type": "hysteria2", "server": "192.0.2.13", "server_port": 443,
                   "password": "pass1", "variant": "realm"}),
            "127.0.0.1",
            10803,
        );
        let error = generate(std::slice::from_ref(&realm)).expect_err("realm names its field");
        assert!(
            error.to_string().contains("\"variant\""),
            "names field: {error}"
        );

        let kcp = fixture(
            "4",
            "VlessKcp",
            "vless",
            json!({"type": "vless", "server": "192.0.2.9", "server_port": 443,
                   "uuid": "11111111-2222-4333-8444-555555555555", "network": "kcp"}),
            "127.0.0.1",
            10804,
        );
        let error = generate(std::slice::from_ref(&kcp)).expect_err("kcp names its field");
        assert!(
            error.to_string().contains("\"network\""),
            "names field: {error}"
        );

        let plain = fixture(
            "5",
            "TuicPlain",
            "tuic",
            json!({"type": "tuic", "server": "192.0.2.12", "server_port": 443,
                   "uuid": "11111111-2222-4333-8444-555555555555", "password": "pass1",
                   "security": "none"}),
            "127.0.0.1",
            10805,
        );
        let error = generate(std::slice::from_ref(&plain)).expect_err("tls mandatory");
        assert!(
            error.to_string().contains("\"security\""),
            "names field: {error}"
        );
    }

    #[test]
    fn t32_intake_spellings_fold_to_singbox_names() {
        let ssh = fixture(
            "1",
            "SshOne",
            "ssh",
            json!({"type": "ssh", "server": "192.0.2.16", "server_port": 22,
                   "username": "testuser", "password": "testpass"}),
            "127.0.0.1",
            10801,
        );
        let node = generate(std::slice::from_ref(&ssh))
            .expect("ssh generates")
            .value()["outbounds"]
            .as_array()
            .expect("outbounds")
            .iter()
            .find(|node| node["tag"] == json!(outbound_tag("1")))
            .expect("ssh outbound")
            .clone();
        assert_eq!(node["user"], json!("testuser"));
        assert!(node.get("username").is_none(), "folded, not duplicated");
        let vmess = fixture(
            "2",
            "VmessOne",
            "vmess",
            json!({"type": "vmess", "server": "192.0.2.11", "server_port": 443,
                   "uuid": "11111111-2222-4333-8444-555555555555", "alter_id": "3"}),
            "127.0.0.1",
            10802,
        );
        let value = generate(std::slice::from_ref(&vmess))
            .expect("vmess generates")
            .value();
        let node = value["outbounds"]
            .as_array()
            .expect("outbounds")
            .iter()
            .find(|node| node["tag"] == json!(outbound_tag("2")))
            .expect("vmess outbound");
        assert_eq!(node["alter_id"], json!(3), "coerced to number");
    }

    fn awg_params(version: &str, extra: serde_json::Value) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        map.insert("type".to_string(), json!("amneziawg"));
        map.insert("server".to_string(), json!("192.0.2.90"));
        map.insert("server_port".to_string(), json!(51820));
        map.insert("private_key".to_string(), json!("LOCAL-PRIVATE"));
        map.insert("public_key".to_string(), json!("REMOTE-PUBLIC"));
        map.insert("awg_version".to_string(), json!(version));
        if let serde_json::Value::Object(extra) = extra {
            for (key, value) in extra {
                map.insert(key, value);
            }
        }
        serde_json::Value::Object(map)
    }

    fn awg_fixture(id: &str, name: &str, params: serde_json::Value, port: u16) -> ProxyConfig {
        fixture(id, name, "amneziawg", params, "127.0.0.1", port)
    }

    fn xhttp_vless_params() -> serde_json::Value {
        json!({"type": "vless", "server": "198.51.100.9", "server_port": 443,
               "uuid": "11111111-2222-4333-8444-555555555555", "encryption": "none",
               "security": "tls", "sni": "198.51.100.9", "network": "xhttp",
               "path": "/split", "mode": "stream-one"})
    }

    #[test]
    fn awg_20_generates_fork_wireguard_endpoint() {
        let params = awg_params(
            "2.0",
            json!({"jc": 4, "jmin": 40, "jmax": 70,
            "s1": 20, "h1": "1-100", "i1": "<b 0xd100><r 50>"}),
        );
        let configs = vec![awg_fixture("7", "AwgOne", params, 10807)];
        assert!(needs_fork(&configs[0]));
        let generated = generate(&configs).expect("AWG 2.0 generates");
        let value = generated.value();
        let endpoints = value["endpoints"].as_array().expect("endpoints array");
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0]["type"], json!("wireguard"));
        assert_eq!(endpoints[0]["tag"], json!(outbound_tag("7")));
        assert_eq!(endpoints[0]["jc"], json!(4));
        assert_eq!(endpoints[0]["jmin"], json!(40));

        assert!(
            endpoints[0].get("awg_version").is_none(),
            "no internal marker"
        );
        assert_eq!(
            value["route"]["rules"]
                .as_array()
                .expect("rules array")
                .len(),
            1
        );
    }

    #[test]
    fn awg_31_extended_params_accepted() {
        let params = awg_params(
            "3.1",
            json!({"jc": 3, "s1": 12,
            "HeaderProtectionKey": "NOT-A-REAL-KEY",
            "ContentPaddingAddition": "1-64",
            "RekeyAfterTime": "120-180",
            "RandomTrailers": "on", "DisableCookies": "off"}),
        );
        let configs = vec![awg_fixture("7", "Awg31", params, 10807)];
        let generated = generate(&configs).expect("AWG 3.1 generates");
        assert!(needs_fork(&configs[0]));
        let value = generated.value();
        assert_eq!(value["endpoints"][0]["RandomTrailers"], json!("on"));
    }

    #[test]
    fn awg_31_field_on_20_row_rejected_with_named_error() {
        let params = awg_params("2.0", json!({"HeaderProtectionKey": "K"}));
        let configs = vec![awg_fixture("7", "Skewed", params, 10807)];
        let error = generate(&configs).expect_err("3.x field on 2.0 fails");
        let text = error.to_string();
        assert!(
            matches!(error, EngineError::BadAwg { .. }),
            "typed matrix error, got {text}"
        );
        assert!(text.contains("HeaderProtectionKey"), "names field: {text}");
        assert!(text.contains("\"2.0\""), "names have-version: {text}");
        assert!(text.contains("\"Skewed\""), "names config: {text}");
    }

    #[test]
    fn awg_unknown_field_rejected() {
        let params = awg_params("2.0", json!({"frobnicate": 1}));
        let configs = vec![awg_fixture("7", "Weird", params, 10807)];
        let error = generate(&configs).expect_err("unknown field fails");
        let text = error.to_string();
        assert!(matches!(error, EngineError::BadAwg { .. }));
        assert!(text.contains("frobnicate"), "names field: {text}");
    }

    #[test]
    fn awg_version_presence_and_range() {
        let mut bare = awg_params("2.0", json!({}));
        bare.as_object_mut().expect("object").remove("awg_version");
        let configs = vec![awg_fixture("7", "NoVer", bare, 10807)];
        let error = generate(&configs).expect_err("missing version fails");
        assert!(error.to_string().contains("awg_version"), "got {error}");

        let params = awg_params("4.0", json!({}));
        let configs = vec![awg_fixture("7", "NewVer", params, 10807)];
        let error = generate(&configs).expect_err("unknown version fails");
        let text = error.to_string();
        assert!(text.contains("4.0"), "names version: {text}");
        assert!(text.contains("awg_version"), "names param: {text}");
    }

    #[test]
    fn awg_missing_credentials_name_field_not_secrets() {
        let mut params = awg_params("2.0", json!({}));
        params.as_object_mut().expect("object").remove("public_key");
        let configs = vec![awg_fixture("7", "Keyless", params, 10807)];
        let error = generate(&configs).expect_err("missing public_key fails");
        let text = error.to_string();
        assert!(text.contains("public_key"), "names field: {text}");
        assert!(!text.contains("LOCAL-PRIVATE"), "no secrets: {text}");
    }

    #[test]
    fn awg_fields_on_stock_rows_are_wrong_layer() {
        let mut wg = wg_params();
        wg.as_object_mut()
            .expect("object")
            .insert("jc".to_string(), json!(4));
        let configs = vec![fixture("3", "PlainWg", "wireguard", wg, "127.0.0.1", 10803)];
        let error = generate(&configs).expect_err("jc on wireguard fails");
        let text = error.to_string();
        assert!(matches!(error, EngineError::WrongLayer { .. }));
        assert!(text.contains("\"jc\""), "names field: {text}");
        assert!(text.contains("\"PlainWg\""), "names config: {text}");

        let mut vless = vless_params();
        vless
            .as_object_mut()
            .expect("object")
            .insert("awg_version".to_string(), json!("2.0"));
        let configs = vec![fixture("2", "Beta", "vless", vless, "127.0.0.1", 10802)];
        let error = generate(&configs).expect_err("awg_version on vless fails");
        assert!(matches!(error, EngineError::WrongLayer { .. }));
        assert!(
            !error.to_string().contains("11111111-2222"),
            "no credential echo: {error}"
        );
    }

    #[test]
    fn xray_noise_on_awg_rows_is_wrong_layer() {
        for key in ["noise", "finalmask", "final_mask"] {
            let mut map = awg_params("2.0", json!({}));
            map.as_object_mut()
                .expect("object")
                .insert(key.to_string(), json!("some-decoy"));
            let configs = vec![awg_fixture("7", "Mixed", map, 10807)];
            let error = generate(&configs).expect_err(&format!("{key} on AWG fails"));
            let text = error.to_string();
            assert!(
                matches!(error, EngineError::WrongLayer { .. }),
                "typed guard, got {text}"
            );
            assert!(text.contains(key), "names field: {text}");
            assert!(!text.contains("some-decoy"), "no values: {text}");
        }
    }

    #[test]
    fn xhttp_vless_needs_fork_wireguard_rejects() {
        let configs = vec![fixture(
            "2",
            "Xhttp",
            "vless",
            xhttp_vless_params(),
            "127.0.0.1",
            10802,
        )];
        assert!(needs_fork(&configs[0]));
        let generated = generate(&configs).expect("xhttp vless generates");
        let value = generated.value();
        let outbounds = value["outbounds"].as_array().expect("outbounds");

        let node = outbounds
            .iter()
            .find(|node| node["tag"] == json!(outbound_tag("2")))
            .expect("xhttp outbound present");
        assert_eq!(node["transport"]["type"], json!("xhttp"));
        assert!(node.get("network").is_none(), "flat consumed");

        let mut wg = wg_params();
        wg.as_object_mut()
            .expect("object")
            .insert("network".to_string(), json!("xhttp"));
        let configs = vec![fixture("3", "WgX", "wireguard", wg, "127.0.0.1", 10803)];
        let error = generate(&configs).expect_err("xhttp on wireguard fails");
        assert!(matches!(error, EngineError::WrongLayer { .. }));
        assert!(!needs_fork(&configs[0]));
    }

    #[test]
    fn stock_sets_need_no_fork() {
        for config in three_standard() {
            assert!(!needs_fork(&config), "stock needs no fork: {}", config.name);
        }
    }

    #[test]
    fn fork_resolution_names_expected_binary() {
        assert!(fork_binary_file_name().contains("sing-box-awg"));
        let searched = resolve_fork_binary_in(Some(Path::new("/nonexistent-manyproxy-dir")), "")
            .expect_err("no fork binary anywhere");
        assert!(
            searched.contains("/nonexistent-manyproxy-dir"),
            "names searched dir: {searched}"
        );
        let error = EngineError::MissingForkBinary {
            rows: "\"AwgOne\"".to_string(),
            binary: fork_binary_file_name(),
            searched,
            xhttp: true,
        };
        let text = error.to_string();
        assert!(text.contains("sing-box-awg"), "names binary: {text}");
        assert!(text.contains("\"AwgOne\""), "names row: {text}");
        assert!(text.contains("stream-one"), "bug note: {text}");
        assert!(
            text.contains("stock configs keep working"),
            "reassures: {text}"
        );
    }

    #[test]
    fn fork_version_probe_fails_on_missing_binary() {
        let error = probe_fork_version(Path::new("/nonexistent-manyproxy-dir/sing-box-awg"))
            .expect_err("probe on missing binary fails");
        assert!(error.contains("fork version probe failed"), "got {error}");
    }

    #[test]
    fn mixed_set_with_fork_runs_both_sidecars() {
        let (mut controller, stock, fork) = EngineController::fake_pair();
        controller.set_fork_probe(|| Ok(PathBuf::from("test-fork-bin")));
        let rows = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            ),
            awg_fixture("7", "AwgOne", awg_params("2.0", json!({"jc": 2})), 10807),
        ];
        controller.start_all(&rows).expect("mixed set starts");
        assert_eq!(controller.running_count(), 2);
        assert!(stock.is_running());
        assert!(fork.is_running());
        assert!(controller.is_running("1"));
        assert!(controller.is_running("7"));
        let stock_live: serde_json::Value =
            serde_json::from_str(&stock.last_config().expect("stock config")).expect("JSON");
        assert_eq!(stock_live["inbounds"].as_array().expect("in").len(), 1);
        let fork_live: serde_json::Value =
            serde_json::from_str(&fork.last_config().expect("fork config")).expect("JSON");
        assert_eq!(fork_live["inbounds"].as_array().expect("in").len(), 1);
        assert_eq!(fork_live["endpoints"].as_array().expect("ep").len(), 1);
        assert_eq!(controller.isolation_map().len(), 2);
    }

    #[test]
    fn missing_fork_fails_only_exotic_rows() {
        let (mut controller, stock, fork) = EngineController::fake_pair();

        let rows = vec![fixture(
            "1",
            "Alpha",
            "shadowsocks",
            ss_params("pass1"),
            "127.0.0.1",
            10801,
        )];
        controller
            .start_all(&rows)
            .expect("stock works without fork");
        assert!(stock.is_running());

        let exotic = awg_fixture("7", "AwgOne", awg_params("2.0", json!({"jc": 2})), 10807);
        let error = controller
            .start_one(&exotic)
            .expect_err("exotic without fork fails");
        let text = error.to_string();
        assert!(
            matches!(error, EngineError::MissingForkBinary { .. }),
            "typed error, got {text}"
        );
        assert!(text.contains("\"AwgOne\""), "names row: {text}");
        assert!(text.contains("sing-box-awg"), "names binary: {text}");
        assert!(
            text.contains("stock configs keep working"),
            "reassures: {text}"
        );
        assert!(stock.is_running(), "stock sidecar untouched");
        assert!(!fork.is_running(), "fork sidecar never started");
    }

    #[test]
    fn xhttp_missing_fork_error_carries_bug_note() {
        let (mut controller, _, _) = EngineController::fake_pair();
        let exotic = fixture(
            "2",
            "Xhttp",
            "vless",
            xhttp_vless_params(),
            "127.0.0.1",
            10802,
        );
        let error = controller
            .start_one(&exotic)
            .expect_err("xhttp without fork fails");
        let text = error.to_string();
        assert!(matches!(error, EngineError::MissingForkBinary { .. }));
        assert!(text.contains("stream-one"), "bug note: {text}");
        assert!(text.contains("sing-box-awg"), "binary: {text}");
    }

    const T08_UUID: &str = "11111111-2222-4333-8444-555555555555";

    fn xray_params() -> serde_json::Value {
        json!({"server": "198.51.100.9", "server_port": 443,
               "uuid": T08_UUID, "encryption": "none",
               "flow": "xtls-rprx-vision", "network": "tcp",
               "security": "tls", "sni": "198.51.100.9"})
    }

    fn xray_fixture(id: &str, name: &str, port: u16) -> ProxyConfig {
        fixture(id, name, "xray", xray_params(), "127.0.0.1", port)
    }

    fn custom_stock_params() -> serde_json::Value {
        json!({"type": "vless", "server": "203.0.113.9", "server_port": 443,
               "uuid": T08_UUID, "fancy": "kept"})
    }

    fn custom_fixture(id: &str, name: &str, params: serde_json::Value, port: u16) -> ProxyConfig {
        fixture(id, name, "custom-outbound", params, "127.0.0.1", port)
    }

    fn extra_fixture(id: &str, name: &str, port: u16) -> ProxyConfig {
        fixture(
            id,
            name,
            "extra-core",
            json!({"exe": "user-core.exe", "config": {"listen": "127.0.0.1"}}),
            "127.0.0.1",
            port,
        )
    }

    fn chain_fixture(id: &str, name: &str, nodes: serde_json::Value, port: u16) -> ProxyConfig {
        fixture(
            id,
            name,
            "chain",
            json!({"nodes": nodes}),
            "127.0.0.1",
            port,
        )
    }

    #[test]
    fn xray_row_bridges_loopback_pointer_to_sidecar() {
        let rows = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            ),
            xray_fixture("2", "XrayOne", 10802),
        ];
        assert!(needs_xray(&rows[1]));
        assert!(!needs_xray(&rows[0]));
        let plan = validate_activation(&rows).expect("mixed set plans");
        assert_eq!(plan.egresses.len(), 1);
        let leg = &plan.egresses[0];
        assert!(
            (XRAY_BRIDGE_PORT_START..=XRAY_BRIDGE_PORT_END).contains(&leg.bridge_port),
            "bridge port in range, got {}",
            leg.bridge_port
        );
        assert_eq!(leg.main_tag, outbound_tag("2"));
        assert_eq!(leg.ingress_tag, xray_ingress_tag("2"));
        assert_eq!(leg.egress_tag, xray_egress_tag("2"));

        let main = generate(&rows).expect("main config generates");
        let value = main.value();
        let pointer = value["outbounds"]
            .as_array()
            .expect("outbounds")
            .iter()
            .find(|node| node["tag"] == json!(outbound_tag("2")))
            .expect("bridge pointer present");
        assert_eq!(pointer["server"], json!("127.0.0.1"), "loopback bridge");
        assert_eq!(
            pointer["server_port"],
            json!(leg.bridge_port),
            "pointer agrees with bookkeeping"
        );

        let sidecar = generate_xray_config(&plan.egresses).expect("sidecar generates");
        let xray = sidecar.value();
        assert_eq!(xray["inbounds"][0]["tag"], json!(xray_ingress_tag("2")));
        assert!(
            xray["outbounds"]
                .as_array()
                .expect("xray outbounds")
                .iter()
                .any(|node| node["tag"] == json!(xray_egress_tag("2"))),
            "egress tag present"
        );
        let ingress = &xray["inbounds"][0];
        assert_eq!(ingress["listen"], json!("127.0.0.1"), "loopback hygiene");
        assert_eq!(ingress["port"], json!(leg.bridge_port));
        let egress = xray["outbounds"]
            .as_array()
            .expect("xray outbounds")
            .iter()
            .find(|node| node["tag"] == json!(xray_egress_tag("2")))
            .expect("xray egress present");
        assert_eq!(egress["protocol"], json!("vless"));
        assert_eq!(
            egress["settings"]["vnext"][0]["address"],
            json!("198.51.100.9")
        );
        assert_eq!(
            egress["settings"]["vnext"][0]["users"][0]["id"],
            json!(T08_UUID)
        );
        let rule = &xray["routing"]["rules"][0];
        assert_eq!(rule["inboundTag"], json!([xray_ingress_tag("2")]));
        assert_eq!(rule["outboundTag"], json!(xray_egress_tag("2")));

        validate_bridges_present(&main.json, &plan.egresses).expect("pointers present");
        assert_eq!(value["route"]["rules"].as_array().expect("rules").len(), 2);
        for row in &rows {
            let rule = value["route"]["rules"]
                .as_array()
                .expect("rules")
                .iter()
                .find(|rule| rule["inbound"] == json!([inbound_tag(&row.id)]))
                .expect("rule per endpoint");
            assert_eq!(rule["outbound"], json!(outbound_tag(&row.id)));
        }
    }

    #[test]
    fn bug13_xray_string_server_port_generates_port() {
        let row = fixture(
            "1",
            "XrayStr",
            "xray",
            json!({"server": "198.51.100.9", "server_port": "443", "uuid": T08_UUID}),
            "127.0.0.1",
            10802,
        );
        let plan = validate_activation(std::slice::from_ref(&row)).expect("string port validates");
        let sidecar = generate_xray_config(&plan.egresses).expect("sidecar generates");
        let xray = sidecar.value();
        let egress = xray["outbounds"]
            .as_array()
            .expect("outbounds array")
            .iter()
            .find(|node| node["tag"] == json!(xray_egress_tag("1")))
            .expect("egress present");
        assert_eq!(
            egress["settings"]["vnext"][0]["port"],
            json!(443),
            "string server_port normalized to a number: {egress}"
        );
    }

    #[test]
    fn bug13_xray_garbage_server_port_fails_validation() {
        let row = fixture(
            "1",
            "XrayBad",
            "xray",
            json!({"server": "198.51.100.9", "server_port": "not-a-port", "uuid": T08_UUID}),
            "127.0.0.1",
            10802,
        );
        let error = validate_activation(std::slice::from_ref(&row))
            .expect_err("garbage string port must fail before generation");
        let text = error.to_string();
        assert!(
            matches!(error, EngineError::BadCustom { .. }),
            "typed, got {text}"
        );
        assert!(text.contains("server_port"), "names the field: {text}");
        assert!(text.contains("XrayBad"), "names the row: {text}");
    }

    #[test]
    fn bridge_count_mismatch_fails_start_with_signature() {
        let error = validate_bridge_counts(1, 0, 1, "test").expect_err("skew fails");
        assert!(matches!(error, EngineError::XrayBridgeMismatch { .. }));
        assert!(
            error.to_string().contains(XRAY_BRIDGE_COUNT_SIGNATURE),
            "signature, got {error}"
        );

        let ghost = XrayEgress {
            owner: "Ghost".to_string(),
            params: serde_json::Map::new(),
            xray_shaped: false,
            bridge_port: 19001,
            bridge_key: "ghost".to_string(),
            main_tag: "ghost-main".to_string(),
            ingress_tag: "ghost-in".to_string(),
            egress_tag: "ghost-out".to_string(),
        };
        let pointerless = serde_json::json!({"outbounds": []}).to_string();
        let error = validate_bridges_present(&pointerless, std::slice::from_ref(&ghost))
            .expect_err("a missing bridge pointer fails");
        assert!(matches!(error, EngineError::XrayBridgeMismatch { .. }));
        assert!(
            error.to_string().contains(XRAY_BRIDGE_COUNT_SIGNATURE),
            "signature, got {error}"
        );

        let replacement = fixture(
            "7",
            "CustomStock",
            "custom-config",
            json!({"core": "stock",
                   "config": {"outbounds": [{"type": "direct", "tag": "direct"}],
                              "note": "mine"}}),
            "127.0.0.1",
            10807,
        );
        let rows = vec![replacement, xray_fixture("2", "XrayOne", 10802)];
        let (mut controller, stock, _, xray, _) = EngineController::fake_quartet();
        controller.set_xray_probe(|| Ok(PathBuf::from("test-xray-bin")));
        let error = controller
            .start_all(&rows)
            .expect_err("a stock replacement cannot shadow the xray bridge row");
        let text = error.to_string();
        assert!(
            matches!(error, EngineError::BadCustom { .. }),
            "the shadow is rejected before bridging, got {text}"
        );
        assert!(
            text.contains("1 row(s)"),
            "counts the shadowed rows, got {text}"
        );
        assert!(!stock.is_running(), "stock never started");
        assert!(!xray.is_running(), "xray never started");
        assert_eq!(stock.check_calls(), 0, "no check ran");
    }

    #[test]
    fn loopback_hygiene_rejects_loopback_egress() {
        for server in ["127.0.0.1", "localhost", "0.0.0.0"] {
            let mut params = xray_params();
            params
                .as_object_mut()
                .expect("object")
                .insert("server".to_string(), json!(server));
            let rows = vec![fixture("2", "Loopy", "xray", params, "127.0.0.1", 10802)];
            let plan = validate_activation(&rows).expect("plan shapes pass");
            let error = generate_xray_config(&plan.egresses).expect_err("loopback egress fails");
            let text = error.to_string();
            assert!(
                matches!(error, EngineError::XrayLoopback { .. }),
                "typed, got {text}"
            );
            assert!(text.contains("\"Loopy\""), "names row: {text}");
        }
    }

    #[test]
    fn custom_outbound_merges_and_broken_names_field() {
        let rows = vec![custom_fixture(
            "9",
            "CustomOne",
            custom_stock_params(),
            10809,
        )];
        let generated = generate(&rows).expect("custom merges");
        assert!(!needs_fork(&rows[0]));
        let node = generated.value()["outbounds"]
            .as_array()
            .expect("outbounds")
            .iter()
            .find(|node| node["tag"] == json!(outbound_tag("9")))
            .expect("merged node present")
            .clone();
        assert_eq!(node["fancy"], json!("kept"), "verbatim merge");
        assert_eq!(node["type"], json!("vless"));

        let broken = vec![custom_fixture(
            "9",
            "Broken",
            json!({"server": "203.0.113.9"}),
            10809,
        )];
        let error = generate(&broken).expect_err("missing type fails");
        let text = error.to_string();
        assert!(matches!(error, EngineError::BadCustom { .. }));
        assert!(text.contains("\"type\""), "names field: {text}");
        assert!(text.contains("\"Broken\""), "names row: {text}");

        let (mut controller, stock, _, _, _) = EngineController::fake_quartet();
        controller
            .start_all(&[fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            )])
            .expect("stock starts");
        assert!(stock.is_running());
        let error = controller
            .start_one(&broken[0].clone())
            .expect_err("broken custom blocked");
        assert!(error.to_string().contains("\"type\""), "got {error}");
        assert!(stock.is_running(), "running endpoint unaffected");
    }

    #[test]
    fn custom_full_config_replaces_owning_core() {
        let replacement = fixture(
            "7",
            "CustomStock",
            "custom-config",
            json!({"core": "stock",
                   "config": {"outbounds": [{"type": "direct", "tag": "direct"}],
                              "custom_marker": "mine"}}),
            "127.0.0.1",
            10807,
        );
        let (mut controller, stock, fork, xray, extra) = EngineController::fake_quartet();
        controller
            .start_all(std::slice::from_ref(&replacement))
            .expect("replacement starts");
        let served = stock.last_config().expect("stock served the replacement");
        assert!(
            served.contains("custom_marker"),
            "replaced wholesale: {served}"
        );
        assert!(!fork.is_running());
        assert!(!xray.is_running());
        assert!(!extra.is_running());

        let second = fixture(
            "8",
            "CustomStockTwo",
            "custom-config",
            json!({"config": {"outbounds": [{"type": "direct", "tag": "direct"}]}}),
            "127.0.0.1",
            10808,
        );
        let error = controller
            .start_one(&second)
            .expect_err("second stock replacement fails");
        assert!(error.to_string().contains("\"core\""), "got {error}");
    }

    #[test]
    fn bug08_custom_full_config_rejects_same_core_siblings() {
        let stock = fixture(
            "1",
            "Alpha",
            "shadowsocks",
            ss_params("pass1"),
            "127.0.0.1",
            10801,
        );
        let custom_stock = fixture(
            "7",
            "CustomStock",
            "custom-config",
            json!({"core": "stock",
                   "config": {"outbounds": [{"type": "direct", "tag": "direct"}]}}),
            "127.0.0.1",
            10807,
        );
        let error = validate_activation(&[stock.clone(), custom_stock.clone()])
            .expect_err("stock replacement shadowing a sibling row fails");
        let text = error.to_string();
        assert!(
            matches!(error, EngineError::BadCustom { .. }),
            "typed: {text}"
        );
        assert!(text.contains("stock"), "names the core: {text}");
        assert!(text.contains("1 row(s)"), "counts the siblings: {text}");
        assert!(
            text.contains("stop or move"),
            "tells the user what to do: {text}"
        );

        let plan = validate_activation(std::slice::from_ref(&custom_stock))
            .expect("a sibling-free replacement still plans");
        assert_eq!(plan.replacements.len(), 1);

        let custom_fork = fixture(
            "8",
            "CustomFork",
            "custom-config",
            json!({"core": "fork",
                   "config": {"outbounds": [{"type": "direct", "tag": "direct"}]}}),
            "127.0.0.1",
            10808,
        );
        validate_activation(&[stock.clone(), custom_fork])
            .expect("a stock row never shadows a fork replacement");

        let xray = xray_fixture("2", "XrayOne", 10802);
        let error = validate_activation(&[xray, custom_stock])
            .expect_err("a stock replacement would shadow an xray bridge row");
        assert!(error.to_string().contains("row(s)"), "got {error}");
    }

    #[test]
    fn extra_slots_appdir_unprivileged_named() {
        assert!(resolve_extra_binary_in(None, "core.exe").is_err());
        for bad in [
            "../evil.exe",
            "..\\evil.exe",
            "/abs/core.exe",
            "",
            ".",
            "..",
        ] {
            assert!(
                resolve_extra_binary_in(Some(Path::new("/app")), bad).is_err(),
                "{bad} rejected"
            );
        }
        let searched =
            resolve_extra_binary_in(Some(Path::new("/nonexistent-manyproxy-dir")), "core.exe")
                .expect_err("missing exe fails");
        assert!(
            searched.contains("/nonexistent-manyproxy-dir"),
            "got {searched}"
        );

        let no_exe = fixture(
            "5",
            "NoExe",
            "extra-core",
            json!({"config": {}}),
            "127.0.0.1",
            10805,
        );
        let error = validate_extra_row(&no_exe).expect_err("exe required");
        assert!(matches!(error, EngineError::BadExtra { .. }));
        assert!(error.to_string().contains("\"exe\""), "got {error}");

        let (mut controller, stock, _, _, extra) = EngineController::fake_quartet();
        controller
            .start_all(&[fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            )])
            .expect("stock starts");
        let slot = extra_fixture("5", "Slot", 10805);
        let error = controller.start_one(&slot).expect_err("missing exe fails");
        let text = error.to_string();
        assert!(matches!(error, EngineError::MissingExtraBinary { .. }));
        assert!(text.contains("\"Slot\""), "names row: {text}");
        assert!(text.contains("user-core.exe"), "names file: {text}");
        assert!(stock.is_running(), "stock untouched");

        controller.set_extra_probe(|_| Ok(PathBuf::from("test-extra-exe")));
        controller.start_one(&slot).expect("slot starts");
        assert!(extra.is_running());
        assert!(controller.is_running("5"));
        let served = extra.last_config().expect("extra got its config");
        assert!(
            served.contains("127.0.0.1"),
            "config passed through: {served}"
        );

        let rival = extra_fixture("6", "Rival", 10806);

        let _rival_extra = controller.push_test_extra_backend();
        controller
            .start_one(&rival)
            .expect("second extra slot starts beside the first");
        assert!(controller.is_running("5"), "first slot keeps serving");
        assert!(controller.is_running("6"), "second slot serves too");
        assert_eq!(controller.running_count(), 3);
    }

    #[test]
    fn chain_midchain_egress_only_violation() {
        let inline_extra = json!({"type": "extra-core", "exe": "x", "config": {}});
        let chain = chain_fixture(
            "8",
            "BadChain",
            json!([inline_extra,
                   {"type": "vless", "server": "198.51.100.9", "server_port": 443}]),
            10808,
        );
        let error = generate(std::slice::from_ref(&chain)).expect_err("extra mid-chain fails");
        let text = error.to_string();
        assert!(matches!(error, EngineError::ChainViolation { .. }));
        assert!(text.contains(CHAIN_EGRESS_RULE), "contract text: {text}");
        assert!(text.contains("\"BadChain\""), "names chain: {text}");

        let rows = vec![
            xray_fixture("2", "XrayOne", 10802),
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            ),
            chain_fixture("8", "RefChain", json!(["2", "1"]), 10808),
        ];
        let error = validate_activation(&rows).expect_err("xray mid-chain fails");
        let text = error.to_string();
        assert!(matches!(error, EngineError::ChainViolation { .. }));
        assert!(text.contains(CHAIN_EGRESS_RULE), "contract text: {text}");

        let rows = vec![
            custom_fixture("9", "CustomOne", custom_stock_params(), 10809),
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            ),
            chain_fixture("8", "CustomMid", json!(["9", "1"]), 10808),
        ];
        let error = validate_activation(&rows).expect_err("custom mid-chain fails");
        assert!(error.to_string().contains(CHAIN_EGRESS_RULE), "got {error}");
    }

    #[test]
    fn chain_valid_stock_chain_generates_detour() {
        let rows = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            ),
            fixture("2", "Beta", "vless", vless_params(), "127.0.0.1", 10802),
            chain_fixture("8", "ChainAB", json!(["1", "2"]), 10808),
        ];
        let generated = generate(&rows).expect("chain generates");
        let value = generated.value();
        let outbounds = value["outbounds"].as_array().expect("outbounds");
        let hop0 = format!("{}-hop-0", outbound_tag("8"));
        let hop1 = format!("{}-hop-1", outbound_tag("8"));
        let first = outbounds
            .iter()
            .find(|node| node["tag"] == json!(hop0))
            .expect("first hop present");
        assert_eq!(first["detour"], json!(hop1), "detour spine");
        let last = outbounds
            .iter()
            .find(|node| node["tag"] == json!(hop1))
            .expect("last hop present");
        assert!(last.get("detour").is_none(), "egress terminates");
        let rule = value["route"]["rules"]
            .as_array()
            .expect("rules")
            .iter()
            .find(|rule| rule["inbound"] == json!([inbound_tag("8")]))
            .expect("chain rule present");
        assert_eq!(rule["outbound"], json!(hop0));
    }

    #[test]
    fn chain_xray_last_bridges_to_sidecar() {
        let rows = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            ),
            xray_fixture("2", "XrayOne", 10802),
            chain_fixture("8", "ChainAX", json!(["1", "2"]), 10808),
        ];
        let plan = validate_activation(&rows).expect("xray-last plans");
        assert_eq!(plan.egresses.len(), 1, "ref-hop reuses its row's leg");
        let (mut controller, stock, _, xray, _) = EngineController::fake_quartet();
        controller.set_xray_probe(|| Ok(PathBuf::from("test-xray-bin")));
        controller.start_all(&rows).expect("xray-last starts");
        assert!(stock.is_running());
        assert!(xray.is_running());
        let sidecar: serde_json::Value =
            serde_json::from_str(&xray.last_config().expect("sidecar config")).expect("JSON");
        assert_eq!(sidecar["inbounds"].as_array().expect("in").len(), 1);
        let main: serde_json::Value =
            serde_json::from_str(&stock.last_config().expect("main config")).expect("JSON");
        let main_outbounds = main["outbounds"].as_array().expect("outbounds");
        let hop_pointer = main_outbounds
            .iter()
            .find(|node| node["tag"] == json!(format!("{}-hop-1", outbound_tag("8"))))
            .expect("chain hop pointer present");
        let row_pointer = main_outbounds
            .iter()
            .find(|node| node["tag"] == json!(outbound_tag("2")))
            .expect("row pointer present");
        assert_eq!(
            hop_pointer["server_port"], row_pointer["server_port"],
            "ref-hop shares its row's bridge port"
        );
    }

    #[test]
    fn mixed_xray_custom_stock_start_all_isolated() {
        let rows = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            ),
            custom_fixture("9", "CustomOne", custom_stock_params(), 10809),
            xray_fixture("2", "XrayOne", 10802),
        ];
        let (mut controller, stock, fork, xray, extra) = EngineController::fake_quartet();
        controller.set_xray_probe(|| Ok(PathBuf::from("test-xray-bin")));
        controller.start_all(&rows).expect("mixed set starts");
        assert_eq!(controller.running_count(), 3);
        assert!(stock.is_running());
        assert!(xray.is_running());
        assert!(!fork.is_running());
        assert!(!extra.is_running());
        assert!(controller.is_running("2"), "xray row reports its sidecar");
        let live: serde_json::Value =
            serde_json::from_str(&stock.last_config().expect("main config")).expect("JSON");
        assert_eq!(live["inbounds"].as_array().expect("in").len(), 3);
        assert_eq!(live["route"]["rules"].as_array().expect("rules").len(), 3);
        let isolation = controller.isolation_map();
        assert_eq!(isolation.len(), 3);
    }

    #[test]
    fn missing_xray_fails_only_xray_rows() {
        let (mut controller, stock, _, xray, _) = EngineController::fake_quartet();
        controller
            .start_all(&[fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            )])
            .expect("stock works without xray");
        assert!(stock.is_running());
        let error = controller
            .start_one(&xray_fixture("2", "XrayOne", 10802))
            .expect_err("xray without binary fails");
        let text = error.to_string();
        assert!(matches!(error, EngineError::MissingXrayBinary { .. }));
        assert!(text.contains("\"XrayOne\""), "names row: {text}");
        assert!(text.contains("xray"), "names binary: {text}");
        assert!(stock.is_running(), "stock sidecar untouched");
        assert!(!xray.is_running(), "xray sidecar never started");
    }

    #[test]
    fn xray_resolution_names_expected_binary() {
        assert!(xray_binary_file_name().contains("xray"));
        let searched = resolve_xray_binary_in(Some(Path::new("/nonexistent-manyproxy-dir")), "")
            .expect_err("no xray binary anywhere");
        assert!(
            searched.contains("/nonexistent-manyproxy-dir"),
            "names searched dir: {searched}"
        );
    }

    #[test]
    fn bridge_ports_skip_taken_and_exhaust_explicitly() {
        let taken: HashSet<u16> = [11801].into_iter().collect();
        let ports = assign_bridge_ports(&["a".to_string()], &taken).expect("assigns");
        assert_eq!(ports["a"], 11802, "taken bridge port skipped");
        let full: HashSet<u16> = (XRAY_BRIDGE_PORT_START..=XRAY_BRIDGE_PORT_END).collect();
        let error = assign_bridge_ports(&["a".to_string()], &full).expect_err("exhaustion fails");
        assert!(matches!(error, EngineError::InvalidGenerated { .. }));
    }

    #[test]
    fn chain_shape_errors_name_nodes() {
        let lookup = |_: &str| None;
        for params in [
            json!({}),
            json!({"nodes": []}),
            json!({"nodes": ["missing-id"]}),
            json!({"nodes": [{"server": "x"}]}),
            json!({"nodes": [{"type": "chain", "nodes": []}]}),
        ] {
            let map = params.as_object().expect("object").clone();
            let error = validate_chain_nodes("Shapy", &map, &lookup).expect_err("shape fails");
            let text = error.to_string();
            assert!(text.contains("\"nodes\""), "names field: {text}");
            assert!(
                !text.contains("missing-id") || text.contains("unknown id"),
                "{text}"
            );
        }
    }

    fn t63_hop(kind: ChainHopKind, label: &str) -> ChainHop {
        ChainHop {
            kind,
            ref_id: None,
            node: None,
            label: label.to_string(),
        }
    }

    fn t63_ordering_rejected(name: &str, hops: &[ChainHop]) -> String {
        let chains = [(name, hops)];
        let error = validate_chain_transition_order(&chains).expect_err("ordering fails");
        assert!(matches!(error, EngineError::ChainViolation { .. }));
        let text = error.to_string();
        assert!(text.contains(CHAIN_EGRESS_RULE), "family wrap: {text}");
        assert!(text.contains(name), "names chain: {text}");
        assert!(
            text.contains("chain hop sequence must stay sing-box → xray → sing-box"),
            "pattern phrase: {text}"
        );
        text
    }

    #[test]
    fn t63_transition_order_accepts_sing_xray_sing() {
        let cases: Vec<(&str, Vec<ChainHop>)> = vec![
            (
                "SingXray",
                vec![
                    t63_hop(ChainHopKind::Standard, "node 1"),
                    t63_hop(ChainHopKind::Xray, "node 2"),
                ],
            ),
            (
                "SingSingXray",
                vec![
                    t63_hop(ChainHopKind::Standard, "node 1"),
                    t63_hop(ChainHopKind::Standard, "node 2"),
                    t63_hop(ChainHopKind::Xray, "node 3"),
                ],
            ),
            ("XrayOnly", vec![t63_hop(ChainHopKind::Xray, "node 1")]),
            (
                "AllSing",
                vec![
                    t63_hop(ChainHopKind::Standard, "node 1"),
                    t63_hop(ChainHopKind::Standard, "node 2"),
                ],
            ),
        ];
        let views: Vec<(&str, &[ChainHop])> = cases
            .iter()
            .map(|(name, hops)| (*name, hops.as_slice()))
            .collect();
        validate_chain_transition_order(&views).expect("sing → xray → sing walk stays valid");

        let rows = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            ),
            xray_fixture("2", "XrayOne", 10802),
            chain_fixture("8", "ChainAX", json!(["1", "2"]), 10808),
        ];
        let plan = validate_activation(&rows).expect("xray-last stays the accepted walk");
        assert_eq!(plan.egresses.len(), 1, "xray hop keeps its row's leg");
    }

    #[test]
    fn t63_rejects_consecutive_extra_core_hops() {
        let extra_first = t63_hop(ChainHopKind::Extra, "node 1");
        let std_mid = t63_hop(ChainHopKind::Standard, "node 2");
        let extra_last = t63_hop(ChainHopKind::Extra, "node 3");
        let shapes: Vec<(&str, Vec<ChainHop>)> = vec![
            (
                "AdjacentExtras",
                vec![extra_first.clone(), extra_last.clone()],
            ),
            ("SpreadExtras", vec![extra_first, std_mid, extra_last]),
        ];
        for (name, hops) in &shapes {
            let chains = [(*name, hops.as_slice())];
            let error = validate_chain_transition_order(&chains)
                .expect_err("a second extra-core hop fails");
            assert!(matches!(error, EngineError::ChainViolation { .. }));
            let text = error.to_string();
            assert!(text.contains(CHAIN_EGRESS_RULE), "family wrap: {text}");
            assert!(text.contains(name), "names chain: {text}");
            assert!(
                text.contains("at most one extracore hop per chain"),
                "family rule: {text}"
            );
            assert!(text.contains("node 1"), "names first hop: {text}");
            assert!(text.contains("node 3"), "names last hop: {text}");
        }
    }

    #[test]
    fn t63_rejects_xray_custom_xray_shapes() {
        let text = t63_ordering_rejected(
            "ConsecutiveXray",
            &[
                t63_hop(ChainHopKind::Xray, "node 1"),
                t63_hop(ChainHopKind::Xray, "node 2"),
            ],
        );
        assert!(
            text.contains("two consecutive xray hops"),
            "pattern: {text}"
        );
        let text = t63_ordering_rejected(
            "XrayThenExtra",
            &[
                t63_hop(ChainHopKind::Xray, "node 1"),
                t63_hop(ChainHopKind::Extra, "node 2"),
            ],
        );
        assert!(
            text.contains("an xray hop followed by an extra/custom hop"),
            "pattern: {text}"
        );
        t63_ordering_rejected(
            "XrayThenCustom",
            &[
                t63_hop(ChainHopKind::Xray, "node 1"),
                t63_hop(ChainHopKind::Custom, "node 2"),
            ],
        );
        t63_ordering_rejected(
            "ExtraThenXray",
            &[
                t63_hop(ChainHopKind::Extra, "node 1"),
                t63_hop(ChainHopKind::Xray, "node 2"),
            ],
        );
        t63_ordering_rejected(
            "CustomThenXray",
            &[
                t63_hop(ChainHopKind::Custom, "node 1"),
                t63_hop(ChainHopKind::Xray, "node 2"),
            ],
        );
        t63_ordering_rejected(
            "XrayCustomXray",
            &[
                t63_hop(ChainHopKind::Xray, "node 1"),
                t63_hop(ChainHopKind::Custom, "node 2"),
                t63_hop(ChainHopKind::Xray, "node 3"),
            ],
        );
    }

    #[test]
    fn t63_rejects_selector_slot() {
        let autoselector = ChainHop {
            kind: ChainHopKind::Standard,
            ref_id: None,
            node: Some(
                json!({"type": "autoselector"})
                    .as_object()
                    .expect("object")
                    .clone(),
            ),
            label: "node 2".to_string(),
        };
        let slots = [t63_hop(ChainHopKind::Standard, "node 1"), autoselector];
        let chains = [("AutoSelect", slots.as_slice())];
        let error = validate_chain_transition_order(&chains).expect_err("autoselector slot fails");
        assert!(matches!(error, EngineError::ChainViolation { .. }));
        let text = error.to_string();
        assert!(text.contains(CHAIN_EGRESS_RULE), "family wrap: {text}");
        assert!(text.contains("AutoSelect"), "names chain: {text}");
        assert!(
            text.contains(
                "chain hops cannot take an autoselector (v1 chains are flat — pick a concrete row)"
            ),
            "family text: {text}"
        );
        assert!(text.contains("node 2"), "names slot: {text}");

        let rows = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            ),
            chain_fixture("8", "AutoChain", json!([{"type": "autoselector"}]), 10808),
        ];
        let error = validate_activation(&rows).expect_err("apply path rejects the slot");
        assert!(matches!(error, EngineError::ChainViolation { .. }));
        let text = error.to_string();
        assert!(
            text.contains("chain hops cannot take an autoselector"),
            "family error through the apply path: {text}"
        );
    }

    #[test]
    fn t63_valid_chains_byte_identical_after_validation() {
        let ref_rows = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            ),
            fixture("2", "Beta", "vless", vless_params(), "127.0.0.1", 10802),
            chain_fixture("8", "ChainAB", json!(["1", "2"]), 10808),
        ];
        let xray_rows = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            ),
            xray_fixture("2", "XrayOne", 10802),
            chain_fixture("8", "ChainAX", json!(["1", "2"]), 10808),
        ];
        let inline_rows = vec![chain_fixture(
            "8",
            "ChainInline",
            json!([{"type": "vless", "server": "198.51.100.9", "server_port": 443}]),
            10808,
        )];
        validate_chain_transition_order(&[(
            "ChainAX",
            [
                t63_hop(ChainHopKind::Standard, "node 1"),
                ChainHop {
                    kind: ChainHopKind::Xray,
                    ref_id: Some("2".to_string()),
                    node: None,
                    label: "node 2 (\"2\")".to_string(),
                },
            ]
            .as_slice(),
        )])
        .expect("today's xray-last chain stays the accepted transition");
        validate_chain_transition_order(&[(
            "ChainInline",
            [ChainHop {
                kind: ChainHopKind::Standard,
                ref_id: None,
                node: Some(
                    json!({"type": "vless", "server": "198.51.100.9", "server_port": 443})
                        .as_object()
                        .expect("object")
                        .clone(),
                ),
                label: "node 1".to_string(),
            }]
            .as_slice(),
        )])
        .expect("today's inline standard chain stays valid");

        assert_eq!(
            generate(&ref_rows).expect("ref chain generates").json,
            r#"{"log":{"level":"warn"},"inbounds":[{"type":"mixed","tag":"in-1","listen":"127.0.0.1","listen_port":10801},{"type":"mixed","tag":"in-2","listen":"127.0.0.1","listen_port":10802},{"type":"mixed","tag":"in-8","listen":"127.0.0.1","listen_port":10808}],"outbounds":[{"type":"shadowsocks","server":"203.0.113.7","server_port":8388,"method":"aes-128-gcm","password":"pass1","tag":"out-1"},{"type":"vless","server":"198.51.100.9","server_port":443,"uuid":"11111111-2222-4333-8444-555555555555","tls":{"enabled":true,"server_name":"198.51.100.9"},"tag":"out-2"},{"type":"shadowsocks","server":"203.0.113.7","server_port":8388,"method":"aes-128-gcm","password":"pass1","tag":"out-8-hop-0","detour":"out-8-hop-1"},{"type":"vless","server":"198.51.100.9","server_port":443,"uuid":"11111111-2222-4333-8444-555555555555","tls":{"enabled":true,"server_name":"198.51.100.9"},"tag":"out-8-hop-1"},{"type":"direct","tag":"direct"}],"endpoints":[],"route":{"rules":[{"inbound":["in-1"],"action":"route","outbound":"out-1"},{"inbound":["in-2"],"action":"route","outbound":"out-2"},{"inbound":["in-8"],"action":"route","outbound":"out-8-hop-0"}],"final":"direct"}}"#,
            "today's stock ref chain bytes"
        );
        assert_eq!(
            generate(&xray_rows).expect("xray chain generates").json,
            r#"{"log":{"level":"warn"},"inbounds":[{"type":"mixed","tag":"in-1","listen":"127.0.0.1","listen_port":10801},{"type":"mixed","tag":"in-2","listen":"127.0.0.1","listen_port":10802},{"type":"mixed","tag":"in-8","listen":"127.0.0.1","listen_port":10808}],"outbounds":[{"type":"shadowsocks","server":"203.0.113.7","server_port":8388,"method":"aes-128-gcm","password":"pass1","tag":"out-1"},{"type":"socks","tag":"out-2","server":"127.0.0.1","server_port":11801,"version":"5"},{"type":"shadowsocks","server":"203.0.113.7","server_port":8388,"method":"aes-128-gcm","password":"pass1","tag":"out-8-hop-0","detour":"out-8-hop-1"},{"type":"socks","tag":"out-8-hop-1","server":"127.0.0.1","server_port":11801,"version":"5"},{"type":"direct","tag":"direct"}],"endpoints":[],"route":{"rules":[{"inbound":["in-1"],"action":"route","outbound":"out-1"},{"inbound":["in-2"],"action":"route","outbound":"out-2"},{"inbound":["in-8"],"action":"route","outbound":"out-8-hop-0"}],"final":"direct"}}"#,
            "today's xray-last chain bytes"
        );
        assert_eq!(
            generate(&inline_rows).expect("inline chain generates").json,
            r#"{"log":{"level":"warn"},"inbounds":[{"type":"mixed","tag":"in-8","listen":"127.0.0.1","listen_port":10808}],"outbounds":[{"type":"vless","server":"198.51.100.9","server_port":443,"tag":"out-8-hop-0"},{"type":"direct","tag":"direct"}],"endpoints":[],"route":{"rules":[{"inbound":["in-8"],"action":"route","outbound":"out-8-hop-0"}],"final":"direct"}}"#,
            "today's inline standard chain bytes"
        );
    }

    #[test]
    fn t23_missing_binaries_fail_typed_without_side_effects() {
        let (mut controller, stock, fork, xray, extra) = EngineController::fake_quartet();
        controller.set_stock_probe(|| {
            Err(EngineError::MissingBinary {
                binary: engine_binary_file_name(),
                searched: "test-seam (no stock binary)".to_string(),
            })
        });
        let error = controller
            .start_all(&three_standard())
            .expect_err("missing stock binary fails");
        assert!(
            matches!(error, EngineError::MissingBinary { .. }),
            "typed, got {error}"
        );
        assert!(error.to_string().contains("never downloads"), "got {error}");
        assert!(!stock.is_running());
        assert!(!fork.is_running());
        assert!(!xray.is_running());
        assert!(!extra.is_running());
        assert_eq!(controller.running_count(), 0, "active restored");
        assert_eq!(controller.status(), &EngineStatus::Stopped);

        let mut real = RealEngineBackend::xray();
        let path = real.config_path.clone();
        assert!(
            real.check_config("{\"secret\": true}").is_err(),
            "no xray binary in test env"
        );
        assert!(!path.exists(), "no file left: {}", path.display());

        let real = RealEngineBackend::new();
        let path = real.config_path.clone();
        real.write_config("{\"password\": \"hunter2\"}")
            .expect("restricted write works");
        assert!(path.exists());
        real.remove_config_file();
        assert!(!path.exists(), "removed: {}", path.display());
    }

    #[test]
    fn bug22_check_spawn_failure_removes_temp_config() {
        let bogus = std::env::temp_dir().join(format!(
            "manyproxy-bug22-bogus-{}-{}.exe",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::write(&bogus, "not a real executable").expect("bogus binary written");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mut perms = std::fs::metadata(&bogus).expect("meta").permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(&bogus, perms);
        }

        let mut real = RealEngineBackend::new();
        real.pin_binary(bogus.clone());
        let path = real.config_path.clone();
        assert!(
            real.check_config("{\"password\": \"hunter2\"}").is_err(),
            "spawning a bogus binary fails"
        );
        assert!(
            !path.exists(),
            "failed check left no secret-bearing temp config: {}",
            path.display()
        );

        let _ = std::fs::remove_file(&bogus);
    }

    #[test]
    fn t23_crash_names_shard_keeps_siblings() {
        let (mut controller, stock, _fork, xray, _extra) = EngineController::fake_quartet();
        controller.set_xray_probe(|| Ok(PathBuf::from("test-xray-bin")));
        let rows = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            ),
            xray_fixture("2", "XrayOne", 10802),
        ];
        controller.start_all(&rows).expect("starts");
        assert!(stock.is_running() && xray.is_running());

        xray.inject_crash("signal: killed");
        let shards = controller.poll_shards();
        assert_eq!(shards.len(), 1, "one crash, got {shards:?}");
        assert_eq!(shards[0].0, "xray", "names the shard: {shards:?}");
        assert_eq!(shards[0].1, "signal: killed");
        assert!(stock.is_running(), "sibling keeps running");
        assert!(!xray.is_running(), "failed shard is down");
        assert!(controller.poll().is_some(), "cause stays pending");
        controller.clear_pending_crash();
        assert!(controller.poll().is_none(), "cleared after handling");
        assert!(matches!(controller.status(), EngineStatus::Crashed { .. }));

        stock.inject_crash("signal: segmentation fault");
        xray.inject_crash("signal: killed");

        controller.set_xray_probe(|| Ok(PathBuf::from("test-xray-bin")));
        let _ = controller.start_all_confirmed(&rows);
        stock.inject_crash("signal: segmentation fault");
        xray.inject_crash("signal: killed");
        let cause = controller.poll().expect("crashes reported");
        assert!(cause.contains("segmentation fault"), "got {cause}");
        assert!(cause.contains("killed"), "got {cause}");
        assert!(cause.contains("stock"), "shard-named: {cause}");
        assert!(cause.contains("xray"), "shard-named: {cause}");
        controller.clear_pending_crash();
        assert!(controller.poll().is_none(), "causes cleared after handling");
    }

    #[test]
    fn t23_nested_system_mutation_rejected_with_path() {
        for (key, doc) in [
            (
                "tun",
                json!({"core": "stock",
                       "config": {"outbounds": [{"type": "direct", "tag": "direct"}],
                                  "route": {"rules": [{"tun": {"secret": "hunter2"}}]}}}),
            ),
            (
                "dns",
                json!({"core": "fork",
                       "config": {"outbounds": [{"type": "direct", "tag": "direct"}],
                                  "dns": {"servers": ["example.com"]}}}),
            ),
            (
                "set_system_proxy",
                json!({"core": "xray",
                       "config": {"outbounds": [{"type": "direct", "tag": "direct"}],
                                  "inbounds": [{"set_system_proxy": true}]}}),
            ),
        ] {
            let evil = fixture("7", "Evil", "custom-config", doc, "127.0.0.1", 10807);
            let (mut controller, stock, fork, xray, _) = EngineController::fake_quartet();
            let error = controller
                .start_all(std::slice::from_ref(&evil))
                .expect_err(&format!("nested {key} rejected"));
            let text = error.to_string();
            assert!(matches!(error, EngineError::BadCustom { .. }), "got {text}");
            assert!(text.contains(key), "names the key: {text}");
            assert!(text.contains('$'), "names the path: {text}");
            assert!(!text.contains("hunter2"), "no values: {text}");
            assert!(!stock.is_running() && !fork.is_running() && !xray.is_running());
        }
    }

    #[test]
    fn t23_lan_enforcement_without_ui_gate() {
        let lan = fixture(
            "9",
            "LanRow",
            "shadowsocks",
            ss_params("pass1"),
            "0.0.0.0",
            10809,
        );
        let (mut controller, stock) = EngineController::fake();
        let error = controller
            .start_one(&lan)
            .expect_err("unconfirmed LAN start fails");
        assert!(
            matches!(error, EngineError::LanConfirmRequired { .. }),
            "typed gate, got {error}"
        );
        let text = error.to_string();
        assert!(text.contains("\"LanRow\""), "names row: {text}");
        assert!(text.contains("0.0.0.0"), "names bind: {text}");
        assert!(!stock.is_running(), "nothing started");
        assert_eq!(controller.running_count(), 0);

        controller
            .start_one_confirmed(&lan)
            .expect("confirmed LAN start works");
        assert!(stock.is_running());
        assert_eq!(controller.running_count(), 1);

        let mut moved = lan.clone();
        moved.bind_ip = "10.0.0.5".to_string();
        let error = controller
            .start_one(&moved)
            .expect_err("moved bind re-requires confirm");
        assert!(matches!(error, EngineError::LanConfirmRequired { .. }));

        let loopback = fixture(
            "1",
            "Alpha",
            "shadowsocks",
            ss_params("pass1"),
            "127.0.0.1",
            10801,
        );
        controller.start_one(&loopback).expect("loopback starts");
    }

    #[test]
    fn t23_spawn_failure_rolls_back_serving_set() {
        let (mut controller, stock, _fork) = EngineController::fake_pair();
        controller.set_fork_probe(|| Ok(PathBuf::from("test-fork-bin")));
        let alpha = fixture(
            "1",
            "Alpha",
            "shadowsocks",
            ss_params("pass1"),
            "127.0.0.1",
            10801,
        );
        controller.start_one(&alpha).expect("alpha starts");
        let served_before = stock.last_config().expect("served config");
        assert!(served_before.contains("pass1"));

        stock.fail_start("stock: address in use");
        let beta = fixture("2", "Beta", "vless", vless_params(), "127.0.0.1", 10802);
        let error = controller.start_one(&beta).expect_err("spawn fails");
        assert!(
            matches!(error, EngineError::SpawnFailed { .. }),
            "got {error}"
        );
        assert_eq!(
            controller.running_ids(),
            vec!["1".to_string()],
            "membership restored"
        );
        assert!(stock.is_running(), "shard restored to previous config");
        let served_after = stock.last_config().expect("restored config");
        assert_eq!(served_before, served_after, "previous config re-served");
        assert_eq!(controller.status(), &EngineStatus::Running);
        stock.clear_failures();
    }

    #[test]
    fn t23_port_probe_and_alias_conflict() {
        let held = std::net::TcpListener::bind("127.0.0.1:0").expect("ephemeral port");
        let port = held.local_addr().expect("addr").port();
        let squatter = fixture(
            "9",
            "Squatter",
            "shadowsocks",
            ss_params("pass1"),
            "127.0.0.1",
            port,
        );
        let (mut controller, stock) = EngineController::fake();
        let error = controller
            .start_all(std::slice::from_ref(&squatter))
            .expect_err("held port fails");
        assert!(
            matches!(error, EngineError::PortUnavailable { .. }),
            "typed, got {error}"
        );
        let text = error.to_string();
        assert!(text.contains(&port.to_string()), "names port: {text}");
        assert!(text.contains("\"Squatter\""), "names row: {text}");
        assert!(!stock.is_running(), "nothing stopped, nothing started");
        drop(held);

        let alias_rows = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10831,
            ),
            fixture("2", "Beta", "vless", vless_params(), "localhost", 10831),
        ];
        let error = validate_activation(&alias_rows).expect_err("alias conflict");
        let text = error.to_string();
        assert!(matches!(error, EngineError::PortConflict { .. }));
        assert!(
            text.contains("\"Alpha\"") && text.contains("\"Beta\""),
            "{text}"
        );
    }

    #[test]
    fn t23_duplicate_id_names_id() {
        let rows = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            ),
            fixture(
                "1",
                "Clone",
                "shadowsocks",
                ss_params("pass2"),
                "127.0.0.1",
                10802,
            ),
        ];
        let error = validate_activation(&rows).expect_err("dup id fails");
        let text = error.to_string();
        assert!(text.contains("\"1\""), "names the id: {text}");
    }

    #[test]
    fn t23_extra_exe_cleared_when_slot_empties() {
        let mut real = RealEngineBackend::extra_slot();
        real.set_extra_exe(PathBuf::from("user-core.exe"));
        assert!(real.extra_exe.is_some());
        real.clear_extra_exe();
        assert!(real.extra_exe.is_none(), "slot forgotten");
    }

    #[test]
    fn t23_config_paths_unique_per_instance() {
        let backends = [
            RealEngineBackend::new(),
            RealEngineBackend::fork(),
            RealEngineBackend::xray(),
            RealEngineBackend::extra_slot(),
            RealEngineBackend::new(),
        ];
        let mut seen = HashSet::new();
        for backend in &backends {
            let path = &backend.config_path;
            assert!(seen.insert(path.clone()), "shared path: {}", path.display());
            let name = path.file_name().expect("file name").to_string_lossy();
            assert!(
                name.contains(&std::process::id().to_string()),
                "pid suffix: {name}"
            );
        }
    }

    #[test]
    fn t23_drop_removes_secret_file() {
        let path = {
            let real = RealEngineBackend::new();
            let path = real.config_path.clone();
            real.write_config("{\"password\": \"hunter2\"}")
                .expect("restricted write works");
            assert!(path.exists());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                let mode = std::fs::metadata(&path)
                    .expect("metadata")
                    .permissions()
                    .mode()
                    & 0o777;
                assert_eq!(mode, 0o600, "owner-only, got {mode:o}");
            }
            path
        };
        assert!(!path.exists(), "drop cleaned: {}", path.display());
    }

    fn ssh_fixture(id: &str, name: &str, port: u16) -> ProxyConfig {
        fixture(
            id,
            name,
            "ssh",
            json!({"type": "ssh", "server": "198.51.100.7", "server_port": 22,
                   "user": "alice", "password": "hunter2"}),
            "127.0.0.1",
            port,
        )
    }

    fn anytls_fixture(id: &str, name: &str, port: u16) -> ProxyConfig {
        fixture(
            id,
            name,
            "anytls",
            json!({"type": "anytls", "server": "198.51.100.9", "server_port": 443,
                   "password": "anytls-secret"}),
            "127.0.0.1",
            port,
        )
    }

    fn noise_fixture(id: &str, name: &str, port: u16) -> ProxyConfig {
        fixture(
            id,
            name,
            "noise",
            json!({"type": "noise", "server": "192.0.2.44", "server_port": 51820,
                   "password": "NOISE-KEY", "key": "NOISE-KEY"}),
            "127.0.0.1",
            port,
        )
    }

    #[test]
    fn ticket27_ssh_generates_native_stock_outbound() {
        assert!(is_ssh_protocol("SSH"));
        assert!(!needs_fork(&ssh_fixture("1", "SshOne", 10801)));
        let generated = generate(&[ssh_fixture("1", "SshOne", 10801)]).expect("ssh generates");
        let value = generated.value();
        let node = value["outbounds"]
            .as_array()
            .expect("outbounds")
            .iter()
            .find(|node| node["tag"] == json!(outbound_tag("1")))
            .expect("ssh outbound present");
        assert_eq!(node["type"], json!("ssh"));
        let rule = value["route"]["rules"]
            .as_array()
            .expect("rules")
            .iter()
            .find(|rule| rule["inbound"] == json!([inbound_tag("1")]))
            .expect("ssh rule present");
        assert_eq!(rule["outbound"], json!(outbound_tag("1")));

        let mut bad = ssh_fixture("2", "NoUser", 10802);
        bad.params = json!({"type": "ssh", "server": "198.51.100.7",
            "server_port": 22, "password": "hunter2"})
        .to_string();
        let error = generate(std::slice::from_ref(&bad)).expect_err("user required");
        assert!(
            matches!(error, EngineError::BadParams { .. }),
            "typed, got {error}"
        );
        assert!(
            !error.to_string().contains("hunter2"),
            "no secrets: {error}"
        );
    }

    #[test]
    fn ticket27_anytls_generates_native_stock_outbound() {
        let row = anytls_fixture("9", "AnyOne", 10809);
        assert!(!needs_fork(&row), "native stock, not fork");
        let generated = generate(std::slice::from_ref(&row)).expect("anytls generates");
        let value = generated.value();
        let node = value["outbounds"]
            .as_array()
            .expect("outbounds")
            .iter()
            .find(|node| node["tag"] == json!(outbound_tag("9")))
            .expect("anytls outbound present");
        assert_eq!(node["type"], json!("anytls"));

        let mut bad = anytls_fixture("9", "NoPass", 10809);
        bad.params =
            json!({"type": "anytls", "server": "198.51.100.9", "server_port": 443}).to_string();
        let error = generate(std::slice::from_ref(&bad)).expect_err("password required");
        assert!(
            matches!(error, EngineError::BadParams { .. }),
            "typed, got {error}"
        );
    }

    #[test]
    fn t08_direct_serves_bare_final() {
        assert!(is_direct_protocol("Direct"));
        let row = fixture(
            "7",
            "DirectOne",
            "direct",
            json!({"type": "direct"}),
            "127.0.0.1",
            10807,
        );
        assert!(!needs_fork(&row), "stock, not fork");
        let generated = generate(std::slice::from_ref(&row)).expect("direct generates");
        assert!(!needs_xray(&row), "no bridge");
        let value = generated.value();
        let node = value["outbounds"]
            .as_array()
            .expect("outbounds")
            .iter()
            .find(|node| node["tag"] == json!(outbound_tag("7")))
            .expect("direct outbound present");
        assert_eq!(node["type"], json!("direct"));
        assert_eq!(
            node.as_object().expect("object").len(),
            2,
            "bare final carries no fields: {node}"
        );
        let rule = value["route"]["rules"]
            .as_array()
            .expect("rules")
            .iter()
            .find(|rule| rule["inbound"] == json!([inbound_tag("7")]))
            .expect("direct rule present");
        assert_eq!(rule["outbound"], json!(outbound_tag("7")));
        validate_activation(std::slice::from_ref(&row)).expect("direct activates");
    }

    #[test]
    fn ticket27_noise_routes_fork_with_mismatch_guards() {
        assert!(is_noise_protocol("noise"));
        assert!(needs_fork(&noise_fixture("3", "NoiseOne", 10803)));
        let generated =
            generate(&[noise_fixture("3", "NoiseOne", 10803)]).expect("noise generates");
        let value = generated.value();
        let node = value["outbounds"]
            .as_array()
            .expect("outbounds")
            .iter()
            .find(|node| node["tag"] == json!(outbound_tag("3")))
            .expect("noise outbound present");
        assert_eq!(node["type"], json!("noise"));

        let mut awg_on_noise = noise_fixture("4", "Confused", 10804);
        awg_on_noise.params = json!({"type": "noise", "server": "192.0.2.44",
            "server_port": 51820, "password": "k", "jc": "4"})
        .to_string();
        let error = generate(std::slice::from_ref(&awg_on_noise)).expect_err("AWG on Noise fails");
        assert!(
            matches!(error, EngineError::WrongLayer { .. }),
            "typed, got {error}"
        );
        let text = error.to_string();
        assert!(text.contains("\"jc\""), "names field: {text}");
        assert!(text.contains("Noise"), "names confusion: {text}");

        let mut xray_on_stock = fixture(
            "5",
            "StockNoise",
            "vless",
            json!({"type": "vless", "server": "198.51.100.9", "server_port": 443,
                   "uuid": "11111111-2222-4333-8444-555555555555", "noise": "a"}),
            "127.0.0.1",
            10805,
        );
        let error =
            generate(std::slice::from_ref(&xray_on_stock)).expect_err("noise on stock fails");
        assert!(
            matches!(error, EngineError::WrongLayer { .. }),
            "typed, got {error}"
        );

        let (mut controller, stock, _, _, _) = EngineController::fake_quartet();
        controller
            .start_all(&[fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            )])
            .expect("stock starts without fork");
        assert!(stock.is_running());
        let error = controller
            .start_one(&noise_fixture("3", "NoiseOne", 10803))
            .expect_err("missing fork fails Noise only");
        assert!(
            matches!(error, EngineError::MissingForkBinary { .. }),
            "typed, got {error}"
        );
        let _ = &mut xray_on_stock;
    }

    #[test]
    fn ticket27_three_extra_slots_serve_each_endpoint() {
        let rows = vec![
            extra_fixture("5", "SlotA", 10805),
            extra_fixture("6", "SlotB", 10806),
            extra_fixture("7", "SlotC", 10807),
        ];
        let plan = validate_activation(&rows).expect("three extras plan");
        assert_eq!(plan.extras.len(), 3, "no singleton cap");
        let (mut controller, _, _, _, first) = EngineController::fake_quartet();
        let second = controller.push_test_extra_backend();
        let third = controller.push_test_extra_backend();
        controller.set_extra_probe(|_| Ok(PathBuf::from("test-extra-exe")));
        controller.start_all(&rows).expect("start-all serves each");
        assert_eq!(controller.running_count(), 3);
        assert!(controller.is_running("5"));
        assert!(controller.is_running("6"));
        assert!(controller.is_running("7"));
        for (handle, row) in [&first, &second, &third].iter().zip(rows.iter()) {
            assert!(handle.is_running(), "{} up", row.name);
            let served = handle.last_config().expect("slot got its config");
            assert!(
                served.contains("127.0.0.1"),
                "{} config passed through: {served}",
                row.name
            );
        }
    }

    fn t29_vless_row(
        id: &str,
        name: &str,
        encryption: serde_json::Value,
        port: u16,
    ) -> ProxyConfig {
        let mut params = serde_json::Map::new();
        params.insert("type".to_string(), json!("vless"));
        params.insert("server".to_string(), json!("198.51.100.9"));
        params.insert("server_port".to_string(), json!(443));
        params.insert(
            "uuid".to_string(),
            json!("11111111-2222-4333-8444-555555555555"),
        );
        params.insert("encryption".to_string(), encryption);
        fixture(
            id,
            name,
            "vless",
            serde_json::Value::Object(params),
            "127.0.0.1",
            port,
        )
    }

    fn t29_generated_node(value: &serde_json::Value, tag: &str) -> serde_json::Value {
        value["outbounds"]
            .as_array()
            .expect("outbounds array")
            .iter()
            .find(|node| node["tag"] == json!(tag))
            .expect("outbound present")
            .clone()
    }

    #[test]
    fn t29_vless_encryption_none_drops_key_stays_stock() {
        let row = t29_vless_row("t29a", "VlessNone", json!("none"), 19921);
        assert!(!needs_xray(&row), "none stays stock");
        assert!(!needs_fork(&row), "none needs no fork");
        let plan = validate_activation(std::slice::from_ref(&row)).expect("none plans");
        assert!(
            plan.egresses.is_empty(),
            "no xray legs: {:?}",
            plan.egresses
        );
        assert_eq!(plan.main_rows.len(), 1);
        let generated = generate(&plan.main_rows).expect("stock generates");
        let node = t29_generated_node(&generated.value(), &outbound_tag("t29a"));
        assert_eq!(node["type"], json!("vless"));
        assert!(
            !node.as_object().expect("object").contains_key("encryption"),
            "key dropped for stock: {node}"
        );
    }

    #[test]
    fn t29_vless_without_encryption_unchanged() {
        let row = fixture(
            "t29b",
            "VlessPlain",
            "vless",
            json!({"type": "vless", "server": "198.51.100.9", "server_port": 443,
                   "uuid": "11111111-2222-4333-8444-555555555555"}),
            "127.0.0.1",
            19922,
        );
        assert!(!needs_xray(&row), "plain vless stays stock");
        let generated = generate(std::slice::from_ref(&row)).expect("plain generates");
        let node = t29_generated_node(&generated.value(), &outbound_tag("t29b"));
        assert_eq!(node["type"], json!("vless"));
        assert!(
            !node.as_object().expect("object").contains_key("encryption"),
            "no key invented: {node}"
        );
    }

    #[test]
    fn t29_vless_encryption_blank_and_case_fold_stay_stock() {
        for (id, value) in [
            ("t29c", json!("None")),
            ("t29d", json!(" NONE ")),
            ("t29e", json!("")),
        ] {
            let row = t29_vless_row(id, "VlessFold", value, 19923);
            assert!(!needs_xray(&row), "{id} stays stock");
            let generated = generate(std::slice::from_ref(&row)).expect("stock generates");
            let node = t29_generated_node(&generated.value(), &outbound_tag(id));
            assert!(
                !node.as_object().expect("object").contains_key("encryption"),
                "{id} key dropped: {node}"
            );
        }
    }

    #[test]
    fn t29_vless_real_encryption_routes_xray_bridge() {
        let row = t29_vless_row("t29x", "VlessEnc", json!("synthetic-t29-encryption"), 19924);
        assert!(needs_xray(&row), "real value needs xray");
        assert!(!needs_fork(&row), "xray, not fork");
        let plan = validate_activation(std::slice::from_ref(&row)).expect("xray plans");
        assert_eq!(plan.main_rows.len(), 1, "pointer served by main");
        assert_eq!(plan.egresses.len(), 1, "one bridge leg");
        let egress = &plan.egresses[0];
        assert_eq!(egress.bridge_key, "t29x");
        assert!(!egress.xray_shaped, "vless-shaped leg");
        let xray = generate_xray_config(&plan.egresses).expect("xray config builds");
        let outbounds = xray.value()["outbounds"].clone();
        let node = outbounds
            .as_array()
            .expect("xray outbounds")
            .iter()
            .find(|node| node["tag"] == json!(xray_egress_tag("t29x")))
            .expect("xray vless egress present");
        assert_eq!(node["protocol"], json!("vless"));
        assert_eq!(
            node["settings"]["vnext"][0]["users"][0]["encryption"],
            json!("synthetic-t29-encryption"),
            "verbatim passthrough"
        );
        let generated = generate(std::slice::from_ref(&row)).expect("main generates");
        let pointer = t29_generated_node(&generated.value(), &outbound_tag("t29x"));
        assert_eq!(pointer["type"], json!("socks"), "bridge pointer: {pointer}");
        assert_eq!(pointer["server"], json!("127.0.0.1"));
        assert_eq!(
            pointer["server_port"].as_u64().expect("bridge port"),
            egress.bridge_port as u64,
            "pointer meets ingress"
        );
    }

    #[test]
    fn t29_vless_real_encryption_carries_flow_and_packet_encoding() {
        let row = fixture(
            "t29y",
            "VlessWs",
            "vless",
            json!({"type": "vless", "server": "198.51.100.9", "server_port": 443,
                   "uuid": "11111111-2222-4333-8444-555555555555",
                   "encryption": "synthetic-t29-encryption",
                   "flow": "xtls-rprx-vision",
                   "network": "ws", "packetEncoding": "packet",
                   "path": "/ws", "host": "example.com"}),
            "127.0.0.1",
            19925,
        );
        let plan = validate_activation(std::slice::from_ref(&row)).expect("plans");
        assert_eq!(plan.egresses.len(), 1, "one bridge leg");
        let xray = generate_xray_config(&plan.egresses).expect("builds");
        let outbounds = xray.value()["outbounds"].clone();
        let node = outbounds
            .as_array()
            .expect("xray outbounds")
            .iter()
            .find(|node| node["tag"] == json!(xray_egress_tag("t29y")))
            .expect("xray vless egress present");
        assert_eq!(
            node["settings"]["vnext"][0]["users"][0]["flow"],
            json!("xtls-rprx-vision"),
            "flow passthrough"
        );
        assert_eq!(
            node["streamSettings"]["packetEncoding"],
            json!("packet"),
            "xray-schema naming"
        );
    }

    #[test]
    fn t29_vless_encryption_nonstring_is_typed_error() {
        let row = t29_vless_row("t29z", "VlessBad", json!(42), 19926);
        let error = validate_activation(std::slice::from_ref(&row)).expect_err("typed");
        assert!(
            matches!(error, EngineError::BadCustom { .. }),
            "typed, got {error}"
        );
        let text = error.to_string();
        assert!(text.contains("\"encryption\""), "names the field: {text}");
        assert!(text.contains("xray"), "names the sidecar: {text}");
    }

    #[test]
    fn t29_vless_real_encryption_served_by_xray_sidecar() {
        let row = t29_vless_row(
            "t29w",
            "VlessSidecar",
            json!("synthetic-t29-encryption"),
            19927,
        );
        let (mut controller, stock, _fork, xray, _extra) = EngineController::fake_quartet();
        controller.set_xray_probe(|| Ok(PathBuf::from("test-xray-bin")));
        controller
            .start_all(std::slice::from_ref(&row))
            .expect("starts via bridge");
        assert!(xray.is_running(), "xray sidecar serves the leg");
        assert!(stock.is_running(), "stock serves the bridge pointer");
        assert!(controller.is_running("t29w"), "row reports running");
        let served_xray = xray.last_config().expect("xray config");
        assert!(
            served_xray.contains("synthetic-t29-encryption"),
            "value reaches xray"
        );
        let served_stock = stock.last_config().expect("stock config");
        assert!(
            !served_stock.contains("synthetic-t29-encryption"),
            "never in stock: {served_stock}"
        );
    }

    fn t29_ansi(detail: &str) -> String {
        detail.replace("[ESC]", &char::from(27u8).to_string())
    }

    #[test]
    fn t29_strip_ansi_codes_drops_csi_keeps_text() {
        assert_eq!(strip_ansi_codes("plain"), "plain");
        assert_eq!(
            strip_ansi_codes(&t29_ansi("[ESC][31mFATAL[ESC][0m")),
            "FATAL"
        );
        assert_eq!(
            strip_ansi_codes(&t29_ansi("[ESC][31;1mboom[ESC][0m")),
            "boom"
        );
        assert_eq!(
            strip_ansi_codes(&t29_ansi("a[ESC]b")),
            "a",
            "non-CSI escape eats one char"
        );
        assert_eq!(
            strip_ansi_codes(&t29_ansi("trailing[ESC]")),
            "trailing",
            "bare tail dropped"
        );
    }

    #[test]
    fn t29_check_failed_strips_ansi_escape_codes() {
        let error = EngineError::CheckFailed {
            detail: t29_ansi("[ESC][31mFATAL[ESC][0m decode failed: boom"),
        };
        let text = error.to_string();
        assert!(text.contains("FATAL"), "text kept: {text}");
        assert!(text.contains("boom"), "text kept: {text}");
        assert!(
            !text.contains(char::from(27u8)),
            "no escape codes: {text:?}"
        );
        assert!(!text.contains("[31m"), "no SGR residue: {text}");
    }

    #[test]
    fn a7_check_failed_redacts_stderr_secrets() {
        let error = EngineError::CheckFailed {
            detail: t29_ansi(
                "[ESC][31mFATAL[ESC][0m password=hunter2 uuid 11111111-2222-4333-8444-555555555555",
            ),
        };
        let text = error.to_string();
        assert!(text.contains("FATAL"), "text kept: {text}");
        assert!(!text.contains("hunter2"), "password redacted: {text}");
        assert!(!text.contains("11111111-2222"), "uuid redacted: {text}");
        assert!(
            !text.contains(char::from(27u8)),
            "no escape codes: {text:?}"
        );
    }

    #[test]
    fn t29_controller_check_failure_has_no_ansi() {
        let row = fixture(
            "t29s",
            "AnsiRow",
            "shadowsocks",
            json!({"type": "shadowsocks", "server": "203.0.113.7",
                   "server_port": 8388, "method": "aes-128-gcm", "password": "pass1"}),
            "127.0.0.1",
            19928,
        );
        let (mut controller, stock) = EngineController::fake();
        stock.fail_check(&t29_ansi("[ESC][31;1mFATAL[ESC][0m outbounds[0]: boom"));
        let error = controller
            .start_all(std::slice::from_ref(&row))
            .expect_err("blocked");
        assert!(matches!(error, EngineError::CheckFailed { .. }));
        let text = error.to_string();
        assert!(text.contains("FATAL"), "kept: {text}");
        assert!(!text.contains(char::from(27u8)), "stripped: {text:?}");
        let last = controller.last_error().expect("recorded");
        assert!(!last.contains(char::from(27u8)), "stored clean: {last:?}");
    }

    fn t37_preset_map() -> HashMap<String, Vec<serde_json::Value>> {
        let mut map = HashMap::new();
        map.insert(
            "p1".to_string(),
            vec![
                json!({"action": "route", "domain": ["example.com"]}),
                json!({"action": "direct", "ip_cidr": ["10.0.0.0/8"]}),
                json!({"action": "block", "port": [25]}),
            ],
        );
        map
    }

    fn t37_row_with_preset() -> ProxyConfig {
        let mut row = fixture(
            "1",
            "Alpha",
            "shadowsocks",
            ss_params("pw"),
            "127.0.0.1",
            10801,
        );
        row.preset_id = Some("p1".to_string());
        row
    }

    #[test]
    fn ticket37_generate_without_presets_is_byte_identical() {
        let rows = three_standard();
        let plain = generate(&rows).expect("generates");
        let empty = HashMap::new();
        let with_empty = generate_with_presets(&rows, &empty).expect("generates");
        assert_eq!(plain.json, with_empty.json, "empty map changes nothing");
    }

    #[test]
    fn ticket37_preset_rules_merge_before_isolation_rule() {
        let row = t37_row_with_preset();
        let generated = generate_with_presets(std::slice::from_ref(&row), &t37_preset_map())
            .expect("generates");
        let value = generated.value();
        let rules = value["route"]["rules"].as_array().expect("rules array");
        assert_eq!(rules.len(), 4, "3 preset + 1 isolation, got {rules:?}");
        let in_tag = inbound_tag(&row.id);
        let out_tag = outbound_tag(&row.id);
        for (index, rule) in rules.iter().take(3).enumerate() {
            assert_eq!(
                rule["inbound"],
                json!([in_tag]),
                "preset rule {index} scoped to the row, got {rule}"
            );
        }
        assert_eq!(rules[0]["action"], json!("route"));
        assert_eq!(rules[0]["outbound"], json!(out_tag));
        assert_eq!(rules[0]["domain"], json!(["example.com"]));
        assert_eq!(rules[1]["action"], json!("direct"));
        assert_eq!(rules[2]["action"], json!("reject"), "block normalizes");

        assert_eq!(rules[3]["inbound"], json!([in_tag]));
        assert_eq!(rules[3]["outbound"], json!(out_tag));
    }

    #[test]
    fn ticket37_unattached_rows_keep_isolation_only() {
        let attached = t37_row_with_preset();
        let plain = fixture(
            "2",
            "Beta",
            "shadowsocks",
            ss_params("pw2"),
            "127.0.0.1",
            10802,
        );
        let generated =
            generate_with_presets(&[attached, plain], &t37_preset_map()).expect("generates");
        let value = generated.value();
        let rules = value["route"]["rules"].as_array().expect("rules array");
        assert_eq!(rules.len(), 5, "3 preset + 2 isolation, got {rules:?}");
        let beta_in = inbound_tag("2");
        let beta_rules: Vec<&serde_json::Value> = rules
            .iter()
            .filter(|rule| rule["inbound"] == json!([beta_in]))
            .collect();
        assert_eq!(beta_rules.len(), 1, "unattached row keeps one rule");
        assert_eq!(beta_rules[0]["action"], json!("route"));
    }

    #[test]
    fn ticket37_dangling_preset_reference_fails_loud() {
        let row = t37_row_with_preset();
        let empty = HashMap::new();
        let error = generate_with_presets(std::slice::from_ref(&row), &empty)
            .expect_err("dangling preset must fail");
        let text = error.to_string();
        assert!(text.contains("Alpha"), "names the row, got {text}");
        assert!(matches!(error, EngineError::BadPreset { .. }));
    }

    #[test]
    fn ticket37_bad_stored_rule_names_index_and_field() {
        let row = t37_row_with_preset();
        let mut map = HashMap::new();
        map.insert(
            "p1".to_string(),
            vec![json!({"action": "route"}), json!({"nope": true})],
        );
        let error = generate_with_presets(std::slice::from_ref(&row), &map)
            .expect_err("bad stored rule must fail");
        let text = error.to_string();
        assert!(text.contains("rule 1"), "names rule index, got {text}");
        assert!(text.contains("\"action\""), "names field, got {text}");
        assert!(!text.contains("nope"), "never echoes content, got {text}");
    }

    #[test]
    fn ticket37_controller_start_serves_preset_rules_first() {
        let (mut controller, stock, _, _, _) = EngineController::fake_quartet();
        controller.set_preset_rules(t37_preset_map());
        let row = t37_row_with_preset();
        controller.start_one_confirmed(&row).expect("start works");
        let served: serde_json::Value =
            serde_json::from_str(&stock.last_config().expect("config served"))
                .expect("served JSON parses");
        let rules = served["route"]["rules"].as_array().expect("rules array");
        assert_eq!(
            rules.len(),
            4,
            "preset rules before isolation, got {rules:?}"
        );
        assert_eq!(rules[0]["action"], json!("route"));
        assert_eq!(rules[0]["outbound"], json!(outbound_tag("1")));
        assert_eq!(rules[3]["outbound"], json!(outbound_tag("1")));
    }

    #[test]
    fn ticket37_controller_start_without_presets_unchanged() {
        let (mut controller, stock, _, _, _) = EngineController::fake_quartet();
        let row = fixture(
            "1",
            "Alpha",
            "shadowsocks",
            ss_params("pw"),
            "127.0.0.1",
            10801,
        );
        controller.start_one_confirmed(&row).expect("start works");
        let served: serde_json::Value =
            serde_json::from_str(&stock.last_config().expect("config served"))
                .expect("served JSON parses");
        let rules = served["route"]["rules"].as_array().expect("rules array");
        assert_eq!(rules.len(), 1, "isolation only, got {rules:?}");
    }

    #[test]
    fn ticket37_live_singbox_check_accepts_preset_rules() {
        if std::env::var("MANYPROXY_LIVE_CHECK").unwrap_or_default() != "1" {
            return;
        }
        let binary = std::env::var("MANYPROXY_SINGBOX_BIN").map_or_else(
            |_| {
                if cfg!(windows) {
                    PathBuf::from("target/debug/sing-box.exe")
                } else {
                    PathBuf::from("target/debug/sing-box")
                }
            },
            PathBuf::from,
        );
        let row = t37_row_with_preset();
        let generated = generate_with_presets(std::slice::from_ref(&row), &t37_preset_map())
            .expect("PROBE preset: generates");
        let value = generated.value();
        let rules = value["route"]["rules"].as_array().expect("rules array");
        assert_eq!(rules.len(), 4, "preset rules before isolation");
        let path = std::env::temp_dir().join(format!(
            "manyproxy-t37-live-preset-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::write(&path, &generated.json).expect("temp config writes");
        let output = std::process::Command::new(&binary)
            .arg("check")
            .arg("-c")
            .arg(&path)
            .output()
            .expect("check runs");
        let _ = std::fs::remove_file(&path);
        assert!(
            output.status.success(),
            "PROBE preset: sing-box check EXIT 0, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn t41_reserved_ports_rejected_at_save_and_start() {
        for port in [CLASH_PORT_START, 18755, CLASH_PORT_END] {
            let error = validate_listen_for_save("Row", "127.0.0.1", port)
                .expect_err("clash range rejected at save");
            assert!(
                matches!(error, EngineError::ReservedPort { .. }),
                "typed, got {error}"
            );
            assert!(
                error
                    .to_string()
                    .contains("clash-controller range 18501-18999"),
                "names the range: {error}"
            );
        }
        for port in [XRAY_BRIDGE_PORT_START, 11850, XRAY_BRIDGE_PORT_END] {
            let error = validate_listen_for_save("Row", "127.0.0.1", port)
                .expect_err("bridge range rejected at save");
            assert!(matches!(error, EngineError::ReservedPort { .. }));
            assert!(
                error.to_string().contains("xray-bridge range 11801-11999"),
                "names the range: {error}"
            );
        }

        for port in [0, 10801, 10999, 18081, 10921] {
            validate_listen_for_save("Row", "127.0.0.1", port).expect("save allows it");
        }

        let (mut controller, stock) = EngineController::fake();
        let bad = fixture(
            "9",
            "ClashSquatter",
            "shadowsocks",
            ss_params("pass1"),
            "127.0.0.1",
            18501,
        );
        let error = controller
            .start_one(&bad)
            .expect_err("reserved port fails at start");
        assert!(matches!(error, EngineError::ReservedPort { .. }));
        assert!(!stock.is_running(), "nothing spawned");
        assert_eq!(controller.running_count(), 0);
    }

    #[test]
    fn t41_bad_bind_port_zero_privileged_fail_before_spawn() {
        let (mut controller, stock) = EngineController::fake();
        let garbage = fixture(
            "9",
            "GarbageBind",
            "shadowsocks",
            ss_params("pass1"),
            "999.999.0.1",
            10809,
        );
        let error = controller
            .start_one(&garbage)
            .expect_err("garbage bind fails");
        assert!(matches!(error, EngineError::BadBind { .. }), "got {error}");
        let zero = fixture(
            "9",
            "ZeroPort",
            "shadowsocks",
            ss_params("pass1"),
            "127.0.0.1",
            0,
        );
        let error = controller.start_one(&zero).expect_err("port 0 fails");
        assert!(matches!(error, EngineError::BadPort { .. }), "got {error}");
        let privy = fixture(
            "9",
            "Privileged",
            "shadowsocks",
            ss_params("pass1"),
            "127.0.0.1",
            443,
        );
        let error = controller
            .start_one(&privy)
            .expect_err("privileged port fails with guidance");
        assert!(matches!(error, EngineError::BadPort { .. }), "got {error}");
        assert!(error.to_string().contains("10801-10999"), "guides: {error}");
        assert!(!stock.is_running(), "nothing spawned");

        validate_listen_for_start("Row", "", 10801).expect("blank bind is loopback");
        validate_listen_for_start("Row", "localhost", 10801).expect("localhost ok");
    }

    #[test]
    fn t41_startup_reaper_kills_only_matching() {
        assert!(should_reap_process("sing-box.exe", 10801, &[10801]));
        assert!(should_reap_process("SING-BOX-AWG.EXE", 10801, &[10801]));
        assert!(should_reap_process("xray.exe", 11850, &[11850]));
        assert!(should_reap_process(
            "C:\\Apps\\ManyProxy\\sing-box.exe",
            10801,
            &[10801]
        ));

        assert!(!should_reap_process("chrome.exe", 10801, &[10801]));
        assert!(!should_reap_process("sing-box.exe", 9999, &[10801]));
        assert!(!should_reap_process("my-sing-box.exe", 10801, &[10801]));
        assert!(!should_reap_process("", 10801, &[10801]));

        let netstat = "\
Active Connections\r
\r
  Proto  Local Address          Foreign Address        State           PID\r
  TCP    127.0.0.1:10801        0.0.0.0:0              LISTENING       111\r
  TCP    127.0.0.1:10801        0.0.0.0:0              LISTENING       222\r
  TCP    127.0.0.1:9999         0.0.0.0:0              LISTENING       333\r
  UDP    127.0.0.1:10801        *:*                                    444\r
";
        let image_of = |pid: u32| match pid {
            111 => Some("sing-box.exe".to_string()),
            222 => Some("chrome.exe".to_string()),
            333 => Some("sing-box.exe".to_string()),
            _ => None,
        };
        let reaped = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let reaped_in = reaped.clone();
        let kill = move |pid: u32| {
            reaped_in.borrow_mut().push(pid);
            true
        };
        let dir = std::env::temp_dir().join(format!(
            "manyproxy-t41-reap-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        std::fs::write(dir.join("manyproxy-singbox-111-0.json"), "{}").expect("stale temp");
        std::fs::write(dir.join("notes.txt"), "keep").expect("other file");
        std::fs::write(dir.join("manyproxy-old.txt"), "keep").expect("wrong suffix kept");
        let report = startup_reap_and_sweep(&[10801], &dir, netstat, &image_of, &kill);
        assert_eq!(report.reaped_pids, vec![111], "only ours: {report:?}");
        assert!(report.errors.is_empty(), "{report:?}");
        assert_eq!(report.swept_files.len(), 1, "{report:?}");
        assert!(!dir.join("manyproxy-singbox-111-0.json").exists(), "swept");
        assert!(dir.join("notes.txt").exists(), "foreign file kept");
        assert!(dir.join("manyproxy-old.txt").exists(), "wrong suffix kept");
        assert_eq!(*reaped.borrow(), vec![111]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn t41_netstat_tasklist_parsers() {
        let owners = parse_netstat_tcp_owners(
            "Active Connections\r\n  TCP    127.0.0.1:18501   0.0.0.0:0  LISTENING  42\r\n\
             garbage line\r\n  UDP    127.0.0.1:53  *:*  7\r\n",
        );
        assert_eq!(owners, vec![(18501, 42)]);
        let table = parse_tasklist_table(
            "Image Name                     PID Session#      Mem Usage\r\n\
             sing-box.exe                 42 Console          5,012 K\r\n\
             chrome.exe                  777 Console        250,000 K\r\n",
        );
        assert_eq!(
            table,
            vec![
                ("sing-box.exe".to_string(), 42),
                ("chrome.exe".to_string(), 777)
            ]
        );
        assert!(parse_netstat_tcp_owners("").is_empty());
        assert!(parse_tasklist_table("").is_empty());
    }

    #[test]
    fn t41_stale_temp_sweep_only_ours_sorted() {
        let missing = std::env::temp_dir().join(format!(
            "manyproxy-t41-sweep-missing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&missing);
        assert!(stale_temp_files(&missing).is_empty());
        assert!(sweep_stale_temp_files(&missing).is_empty());

        let dir = std::env::temp_dir().join(format!(
            "manyproxy-t41-sweep-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        std::fs::write(dir.join("manyproxy-b.json"), "{}").expect("stale temp");
        std::fs::write(dir.join("manyproxy-a.json"), "{}").expect("stale temp");
        std::fs::write(dir.join("manyproxy-old.txt"), "keep").expect("wrong suffix kept");
        std::fs::write(dir.join("notes.txt"), "keep").expect("foreign file kept");
        std::fs::write(dir.join("manyproxy-x.json.bak"), "keep").expect("wrong suffix kept");
        std::fs::create_dir_all(dir.join("manyproxy-nested.json")).expect("nested dir kept");
        assert_eq!(
            stale_temp_files(&dir),
            vec![dir.join("manyproxy-a.json"), dir.join("manyproxy-b.json")]
        );
        assert_eq!(
            sweep_stale_temp_files(&dir),
            vec![dir.join("manyproxy-a.json"), dir.join("manyproxy-b.json")]
        );
        assert!(!dir.join("manyproxy-a.json").exists(), "swept");
        assert!(!dir.join("manyproxy-b.json").exists(), "swept");
        assert!(dir.join("notes.txt").exists(), "foreign file kept");
        assert!(dir.join("manyproxy-old.txt").exists(), "wrong suffix kept");
        assert!(
            dir.join("manyproxy-x.json.bak").exists(),
            "wrong suffix kept"
        );
        assert!(
            dir.join("manyproxy-nested.json").is_dir(),
            "directories never swept"
        );
        assert!(stale_temp_files(&dir).is_empty());
        assert!(sweep_stale_temp_files(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn t41_crash_drops_served_clash_marks_stale_and_restart_recovers() {
        let (mut controller, _stock, fork) = EngineController::fake_pair();
        controller.set_fork_probe(|| Ok(PathBuf::from("test-fork-bin")));
        let rows = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            ),
            awg_fixture("7", "AwgOne", awg_params("2.0", json!({"jc": 2})), 10807),
        ];
        controller.start_all(&rows).expect("mixed set starts");
        assert_eq!(controller.clash_targets().len(), 2, "both cores tracked");
        assert!(!controller.totals_stale());

        fork.inject_crash("signal: segmentation fault");
        let crashes = controller.poll_shards();
        assert_eq!(crashes.len(), 1);
        assert_eq!(crashes[0].0, "fork");
        assert_eq!(controller.crashed_shards(), vec!["fork".to_string()]);
        assert!(controller.is_shard_crashed("fork"));
        assert!(!controller.is_shard_crashed("stock"));
        assert!(controller.totals_stale(), "totals stale after crash");
        controller.clear_totals_stale();
        assert!(!controller.totals_stale(), "marker clears");
        assert_eq!(controller.clash_targets().len(), 1, "dead core dropped");
        assert_eq!(controller.clash_targets()[0].core, "stock");
        assert_eq!(controller.live_clash_targets().len(), 1);
        assert_eq!(controller.ids_on_crashed_shards(), vec!["7".to_string()]);
        assert!(matches!(controller.status(), EngineStatus::Crashed { .. }));

        controller
            .restart_crashed()
            .expect("manual Restart recovers");
        assert!(controller.crashed_shards().is_empty());
        assert!(!controller.totals_stale());
        assert_eq!(controller.clash_targets().len(), 2, "both cores back");
        assert_eq!(controller.restart_attempts(), 0);
        assert!(matches!(controller.status(), EngineStatus::Running));
    }

    #[test]
    fn t41_restart_idle_is_noop() {
        let (mut controller, stock) = EngineController::fake();
        controller.start_all(&three_standard()).expect("starts");
        let checks = stock.check_calls();
        controller.restart_crashed().expect("idle restart ok");
        assert_eq!(stock.check_calls(), checks, "no re-apply when idle");
    }

    #[test]
    fn t41_manual_reapply_clears_crash_state() {
        let (mut controller, stock) = EngineController::fake();
        let rows = three_standard();
        controller.start_all(&rows).expect("starts");
        stock.inject_crash("boom");
        assert!(!controller.poll_shards().is_empty());
        assert_eq!(controller.crashed_shards(), vec!["stock".to_string()]);
        assert!(controller.pending_crash().is_some(), "cause retained");

        controller.start_one(&rows[0]).expect("manual re-apply");
        assert!(
            controller.crashed_shards().is_empty(),
            "manual apply clears crash state"
        );
        assert!(controller.pending_crash().is_none(), "cause spent");

        stock.inject_crash("boom again");
        assert!(!controller.poll_shards().is_empty());
        controller.stop_one(&rows[0].id);
        assert!(
            controller.crashed_shards().is_empty(),
            "stopping the crashed rows clears crash state"
        );
    }

    #[test]
    fn t41_auto_restart_backoff_bounds() {
        use std::time::{Duration, Instant};
        assert_eq!(auto_restart_backoff_secs(5, 0), 5);
        assert_eq!(auto_restart_backoff_secs(5, 1), 10);
        assert_eq!(auto_restart_backoff_secs(5, 2), 20);
        assert_eq!(auto_restart_backoff_secs(5, 10), 300, "capped");

        let (mut controller, stock) = EngineController::fake();
        controller.start_all(&three_standard()).expect("starts");
        stock.inject_crash("boom");
        assert!(!controller.poll_shards().is_empty());
        let now = Instant::now();

        assert!(
            controller.auto_restart().enabled,
            "auto-restart is on by default"
        );
        assert!(controller.auto_restart_due(now), "first attempt immediate");

        controller.set_auto_restart(AutoRestartConfig {
            enabled: true,
            max_retries: 2,
            base_backoff_secs: 5,
        });
        controller.note_auto_restart_attempt(now);
        assert_eq!(controller.restart_attempts(), 1);
        assert!(!controller.auto_restart_due(now + Duration::from_secs(9)));
        assert!(controller.auto_restart_due(now + Duration::from_secs(10)));
        controller.note_auto_restart_attempt(now + Duration::from_secs(10));
        assert!(
            !controller.auto_restart_due(now + Duration::from_secs(10_000)),
            "retry bound holds"
        );
        controller.set_auto_restart(AutoRestartConfig {
            enabled: false,
            max_retries: 3,
            base_backoff_secs: 5,
        });
        assert!(!controller.auto_restart().enabled);
        assert_eq!(controller.restart_attempts(), 0, "disable resets");
        assert!(!controller.auto_restart_due(now), "disabled: nothing due");
        controller.note_restart_success();
        assert_eq!(controller.restart_attempts(), 0);
    }

    #[test]
    fn t41_auto_restart_budget_bounds_persistent_crash_loop() {
        use std::time::{Duration, Instant};
        let (mut controller, stock) = EngineController::fake();
        controller.start_all(&three_standard()).expect("starts");
        controller.set_auto_restart(AutoRestartConfig {
            enabled: true,
            max_retries: 2,
            base_backoff_secs: 5,
        });
        let mut now = Instant::now();
        stock.inject_crash("boom");
        assert!(!controller.poll_shards().is_empty());
        stock.fail_check("still failing to start");

        assert!(controller.auto_restart_due(now), "first attempt immediate");
        controller.note_auto_restart_attempt(now);
        assert!(controller.restart_crashed().is_err(), "spawn fails");
        assert_eq!(
            controller.restart_attempts(),
            1,
            "a failed spawn keeps the attempt"
        );

        assert!(
            !controller.auto_restart_due(now + Duration::from_secs(9)),
            "backoff holds after a failed spawn"
        );
        now += Duration::from_secs(10);
        assert!(
            controller.auto_restart_due(now),
            "second attempt after backoff"
        );
        controller.note_auto_restart_attempt(now);
        assert!(controller.restart_crashed().is_err(), "spawn still fails");
        assert_eq!(controller.restart_attempts(), 2);

        assert!(
            !controller.auto_restart_due(now + Duration::from_secs(100_000)),
            "retry budget is exhausted, no further auto attempts"
        );
        assert!(
            controller.pending_crash().is_some(),
            "the crash notification path still holds the cause"
        );

        stock.clear_failures();
        controller
            .restart_crashed()
            .expect("a manual restart still recovers");
        assert!(
            controller.crashed_shards().is_empty(),
            "manual recovery clears crash state"
        );
    }

    #[test]
    fn t41_auto_restart_budget_resets_only_after_steady_up() {
        use std::time::{Duration, Instant};
        let (mut controller, stock) = EngineController::fake();
        controller.start_all(&three_standard()).expect("starts");
        controller.set_auto_restart(AutoRestartConfig {
            enabled: true,
            max_retries: 3,
            base_backoff_secs: 5,
        });
        let now = Instant::now();
        stock.inject_crash("boom");
        assert!(!controller.poll_shards().is_empty());
        controller.note_auto_restart_attempt(now);
        controller.restart_crashed().expect("spawn ok");
        assert_eq!(
            controller.restart_attempts(),
            1,
            "a successful spawn alone does not reset the budget"
        );

        controller.settle_restart_budget(now + Duration::from_secs(4));
        assert_eq!(
            controller.restart_attempts(),
            1,
            "below the steady-up threshold: budget held"
        );
        controller.settle_restart_budget(now + Duration::from_secs(5));
        assert_eq!(
            controller.restart_attempts(),
            0,
            "steady up resets the budget"
        );

        stock.inject_crash("boom again");
        assert!(!controller.poll_shards().is_empty());
        assert!(
            controller.auto_restart_due(now + Duration::from_secs(6)),
            "a fresh budget restarts immediately"
        );
    }

    #[test]
    fn t41_experimental_merge_preserves_keys() {
        let doc = serde_json::json!({
            "inbounds": [], "outbounds": [],
            "experimental": {"telemetry": {"enabled": true}},
        });
        let out = inject_clash_api(&doc.to_string(), 18501, "s3cret").expect("injects");
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("stays JSON");
        assert_eq!(
            parsed["experimental"]["telemetry"],
            serde_json::json!({"enabled": true}),
            "existing keys survive"
        );
        assert_eq!(
            parsed["experimental"]["clash_api"]["external_controller"],
            serde_json::json!("127.0.0.1:18501")
        );

        let bare = serde_json::json!({"inbounds": [], "outbounds": []});
        let out = inject_clash_api(&bare.to_string(), 18501, "s3cret").expect("injects");
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("JSON");
        assert_eq!(
            parsed["experimental"]["clash_api"]["secret"],
            serde_json::json!("s3cret")
        );
    }

    #[test]
    fn t41_clash_secret_is_128bit_unique() {
        for core in ["stock", "fork"] {
            let secret = clash_secret(core);
            assert_eq!(secret.len(), 32, "128-bit hex: {secret}");
            assert!(
                secret.bytes().all(|byte| byte.is_ascii_hexdigit()),
                "hex only: {secret}"
            );
        }
        assert_ne!(clash_secret("stock"), clash_secret("stock"), "unique");
    }

    #[test]
    fn t41_clash_reservation_holds_ports() {
        // Hold the shared reservation lock for the whole body: a parallel test's
        // config generation reserves from this same window and would otherwise
        // take `first` back between the drop and the re-bind.
        let _port_window = port_window_lock();
        let mut holds = Vec::new();
        let first = reserve_clash_port(&mut holds).expect("range has room");
        assert!((CLASH_PORT_START..=CLASH_PORT_END).contains(&first));
        let second = reserve_clash_port(&mut holds).expect("second pick");
        assert_ne!(first, second, "no double reservation");
        assert!(
            std::net::TcpListener::bind(format!("127.0.0.1:{first}")).is_err(),
            "held port reads busy"
        );
        drop(holds);
        assert!(
            std::net::TcpListener::bind(format!("127.0.0.1:{first}")).is_ok(),
            "release frees the port"
        );
    }

    #[test]
    fn t41_stop_failure_surfaced_not_silent() {
        let (mut controller, stock) = EngineController::fake();
        let rows = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            ),
            fixture("2", "Beta", "vless", vless_params(), "127.0.0.1", 10802),
        ];
        controller.start_all(&rows).expect("starts");
        let (removed, missing) = controller.stop_one_result("missing");
        assert!(!removed);
        assert!(missing.is_none());
        stock.fail_check("re-gen broke");
        let (removed, error) = controller.stop_one_result("2");
        assert!(removed, "membership answers first");
        let error = error.expect("apply failure surfaces");
        assert!(
            matches!(error, EngineError::CheckFailed { .. }),
            "got {error}"
        );
        assert_eq!(controller.running_count(), 2, "snapshot restored");
        assert!(controller.last_error().is_some());
        stock.clear_failures();
        let (removed, error) = controller.stop_one_result("2");
        assert!(removed);
        assert!(error.is_none());
        assert_eq!(controller.running_count(), 1);
        let generations = controller.generation();
        assert!(generations >= 3, "fence moved on every mutation");
        assert!(controller.stop_all_result().is_none());
        assert_eq!(controller.running_count(), 0);
        assert!(
            controller.clash_targets().is_empty(),
            "secrets cleared on stop"
        );
    }

    #[test]
    fn t41_start_all_subset_single_apply() {
        let (mut controller, stock) = EngineController::fake();
        let mut rows = three_standard();
        rows.push(fixture(
            "9",
            "Broken",
            "vless",
            json!({}),
            "127.0.0.1",
            10809,
        ));
        let outcome = controller.start_all_subset(&rows, true);
        assert!(outcome.error.is_none(), "no union abort: {outcome:?}");
        assert_eq!(outcome.applied, vec!["1", "2", "3"]);
        assert_eq!(outcome.failed.len(), 1);
        assert_eq!(outcome.failed[0].0, "Broken");
        assert_eq!(stock.check_calls(), 1, "single apply, no per-row flap");
        assert_eq!(controller.running_count(), 3);

        let mut controller = EngineController::fake().0;
        let dupe = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            ),
            fixture("2", "Beta", "vless", vless_params(), "127.0.0.1", 10801),
        ];
        let outcome = controller.start_all_subset(&dupe, true);
        assert!(
            outcome.applied.is_empty(),
            "conflict stays atomic: {outcome:?}"
        );
        assert!(outcome.error.is_some(), "union abort named: {outcome:?}");
        assert_eq!(outcome.failed.len(), 2, "every survivor named: {outcome:?}");
        assert_eq!(controller.running_count(), 0);
    }

    #[test]
    fn t41_dns_failure_typed_before_spawn() {
        let hostile = fixture(
            "9",
            "BadHost",
            "shadowsocks",
            json!({"type": "shadowsocks", "server": "no-such-host.invalid",
                   "server_port": 8388, "method": "aes-128-gcm", "password": "x"}),
            "127.0.0.1",
            10809,
        );
        let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let calls_in = calls.clone();
        let resolver = move |server: &str| {
            calls_in.borrow_mut().push(server.to_string());
            if server == "no-such-host.invalid" {
                return Err("nxdomain".to_string());
            }
            Ok(())
        };
        let error = validate_egress_resolution_with(std::slice::from_ref(&hostile), &resolver)
            .expect_err("bad hostname fails");
        assert!(
            matches!(error, EngineError::DnsFailure { .. }),
            "typed, got {error}"
        );
        let text = error.to_string();
        assert!(text.contains("\"BadHost\""), "names row: {text}");
        assert!(
            text.contains("no-such-host.invalid"),
            "names server: {text}"
        );
        assert!(text.contains("DNS failure"), "kind named: {text}");

        calls.borrow_mut().clear();
        validate_egress_resolution_with(&three_standard(), &resolver).expect("IPs pass");
        assert!(calls.borrow().is_empty(), "no lookup for IPs");

        let (mut controller, stock) = EngineController::fake();
        let error = controller
            .start_one(&hostile)
            .expect_err("apply gate refuses");
        assert!(
            matches!(error, EngineError::DnsFailure { .. }),
            "got {error}"
        );
        assert!(!stock.is_running());
    }

    #[test]
    fn t41_dns_gate_skips_cidr_interface_address() {
        let wg = fixture(
            "10",
            "WG",
            "amneziawg",
            json!({"type": "wireguard", "server": "203.0.113.7",
                   "server_port": 51820, "address": "100.64.9.141/32"}),
            "127.0.0.1",
            10810,
        );
        let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let calls_in = calls.clone();
        let resolver = move |server: &str| {
            calls_in.borrow_mut().push(server.to_string());
            Err("must not be consulted".to_string())
        };
        validate_egress_resolution_with(std::slice::from_ref(&wg), &resolver)
            .expect("CIDR address + literal server pass with no lookup");
        assert!(calls.borrow().is_empty(), "no lookup at all");

        assert!(
            resolve_egress_host("10.0.0.2/32", &resolver).is_ok(),
            "CIDR literal passes"
        );
    }

    #[test]
    fn t41_row_local_split_covers_new_variants() {
        for error in [
            EngineError::BadBind {
                name: "R".to_string(),
                bind: "bogus".to_string(),
            },
            EngineError::BadPort {
                name: "R".to_string(),
                detail: "port 0".to_string(),
            },
            EngineError::ReservedPort {
                name: "R".to_string(),
                port: 18501,
                range: CLASH_RESERVED_NAME,
            },
            EngineError::DnsFailure {
                name: "R".to_string(),
                server: "bad.invalid".to_string(),
            },
            EngineError::BadExtra {
                name: "R".to_string(),
                field: "exe".to_string(),
                detail: "missing".to_string(),
            },
        ] {
            assert!(is_row_local_error(&error), "row-local: {error}");
        }
        for error in [
            EngineError::PortConflict {
                bind: "127.0.0.1".to_string(),
                port: 10801,
                first: "A".to_string(),
                second: "B".to_string(),
            },
            EngineError::MissingBinary {
                binary: engine_binary_file_name(),
                searched: "dir".to_string(),
            },
            EngineError::LanConfirmRequired {
                names: "A".to_string(),
            },
        ] {
            assert!(!is_row_local_error(&error), "union-level: {error}");
        }
    }

    #[test]
    fn t41_typo_hints_name_likely_key() {
        assert_eq!(suggest_param_key("securty"), Some("security"));
        assert_eq!(suggest_param_key("server"), None, "exact keys quiet");
        assert_eq!(suggest_param_key("up"), None, "short keys quiet");
        assert_eq!(suggest_param_key("fingerprint"), None);
        let mut params = vless_params();
        params
            .as_object_mut()
            .expect("object")
            .insert("securty".to_string(), json!("tls"));
        let hints = typo_hints_for("Typo", "vless", params.as_object().expect("object"));
        assert_eq!(hints.len(), 1, "{hints:?}");
        assert!(hints[0].contains("securty"), "{hints:?}");
        assert!(hints[0].contains("security"), "{hints:?}");

        let custom = json!({"type": "vless", "securty": "tls"})
            .as_object()
            .expect("object")
            .clone();
        assert!(typo_hints_for("C", "custom-outbound", &custom).is_empty());

        let row = fixture("1", "Typo", "vless", params, "127.0.0.1", 10801);
        let plan = validate_activation(std::slice::from_ref(&row)).expect("warns, not fails");
        assert_eq!(plan.warnings.len(), 1, "{:?}", plan.warnings);
    }

    #[test]
    fn t02_mux_emission_shape_on_four_protocols() {
        fn muxed(protocol: &str, params: serde_json::Value) -> ProxyConfig {
            let mut params = params;
            let obj = params.as_object_mut().expect("object");
            obj.insert("mux".to_string(), json!("on"));
            obj.insert("brutal_enable".to_string(), json!(true));
            obj.insert("brutal_up_mbps".to_string(), json!(100));
            obj.insert("brutal_down_mbps".to_string(), json!(50));
            fixture("9", "Muxed", protocol, params, "127.0.0.1", 10809)
        }
        for (protocol, params) in [
            ("vless", vless_params()),
            (
                "trojan",
                json!({"type": "trojan", "server": "198.51.100.9", "server_port": 443,
                       "password": "secret"}),
            ),
            (
                "vmess",
                json!({"type": "vmess", "server": "198.51.100.9", "server_port": 443,
                       "uuid": "11111111-2222-4333-8444-555555555555"}),
            ),
            ("shadowsocks", ss_params("pass1")),
        ] {
            let row = muxed(protocol, params);
            let stored = row.params.clone();
            let generated = generate(std::slice::from_ref(&row)).expect("mux row generates");
            let value = generated.value();
            let node = value["outbounds"]
                .as_array()
                .expect("outbounds")
                .iter()
                .find(|node| node["type"] == json!(protocol))
                .unwrap_or_else(|| panic!("{protocol} egress present"));
            assert_eq!(
                node["multiplex"],
                json!({"enabled": true}),
                "{protocol} emits the 1.14.2 shape"
            );
            for leaked in ["mux", "brutal_enable", "brutal_up_mbps", "brutal_down_mbps"] {
                assert!(
                    node.get(leaked).is_none(),
                    "{protocol} emission leaks no {leaked}: {node}"
                );
            }
            assert!(
                stored.contains("brutal_up_mbps"),
                "stored JSON preserves brutal speeds"
            );
        }
    }

    #[test]
    fn t02_mux_default_omits_off_disables_garbage_fails() {
        let plain = fixture("9", "Plain", "vless", vless_params(), "127.0.0.1", 10809);
        let generated = generate(std::slice::from_ref(&plain)).expect("plain generates");
        let value = generated.value();
        let node = value["outbounds"]
            .as_array()
            .expect("outbounds")
            .iter()
            .find(|node| node["type"] == json!("vless"))
            .expect("vless egress present");
        assert!(
            node.get("multiplex").is_none(),
            "default emits no multiplex key"
        );
        let mut off_params = vless_params();
        off_params
            .as_object_mut()
            .expect("object")
            .insert("mux".to_string(), json!("off"));
        let off = fixture("9", "Off", "vless", off_params, "127.0.0.1", 10809);
        let generated = generate(std::slice::from_ref(&off)).expect("off generates");
        let value = generated.value();
        let node = value["outbounds"]
            .as_array()
            .expect("outbounds")
            .iter()
            .find(|node| node["type"] == json!("vless"))
            .expect("vless egress present");
        assert_eq!(node["multiplex"], json!({"enabled": false}));
        let mut junk_params = vless_params();
        junk_params
            .as_object_mut()
            .expect("object")
            .insert("mux".to_string(), json!("sometimes"));
        let junk = fixture("9", "Junk", "vless", junk_params, "127.0.0.1", 10809);
        let error = generate(std::slice::from_ref(&junk)).expect_err("garbage mux fails");
        let text = error.to_string();
        assert!(text.contains("mux"), "names the field, got {text}");
    }

    #[test]
    fn t02_mux_flats_dropped_on_non_mux_protocol() {
        let mut params = json!({"type": "socks", "server": "198.51.100.9", "server_port": 1080});
        let obj = params.as_object_mut().expect("object");
        obj.insert("mux".to_string(), json!("on"));
        obj.insert("brutal_enable".to_string(), json!(true));
        let row = fixture("9", "SocksJunk", "socks", params, "127.0.0.1", 10809);
        let generated = generate(std::slice::from_ref(&row)).expect("socks junk generates");
        let value = generated.value();
        let node = value["outbounds"]
            .as_array()
            .expect("outbounds")
            .iter()
            .find(|node| node["type"] == json!("socks"))
            .expect("socks egress present");
        assert!(node.get("multiplex").is_none(), "no mux arm on socks");
        assert!(node.get("mux").is_none(), "mux flat dropped: {node}");
        assert!(
            node.get("brutal_enable").is_none(),
            "brutal flat dropped: {node}"
        );
    }

    #[test]
    fn t68_params_declare_mux_matches_engine_logic() {
        // Divergence-proofing: `params_declare_mux` and the engine's own
        // params-key mux logic (`apply_mux` via `generate`) must agree on the
        // same fixtures the t02 multiplex emission tests pin — `mux:"on"` on
        // the four mux-capable protocol shapes, then the default / off /
        // junk vless controls. One shared rule; drift is a bug because
        // health.rs probes exactly what this function claims is muxed.
        fn engine_multiplex(
            protocol: &str,
            params: serde_json::Value,
        ) -> Option<serde_json::Value> {
            let row = fixture("9", "Muxed", protocol, params, "127.0.0.1", 10809);
            let generated = generate(std::slice::from_ref(&row)).expect("params generate");
            let value = generated.value();
            let node = value["outbounds"]
                .as_array()
                .expect("outbounds")
                .iter()
                .find(|node| node["type"] == json!(protocol))
                .unwrap_or_else(|| panic!("{protocol} egress present"));
            node.get("multiplex").cloned()
        }
        let on = |mut params: serde_json::Value| {
            let obj = params.as_object_mut().expect("object");
            obj.insert("mux".to_string(), json!("on"));
            obj.insert("brutal_enable".to_string(), json!(true));
            obj.insert("brutal_up_mbps".to_string(), json!(100));
            obj.insert("brutal_down_mbps".to_string(), json!(50));
            params
        };
        for (protocol, base) in [
            ("vless", vless_params()),
            (
                "trojan",
                json!({"type": "trojan", "server": "198.51.100.9", "server_port": 443,
                       "password": "secret"}),
            ),
            (
                "vmess",
                json!({"type": "vmess", "server": "198.51.100.9", "server_port": 443,
                       "uuid": "11111111-2222-4333-8444-555555555555"}),
            ),
            ("shadowsocks", ss_params("pass1")),
        ] {
            let params = on(base);
            assert!(
                params_declare_mux(&params.to_string()),
                "{protocol} declares the mux:\"on\" shape"
            );
            assert_eq!(
                engine_multiplex(protocol, params),
                Some(json!({"enabled": true})),
                "{protocol} engine parity on mux:\"on\""
            );
        }

        for spelling in ["on", "  on  ", "ON", "On"] {
            let mut params = vless_params();
            params
                .as_object_mut()
                .expect("object")
                .insert("mux".to_string(), json!(spelling));
            assert!(
                params_declare_mux(&params.to_string()),
                "{spelling:?} declares mux like the engine's trimmed compare"
            );
            assert_eq!(
                engine_multiplex("vless", params),
                Some(json!({"enabled": true})),
                "engine emits enabled for {spelling:?}"
            );
        }
        for spelling in ["off", "OFF"] {
            let mut params = vless_params();
            params
                .as_object_mut()
                .expect("object")
                .insert("mux".to_string(), json!(spelling));
            assert!(
                !params_declare_mux(&params.to_string()),
                "{spelling:?} says nothing to warm: the row multiplexes off"
            );
            assert_eq!(
                engine_multiplex("vless", params),
                Some(json!({"enabled": false})),
                "engine emits disabled for {spelling:?}"
            );
        }
        let mut blank = vless_params();
        blank
            .as_object_mut()
            .expect("object")
            .insert("mux".to_string(), json!("   "));
        assert!(
            !params_declare_mux(&blank.to_string()),
            "blank declares nothing, like the engine's empty default"
        );
        assert_eq!(
            engine_multiplex("vless", blank),
            None,
            "engine emits no multiplex key for blank"
        );
        assert!(
            !params_declare_mux(&vless_params().to_string()),
            "the default shape declares nothing"
        );

        // Junk `mux` values fail config generation and declare nothing here.
        let mut junk = vless_params();
        junk.as_object_mut()
            .expect("object")
            .insert("mux".to_string(), json!("sometimes"));
        assert!(
            !params_declare_mux(&junk.to_string()),
            "junk never counts as muxed"
        );
        let junk_row = fixture("9", "Junk", "vless", junk, "127.0.0.1", 10809);
        assert!(
            generate(std::slice::from_ref(&junk_row)).is_err(),
            "engine rejects the junk spelling outright"
        );

        // Non-string `mux` values are dropped exactly like `take_flat` does.
        let mut not_string = vless_params();
        not_string
            .as_object_mut()
            .expect("object")
            .insert("mux".to_string(), json!(true));
        assert!(
            !params_declare_mux(&not_string.to_string()),
            "a bool mux value declares nothing"
        );
        assert_eq!(
            engine_multiplex("vless", not_string),
            None,
            "engine drops the non-string mux key"
        );

        // Raw params the engine's params_object cannot parse never declare.
        for raw in ["", "   ", "null", "[]", "{{{"] {
            assert!(
                !params_declare_mux(raw),
                "unparseable params never declare mux: {raw:?}"
            );
        }
    }

    #[test]
    fn review_xray_flat_unknown_keys_reject_loud() {
        let mut ph2 = xray_params();
        ph2.as_object_mut()
            .expect("object")
            .insert("xPaddingBytes".to_string(), json!("100-200"));
        let rows = vec![fixture("2", "XrayPh2", "xray", ph2, "127.0.0.1", 10802)];
        let plan = validate_activation(&rows).expect("ph2 set plans");
        let error = generate_xray_config(&plan.egresses).expect_err("Ph2 key fails typed");
        let text = error.to_string();
        assert!(
            text.contains("xPaddingBytes"),
            "names the field, got {text}"
        );

        let mut aliases = xray_params();
        let obj = aliases.as_object_mut().expect("object");
        obj.remove("fp");
        obj.insert("fingerprint".to_string(), json!("firefox"));
        obj.insert("service_name".to_string(), json!("svc"));
        let rows = vec![fixture(
            "2",
            "XrayAlias",
            "xray",
            aliases,
            "127.0.0.1",
            10802,
        )];
        let plan = validate_activation(&rows).expect("alias set plans");
        let sidecar = generate_xray_config(&plan.egresses).expect("alias row generates");
        let value = sidecar.value();
        let stream = &value["outbounds"]
            .as_array()
            .expect("xray outbounds")
            .iter()
            .find(|node| node["tag"] == json!(xray_egress_tag("2")))
            .expect("xray egress present")["streamSettings"];
        assert_eq!(
            stream["fp"],
            json!("firefox"),
            "fingerprint falls back to fp"
        );
        assert_eq!(
            stream["serviceName"],
            json!("svc"),
            "service_name lands as serviceName"
        );
    }

    #[test]
    fn review_transport_headers_reject_loud() {
        let mut params = vless_params();
        params
            .as_object_mut()
            .expect("object")
            .insert("headers".to_string(), json!({"X-Custom": "1"}));
        let row = fixture("9", "HeadersMap", "vless", params, "127.0.0.1", 10809);
        let error = generate(std::slice::from_ref(&row)).expect_err("custom headers fail");
        assert!(
            error.to_string().contains("headers"),
            "names the field, got {error}"
        );
        let mut blank = vless_params();
        blank
            .as_object_mut()
            .expect("object")
            .insert("headers".to_string(), json!(""));
        let blank_row = fixture("9", "HeadersBlank", "vless", blank, "127.0.0.1", 10809);
        generate(std::slice::from_ref(&blank_row)).expect("blank headers stay a no-op");
        let mut empty = vless_params();
        empty
            .as_object_mut()
            .expect("object")
            .insert("headers".to_string(), json!({}));
        let empty_row = fixture("9", "HeadersEmpty", "vless", empty, "127.0.0.1", 10809);
        generate(std::slice::from_ref(&empty_row)).expect("empty headers stay a no-op");
    }

    #[test]
    fn t03_panel_rows_generate_shapes() {
        fn node_of(row: &ProxyConfig, ty: &str) -> serde_json::Value {
            generate(std::slice::from_ref(row))
                .expect("panel row generates")
                .value()["outbounds"]
                .as_array()
                .expect("outbounds")
                .iter()
                .find(|node| node["type"] == json!(ty))
                .unwrap_or_else(|| panic!("{ty} egress present"))
                .clone()
        }

        let modern = fixture(
            "31",
            "VmessModern",
            "vmess",
            json!({"type": "vmess", "server": "198.51.100.9", "server_port": 443,
                   "uuid": "11111111-2222-4333-8444-555555555555", "alter_id": "3",
                   "encryption": "chacha20-poly1305", "packet_encoding": "xudp",
                   "mux": "on"}),
            "127.0.0.1",
            10831,
        );
        let node = node_of(&modern, "vmess");
        assert_eq!(node["security"], json!("chacha20-poly1305"));
        assert_eq!(node["alter_id"], json!(3), "string aid coerces");
        assert_eq!(node["packet_encoding"], json!("xudp"), "rides to check");
        assert_eq!(node["multiplex"], json!({"enabled": true}));

        let legacy = fixture(
            "32",
            "VmessLegacy",
            "vmess",
            json!({"type": "vmess", "server": "198.51.100.9", "server_port": 443,
                   "uuid": "11111111-2222-4333-8444-555555555555", "alter_id": "0",
                   "security": "auto", "tls": true, "sni": "198.51.100.9",
                   "insecure": true}),
            "127.0.0.1",
            10832,
        );
        let node = node_of(&legacy, "vmess");
        assert_eq!(node["security"], json!("auto"), "legacy cipher stays");
        assert_eq!(node["tls"]["server_name"], json!("198.51.100.9"));
        assert_eq!(node["tls"]["insecure"], json!(true), "legacy insecure maps");
        assert!(node.get("encryption").is_none(), "no new-style key");

        let aided = fixture(
            "33",
            "VmessAid",
            "vmess",
            json!({"type": "vmess", "server": "198.51.100.9", "server_port": 443,
                   "uuid": "11111111-2222-4333-8444-555555555555", "aid": "2"}),
            "127.0.0.1",
            10833,
        );
        let node = node_of(&aided, "vmess");
        assert_eq!(node["alter_id"], json!(2), "`aid` folds");
        assert!(node.get("aid").is_none(), "alias never leaks");

        let trojan = fixture(
            "34",
            "TrojanPage",
            "trojan",
            json!({"type": "trojan", "server": "198.51.100.9", "server_port": 443,
                   "password": "secret", "security": "tls", "sni": "198.51.100.9",
                   "network": "ws", "path": "/w", "mux": "on"}),
            "127.0.0.1",
            10834,
        );
        let node = node_of(&trojan, "trojan");
        assert_eq!(node["password"], json!("secret"));
        assert_eq!(node["tls"]["server_name"], json!("198.51.100.9"));
        assert_eq!(node["transport"]["type"], json!("ws"));
        assert_eq!(node["multiplex"], json!({"enabled": true}));

        let mut ss = ss_params("pass1");
        ss.as_object_mut()
            .expect("object")
            .insert("uot".to_string(), serde_json::Value::Bool(true));
        ss.as_object_mut().expect("object").insert(
            "plugin_opts".to_string(),
            json!("obfs=http;obfs-host=h.example.com"),
        );
        ss.as_object_mut()
            .expect("object")
            .insert("mux".to_string(), json!("on"));
        let row = fixture("35", "SsPage", "shadowsocks", ss, "127.0.0.1", 10835);
        let node = node_of(&row, "shadowsocks");
        assert_eq!(node["method"], json!("aes-128-gcm"));
        assert_eq!(node["password"], json!("pass1"));
        assert_eq!(node["udp_over_tcp"], json!(true), "`uot` folds");
        assert!(node.get("uot").is_none(), "alias never leaks");
        assert_eq!(
            node["plugin_opts"],
            json!("obfs=http;obfs-host=h.example.com"),
            "plugin_opts rides"
        );
        assert_eq!(node["multiplex"], json!({"enabled": true}));
    }

    #[test]
    fn t05_hysteria_v1_and_tuic_page_shapes() {
        fn node_of(row: &ProxyConfig, ty: &str) -> serde_json::Value {
            generate(std::slice::from_ref(row))
                .expect("panel row generates")
                .value()["outbounds"]
                .as_array()
                .expect("outbounds")
                .iter()
                .find(|node| node["type"] == json!(ty))
                .unwrap_or_else(|| panic!("{ty} egress present"))
                .clone()
        }

        let v1 = fixture(
            "51",
            "HyOne",
            "hysteria",
            json!({"type": "hysteria", "server": "192.0.2.14", "server_port": 443,
                   "auth": "secret", "up": "100", "down": "50 Mbps",
                   "server_ports": "9000-9010", "hop_interval": "30s",
                   "obfs": "obfspw", "auth_type": "STRING",
                   "recv_window_conn": 1024, "recv_window": 2048,
                   "disable_mtu_discovery": true}),
            "127.0.0.1",
            10851,
        );
        let node = node_of(&v1, "hysteria");
        assert_eq!(node["auth_str"], json!("secret"), "`auth` maps");
        assert!(node.get("auth").is_none(), "link spelling never leaks");
        assert_eq!(node["up"], json!("100"), "v1 speeds ride");
        assert_eq!(node["down"], json!("50 Mbps"));
        assert_eq!(node["server_ports"], json!("9000-9010"), "v1 tunables ride");
        assert_eq!(node["disable_mtu_discovery"], json!(true));

        let toggled = fixture(
            "52",
            "HyToggled",
            "hysteria",
            json!({"type": "hysteria", "server": "192.0.2.14", "server_port": 443,
                   "password": "v2secret", "up": "100", "down": "100"}),
            "127.0.0.1",
            10852,
        );
        let node = node_of(&toggled, "hysteria");
        assert_eq!(node["auth_str"], json!("v2secret"), "password falls back");
        assert!(node.get("password").is_none(), "alias never leaks");

        let both = fixture(
            "53",
            "HyBoth",
            "hysteria",
            json!({"type": "hysteria", "server": "192.0.2.14", "server_port": 443,
                   "auth": "v1secret", "password": "v2secret",
                   "up": "100", "down": "100"}),
            "127.0.0.1",
            10853,
        );
        let node = node_of(&both, "hysteria");
        assert_eq!(node["auth_str"], json!("v1secret"), "canonical wins");
        assert!(node.get("password").is_none(), "alias never leaks");

        let nospeed = fixture(
            "54",
            "HyNoSpeed",
            "hysteria",
            json!({"type": "hysteria", "server": "192.0.2.14", "server_port": 443,
                   "auth": "secret"}),
            "127.0.0.1",
            10854,
        );
        let error = generate(std::slice::from_ref(&nospeed)).expect_err("v1 needs speeds");
        assert!(
            error.to_string().contains("up") || error.to_string().contains("down"),
            "names the missing speed, got {error}"
        );

        let tuic = fixture(
            "55",
            "TuicPage",
            "tuic",
            json!({"type": "tuic", "server": "192.0.2.12", "server_port": 443,
                   "uuid": "11111111-2222-4333-8444-555555555555", "password": "secret",
                   "congestion_control": "bbr", "udp_relay_mode": "quic",
                   "udp_over_stream": true, "zero_rtt": true, "heartbeat": "10s",
                   "security": "tls", "sni": "192.0.2.12"}),
            "127.0.0.1",
            10855,
        );
        let node = node_of(&tuic, "tuic");
        assert_eq!(node["congestion_control"], json!("bbr"));
        assert_eq!(node["udp_relay_mode"], json!("quic"));
        assert_eq!(node["udp_over_stream"], json!(true));
        assert_eq!(
            node["zero_rtt_handshake"],
            json!(true),
            "`zero_rtt` alias folds"
        );
        assert!(node.get("zero_rtt").is_none(), "alias never leaks");
        assert_eq!(node["heartbeat"], json!("10s"));
        assert_eq!(node["tls"]["server_name"], json!("192.0.2.12"));
    }

    #[test]
    fn t06_naive_anytls_ssh_page_shapes() {
        fn node_of(row: &ProxyConfig, ty: &str) -> serde_json::Value {
            generate(std::slice::from_ref(row))
                .expect("panel row generates")
                .value()["outbounds"]
                .as_array()
                .expect("outbounds")
                .iter()
                .find(|node| node["type"] == json!(ty))
                .unwrap_or_else(|| panic!("{ty} egress present"))
                .clone()
        }

        let naive = fixture(
            "61",
            "NaivePage",
            "naive",
            json!({"type": "naive", "server": "192.0.2.22", "server_port": 443,
                   "username": "u", "password": "p", "udp_over_tcp": true,
                   "quic": true, "quic_congestion_control": "bbr2",
                   "security": "tls", "sni": "192.0.2.22"}),
            "127.0.0.1",
            10861,
        );
        let node = node_of(&naive, "naive");
        assert_eq!(node["udp_over_tcp"], json!(true));
        assert_eq!(node["quic"], json!(true));
        assert_eq!(node["quic_congestion_control"], json!("bbr2"));
        assert_eq!(node["tls"]["server_name"], json!("192.0.2.22"));

        let throne = fixture(
            "62",
            "NaiveThrone",
            "naive",
            json!({"type": "naive", "server": "192.0.2.22", "server_port": 443,
                   "username": "u", "password": "p", "uot": true,
                   "quic_congestion_control": "cubic", "congestion_control": "bbr",
                   "security": "tls", "sni": "192.0.2.22"}),
            "127.0.0.1",
            10862,
        );
        let node = node_of(&throne, "naive");
        assert_eq!(node["udp_over_tcp"], json!(true), "`uot` folds");
        assert!(node.get("uot").is_none(), "alias never leaks");
        assert_eq!(
            node["quic_congestion_control"],
            json!("cubic"),
            "canonical wins"
        );
        assert!(
            node.get("congestion_control").is_none(),
            "alias never leaks"
        );

        let bare_cc = fixture(
            "63",
            "NaiveBareCc",
            "naive",
            json!({"type": "naive", "server": "192.0.2.22", "server_port": 443,
                   "username": "u", "password": "p", "congestion_control": "reno",
                   "security": "tls", "sni": "192.0.2.22"}),
            "127.0.0.1",
            10863,
        );
        let node = node_of(&bare_cc, "naive");
        assert_eq!(node["quic_congestion_control"], json!("reno"));
        assert!(
            node.get("congestion_control").is_none(),
            "alias never leaks"
        );

        let anytls = fixture(
            "64",
            "AnytlsPage",
            "anytls",
            json!({"type": "anytls", "server": "192.0.2.15", "server_port": 8443,
                   "password": "secret", "idle_session_check_interval": "30s",
                   "idle_session_timeout": "60s", "min_idle_session": 5,
                   "security": "tls", "sni": "192.0.2.15"}),
            "127.0.0.1",
            10864,
        );
        let node = node_of(&anytls, "anytls");
        assert_eq!(node["idle_session_check_interval"], json!("30s"));
        assert_eq!(node["idle_session_timeout"], json!("60s"));
        assert_eq!(node["min_idle_session"], json!(5));

        let short = fixture(
            "65",
            "AnytlsShort",
            "anytls",
            json!({"type": "anytls", "server": "192.0.2.15", "server_port": 8443,
                   "password": "secret", "interval": "30s", "timeout": "60s",
                   "min": "5", "security": "tls", "sni": "192.0.2.15"}),
            "127.0.0.1",
            10865,
        );
        let node = node_of(&short, "anytls");
        assert_eq!(node["idle_session_check_interval"], json!("30s"));
        assert_eq!(node["idle_session_timeout"], json!("60s"));
        assert_eq!(node["min_idle_session"], json!(5), "count coerces");
        for leaked in ["interval", "timeout", "min"] {
            assert!(node.get(leaked).is_none(), "alias never leaks: {leaked}");
        }

        let ssh = fixture(
            "66",
            "SshPass",
            "ssh",
            json!({"type": "ssh", "server": "192.0.2.23", "server_port": 22,
                   "user": "root", "password": "p", "private_key_pass": "pp"}),
            "127.0.0.1",
            10866,
        );
        let node = node_of(&ssh, "ssh");
        assert_eq!(node["private_key_passphrase"], json!("pp"));
        assert!(node.get("private_key_pass").is_none(), "alias never leaks");
        let both = fixture(
            "67",
            "SshBoth",
            "ssh",
            json!({"type": "ssh", "server": "192.0.2.23", "server_port": 22,
                   "user": "root", "password": "p",
                   "private_key_passphrase": "canon", "private_key_pass": "alias"}),
            "127.0.0.1",
            10867,
        );
        let node = node_of(&both, "ssh");
        assert_eq!(node["private_key_passphrase"], json!("canon"));
        assert!(node.get("private_key_pass").is_none(), "alias never leaks");
    }

    #[test]
    fn t06_socks_http_page_shapes() {
        fn node_of(row: &ProxyConfig, ty: &str) -> serde_json::Value {
            generate(std::slice::from_ref(row))
                .expect("panel row generates")
                .value()["outbounds"]
                .as_array()
                .expect("outbounds")
                .iter()
                .find(|node| node["type"] == json!(ty))
                .unwrap_or_else(|| panic!("{ty} egress present"))
                .clone()
        }
        let socks = fixture(
            "68",
            "SocksPage",
            "socks",
            json!({"type": "socks", "server": "192.0.2.20", "server_port": 1080,
                   "version": "5", "username": "u", "password": "p",
                   "network": "udp", "udp_over_tcp": true}),
            "127.0.0.1",
            10868,
        );
        let node = node_of(&socks, "socks");
        assert_eq!(node["username"], json!("u"));
        assert_eq!(node["network"], json!("udp"));
        assert_eq!(node["udp_over_tcp"], json!(true));
        assert!(node.get("version").is_none(), "stock default omits");
        let http = fixture(
            "69",
            "HttpPage",
            "http",
            json!({"type": "http", "server": "192.0.2.21", "server_port": 8080,
                   "username": "u", "password": "p", "path": "/p",
                   "headers": "Host: h.example.com",
                   "security": "tls", "sni": "192.0.2.21"}),
            "127.0.0.1",
            10869,
        );
        let node = node_of(&http, "http");
        assert_eq!(node["path"], json!("/p"));
        assert_eq!(node["headers"], json!("Host: h.example.com"));
        assert_eq!(node["tls"]["server_name"], json!("192.0.2.21"));

        let https = fixture(
            "70",
            "HttpsPlain",
            "https",
            json!({"type": "https", "server": "192.0.2.21", "server_port": 443,
                   "username": "u", "password": "p"}),
            "127.0.0.1",
            10870,
        );
        let node = node_of(&https, "http");
        assert!(node.get("tls").is_some(), "https serves TLS-on: {node}");
    }

    #[test]
    fn t06_version_gates_keep_typed_errors() {
        let v4 = fixture(
            "71",
            "SocksFour",
            "socks",
            json!({"type": "socks", "server": "192.0.2.20", "server_port": 1080,
                   "version": "4"}),
            "127.0.0.1",
            10871,
        );
        let error = generate(std::slice::from_ref(&v4)).expect_err("socks v4 fails");
        let text = error.to_string();
        assert!(text.contains("version"), "names the field, got {text}");
        let http_v = fixture(
            "72",
            "HttpVersion",
            "http",
            json!({"type": "http", "server": "192.0.2.21", "server_port": 8080,
                   "version": "h2"}),
            "127.0.0.1",
            10872,
        );
        let error = generate(std::slice::from_ref(&http_v)).expect_err("http version fails");
        let text = error.to_string();
        assert!(text.contains("version"), "names the field, got {text}");
    }

    #[test]
    fn t41_custom_listens_conflict_names_both() {
        fn replacement(name: &str, core: &str, port: u16) -> ProxyConfig {
            fixture(
                name,
                name,
                "custom-config",
                json!({
                    "core": core,
                    "config": {
                        "outbounds": [{"type": "direct", "tag": "direct"}],
                        "inbounds": [
                            {"type": "mixed", "tag": "in-x",
                             "listen": "127.0.0.1", "listen_port": port}
                        ],
                    }
                }),
                "127.0.0.1",
                10801,
            )
        }
        let rows = vec![
            replacement("CustomA", "stock", 10991),
            replacement("CustomB", "fork", 10991),
        ];
        let error = validate_activation(&rows).expect_err("shared port conflicts");
        assert!(
            matches!(error, EngineError::PortConflict { .. }),
            "got {error}"
        );
        let text = error.to_string();
        assert!(text.contains("CustomA"), "names both: {text}");
        assert!(text.contains("CustomB"), "names both: {text}");
        assert!(text.contains("10991"), "names port: {text}");

        let rows = vec![
            replacement("CustomA", "stock", 10991),
            replacement("CustomB", "fork", 10992),
        ];
        let plan = validate_activation(&rows).expect("distinct ports validate");
        assert_eq!(plan.replacement_listens.len(), 2);
    }

    #[test]
    fn t41_xray_served_ports_parsed_for_own_skip() {
        let doc = serde_json::json!({
            "inbounds": [
                {"protocol": "vless", "listen": "127.0.0.1", "port": 11801},
                {"protocol": "vless", "listen": "127.0.0.1", "port": 0},
                {"protocol": "vless"},
            ],
        });
        assert_eq!(
            served_listen_ports(&doc.to_string()),
            vec![("127.0.0.1".to_string(), 11801)]
        );
        assert!(served_listen_ports("not json").is_empty(), "fails open");
    }

    #[test]
    fn t41_crash_and_stop_clear_clash() {
        let (mut controller, stock) = EngineController::fake();
        controller.start_all(&three_standard()).expect("starts");
        assert_eq!(controller.clash_targets().len(), 1);
        stock.inject_crash("boom");
        assert!(!controller.poll_shards().is_empty());
        assert!(controller.clash_targets().is_empty(), "crash drops clash");
        assert!(controller.live_clash_targets().is_empty());
        controller.restart_crashed().expect("recovers");
        assert_eq!(controller.clash_targets().len(), 1);
        controller.stop_all();
        assert!(controller.clash_targets().is_empty(), "stop clears clash");
        assert_eq!(controller.status(), &EngineStatus::Stopped);
    }

    fn t42_outbound(generated: &GeneratedConfig, id: &str) -> serde_json::Value {
        let want = outbound_tag(id);
        generated.value()["outbounds"]
            .as_array()
            .expect("outbounds array")
            .iter()
            .find(|node| node["tag"] == json!(want))
            .unwrap_or_else(|| panic!("outbound {want} present"))
            .clone()
    }

    fn t42_row_from_link(id: &str, link: &str, bind: &str, port: u16) -> ProxyConfig {
        use crate::import::parse_link;
        let imported = parse_link(link).unwrap_or_else(|error| panic!("link parses: {error}"));
        ProxyConfig {
            id: id.to_string(),
            name: imported.name.clone(),
            protocol: imported.protocol.clone(),
            params: imported.params.clone(),
            group: "t42".to_string(),
            enabled: true,
            bind_ip: bind.to_string(),
            port,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        }
    }

    #[test]
    fn t42_socks_link_serves_native() {
        let row = t42_row_from_link(
            "s1",
            "socks://user:pass@192.0.2.20:1080#socks-one",
            "127.0.0.1",
            10801,
        );
        assert_eq!(row.protocol, "socks");
        let node = t42_outbound(
            &generate(std::slice::from_ref(&row)).expect("socks generates"),
            "s1",
        );
        assert_eq!(node["type"], json!("socks"));
        assert_eq!(node["server"], json!("192.0.2.20"));
        assert_eq!(node["server_port"], json!(1080));
        assert_eq!(node["username"], json!("user"));
        assert!(node.get("version").is_none(), "no 1.15-risk version key");
    }

    #[test]
    fn t42_http_and_https_serve_native() {
        let plain = t42_row_from_link(
            "h1",
            "http://user:pass@192.0.2.21:8080#http-one",
            "127.0.0.1",
            10801,
        );
        let node = t42_outbound(
            &generate(std::slice::from_ref(&plain)).expect("http generates"),
            "h1",
        );
        assert_eq!(node["type"], json!("http"));
        assert_eq!(node["server_port"], json!(8080));
        assert_eq!(node["username"], json!("user"));
        assert!(node.get("tls").is_none(), "plain http carries no tls");
        assert!(node.get("version").is_none(), "no 1.15-only version key");

        let secure = t42_row_from_link("h2", "https://192.0.2.22#https-one", "127.0.0.1", 10802);
        let node = t42_outbound(
            &generate(std::slice::from_ref(&secure)).expect("https generates"),
            "h2",
        );
        assert_eq!(node["type"], json!("http"));
        assert_eq!(node["tls"]["enabled"], json!(true));
    }

    #[test]
    fn t42_snell_link_serves_native() {
        let row = t42_row_from_link(
            "sn1",
            "snell://psk-secret@192.0.2.23:8388?version=4#snell-one",
            "127.0.0.1",
            10801,
        );
        assert_eq!(row.protocol, "snell");
        let node = t42_outbound(
            &generate(std::slice::from_ref(&row)).expect("snell generates"),
            "sn1",
        );
        assert_eq!(node["type"], json!("snell"));
        assert_eq!(node["version"], json!(4));
        assert_eq!(node["psk"], json!("psk-secret"));
    }

    #[test]
    fn t42_shadowtls_link_serves_native() {
        let row = t42_row_from_link(
            "st1",
            "shadowtls://shadowpw@192.0.2.24:443?sni=example.com&version=3#st-one",
            "127.0.0.1",
            10801,
        );
        assert_eq!(row.protocol, "shadowtls");
        let node = t42_outbound(
            &generate(std::slice::from_ref(&row)).expect("shadowtls generates"),
            "st1",
        );
        assert_eq!(node["type"], json!("shadowtls"));
        assert_eq!(node["version"], json!(3));
        assert_eq!(node["tls"]["enabled"], json!(true));
        assert_eq!(node["tls"]["server_name"], json!("example.com"));
    }

    #[test]
    fn t42_naive_link_serves_native() {
        let row = t42_row_from_link(
            "n1",
            "naive+https://user:pass@192.0.2.25:8443#naive-one",
            "127.0.0.1",
            10801,
        );
        let node = t42_outbound(
            &generate(std::slice::from_ref(&row)).expect("naive generates"),
            "n1",
        );
        assert_eq!(node["type"], json!("naive"));
        assert_eq!(node["username"], json!("user"));
        assert_eq!(node["tls"]["enabled"], json!(true));
    }

    #[test]
    fn t42_hy2_bbr_by_omission() {
        let row = t42_row_from_link(
            "hy1",
            "hysteria2://letmein@192.0.2.26:8443?sni=example.com#hy2-bbr",
            "127.0.0.1",
            10801,
        );
        let node = t42_outbound(
            &generate(std::slice::from_ref(&row)).expect("hy2 generates"),
            "hy1",
        );
        assert_eq!(node["type"], json!("hysteria2"));
        assert_eq!(node["password"], json!("letmein"));
        assert_eq!(node["tls"]["enabled"], json!(true));
        assert!(node.get("up_mbps").is_none(), "BBR: up omitted");
        assert!(node.get("down_mbps").is_none(), "BBR: down omitted");
    }

    #[test]
    fn t42_hy2_updown_coerced_to_numbers() {
        let row = t42_row_from_link(
            "hy2",
            "hysteria2://letmein@192.0.2.26:8443?sni=example.com&up=100%20Mbps&down=50#hy2-speed",
            "127.0.0.1",
            10801,
        );
        let node = t42_outbound(
            &generate(std::slice::from_ref(&row)).expect("hy2 generates"),
            "hy2",
        );
        assert_eq!(node["up_mbps"], json!(100));
        assert_eq!(node["down_mbps"], json!(50));
    }

    #[test]
    fn t42_hy2_numeric_and_bad_speeds() {
        let row = fixture(
            "hy4",
            "Hy2Numeric",
            "hysteria2",
            json!({"type": "hysteria2", "server": "192.0.2.26", "server_port": 443,
                   "password": "pw", "up_mbps": 200, "down_mbps": 100}),
            "127.0.0.1",
            10801,
        );
        let node = t42_outbound(
            &generate(std::slice::from_ref(&row)).expect("numeric speeds generate"),
            "hy4",
        );
        assert_eq!(node["up_mbps"], json!(200));
        assert_eq!(node["down_mbps"], json!(100));
        let bad = fixture(
            "hy5",
            "Hy2BadSpeed",
            "hysteria2",
            json!({"type": "hysteria2", "server": "192.0.2.26", "server_port": 443,
                   "password": "pw", "up": "fast"}),
            "127.0.0.1",
            10802,
        );
        let error = generate(std::slice::from_ref(&bad)).expect_err("bad speed fails");
        assert!(
            error.to_string().contains("up_mbps"),
            "names field: {error}"
        );
    }

    #[test]
    fn t42_socks_version_shapes() {
        for (id, version) in [("sv1", json!("5")), ("sv2", json!(5))] {
            let row = fixture(
                id,
                "SocksVersion",
                "socks",
                json!({"type": "socks", "server": "192.0.2.20", "server_port": 1080,
                       "version": version}),
                "127.0.0.1",
                10801,
            );
            let node = t42_outbound(
                &generate(std::slice::from_ref(&row)).expect("version 5 generates"),
                id,
            );
            assert!(node.get("version").is_none(), "default omits");
        }
        let row = fixture(
            "sv3",
            "SocksV4",
            "socks",
            json!({"type": "socks", "server": "192.0.2.20", "server_port": 1080,
                   "version": 4}),
            "127.0.0.1",
            10803,
        );
        let error = generate(std::slice::from_ref(&row)).expect_err("version 4 fails");
        assert!(
            error.to_string().contains("\"version\""),
            "names field: {error}"
        );
    }

    #[test]
    fn t42_hy2_obfs_mapped() {
        let row = t42_row_from_link(
            "hy3",
            "hysteria2://letmein@192.0.2.26:8443?sni=example.com&obfs=salamander&obfs-password=obfspw#hy2-obfs",
            "127.0.0.1",
            10801,
        );
        let node = t42_outbound(
            &generate(std::slice::from_ref(&row)).expect("hy2 generates"),
            "hy3",
        );
        assert_eq!(node["obfs"]["type"], json!("salamander"));
        assert_eq!(node["obfs"]["password"], json!("obfspw"));
    }

    #[test]
    fn t42_trojan_allowinsecure_maps_tls_insecure() {
        for (id, query) in [
            ("ti1", "allowInsecure=1"),
            ("ti2", "allow_insecure=true"),
            ("ti3", "insecure=1"),
        ] {
            let row = t42_row_from_link(
                id,
                &format!("trojan://secret@192.0.2.27:443?sni=example.com&{query}#tj-insecure"),
                "127.0.0.1",
                10801,
            );
            let node = t42_outbound(
                &generate(std::slice::from_ref(&row)).expect("trojan generates"),
                id,
            );
            assert_eq!(node["tls"]["insecure"], json!(true), "query {query}");
            assert!(
                node.get("allowInsecure").is_none(),
                "mapped, not passed: {query}"
            );
            assert!(
                node.get("allow_insecure").is_none(),
                "mapped, not passed: {query}"
            );
            assert!(node.get("insecure").is_none(), "folded into tls: {query}");
        }
    }

    #[test]
    fn t42_quic_transport_emitted() {
        let row = t42_row_from_link(
            "q1",
            "vless://11111111-2222-4333-8444-555555555555@192.0.2.28:443?encryption=none&security=tls&sni=example.com&type=quic#quic-one",
            "127.0.0.1",
            10801,
        );
        let node = t42_outbound(
            &generate(std::slice::from_ref(&row)).expect("quic generates"),
            "q1",
        );
        assert_eq!(node["transport"], json!({"type": "quic"}));
    }

    #[test]
    fn t42_kcp_decision_is_typed_bad_option() {
        let row = fixture(
            "k1",
            "VlessKcp",
            "vless",
            json!({"type": "vless", "server": "192.0.2.9", "server_port": 443,
                   "uuid": "11111111-2222-4333-8444-555555555555", "network": "kcp"}),
            "127.0.0.1",
            10801,
        );
        let error = generate(std::slice::from_ref(&row)).expect_err("kcp names its field");
        assert!(
            error.to_string().contains("\"network\""),
            "names field: {error}"
        );
    }

    #[test]
    fn t42_ech_mapped_to_tls() {
        let row = t42_row_from_link(
            "e1",
            "vless://11111111-2222-4333-8444-555555555555@192.0.2.29:443?encryption=none&security=tls&sni=example.com&ech=1#ech-one",
            "127.0.0.1",
            10801,
        );
        let node = t42_outbound(
            &generate(std::slice::from_ref(&row)).expect("ech generates"),
            "e1",
        );
        assert_eq!(node["tls"]["ech"]["enabled"], json!(true));
        assert!(
            node.get("ech").is_none(),
            "folded into tls, not passed through"
        );
    }

    #[test]
    fn t42_hy2_unknown_extra_is_typed_bad_option() {
        let row = t42_row_from_link(
            "hw1",
            "hysteria2://letmein@192.0.2.26:8443?sni=example.com&bogus_xyz=1#hy2-wild",
            "127.0.0.1",
            10801,
        );
        let error = generate(std::slice::from_ref(&row)).expect_err("wild extra fails");
        assert!(
            matches!(error, EngineError::BadCustom { .. }),
            "typed, got {error}"
        );
        assert!(
            error.to_string().contains("bogus_xyz"),
            "names field: {error}"
        );
    }

    #[test]
    fn t42_tuic_unknown_extra_is_typed_bad_option() {
        let row = t42_row_from_link(
            "tw1",
            "tuic://11111111-2222-4333-8444-555555555555:tuicpass@192.0.2.30:4433?bogus_xyz=1#tuic-wild",
            "127.0.0.1",
            10801,
        );
        let error = generate(std::slice::from_ref(&row)).expect_err("wild extra fails");
        assert!(
            matches!(error, EngineError::BadCustom { .. }),
            "typed, got {error}"
        );
        assert!(
            error.to_string().contains("bogus_xyz"),
            "names field: {error}"
        );
    }

    #[test]
    fn t42_no_115_fields_ever_emitted() {
        let rows = vec![
            t42_row_from_link(
                "f1",
                "socks://u:p@192.0.2.20:1080#f",
                "127.0.0.1",
                10801,
            ),
            t42_row_from_link(
                "f2",
                "http://u:p@192.0.2.21:8080#f",
                "127.0.0.1",
                10802,
            ),
            t42_row_from_link(
                "f3",
                "hysteria2://pw@192.0.2.26:8443?sni=example.com#f",
                "127.0.0.1",
                10803,
            ),
            t42_row_from_link(
                "f4",
                "vless://11111111-2222-4333-8444-555555555555@192.0.2.28:443?encryption=none&security=tls&sni=example.com&type=quic#f",
                "127.0.0.1",
                10804,
            ),
        ];
        let generated = generate(&rows).expect("t42 set generates");
        assert!(
            !generated.json.contains("certificate_sha256"),
            "no 1.15 pinning field"
        );
        let value = generated.value();
        for node in value["outbounds"].as_array().expect("outbounds") {
            if node["type"] == json!("http") {
                assert!(node.get("version").is_none(), "no 1.15 http version");
            }
        }

        let pinned = fixture(
            "p1",
            "Pinned",
            "vless",
            json!({"type": "vless", "server": "192.0.2.9", "server_port": 443,
                   "uuid": "11111111-2222-4333-8444-555555555555",
                   "security": "tls", "sni": "192.0.2.9",
                   "certificate_sha256": "abc123"}),
            "127.0.0.1",
            10811,
        );
        let error = generate(std::slice::from_ref(&pinned)).expect_err("pinning names its field");
        assert!(
            error.to_string().contains("certificate_sha256"),
            "names field: {error}"
        );
        let http_version = fixture(
            "p2",
            "HttpVersion",
            "http",
            json!({"type": "http", "server": "192.0.2.21", "server_port": 8080,
                   "version": "2"}),
            "127.0.0.1",
            10812,
        );
        let error =
            generate(std::slice::from_ref(&http_version)).expect_err("http version rejected");
        assert!(
            error.to_string().contains("\"version\""),
            "names field: {error}"
        );
    }

    #[test]
    fn t42_proxy_preset_action_serves_as_route() {
        use std::collections::HashMap;
        let row = fixture(
            "px1",
            "ProxyRow",
            "vless",
            json!({"type": "vless", "server": "192.0.2.9", "server_port": 443,
                   "uuid": "11111111-2222-4333-8444-555555555555"}),
            "127.0.0.1",
            10801,
        );
        let mut row = row;
        row.preset_id = Some("7".to_string());
        let mut presets = HashMap::new();
        presets.insert(
            "7".to_string(),
            vec![json!({"action": "proxy", "domain": ["example.com"]})],
        );
        let generated =
            generate_with_presets(std::slice::from_ref(&row), &presets).expect("proxy merges");
        let rules = generated.value()["route"]["rules"]
            .as_array()
            .expect("rules")
            .clone();
        assert!(
            rules.iter().any(|rule| rule["action"] == json!("route")
                && rule["outbound"] == json!(outbound_tag("px1"))),
            "proxy alias serves as route: {rules:?}"
        );
        assert!(
            rules.iter().all(|rule| rule["action"] != json!("proxy")),
            "no raw proxy action reaches the sidecar"
        );
    }

    #[test]
    fn t42_warp_preset_action_is_guidance_error() {
        use std::collections::HashMap;
        let row = fixture(
            "w1",
            "WarpRow",
            "vless",
            json!({"type": "vless", "server": "192.0.2.9", "server_port": 443,
                   "uuid": "11111111-2222-4333-8444-555555555555"}),
            "127.0.0.1",
            10801,
        );
        let mut row = row;
        row.preset_id = Some("9".to_string());
        let mut presets = HashMap::new();
        presets.insert("9".to_string(), vec![json!({"action": "warp"})]);
        let error =
            generate_with_presets(std::slice::from_ref(&row), &presets).expect_err("warp fails");
        let text = error.to_string();
        assert!(text.contains("warp"), "names warp: {text}");
        assert!(
            text.contains("endpoint"),
            "names the missing endpoint design: {text}"
        );
    }

    fn t58_generate_rules(rules: Vec<serde_json::Value>) -> GeneratedConfig {
        let mut map = HashMap::new();
        map.insert("p1".to_string(), rules);
        let row = t37_row_with_preset();
        generate_with_presets(std::slice::from_ref(&row), &map).expect("generates")
    }

    #[test]
    fn t58_emission_serves_each_action_verbatim() {
        let in_tag = inbound_tag("1");
        let out_tag = outbound_tag("1");

        let generated = t58_generate_rules(vec![json!({"action": "sniff"})]);
        let value = generated.value();
        let rules = value["route"]["rules"].as_array().expect("rules array");
        assert_eq!(rules.len(), 2, "sniff rule + isolation, got {rules:?}");
        assert_eq!(
            rules[0]["action"],
            json!("sniff"),
            "sniff is served verbatim"
        );
        assert_eq!(
            rules[0]["inbound"],
            json!([in_tag]),
            "still scoped to the row"
        );
        assert!(
            !rules[0].as_object().unwrap().contains_key("outbound"),
            "no outbound synthesized for sniff, got {rules:?}"
        );
        assert_eq!(
            rules[1]["outbound"],
            json!(out_tag),
            "isolation follows presets"
        );

        let generated = t58_generate_rules(vec![json!({
            "action": "resolve", "server": "dns-local", "strategy": "prefer_ipv4",
        })]);
        let value = generated.value();
        let rules = value["route"]["rules"].as_array().expect("rules array");
        assert_eq!(rules[0]["action"], json!("resolve"));
        assert_eq!(rules[0]["server"], json!("dns-local"));
        assert_eq!(rules[0]["strategy"], json!("prefer_ipv4"));
        assert!(
            !rules[0].as_object().unwrap().contains_key("outbound"),
            "resolve never routes, got {rules:?}"
        );

        let generated = t58_generate_rules(vec![json!({
            "action": "route-options", "override_address": "192.0.2.53", "override_port": 53,
        })]);
        let value = generated.value();
        let rules = value["route"]["rules"].as_array().expect("rules array");
        assert_eq!(
            rules[0]["action"],
            json!("route-options"),
            "served verbatim"
        );
        assert_eq!(rules[0]["override_address"], json!("192.0.2.53"));
        assert_eq!(rules[0]["override_port"], json!(53));
        assert_eq!(
            rules[0]["outbound"],
            json!(out_tag),
            "route-options joins the outbound-carrying set"
        );

        let generated = t58_generate_rules(vec![json!({
            "action": "route-options", "outbound": "direct",
        })]);
        let value = generated.value();
        let rules = value["route"]["rules"].as_array().expect("rules array");
        assert_eq!(
            rules[0]["outbound"],
            json!("direct"),
            "known direct exit is kept"
        );

        let generated = t58_generate_rules(vec![json!({
            "action": "reject", "method": "drop", "no_drop": true,
        })]);
        let value = generated.value();
        let rules = value["route"]["rules"].as_array().expect("rules array");
        assert_eq!(rules[0]["action"], json!("reject"));
        assert_eq!(rules[0]["method"], json!("drop"));
        assert_eq!(rules[0]["no_drop"], json!(true));

        let generated = t58_generate_rules(vec![json!({
            "action": "block", "method": "reply",
        })]);
        let value = generated.value();
        let rules = value["route"]["rules"].as_array().expect("rules array");
        assert_eq!(
            rules[0]["action"],
            json!("reject"),
            "block serves as reject"
        );
        assert_eq!(
            rules[0]["method"],
            json!("reply"),
            "reject method flows through the merge"
        );
    }

    #[test]
    fn t59_emission_passes_new_attributes_verbatim() {
        let in_tag = inbound_tag("1");
        let out_tag = outbound_tag("1");

        let generated = t58_generate_rules(vec![json!({
            "action": "route",
            "source_ip_cidr": ["10.0.0.0/8", "127.0.0.0/8"],
            "invert": true,
            "process_path_regex": "mstsc\\.exe$",
            "source_port": 10808,
            "source_port_range": "1000:2000",
            "ip_version": 6,
        })]);
        let value = generated.value();
        let rules = value["route"]["rules"].as_array().expect("rules array");
        assert_eq!(rules.len(), 2, "route rule + isolation, got {rules:?}");
        assert_eq!(
            rules[0]["source_ip_cidr"],
            json!(["10.0.0.0/8", "127.0.0.0/8"]),
            "source cidr rides the clone verbatim"
        );
        assert_eq!(rules[0]["invert"], json!(true));
        assert_eq!(rules[0]["process_path_regex"], json!("mstsc\\.exe$"));
        assert_eq!(rules[0]["source_port"], json!(10808));
        assert_eq!(rules[0]["source_port_range"], json!("1000:2000"));
        assert_eq!(rules[0]["ip_version"], json!(6));
        assert_eq!(
            rules[0]["inbound"],
            json!([in_tag]),
            "still scoped to the row"
        );
        assert_eq!(
            rules[0]["outbound"],
            json!(out_tag),
            "route exit is synthesized around the new attributes"
        );

        let generated = t58_generate_rules(vec![json!({
            "action": "direct",
            "source_ip_is_private": true,
            "source_port": "9090",
            "source_ip_cidr": "127.0.0.0/8",
            "custom_attr": 7,
        })]);
        let value = generated.value();
        let rules = value["route"]["rules"].as_array().expect("rules array");
        assert_eq!(rules[0]["source_ip_is_private"], json!(true));
        assert_eq!(rules[0]["source_port"], json!("9090"));
        assert_eq!(rules[0]["source_ip_cidr"], json!("127.0.0.0/8"));
        assert_eq!(
            rules[0]["custom_attr"],
            json!(7),
            "unknown attributes stay tolerated at emission too"
        );
        assert!(
            !rules[0].as_object().unwrap().contains_key("outbound"),
            "direct never grows an outbound, got {rules:?}"
        );
    }

    #[test]
    fn t58_route_options_unknown_exit_stays_loud() {
        let bad = |rules: Vec<serde_json::Value>| {
            let mut map = HashMap::new();
            map.insert("p1".to_string(), rules);
            let row = t37_row_with_preset();
            generate_with_presets(std::slice::from_ref(&row), &map)
                .expect_err("unknown exit must fail loudly")
        };
        for action in ["route", "route-options"] {
            let error = bad(vec![
                json!({"action": action, "outbound": "some-other-row"}),
            ]);
            let text = error.to_string();
            assert!(matches!(error, EngineError::BadPreset { .. }), "{text}");
            assert!(text.contains("names an unknown exit"), "{text}");
        }
    }

    #[test]
    fn t60_rule_names_scrubbed_from_emission() {
        let in_tag = inbound_tag("1");
        let out_tag = outbound_tag("1");
        let generated = t58_generate_rules(vec![
            json!({
                "action": "route", "name": "Office exit",
                "domain": ["example.com"],
            }),
            json!({
                "action": "direct", "name": "Local LAN",
                "ip_cidr": ["10.0.0.0/8"],
            }),
            json!({"action": "block", "name": "Mail ports", "port": [25]}),
            json!({"action": "sniff", "name": "Sniff opener"}),
        ]);
        let value = generated.value();
        let rules = value["route"]["rules"].as_array().expect("rules array");
        assert_eq!(
            rules.len(),
            5,
            "4 named preset rules + 1 isolation, got {rules:?}"
        );
        for (index, rule) in rules.iter().enumerate() {
            assert!(
                !rule.as_object().expect("rule object").contains_key("name"),
                "rule {index} never carries its authoring name into config: {rule}"
            );
        }
        assert_eq!(rules[0]["action"], json!("route"));
        assert_eq!(
            rules[0]["domain"],
            json!(["example.com"]),
            "matchers survive"
        );
        assert_eq!(
            rules[0]["outbound"],
            json!(out_tag),
            "route exit still synthesized"
        );
        assert_eq!(rules[1]["action"], json!("direct"));
        assert_eq!(rules[2]["action"], json!("reject"), "block normalizes");
        assert_eq!(rules[3]["action"], json!("sniff"));
        assert!(
            rules.iter().all(|rule| rule["inbound"] == json!([in_tag])),
            "every emitted rule stays scoped to the row"
        );
        assert_eq!(
            rules[4]["outbound"],
            json!(out_tag),
            "isolation rule unchanged"
        );

        let generated = t58_generate_rules(vec![json!({
            "action": "route", "name": "Direct exit", "outbound": "direct",
        })]);
        let value = generated.value();
        let rules = value["route"]["rules"].as_array().expect("rules array");
        assert_eq!(
            rules[0]["outbound"],
            json!("direct"),
            "an explicit known exit is kept"
        );
        assert!(
            !rules[0].as_object().unwrap().contains_key("name"),
            "the name still never reaches config"
        );
    }

    #[test]
    fn t60_unknown_exit_emission_stays_loud() {
        for action in ["route", "proxy", "route-options"] {
            let mut map = HashMap::new();
            map.insert(
                "p1".to_string(),
                vec![json!({"action": action, "name": "Named hop", "outbound": "some-other-row"})],
            );
            let row = t37_row_with_preset();
            let error = generate_with_presets(std::slice::from_ref(&row), &map)
                .expect_err("unknown exit must stay loud");
            let text = error.to_string();
            assert!(matches!(error, EngineError::BadPreset { .. }), "{text}");
            assert!(text.contains("rule 0"), "{text}");
            assert!(text.contains("names an unknown exit"), "{text}");
            assert!(
                !text.contains("Named hop"),
                "the authoring label stays out of the failure: {text}"
            );
        }
    }

    #[test]
    fn t58_live_throne_country_profile_check() {
        if std::env::var("MANYPROXY_LIVE_CHECK").unwrap_or_default() != "1" {
            return;
        }
        // Throne-country-style profile in Throne doc shape: sniff opener,
        // private-range direct rules, country-suffix direct list, catch-all
        // route. Throne's own profiles open with a leading DNS rule; that
        // action is a non-concept here, so the proof profile carries none.
        let profile = r#"{
            "kind": "throne-route-profile",
            "v": 1,
            "name": "throne-country",
            "rules": [
                {"action": "sniff"},
                {"action": "direct", "source_ip_is_private": true},
                {"action": "direct", "ip_is_private": true},
                {"action": "direct", "domain_suffix": ["cn"]},
                {"action": "route"}
            ]
        }"#;
        let parsed = crate::preset::parse_preset_json(profile, "throne-country")
            .expect("PROBE profile parses at intake");
        let mut map = HashMap::new();
        map.insert("p1".to_string(), parsed.rules);
        let row = t37_row_with_preset();
        let generated = generate_with_presets(std::slice::from_ref(&row), &map)
            .expect("PROBE profile: generates");
        let value = generated.value();
        let rules = value["route"]["rules"].as_array().expect("rules array");
        assert_eq!(
            rules.len(),
            6,
            "5 preset rules + 1 isolation, got {rules:?}"
        );
        assert_eq!(rules[0]["action"], json!("sniff"), "sniff opener served");
        assert_eq!(
            rules[1]["source_ip_is_private"],
            json!(true),
            "private source range goes direct"
        );
        assert_eq!(
            rules[2]["ip_is_private"],
            json!(true),
            "private IP goes direct"
        );
        assert_eq!(
            rules[3]["domain_suffix"],
            json!(["cn"]),
            "country suffix goes direct"
        );
        assert_eq!(
            rules[4]["outbound"],
            json!(outbound_tag("1")),
            "catch-all routes to the row's outbound"
        );
        let binary = std::env::var("MANYPROXY_SINGBOX_BIN").map_or_else(
            |_| {
                if cfg!(windows) {
                    PathBuf::from("target/debug/sing-box.exe")
                } else {
                    PathBuf::from("target/debug/sing-box")
                }
            },
            PathBuf::from,
        );
        let path = std::env::temp_dir().join(format!(
            "manyproxy-t58-live-profile-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::write(&path, &generated.json).expect("temp config writes");
        let output = std::process::Command::new(&binary)
            .arg("check")
            .arg("-c")
            .arg(&path)
            .output()
            .expect("check runs");
        let _ = std::fs::remove_file(&path);
        assert!(
            output.status.success(),
            "PROBE profile: sing-box check EXIT 0, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn t48_clash_reservation_none_when_exhausted() {
        const START: u16 = 12777;
        const END: u16 = 12778;
        let mut held = Vec::new();
        for port in START..=END {
            held.push(
                std::net::TcpListener::bind(format!("127.0.0.1:{port}"))
                    .expect("private range binds"),
            );
        }
        let mut extra = Vec::new();
        assert!(
            reserve_port_in_range(&mut extra, START, END).is_none(),
            "exhausted range yields None"
        );
        drop(held);
        let mut fresh = Vec::new();
        assert!(
            reserve_port_in_range(&mut fresh, START, END).is_some(),
            "freed range reserves again"
        );
    }

    #[test]
    fn t48_wait_give_up_arm_returns() {
        struct NeverExits {
            kills: usize,
        }

        impl WaitTarget for NeverExits {
            fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
                Ok(None)
            }

            fn kill(&mut self) -> std::io::Result<()> {
                self.kills += 1;
                Ok(())
            }
        }

        let mut stub = NeverExits { kills: 0 };
        let start = Instant::now();
        wait_child_bounded(&mut stub, Duration::from_millis(10));
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "give-up returns instead of hanging"
        );
        assert!(stub.kills >= 1, "escalation kill attempted");
    }

    #[cfg(windows)]
    #[test]
    fn t48_stub_executable_lifecycle() {
        if std::env::var("MANYPROXY_LIVE_CHECK").unwrap_or_default() != "1" {
            return;
        }
        use std::io::BufRead as _;

        let comspec = std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string());

        let mut cmd = std::process::Command::new(&comspec);
        cmd.args(["/c", "(echo line-one>&2 & echo line-two>&2 & exit 3)"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped());
        spawn_hidden(&mut cmd);
        let mut exiter = cmd.spawn().expect("stub spawns");
        let pipe = exiter.stderr.take().expect("stderr piped");
        let reader = std::io::BufReader::new(pipe);

        let lines: Vec<String> = reader
            .lines()
            .map(|line| line.expect("stub stderr reads").trim_end().to_string())
            .collect();
        assert_eq!(lines, vec!["line-one", "line-two"]);

        let mut backend = RealEngineBackend::new();
        backend.child = Some(exiter);
        assert!(backend.is_running());
        let deadline = Instant::now() + Duration::from_secs(10);
        let cause = loop {
            if let Some(cause) = backend.poll_exit() {
                break cause;
            }
            assert!(Instant::now() < deadline, "exited stub reaps");
            std::thread::sleep(Duration::from_millis(20));
        };
        assert!(cause.contains("engine exited"), "named cause: {cause}");
        assert!(cause.contains('3'), "exit code carried: {cause}");
        assert!(backend.poll_exit().is_none(), "reap is once");
        assert!(!backend.is_running());

        let buffer = backend.log_buffer.clone();
        let gen = backend.reader_gen;
        push_pipe_line(&buffer, gen, "sidecar-hello".to_string());
        assert_eq!(backend.drain_log_lines(), vec!["sidecar-hello"]);
        assert!(backend.drain_log_lines().is_empty(), "drain takes");

        let mut cmd = std::process::Command::new(&comspec);
        cmd.args(["/c", "ping -n 30 127.0.0.1 >NUL"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        spawn_hidden(&mut cmd);
        let sleeper = cmd.spawn().expect("sleeper spawns");
        let pid = sleeper.id();
        backend.child = Some(sleeper);
        push_pipe_line(&buffer, backend.reader_gen, "stale-line".to_string());
        let start = Instant::now();
        backend.stop_child();
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "stop never hangs the caller"
        );
        assert!(!backend.is_running(), "child reaped (pid {pid})");
        assert!(
            backend.drain_log_lines().is_empty(),
            "stop drops stale lines with the sidecar"
        );
    }

    fn t62_temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "manyproxy-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir works");
        dir
    }

    /// Stages bytes the same way the confirmed download flow does: a file at
    /// the name-derived staged path (the engine resolves it via the staged
    /// map — never from disk globs).
    fn t62_stage(dir: &Path, name: &str, files: &mut HashMap<String, PathBuf>) -> PathBuf {
        let file = dir.join(crate::store::rule_set_file_name(name));
        std::fs::write(&file, format!("staged:{name}")).expect("staged bytes write");
        files.insert(name.to_string(), file.clone());
        file
    }

    #[test]
    fn t62_emission_local_rule_set_entries() {
        let dir = t62_temp_dir("t62emit");
        let mut files = HashMap::new();
        let song = t62_stage(&dir, "Song", &mut files);
        let other = t62_stage(&dir, "Other", &mut files);

        let mut presets = HashMap::new();
        presets.insert(
            "p1".to_string(),
            vec![
                json!({"action": "route", "rule_set": ["Song"]}),
                json!({"action": "direct", "rule_set": ["Song", "Other"]}),
                json!({"action": "block", "rule_set": ["Song"], "port": [25]}),
            ],
        );
        let row = t37_row_with_preset();
        let generated = generate_with_rule_sets(std::slice::from_ref(&row), &presets, &files)
            .expect("staged map generates");
        let value = generated.value();

        // One entry per distinct staged set, local + binary + absolute staged
        // path, deduped across every rule in the build.
        let entries = value["route"]["rule_set"].as_array().expect("emitted");
        assert_eq!(
            entries.len(),
            2,
            "Song dedupes, Other joins once: {entries:?}"
        );
        assert_eq!(entries[0]["type"], json!("local"), "{entries:?}");
        assert_eq!(entries[0]["tag"], json!("rs-Song"));
        assert_eq!(entries[0]["format"], json!("binary"));
        assert_eq!(
            entries[0]["path"],
            json!(song.display().to_string()),
            "the sidecar points at the staged file, never a remote URL"
        );
        assert_eq!(entries[1]["tag"], json!("rs-Other"));
        assert_eq!(entries[1]["path"], json!(other.display().to_string()));

        // Rules reference the rs-* tags; matchers and scoping stay intact.
        let rules = value["route"]["rules"].as_array().expect("rules array");
        assert_eq!(rules.len(), 4, "3 preset rules + 1 isolation");
        assert_eq!(rules[0]["rule_set"], json!(["rs-Song"]));
        assert_eq!(rules[1]["rule_set"], json!(["rs-Song", "rs-Other"]));
        assert_eq!(rules[2]["rule_set"], json!(["rs-Song"]));
        let in_tag = inbound_tag(&row.id);
        let out_tag = outbound_tag(&row.id);
        assert_eq!(rules[0]["outbound"], json!(out_tag));
        // "block" serves as reject — a reject rule carries no outbound exit;
        // the port matcher and rule_set reference stay scoped.
        assert_eq!(rules[2]["action"], json!("reject"));
        assert!(
            rules[2].get("outbound").is_none(),
            "reject has no exit: {}",
            rules[2]
        );
        assert_eq!(rules[2]["port"], json!([25]));
        assert_eq!(rules[3]["outbound"], json!(out_tag), "the isolation route");
        for rule in rules.iter() {
            assert_eq!(rule["inbound"], json!([in_tag]), "{rule}");
        }

        // A build whose preset rules carry no rule_set keeps `route.rule_set`
        // absent: the key exists only when staged sets are actually used.
        let plain = three_standard();
        let mut empty_file_map = HashMap::new();
        empty_file_map.insert("Song".to_string(), song);
        let generated =
            generate_with_rule_sets(&plain, &HashMap::new(), &empty_file_map).expect("generates");
        assert!(
            generated.value()["route"].get("rule_set").is_none(),
            "the route stays clean without rule_set references"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn t62_missing_rule_set_fails_loud() {
        let dir = t62_temp_dir("t62missing");

        // Staged by name in the map, but the file vanished from disk: loud
        // BadPreset naming the set and the path, never skip, never substitute.
        let vanished = dir.join("Ghost.srs");
        let mut files = HashMap::new();
        files.insert("Ghost".to_string(), vanished.clone());
        let mut presets = HashMap::new();
        presets.insert(
            "p1".to_string(),
            vec![json!({"action": "route", "rule_set": ["Ghost"]})],
        );
        let row = t37_row_with_preset();
        let error = generate_with_rule_sets(std::slice::from_ref(&row), &presets, &files)
            .expect_err("vanished staged file must fail");
        let text = error.to_string();
        assert!(matches!(error, EngineError::BadPreset { .. }), "{text}");
        assert!(text.contains("Ghost"), "names the set: {text}");
        assert!(
            text.contains(vanished.display().to_string().as_str()),
            "names the missing staged path: {text}"
        );
        assert!(
            text.contains("re-download"),
            "points at the staging surface: {text}"
        );

        // A reference with no staged name at all resolves through the same
        // staging grammar and fails just as loud.
        let error = generate_with_rule_sets(std::slice::from_ref(&row), &presets, &HashMap::new())
            .expect_err("unlisted name must fail");
        let text = error.to_string();
        assert!(matches!(error, EngineError::BadPreset { .. }), "{text}");
        assert!(text.contains("Ghost"), "{text}");
        assert!(text.contains("missing"), "{text}");

        // Two distinct staged names that sanitize to one tag collide — the
        // mapping stays one set per tag, loud.
        let mut files = HashMap::new();
        let first = t62_stage(&dir, "a b", &mut files);
        let second = t62_stage(&dir, "a-b", &mut files);
        assert_eq!(
            first, second,
            "distinct names in one sanitize bucket share a file — that is the collision"
        );
        presets.insert(
            "p1".to_string(),
            vec![json!({"action": "direct", "rule_set": ["a b", "a-b"]})],
        );
        let error = generate_with_rule_sets(std::slice::from_ref(&row), &presets, &files)
            .expect_err("tag collision must fail");
        let text = error.to_string();
        assert!(matches!(error, EngineError::BadPreset { .. }), "{text}");
        assert!(text.contains("rs-a-b"), "{text}");
        assert!(text.contains("rename"), "{text}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn t62_no_remote_rule_set_emission_ever() {
        // Q4a: Option B is dead — the sidecar never fetches. Whatever the
        // staging state, a generated config can never gain a remote entry and
        // an unresolvable reference fails the build instead of substituting.
        let dir = t62_temp_dir("t62remote");
        let mut files = HashMap::new();
        t62_stage(&dir, "Song", &mut files);
        let mut presets = HashMap::new();
        presets.insert(
            "p1".to_string(),
            vec![json!({"action": "route", "rule_set": ["Song"]})],
        );
        let row = t37_row_with_preset();
        let generated = generate_with_rule_sets(std::slice::from_ref(&row), &presets, &files)
            .expect("generates");
        let value = generated.value();
        for entry in value["route"]["rule_set"].as_array().expect("entries") {
            assert_eq!(
                entry["type"],
                json!("local"),
                "only local entries exist: {entry}"
            );
        }
        assert!(
            !value.to_string().contains("\"type\":\"remote\""),
            "no generated config ever contains a remote rule_set entry: {}",
            generated.json
        );

        // Nothing to resolve → the build fails loud; there is no remote-type
        // fallback path that would emit anything.
        let error = generate_with_rule_sets(std::slice::from_ref(&row), &presets, &HashMap::new())
            .expect_err("unresolvable must fail");
        assert!(matches!(error, EngineError::BadPreset { .. }), "{error}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn t62_live_srs_rule_set_preset_check() {
        if std::env::var("MANYPROXY_LIVE_CHECK").unwrap_or_default() != "1" {
            return;
        }
        let binary = std::env::var("MANYPROXY_SINGBOX_BIN").map_or_else(
            |_| {
                if cfg!(windows) {
                    PathBuf::from("target/debug/sing-box.exe")
                } else {
                    PathBuf::from("target/debug/sing-box")
                }
            },
            PathBuf::from,
        );

        // Proof-time compile with the pinned binary (outside the app: it
        // models the maintainer-hosted artifact; the app fetch is the only
        // in-app path and stays behind the per-download confirm).
        let dir = t62_temp_dir("t62live");
        let source = dir.join("source.json");
        std::fs::write(
            &source,
            r#"{"version":1,"rules":[{"ip_cidr":["10.0.0.0/8"],"outbound":"direct"}]}"#,
        )
        .expect("source set writes");
        let staged = dir.join(crate::store::rule_set_file_name("living-set"));
        let compiled = std::process::Command::new(&binary)
            .arg("rule-set")
            .arg("compile")
            .arg("--output")
            .arg(&staged)
            .arg(&source)
            .output()
            .expect("the pinned binary compiles rule sets");
        assert!(
            compiled.status.success(),
            "PROBE compile: {} stderr: {}",
            compiled.status,
            String::from_utf8_lossy(&compiled.stderr)
        );
        assert!(staged.is_file(), "the compiled .srs stages locally");
        std::fs::remove_file(&source).ok();

        // The staged file rides the app's own staging grammar (name → staged
        // file name), synced into the engine exactly like main.rs does.
        let mut files = HashMap::new();
        files.insert("living-set".to_string(), staged.clone());
        let mut presets = HashMap::new();
        presets.insert(
            "p1".to_string(),
            vec![json!({"action": "route", "rule_set": ["living-set"]})],
        );
        let row = t37_row_with_preset();
        let generated = generate_with_rule_sets(std::slice::from_ref(&row), &presets, &files)
            .expect("PROBE srs: generates");
        let value = generated.value();
        let entries = value["route"]["rule_set"].as_array().expect("entries");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0]["type"], json!("local"));
        assert_eq!(entries[0]["tag"], json!("rs-living-set"));
        assert_eq!(
            entries[0]["path"],
            json!(staged.display().to_string()),
            "the sidecar loads the app-staged bytes"
        );
        assert_eq!(
            value["route"]["rules"][0]["rule_set"],
            json!(["rs-living-set"])
        );

        let path = dir.join("generated.json");
        std::fs::write(&path, &generated.json).expect("temp config writes");
        let output = std::process::Command::new(&binary)
            .arg("check")
            .arg("-c")
            .arg(&path)
            .output()
            .expect("check runs");
        std::fs::remove_dir_all(&dir).ok();
        assert!(
            output.status.success(),
            "PROBE srs: sing-box check EXIT 0, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn t80_shard_processes_lists_live_pids() {
        let (mut controller, stock, fork, xray, extra) = EngineController::fake_quartet();
        controller.set_fork_probe(|| Ok(PathBuf::from("test-fork-bin")));
        controller.set_xray_probe(|| Ok(PathBuf::from("test-xray-bin")));
        controller.set_extra_probe(|_| Ok(PathBuf::from("test-extra-exe")));

        assert_eq!(
            controller.shard_processes(),
            vec![
                ("stock".to_string(), None),
                ("fork".to_string(), None),
                ("xray".to_string(), None),
                ("extra-0".to_string(), None),
            ],
            "idle slots are named but offer no pid to sample"
        );

        let rows = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            ),
            noise_fixture("2", "NoiseOne", 10802),
            xray_fixture("3", "XrayOne", 10803),
            extra_fixture("4", "SlotA", 10804),
        ];
        controller.start_all(&rows).expect("every shard starts");
        for (handle, pid) in [
            (&stock, 4101u32),
            (&fork, 4102),
            (&xray, 4103),
            (&extra, 4104),
        ] {
            handle.set_pid(Some(pid));
        }

        assert_eq!(
            controller.shard_processes(),
            vec![
                ("stock".to_string(), Some(4101)),
                ("fork".to_string(), Some(4102)),
                ("xray".to_string(), Some(4103)),
                ("extra-4".to_string(), Some(4104)),
            ],
            "one (label, pid) per live shard; extras named by their row id"
        );

        // The cohort is exactly what poll_shards tracks: the same crash comes
        // back under the same label, and that shard stops offering a pid.
        extra.inject_crash("signal: killed");
        let crashes = controller.poll_shards();
        assert_eq!(
            crashes.first().map(|(shard, _)| shard.as_str()),
            Some("extra-4"),
            "poll_shards and shard_processes agree on the shard name: {crashes:?}"
        );
        let after = controller.shard_processes();
        assert!(
            after.contains(&("extra-4".to_string(), None)),
            "a dead extra shard drops its pid: {after:?}"
        );

        controller.stop_all();
        assert!(
            controller
                .shard_processes()
                .iter()
                .all(|(_, pid)| pid.is_none()),
            "stopping every shard clears the whole cohort"
        );
    }
}
