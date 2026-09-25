//! Ticket 05: standard multi-engine (one combined sidecar config).
//!
//! One generated engine config serves N active library rows: one `mixed`
//! (SOCKS5+HTTP) inbound per active config on its loopback-default bind, N
//! outbounds/`endpoints` built from the stored protocol params, and
//! `route.rules[]` mapping each inbound to exactly its own outbound
//! (`{"action":"route","outbound":...}` form) with a `direct` catch-all
//! (`route.final`). WireGuard clients become `endpoints[]` (sing-box ≥1.11).
//!
//! The engine runs as a supervised sidecar child process. Unit and headless
//! UI tests never touch a real binary or remote server: [`EngineBackend`] is
//! the trait seam, [`FakeEngineBackend`] is the mock driver.
//!
//! Hard prohibitions (spec + contracts): never set the system proxy, never
//! TUN, never DNS rewrite, no admin for loopback use. The generator emits
//! none of those keys and [`generate`] rejects them structurally.
//!
//! Ticket 07 (fork-core exotic, extends — never rewrites — ticket 05):
//! AmneziaWG-variant and Noise/XHTTP rows run beside standard rows via a
//! compatible fork-core binary (`sing-box-awg`). AWG params validate against
//! an explicit version matrix (2.0: jc/jmin/jmax/s1-s4/h1-h4/i1-i5; 3.x adds
//! header-protection/padding/trailers/ranged-timings per the official
//! AmneziaWG docs); cross-version and unknown fields fail with named errors.
//! Wrong-layer mixes (AWG fields on stock rows, Xray noise/finalmask on AWG
//! rows) fail at validation, never silently. XHTTP transports route via the
//! fork with known-bug notes surfaced in errors. The fork never replaces
//! stock handling: stock-only sets never probe the fork binary, and a missing
//! fork binary fails only exotic rows.

#[cfg(test)]
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::rc::Rc;

use crate::store::ProxyConfig;

/// Base name of the sidecar binary (resolved in the app dir, then `PATH`).
pub const ENGINE_BINARY_BASE: &str = "sing-box";

/// Ticket 07: base name of the compatible fork-core binary (AWG/Noise/XHTTP
/// capable sing-box fork). Stock handling never consults it: only exotic
/// rows resolve it.
pub const FORK_BINARY_BASE: &str = "sing-box-awg";

/// Ticket 07: known fork issue surfaced in every XHTTP-related error
/// (research 02): fork XHTTP `stream-one`/`auto` modes are unreliable, and
/// sidecar XHTTP pins the Xray version.
pub const XHTTP_KNOWN_BUGS: &str = "known fork issue: XHTTP stream-one/auto modes are unreliable (research 02); if streams stall, pin a working mode or Xray-sidecar version";

/// Canonical sing-box outbound/endpoint type for the ticket-05 standard set.
/// Anything else (anytls, naive, socks, http, unknown) is an
/// [`EngineError::UnsupportedProtocol`] owned by a later ticket.
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
        _ => None,
    }
}

/// Inbound tag for a library row (unique: ids are never reused).
pub fn inbound_tag(id: &str) -> String {
    format!("in-{}", sanitize_tag(id))
}

/// Outbound/endpoint tag for a library row.
pub fn outbound_tag(id: &str) -> String {
    format!("out-{}", sanitize_tag(id))
}

fn sanitize_tag(raw: &str) -> String {
    raw.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Failures of the activation seam. Messages name configs, binds, ports, and
/// protocols — never secrets (no params values, keys, or passwords).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// Two active configs want the same local endpoint.
    PortConflict {
        bind: String,
        port: u16,
        first: String,
        second: String,
    },
    /// Protocol belongs to a later (exotic/custom-core) ticket.
    UnsupportedProtocol { name: String, protocol: String },
    /// Stored params are not a JSON object (never echoes the params).
    InvalidParamsJson { name: String },
    /// Stored params miss a required sing-box field (names the field only).
    BadParams { name: String, field: &'static str },
    /// Nothing to run.
    NoActiveConfigs,
    /// Generated JSON fails the structural self-check.
    InvalidGenerated { detail: String },
    /// The engine's own `check` rejected the generated config.
    CheckFailed { detail: String },
    /// No `sing-box` binary in the app dir or `PATH` (never downloads).
    MissingBinary { searched: String },
    /// Ticket 07: no fork-core binary for exotic rows (names the rows and
    /// the expected binary; stock rows are unaffected).
    MissingForkBinary {
        rows: String,
        binary: String,
        searched: String,
        xhttp: bool,
    },
    /// Ticket 07: AmneziaWG version-matrix failure (names the field; the
    /// detail echoes version strings only, never key material).
    BadAwg {
        name: String,
        field: String,
        detail: String,
    },
    /// Ticket 07: wrong-layer obfuscation mix (names the field, never values).
    WrongLayer {
        name: String,
        field: String,
        detail: String,
    },
    /// The sidecar process could not be spawned.
    SpawnFailed { detail: String },
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
                write!(f, "engine config check failed ({detail})")
            }
            EngineError::MissingBinary { searched } => write!(
                f,
                "engine binary not found (looked for {searched}); install sing-box — ManyProxy never downloads executables silently"
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
                    "fork engine binary not found for {rows} (looked for {binary} in {searched}); install the AWG-capable fork — stock configs keep working, only AmneziaWG/Noise/XHTTP rows need it"
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
            EngineError::SpawnFailed { detail } => {
                write!(f, "engine failed to start ({detail})")
            }
        }
    }
}

// -- Ticket 07: fork-core exotic helpers (AWG version matrix, wrong-layer
// guards, XHTTP path, fork resolution) -------------------------------------

/// AmneziaWG-variant protocol names (canonical stored form is `amneziawg`;
/// import accepts every alias here).
pub fn is_awg_protocol(protocol: &str) -> bool {
    matches!(
        protocol.trim().to_ascii_lowercase().as_str(),
        "amneziawg" | "awg" | "amnezia-wg"
    )
}

/// AmneziaWG 2.0 obfuscation fields (official docs: junk-train Jc/Jmin/Jmax,
/// length prefixes S1-S4, header ranges H1-H4, CPS chains I1-I5). Compared
/// lowercase — links/docs spell them `Jc`, `S1`, … — values pass through
/// verbatim.
const AWG_V2_KEYS: &[&str] = &[
    "jc", "jmin", "jmax", "s1", "s2", "s3", "s4", "h1", "h2", "h3", "h4", "i1", "i2", "i3", "i4",
    "i5",
];

/// AmneziaWG 3.x additions (official docs: HeaderProtectionKey,
/// ContentPaddingAddition, ranged timings Rekey*/Keepalive*/MaxHandshake*,
/// RandomTrailers, DisableCookies). Valid only with `awg_version` 3.x.
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

/// Plain WireGuard/endpoint fields allowed on AWG rows at any version (the
/// fork extends the wireguard endpoint schema; its own `check` is
/// authoritative for shapes this list does not know).
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
];

/// Xray-side noise semantics (research 02: "Amnezia Noise" ≠ "Xray Noise").
/// These keys on an AWG row are a wrong-layer mix, never silently accepted.
const XRAY_NOISE_KEYS: &[&str] = &["noise", "finalmask", "final_mask", "xray_noise"];

/// Explicit AWG version matrix: 2.0 vs 3.x (research 02 version-skew risk).
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

/// Declared AWG version from `awg_version` (else legacy `version`); strings
/// and numbers both accepted. Missing/unknown values name the field.
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

/// Wrong-layer mismatch guards (research 02 silent-mismatch class): AWG
/// fields on non-AWG rows, and Xray noise/finalmask semantics on AWG rows,
/// fail here with explicit errors — never silently.
fn check_wrong_layer(
    name: &str,
    protocol: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), EngineError> {
    let awg = is_awg_protocol(protocol);
    for key in params.keys() {
        let lower = key.to_ascii_lowercase();
        if !awg && is_awg_layer_key(&lower) {
            return Err(EngineError::WrongLayer {
                name: name.to_string(),
                field: key.clone(),
                detail: format!(
                    "\"{key}\" is an AmneziaWG field but protocol is \"{protocol}\" \
                     (Amnezia Noise ≠ Xray Noise: match obfuscation to its server; research 02)"
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
    }
    Ok(())
}

/// Validate one AWG row against the explicit version matrix: declared
/// version, required WireGuard credentials, then per-version field
/// membership (cross-version and unknown fields name the field).
fn validate_awg_row(
    name: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<AwgVersion, EngineError> {
    let version = awg_version_of(name, params)?;
    for field in ["private_key", "public_key"] {
        let present = params
            .get(field)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.trim().is_empty());
        if !present {
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

/// Transport marker: flat v2ray-style `network` (what import writes),
/// sing-box `transport.type`, or Xray `streamSettings.network` (what JSON
/// import preserves verbatim).
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

/// True when the stored params select the XHTTP transport (fork-only;
/// research 02: stock sing-box has no `xhttp`).
pub fn config_is_xhttp(config: &ProxyConfig) -> bool {
    let Ok(serde_json::Value::Object(map)) =
        serde_json::from_str::<serde_json::Value>(&config.params)
    else {
        return false;
    };
    transport_of(&map).is_some_and(|transport| transport.eq_ignore_ascii_case("xhttp"))
}

/// True for rows the stock binary cannot serve: AWG variants, plus XHTTP
/// transports on the VLESS family. Anything else (including malformed
/// params) is not fork-routed: malformed rows fail at [`generate`] with
/// their own named error.
pub fn needs_fork(config: &ProxyConfig) -> bool {
    if is_awg_protocol(&config.protocol) {
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

/// One row of the per-endpoint isolation mapping: traffic arriving on
/// `inbound_tag` exits only via `outbound_tag`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsolationEntry {
    pub config_id: String,
    pub inbound_tag: String,
    pub outbound_tag: String,
}

/// A generated combined engine config plus its isolation mapping.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone)]
pub struct GeneratedConfig {
    /// Compact JSON fed to `check` and the sidecar's `-c` file.
    pub json: String,
    pub inbound_tags: Vec<String>,
    pub outbound_tags: Vec<String>,
    pub isolation: Vec<IsolationEntry>,
    /// Ticket 07: true when any row needs the fork-core binary (AWG/XHTTP).
    /// The controller partitions such sets: stock rows stay on the stock
    /// sidecar, exotic rows go to the fork sidecar.
    pub requires_fork: bool,
}

impl GeneratedConfig {
    /// Parsed view of [`GeneratedConfig::json`] (tests assert structure).
    #[cfg(test)]
    pub fn value(&self) -> serde_json::Value {
        serde_json::from_str(&self.json).expect("generated config is valid JSON")
    }
}

/// Build one combined engine config from the active library rows.
///
/// Order: conflict check → per-row outbound/endpoint mapping → route table →
/// structural self-check. Pure: no processes, no network, no secrets in
/// errors.
pub fn generate(configs: &[ProxyConfig]) -> Result<GeneratedConfig, EngineError> {
    if configs.is_empty() {
        return Err(EngineError::NoActiveConfigs);
    }
    let mut taken: HashMap<(String, u16), &str> = HashMap::new();
    for config in configs {
        let key = (config.bind_ip.to_ascii_lowercase(), config.port);
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

    let mut inbounds = Vec::with_capacity(configs.len());
    let mut outbounds: Vec<serde_json::Value> = Vec::with_capacity(configs.len() + 1);
    let mut endpoints = Vec::new();
    let mut rules = Vec::with_capacity(configs.len());
    let mut isolation = Vec::with_capacity(configs.len());
    let mut outbound_tags = Vec::with_capacity(configs.len());
    let mut requires_fork = false;

    for config in configs {
        let in_tag = inbound_tag(&config.id);
        let out_tag = outbound_tag(&config.id);
        let mut params = serde_json::from_str::<serde_json::Value>(&config.params)
            .ok()
            .and_then(|value| value.as_object().cloned())
            .ok_or_else(|| EngineError::InvalidParamsJson {
                name: config.name.clone(),
            })?;
        let server = params
            .get("server")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|server| !server.is_empty())
            .ok_or(EngineError::BadParams {
                name: config.name.clone(),
                field: "server",
            })?;
        let _ = server;
        // Ticket 07: wrong-layer guards run before dispatch so obfuscation
        // mixes fail explicitly instead of connecting nowhere.
        check_wrong_layer(&config.name, &config.protocol, &params)?;
        let xhttp =
            transport_of(&params).is_some_and(|transport| transport.eq_ignore_ascii_case("xhttp"));
        // Ticket 07: AWG variants validate against the version matrix and
        // become fork `wireguard` endpoints (the fork extends the wireguard
        // endpoint schema with the AWG fields, passed through verbatim); the
        // fork's own `check` stays authoritative for value shapes. XHTTP on
        // the VLESS family is fork-routed with its params verbatim.
        let (canonical, fork_row) = if is_awg_protocol(&config.protocol) {
            validate_awg_row(&config.name, &params)?;
            ("wireguard", true)
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
            (canonical, xhttp)
        } else {
            return Err(EngineError::UnsupportedProtocol {
                name: config.name.clone(),
                protocol: config.protocol.clone(),
            });
        };
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
        params.insert(
            "type".to_string(),
            serde_json::Value::String(canonical.to_string()),
        );
        params.insert(
            "tag".to_string(),
            serde_json::Value::String(out_tag.clone()),
        );
        let node = serde_json::Value::Object(params);
        if fork_row {
            requires_fork = true;
        }
        if canonical == "wireguard" {
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
        rules.push(serde_json::json!({
            "inbound": in_tag,
            "action": "route",
            "outbound": out_tag,
        }));
        outbound_tags.push(out_tag.clone());
        isolation.push(IsolationEntry {
            config_id: config.id.clone(),
            inbound_tag: in_tag,
            outbound_tag: out_tag,
        });
    }
    outbounds.push(serde_json::json!({"type": "direct", "tag": "direct"}));

    let document = serde_json::json!({
        "log": {"level": "warn"},
        "inbounds": inbounds,
        "outbounds": outbounds,
        "endpoints": endpoints,
        "route": {"rules": rules, "final": "direct"},
    });
    validate_structure(&document)?;
    Ok(GeneratedConfig {
        json: serde_json::to_string(&document).expect("engine config serializes"),
        inbound_tags: isolation
            .iter()
            .map(|entry| entry.inbound_tag.clone())
            .collect(),
        outbound_tags,
        isolation,
        requires_fork,
    })
}

/// Structural self-check run before the engine's own `check`: unique tags and
/// binds, every rule resolves to exactly one outbound/endpoint, `final`
/// resolves to `direct`, and none of the prohibited keys (`tun`, `dns`,
/// `set_system_proxy`) appear anywhere in the generated shape.
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
        if !binds.insert((listen.to_ascii_lowercase(), port)) {
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
        let inbound = obj
            .get("inbound")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| invalid("rule without inbound"))?;
        let outbound = obj
            .get("outbound")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| invalid("rule without outbound"))?;
        if obj.get("action").and_then(serde_json::Value::as_str) != Some("route") {
            return Err(invalid("rule without route action"));
        }
        if !inbound_names.contains(inbound) {
            return Err(invalid(&format!("rule references unknown \"{inbound}\"")));
        }
        if !exits.contains(outbound) {
            return Err(invalid(&format!("rule references unknown \"{outbound}\"")));
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

/// Sidecar driver seam: the real backend shells out to `sing-box`, the fake
/// backend records configs in memory. Errors are plain strings so the fake
/// and the real backend share one failure shape.
pub trait EngineBackend {
    /// Run the engine's own `check` on the generated JSON (no processes kept).
    fn check_config(&mut self, config_json: &str) -> Result<(), String>;
    /// (Re)start the sidecar with the generated JSON.
    fn start(&mut self, config_json: &str) -> Result<(), String>;
    /// Stop the sidecar if running.
    fn stop(&mut self);
    /// True while a sidecar process (real or fake) is up.
    #[cfg_attr(not(test), allow(dead_code))]
    fn is_running(&self) -> bool;
    /// `Some(cause)` once when a previously running sidecar has exited.
    fn poll_exit(&mut self) -> Option<String>;
}

/// Binary file name for this platform.
pub fn engine_binary_file_name() -> String {
    if cfg!(windows) {
        format!("{ENGINE_BINARY_BASE}.exe")
    } else {
        ENGINE_BINARY_BASE.to_string()
    }
}

/// Resolve the sidecar binary: app directory first, then `PATH`. Never
/// downloads anything; the [`EngineError::MissingBinary`] names every
/// directory searched.
pub fn resolve_engine_binary() -> Result<PathBuf, EngineError> {
    let app_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let path_env = std::env::var("PATH").unwrap_or_default();
    resolve_engine_binary_in(app_dir.as_deref(), &path_env)
}

/// Testable core of [`resolve_engine_binary`] (explicit search roots).
pub fn resolve_engine_binary_in(
    app_dir: Option<&Path>,
    path_env: &str,
) -> Result<PathBuf, EngineError> {
    let file = engine_binary_file_name();
    let mut searched = Vec::new();
    if let Some(dir) = app_dir {
        let candidate = dir.join(&file);
        if candidate.is_file() {
            return Ok(candidate);
        }
        searched.push(dir.to_path_buf());
    }
    for dir in std::env::split_paths(path_env) {
        let candidate = dir.join(&file);
        if candidate.is_file() {
            return Ok(candidate);
        }
        searched.push(dir);
    }
    let searched = searched
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join("; ");
    Err(EngineError::MissingBinary { searched })
}

/// Ticket 07: binary file name for the fork core on this platform.
pub fn fork_binary_file_name() -> String {
    if cfg!(windows) {
        format!("{FORK_BINARY_BASE}.exe")
    } else {
        FORK_BINARY_BASE.to_string()
    }
}

/// Ticket 07: resolve the fork-core binary (app dir first, then `PATH`).
/// Never downloads; `Err` is the `; `-joined searched-directories list.
/// Stock paths never call this: only exotic rows do.
pub fn resolve_fork_binary_in(app_dir: Option<&Path>, path_env: &str) -> Result<PathBuf, String> {
    let file = fork_binary_file_name();
    let mut searched = Vec::new();
    if let Some(dir) = app_dir {
        let candidate = dir.join(&file);
        if candidate.is_file() {
            return Ok(candidate);
        }
        searched.push(dir.to_path_buf());
    }
    for dir in std::env::split_paths(path_env) {
        let candidate = dir.join(&file);
        if candidate.is_file() {
            return Ok(candidate);
        }
        searched.push(dir);
    }
    Err(searched
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join("; "))
}

/// Ticket 07: resolve the fork-core binary, naming the expected binary on
/// failure. Row names are filled in by the controller ([`EngineController`]
/// knows the exotic set); direct callers get the unnamed form.
pub fn resolve_fork_binary() -> Result<PathBuf, EngineError> {
    let app_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
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

/// Ticket 07: probe the fork binary version (`<binary> version`, first
/// line). Lets a wrong/plain binary fail fast with a named error instead of
/// serving exotic rows badly. Paths echoed are binary locations, never
/// secrets.
pub fn probe_fork_version(binary: &Path) -> Result<String, String> {
    let output = std::process::Command::new(binary)
        .arg("version")
        .output()
        .map_err(|error| {
            format!(
                "fork version probe failed for {} ({error})",
                binary.display()
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "fork version probe failed for {} (non-zero exit)",
            binary.display()
        ));
    }
    let first = String::from_utf8_lossy(&output.stdout)
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
    Ok(truncate_detail(&first))
}

fn truncate_detail(detail: &str) -> String {
    const LIMIT: usize = 1000;
    let mut text: String = detail.chars().take(LIMIT).collect();
    if detail.chars().count() > LIMIT {
        text.push('…');
    }
    text
}

/// Production driver: `sing-box check` validates, `sing-box run` serves. The
/// generated JSON lives in a temp `-c` file that is removed on stop so no
/// secrets linger on disk.
///
/// Ticket 07: the same driver serves the fork core ([`RealEngineBackend::fork`]
/// resolves `sing-box-awg`, probes its version, and uses its own temp file
/// so the stock and fork sidecars run beside each other).
pub struct RealEngineBackend {
    child: Option<std::process::Child>,
    config_path: PathBuf,
    fork: bool,
}

impl RealEngineBackend {
    pub fn new() -> Self {
        Self {
            child: None,
            config_path: std::env::temp_dir().join("manyproxy-singbox.json"),
            fork: false,
        }
    }

    /// Ticket 07: fork-core driver (own binary, own temp file, version probe
    /// on every check). Stock rows never touch this.
    pub fn fork() -> Self {
        Self {
            child: None,
            config_path: std::env::temp_dir().join("manyproxy-fork.json"),
            fork: true,
        }
    }

    fn resolve_binary(&self) -> Result<PathBuf, String> {
        if self.fork {
            resolve_fork_binary().map_err(|error| error.to_string())
        } else {
            resolve_engine_binary().map_err(|error| error.to_string())
        }
    }

    fn write_config(&self, config_json: &str) -> Result<PathBuf, String> {
        std::fs::write(&self.config_path, config_json)
            .map_err(|error| format!("cannot write engine config ({error})"))?;
        Ok(self.config_path.clone())
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
            // Version probe is load-bearing: a plain/wrong binary fails fast
            // with a named error instead of serving exotic rows badly.
            probe_fork_version(&binary)?;
        }
        let path = self.write_config(config_json)?;
        let output = std::process::Command::new(&binary)
            .arg("check")
            .arg("-c")
            .arg(&path)
            .output()
            .map_err(|error| format!("engine check could not run ({error})"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(truncate_detail(&String::from_utf8_lossy(&output.stderr)))
        }
    }

    fn start(&mut self, config_json: &str) -> Result<(), String> {
        let binary = self.resolve_binary()?;
        let path = self.write_config(config_json)?;
        self.stop();
        let child = std::process::Command::new(&binary)
            .arg("run")
            .arg("-c")
            .arg(&path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|error| format!("engine failed to start ({error})"))?;
        self.child = Some(child);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_file(&self.config_path);
    }

    fn is_running(&self) -> bool {
        self.child.is_some()
    }

    fn poll_exit(&mut self) -> Option<String> {
        let status = self.child.as_mut()?.try_wait().ok()??;
        self.child = None;
        Some(format!("engine exited ({status})"))
    }
}

#[cfg(test)]
#[derive(Debug, Default)]
struct FakeState {
    running: bool,
    last_config: Option<String>,
    check_calls: usize,
    fail_check: Option<String>,
    exited: Option<String>,
}

/// Read/write handle to a [`FakeEngineBackend`]'s recorded state (tests
/// assert generated configs and inject check failures / crashes).
#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub struct FakeHandle {
    state: Rc<RefCell<FakeState>>,
}

#[cfg(test)]
impl FakeHandle {
    /// Last config JSON handed to `start`, if any.
    pub fn last_config(&self) -> Option<String> {
        self.state.borrow().last_config.clone()
    }

    /// Whether the fake sidecar is up.
    pub fn is_running(&self) -> bool {
        self.state.borrow().running
    }

    /// How many times `check_config` ran.
    pub fn check_calls(&self) -> usize {
        self.state.borrow().check_calls
    }

    /// Make the next (and subsequent, until cleared) `check_config` fail.
    pub fn fail_check(&self, detail: &str) {
        self.state.borrow_mut().fail_check = Some(detail.to_string());
    }

    /// Simulate a sidecar crash; the next [`EngineController::poll`] reports it.
    pub fn inject_crash(&self, cause: &str) {
        let mut state = self.state.borrow_mut();
        state.running = false;
        state.exited = Some(cause.to_string());
    }
}

/// In-memory mock driver: records configs, never spawns processes, never
/// touches the network. All unit and headless UI tests run against this.
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
        state.running = true;
        state.exited = None;
        state.last_config = Some(config_json.to_string());
        Ok(())
    }

    fn stop(&mut self) {
        let mut state = self.state.borrow_mut();
        state.running = false;
    }

    fn is_running(&self) -> bool {
        self.state.borrow().running
    }

    fn poll_exit(&mut self) -> Option<String> {
        self.state.borrow_mut().exited.take()
    }
}

/// Sidecar lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineStatus {
    Stopped,
    Running,
    Crashed { cause: String },
}

/// Supervised sidecar owner: one combined config for the active set,
/// regenerated and re-checked on every membership change. A failed apply
/// never clobbers a previously running sidecar: `check` runs before the old
/// process is stopped.
///
/// Ticket 07: two sidecars run beside each other — stock rows stay on the
/// stock `sing-box` sidecar, exotic (AWG/XHTTP) rows go to the fork-core
/// sidecar. Each subset gets its own generated config, `check`, and process;
/// port conflicts are still checked across the union first. When the fork
/// binary is absent, stock subsets keep working and only exotic rows fail
/// (typed [`EngineError::MissingForkBinary`]).
pub struct EngineController {
    backend: Box<dyn EngineBackend>,
    /// Ticket 07: fork-core sidecar for exotic rows only.
    fork_backend: Box<dyn EngineBackend>,
    /// Ticket 07: fork-binary probe seam (default: real resolution; tests
    /// inject present/missing without touching the filesystem).
    fork_probe: Box<dyn Fn() -> Result<PathBuf, EngineError>>,
    active: Vec<ProxyConfig>,
    status: EngineStatus,
    last_error: Option<String>,
    last_config_json: Option<String>,
}

/// Ticket 07: fill row names + the XHTTP note flag into a probe failure.
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

impl EngineController {
    pub fn new(backend: Box<dyn EngineBackend>) -> Self {
        Self {
            backend,
            fork_backend: Box::new(RealEngineBackend::fork()),
            fork_probe: Box::new(resolve_fork_binary),
            active: Vec::new(),
            status: EngineStatus::Stopped,
            last_error: None,
            last_config_json: None,
        }
    }

    /// Production controller (real `sing-box` + real fork sidecars, lazily
    /// resolved).
    pub fn production() -> Self {
        Self::new(Box::new(RealEngineBackend::new()))
    }

    /// Test controller plus a handle to the mock driver.
    #[cfg(test)]
    pub fn fake() -> (Self, FakeHandle) {
        let (controller, stock, _) = Self::fake_pair();
        (controller, stock)
    }

    /// Ticket 07: test controller with handles to both mock drivers. The
    /// fork probe defaults to missing (deterministic without touching the
    /// filesystem); override with [`EngineController::set_fork_probe`].
    #[cfg(test)]
    pub fn fake_pair() -> (Self, FakeHandle, FakeHandle) {
        let (backend, stock) = FakeEngineBackend::shared();
        let (fork_backend, fork) = FakeEngineBackend::shared();
        let controller = Self {
            backend: Box::new(backend),
            fork_backend: Box::new(fork_backend),
            fork_probe: Box::new(|| {
                Err(EngineError::MissingForkBinary {
                    rows: String::new(),
                    binary: fork_binary_file_name(),
                    searched: "test-seam (no fork binary)".to_string(),
                    xhttp: false,
                })
            }),
            active: Vec::new(),
            status: EngineStatus::Stopped,
            last_error: None,
            last_config_json: None,
        };
        (controller, stock, fork)
    }

    /// Ticket 07: override the fork-binary probe (tests inject present /
    /// missing without filesystem access).
    #[cfg(test)]
    pub fn set_fork_probe(&mut self, probe: impl Fn() -> Result<PathBuf, EngineError> + 'static) {
        self.fork_probe = Box::new(probe);
    }

    /// Start one config (idempotent); restarts the sidecar with the union.
    pub fn start_one(&mut self, config: &ProxyConfig) -> Result<(), EngineError> {
        let mut candidate = self.active.clone();
        if let Some(slot) = candidate.iter_mut().find(|row| row.id == config.id) {
            *slot = config.clone();
        } else {
            candidate.push(config.clone());
        }
        // Pure pre-check: a bad candidate never disturbs the running sidecar.
        generate(&candidate).inspect_err(|error| {
            self.last_error = Some(error.to_string());
        })?;
        self.active = candidate;
        self.apply()
    }

    /// Stop one config; returns true when it was active. Stops the sidecar
    /// when the set becomes empty.
    pub fn stop_one(&mut self, id: &str) -> bool {
        let before = self.active.len();
        self.active.retain(|row| row.id != id);
        if self.active.len() == before {
            return false;
        }
        let _ = self.apply();
        true
    }

    /// Replace the active set (start-all path); validates before touching the
    /// running sidecar.
    pub fn start_all(&mut self, configs: &[ProxyConfig]) -> Result<(), EngineError> {
        let candidate = configs.to_vec();
        if candidate.is_empty() {
            self.active.clear();
            return self.apply();
        }
        generate(&candidate).inspect_err(|error| {
            self.last_error = Some(error.to_string());
        })?;
        self.active = candidate;
        self.apply()
    }

    /// Stop everything and take the sidecar down.
    pub fn stop_all(&mut self) {
        self.active.clear();
        let _ = self.apply();
    }

    fn apply(&mut self) -> Result<(), EngineError> {
        if self.active.is_empty() {
            self.backend.stop();
            self.fork_backend.stop();
            self.status = EngineStatus::Stopped;
            self.last_config_json = None;
            return Ok(());
        }
        // Pure pre-check over the union: conflicts, wrong-layer, AWG matrix.
        // A bad candidate never disturbs either running sidecar.
        let generated = generate(&self.active).inspect_err(|error| {
            self.last_error = Some(error.to_string());
        })?;
        let (stock, exotic): (Vec<ProxyConfig>, Vec<ProxyConfig>) = self
            .active
            .iter()
            .cloned()
            .partition(|row| !needs_fork(row));
        // Fork gate before touching anything: stock subsets keep serving
        // when the fork binary is absent; only exotic rows fail.
        if !exotic.is_empty() {
            if let Err(error) = (self.fork_probe)() {
                let names = exotic
                    .iter()
                    .map(|row| format!("\"{}\"", row.name))
                    .collect::<Vec<_>>()
                    .join(", ");
                let xhttp = exotic.iter().any(config_is_xhttp);
                let error = missing_fork_for(error, &names, xhttp);
                self.last_error = Some(error.to_string());
                return Err(error);
            }
        }
        // Subsets of a validated union cannot fail validation again, but the
        // errors stay typed rather than panicking.
        let stock_gen = if stock.is_empty() {
            None
        } else {
            Some(generate(&stock).inspect_err(|error| {
                self.last_error = Some(error.to_string());
            })?)
        };
        let exotic_gen = if exotic.is_empty() {
            None
        } else {
            Some(generate(&exotic).inspect_err(|error| {
                self.last_error = Some(error.to_string());
            })?)
        };
        if let Some(gen) = &stock_gen {
            if let Err(detail) = self.backend.check_config(&gen.json) {
                let error = EngineError::CheckFailed { detail };
                self.last_error = Some(error.to_string());
                return Err(error);
            }
        }
        if let Some(gen) = &exotic_gen {
            if let Err(mut detail) = self.fork_backend.check_config(&gen.json) {
                if exotic.iter().any(config_is_xhttp) {
                    detail = format!("{detail} ({XHTTP_KNOWN_BUGS})");
                }
                let error = EngineError::CheckFailed { detail };
                self.last_error = Some(error.to_string());
                return Err(error);
            }
        }
        // Restart only populated sidecars; empty subsets stay down. A failed
        // start on one side never takes down the other side.
        let mut first_error: Option<EngineError> = None;
        match stock_gen {
            Some(gen) => {
                self.backend.stop();
                if let Err(detail) = self.backend.start(&gen.json) {
                    let error = EngineError::SpawnFailed { detail };
                    self.last_error = Some(error.to_string());
                    first_error = Some(error);
                }
            }
            None => self.backend.stop(),
        }
        match exotic_gen {
            Some(gen) => {
                self.fork_backend.stop();
                if let Err(detail) = self.fork_backend.start(&gen.json) {
                    let error = EngineError::SpawnFailed { detail };
                    self.last_error = Some(error.to_string());
                    first_error = Some(error);
                }
            }
            None => self.fork_backend.stop(),
        }
        self.status = if self.backend.is_running() || self.fork_backend.is_running() {
            EngineStatus::Running
        } else {
            EngineStatus::Stopped
        };
        self.last_config_json = Some(generated.json);
        if let Some(error) = first_error {
            return Err(error);
        }
        self.last_error = None;
        Ok(())
    }

    /// Crash detection: call on UI actions; reports the named cause once.
    /// Ticket 07: watches both sidecars, stock first.
    pub fn poll(&mut self) -> Option<String> {
        let cause = self
            .backend
            .poll_exit()
            .or_else(|| self.fork_backend.poll_exit())?;
        if self.active.is_empty() {
            return None;
        }
        self.status = EngineStatus::Crashed {
            cause: cause.clone(),
        };
        self.last_error = Some(format!("engine crashed ({cause})"));
        Some(cause)
    }

    /// Ids of the active set.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn running_ids(&self) -> Vec<String> {
        self.active.iter().map(|row| row.id.clone()).collect()
    }

    /// Number of active endpoints.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn running_count(&self) -> usize {
        self.active.len()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_running(&self, id: &str) -> bool {
        let Some(row) = self.active.iter().find(|row| row.id == id) else {
            return false;
        };
        if needs_fork(row) {
            self.fork_backend.is_running()
        } else {
            self.backend.is_running()
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn status(&self) -> &EngineStatus {
        &self.status
    }

    /// One-line status for the status bar.
    pub fn status_text(&self) -> String {
        match &self.status {
            EngineStatus::Stopped => "engine stopped".to_string(),
            EngineStatus::Running => {
                format!("engine running · {} endpoint(s)", self.active.len())
            }
            EngineStatus::Crashed { cause } => format!("engine crashed ({cause})"),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Current per-endpoint isolation mapping (each endpoint exits only via
    /// its own remote).
    #[cfg_attr(not(test), allow(dead_code))]
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

    /// Active configs bound off loopback (the UI warns before serving these).
    pub fn lan_warnings(&self) -> Vec<String> {
        self.active
            .iter()
            .filter(|row| !is_loopback_bind(&row.bind_ip))
            .map(|row| format!("\"{}\" serves on {} (LAN-visible)", row.name, row.bind_ip))
            .collect()
    }
}

/// True for loopback literals (spec: default bind; anything else is an
/// explicit LAN exposure). Unparseable input warns rather than serves
/// silently.
pub fn is_loopback_bind(bind: &str) -> bool {
    bind.trim()
        .parse::<std::net::IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

/// Warning text for a non-loopback bind, if any.
pub fn lan_exposure_warning(name: &str, bind: &str) -> Option<String> {
    if is_loopback_bind(bind) {
        None
    } else {
        Some(format!(
            "\"{name}\" binds {bind}: LAN-visible; anyone on the network can use this proxy"
        ))
    }
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

    /// Slice scenario 1 (structure): 3 stopped standard configs generate 3
    /// mixed loopback inbounds with unique tags and ports.
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
        assert_eq!(generated.inbound_tags.len(), 3);
    }

    /// Slice scenario 1 (isolation): every rule routes its own inbound to
    /// exactly its own outbound; unmatched traffic falls to direct.
    #[test]
    fn route_rules_isolate_each_endpoint_with_direct_final() {
        let generated = generate(&three_standard()).expect("standard set generates");
        let value = generated.value();
        let rules = value["route"]["rules"].as_array().expect("rules array");
        assert_eq!(rules.len(), 3);
        for (entry, rule) in generated.isolation.iter().zip(rules.iter()) {
            assert_eq!(rule["inbound"], json!(entry.inbound_tag));
            assert_eq!(rule["outbound"], json!(entry.outbound_tag));
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
        for entry in &generated.isolation {
            if value["endpoints"]
                .as_array()
                .expect("endpoints array")
                .iter()
                .any(|node| node["tag"] == json!(entry.outbound_tag))
            {
                continue;
            }
            assert!(
                outbound_tags.contains(&entry.outbound_tag),
                "outbound tag resolves: {}",
                entry.outbound_tag
            );
        }
    }

    /// Research mapping: WireGuard clients live in `endpoints[]`, never in
    /// `outbounds[]`.
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
        assert_eq!(outbound_tags.len(), 3); // ss + vless + direct
        assert_eq!(generated.outbound_tags.len(), 3); // one tag per row (wg tag lives in endpoints)
        assert!(generated.outbound_tags.contains(&outbound_tag("1")));
    }

    /// Exotic protocols stay out of this slice with a named error.
    #[test]
    fn unsupported_protocol_names_config_without_secrets() {
        let configs = vec![fixture(
            "9",
            "Odd",
            "anytls",
            json!({"server": "10.0.0.9", "server_port": 443, "password": "TOP-SECRET"}),
            "127.0.0.1",
            10809,
        )];
        let error = generate(&configs).expect_err("anytls is not standard");
        let text = error.to_string();
        assert!(text.contains("\"Odd\""), "names config: {text}");
        assert!(text.contains("anytls"), "names protocol: {text}");
        assert!(!text.contains("TOP-SECRET"), "no secrets: {text}");
    }

    /// Slice scenario 3: a taken port blocks start, naming both configs.
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

    /// Prohibitions: no TUN, no DNS rewrite, no system-proxy keys.
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

    /// Controller on the mock: start-all 3, stop 1 (its endpoint drops, the
    /// others keep their isolation mapping).
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

    /// A failed engine `check` blocks start and leaves nothing running.
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

    /// Crash detection surfaces the named cause exactly once.
    #[test]
    fn crash_poll_records_named_cause() {
        let (mut controller, handle) = EngineController::fake();
        controller.start_all(&three_standard()).expect("starts");
        handle.inject_crash("signal: segmentation fault");
        assert_eq!(
            controller.poll().as_deref(),
            Some("signal: segmentation fault")
        );
        assert!(matches!(controller.status(), EngineStatus::Crashed { .. }));
        assert!(controller.status_text().contains("segmentation fault"));
        assert!(controller.poll().is_none(), "cause reported once");
    }

    /// Binary resolution failure names the searched directories.
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

    /// Param errors name the config and field, never the secret values.
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

    /// Custom binds are allowed but flagged; loopback stays quiet.
    #[test]
    fn lan_warning_only_for_non_loopback() {
        assert!(is_loopback_bind("127.0.0.1"));
        assert!(is_loopback_bind("::1"));
        assert!(!is_loopback_bind("0.0.0.0"));
        assert!(lan_exposure_warning("A", "127.0.0.1").is_none());
        let warning = lan_exposure_warning("B", "0.0.0.0").expect("LAN warns");
        assert!(warning.contains("\"B\""), "names config: {warning}");

        let (mut controller, _) = EngineController::fake();
        let mut rows = three_standard();
        rows[0].bind_ip = "0.0.0.0".to_string();
        controller.start_all(&rows).expect("custom bind starts");
        let warnings = controller.lan_warnings();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("\"Alpha\""));
    }

    /// Empty activation set is a typed error, not an empty engine config.
    #[test]
    fn empty_set_is_no_active_configs() {
        assert!(matches!(generate(&[]), Err(EngineError::NoActiveConfigs)));
    }

    // -- Ticket 07: fork-core exotic (AWG version matrix, wrong-layer
    // guards, XHTTP path, fork gating; TDD at the validation seam) --------

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

    /// Scenario 1: an AmneziaWG 2.0 row is not "unsupported type" — it
    /// becomes a fork `wireguard` endpoint with its params verbatim.
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
        assert!(generated.requires_fork);
        let value = generated.value();
        let endpoints = value["endpoints"].as_array().expect("endpoints array");
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0]["type"], json!("wireguard"));
        assert_eq!(endpoints[0]["tag"], json!(outbound_tag("7")));
        assert_eq!(endpoints[0]["jc"], json!(4));
        assert_eq!(endpoints[0]["jmin"], json!(40));
        assert_eq!(endpoints[0]["awg_version"], json!("2.0"));
        assert_eq!(generated.isolation.len(), 1);
    }

    /// 3.x rows accept the documented additions (header protection, padding,
    /// trailers, ranged timings) alongside the 2.0 base.
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
        assert!(generated.requires_fork);
        let value = generated.value();
        assert_eq!(value["endpoints"][0]["RandomTrailers"], json!("on"));
    }

    /// Sub-version skew: a 3.x-only field on a 2.0 row names the field.
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

    /// Unknown AWG fields are rejected with a named error, never passed
    /// through silently.
    #[test]
    fn awg_unknown_field_rejected() {
        let params = awg_params("2.0", json!({"frobnicate": 1}));
        let configs = vec![awg_fixture("7", "Weird", params, 10807)];
        let error = generate(&configs).expect_err("unknown field fails");
        let text = error.to_string();
        assert!(matches!(error, EngineError::BadAwg { .. }));
        assert!(text.contains("frobnicate"), "names field: {text}");
    }

    /// Missing version and unknown versions name the missing param/version.
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

    /// AWG rows without WireGuard credentials name the missing field without
    /// echoing any key material.
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

    /// Wrong layer, direction 1: AWG fields on stock rows are blocked with
    /// an explicit error, never silent.
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

    /// Wrong layer, direction 2: Xray noise/finalmask semantics on AWG rows
    /// are blocked (Amnezia Noise ≠ Xray Noise).
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

    /// XHTTP on the VLESS family routes via the fork with params verbatim;
    /// XHTTP on WireGuard is a wrong-layer mix.
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
        assert!(generated.requires_fork);
        let value = generated.value();
        let outbounds = value["outbounds"].as_array().expect("outbounds");
        assert!(outbounds
            .iter()
            .any(|node| node["network"] == json!("xhttp")));

        let mut wg = wg_params();
        wg.as_object_mut()
            .expect("object")
            .insert("network".to_string(), json!("xhttp"));
        let configs = vec![fixture("3", "WgX", "wireguard", wg, "127.0.0.1", 10803)];
        let error = generate(&configs).expect_err("xhttp on wireguard fails");
        assert!(matches!(error, EngineError::WrongLayer { .. }));
        assert!(!needs_fork(&configs[0]));
    }

    /// Stock-only sets never need the fork: output shape is unchanged.
    #[test]
    fn stock_sets_need_no_fork() {
        for config in three_standard() {
            assert!(!needs_fork(&config), "stock needs no fork: {}", config.name);
        }
        let generated = generate(&three_standard()).expect("standard set generates");
        assert!(!generated.requires_fork);
    }

    /// Fork resolution names the expected binary and the searched dirs.
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

    /// Version probe fails fast on a missing binary.
    #[test]
    fn fork_version_probe_fails_on_missing_binary() {
        let error = probe_fork_version(Path::new("/nonexistent-manyproxy-dir/sing-box-awg"))
            .expect_err("probe on missing binary fails");
        assert!(error.contains("fork version probe failed"), "got {error}");
    }

    /// Mixed sets with a present fork run both sidecars side by side, each
    /// with its own single-row config.
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

    /// Prohibited side effect: a missing fork binary fails only exotic rows
    /// with a clear error; stock rows keep working.
    #[test]
    fn missing_fork_fails_only_exotic_rows() {
        let (mut controller, stock, fork) = EngineController::fake_pair();
        // Default fake_pair probe is missing: stock-only sets are unaffected.
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

    /// XHTTP rows surface the known-bug note in the missing-binary error.
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
}
