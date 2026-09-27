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

/// Base name of the sidecar binary (resolved in the app dir).
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

/// Ticket 36: loopback clash-controller range for per-core traffic
/// metering. Disjoint from endpoints (10801-10999 + 18081 spillover),
/// the xray bridge (11801-11999), and row binds.
pub const CLASH_PORT_START: u16 = 18501;
pub const CLASH_PORT_END: u16 = 18999;

/// Ticket 36: one clash controller identity per serving core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClashTarget {
    pub core: String,
    pub port: u16,
    pub secret: String,
}

/// Ticket 36: loopback bearer secret (16 hex chars from pid + wall clock
/// + an atomic nonce — never persisted, regenerated per start).
fn clash_secret(core: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    static NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut hasher = DefaultHasher::new();
    (
        std::process::id(),
        std::time::SystemTime::now(),
        NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        core,
    )
        .hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Ticket 36: first free loopback port in the clash range (single-shot
/// binds, no retry sleeps — the caller probe-binds the pick for real).
fn find_clash_port() -> Option<u16> {
    (CLASH_PORT_START..=CLASH_PORT_END)
        .find(|port| std::net::TcpListener::bind(format!("127.0.0.1:{port}")).is_ok())
}

/// Ticket 36: splice a loopback-only clash controller into a generated
/// sidecar config (shape proven live against stock 1.14.2 `check`).
/// Refuses non-JSON input typed (never emits a half-document). Never
/// called on user-owned custom JSON (generated cores only). No
/// re-validation here: `generate()` already validated the pre-image and
/// the added `experimental` block cannot affect route structure.
fn inject_clash_api(json: &str, port: u16, secret: &str) -> Result<String, EngineError> {
    let invalid = |detail: &str| EngineError::InvalidGenerated {
        detail: detail.to_string(),
    };
    let mut document: serde_json::Value = serde_json::from_str(json)
        .map_err(|_| invalid("sidecar config is not JSON (clash injection refused)"))?;
    let obj = document
        .as_object_mut()
        .ok_or_else(|| invalid("sidecar config is not an object (clash injection refused)"))?;
    obj.insert(
        "experimental".to_string(),
        serde_json::json!({"clash_api": {
            "external_controller": format!("127.0.0.1:{port}"),
            "secret": secret,
        }}),
    );
    serde_json::to_string(&document)
        .map_err(|_| invalid("clash-injected config does not serialize"))
}

/// Ticket 36: listen (bind, port) pairs served by a sidecar config, read
/// best-effort for the re-apply probe skip. Garbage fails open to an
/// empty set (probe everything — never skip on a guess).
fn served_listen_ports(json: &str) -> Vec<(String, u16)> {
    let mut out = Vec::new();
    let Ok(document) = serde_json::from_str::<serde_json::Value>(json) else {
        return out;
    };
    let Some(inbounds) = document
        .get("inbounds")
        .and_then(serde_json::Value::as_array)
    else {
        return out;
    };
    for node in inbounds {
        let port = node
            .get("listen_port")
            .and_then(serde_json::Value::as_u64)
            .filter(|port| (1..=65_535).contains(port))
            .map(|port| port as u16);
        let bind = node
            .get("listen")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        if let (Some(bind), Some(port)) = (bind, port) {
            out.push((bind, port));
        }
    }
    out
}

/// Ticket 36: recover a clash target from a served sidecar config (the
/// injected `experimental.clash_api` block). Anything else (user-owned
/// replacements, extra slots, garbage) yields nothing — those cores stay
/// untracked.
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

/// Ticket 28: the five kind predicates share one alias-table lookup.
fn matches_protocol_aliases(protocol: &str, aliases: &[&str]) -> bool {
    aliases.contains(&protocol.trim().to_ascii_lowercase().as_str())
}

/// Ticket 08: Xray-only VLESS rows (research 02 semantics live here, never
/// in the stock schema).
pub fn is_xray_protocol(protocol: &str) -> bool {
    matches_protocol_aliases(
        protocol,
        &["xray", "xray-vless", "xray_vless", "vless-xray"],
    )
}

/// Ticket 08: single-outbound custom JSON rows (merged into the owning
/// core's generated config; `core` picks stock/fork/xray).
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

/// Ticket 08: full-config custom rows (replace the owning core's generated
/// config wholesale; the user owns its inbounds).
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

/// Ticket 08: user extra-core executable slot (ticket 27: N named slots —
/// one sidecar per active row, each serving its own endpoint).
pub fn is_extra_core_protocol(protocol: &str) -> bool {
    matches_protocol_aliases(
        protocol,
        &["extra", "extra-core", "extra_core", "extracore"],
    )
}

/// Ticket 08: chained-outbound rows (inline hops, or id references within
/// the same sidecar config).
pub fn is_chain_protocol(protocol: &str) -> bool {
    matches_protocol_aliases(
        protocol,
        &["chain", "chained", "chain-outbound", "chain_outbound"],
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

/// Canonical sing-box outbound/endpoint type for the ticket-05 standard set
/// plus ticket-27 natives (AnyTLS, SSH).
/// Anything else (naive, socks, http, unknown) is an
/// [`EngineError::UnsupportedProtocol`] owned by a later ticket.
/// Ticket 28: kept parallel to the import/store/health tables — this maps
/// to sing-box outbound types for codegen, the others to stored names,
/// grammar acceptance, and probe capability.
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
        // Ticket 27: native sing-box outbounds (stock sidecar, own endpoint).
        "anytls" => Some("anytls"),
        "ssh" => Some("ssh"),
        _ => None,
    }
}

/// Ticket 27: native SSH rows (sing-box `ssh` outbound, stock sidecar).
/// Ticket 28: same alias table as the other kind predicates (ASCII case
/// folding matches the old `eq_ignore_ascii_case` exactly).
#[cfg_attr(not(test), allow(dead_code))]
pub fn is_ssh_protocol(protocol: &str) -> bool {
    matches_protocol_aliases(protocol, &["ssh"])
}

/// Ticket 27: standalone Noise rows (fork sidecar via [`needs_fork`]).
/// Amnezia Noise ≠ Xray Noise (research 02): Noise rows carry their own
/// `password`/`key` and must never mix AWG obfuscation fields.
pub fn is_noise_protocol(protocol: &str) -> bool {
    matches_protocol_aliases(protocol, &["noise"])
}

/// Inbound tag for a library row (unique: ids are never reused).
/// Ticket 28: shares the prefix join with [`outbound_tag`] (one sanitize
/// pass; the `in-`/`out-` prefixes themselves are load-bearing routing).
pub fn inbound_tag(id: &str) -> String {
    tagged("in", id)
}

/// Outbound/endpoint tag for a library row.
pub fn outbound_tag(id: &str) -> String {
    tagged("out", id)
}

fn tagged(prefix: &str, id: &str) -> String {
    format!("{prefix}-{}", sanitize_tag(id))
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
    /// Ticket 37: attached routing-preset failure (names the row and the
    /// rule index + field; never echoes rule content).
    BadPreset { name: String, detail: String },
    /// The sidecar process could not be spawned.
    SpawnFailed { detail: String },
    /// Ticket 23: a local endpoint is already held (by another process or a
    /// sibling sidecar). Names the bind, port, and row only — never secrets.
    PortUnavailable {
        bind: String,
        port: u16,
        name: String,
    },
    /// Ticket 23: a start would serve a non-loopback bind without an explicit
    /// LAN confirm. Names the rows + binds only — never secrets.
    LanConfirmRequired { names: String },
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
                // Ticket 29: `check` stderr arrives with ANSI color escapes
                // (the raw `[31mFATAL[0m` toast); the surfaced error keeps the
                // text and drops the codes.
                write!(
                    f,
                    "engine config check failed ({})",
                    strip_ansi_codes(detail)
                )
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
/// fields on non-AWG rows, Xray noise/finalmask semantics on AWG rows, and
/// ticket-27 Noise guards — AWG obfuscation on Noise rows plus Xray-noise
/// semantics on Noise or stock rows — fail here with explicit errors — never
/// silently.
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

/// Validate one AWG row against the explicit version matrix: declared
/// version, required WireGuard credentials, then per-version field
/// membership (cross-version and unknown fields name the field).
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

/// Ticket 27: validate one standalone Noise row (fork sidecar): server +
/// port shape plus one noise secret (`password`/`key`), verbatim
/// passthrough otherwise. AWG-vs-Noise confusion (AWG obfuscation fields on
/// a Noise row) fails here explicitly — never silently.
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

/// Ticket 27: validate one native SSH row (stock sidecar): server shape plus
/// user + one secret (password or private key). Errors name the field only.
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

/// Ticket 27: validate one native AnyTLS row (stock sidecar): password
/// required (sing-box `anytls` authenticates by password).
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

/// True for rows the stock binary cannot serve: AWG variants, standalone
/// Noise, plus XHTTP transports on the VLESS family. Anything else (including
/// malformed params) is not fork-routed: malformed rows fail at [`generate`]
/// with their own named error.
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

/// Ticket 29: VLESS `encryption` routing (live-proven 2026-09-26 against
/// stock sing-box 1.14.2 and xray-core 26.3.27):
/// - stock strict-decodes and rejects ANY `encryption` key on vless
///   outbounds — even `"none"` (`unknown field "encryption"`);
/// - xray accepts `"none"` in `users[].encryption` but validates real values
///   (`unsupported "encryption"` for anything it cannot parse; genuine client
///   values are full key strings minted by `xray vlessenc`).
///
/// Absent/blank/`"none"` drops the key and stays stock, while a real value
/// routes the row through the xray sidecar bridge with fields passthrough
/// (xray `streamSettings.packetEncoding` is camelCase, matching intake).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VlessEncryption {
    /// No key, null, blank, or `"none"` (ASCII case-insensitive): stock.
    Stock,
    /// A real value: xray bridge leg.
    Xray,
    /// Present but not a string: typed error.
    BadType,
}

/// Ticket 29: classify one standard VLESS row's `encryption` param.
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

/// Ticket 29: true for standard `vless` rows carrying a real `encryption`
/// value (served through the xray sidecar bridge, never stock).
fn vless_needs_xray(protocol: &str, params: &serde_json::Map<String, serde_json::Value>) -> bool {
    canonical_type(protocol) == Some("vless")
        && vless_encryption_of(params) == VlessEncryption::Xray
}

/// Ticket 29: row-level form of [`vless_needs_xray`] (malformed params are
/// not xray-routed: they fail with their own named error, like every other
/// routing predicate here).
fn config_vless_needs_xray(config: &ProxyConfig) -> bool {
    serde_json::from_str::<serde_json::Value>(&config.params)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .is_some_and(|map| vless_needs_xray(&config.protocol, &map))
}

/// Ticket 29: malformed `encryption` on a VLESS row (names the field and the
/// xray sidecar; never echoes the value).
fn bad_vless_encryption(name: &str) -> EngineError {
    EngineError::BadCustom {
        name: name.to_string(),
        field: "encryption".to_string(),
        detail: "vless \"encryption\" must be \"none\" (stock, key dropped) or a string value served via the xray sidecar".to_string(),
    }
}

/// Ticket 08: true for rows served through the Xray sidecar bridge: Xray
/// rows plus xray-core custom outbounds. Malformed params are not
/// xray-routed: they fail at [`generate`]/[`validate_activation`] with
/// their own named error.
/// Ticket 29: standard VLESS rows carrying a real `encryption` value ride
/// the same bridge (stock sing-box rejects the `encryption` key outright).
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

/// Ticket 28: one JSON port-shape check for the custom/xray validators
/// (numbers and digit strings in 1..=65535; presence rules stay per-shape).
fn json_port_valid(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Number(number) => number
            .as_u64()
            .is_some_and(|port| (1..=65_535).contains(&port)),
        serde_json::Value::String(text) => {
            text.trim().parse::<u16>().ok().is_some_and(|port| port > 0)
        }
        _ => false,
    }
}

/// Ticket 28: trimmed non-empty string field — one read shape for every
/// params validator (presence errors stay per-caller, so taxonomies never
/// change).
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

/// Ticket 28: true when any of `keys` holds a non-blank string.
fn any_field(params: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> bool {
    keys.iter().any(|key| str_field(params, key).is_some())
}

/// Ticket 08: validate one Xray-only VLESS row (server/uuid shape checked
/// here; every other Xray-only field passes through verbatim into the
/// sidecar config).
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
    // Ticket 23 SEC-HIGH: the replacement is user JSON served by a bundled
    // sidecar, so the recursive system-mutation scan runs here — not just in
    // tests. A nested `tun`/`dns`/`set_system_proxy` at any depth names its
    // key and path (never values).
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
    // Ticket 29: inline vless hops carrying a real `encryption` value are
    // xray-bridged (stock rejects the key outright); a non-string value
    // fails typed here instead of at `check` time.
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
    /// VLESS-shaped row params, or xray-shaped custom params when
    /// [`XrayEgress::xray_shaped`] is set.
    pub params: serde_json::Map<String, serde_json::Value>,
    pub xray_shaped: bool,
    pub bridge_port: u16,
    /// Ticket 23 LOW: the bridge-map key for this leg (row id, or
    /// `{chain-id}#hop{i}` for inline xray chain hops) — recorded at plan
    /// time so `apply` reuses the bookkeeping instead of re-deriving it with
    /// swallowed errors.
    pub bridge_key: String,
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
            let server =
                str_field(&egress.params, "server").ok_or_else(|| EngineError::BadParams {
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
            // Ticket 29: `packetEncoding` stays camelCase per the xray schema
            // (`streamSettings.packetEncoding`, accepted by `xray run -test`).
            for key in [
                "network",
                "security",
                "sni",
                "pbk",
                "sid",
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
    /// Rows served by the fork sidecar: AWG/Noise/XHTTP plus fork-core customs.
    pub fork_rows: Vec<ProxyConfig>,
    /// Xray sidecar legs (row VLESS, xray-core customs, xray-last chain hops).
    pub egresses: Vec<XrayEgress>,
    /// Ticket 27: N named extra-core slot rows (each serves its own endpoint
    /// in its own sidecar; replaces the ticket-08 singleton).
    pub extras: Vec<ProxyConfig>,
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
    // Ticket 23 LOW: a duplicated id names the id (rows are keyed by id
    // everywhere downstream, so two rows sharing one never serve sanely).
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
        // Ticket 23: loopback aliases compare equal (localhost ≡ 127.0.0.1).
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
    // Classify + validate every row; collect bridge keys in activation order.
    let mut main_rows = Vec::new();
    let mut fork_rows = Vec::new();
    let mut egresses: Vec<XrayEgress> = Vec::new();
    // Ticket 27: N named extra-core slots (no singleton cap).
    let mut extras: Vec<ProxyConfig> = Vec::new();
    let mut replacements: Vec<(OwningCore, String)> = Vec::new();
    let mut bridge_keys: Vec<String> = Vec::new();
    for config in configs {
        let params = params_object(config)?;
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
        } else if is_xray_protocol(&config.protocol) || vless_needs_xray(&config.protocol, &params)
        {
            // Ticket 29: standard VLESS rows carrying a real `encryption`
            // value validate like xray rows and ride the same bridge (stock
            // sing-box rejects the `encryption` key outright).
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
            // after `active` moves. Ticket 27: Noise is fork-routed (never
            // UnsupportedProtocol); SSH/AnyTLS are canonical natives.
            if canonical_type(&config.protocol).is_none()
                && !is_awg_protocol(&config.protocol)
                && !is_noise_protocol(&config.protocol)
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
                // Ticket 29: VLESS-`encryption` rows are bridge legs
                // (egress-only like xray rows, but classified Standard by
                // protocol alone), so a mid-chain reference fails here.
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
    // Bridge ports: deterministic over the union's taken user ports.
    let taken_ports: HashSet<u16> = configs
        .iter()
        .filter(|row| !is_custom_config_protocol(&row.protocol))
        .map(|row| row.port)
        .collect();
    let ports = assign_bridge_ports(&bridge_keys, &taken_ports)?;
    for config in configs {
        // Ticket 29: VLESS `encryption` rows ride the xray bridge too.
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
    // Ticket 29: xray itself rejects unparsable values at load (`unsupported
    // "encryption""); a non-string never reaches it — typed here instead.
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
///
/// Ticket 37: rows without an attached preset generate exactly as before;
/// use [`generate_with_presets`] to merge per-row preset rules. Production
/// serves through the controller (which always threads its preset map);
/// this entry stays for preset-less callers and the test suite.
#[cfg_attr(not(test), allow(dead_code))]
pub fn generate(configs: &[ProxyConfig]) -> Result<GeneratedConfig, EngineError> {
    generate_with_presets(configs, &HashMap::new())
}

/// Ticket 37: [`generate`] plus per-row routing presets. `presets` maps
/// preset id → ordered route-rule objects (see [`Store::preset_rules_map`]);
/// each attached row's rules merge ahead of its own isolation rule. An empty
/// map is byte-identical to [`generate`].
pub fn generate_with_presets(
    configs: &[ProxyConfig],
    presets: &HashMap<String, Vec<serde_json::Value>>,
) -> Result<GeneratedConfig, EngineError> {
    if configs.is_empty() {
        return Err(EngineError::NoActiveConfigs);
    }
    let mut taken: HashMap<(String, u16), &str> = HashMap::new();
    for config in configs {
        // Ticket 32: custom full-config rows are user-managed (their bind
        // is documentation, not an endpoint we serve), so they never
        // conflict — exactly like `validate_activation`.
        if is_custom_config_protocol(&config.protocol) {
            continue;
        }
        // Ticket 23: loopback aliases compare equal (localhost ≡ 127.0.0.1).
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
    // Ticket 23 LOW: bridge preview/apply agreement — custom full-config rows
    // are user-managed (their bind is documentation), so their ports never
    // reserve bridge space, exactly like `validate_activation`.
    let taken_ports: HashSet<u16> = configs
        .iter()
        .filter(|row| !is_custom_config_protocol(&row.protocol))
        .map(|row| row.port)
        .collect();
    let bridges = assign_bridge_ports(&keys, &taken_ports)?;
    build_main_config(configs, &bridges, presets)
}

/// Ticket 37: resolve one row's attached preset into merge-ready route rules.
/// Route actions point at the row's serving tag (route to row outbound).
/// Direct passes through, and the Throne `block` spelling is served as
/// `reject` (live-proven: stock sing-box 1.14.2 has no `block` action).
/// Rules without an `inbound` scope to the row's inbound. Presets are
/// per-row policy, so an unscoped rule must never leak onto sibling rows
/// sharing the sidecar; an explicit `inbound` is kept verbatim. Stored rules
/// re-validate here (index and field, never values). A dangling preset id
/// fails loudly instead of serving preset-less.
fn preset_rules_for(
    config: &ProxyConfig,
    serving_tag: &str,
    inbound_tag: &str,
    presets: &HashMap<String, Vec<serde_json::Value>>,
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
        let obj = rule
            .as_object()
            .ok_or_else(|| bad(format!("rule {index}: must be a JSON object")))?;
        let action = obj
            .get("action")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| bad(format!("rule {index}: field \"action\" is required")))?;
        if !crate::preset::VALID_PRESET_ACTIONS.contains(&action) {
            return Err(bad(format!(
                "rule {index}: field \"action\" must be one of {}",
                crate::preset::VALID_PRESET_ACTIONS.join(", ")
            )));
        }
        if action == "route" {
            if let Some(outbound) = obj.get("outbound") {
                if !outbound.is_string() {
                    return Err(bad(format!(
                        "rule {index}: field \"outbound\" must be a string"
                    )));
                }
            }
        } else if obj.contains_key("outbound") {
            return Err(bad(format!(
                "rule {index}: field \"outbound\" needs action \"route\""
            )));
        }
        let mut merged = obj.clone();
        // Normalize the served spelling (`block` → `reject`); route/direct
        // spellings are already what the sidecar accepts.
        merged.insert(
            "action".to_string(),
            serde_json::Value::String(crate::preset::served_action(action).to_string()),
        );
        if action == "route" {
            merged.insert(
                "outbound".to_string(),
                serde_json::Value::String(serving_tag.to_string()),
            );
        }
        if !merged.contains_key("inbound") {
            merged.insert(
                "inbound".to_string(),
                serde_json::Value::Array(vec![serde_json::Value::String(inbound_tag.to_string())]),
            );
        }
        out.push(serde_json::Value::Object(merged));
    }
    Ok(out)
}
/// rows and xray-core customs; `{id}#hop{i}` for xray-last inline chain
/// hops). Both [`generate`] and [`validate_activation`] derive keys this
/// way so one shared port map keeps pointers and ingress in agreement.
fn bridge_keys_for(configs: &[ProxyConfig]) -> Result<Vec<String>, EngineError> {
    let mut keys = Vec::new();
    for config in configs {
        // Ticket 29: VLESS `encryption` rows take bridge ports too.
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

/// Ticket 05/07 standard node builder (extracted verbatim for chain-hop
/// reuse): stored params → sing-box outbound/endpoint node. Returns the
/// node, whether it belongs in `endpoints[]` (wireguard), and whether it
/// needs the fork sidecar.
fn build_standard_node(
    config: &ProxyConfig,
    params: &mut serde_json::Map<String, serde_json::Value>,
    out_tag: &str,
) -> Result<(serde_json::Value, bool, bool), EngineError> {
    str_field(params, "server").ok_or(EngineError::BadParams {
        name: config.name.clone(),
        field: "server",
    })?;
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
    // Ticket 27: standalone Noise validates its own shape and becomes a
    // fork `noise` outbound (fork sidecar path); native SSH/AnyTLS validate
    // their credentials and stay stock outbounds.
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
    // Ticket 29: stock sing-box rejects ANY `encryption` key on vless
    // outbounds, so absent/blank/`"none"` drops the key here (stays stock).
    // A real value never reaches stock emission (routing sends it xray-ward)
    // and fails typed if it does; a non-string fails typed naming the field.
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
    // Ticket 32: v2ray-flat stored params → stock sing-box 1.14.2 schema
    // (live-probed: strict decode rejects top-level `security`/`sni`/
    // `path`/… on outbounds). TLS intent folds into a `tls` object,
    // stream settings into a `transport` object; keys meaningless for the
    // protocol are dropped (other clients ignore them too); keys
    // meaningful but unmappable fail named (never a silently wrong
    // connection). Anything unrecognized passes through for the engine's
    // own `check` to judge loudly.
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

/// Ticket 32: first non-blank string among `keys` (exact-case, matching
/// every other params reader here); removes ALL listed keys whether or
/// not one matched, so alias spellings never leak into the emission.
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

/// Ticket 32: `{"field": ...}` error naming the offending stored field
/// (never values, keys, or tokens).
fn bad_option(name: &str, field: &str, detail: &str) -> EngineError {
    EngineError::BadCustom {
        name: name.to_string(),
        field: field.to_string(),
        detail: detail.to_string(),
    }
}

/// Ticket 32: split a flat `alpn` value (`"h2"`, `"h2,http/1.1"`) into the
/// string array the sing-box `tls` object wants. Empty means omit.
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

/// Ticket 32: one sing-box `tls` outbound object. `server_name` falls back
/// to the row's server; `reality` carries the public key (short id only
/// when the row sets one). The uTLS fingerprint lives in the `utls`
/// object — live-probed: 1.14.2 strict-decodes and rejects a top-level
/// `fingerprint` on the tls object.
fn tls_object(
    server: &str,
    server_name: Option<String>,
    fingerprint: Option<String>,
    alpn: Vec<serde_json::Value>,
    reality: Option<(String, Option<String>)>,
) -> serde_json::Value {
    let mut tls = serde_json::Map::with_capacity(5);
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
    serde_json::Value::Object(tls)
}

/// Ticket 32: TLS intent behind one row's flat `security` marker.
enum TlsIntent {
    Off,
    On { reality: bool },
}

/// Ticket 32: fold one row's flat TLS keys (`security`/`sni`/`fp`/`alpn`/
/// `pbk`/`sid`/`peer`) into a sing-box `tls` object, or drop them when the
/// intent is off. `default_on` serves the protocols that are definitionally
/// TLS (trojan, tuic, hysteria, hysteria2, anytls); `mandatory` rejects an
/// explicit `security=none` on protocols where plaintext cannot work.
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
    match intent {
        TlsIntent::Off => {
            // No TLS: key material for it is meaningless here.
            Ok(())
        }
        TlsIntent::On { reality: false } => {
            params.insert(
                "tls".to_string(),
                tls_object(server, server_name, fingerprint, alpn, None),
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
                ),
            );
            Ok(())
        }
    }
}

/// Ticket 32: fold one row's flat stream keys (`network`/`path`/`host`/…)
/// into a sing-box `transport` object (`None` for plain TCP). Only shapes
/// the live probe proved 1.14.2 accepts are emitted; anything else names
/// `network` instead of emitting a guess.
fn apply_transport(
    name: &str,
    params: &mut serde_json::Map<String, serde_json::Value>,
) -> Result<(), EngineError> {
    // A native `transport` object wins outright; the flats beside it are
    // redundant intake spellings, not a second transport.
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
    take_flat(params, &["headers"]);
    let service_name = take_flat(params, &["serviceName", "service_name"]).or(path.clone());
    let method = take_flat(params, &["method"]);
    let transport = match network.to_ascii_lowercase().as_str() {
        "" | "tcp" | "raw" | "none" => None,
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
            // v2ray grpc names the service `serviceName`; some links use
            // `path` (or `host`) for the same slot.
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
            // Fork-routed (stock has no xhttp): the fork judges the shape;
            // the pointer carries the intake slots plus any mode pin.
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

/// Ticket 32: keys that carry no meaning for protocols without TLS or
/// stream transports (intake spellings from other protocols' links).
/// Dropped so provider junk never fails `check`; anything meaningful but
/// unmappable errors at its own site instead of here. `keep` spares a key
/// that is native to the caller (the shadowsocks cipher `method`).
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
    ] {
        if !keep.contains(&key) {
            params.remove(key);
        }
    }
}

/// Ticket 32: v2ray-flat stored params → stock sing-box 1.14.2 outbound
/// schema (see [`apply_tls`] / [`apply_transport`] for the contract).
fn translate_outbound(
    canonical: &str,
    name: &str,
    params: &mut serde_json::Map<String, serde_json::Value>,
) -> Result<(), EngineError> {
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
        }
        "vmess" => {
            // Legacy links carry a `tls` bool plus a cipher `security`;
            // new-style links carry tls-ness in `security` and the cipher
            // in `encryption`. The bool decides which grammar owns the row.
            if params.get("tls").is_some_and(|tls| tls.is_boolean()) {
                let on = params
                    .remove("tls")
                    .and_then(|tls| tls.as_bool())
                    .unwrap_or(false);
                if on {
                    let server_name = take_flat(params, &["sni"]);
                    let fingerprint = take_flat(params, &["fp", "fingerprint"]);
                    let alpn = split_alpn(take_flat(params, &["alpn"]));
                    params.insert(
                        "tls".to_string(),
                        tls_object(&server, server_name, fingerprint, alpn, None),
                    );
                }
                // `security` stays: it is the sing-box packet cipher here.
                take_flat(params, &["pbk", "sid", "peer"]);
            } else {
                let cipher =
                    take_flat(params, &["encryption"]).unwrap_or_else(|| "auto".to_string());
                apply_tls(name, params, &server, false, false)?;
                params.insert("security".to_string(), serde_json::Value::String(cipher));
            }
            // `alter_id` is numeric in sing-box; intake keeps strings.
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
        }
        "tuic" | "hysteria" | "hysteria2" | "anytls" => {
            apply_tls(name, params, &server, true, true)?;
            // Stream transports don't exist on these protocols; the flats
            // beside them are other protocols' intake spellings.
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
            if canonical == "hysteria" {
                // sing-box holds the plaintext credential in `auth_str`;
                // `auth` is base64 bytes there, never the link secret.
                let auth = take_flat(params, &["auth"]);
                if params.get("auth_str").is_none() {
                    if let Some(auth) = auth {
                        params.insert("auth_str".to_string(), serde_json::Value::String(auth));
                    }
                }
                // Live-probed: 1.14.2 fails initialize without upload and
                // download speeds. Name them instead of inventing
                // bandwidth that would silently throttle the row.
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
            if canonical == "hysteria2" {
                if let Some(variant) = take_flat(params, &["variant"]) {
                    return Err(bad_option(
                        name,
                        "variant",
                        &format!("hysteria2 variant \"{variant}\" has no stock sing-box mapping"),
                    ));
                }
            }
        }
        "ssh" => {
            // Live-probed: the stock `ssh` outbound names the login `user`;
            // intake also accepts `username`, so fold it (explicit `user`
            // wins). No TLS or stream transports on this shape.
            if params.get("user").is_none() {
                if let Some(user) = take_flat(params, &["username"]) {
                    params.insert("user".to_string(), serde_json::Value::String(user));
                }
            } else {
                params.remove("username");
            }
            drop_foreign_flats(params, &["user"]);
        }
        "wireguard" => {
            // No TLS or stream transports on these shapes; the flats
            // beside them are other protocols' intake spellings.
            drop_foreign_flats(params, &[]);
        }
        "shadowsocks" => {
            // Same, except `method` is the native cipher name here. A
            // SIP002 `plugin` names an external executable sing-box would
            // try to run at initialize: fail named instead of emitting a
            // config `check` rejects with "plugin not found".
            if take_flat(params, &["plugin"]).is_some() {
                return Err(bad_option(
                    name,
                    "plugin",
                    "shadowsocks plugins need their executable beside the engine (stock check fails without it)",
                ));
            }
            drop_foreign_flats(params, &["method"]);
        }
        _ => {}
    }
    // `flow` survives only on vless (xtls-vision); elsewhere it is junk.
    if canonical != "vless" {
        params.remove("flow");
    }
    Ok(())
}

/// sing-box ≥1.11 WireGuard clients are `endpoints[]`, not outbounds: the
/// remote peer lives in `peers[]` while local material (private key,
/// interface addresses, MTU, and AmneziaWG obfuscation fields) stays
/// top-level. Stored rows keep outbound-style keys (`server`, `public_key`);
/// this translates them, erroring on missing credentials instead of emitting
/// a config the engine rejects with `unknown field "server"`.
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
    // Endpoint list fields stay lists: interface addresses arrive as a
    // single CIDR string from conf/URI intake.
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
        // Live-probed: 1.14.2 fails initialize with "missing allowed ips
        // for peer 0". Link intake never carries AllowedIPs, and an egress
        // that routes nothing is useless, so default to the v4 full
        // tunnel (documented; a row that wants split tunnel sets it).
        peer.insert(
            "allowed_ips".to_string(),
            serde_json::Value::Array(vec![serde_json::Value::String("0.0.0.0/0".to_string())]),
        );
    }
    // Per the schema (and live-probed against 1.14.2): the dial
    // keepalive is `peers[].persistent_keepalive_interval`, numeric
    // seconds. Intake keeps strings, so coerce; a failed parse stays
    // verbatim for `check` to name loudly.
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
    // Internal classification markers never reach the engine.
    params.remove("awg_version");
    // Client-side resolver setting from conf files: not an endpoint field.
    params.remove("dns");
    // Ticket 32: stock endpoint numerics arrive as strings from conf/URI
    // intake (`mtu`); coerce, leaving failures verbatim for `check`.
    // (Peer keepalive is renamed + coerced at build time above; AWG
    // junk-train numerics get the same treatment below for the fork.)
    if let Some(number) = params
        .get("mtu")
        .and_then(serde_json::Value::as_str)
        .and_then(|text| text.trim().parse::<u64>().ok())
    {
        params.insert("mtu".to_string(), serde_json::Value::Number(number.into()));
    }
    // The fork's AmneziaWG numeric options (jc/jmin/jmax/s1-s4/h1-h4) are
    // typed numerics; conf/URI intake keeps everything as strings, so
    // coerce the plain-numeric fields. H-ranges ("1-100") and i-chains
    // ("<b …><r …>") are truly opaque and pass through verbatim for the
    // fork to judge (a failed parse keeps the original string).
    for key in [
        "jc", "jmin", "jmax", "s1", "s2", "s3", "s4", "h1", "h2", "h3", "h4",
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
    // Ticket 29: standard VLESS rows with a real `encryption` value emit the
    // same loopback bridge pointer (stock rejects the key outright).
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
                    // Ticket 29: xray rows and VLESS-`encryption` rows are
                    // bridge legs served through the main config, so they
                    // resolve before the fork-sidecar rejection below.
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
    presets: &HashMap<String, Vec<serde_json::Value>>,
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
            // Ticket 37: attached preset rules merge ahead of the row's own
            // isolation rule (route→row-outbound, scoped to this inbound).
            for preset_rule in preset_rules_for(config, &built.serving_tag, &in_tag, presets)? {
                rules.push(preset_rule);
            }
            rules.push(serde_json::json!({
                "inbound": [in_tag],
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
        // Ticket 37: attached preset rules merge ahead of the row's own
        // isolation rule (route→row-outbound, scoped to this inbound).
        for preset_rule in preset_rules_for(config, &out_tag, &in_tag, presets)? {
            rules.push(preset_rule);
        }
        rules.push(serde_json::json!({
            "inbound": [in_tag],
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
        // Ticket 32: `inbound` is an array (what stock sing-box 1.14.2
        // accepts); every entry must name a generated inbound. A missing,
        // empty, or non-string entry fails instead of routing nowhere.
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
        // Ticket 37: preset rules carry `direct`/`reject` actions (no
        // outbound); isolation rules stay `route`. A raw `block` reaching
        // this gate means the merge skipped normalization — fail closed.
        // Anything else fails.
        match obj.get("action").and_then(serde_json::Value::as_str) {
            Some("route") => {
                let outbound = outbound.ok_or_else(|| invalid("rule without outbound"))?;
                if !exits.contains(outbound) {
                    return Err(invalid(&format!("rule references unknown \"{outbound}\"")));
                }
            }
            Some("direct") | Some("reject") => {
                if outbound.is_some() {
                    return Err(invalid("rule with direct/reject action carries outbound"));
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
    /// Ticket 28: the no-op trait defaults below stay — the fake relies on
    /// them (no files, no binaries), so dropping any default only moves
    /// code into the fake for zero behavior gain.
    fn drain_log_lines(&mut self) -> Vec<String> {
        Vec::new()
    }
    /// Ticket 08: point the extra-slot driver at one row's executable
    /// before `start` (real driver swaps its binary; the fake ignores it
    /// and keeps recording configs so tests stay hermetic).
    fn set_extra_exe(&mut self, _exe: PathBuf) {}
    /// Ticket 23 LOW: forget the extra-slot executable when the slot empties
    /// (default no-op; the real driver clears it).
    fn clear_extra_exe(&mut self) {}
    /// Ticket 23: pin the probe-validated binary for the next `start` (no
    /// probe/spawn skew; default no-op, the fake needs no binaries).
    fn pin_binary(&mut self, _path: PathBuf) {}
    /// Ticket 23: best-effort removal of the secret config file (crash and
    /// check paths; default no-op, the fake writes no files).
    fn remove_config_file(&self) {}
}

/// Binary file name for this platform.
pub fn engine_binary_file_name() -> String {
    if cfg!(windows) {
        format!("{ENGINE_BINARY_BASE}.exe")
    } else {
        ENGINE_BINARY_BASE.to_string()
    }
}

/// Ticket 28: the real app directory behind every bundled-core lookup.
fn current_app_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
}

/// Ticket 28: shared app-dir join for bundled cores (executable-gated).
/// `Err` is the searched directory; each `resolve_*` keeps its own error
/// type because the controller names different rows/binaries.
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

/// Resolve the sidecar binary in the app directory (never `PATH`, never
/// downloads anything); the [`EngineError::MissingBinary`] names the
/// directory searched. Candidates must be executable.
pub fn resolve_engine_binary() -> Result<PathBuf, EngineError> {
    let app_dir = current_app_dir();
    let path_env = std::env::var("PATH").unwrap_or_default();
    resolve_engine_binary_in(app_dir.as_deref(), &path_env)
}

/// Testable core of [`resolve_engine_binary`] (explicit search root).
///
/// Ticket 23: app directory ONLY (never `PATH` — the bundled core must be
/// the placed binary, not whatever the environment resolves; the extra slot
/// was already app-dir-only). Candidates must also be executable.
pub fn resolve_engine_binary_in(
    app_dir: Option<&Path>,
    _path_env: &str,
) -> Result<PathBuf, EngineError> {
    resolve_bundled_in(app_dir, &engine_binary_file_name())
        .map_err(|searched| EngineError::MissingBinary { searched })
}

/// Ticket 07: binary file name for the fork core on this platform.
pub fn fork_binary_file_name() -> String {
    if cfg!(windows) {
        format!("{FORK_BINARY_BASE}.exe")
    } else {
        FORK_BINARY_BASE.to_string()
    }
}

/// Ticket 07: resolve the fork-core binary (app dir ONLY, never `PATH` —
/// ticket 23: same rule as every bundled core; candidates must be
/// executable). Never downloads; `Err` is the searched directory.
/// Stock paths never call this: only exotic rows do.
pub fn resolve_fork_binary_in(app_dir: Option<&Path>, _path_env: &str) -> Result<PathBuf, String> {
    resolve_bundled_in(app_dir, &fork_binary_file_name())
}

/// Ticket 07: resolve the fork-core binary, naming the expected binary on
/// failure. Row names are filled in by the controller ([`EngineController`]
/// knows the exotic set); direct callers get the unnamed form.
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

/// Ticket 08: resolve the Xray-core binary (app dir ONLY, never `PATH` —
/// ticket 23: same rule as every bundled core; candidates must be
/// executable). Never downloads; `Err` is the searched directory.
/// Stock/fork paths never call this: only Xray rows do.
pub fn resolve_xray_binary_in(app_dir: Option<&Path>, _path_env: &str) -> Result<PathBuf, String> {
    resolve_bundled_in(app_dir, &xray_binary_file_name())
}

/// Ticket 08: resolve the Xray-core binary, naming the expected binary on
/// failure. Row names are filled in by the controller ([`EngineController`]
/// knows the Xray set); direct callers get the unnamed form.
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
    if is_executable(&candidate) {
        Ok(candidate)
    } else {
        Err(dir.display().to_string())
    }
}

/// Ticket 08: resolve one user extra-core executable against the real app
/// dir. Row names are filled in by the controller; direct callers get the
/// unnamed form.
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

/// Ticket 29: strip ANSI CSI escape sequences (`ESC [ params final-byte`,
/// the SGR colors/bold that `sing-box check` paints its `FATAL` line with)
/// so engine errors stay human-readable in toasts. Keeps the text, drops
/// only the codes. Hand-rolled (no new deps); a trailing bare ESC is dropped.
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

// -- Ticket 23: lifecycle hardening -----------------------------------------
// Sidecar lifecycle that cannot orphan processes, leak secret files, or
// split-brain on partial failure. Pure helpers first (tested directly);
// the `RealEngineBackend` + `EngineController` changes below consume them.

/// Per-instance config-file counter (pairs with the pid suffix so the four
/// sidecar backends — and any two ManyProxy processes — never share a path).
static CONFIG_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Per-instance unique temp config path (`manyproxy-<stem>-<pid>-<n>.json`).
fn unique_config_path(stem: &str) -> PathBuf {
    let n = CONFIG_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("manyproxy-{stem}-{}-{n}.json", std::process::id()))
}

/// Notice line pushed when the log buffer lock is found poisoned (fail-open
/// with a marker, never a silent drop — the reader keeps serving).
const LOG_POISON_NOTICE: &str = "log pipe unreadable (log reader failed)";

/// Write the sidecar config with owner-only permissions (0600 on Unix; the
/// Windows temp dir is per-user by construction, tightened best-effort via
/// [`crate::store::harden_file_permissions`]). The file holds secrets, so
/// creation must not widen access.
fn write_config_restricted(path: &Path, config_json: &str) -> Result<(), String> {
    use std::io::Write as _;
    let mut options = std::fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| format!("cannot write engine config ({error})"))?;
    file.write_all(config_json.as_bytes())
        .map_err(|error| format!("cannot write engine config ({error})"))?;
    drop(file);
    // Ticket 27: user-only permissions on every platform (Unix already 0600
    // at create; Windows ACL tightening is best-effort here).
    crate::store::harden_file_permissions(path);
    Ok(())
}

/// True when the path is a file the sidecar may execute: present plus the
/// exec bit on Unix (Windows executability is the `.exe` association itself).
fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        return path
            .metadata()
            .map(|meta| meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Canonical conflict key for a bind: loopback aliases (`127.0.0.1`,
/// `localhost`, `::1`, `::ffff:127.0.0.1`; blank means the loopback default)
/// compare equal so two rows can never claim the same local endpoint under
/// different spellings. Anything else compares case-insensitively verbatim.
fn normalize_bind_key(bind: &str) -> String {
    match bind.trim().to_ascii_lowercase().as_str() {
        "" | "localhost" | "127.0.0.1" | "::1" | "::ffff:127.0.0.1" => "loopback".to_string(),
        other => other.to_string(),
    }
}

/// Probe-bind one endpoint before any sidecar is stopped: catches a foreign
/// holder (or a sibling sidecar that kept its socket) while rollback is still
/// cheap. Probes are serialized process-wide so parallel applies (and test
/// threads sharing fixed ports) never fail on each other; a genuinely held
/// port still fails after bounded retries. Blank binds probe the loopback
/// default.
fn probe_bind_available(bind: &str, port: u16) -> Result<(), String> {
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
            Ok(listener) => {
                drop(listener);
                return Ok(());
            }
            Err(error) => {
                last = format!("{host}:{port} is already in use ({error})");
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
    Err(last)
}

/// Recursive forbidden-key scan returning the offending `(key, path)` pair.
/// Shared by [`assert_no_system_mutation`] (exact messages preserved) and the
/// custom full-config replacement gate (ticket 23 SEC-HIGH: replacements are
/// user JSON served by bundled sidecars, so nested `tun`/`dns` would mutate
/// the system exactly like generated keys would).
fn find_system_mutation_key(document: &serde_json::Value) -> Option<(String, String)> {
    fn walk(node: &serde_json::Value, path: &str) -> Option<(String, String)> {
        match node {
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    let lowered = key.to_ascii_lowercase();
                    if lowered == "tun" || lowered == "dns" || lowered == "set_system_proxy" {
                        return Some((key.clone(), path.to_string()));
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

/// Rows in the candidate set whose non-loopback bind has no matching entry in
/// the controller's confirmed set. `(id, name, bind)` triples; errors name
/// rows + binds only.
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

/// Bounded child reap: the caller must `kill()` first; this waits up to
/// `timeout` for the exit, escalates with a second kill, then gives up rather
/// than hanging the UI thread (a SIGKILL-equivalent reap is near-certain, so
/// the give-up arm is pathological only).
fn wait_child_bounded(child: &mut std::process::Child, timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {}
            Err(_) => return,
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    for _ in 0..20 {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {}
            Err(_) => return,
        }
        std::thread::sleep(Duration::from_millis(50));
    }
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
/// ([`RealEngineBackend::xray`]: `xray run -c`, own temp file) and each user
/// extra-core slot ([`RealEngineBackend::extra_slot`]: the row's executable
/// from the app dir, config file passed through as
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
    /// Ticket 23: controller-pinned binary (the path the probe validated —
    /// no probe/spawn skew). Falls back to fresh resolution when stale.
    pinned_binary: Option<PathBuf>,
    /// Ticket 13: piped-stderr lines (raw until `LogStore` ingest redacts
    /// them) plus at most one named pipe-error line; bounded, cleared on
    /// every start/stop so restarts never replay stale lines.
    /// Ticket 23: `(reader generation, lines)`. The generation bumps on every
    /// stop/start so a previous start's reader thread can never interleave
    /// stale lines after a restart.
    log_buffer: Arc<Mutex<(u64, VecDeque<String>)>>,
    /// Ticket 23: current reader generation (mirrored into `log_buffer`).
    reader_gen: u64,
    /// Ticket 23: the live reader thread. Reaped when finished; a still-live
    /// handle at stop is detached (it exits on pipe EOF after the kill) and
    /// its lines are fenced out by the generation.
    reader_handle: Option<std::thread::JoinHandle<()>>,
}

/// Ticket 13: pipe-reader buffer bound (the `LogStore` ring bounds display
/// storage separately; this only caps lines awaiting drain).
pub const LOG_PIPE_BUFFER: usize = 2000;

/// Ticket 13: push one reader-thread line, dropping the oldest when full
/// (bounded memory even if the UI never drains; `VecDeque` makes the cap
/// O(1)).
/// Ticket 23: lines from a stale generation are dropped (a restart's reader
/// cannot interleave output from the previous child). A poisoned lock
/// recovers with a named notice line instead of a silent drop.
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
            slot.0 = expected_gen;
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
            // Ticket 23: per-instance unique path (pid suffix) so sidecars
            // never share a secret file.
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

    /// Ticket 07: fork-core driver (own binary, own temp file, version probe
    /// on every check). Stock rows never touch this.
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

    /// Ticket 08: Xray sidecar driver (own binary, own temp file).
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

    /// Ticket 08: user extra-core slot driver (own temp file for the
    /// passed-through config; the row's executable arrives per activation
    /// via [`EngineBackend::set_extra_exe`]).
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
        // Ticket 23: the controller pins the probed path before `check`, so
        // spawn uses the validated binary (no probe/spawn skew). A stale pin
        // (deleted since) falls through to a fresh resolution, never to a
        // stale spawn.
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
        // Ticket 23: owner-only permissions; the file holds secrets.
        write_config_restricted(&self.config_path, config_json)?;
        Ok(self.config_path.clone())
    }

    /// Ticket 23: kill only the child process (bounded wait with kill
    /// escalation, never a UI-thread hang) and fence out its reader thread.
    /// The config file is left alone — file removal is
    /// [`RealEngineBackend::remove_config_file`], so `start()` can
    /// stop-then-write-then-spawn without deleting the file it just wrote.
    fn stop_child(&mut self) {
        self.reader_gen = self.reader_gen.wrapping_add(1);
        let gen = self.reader_gen;
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            wait_child_bounded(&mut child, Duration::from_secs(3));
        }
        // Ticket 13: drop buffered (raw) lines with the sidecar so a later
        // start never replays stale output.
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
        // Reap a finished reader; detach a still-live one (it exits on pipe
        // EOF after the kill, and its lines are fenced out by the bumped
        // generation).
        let finished = self
            .reader_handle
            .as_ref()
            .is_some_and(|handle| handle.is_finished());
        if finished {
            if let Some(handle) = self.reader_handle.take() {
                let _ = handle.join();
            }
        } else {
            self.reader_handle.take();
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
            // Version probe is load-bearing: a plain/wrong binary fails fast
            // with a named error instead of serving exotic rows badly.
            probe_fork_version(&binary)?;
        }
        // Ticket 08: xray/extra have no stable `check` CLI contract here, so
        // the write + ticket-08 validators gate them; runtime failures
        // surface via `poll` like every other crash.
        if self.xray || self.extra_exe.is_some() {
            self.write_config(config_json)?;
            // Ticket 23: the check file holds secrets — remove it even
            // though no process was spawned.
            self.remove_config_file();
            return Ok(());
        }
        let path = self.write_config(config_json)?;
        let output = std::process::Command::new(&binary)
            .arg("check")
            .arg("-c")
            .arg(&path)
            .output()
            .map_err(|error| format!("engine check could not run ({error})"))?;
        // Ticket 23: `check` leaves no secret file behind either way.
        self.remove_config_file();
        if output.status.success() {
            Ok(())
        } else {
            // Ticket 29: strip ANSI color escapes before truncating so the
            // cap counts visible text and the toast keeps `FATAL` clean.
            Err(truncate_detail(&strip_ansi_codes(
                &String::from_utf8_lossy(&output.stderr),
            )))
        }
    }

    fn start(&mut self, config_json: &str) -> Result<(), String> {
        let binary = self.resolve_binary()?;
        // Ticket 23 CRITICAL order: stop the previous child FIRST, then
        // write, then spawn — the old write→stop→spawn order deleted the
        // file it had just written, so the spawn read a deleted path.
        self.stop_child();
        let path = self.write_config(config_json)?;
        // Unprivileged by construction: plain spawn, never elevated, no
        // admin for loopback use (spec boundary).
        // Ticket 13: stderr is piped (never nulled): a reader thread pumps
        // lines into the bounded buffer so the pipe cannot stall the
        // sidecar and the logs dialog shows live lines. The thread ends on
        // EOF when the sidecar exits.
        let spawn = std::process::Command::new(&binary)
            .args(self.start_args())
            .arg(&path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn();
        let mut child = match spawn {
            Ok(child) => child,
            // Ticket 23: a failed spawn leaves no secret file behind.
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

    fn poll_exit(&mut self) -> Option<String> {
        let status = self.child.as_mut()?.try_wait().ok()??;
        self.child = None;
        // Ticket 23: a reaped crash leaves no secret file behind. Buffered
        // lines are kept for post-mortem (the next stop/start clears them).
        self.remove_config_file();
        Some(format!("engine exited ({status})"))
    }

    /// Ticket 13: take buffered stderr lines. A poisoned lock surfaces one
    /// named error instead of raw state (fail-closed, never stale).
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
        // Ticket 23 LOW: the slot must not keep pointing at a row that is no
        // longer active.
        self.extra_exe = None;
    }

    fn pin_binary(&mut self, path: PathBuf) {
        self.pinned_binary = Some(path);
    }

    fn remove_config_file(&self) {
        let _ = std::fs::remove_file(&self.config_path);
    }
}

/// Ticket 23 HIGH: RAII teardown — dropping a backend (window close, panic
/// unwind, controller teardown) kills the sidecar with a bounded reap and
/// removes the secret config file, best-effort. No `main()` close hook is
/// needed: teardown lives in the backend itself.
impl Drop for RealEngineBackend {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            wait_child_bounded(&mut child, Duration::from_secs(1));
        }
        self.remove_config_file();
    }
}

#[cfg(test)]
#[derive(Debug, Default)]
struct FakeState {
    running: bool,
    last_config: Option<String>,
    check_calls: usize,
    fail_check: Option<String>,
    /// Ticket 23: one-shot next-`start` failure (rollback tests). `None` by
    /// default, so every existing consumer behaves exactly as before.
    fail_start: Option<String>,
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

    /// Ticket 23: fail the next `start` once (consumed on use), then behave
    /// normally — lets rollback tests fail one spawn while the retry path
    /// stays healthy.
    pub fn fail_start(&self, detail: &str) {
        self.state.borrow_mut().fail_start = Some(detail.to_string());
    }

    /// Ticket 23: clear injected `check`/`start` failures.
    pub fn clear_failures(&self) {
        let mut state = self.state.borrow_mut();
        state.fail_check = None;
        state.fail_start = None;
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
/// Ticket 28: kept mirrored with the health fake on purpose — this one is
/// `Rc<RefCell>` (single-threaded UI harness), the health fake is
/// `Arc<Mutex>` (shared across probe threads).
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
/// Ticket 27: N named extra-core slot sidecars (one per active extra-core
/// row) run the user's executables unprivileged with their configs passed
/// through. Custom full-config rows replace their owning core's generated
/// config wholesale.
pub struct EngineController {
    backend: Box<dyn EngineBackend>,
    /// Ticket 07: fork-core sidecar for exotic rows only.
    fork_backend: Box<dyn EngineBackend>,
    /// Ticket 08: Xray-core sidecar for Xray-only rows.
    xray_backend: Box<dyn EngineBackend>,
    /// Ticket 27: one user extra-core sidecar per active extra-core row
    /// (activation order; replaces the ticket-08 singleton).
    extra_backends: Vec<Box<dyn EngineBackend>>,
    /// Ticket 07: fork-binary probe seam (default: real resolution; tests
    /// inject present/missing without touching the filesystem).
    fork_probe: Box<dyn Fn() -> Result<PathBuf, EngineError>>,
    /// Ticket 08: xray-binary probe seam (same contract as the fork probe).
    xray_probe: Box<dyn Fn() -> Result<PathBuf, EngineError>>,
    /// Ticket 08: extra-executable probe seam (app dir only, per exe name).
    #[allow(clippy::type_complexity)]
    extra_probe: Box<dyn Fn(&str) -> Result<PathBuf, EngineError>>,
    /// Ticket 23: stock-binary probe seam (default: real resolution; lets the
    /// controller fail a missing stock binary typed BEFORE touching anything,
    /// and pin the validated path for spawn).
    stock_probe: Box<dyn Fn() -> Result<PathBuf, EngineError>>,
    /// Ticket 23: `(id, bind)` pairs the user explicitly confirmed for LAN
    /// exposure. Unconfirmed starts carrying such pairs fail with
    /// [`EngineError::LanConfirmRequired`] independent of any UI gate.
    lan_confirmed: HashSet<(String, String)>,
    /// Ticket 23: last configs successfully served per shard
    /// (stock/fork/xray) — the rollback target when a later apply's
    /// spawn fails midway.
    served: [Option<String>; 3],
    /// Ticket 27: last configs served per extra slot (parallel to
    /// [`EngineController::extra_backends`]).
    served_extra: Vec<Option<String>>,
    /// Ticket 37: preset id → ordered route-rule objects for the engine
    /// merge (synced from the store before start; empty means no row
    /// carries a preset). Dangling references fail loudly at merge time.
    preset_rules: HashMap<String, Vec<serde_json::Value>>,
    active: Vec<ProxyConfig>,
    status: EngineStatus,
    last_error: Option<String>,
    last_config_json: Option<String>,
    /// Ticket 36: live clash controllers per serving core (port + secret).
    /// In-memory only (regenerated per start, never persisted); cleared
    /// with the served snapshots whenever membership empties or a core
    /// serves a user-owned replacement (never mutated user JSON).
    clash: Vec<ClashTarget>,
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
            extra_backends: vec![Box::new(RealEngineBackend::extra_slot())],
            fork_probe: Box::new(resolve_fork_binary),
            xray_probe: Box::new(resolve_xray_binary),
            extra_probe: Box::new(resolve_extra_binary),
            stock_probe: Box::new(resolve_engine_binary),
            lan_confirmed: HashSet::new(),
            served: [None, None, None],
            served_extra: vec![None],
            preset_rules: HashMap::new(),
            active: Vec::new(),
            status: EngineStatus::Stopped,
            last_error: None,
            last_config_json: None,
            clash: Vec::new(),
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
    /// Ticket 27: the extra handle is the first of
    /// [`EngineController::extra_backends`]; use
    /// [`EngineController::push_test_extra_backend`] for more slots.
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
            // Ticket 23: the stock probe defaults to present (no filesystem
            // access); override with `set_stock_probe` for missing-binary
            // scenarios.
            stock_probe: Box::new(|| Ok(PathBuf::from("test-stock-bin"))),
            lan_confirmed: HashSet::new(),
            served: [None, None, None],
            served_extra: vec![None],
            preset_rules: HashMap::new(),
            active: Vec::new(),
            status: EngineStatus::Stopped,
            last_error: None,
            last_config_json: None,
            clash: Vec::new(),
        };
        (controller, stock, fork, xray, extra)
    }

    /// Ticket 27: test-only extra slot growth — push one more mock extra
    /// backend and return its handle (keeps `fake_quartet` hermetic for
    /// N-slot tests without touching the filesystem).
    #[cfg(test)]
    pub fn push_test_extra_backend(&mut self) -> FakeHandle {
        let (backend, handle) = FakeEngineBackend::shared();
        self.extra_backends.push(Box::new(backend));
        self.served_extra.push(None);
        handle
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

    /// Ticket 23: override the stock-binary probe (tests inject present /
    /// missing without filesystem access).
    #[cfg(test)]
    pub fn set_stock_probe(&mut self, probe: impl Fn() -> Result<PathBuf, EngineError> + 'static) {
        self.stock_probe = Box::new(probe);
    }

    /// Ticket 37: replace the preset rules for the engine merge (the UI
    /// syncs these from the store before every start; tests inject
    /// directly). Empty means no row carries a preset.
    pub fn set_preset_rules(&mut self, presets: HashMap<String, Vec<serde_json::Value>>) {
        self.preset_rules = presets;
    }

    /// Ticket 23: drop LAN confirms for rows that are no longer active (or
    /// whose bind changed — confirms key on `(id, bind)`).
    fn prune_lan_confirmed(&mut self) {
        let live: HashSet<(String, String)> = self
            .active
            .iter()
            .map(|row| (row.id.clone(), row.bind_ip.clone()))
            .collect();
        self.lan_confirmed.retain(|entry| live.contains(entry));
    }

    /// Ticket 23: shard backend by index (0 stock, 1 fork, 2 xray).
    /// Ticket 27: extra slots ride in [`EngineController::extra_backends`]
    /// (see `apply`: per-slot start/stop, never a shared index).
    fn backend_at(&mut self, index: usize) -> &mut dyn EngineBackend {
        match index {
            0 => self.backend.as_mut(),
            1 => self.fork_backend.as_mut(),
            _ => self.xray_backend.as_mut(),
        }
    }

    /// Ticket 23: true while any sidecar process handle is up (liveness is
    /// handle presence, never a cached flag).
    fn any_backend_running(&self) -> bool {
        self.backend.is_running()
            || self.fork_backend.is_running()
            || self.xray_backend.is_running()
            || self.extra_backends.iter().any(|slot| slot.is_running())
    }

    /// Ticket 27: index of one extra row among the active extra slots
    /// (activation order — the same order `apply` serves them in).
    fn extra_slot_index(&self, id: &str) -> Option<usize> {
        self.active
            .iter()
            .filter(|row| is_extra_core_protocol(&row.protocol))
            .position(|row| row.id == id)
    }

    /// Start one config (idempotent); restarts the sidecar with the union.
    /// Ticket 23 SEC-MED: fails with [`EngineError::LanConfirmRequired`]
    /// when the resulting set serves a non-loopback bind the user never
    /// confirmed — independent of the UI dialog gate. Use
    /// [`EngineController::start_one_confirmed`] past an explicit confirm.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn start_one(&mut self, config: &ProxyConfig) -> Result<(), EngineError> {
        self.start_one_inner(config, false)
    }

    /// Ticket 23: [`EngineController::start_one`] past an explicit LAN
    /// confirm (records the confirm for the set's LAN binds).
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
        // Pure pre-check: a bad candidate never disturbs the running sidecars.
        validate_activation(&candidate).inspect_err(|error| {
            self.last_error = Some(error.to_string());
        })?;
        self.enforce_lan_confirm(&candidate, confirmed)?;
        // Ticket 23 HIGH: `active` commits only with a successful apply — a
        // failed apply restores the previous membership instead of claiming
        // rows nothing serves.
        let prev = self.active.clone();
        let prev_status = self.status.clone();
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

    /// Ticket 23: fail when the candidate set serves unconfirmed LAN binds
    /// (or record the confirm when past the gate).
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

    /// Stop one config; returns true when it was active. Stops the sidecar
    /// when the set becomes empty.
    /// Ticket 32: a failing re-gen restores the membership snapshot (active
    /// and the running sidecar agree) and records the error in
    /// [`EngineController::last_error`] — never a silent half-state. The
    /// bool answers membership (the row was active); the apply outcome is
    /// on the `last_error` seam, like the start paths.
    pub fn stop_one(&mut self, id: &str) -> bool {
        let before = self.active.len();
        let prev = self.active.clone();
        let prev_status = self.status.clone();
        self.active.retain(|row| row.id != id);
        if self.active.len() == before {
            return false;
        }
        if self.apply().is_err() {
            self.active = prev;
            self.status = prev_status;
        }
        self.prune_lan_confirmed();
        true
    }

    /// Replace the active set (start-all path); validates before touching the
    /// running sidecar.
    /// Ticket 23 SEC-MED: fails with [`EngineError::LanConfirmRequired`]
    /// when the set serves a non-loopback bind the user never confirmed.
    /// Use [`EngineController::start_all_confirmed`] past an explicit confirm.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn start_all(&mut self, configs: &[ProxyConfig]) -> Result<(), EngineError> {
        self.start_all_inner(configs, false)
    }

    /// Ticket 23: [`EngineController::start_all`] past an explicit LAN
    /// confirm (records the confirm for the set's LAN binds).
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
        // Ticket 23 HIGH: `active` commits only with a successful apply.
        let prev = self.active.clone();
        let prev_status = self.status.clone();
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

    /// Stop everything and take the sidecar down.
    /// Ticket 32: like [`EngineController::stop_one`], a failing apply
    /// restores the membership snapshot instead of leaving a silent
    /// half-state (the empty-set apply cannot fail today; this is
    /// defense in depth for future fallible teardown).
    pub fn stop_all(&mut self) {
        let prev = self.active.clone();
        let prev_status = self.status.clone();
        self.active.clear();
        if self.apply().is_err() {
            self.active = prev;
            self.status = prev_status;
        }
        self.prune_lan_confirmed();
    }

    fn apply(&mut self) -> Result<(), EngineError> {
        if self.active.is_empty() {
            self.backend.stop();
            self.fork_backend.stop();
            self.xray_backend.stop();
            for slot in self.extra_backends.iter_mut() {
                slot.stop();
                // Ticket 23 LOW: the emptied slot forgets its executable.
                slot.clear_extra_exe();
            }
            // Ticket 23 LOW: the served snapshots reset with the membership.
            self.served = [None, None, None];
            for served in self.served_extra.iter_mut() {
                *served = None;
            }
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
        // fails only its own rows; the other sidecars keep serving. Ticket
        // 23: the stock gate is typed too (was: wrapped as CheckFailed deep
        // in `check`), and every validated path is pinned for spawn (no
        // probe/spawn skew).
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
        // Ticket 27: one binary gate per extra slot (each names its own exe;
        // a missing exe fails only the extra rows, like every other gate).
        // Grow real sidecars for slots beyond the current backends (tests
        // pre-size via `push_test_extra_backend`, so no real spawn there).
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
        // Per-sidecar configs. Custom full-config rows replace their core
        // wholesale; anything else generates. Subsets of a validated union
        // cannot fail validation again, but the errors stay typed.
        let replacement = |core: OwningCore| {
            plan.replacements
                .iter()
                .find(|(owned, _)| *owned == core)
                .map(|(_, json)| json.clone())
        };
        // Bridge map for the main config, straight from the plan's bridge
        // bookkeeping (`bridge_key` is the exact key scheme `build_main_config`
        // expects, with the plan's ports). Ticket 23 LOW: no re-derivation
        // with swallowed validation context — the validated plan is the
        // single source of truth.
        let mut plan_bridges: HashMap<String, u16> = HashMap::with_capacity(plan.egresses.len());
        for egress in &plan.egresses {
            plan_bridges.insert(egress.bridge_key.clone(), egress.bridge_port);
        }
        let mut stock_gen: Option<String> = match replacement(OwningCore::Stock) {
            Some(json) => Some(json),
            None if plan.main_rows.is_empty() => None,
            None => Some(
                build_main_config(&plan.main_rows, &plan_bridges, &self.preset_rules)
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
                generate_with_presets(&plan.fork_rows, &self.preset_rules)
                    .inspect_err(|error| {
                        self.last_error = Some(error.to_string());
                    })?
                    .json,
            ),
        };
        // Ticket 36: loopback clash controllers on generated cores only
        // (user-owned replacements are never mutated — those cores stay
        // untracked). Exhaustion and injection failures are typed here,
        // before anything stops, like every other apply gate.
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
            let port = find_clash_port().ok_or_else(|| {
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
        // Ticket 27: one passed-through config per extra slot.
        let mut extra_gens: Vec<Option<String>> = Vec::with_capacity(plan.extras.len());
        for row in &plan.extras {
            extra_gens.push(Some(extra_config_json(row).inspect_err(|error| {
                self.last_error = Some(error.to_string());
            })?));
        }
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
        // Ticket 23 SEC-HIGH: replacements are user JSON served by bundled
        // sidecars — the recursive system-mutation scan covers them here too
        // (defense in depth: `validate_activation` already scanned them via
        // `custom_full_config`, and this holds for any future path that
        // reaches `apply` another way).
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
        // Ticket 23 MED: probe-bind every serving endpoint (plus every
        // bridge listener) BEFORE any sidecar is stopped — a foreign holder
        // fails typed here, while rollback is still free.
        // Ticket 36: pairs our currently-served sidecars already hold are
        // skipped (re-apply/start-one-more/stop-one must not mistake our
        // own sockets for a foreign holder). The skip set comes only from
        // configs we served (never guessed); a foreign squatter on a
        // previously-served port surfaces later as a typed spawn failure.
        {
            let mut own: std::collections::HashSet<(String, u16)> =
                std::collections::HashSet::new();
            for served in self
                .served
                .iter()
                .flatten()
                .chain(self.served_extra.iter().flatten())
            {
                own.extend(served_listen_ports(served));
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
            // Ticket 36: clash controllers join the same probe gate.
            for target in &clash {
                endpoints.push((
                    format!("clash controller ({})", target.core),
                    "127.0.0.1".to_string(),
                    target.port,
                ));
            }
            for (name, bind, port) in &endpoints {
                if own.contains(&(bind.clone(), *port)) {
                    continue;
                }
                if probe_bind_available(bind, *port).is_err() {
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
                    // Ticket 29: ANSI-clean on every backend path (stock /
                    // fork / xray check output all surface as toasts).
                    detail = strip_ansi_codes(&detail);
                    let error = EngineError::CheckFailed { detail };
                    self.last_error = Some(error.to_string());
                    return Err(error);
                }
            }
        }
        // Ticket 27: every extra slot checks its own passed-through config.
        for (slot, gen) in self.extra_backends.iter_mut().zip(extra_gens.iter()) {
            if let Some(json) = gen {
                if let Err(detail) = slot.check_config(json) {
                    // Ticket 29: ANSI-clean on the extra-slot path too.
                    let error = EngineError::CheckFailed {
                        detail: strip_ansi_codes(&detail),
                    };
                    self.last_error = Some(error.to_string());
                    return Err(error);
                }
            } else {
                break;
            }
        }
        // Restart only populated sidecars; empty subsets stay down. Each
        // extra slot driver is pointed at its row's executable first; slots
        // beyond the plan forget theirs (ticket 23 LOW).
        for (index, slot) in self.extra_backends.iter_mut().enumerate() {
            if let Some(path) = extra_paths.get(index).and_then(|path| path.as_ref()) {
                slot.set_extra_exe(path.clone());
            } else {
                slot.clear_extra_exe();
            }
        }
        // Ticket 23 HIGH: a failed spawn rolls the shards this apply touched
        // back to their previously served configs (snapshot below) instead of
        // leaving a split brain — some shards on the new set, one shard down.
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
        // Ticket 27: each extra slot serves its own endpoint; shrink the
        // served tail when the plan holds fewer slots than the backends.
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
            // Restore every shard to its pre-apply served config — including
            // shards whose new spawn just failed (the failure may be
            // transient; a persistent failure just leaves that shard down,
            // still truthful, never half-serving). Shards already stopped
            // with no previous config stay down.
            for (index, prev) in prev_served.clone().into_iter().enumerate() {
                let backend = self.backend_at(index);
                backend.stop();
                match prev {
                    Some(prev) => match backend.start(&prev) {
                        Ok(()) => self.served[index] = Some(prev),
                        Err(_) => self.served[index] = None,
                    },
                    None => self.served[index] = None,
                }
            }
            // Ticket 27: extra slots roll back to their own snapshots.
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
                            Err(_) => {
                                if index < self.served_extra.len() {
                                    self.served_extra[index] = None;
                                }
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
            let detail = failures
                .iter()
                .map(|(shard, error)| format!("{shard}: {error}"))
                .collect::<Vec<_>>()
                .join("; ");
            self.last_error = Some(detail);
            let (_, first) = failures.into_iter().next().expect("failures non-empty");
            // Ticket 36: controllers follow the restored served set.
            self.sync_clash_from_served();
            self.status = if self.any_backend_running() {
                EngineStatus::Running
            } else {
                EngineStatus::Stopped
            };
            return Err(first);
        }
        // Ticket 23: status agrees with per-row `is_running` — Running iff
        // any sidecar handle is up (every running shard serves a non-empty
        // subset, and every served row maps to its shard's handle).
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
        // Ticket 36: controllers follow the newly served set.
        self.sync_clash_from_served();
        self.last_error = None;
        Ok(())
    }

    /// Crash detection: call on UI actions; reports every crashed shard.
    /// Ticket 23 MED: all sidecars are reaped per poll (stock first) —
    /// a one-shot poll no longer loses N-1 causes. A single crash returns
    /// its bare cause (previous contract); several join as
    /// `"shard: cause; shard: cause"`. Any observed crash transitions the
    /// status (and records `last_error`), so callers see the failure even
    /// when they only need the per-shard list.
    /// Ticket 27: every extra slot polls as `extra-<id>` (or `extra-<index>`
    /// when the row is gone).
    pub fn poll_shards(&mut self) -> Vec<(String, String)> {
        let mut crashes = Vec::new();
        for (shard, backend) in [
            ("stock", &mut self.backend),
            ("fork", &mut self.fork_backend),
            ("xray", &mut self.xray_backend),
        ] {
            // Ticket 23 HIGH: the failed shard's secret file is cleaned by
            // `poll_exit` itself; siblings keep running untouched.
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
        crashes
    }

    /// Ticket 30: joined per-shard crash detail — always one `shard: cause`
    /// fragment per crashed shard, joined with `; ` (single crashes keep
    /// their shard prefix here, unlike [`EngineController::poll`]'s bare
    /// cause). Pure: the UI builds the error notification from this without
    /// touching generation or schemas. Causes are already redacted upstream.
    pub fn crash_detail(crashes: &[(String, String)]) -> String {
        crashes
            .iter()
            .map(|(shard, cause)| format!("{shard}: {cause}"))
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Crash detection: call on UI actions; reports the named cause(s) once.
    /// Ticket 07: watches both sidecars, stock first.
    /// Ticket 08: watches all four sidecars, stock first.
    pub fn poll(&mut self) -> Option<String> {
        let crashes = self.poll_shards();
        if crashes.is_empty() || self.active.is_empty() {
            return None;
        }
        Some(if crashes.len() == 1 {
            crashes.into_iter().next().expect("one crash").1
        } else {
            crashes
                .iter()
                .map(|(shard, cause)| format!("{shard}: {cause}"))
                .collect::<Vec<_>>()
                .join("; ")
        })
    }

    /// Ticket 13: take buffered stderr lines from every sidecar (stock,
    /// fork, xray, every extra slot). The UI ingests them into `LogStore` on
    /// every reload, so live lines appear without user clicks.
    pub fn drain_log_lines(&mut self) -> Vec<String> {
        let mut lines = self.backend.drain_log_lines();
        lines.extend(self.fork_backend.drain_log_lines());
        lines.extend(self.xray_backend.drain_log_lines());
        for slot in self.extra_backends.iter_mut() {
            lines.extend(slot.drain_log_lines());
        }
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

    /// Ticket 36: live clash controllers per serving core (port + secret)
    /// for the traffic poll tick. Empty when nothing generated serves
    /// (stopped cores, user-owned replacements, extra slots).
    pub fn clash_targets(&self) -> &[ClashTarget] {
        &self.clash
    }

    /// Ticket 36: test seam — point the traffic tick at stub controllers
    /// (mirrors the `set_*_probe` setter precedent).
    #[cfg(test)]
    pub fn set_clash_targets(&mut self, targets: Vec<ClashTarget>) {
        self.clash = targets;
    }

    /// Ticket 36: rebuild [`EngineController::clash`] from the served
    /// configs (single source of truth — covers success, rollback-restore,
    /// and teardown uniformly).
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

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_running(&self, id: &str) -> bool {
        let Some(row) = self.active.iter().find(|row| row.id == id) else {
            return false;
        };
        // Ticket 08: each row reports its serving sidecar. Xray rows ride
        // the xray bridge; custom rows follow their owning core; each extra
        // row reports its own named slot; chains are served by the main config.
        // Ticket 27: the slot index is the row's position among active
        // extra rows (the order `apply` serves them in).
        if is_extra_core_protocol(&row.protocol) {
            return self
                .extra_slot_index(id)
                .and_then(|index| self.extra_backends.get(index))
                .is_some_and(|slot| slot.is_running());
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
    if let Some((key, path)) = find_system_mutation_key(&document) {
        return Err(format!("forbidden key \"{key}\" at {path}"));
    }
    Ok(())
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
            assert_eq!(rule["inbound"], json!([entry.inbound_tag.clone()]));
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

    /// Ticket 36: clash injection adds a loopback-only controller block
    /// (shape proven live against stock 1.14.2); garbage input fails typed.
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

    /// Ticket 36 (re-apply prerequisite): served listen ports parse out of
    /// served configs; garbage fails open to probing (never a skip).
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

    /// Ticket 36 (re-apply prerequisite): starting a second row while the
    /// first is served does not trip the bind probe on the already-served
    /// port (the test listener stands in for our own sidecar's hold), and
    /// stopping one of two served rows works too. A genuinely foreign
    /// holder on a NEW port still fails typed.
    #[test]
    fn ticket36_reapply_skips_own_served_ports() {
        let (mut controller, _stock, _fork) = EngineController::fake_pair();
        let rows = vec![fixture(
            "1",
            "Alpha",
            "shadowsocks",
            ss_params("pass1"),
            "127.0.0.1",
            18411,
        )];
        controller.start_all(&rows).expect("first start works");
        // A real hold on the served port (standing in for our own
        // sidecar's socket): the re-apply must recognize its own ports
        // and skip them instead of crying foreign-holder.
        let held = std::net::TcpListener::bind("127.0.0.1:18411").expect("holds 18411");
        controller
            .start_all(&rows)
            .expect("re-apply skips own ports");
        let two = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                18411,
            ),
            fixture(
                "2",
                "Beta",
                "shadowsocks",
                ss_params("pass2"),
                "127.0.0.1",
                18412,
            ),
        ];
        controller.start_all(&two).expect("second row starts");
        assert!(controller.stop_one("1"), "stop one of two works");
        assert!(controller.is_running("2"), "other row keeps serving");
        // A genuinely foreign hold on a NEW port still fails typed.
        let _foreign = std::net::TcpListener::bind("127.0.0.1:18413").expect("holds 18413");
        let blocked = vec![fixture(
            "9",
            "Blocked",
            "shadowsocks",
            ss_params("pass9"),
            "127.0.0.1",
            18413,
        )];
        let error = controller
            .start_all(&blocked)
            .expect_err("foreign hold fails");
        assert!(
            error.to_string().contains("18413"),
            "names the port, got {error}"
        );
        drop(held);
    }

    /// Ticket 36 (live): a real stock sing-box 1.14.2 sidecar serves a
    /// single-row generated config with the injected clash controller; a
    /// throttled HTTP-proxy burst through the mixed inbound moves the
    /// clash totals (Bearer-authenticated; wrong secret gets 401). The
    /// row's upstream is a loopback TCP acceptor (accept + close): live
    /// probing showed sing-box only counts connection bytes once the
    /// outbound leg establishes, so an unreachable server counts nothing
    /// while an accepted-then-closed leg counts the inbound bytes. Hermetic
    /// by default — runs only with `MANYPROXY_LIVE_TRAFFIC=1` (loopback +
    /// TEMP files only, removed afterwards; the sidecar is killed before
    /// return).
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
        // Dumb upstream: accepts each outbound dial then closes (no
        // shadowsocks handshake — tunnel bytes are not the point, the
        // established leg that starts the inbound counting is). Bound
        // before the sidecar starts; the acceptor thread takes exactly the
        // signalled dial count then exits (nonblocking + deadline, so the
        // join below can never hang the suite).
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
        let check = std::process::Command::new(&binary)
            .arg("check")
            .arg("-c")
            .arg(&path)
            .output()
            .expect("check runs");
        let _ = std::fs::remove_file(&path);
        assert!(
            check.status.success(),
            "PROBE live-traffic: sing-box check EXIT 0, stderr: {}",
            String::from_utf8_lossy(&check.stderr)
        );
        std::fs::write(&path, &injected).expect("temp config writes");
        let mut child = std::process::Command::new(&binary)
            .arg("run")
            .arg("-c")
            .arg(&path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("sidecar runs");
        let result = (|| -> Result<String, String> {
            use std::io::{Read as _, Write as _};
            // The controller comes up with the sidecar — poll to readiness.
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
            // Wrong secret is rejected (live Bearer-auth proof).
            if crate::health::poll_clash_totals(clash_port, "wrong", Duration::from_secs(5)).is_ok()
            {
                return Err("clash accepted a bad secret".to_string());
            }
            // Throttled transfer: N plain HTTP-proxy GETs (the health-probe
            // byte shape) at an unreachable TEST-NET host. The tunnel leg
            // to the loopback acceptor establishes (starting the inbound
            // counting); the request bytes then count however the relay
            // ends.
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
            // Totals flush on connection close — allow a lag window.
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
                .unwrap_or(true),
            "no temp litter left"
        );
        result.expect("PROBE live-traffic: clash totals move");
    }
    /// through the tunnel): outbound-style stored keys become endpoint
    /// schema — peer in `peers[]`, list fields as lists, numerics as
    /// numbers, internal markers and client settings dropped. The fork
    /// rejects `server`/`dns`/`awg_version` and string `jmin` outright.
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

    /// Exotic protocols stay out of this slice with a named error.
    /// Ticket 27: AnyTLS/SSH/Noise are native now — the residual
    /// UnsupportedProtocol case is a genuinely unknown protocol.
    #[test]
    fn unsupported_protocol_names_config_without_secrets() {
        let configs = vec![fixture(
            "9",
            "Odd",
            "naive",
            json!({"server": "10.0.0.9", "server_port": 443, "username": "u", "password": "TOP-SECRET"}),
            "127.0.0.1",
            10809,
        )];
        let error = generate(&configs).expect_err("naive is not standard");
        let text = error.to_string();
        assert!(text.contains("\"Odd\""), "names config: {text}");
        assert!(text.contains("naive"), "names protocol: {text}");
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
    /// Ticket 23: stale generations are fenced out; a poisoned lock recovers
    /// with a named notice instead of a silent drop.
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

    /// Ticket 23: a previous generation's reader cannot interleave lines
    /// after a restart (generation fence).
    #[test]
    fn log_pipe_stale_generation_dropped() {
        let buffer = Arc::new(Mutex::new((1u64, VecDeque::new())));
        push_pipe_line(&buffer, 0, "stale line".to_string());
        push_pipe_line(&buffer, 1, "fresh line".to_string());
        let slot = buffer.lock().expect("lock works");
        let lines: Vec<&String> = slot.1.iter().collect();
        assert_eq!(lines, vec!["fresh line"], "stale dropped, got {lines:?}");
    }

    /// Ticket 23: a poisoned log lock recovers with a named notice line —
    /// never a silent drop.
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
        // Ticket 23: the unconfirmed start refuses the LAN bind; past the
        // confirm the bind starts and still warns.
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

    // -- Ticket 32: engine correctness (route arrays, custom agreement,
    // AWG coercion, stop snapshot; TDD at the generation seam) --------------

    /// Ticket 32: route rules carry `inbound` as an array (what stock
    /// sing-box 1.14.2 `check` accepts); one entry per endpoint, each
    /// resolving to its own outbound.
    #[test]
    fn t32_route_rules_use_inbound_array() {
        let generated = generate(&three_standard()).expect("standard set generates");
        let value = generated.value();
        let rules = value["route"]["rules"].as_array().expect("rules array");
        assert_eq!(rules.len(), 3);
        for (entry, rule) in generated.isolation.iter().zip(rules.iter()) {
            assert_eq!(rule["inbound"], json!([entry.inbound_tag]), "array form");
            assert_eq!(rule["outbound"], json!(entry.outbound_tag));
            assert_eq!(rule["action"], json!("route"));
        }
    }

    /// Ticket 32: custom full-config rows never conflict — `generate` and
    /// `validate_activation` agree (their bind is documentation, not an
    /// endpoint we serve).
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
        // Same bind+port, different cores: the pre-check plans cleanly.
        let rows = vec![
            custom_full("c1", "CustomA", "stock"),
            custom_full("c2", "CustomB", "fork"),
        ];
        validate_activation(&rows).expect("customs never conflict in validate");
        // `generate` serves no custom full-config, but it must agree on the
        // conflict verdict: never a PortConflict.
        let error = generate(&rows).expect_err("customs join no bare config");
        assert!(
            !matches!(error, EngineError::PortConflict { .. }),
            "generate agrees (no conflict), got {error}"
        );
        // A custom full-config sharing a standard row's endpoint agrees too.
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

    /// Ticket 32: stop with a failing re-gen restores the snapshot (active
    /// membership and the running sidecar agree) and records the error —
    /// never a silent half-state.
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

    /// Ticket 32: AWG string numerics (jc/jmin/jmax/s1-s4/h1-h4) coerce to
    /// numbers; h-ranges and i-chains pass through verbatim.
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

    /// Ticket 32 (live): every standard row shape the app emits passes the
    /// real stock sing-box 1.14.2 `check` (EXIT 0) — route arrays, `tls` /
    /// `transport` translation, endpoint reshape. Hermetic by default —
    /// runs only with `MANYPROXY_LIVE_CHECK=1` (TEMP files only, removed
    /// afterwards). The binary resolves via `MANYPROXY_SINGBOX_BIN` (else
    /// the debug build beside the crate). Rows come from the real link
    /// grammar so intake shapes are covered end to end.
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
                // Valid base64 key material plus numeric-ish endpoint opts.
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
                // Real keys but no address/allowed_ips: the allowed-ips
                // default carries it (address is optional to `check`).
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
    /// Ticket 32: meaningful-but-unmappable intake fails named (field +
    /// reason, never values) instead of emitting a config `check` rejects
    /// — or worse, a silently wrong connection.
    #[test]
    fn t32_translation_errors_name_field() {
        // Shadowsocks plugin: names an external executable.
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
        // Hysteria without speeds: sing-box requires up/down.
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
        // Hysteria2 realm variant: no sing-box mapping.
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
        // mKCP transport: no sing-box mapping.
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
        // Plaintext on a TLS-mandatory protocol.
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

    /// Ticket 32: intake spellings fold to sing-box names — `username` to
    /// `user` on ssh, string `alter_id` to a number on vmess.
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
        // The endpoint reshape drops internal markers (`awg_version` never
        // reaches the engine; the fork judges the matrix fields as given).
        assert!(
            endpoints[0].get("awg_version").is_none(),
            "no internal marker"
        );
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
        // Ticket 32: the xhttp marker folds into the `transport` object
        // the fork sidecar validates (no v2ray-flat top-level keys).
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
                .find(|rule| rule["inbound"] == json!([entry.inbound_tag.clone()]))
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

    /// Extra-core slots: app-dir-only resolution rejects traversal, a
    /// missing exe names the row + file, and N named slots each serve their
    /// own endpoint (ticket 27 replaces the ticket-08 singleton).
    /// The active rows' configs reach their sidecars unprivileged.
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
        // Ticket 27: N named slots — the second row starts beside the first
        // (its own sidecar, its own endpoint), no singleton cap.
        let _rival_extra = controller.push_test_extra_backend();
        controller
            .start_one(&rival)
            .expect("second extra slot starts beside the first");
        assert!(controller.is_running("5"), "first slot keeps serving");
        assert!(controller.is_running("6"), "second slot serves too");
        assert_eq!(controller.running_count(), 3);
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
            .find(|rule| rule["inbound"] == json!([inbound_tag("8")]))
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

    // -- Ticket 23: lifecycle hardening (TDD at the FakeEngineBackend seam) --

    /// Scenario: start with no binaries → typed Missing* error, no crash, no
    /// file left behind.
    #[test]
    fn t23_missing_binaries_fail_typed_without_side_effects() {
        let (mut controller, stock, fork, xray, extra) = EngineController::fake_quartet();
        controller.set_stock_probe(|| {
            Err(EngineError::MissingBinary {
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

        // The check path writes no secret file when the binary is missing…
        let mut real = RealEngineBackend::xray();
        let path = real.config_path.clone();
        assert!(
            real.check_config("{\"secret\": true}").is_err(),
            "no xray binary in test env"
        );
        assert!(!path.exists(), "no file left: {}", path.display());
        // …and write-then-remove leaves nothing behind either.
        let real = RealEngineBackend::new();
        let path = real.config_path.clone();
        real.write_config("{\"password\": \"hunter2\"}")
            .expect("restricted write works");
        assert!(path.exists());
        real.remove_config_file();
        assert!(!path.exists(), "removed: {}", path.display());
    }

    /// Scenario: one sidecar dies externally → the poll names its shard, its
    /// file is cleaned, siblings keep running. Every crashed shard is
    /// reported per poll (no N-1 loss).
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
        assert!(controller.poll().is_none(), "cause reported once");
        assert!(matches!(controller.status(), EngineStatus::Crashed { .. }));

        // Two simultaneous crashes are both reported, shard-named.
        stock.inject_crash("signal: segmentation fault");
        xray.inject_crash("signal: killed");
        // (xray is down already: re-arm it by restarting, then crash both)
        controller.set_xray_probe(|| Ok(PathBuf::from("test-xray-bin")));
        let _ = controller.start_all_confirmed(&rows);
        stock.inject_crash("signal: segmentation fault");
        xray.inject_crash("signal: killed");
        let cause = controller.poll().expect("crashes reported");
        assert!(cause.contains("segmentation fault"), "got {cause}");
        assert!(cause.contains("killed"), "got {cause}");
        assert!(cause.contains("stock"), "shard-named: {cause}");
        assert!(cause.contains("xray"), "shard-named: {cause}");
        assert!(controller.poll().is_none(), "causes reported once");
    }

    /// Scenario: custom full-config with nested tun/dns → rejected naming the
    /// key and its path (never values).
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

    /// SEC-MED: controller-level LAN enforcement, independent of any UI gate.
    /// (`0.0.0.0` stands in for a LAN bind: non-loopback, and bindable on
    /// any host so the port probe passes.)
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

        // The confirm sticks to (id, bind): an edited bind re-requires it.
        let mut moved = lan.clone();
        moved.bind_ip = "10.0.0.5".to_string();
        let error = controller
            .start_one(&moved)
            .expect_err("moved bind re-requires confirm");
        assert!(matches!(error, EngineError::LanConfirmRequired { .. }));
        // Loopback rows never trip the gate.
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

    /// HIGH: a failed spawn rolls the touched shards back to their previously
    /// served configs and restores the membership (no split brain, no lie).
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

        // The next spawn fails once (then the driver is healthy again, so the
        // rollback restore succeeds).
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

    /// MED: probe-bind catches a foreign holder before anything stops, and
    /// loopback aliases conflict (localhost ≡ 127.0.0.1).
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

    /// LOW: duplicated ids name the id instead of serving garbage.
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

    /// LOW: the emptied extra slot forgets its executable.
    #[test]
    fn t23_extra_exe_cleared_when_slot_empties() {
        let mut real = RealEngineBackend::extra_slot();
        real.set_extra_exe(PathBuf::from("user-core.exe"));
        assert!(real.extra_exe.is_some());
        real.clear_extra_exe();
        assert!(real.extra_exe.is_none(), "slot forgotten");
    }

    /// Unique temp paths per backend instance (pid-suffixed, never shared).
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

    /// Restricted writes + Drop teardown leave no secret file behind.
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

    // -- Ticket 27: SSH + AnyTLS + Noise end-to-end -------------------------

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

    /// SSH rows generate native stock `ssh` outbounds on their own endpoint
    /// (no fork, no UnsupportedProtocol, no secrets in errors).
    #[test]
    fn ticket27_ssh_generates_native_stock_outbound() {
        assert!(is_ssh_protocol("SSH"));
        assert!(!needs_fork(&ssh_fixture("1", "SshOne", 10801)));
        let generated = generate(&[ssh_fixture("1", "SshOne", 10801)]).expect("ssh generates");
        assert!(!generated.requires_fork, "native stock, not fork");
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

    /// AnyTLS rows generate native stock `anytls` outbounds (the old
    /// UnsupportedProtocol is gone).
    #[test]
    fn ticket27_anytls_generates_native_stock_outbound() {
        let generated =
            generate(&[anytls_fixture("9", "AnyOne", 10809)]).expect("anytls generates");
        assert!(!generated.requires_fork, "native stock, not fork");
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

    /// Standalone Noise rows ride the fork sidecar with explicit AWG-vs-Noise
    /// mismatch guards in both directions (plus Xray-noise on stock).
    #[test]
    fn ticket27_noise_routes_fork_with_mismatch_guards() {
        assert!(is_noise_protocol("noise"));
        assert!(needs_fork(&noise_fixture("3", "NoiseOne", 10803)));
        let generated =
            generate(&[noise_fixture("3", "NoiseOne", 10803)]).expect("noise generates");
        assert!(generated.requires_fork, "fork sidecar path");
        let value = generated.value();
        let node = value["outbounds"]
            .as_array()
            .expect("outbounds")
            .iter()
            .find(|node| node["tag"] == json!(outbound_tag("3")))
            .expect("noise outbound present");
        assert_eq!(node["type"], json!("noise"));

        // AWG obfuscation on a Noise row: explicit AWG-vs-Noise rejection.
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

        // Xray noise semantics on a stock row: explicit wrong-layer rejection.
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

        // Fork gate: a missing fork binary fails only the Noise row.
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

    /// Three extra-core rows start together with per-slot isolation (N named
    /// slots, each its own endpoint and sidecar config).
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

    // -- Ticket 29: VLESS `encryption` routing + ANSI-clean engine errors --

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

    /// Scenario 1: `encryption: "none"` stays stock with the key dropped.
    /// Stock sing-box 1.14.2 strict-decodes and rejects ANY `encryption` key
    /// on vless outbounds (`unknown field "encryption"`, live-proven), so the
    /// key must never reach the stock config.
    #[test]
    fn t29_vless_encryption_none_drops_key_stays_stock() {
        let row = t29_vless_row("t29a", "VlessNone", json!("none"), 18921);
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
        assert!(!generated.requires_xray, "stock sidecar serves it");
        let node = t29_generated_node(&generated.value(), &outbound_tag("t29a"));
        assert_eq!(node["type"], json!("vless"));
        assert!(
            !node.as_object().expect("object").contains_key("encryption"),
            "key dropped for stock: {node}"
        );
    }

    /// Rows without `encryption` are untouched (prohibited side effect).
    #[test]
    fn t29_vless_without_encryption_unchanged() {
        let row = fixture(
            "t29b",
            "VlessPlain",
            "vless",
            json!({"type": "vless", "server": "198.51.100.9", "server_port": 443,
                   "uuid": "11111111-2222-4333-8444-555555555555"}),
            "127.0.0.1",
            18922,
        );
        assert!(!needs_xray(&row), "plain vless stays stock");
        let generated = generate(std::slice::from_ref(&row)).expect("plain generates");
        assert!(!generated.requires_xray, "stock serves it");
        let node = t29_generated_node(&generated.value(), &outbound_tag("t29b"));
        assert_eq!(node["type"], json!("vless"));
        assert!(
            !node.as_object().expect("object").contains_key("encryption"),
            "no key invented: {node}"
        );
    }

    /// Blank and case-folded `"none"` also stay stock with no key emitted.
    #[test]
    fn t29_vless_encryption_blank_and_case_fold_stay_stock() {
        for (id, value) in [
            ("t29c", json!("None")),
            ("t29d", json!(" NONE ")),
            ("t29e", json!("")),
        ] {
            let row = t29_vless_row(id, "VlessFold", value, 18923);
            assert!(!needs_xray(&row), "{id} stays stock");
            let generated = generate(std::slice::from_ref(&row)).expect("stock generates");
            assert!(!generated.requires_xray, "{id} served by stock");
            let node = t29_generated_node(&generated.value(), &outbound_tag(id));
            assert!(
                !node.as_object().expect("object").contains_key("encryption"),
                "{id} key dropped: {node}"
            );
        }
    }

    /// Scenario 2: a real `encryption` value routes through the xray sidecar
    /// bridge (bridge pointer in the main config, verbatim `encryption` in
    /// the xray `users[]` entry). Synthetic value only — never key material.
    #[test]
    fn t29_vless_real_encryption_routes_xray_bridge() {
        let row = t29_vless_row("t29x", "VlessEnc", json!("synthetic-t29-encryption"), 18924);
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
        assert!(generated.requires_xray, "main knows the bridge");
        let pointer = t29_generated_node(&generated.value(), &outbound_tag("t29x"));
        assert_eq!(pointer["type"], json!("socks"), "bridge pointer: {pointer}");
        assert_eq!(pointer["server"], json!("127.0.0.1"));
        assert_eq!(
            pointer["server_port"].as_u64().expect("bridge port"),
            egress.bridge_port as u64,
            "pointer meets ingress"
        );
    }

    /// Scenario 2 detail: `flow` plus camelCase `packetEncoding` ride the
    /// xray leg per the xray schema (`streamSettings.packetEncoding`,
    /// accepted by xray 26.3.27 `run -test`).
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
            18925,
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

    /// A non-string `encryption` fails typed, naming the field and the xray
    /// sidecar — never a stock `unknown field` at check time, never values.
    #[test]
    fn t29_vless_encryption_nonstring_is_typed_error() {
        let row = t29_vless_row("t29z", "VlessBad", json!(42), 18926);
        let error = validate_activation(std::slice::from_ref(&row)).expect_err("typed");
        assert!(
            matches!(error, EngineError::BadCustom { .. }),
            "typed, got {error}"
        );
        let text = error.to_string();
        assert!(text.contains("\"encryption\""), "names the field: {text}");
        assert!(text.contains("xray"), "names the sidecar: {text}");
    }

    /// Scenario 2 end to end: the row is served via the xray sidecar on its
    /// own endpoint; the value never lands in the stock config.
    #[test]
    fn t29_vless_real_encryption_served_by_xray_sidecar() {
        let row = t29_vless_row(
            "t29w",
            "VlessSidecar",
            json!("synthetic-t29-encryption"),
            18927,
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

    /// Ticket 29: build an ANSI-colored `check` stderr line without invisible
    /// bytes in source (`char::from(27u8)` keeps the ESC byte visible).
    fn t29_ansi(detail: &str) -> String {
        detail.replace("[ESC]", &char::from(27u8).to_string())
    }

    /// Ticket 29: the ANSI stripper drops CSI sequences and keeps text
    /// (SGR colors, bold, multi-param, non-CSI escapes, trailing bare ESC).
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

    /// Scenario 3: engine `check` stderr arrives with ANSI color escapes; the
    /// surfaced error keeps the text and drops the codes.
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

    /// Scenario 3 end to end: an ANSI-colored `check` failure from any
    /// backend surfaces without escape codes (toast-safe).
    #[test]
    fn t29_controller_check_failure_has_no_ansi() {
        let row = fixture(
            "t29s",
            "AnsiRow",
            "shadowsocks",
            json!({"type": "shadowsocks", "server": "203.0.113.7",
                   "server_port": 8388, "method": "aes-128-gcm", "password": "pass1"}),
            "127.0.0.1",
            18928,
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

    // -- Ticket 37: routing presets --------------------------------------

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

    /// Ticket 37: no presets attached means byte-identical output to the
    /// pre-preset generator (unattached rows never change behavior).
    #[test]
    fn ticket37_generate_without_presets_is_byte_identical() {
        let rows = three_standard();
        let plain = generate(&rows).expect("generates");
        let empty = HashMap::new();
        let with_empty = generate_with_presets(&rows, &empty).expect("generates");
        assert_eq!(plain.json, with_empty.json, "empty map changes nothing");
    }

    /// Ticket 37: an attached preset merges its rules ahead of the row's
    /// isolation rule — route actions point at the row outbound, direct
    /// passes through, the Throne `block` spelling is served as `reject`
    /// (what stock 1.14.2 accepts), and every preset rule is scoped to the
    /// row inbound.
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
        // The isolation rule still closes the table.
        assert_eq!(rules[3]["inbound"], json!([in_tag]));
        assert_eq!(rules[3]["outbound"], json!(out_tag));
    }

    /// Ticket 37: a second row without a preset keeps exactly its isolation
    /// rule while the attached row carries its presets (no cross-row leak).
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

    /// Ticket 37: a dangling preset reference fails loudly naming the row
    /// (never a silent no-preset serve).
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

    /// Ticket 37: corrupt stored preset rules fail naming the rule index and
    /// the field (never values).
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

    /// Ticket 37: the controller serves preset rules ahead of the isolation
    /// rule on start (fake backend records the served config).
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

    /// Ticket 37: controller without preset rules keeps serving the
    /// isolation-only shape (unattached start path unchanged).
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

    /// Ticket 37 (live): an attached preset passes the real stock sing-box
    /// 1.14.2 `check` (EXIT 0) with the preset rules present before the
    /// isolation rule. Hermetic by default — runs only with
    /// `MANYPROXY_LIVE_CHECK=1` (TEMP files only, removed afterwards). The
    /// binary resolves via `MANYPROXY_SINGBOX_BIN` (else the debug build
    /// beside the crate).
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
}
