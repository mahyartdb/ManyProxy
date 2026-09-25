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
//!
//! Ticket 08 (Xray sidecar + custom + chaining, extends — never rewrites —
//! tickets 05/07): Xray-only VLESS rows run in a supervised `xray` sidecar
//! bridged to the main config over loopback ingress listeners (research 02
//! sidecar-bridge model). Custom outbound JSON rows merge into their owning
//! core's generated config (`core`: stock/fork/xray); custom full-config rows
//! replace the generated config for their owning core. One user extra-core
//! executable slot runs unprivileged from the app dir with its config file
//! passed through. Chains carry inline hops (or same-sidecar id references)
//! with the egress-only rule: Extra/Custom/Xray nodes must be last.
//! Standard + AWG handling below is untouched.

#[cfg(test)]
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::rc::Rc;
use std::sync::{Arc, Mutex};

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

/// Ticket 08: base name of the Xray-core sidecar binary (Xray-only VLESS
/// rows run here, bridged to the main config over loopback ingress
/// listeners; research 02 sidecar-bridge model).
pub const XRAY_BINARY_BASE: &str = "xray";

/// Ticket 08: loopback bridge-port range for Xray ingress listeners.
/// Disjoint from the library auto-port range (10801-10999) so bridge
/// listeners never collide with user endpoints by construction; the
/// allocator still skips taken user ports defensively.
pub const XRAY_BRIDGE_PORT_START: u16 = 11801;
pub const XRAY_BRIDGE_PORT_END: u16 = 11999;

/// Ticket 08: exact Throne-contract violation errors (research 02).
/// Mid-chain Extra/Custom/Xray placement is rejected with
/// [`CHAIN_EGRESS_RULE`]; a tag-count skew between the Xray sidecar config
/// and the bridge bookkeeping fails start with
/// [`XRAY_BRIDGE_COUNT_SIGNATURE`].
pub const CHAIN_EGRESS_RULE: &str = "Cannot use Extracore or Custom or Xray configs in a chain";
pub const XRAY_BRIDGE_COUNT_SIGNATURE: &str = "xray ingress tags size does not match bridge count!";

/// Ticket 08: Xray-only VLESS rows (research 02: VLESS PQ `encryption`,
/// finalmask/noise, XHTTP and other Xray-only semantics live here, never in
/// the stock schema).
pub fn is_xray_protocol(protocol: &str) -> bool {
    matches!(
        protocol.trim().to_ascii_lowercase().as_str(),
        "xray" | "xray-vless" | "xray_vless" | "vless-xray"
    )
}

/// Ticket 08: single-outbound custom JSON rows (merged into the owning
/// core's generated config; `core` picks stock/fork/xray).
pub fn is_custom_outbound_protocol(protocol: &str) -> bool {
    matches!(
        protocol.trim().to_ascii_lowercase().as_str(),
        "custom" | "custom-out" | "custom-outbound" | "custom_outbound" | "custom-outbound-json"
    )
}

/// Ticket 08: full-config custom rows (replace the owning core's generated
/// config wholesale; the user owns its inbounds).
pub fn is_custom_config_protocol(protocol: &str) -> bool {
    matches!(
        protocol.trim().to_ascii_lowercase().as_str(),
        "custom-config" | "custom_config" | "custom-full" | "custom_full" | "custom-full-config"
    )
}

/// Ticket 08: user extra-core executable slot (single slot: one active row).
pub fn is_extra_core_protocol(protocol: &str) -> bool {
    matches!(
        protocol.trim().to_ascii_lowercase().as_str(),
        "extra" | "extra-core" | "extra_core" | "extracore"
    )
}

/// Ticket 08: chained-outbound rows (inline hops, or id references within
/// the same sidecar config).
pub fn is_chain_protocol(protocol: &str) -> bool {
    matches!(
        protocol.trim().to_ascii_lowercase().as_str(),
        "chain" | "chained" | "chain-outbound" | "chain_outbound"
    )
}

/// Ticket 08: egress-only kinds (research 02 chain rule): Extra, Custom
/// (both shapes), and Xray nodes must terminate a chain, never sit mid-chain.
pub fn is_egress_only_protocol(protocol: &str) -> bool {
    is_extra_core_protocol(protocol)
        || is_custom_outbound_protocol(protocol)
        || is_custom_config_protocol(protocol)
        || is_xray_protocol(protocol)
}

/// Ticket 08: which generated config owns a custom row. Stock is the
/// default; unknown values name the `core` field instead of guessing.
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

/// Ticket 08: read the owning core from a custom row's params (`core`;
/// missing means stock). Values are matched case-insensitively; anything
/// else is a [`EngineError::BadCustom`] naming `core`.
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
    /// Ticket 08: custom-row schema failure (names the offending field and
    /// the reason; never echoes values, keys, or tokens).
    BadCustom {
        name: String,
        field: String,
        detail: String,
    },
    /// Ticket 08: chain egress-only violation (research 02 contract text is
    /// always included; the detail names the row and the offending node).
    ChainViolation { name: String, detail: String },
    /// Ticket 08: Xray bridge bookkeeping skew (research 02 breakage
    /// signature is always included; start fails instead of serving a
    /// half-bridged set).
    XrayBridgeMismatch { detail: String },
    /// Ticket 08: loopback hygiene failure (sidecar traffic must never loop:
    /// ingress binds loopback, bridge ports avoid user endpoints, egresses
    /// never point at loopback).
    XrayLoopback { name: String, detail: String },
    /// Ticket 08: no `xray` binary for Xray rows (names the rows and the
    /// expected binary; stock/fork rows are unaffected).
    MissingXrayBinary {
        rows: String,
        binary: String,
        searched: String,
    },
    /// Ticket 08: no user extra-core executable in the app dir (names the
    /// rows and the expected file; stock/fork/xray rows are unaffected).
    MissingExtraBinary {
        rows: String,
        binary: String,
        searched: String,
    },
    /// Ticket 08: extra-core slot failure (names the field; never echoes
    /// paths with secrets or config contents).
    BadExtra {
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

/// Ticket 08: true for rows served through the Xray sidecar bridge: Xray
/// rows plus xray-core custom outbounds. Malformed params are not
/// xray-routed: they fail at [`generate`]/[`validate_activation`] with
/// their own named error.
pub fn needs_xray(config: &ProxyConfig) -> bool {
    if is_xray_protocol(&config.protocol) {
        return true;
    }
    if is_custom_outbound_protocol(&config.protocol) {
        if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(&config.params) {
            return owning_core_of(&config.name, &map).is_ok_and(|core| core == OwningCore::Xray);
        }
    }
    false
}

// -- Ticket 08: Xray sidecar bridge, custom rows, extra slot, chains -----
// (helpers; the `generate` loop and `EngineController` below consume
// them. Standard + AWG paths above are untouched.)

/// Parse stored params into an object map (never echoes the params).
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

/// Ticket 08: validate one Xray-only VLESS row (server/uuid shape checked
/// here; every other Xray-only field passes through verbatim into the
/// sidecar config).
fn validate_xray_row(
    name: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), EngineError> {
    let server = params
        .get("server")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|server| !server.is_empty());
    if server.is_none() {
        return Err(EngineError::BadParams {
            name: name.to_string(),
            field: "server",
        });
    }
    match params.get("server_port") {
        Some(serde_json::Value::Number(number)) => number
            .as_u64()
            .filter(|port| (1..=65_535).contains(port))
            .ok_or(EngineError::BadCustom {
                name: name.to_string(),
                field: "server_port".to_string(),
                detail: "xray server_port must be 1-65535".to_string(),
            })
            .map(|_| ())?,
        Some(serde_json::Value::String(text)) => text
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|port| *port > 0)
            .ok_or(EngineError::BadCustom {
                name: name.to_string(),
                field: "server_port".to_string(),
                detail: "xray server_port must be 1-65535".to_string(),
            })
            .map(|_| ())?,
        _ => {
            return Err(EngineError::BadParams {
                name: name.to_string(),
                field: "server_port",
            });
        }
    }
    let uuid = params
        .get("uuid")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|uuid| !uuid.is_empty());
    if uuid.is_none() {
        return Err(EngineError::BadParams {
            name: name.to_string(),
            field: "uuid",
        });
    }
    Ok(())
}

/// Ticket 08: validate one custom-outbound row. Stock/fork customs are
/// sing-box-shaped (require `type`); xray-core customs are xray-shaped
/// (require `protocol`). The `core` selector is stripped before the merge so
/// it never pollutes the generated outbound. Returns the owning core and the
/// merge-ready map.
fn validate_custom_outbound(
    name: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<(OwningCore, serde_json::Map<String, serde_json::Value>), EngineError> {
    let core = owning_core_of(name, params)?;
    let mut map = params.clone();
    map.remove("core");
    if core == OwningCore::Xray {
        let protocol = map
            .get("protocol")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|protocol| !protocol.is_empty());
        if protocol.is_none() {
            return Err(EngineError::BadCustom {
                name: name.to_string(),
                field: "protocol".to_string(),
                detail: "xray-core custom outbound needs an Xray \"protocol\" (e.g. \"vless\")"
                    .to_string(),
            });
        }
    } else {
        let outbound_type = map
            .get("type")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|outbound_type| !outbound_type.is_empty());
        if outbound_type.is_none() {
            return Err(EngineError::BadCustom {
                name: name.to_string(),
                field: "type".to_string(),
                detail: "custom outbound needs a sing-box \"type\" (e.g. \"vless\")".to_string(),
            });
        }
        if let Some(port) = map.get("server_port") {
            let valid = match port {
                serde_json::Value::Number(number) => number
                    .as_u64()
                    .is_some_and(|port| (1..=65_535).contains(&port)),
                serde_json::Value::String(text) => {
                    text.trim().parse::<u16>().ok().is_some_and(|port| port > 0)
                }
                _ => false,
            };
            if !valid {
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

/// Ticket 08: extract one custom full-config replacement. Either
/// `{"core": ..., "config": {...}}` or the params object itself when it
/// already carries `outbounds`. Lite structural gate: the replacement must
/// be an object with an `outbounds` array and no prohibited top-level
/// sections (`tun`/`dns`); anything else names the offending field. The row's
/// own endpoint lives inside the custom config (user-managed).
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
    for forbidden in ["tun", "dns"] {
        if top.contains_key(forbidden) {
            return Err(EngineError::BadCustom {
                name: name.to_string(),
                field: forbidden.to_string(),
                detail: format!("custom full config must not carry \"{forbidden}\" (no system network mutation)"),
            });
        }
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
    if top.values().any(|value| {
        value
            .as_object()
            .is_some_and(|obj| obj.contains_key("set_system_proxy"))
    }) {
        return Err(EngineError::BadCustom {
            name: name.to_string(),
            field: "config".to_string(),
            detail: "custom full config must not carry \"set_system_proxy\"".to_string(),
        });
    }
    Ok((core, document))
}

/// Ticket 08: validate one extra-core slot row. Returns the executable file
/// name (app dir only, never a path) and the config JSON string passed
/// through to the sidecar file.
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
    if exe.contains(['/', '\\', ':']) || exe == "." || exe == ".." {
        return Err(EngineError::BadExtra {
            name: config.name.clone(),
            field: "exe".to_string(),
            detail: "extra-core \"exe\" must be a plain file name in the app directory (no paths)"
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

/// Ticket 08: the config JSON an extra-core sidecar serves (validated,
/// never echoed back in errors).
pub fn extra_config_json(config: &ProxyConfig) -> Result<String, EngineError> {
    validate_extra_row(config).map(|(_, config_json)| config_json)
}

/// Ticket 08: chain hop kinds. Only [`ChainHopKind::Standard`] may sit
/// mid-chain; everything else is egress-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainHopKind {
    Standard,
    Xray,
    Custom,
    Extra,
}

/// Ticket 08: one validated chain hop: an id reference into the same
/// sidecar config, or an inline outbound object carried in the chain params.
#[derive(Debug, Clone)]
pub struct ChainHop {
    pub kind: ChainHopKind,
    pub ref_id: Option<String>,
    pub node: Option<serde_json::Map<String, serde_json::Value>>,
    /// Human hop label for errors (`node 2`, never params contents).
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

/// Ticket 08: classify one inline hop object. Nested chains are rejected
/// outright (v1 chains are flat).
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

/// Ticket 08: validate one chain row's `nodes` array. `lookup` maps an
/// id-reference to its row protocol (`None` = unknown id). Mid-chain
/// Extra/Custom/Xray nodes fail with the egress-only rule; unknown ids,
/// nested chains, and typeless hops fail naming `nodes`.
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

/// Ticket 08: ingress/egress tags for an Xray bridge leg (unique: ids are
/// never reused; chain-hop legs suffix the hop index).
pub fn xray_ingress_tag(id: &str) -> String {
    format!("x-in-{}", sanitize_tag(id))
}

/// Ticket 08: egress tag for an Xray bridge leg.
pub fn xray_egress_tag(id: &str) -> String {
    format!("x-out-{}", sanitize_tag(id))
}

/// Ticket 08: one Xray sidecar egress leg: everything the bridge needs to
/// keep the main-config pointer and the sidecar ingress/egress in agreement.
#[derive(Debug, Clone)]
pub struct XrayEgress {
    /// Row or chain-hop owner name (for errors; never params values).
    pub owner: String,
    pub owner_id: String,
    /// VLESS-shaped row params, or xray-shaped custom params when
    /// [`XrayEgress::xray_shaped`] is set.
    pub params: serde_json::Map<String, serde_json::Value>,
    pub xray_shaped: bool,
    pub bridge_port: u16,
    /// Pointer tag in the main config (`out-<id>` or `<chain>-hop-<i>`).
    pub main_tag: String,
    pub ingress_tag: String,
    pub egress_tag: String,
}

/// Ticket 08: deterministic loopback bridge-port assignment. Keys arrive in
/// activation order; taken user ports are skipped defensively (the range is
/// disjoint from the auto range by construction). Exhaustion is a typed
/// error, never a silent reuse.
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

/// Ticket 08: tag-count cross-check (research 02 breakage signature). The
/// sidecar ingress count and the egress count must each equal the bridge
/// bookkeeping count; any skew fails start instead of serving half-bridged.
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

/// Ticket 08: loopback-egress guard — a bridge leg whose remote is itself
/// loopback would recirculate sidecar traffic, so it fails naming the row.
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

/// Ticket 08: build the paired Xray sidecar config for the bridge legs:
/// one loopback SOCKS ingress per leg, one VLESS/custom egress per leg,
/// and `routing.rules[]` binding each ingress to exactly its own egress
/// (per-endpoint isolation, mirrored on the sidecar). Loopback hygiene:
/// ingress binds `127.0.0.1`, bridge ports avoid user endpoints by
/// construction, egress remotes must not be loopback.
pub fn generate_xray_config(egresses: &[XrayEgress]) -> Result<GeneratedXrayConfig, EngineError> {
    let mut inbounds = Vec::with_capacity(egresses.len());
    let mut outbounds: Vec<serde_json::Value> = Vec::with_capacity(egresses.len() + 1);
    let mut rules = Vec::with_capacity(egresses.len());
    let mut ingress_tags = Vec::with_capacity(egresses.len());
    let mut egress_tags = Vec::with_capacity(egresses.len());
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
            let server = egress
                .params
                .get("server")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|server| !server.is_empty())
                .ok_or_else(|| EngineError::BadParams {
                    name: egress.owner.clone(),
                    field: "server",
                })?;
            check_egress_not_loopback(&egress.owner, server, "xray egress")?;
            let server_port = match egress.params.get("server_port") {
                Some(serde_json::Value::Number(number)) => number.as_u64().unwrap_or(0) as u16,
                _ => 0,
            };
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
                "network", "security", "sni", "pbk", "sid", "path", "host", "mode", "fp", "alpn",
            ] {
                if let Some(value) = egress.params.get(key) {
                    stream.insert(key.to_string(), value.clone());
                }
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
        ingress_tags.push(egress.ingress_tag.clone());
        egress_tags.push(egress.egress_tag.clone());
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
        ingress_tags,
        egress_tags,
    })
}

/// Ticket 08: a generated paired Xray sidecar config.
#[derive(Debug, Clone)]
pub struct GeneratedXrayConfig {
    /// Compact JSON fed to the xray sidecar's `-c` file.
    pub json: String,
    #[cfg_attr(not(test), allow(dead_code))]
    pub ingress_tags: Vec<String>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub egress_tags: Vec<String>,
}

impl GeneratedXrayConfig {
    /// Parsed view of [`GeneratedXrayConfig::json`] (tests assert structure).
    #[cfg(test)]
    pub fn value(&self) -> serde_json::Value {
        serde_json::from_str(&self.json).expect("xray config is valid JSON")
    }
}

/// Ticket 08: verify every bridge leg's pointer survived into the main
/// config (a stock custom-config replacement that drops a pointer breaks
/// the bridge: start fails naming the tag instead of half-serving).
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

/// Ticket 08: main-config loopback pointer at one Xray ingress listener.
/// The pointer is a plain SOCKS outbound at `127.0.0.1`; the sidecar owns
/// the real egress, so sidecar traffic can never leak between endpoints.
fn bridge_pointer(main_tag: &str, bridge_port: u16) -> serde_json::Value {
    serde_json::json!({
        "type": "socks",
        "tag": main_tag,
        "server": "127.0.0.1",
        "server_port": bridge_port,
        "version": "5",
    })
}

/// Ticket 08: the controller's per-activation build plan — one pure
/// validation pass over the union that partitions rows per sidecar, assigns
/// bridge ports, extracts custom-config replacements, and enforces the
/// chain egress-only rule. A bad candidate fails here before any running
/// sidecar is touched.
#[derive(Debug, Clone)]
pub struct ActivationPlan {
    /// Rows served by the main (stock) sidecar: standard, Xray pointers,
    /// stock-core customs, and chains.
    pub main_rows: Vec<ProxyConfig>,
    /// Rows served by the fork sidecar: AWG/XHTTP plus fork-core customs.
    pub fork_rows: Vec<ProxyConfig>,
    /// Xray sidecar legs (row VLESS, xray-core customs, xray-last chain hops).
    pub egresses: Vec<XrayEgress>,
    /// The single extra-core slot row, if any.
    pub extra: Option<ProxyConfig>,
    /// Custom full-config replacements (`(core, config json)`; at most one
    /// per core).
    pub replacements: Vec<(OwningCore, String)>,
    /// Quoted row names per gate (fork/xray/extra binary resolution).
    pub fork_names: String,
    pub fork_xhttp: bool,
    pub xray_names: String,
    pub extra_names: String,
}

/// Ticket 08: build the activation plan (pure: no processes, no network, no
/// secrets in errors). Port conflicts still name both parties; custom rows
/// name their offending field; chains enforce the egress-only rule.
pub fn validate_activation(configs: &[ProxyConfig]) -> Result<ActivationPlan, EngineError> {
    if configs.is_empty() {
        return Err(EngineError::NoActiveConfigs);
    }
    // Port conflicts across endpoint rows (custom full-config rows are
    // user-managed: their bind is documentation, not an endpoint we serve).
    let mut taken: HashMap<(String, u16), &str> = HashMap::new();
    for config in configs {
        if is_custom_config_protocol(&config.protocol) {
            continue;
        }
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
    // Classify + validate every row; collect bridge keys in activation order.
    let mut main_rows = Vec::new();
    let mut fork_rows = Vec::new();
    let mut egresses: Vec<XrayEgress> = Vec::new();
    let mut extra: Option<ProxyConfig> = None;
    let mut replacements: Vec<(OwningCore, String)> = Vec::new();
    let mut bridge_keys: Vec<String> = Vec::new();
    for config in configs {
        let params = params_object(config)?;
        if is_extra_core_protocol(&config.protocol) {
            validate_extra_row(config)?;
            if let Some(previous) = &extra {
                return Err(EngineError::BadExtra {
                    name: config.name.clone(),
                    field: "exe".to_string(),
                    detail: format!(
                        "only one extra-core row can be active at a time (single user slot; \"{}\" already holds it)",
                        previous.name
                    ),
                });
            }
            extra = Some(config.clone());
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
        } else if is_xray_protocol(&config.protocol) {
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
            // Same schema gate `generate` applies (AWG matrix, server
            // shape): the pre-check must reject bad rows before `active`
            // moves, exactly like the ticket-05/07 seam did.
            let mut owned = params.clone();
            build_standard_node(config, &mut owned, "validate")?;
            fork_rows.push(config.clone());
        } else {
            check_wrong_layer(&config.name, &config.protocol, &params)?;
            // Same schema gate `generate` applies (server shape, known
            // protocol): unknown protocols and bad params fail here, not
            // after `active` moves.
            if canonical_type(&config.protocol).is_none() && !is_awg_protocol(&config.protocol) {
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
    // Chains validate against the union with a same-sidecar rule: id
    // references must resolve to rows served by the main config (v1 chains
    // stay within one sidecar; the Xray bridge pointer lives there).
    let main_ids: HashSet<&str> = main_rows.iter().map(|row| row.id.as_str()).collect();
    let by_id: HashMap<&str, &ProxyConfig> =
        configs.iter().map(|row| (row.id.as_str(), row)).collect();
    for config in configs {
        if !is_chain_protocol(&config.protocol) {
            continue;
        }
        let params = params_object(config)?;
        let hops = validate_chain_nodes(&config.name, &params, &|id| {
            by_id.get(id).map(|row| row.protocol.clone())
        })?;
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
            }
            if hop.kind == ChainHopKind::Xray && hop.ref_id.is_none() {
                bridge_keys.push(format!("{}#hop{}", config.id, index));
            }
        }
    }
    // Bridge ports: deterministic over the union's taken user ports.
    let taken_ports: HashSet<u16> = configs
        .iter()
        .filter(|row| !is_custom_config_protocol(&row.protocol))
        .map(|row| row.port)
        .collect();
    let ports = assign_bridge_ports(&bridge_keys, &taken_ports)?;
    for config in configs {
        if is_xray_protocol(&config.protocol) {
            let params = params_object(config)?;
            egresses.push(XrayEgress {
                owner: config.name.clone(),
                owner_id: config.id.clone(),
                params,
                xray_shaped: false,
                bridge_port: ports[&config.id],
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
                    owner_id: config.id.clone(),
                    params: map,
                    xray_shaped: true,
                    bridge_port: ports[&config.id],
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
                    owner_id: config.id.clone(),
                    params: map,
                    xray_shaped,
                    bridge_port: ports[&format!("{}#hop{}", config.id, index)],
                    main_tag: format!("{}-hop-{index}", outbound_tag(&config.id)),
                    ingress_tag: format!("{}-hop-{index}", xray_ingress_tag(&config.id)),
                    egress_tag: format!("{}-hop-{index}", xray_egress_tag(&config.id)),
                });
            }
        }
    }
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
    let extra_names = extra
        .as_ref()
        .map(|row| format!("\"{}\"", row.name))
        .unwrap_or_default();
    Ok(ActivationPlan {
        main_rows,
        fork_rows,
        egresses,
        extra,
        replacements,
        fork_names,
        fork_xhttp,
        xray_names,
        extra_names,
    })
}

/// Ticket 08: shape-check one inline xray chain hop (VLESS fields; the
/// sidecar carries them verbatim). Errors name `nodes`, never values.
fn validate_xray_hop(
    name: &str,
    label: &str,
    map: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), EngineError> {
    let server = map
        .get("server")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|server| !server.is_empty());
    if server.is_none() {
        return Err(EngineError::BadCustom {
            name: name.to_string(),
            field: "nodes".to_string(),
            detail: format!("{label} xray hop is missing \"server\""),
        });
    }
    let uuid = map
        .get("uuid")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|uuid| !uuid.is_empty());
    if uuid.is_none() {
        return Err(EngineError::BadCustom {
            name: name.to_string(),
            field: "nodes".to_string(),
            detail: format!("{label} xray hop is missing \"uuid\""),
        });
    }
    Ok(())
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
    /// Ticket 08: true when any row needs the Xray sidecar (Xray-only
    /// VLESS, xray-core customs, xray-last chain hops).
    pub requires_xray: bool,
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
    let keys = bridge_keys_for(configs)?;
    let taken_ports: HashSet<u16> = configs.iter().map(|row| row.port).collect();
    let bridges = assign_bridge_ports(&keys, &taken_ports)?;
    build_main_config(configs, &bridges)
}
/// rows and xray-core customs; `{id}#hop{i}` for xray-last inline chain
/// hops). Both [`generate`] and [`validate_activation`] derive keys this
/// way so one shared port map keeps pointers and ingress in agreement.
fn bridge_keys_for(configs: &[ProxyConfig]) -> Result<Vec<String>, EngineError> {
    let mut keys = Vec::new();
    for config in configs {
        if is_xray_protocol(&config.protocol) {
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

/// Ticket 05/07 standard node builder (extracted verbatim for chain-hop
/// reuse): stored params → sing-box outbound/endpoint node. Returns the
/// node, whether it belongs in `endpoints[]` (wireguard), and whether it
/// needs the fork sidecar.
fn build_standard_node(
    config: &ProxyConfig,
    params: &mut serde_json::Map<String, serde_json::Value>,
    out_tag: &str,
) -> Result<(serde_json::Value, bool, bool), EngineError> {
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
    check_wrong_layer(&config.name, &config.protocol, params)?;
    let xhttp =
        transport_of(params).is_some_and(|transport| transport.eq_ignore_ascii_case("xhttp"));
    // Ticket 07: AWG variants validate against the version matrix and
    // become fork `wireguard` endpoints (the fork extends the wireguard
    // endpoint schema with the AWG fields, passed through verbatim); the
    // fork's own `check` stays authoritative for value shapes. XHTTP on
    // the VLESS family is fork-routed with its params verbatim.
    let (canonical, fork_row) = if is_awg_protocol(&config.protocol) {
        validate_awg_row(&config.name, params)?;
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
        serde_json::Value::String(out_tag.to_string()),
    );
    let node = serde_json::Value::Object(params.clone());
    Ok((node, canonical == "wireguard", fork_row))
}

/// Ticket 08: one row's main-config emission: its outbound/endpoint nodes
/// plus the tag the route rule must point at.
struct RowBuilt {
    nodes: Vec<(serde_json::Value, bool)>,
    serving_tag: String,
    requires_fork: bool,
    requires_xray: bool,
}

/// Ticket 08: build one ticket-08 row for the main config (`None` for
/// standard/AWG rows, which fall through to [`build_standard_node`]).
/// Extra and full-config rows are rejected here: they run in their own
/// sidecar / replace a core wholesale, so a bare main config can never
/// serve them (the controller partitions them out first).
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
    if is_xray_protocol(&config.protocol) {
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
            requires_fork: false,
            requires_xray: true,
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
                requires_fork: false,
                requires_xray: true,
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
            requires_fork: core == OwningCore::Fork,
            requires_xray: false,
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
        let mut requires_xray = false;
        let mut requires_fork_hop = false;
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
                    if needs_fork(target) {
                        return Err(EngineError::BadCustom {
                            name: config.name.clone(),
                            field: "nodes".to_string(),
                            detail: format!(
                                "{} references fork-sidecar row \"{id}\" (v1 chains stay within one sidecar)",
                                hop.label
                            ),
                        });
                    }
                    if is_xray_protocol(&target.protocol) {
                        let port = *bridges.get(&target.id).ok_or_else(|| {
                            EngineError::InvalidGenerated {
                                detail: format!("no bridge port assigned for \"{}\"", target.name),
                            }
                        })?;
                        requires_xray = true;
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
                            requires_xray = true;
                            bridge_pointer(&hop_tag, port)
                        } else {
                            if core == OwningCore::Fork {
                                requires_fork_hop = true;
                            }
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
                        // Chain position: every hop but the last dials
                        // through the next hop (sing-box `detour`).
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
                        requires_xray = true;
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
        return Ok(Some(RowBuilt {
            nodes,
            serving_tag,
            requires_fork: requires_fork_hop,
            requires_xray,
        }));
    }
    Ok(None)
}

fn build_main_config(
    configs: &[ProxyConfig],
    bridges: &HashMap<String, u16>,
) -> Result<GeneratedConfig, EngineError> {
    if configs.is_empty() {
        return Err(EngineError::NoActiveConfigs);
    }
    let mut inbounds = Vec::with_capacity(configs.len());
    let mut outbounds: Vec<serde_json::Value> = Vec::with_capacity(configs.len() + 1);
    let mut endpoints = Vec::new();
    let mut rules = Vec::with_capacity(configs.len());
    let mut isolation = Vec::with_capacity(configs.len());
    let mut outbound_tags = Vec::with_capacity(configs.len());
    let mut requires_fork = false;
    let mut requires_xray = false;

    for config in configs {
        let in_tag = inbound_tag(&config.id);
        let out_tag = outbound_tag(&config.id);
        let mut params = params_object(config)?;
        // Ticket 08 rows first; standard + AWG rows fall through to the
        // ticket-05/07 builder below (untouched).
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
            rules.push(serde_json::json!({
                "inbound": in_tag,
                "action": "route",
                "outbound": built.serving_tag,
            }));
            outbound_tags.push(built.serving_tag.clone());
            isolation.push(IsolationEntry {
                config_id: config.id.clone(),
                inbound_tag: in_tag,
                outbound_tag: built.serving_tag,
            });
            requires_fork |= built.requires_fork;
            requires_xray |= built.requires_xray;
            continue;
        }
        let (node, is_endpoint, fork_row) = build_standard_node(config, &mut params, &out_tag)?;
        if fork_row {
            requires_fork = true;
        }
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
        requires_xray,
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
    /// Ticket 13: take buffered sidecar stderr lines (empty when none).
    /// The real backend pipes stderr and pumps it on a reader thread; the
    /// fake backend replays scripted lines. UI ingests the lines through
    /// `LogStore::push_raw` (tag match + redact + bounded ring).
    fn drain_log_lines(&mut self) -> Vec<String> {
        Vec::new()
    }
    /// Ticket 08: point the extra-slot driver at one row's executable
    /// before `start` (real driver swaps its binary; the fake ignores it
    /// and keeps recording configs so tests stay hermetic).
    fn set_extra_exe(&mut self, _exe: PathBuf) {}
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

/// Ticket 08: binary file name for the Xray core on this platform.
pub fn xray_binary_file_name() -> String {
    if cfg!(windows) {
        format!("{XRAY_BINARY_BASE}.exe")
    } else {
        XRAY_BINARY_BASE.to_string()
    }
}

/// Ticket 08: resolve the Xray-core binary (app dir first, then `PATH`).
/// Never downloads; `Err` is the `; `-joined searched-directories list.
/// Stock/fork paths never call this: only Xray rows do.
pub fn resolve_xray_binary_in(app_dir: Option<&Path>, path_env: &str) -> Result<PathBuf, String> {
    let file = xray_binary_file_name();
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

/// Ticket 08: resolve the Xray-core binary, naming the expected binary on
/// failure. Row names are filled in by the controller ([`EngineController`]
/// knows the Xray set); direct callers get the unnamed form.
pub fn resolve_xray_binary() -> Result<PathBuf, EngineError> {
    let app_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let path_env = std::env::var("PATH").unwrap_or_default();
    resolve_xray_binary_in(app_dir.as_deref(), &path_env).map_err(|searched| {
        EngineError::MissingXrayBinary {
            rows: String::new(),
            binary: xray_binary_file_name(),
            searched,
        }
    })
}

/// Ticket 08: resolve one user extra-core executable. App directory ONLY
/// (never `PATH`): the slot is explicit user placement, and the lookup
/// stays auditable. The name must be a plain file name (no directories,
/// no absolute paths); traversal attempts fail without touching the
/// filesystem. Execution is always unprivileged (plain spawn, never
/// elevated). `Err` is a human reason or the searched directory.
pub fn resolve_extra_binary_in(app_dir: Option<&Path>, exe: &str) -> Result<PathBuf, String> {
    let name = exe.trim();
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', ':']) {
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
    if candidate.is_file() {
        Ok(candidate)
    } else {
        Err(dir.display().to_string())
    }
}

/// Ticket 08: resolve one user extra-core executable against the real app
/// dir. Row names are filled in by the controller; direct callers get the
/// unnamed form.
pub fn resolve_extra_binary(exe: &str) -> Result<PathBuf, EngineError> {
    let app_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    resolve_extra_binary_in(app_dir.as_deref(), exe).map_err(|searched| {
        EngineError::MissingExtraBinary {
            rows: String::new(),
            binary: exe.trim().to_string(),
            searched,
        }
    })
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
/// Ticket 13: sidecar stderr is piped (no longer nulled) and pumped by a
/// reader thread into a bounded buffer; the UI drains it into `LogStore`
/// (existing tag match, ingest-time redaction, bounded ring). The thread
/// ends on EOF when the sidecar exits, so pipes close cleanly; an unreadable
/// pipe records one named error line instead of stale output (fail-closed).
///
/// Ticket 07: the same driver serves the fork core ([`RealEngineBackend::fork`]
/// resolves `sing-box-awg`, probes its version, and uses its own temp file
/// so the stock and fork sidecars run beside each other).
///
/// Ticket 08: the same driver serves the Xray sidecar
/// ([`RealEngineBackend::xray`]: `xray run -c`, own temp file) and the
/// single user extra-core slot ([`RealEngineBackend::extra`]: the user's
/// executable from the app dir, config file passed through as
/// `-c <config>`, spawned unprivileged — never elevated — with its own
/// temp file). Neither Xray nor extra-core has a stable cross-version
/// `check` CLI contract in this repo's context, so their `check_config`
/// writes the file and trusts the ticket-08 validators (structure, bridge
/// bookkeeping, loopback hygiene); a bad runtime still surfaces via `poll`.
pub struct RealEngineBackend {
    child: Option<std::process::Child>,
    config_path: PathBuf,
    fork: bool,
    /// Ticket 08: xray sidecar (`xray run -c`) vs stock/fork shape.
    xray: bool,
    /// Ticket 08: user extra-core executable (its config is passed through
    /// as `-c <file>`); `None` for the bundled-core shapes.
    extra_exe: Option<PathBuf>,
    /// Ticket 13: piped-stderr lines (raw until `LogStore` ingest redacts
    /// them) plus at most one named pipe-error line; bounded, cleared on
    /// every start/stop so restarts never replay stale lines.
    log_buffer: Arc<Mutex<Vec<String>>>,
}

/// Ticket 13: pipe-reader buffer bound (the `LogStore` ring bounds display
/// storage separately; this only caps lines awaiting drain).
pub const LOG_PIPE_BUFFER: usize = 2000;

/// Ticket 13: push one reader-thread line, dropping the oldest when full
/// (bounded memory even if the UI never drains).
fn push_pipe_line(buffer: &Arc<Mutex<Vec<String>>>, line: String) {
    if let Ok(mut lines) = buffer.lock() {
        if lines.len() >= LOG_PIPE_BUFFER {
            lines.remove(0);
        }
        lines.push(line);
    }
}

impl RealEngineBackend {
    pub fn new() -> Self {
        Self {
            child: None,
            config_path: std::env::temp_dir().join("manyproxy-singbox.json"),
            fork: false,
            xray: false,
            extra_exe: None,
            log_buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Ticket 07: fork-core driver (own binary, own temp file, version probe
    /// on every check). Stock rows never touch this.
    pub fn fork() -> Self {
        Self {
            child: None,
            config_path: std::env::temp_dir().join("manyproxy-fork.json"),
            fork: true,
            xray: false,
            extra_exe: None,
            log_buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Ticket 08: Xray sidecar driver (own binary, own temp file).
    pub fn xray() -> Self {
        Self {
            child: None,
            config_path: std::env::temp_dir().join("manyproxy-xray.json"),
            fork: false,
            xray: true,
            extra_exe: None,
            log_buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Ticket 08: user extra-core slot driver (own temp file for the
    /// passed-through config; the row's executable arrives per activation
    /// via [`EngineBackend::set_extra_exe`]).
    pub fn extra_slot() -> Self {
        Self {
            child: None,
            config_path: std::env::temp_dir().join("manyproxy-extra.json"),
            fork: false,
            xray: false,
            extra_exe: None,
            log_buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn resolve_binary(&self) -> Result<PathBuf, String> {
        if let Some(exe) = &self.extra_exe {
            return Ok(exe.clone());
        }
        if self.xray {
            resolve_xray_binary().map_err(|error| error.to_string())
        } else if self.fork {
            resolve_fork_binary().map_err(|error| error.to_string())
        } else {
            resolve_engine_binary().map_err(|error| error.to_string())
        }
    }

    /// Extra-core executables take their config as `-c <file>` (v1
    /// convention, documented at the slot); bundled cores use their own
    /// `run -c` shape.
    fn start_args(&self) -> Vec<String> {
        if self.extra_exe.is_some() {
            vec!["-c".to_string()]
        } else {
            vec!["run".to_string(), "-c".to_string()]
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
        // Ticket 08: xray/extra have no stable `check` CLI contract here, so
        // the write + ticket-08 validators gate them; runtime failures
        // surface via `poll` like every other crash.
        if self.xray || self.extra_exe.is_some() {
            self.write_config(config_json)?;
            return Ok(());
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
        // Unprivileged by construction: plain spawn, never elevated, no
        // admin for loopback use (spec boundary).
        // Ticket 13: stderr is piped (never nulled): a reader thread pumps
        // lines into the bounded buffer so the pipe cannot stall the
        // sidecar and the logs dialog shows live lines. The thread ends on
        // EOF when the sidecar exits.
        let mut child = std::process::Command::new(&binary)
            .args(self.start_args())
            .arg(&path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|error| format!("engine failed to start ({error})"))?;
        match child.stderr.take() {
            Some(pipe) => {
                let buffer = self.log_buffer.clone();
                std::thread::spawn(move || {
                    use std::io::BufRead as _;
                    let reader = std::io::BufReader::new(pipe);
                    for line in reader.lines() {
                        match line {
                            Ok(text) => push_pipe_line(&buffer, text),
                            Err(error) => {
                                push_pipe_line(&buffer, format!("log pipe unreadable ({error})"));
                                break;
                            }
                        }
                    }
                });
            }
            None => {
                push_pipe_line(
                    &self.log_buffer,
                    "log pipe unavailable (sidecar stderr was not piped)".to_string(),
                );
            }
        }
        self.child = Some(child);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        // Ticket 13: drop buffered (raw) lines with the sidecar so a later
        // start never replays stale output.
        if let Ok(mut lines) = self.log_buffer.lock() {
            lines.clear();
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

    /// Ticket 13: take buffered stderr lines. A poisoned lock surfaces one
    /// named error instead of raw state (fail-closed, never stale).
    fn drain_log_lines(&mut self) -> Vec<String> {
        match self.log_buffer.lock() {
            Ok(mut lines) => std::mem::take(&mut *lines),
            Err(_) => vec!["log pipe unreadable (log reader failed)".to_string()],
        }
    }

    fn set_extra_exe(&mut self, exe: PathBuf) {
        self.extra_exe = Some(exe);
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
    pending_logs: Vec<String>,
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

    /// Ticket 13: script one mock-sidecar stderr line for the next
    /// [`EngineBackend::drain_log_lines`].
    pub fn emit_log(&self, line: &str) {
        self.state.borrow_mut().pending_logs.push(line.to_string());
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

    fn drain_log_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.state.borrow_mut().pending_logs)
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
///
/// Ticket 08: four sidecars — the Xray sidecar bridges Xray-only rows over
/// loopback ingress listeners (paired config + tag-count cross-check), and
/// the single extra-core slot runs the user's executable unprivileged with
/// its config passed through. Custom full-config rows replace their owning
/// core's generated config wholesale.
pub struct EngineController {
    backend: Box<dyn EngineBackend>,
    /// Ticket 07: fork-core sidecar for exotic rows only.
    fork_backend: Box<dyn EngineBackend>,
    /// Ticket 08: Xray-core sidecar for Xray-only rows.
    xray_backend: Box<dyn EngineBackend>,
    /// Ticket 08: single user extra-core slot sidecar.
    extra_backend: Box<dyn EngineBackend>,
    /// Ticket 07: fork-binary probe seam (default: real resolution; tests
    /// inject present/missing without touching the filesystem).
    fork_probe: Box<dyn Fn() -> Result<PathBuf, EngineError>>,
    /// Ticket 08: xray-binary probe seam (same contract as the fork probe).
    xray_probe: Box<dyn Fn() -> Result<PathBuf, EngineError>>,
    /// Ticket 08: extra-executable probe seam (app dir only, per exe name).
    #[allow(clippy::type_complexity)]
    extra_probe: Box<dyn Fn(&str) -> Result<PathBuf, EngineError>>,
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

/// Ticket 08: fill row names into an xray probe failure.
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

/// Ticket 08: fill row names into an extra-exe probe failure.
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
            extra_backend: Box::new(RealEngineBackend::extra_slot()),
            fork_probe: Box::new(resolve_fork_binary),
            xray_probe: Box::new(resolve_xray_binary),
            extra_probe: Box::new(resolve_extra_binary),
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
    /// Ticket 08: xray/extra backends ride along stopped with missing
    /// probes; use [`EngineController::fake_quartet`] for their handles.
    #[cfg(test)]
    pub fn fake_pair() -> (Self, FakeHandle, FakeHandle) {
        let (controller, stock, fork, _, _) = Self::fake_quartet();
        (controller, stock, fork)
    }

    /// Ticket 08: test controller with handles to all four mock drivers.
    /// Fork/xray/extra probes default to missing (deterministic without
    /// touching the filesystem); override with the `set_*_probe` setters.
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
            extra_backend: Box::new(extra_backend),
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
            active: Vec::new(),
            status: EngineStatus::Stopped,
            last_error: None,
            last_config_json: None,
        };
        (controller, stock, fork, xray, extra)
    }

    /// Ticket 07: override the fork-binary probe (tests inject present /
    /// missing without filesystem access).
    #[cfg(test)]
    pub fn set_fork_probe(&mut self, probe: impl Fn() -> Result<PathBuf, EngineError> + 'static) {
        self.fork_probe = Box::new(probe);
    }

    /// Ticket 08: override the xray-binary probe (tests inject present /
    /// missing without filesystem access).
    #[cfg(test)]
    pub fn set_xray_probe(&mut self, probe: impl Fn() -> Result<PathBuf, EngineError> + 'static) {
        self.xray_probe = Box::new(probe);
    }

    /// Ticket 08: override the extra-executable probe (tests inject
    /// present / missing without filesystem access).
    #[cfg(test)]
    pub fn set_extra_probe(
        &mut self,
        probe: impl Fn(&str) -> Result<PathBuf, EngineError> + 'static,
    ) {
        self.extra_probe = Box::new(probe);
    }

    /// Start one config (idempotent); restarts the sidecar with the union.
    pub fn start_one(&mut self, config: &ProxyConfig) -> Result<(), EngineError> {
        let mut candidate = self.active.clone();
        if let Some(slot) = candidate.iter_mut().find(|row| row.id == config.id) {
            *slot = config.clone();
        } else {
            candidate.push(config.clone());
        }
        // Pure pre-check: a bad candidate never disturbs the running sidecars.
        validate_activation(&candidate).inspect_err(|error| {
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
        validate_activation(&candidate).inspect_err(|error| {
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
            self.xray_backend.stop();
            self.extra_backend.stop();
            self.status = EngineStatus::Stopped;
            self.last_config_json = None;
            return Ok(());
        }
        // Pure plan over the union: conflicts, schemas, chain rule,
        // custom-config uniqueness, bridge assignment. A bad set never
        // disturbs any running sidecar.
        let plan = validate_activation(&self.active).inspect_err(|error| {
            self.last_error = Some(error.to_string());
        })?;
        // Binary gates before touching anything: a missing sidecar binary
        // fails only its own rows; the other sidecars keep serving.
        if !plan.fork_rows.is_empty() {
            if let Err(error) = (self.fork_probe)() {
                let error = missing_fork_for(error, &plan.fork_names, plan.fork_xhttp);
                self.last_error = Some(error.to_string());
                return Err(error);
            }
        }
        if !plan.egresses.is_empty() {
            if let Err(error) = (self.xray_probe)() {
                let error = missing_xray_for(error, &plan.xray_names);
                self.last_error = Some(error.to_string());
                return Err(error);
            }
        }
        let extra_path: Option<PathBuf> = match &plan.extra {
            Some(row) => {
                let (exe, _) = validate_extra_row(row).inspect_err(|error| {
                    self.last_error = Some(error.to_string());
                })?;
                match (self.extra_probe)(&exe) {
                    Ok(path) => Some(path),
                    Err(error) => {
                        let error = missing_extra_for(error, &plan.extra_names);
                        self.last_error = Some(error.to_string());
                        return Err(error);
                    }
                }
            }
            None => None,
        };
        // Per-sidecar configs. Custom full-config rows replace their core
        // wholesale; anything else generates. Subsets of a validated union
        // cannot fail validation again, but the errors stay typed.
        let replacement = |core: OwningCore| {
            plan.replacements
                .iter()
                .find(|(owned, _)| *owned == core)
                .map(|(_, json)| json.clone())
        };
        // Bridge map for the main config: row ids plus `{chain-id}#hop{i}`
        // legs, exactly the key scheme `build_main_config` expects, with
        // the plan's ports (assigned over the union's taken ports) so
        // pointers and sidecar ingress agree by construction.
        let mut plan_bridges: HashMap<String, u16> = HashMap::new();
        for config in &plan.main_rows {
            if is_xray_protocol(&config.protocol) {
                if let Some(egress) = plan.egresses.iter().find(|egress| {
                    egress.owner_id == config.id && egress.main_tag == outbound_tag(&config.id)
                }) {
                    plan_bridges.insert(config.id.clone(), egress.bridge_port);
                }
            } else if is_custom_outbound_protocol(&config.protocol) {
                if let Some(egress) = plan.egresses.iter().find(|egress| {
                    egress.owner_id == config.id && egress.main_tag == outbound_tag(&config.id)
                }) {
                    plan_bridges.insert(config.id.clone(), egress.bridge_port);
                }
            } else if is_chain_protocol(&config.protocol) {
                if let Ok(params) = params_object(config) {
                    if let Ok(hops) = validate_chain_nodes(&config.name, &params, &|id| {
                        plan.main_rows
                            .iter()
                            .find(|row| row.id == id)
                            .map(|row| row.protocol.clone())
                    }) {
                        for (index, hop) in hops.iter().enumerate() {
                            if hop.kind == ChainHopKind::Xray && hop.ref_id.is_none() {
                                if let Some(egress) = plan.egresses.iter().find(|egress| {
                                    egress.main_tag
                                        == format!("{}-hop-{index}", outbound_tag(&config.id))
                                }) {
                                    plan_bridges.insert(
                                        format!("{}#hop{}", config.id, index),
                                        egress.bridge_port,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        let stock_gen: Option<String> = match replacement(OwningCore::Stock) {
            Some(json) => Some(json),
            None if plan.main_rows.is_empty() => None,
            None => Some(
                build_main_config(&plan.main_rows, &plan_bridges)
                    .inspect_err(|error| {
                        self.last_error = Some(error.to_string());
                    })?
                    .json,
            ),
        };
        let fork_gen: Option<String> = match replacement(OwningCore::Fork) {
            Some(json) => Some(json),
            None if plan.fork_rows.is_empty() => None,
            None => Some(
                generate(&plan.fork_rows)
                    .inspect_err(|error| {
                        self.last_error = Some(error.to_string());
                    })?
                    .json,
            ),
        };
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
        let extra_gen: Option<String> = match &plan.extra {
            Some(row) => Some(extra_config_json(row).inspect_err(|error| {
                self.last_error = Some(error.to_string());
            })?),
            None => None,
        };
        // Bridge bookkeeping cross-checks (load-bearing): the sidecar
        // tag counts must match the bridge count, and every pointer must
        // have survived into the serving main config (a stock replacement
        // that drops a pointer breaks the bridge explicitly).
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
        for (label, backend, gen) in [
            ("stock", &mut self.backend, &stock_gen),
            ("fork", &mut self.fork_backend, &fork_gen),
            ("xray", &mut self.xray_backend, &xray_gen),
            ("extra", &mut self.extra_backend, &extra_gen),
        ] {
            if let Some(json) = gen {
                if let Err(mut detail) = backend.check_config(json) {
                    if label == "fork" && plan.fork_xhttp {
                        detail = format!("{detail} ({XHTTP_KNOWN_BUGS})");
                    }
                    let error = EngineError::CheckFailed { detail };
                    self.last_error = Some(error.to_string());
                    return Err(error);
                }
            }
        }
        // Restart only populated sidecars; empty subsets stay down. A failed
        // start on one side never takes down the other side. The extra slot
        // driver is pointed at the active row's executable first (single
        // active row enforced by the plan).
        if let Some(path) = &extra_path {
            self.extra_backend.set_extra_exe(path.clone());
        }
        let mut first_error: Option<EngineError> = None;
        for (backend, gen) in [
            (&mut self.backend, &stock_gen),
            (&mut self.fork_backend, &fork_gen),
            (&mut self.xray_backend, &xray_gen),
            (&mut self.extra_backend, &extra_gen),
        ] {
            match gen {
                Some(json) => {
                    backend.stop();
                    if let Err(detail) = backend.start(json) {
                        let error = EngineError::SpawnFailed { detail };
                        self.last_error = Some(error.to_string());
                        first_error = Some(error);
                    }
                }
                None => backend.stop(),
            }
        }
        self.status = if self.backend.is_running()
            || self.fork_backend.is_running()
            || self.xray_backend.is_running()
            || self.extra_backend.is_running()
        {
            EngineStatus::Running
        } else {
            EngineStatus::Stopped
        };
        self.last_config_json = stock_gen
            .clone()
            .or_else(|| fork_gen.clone())
            .or_else(|| xray_gen.clone())
            .or(extra_gen.clone());
        if let Some(error) = first_error {
            return Err(error);
        }
        self.last_error = None;
        Ok(())
    }

    /// Crash detection: call on UI actions; reports the named cause once.
    /// Ticket 07: watches both sidecars, stock first.
    /// Ticket 08: watches all four sidecars, stock first.
    pub fn poll(&mut self) -> Option<String> {
        let cause = self
            .backend
            .poll_exit()
            .or_else(|| self.fork_backend.poll_exit())
            .or_else(|| self.xray_backend.poll_exit())
            .or_else(|| self.extra_backend.poll_exit())?;
        if self.active.is_empty() {
            return None;
        }
        self.status = EngineStatus::Crashed {
            cause: cause.clone(),
        };
        self.last_error = Some(format!("engine crashed ({cause})"));
        Some(cause)
    }

    /// Ticket 13: take buffered stderr lines from every sidecar (stock,
    /// fork, xray, extra). The UI ingests them into `LogStore` on every
    /// reload, so live lines appear without user clicks.
    pub fn drain_log_lines(&mut self) -> Vec<String> {
        let mut lines = self.backend.drain_log_lines();
        lines.extend(self.fork_backend.drain_log_lines());
        lines.extend(self.xray_backend.drain_log_lines());
        lines.extend(self.extra_backend.drain_log_lines());
        lines
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
        // Ticket 08: each row reports its serving sidecar. Xray rows ride
        // the xray bridge; custom rows follow their owning core; the extra
        // slot has its own process; chains are served by the main config.
        if is_extra_core_protocol(&row.protocol) {
            return self.extra_backend.is_running();
        }
        if is_custom_config_protocol(&row.protocol) {
            let core = params_object(row)
                .ok()
                .and_then(|params| owning_core_of(&row.name, &params).ok())
                .unwrap_or(OwningCore::Stock);
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
            let core = params_object(row)
                .ok()
                .and_then(|params| owning_core_of(&row.name, &params).ok())
                .unwrap_or(OwningCore::Stock);
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

    /// Ticket 09: OS-mutation proof seam — fingerprint of the controlled
    /// surface (active binds + generated-config hash). Equal before a
    /// session and after stop-all; different while serving.
    #[cfg_attr(not(test), allow(dead_code))]
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

/// Ticket 09: confirm-dialog title shared by the single- and start-all LAN
/// gates (spec boundary: LAN bind behind explicit warning + confirm).
pub const LAN_CONFIRM_TITLE: &str = "Expose proxy on the local network?";

/// Ticket 09: confirm-gate predicate — every non-loopback bind must pass an
/// explicit user confirm before start. Unparseable binds fail closed
/// (confirm required) instead of serving silently.
pub fn requires_lan_confirm(bind: &str) -> bool {
    !is_loopback_bind(bind)
}

/// Ticket 09: confirm-dialog copy for starting one LAN-bound config. Names
/// the config and the bind only — never params, keys, or secrets.
pub fn lan_confirm_text(name: &str, bind: &str) -> String {
    format!(
        "\"{name}\" will listen on {bind}, reachable by anyone on the local network. Start it anyway?"
    )
}

/// Ticket 09: confirm-dialog copy for start-all with LAN-bound rows.
/// `entries` are `(name, bind)` pairs; output names each pair, never
/// secrets. Empty input yields the loopback-only sentence (no confirm
/// needed upstream — callers check [`requires_lan_confirm`] first).
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

/// Ticket 09: recursive forbidden-key scan over a generated engine JSON
/// document (the OS-mutation structural proof). Rejects any `tun` / `dns`
/// section and any `set_system_proxy` key at any depth. Errors name the
/// offending key and its path only — never values.
#[cfg_attr(not(test), allow(dead_code))]
pub fn assert_no_system_mutation(config_json: &str) -> Result<(), String> {
    let document: serde_json::Value = serde_json::from_str(config_json)
        .map_err(|_| "generated config is not JSON".to_string())?;
    fn walk(node: &serde_json::Value, path: &str) -> Result<(), String> {
        match node {
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    let lowered = key.to_ascii_lowercase();
                    if lowered == "tun" || lowered == "dns" || lowered == "set_system_proxy" {
                        return Err(format!("forbidden key \"{key}\" at {path}"));
                    }
                    let child = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    walk(value, &child)?;
                }
                Ok(())
            }
            serde_json::Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    walk(item, &format!("{path}[{index}]"))?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    walk(&document, "$")
}

/// Ticket 09: fingerprint of everything the app controls across a session
/// (active `bind:port` set + hash of the last generated engine config).
/// Snapshot before a session must equal the snapshot after stop-all; during
/// a session it differs. OS-level proxy/DNS/route bytes still need a live
/// run (see QUICKSTART-RUN.md) — this seam proves the app side neither
/// retains residue nor mutates anything across the session. Carries binds
/// and a hash only, never params or secrets.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub struct SessionFingerprint {
    pub binds: Vec<String>,
    pub config_hash: u64,
}

#[cfg_attr(not(test), allow(dead_code))]
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

    /// Ticket 13: mock sidecars emit stderr lines; the controller drains
    /// every sidecar, the exit is noted once, the pipe closes cleanly
    /// (later drains are empty, later polls are silent), nothing panics.
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
            Some("signal: segmentation fault"),
            "exit noted once"
        );
        assert!(controller.poll().is_none(), "cause reported once");
        assert!(
            controller.drain_log_lines().is_empty(),
            "pipe closed cleanly, no panic"
        );
    }

    /// Ticket 13: the pipe-reader buffer is bounded (oldest lines drop when
    /// full, memory never grows without bound).
    #[test]
    fn log_pipe_buffer_stays_bounded() {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        for index in 0..(LOG_PIPE_BUFFER + 50) {
            push_pipe_line(&buffer, format!("line {index}"));
        }
        let lines = buffer.lock().expect("lock works");
        assert_eq!(lines.len(), LOG_PIPE_BUFFER, "capped, got {}", lines.len());
        assert!(
            lines.first().is_some_and(|line| line == "line 50"),
            "oldest dropped first"
        );
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

    // -- Ticket 09: hardening + release proof --------------------------------

    /// Confirm gate blocks every non-loopback bind (and fails closed on
    /// garbage); loopback stays quiet.
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

    /// Confirm copy names the config + bind and carries no secrets, even
    /// when the row's params are secret-laden.
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

    /// OS-mutation proof: fingerprint before a session equals the fingerprint
    /// after stop-all, differs while serving; the generated config carries
    /// no tun/dns/set_system_proxy anywhere; the fingerprint itself leaks
    /// no secret material.
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

    /// The mutation scan rejects each forbidden key at top level and nested,
    /// naming the key (never values); clean documents pass.
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

    /// Release prohibitions, machine-checked half: every missing-binary error
    /// states the never-downloads contract (no silent binary replacement).
    #[test]
    fn missing_binaries_state_never_downloads() {
        let stock = EngineError::MissingBinary {
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

    /// Binary resolution searches explicit directories only: no download, no
    /// elevation, and the searched list is reported (empty roots fail typed).
    #[test]
    fn binary_resolution_never_elevates_or_downloads() {
        let error = resolve_engine_binary_in(None, "").expect_err("empty search roots fail");
        assert!(
            matches!(error, EngineError::MissingBinary { .. }),
            "typed failure, got {error}"
        );
        assert!(!error.to_string().is_empty());
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

    // -- Ticket 08: Xray sidecar + custom + chaining (TDD at the
    // bridge-bookkeeping/validation seam, FakeEngineBackend pattern) --------

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

    /// Scenario 1 (bridge): an Xray-only VLESS row beside a stock row
    /// yields a loopback pointer in the main config and a paired sidecar
    /// config whose ingress/egress agree with the bookkeeping.
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
        assert!(main.requires_xray);
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
        assert_eq!(sidecar.ingress_tags, vec![xray_ingress_tag("2")]);
        assert_eq!(sidecar.egress_tags, vec![xray_egress_tag("2")]);
        let xray = sidecar.value();
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
        assert_eq!(main.isolation.len(), 2, "each endpoint isolated");
        for entry in &main.isolation {
            let rule = value["route"]["rules"]
                .as_array()
                .expect("rules")
                .iter()
                .find(|rule| rule["inbound"] == json!(entry.inbound_tag))
                .expect("rule per endpoint");
            assert_eq!(rule["outbound"], json!(entry.outbound_tag));
        }
    }

    /// Bridge bookkeeping: tag-count skew fails with the research-02
    /// breakage signature, and a stock replacement that drops a pointer
    /// fails start before any sidecar is touched.
    #[test]
    fn bridge_count_mismatch_fails_start_with_signature() {
        let error = validate_bridge_counts(1, 0, 1, "test").expect_err("skew fails");
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
            .expect_err("dropped pointer breaks the bridge");
        let text = error.to_string();
        assert!(
            matches!(error, EngineError::XrayBridgeMismatch { .. }),
            "typed, got {text}"
        );
        assert!(
            text.contains(XRAY_BRIDGE_COUNT_SIGNATURE),
            "signature, got {text}"
        );
        assert!(!stock.is_running(), "stock never started");
        assert!(!xray.is_running(), "xray never started");
        assert_eq!(stock.check_calls(), 0, "no check ran");
    }

    /// Loopback hygiene: an egress pointing at loopback fails naming the
    /// row instead of recirculating sidecar traffic.
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

    /// Custom outbound JSON merges verbatim into the owning core's config;
    /// a broken snippet blocks start naming the offending field while a
    /// running stock endpoint keeps serving.
    #[test]
    fn custom_outbound_merges_and_broken_names_field() {
        let rows = vec![custom_fixture(
            "9",
            "CustomOne",
            custom_stock_params(),
            10809,
        )];
        let generated = generate(&rows).expect("custom merges");
        assert!(!generated.requires_fork);
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

    /// Custom full-config rows replace their owning core's generated config;
    /// a second replacement for the same core names `core`.
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

    /// Extra-core slot: app-dir-only resolution rejects traversal, a
    /// missing exe names the row + file, the single slot holds one row,
    /// and the active row's config reaches its sidecar unprivileged.
    #[test]
    fn extra_slot_appdir_unprivileged_single() {
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
        let error = controller
            .start_one(&rival)
            .expect_err("single slot holds one row");
        assert!(
            error.to_string().contains("only one extra-core"),
            "got {error}"
        );
    }

    /// Egress-only rule: Extra/Custom/Xray hops mid-chain are rejected with
    /// the contract error; the violation names the chain, never secrets.
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

        // Id reference to an Xray row mid-chain (full-set validation path).
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

        // Custom mid-chain via id reference.
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

    /// A valid stock→stock chain materializes private hops with a detour
    /// spine and routes its own inbound to the first hop.
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
            .find(|rule| rule["inbound"] == json!(inbound_tag("8")))
            .expect("chain rule present");
        assert_eq!(rule["outbound"], json!(hop0));
    }

    /// Xray-last chains are allowed: the terminating hop bridges to the
    /// sidecar while earlier hops stay in the main config. An id reference
    /// reuses its row's bridge leg (one ingress, two loopback pointers —
    /// no cross-endpoint leak: both legs exit via the referenced egress).
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

    /// Scenario 1 (isolation): Xray-only VLESS + custom-JSON + stock rows
    /// start together with per-endpoint isolation on the mocks.
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

    /// A missing xray binary fails only Xray rows; stock keeps serving.
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

    /// Xray resolution names the expected binary and searched dirs.
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

    /// Bridge ports skip taken user ports; exhaustion is typed.
    #[test]
    fn bridge_ports_skip_taken_and_exhaust_explicitly() {
        let taken: HashSet<u16> = [11801].into_iter().collect();
        let ports = assign_bridge_ports(&["a".to_string()], &taken).expect("assigns");
        assert_eq!(ports["a"], 11802, "taken bridge port skipped");
        let full: HashSet<u16> = (XRAY_BRIDGE_PORT_START..=XRAY_BRIDGE_PORT_END).collect();
        let error = assign_bridge_ports(&["a".to_string()], &full).expect_err("exhaustion fails");
        assert!(matches!(error, EngineError::InvalidGenerated { .. }));
    }

    /// Chain shape errors name `nodes`, never params contents.
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
}
