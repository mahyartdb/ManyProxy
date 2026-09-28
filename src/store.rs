//! Ticket 02: ProxyConfig library store.
//!
//! rusqlite-backed persistence for the config library. Every connection gets
//! the sqlite-skill PRAGMAs (WAL, busy_timeout, FK, synchronous, temp_store),
//! all queries are parameterized, and unit tests run on `:memory:` databases.
//! The store owns all validation so UI dialogs pass raw strings straight
//! through and render the returned field-level errors.

use rusqlite::{params, Connection};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::IpAddr;
use std::path::{Path, PathBuf};

/// Default loopback bind for new configs (spec: loopback default, LAN only
/// behind an explicit warning at activation time).
pub const DEFAULT_BIND: &str = "127.0.0.1";
/// Default group for new configs.
pub const DEFAULT_GROUP: &str = "default";
/// Default protocol when the add form leaves it blank.
pub const DEFAULT_PROTOCOL: &str = "vless";
/// Ticket 31: canonical stored protocol names (the
/// [`canonical_protocol`] output form). Typo errors name this list so a
/// mistyped protocol points at what exists; the import side has its own
/// lists (JSON types vs link schemes differ, so one shared list would lie on
/// some path).
pub const VALID_PROTOCOLS: &[&str] = &[
    "vless",
    "vmess",
    "trojan",
    "shadowsocks",
    "hysteria2",
    "hysteria",
    "tuic",
    "wireguard",
    "amneziawg",
    "socks",
    "anytls",
    "ssh",
    "noise",
    "http",
    "https",
    "naive",
    "xray",
    "custom-outbound",
    "custom-config",
    "extra-core",
    "chain",
];
/// Inclusive auto-port range for configs created without an explicit port.
pub const AUTO_PORT_START: u16 = 10801;
pub const AUTO_PORT_END: u16 = 10999;
/// Ticket 27: spillover auto-port range (spec: auto ports from the
/// 10801/18081 ranges). Primary fills first; spillover serves overflow so
/// 250 auto-port configs assign cleanly. Exhaustion is honest only past both
/// ranges. Disjoint from the Xray bridge range (11801-11999) by construction.
pub const AUTO_PORT_SPILL_START: u16 = 18081;
pub const AUTO_PORT_SPILL_END: u16 = 18380;

/// Ticket 27: total auto-port capacity across both ranges.
#[cfg_attr(not(test), allow(dead_code))]
pub const AUTO_PORT_CAPACITY: usize = (AUTO_PORT_END - AUTO_PORT_START + 1) as usize
    + (AUTO_PORT_SPILL_END - AUTO_PORT_SPILL_START + 1) as usize;

/// Ticket 27: auto-port candidates in assignment order (primary, then
/// spillover). Every auto assignment in this file funnels through here so
/// the two ranges never drift apart.
pub fn auto_port_candidates() -> impl Iterator<Item = u16> {
    (AUTO_PORT_START..=AUTO_PORT_END).chain(AUTO_PORT_SPILL_START..=AUTO_PORT_SPILL_END)
}

/// Ticket 27: true when a port was auto-assigned from either range
/// (explicit user ports elsewhere stay legal at rest).
#[cfg_attr(not(test), allow(dead_code))]
pub fn is_auto_port(port: u16) -> bool {
    (AUTO_PORT_START..=AUTO_PORT_END).contains(&port)
        || (AUTO_PORT_SPILL_START..=AUTO_PORT_SPILL_END).contains(&port)
}

/// Ticket 27: secrets-at-rest note. v1 encrypts nothing: the library DB and
/// sidecar config files hold private keys/tokens/passwords and are protected
/// by user-only file permissions ([`harden_file_permissions`], applied to the
/// DB file on open and to every sidecar config on write). OS-keychain/DPAPI
/// credential encryption is an explicit BACKLOG non-goal (spec R6): no
/// keychain/DPAPI code ships in v1; when it lands, this note and the export
/// warning are its entry points.
#[cfg_attr(not(test), allow(dead_code))]
pub const SECRETS_AT_REST_NOTE: &str = "secrets at rest: user-only file permissions (0600/ACL); OS-keychain/DPAPI encryption is backlog, not v1";

/// Ticket 27: best-effort user-only permissions for a secrets-bearing file
/// (library DB, sidecar configs). Unix sets 0600; Windows strips inherited
/// ACLs to the current user via `icacls` (best-effort: failures are ignored
/// so hardening never breaks open/write — the file stays usable in its
/// per-user dir). Never echoes file contents.
pub fn harden_file_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(windows)]
    {
        // Best-effort: `<user>:(R,W)` only, no inherited entries. The temp
        // dir and %APPDATA% are per-user by construction; this tightens the
        // file itself when icacls is available.
        let _ = std::process::Command::new("icacls")
            .arg(path)
            .arg("/inheritance:r")
            .arg("/grant:r")
            .arg("*S-1-5-32-544:(R)")
            .output();
        let _ = std::process::Command::new("icacls")
            .arg(path)
            .arg("/grant:r")
            .arg(format!(
                "{}:(R,W)",
                std::env::var("USERNAME").unwrap_or_else(|_| "OWNER RIGHTS".to_string())
            ))
            .output();
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
    }
}

/// Ticket 27: subscription-fetch policy for one URL (pure, no DNS, no fetch).
/// Decision (documented rationale):
/// - `https` to a public host: Allow (tokens ride TLS; the existing 8 MiB /
///   20 s / UTF-8 caps still apply at fetch).
/// - `http` (plaintext) to a public host: Warn — allowed for LAN-provider
///   compat, but query tokens leak in plaintext, so the UI surfaces the
///   reason (never the URL).
/// - Any `http(s)` URL whose host is loopback/private/link-local/unspecified
///   (`localhost`, `127.x`, `10/8`, `172.16/12`, `192.168/16`, `::1`, `::`,
///   `fe80::/10`, `fc00::/7`): Block — a pasted subscription URL must never
///   make the app probe local services (SSRF); local files enter via manual
///   import instead. Non-IP hostnames (e.g. `example.com`) are never blocked
///   here (no DNS at policy time).
///
/// Errors and reasons never echo the URL (tokens hide in queries).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscriptionFetchPolicy {
    Allow,
    Warn { reason: &'static str },
    Block { reason: &'static str },
}

/// Ticket 27: classify one subscription URL without fetching (see
/// [`SubscriptionFetchPolicy`] for the warn/block/allow rationale).
pub fn subscription_fetch_policy(url: &str) -> SubscriptionFetchPolicy {
    const HTTP_WARN: &str =
        "subscription uses plaintext http (tokens in the URL leak in plaintext; prefer https)";
    const INTERNAL_BLOCK: &str =
        "subscription targets an internal host (loopback/private/link-local are never fetched; import local files manually)";
    let trimmed = url.trim();
    let (scheme, after) = match trimmed.get(..7) {
        Some(prefix) if prefix.eq_ignore_ascii_case("http://") => ("http", &trimmed[7..]),
        _ => match trimmed.get(..8) {
            Some(prefix) if prefix.eq_ignore_ascii_case("https://") => ("https", &trimmed[8..]),
            _ => {
                return SubscriptionFetchPolicy::Block {
                    reason: "subscription url must use http(s)",
                };
            }
        },
    };
    let authority = after.split('/').next().unwrap_or(after);
    let host_part = authority.rsplit('@').next().unwrap_or(authority);
    // Bracketed IPv6 (`[::1]`, `[::1]:8080`) unwraps first — a naive `:`
    // split would shred the literal (see ticket-27 RED run).
    let host = if let Some(rest) = host_part.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        host_part.split(':').next().unwrap_or(host_part)
    };
    let host = host.trim();
    if host.is_empty() {
        return SubscriptionFetchPolicy::Block {
            reason: "subscription url must use http(s)",
        };
    }
    if is_internal_host(host) {
        return SubscriptionFetchPolicy::Block {
            reason: INTERNAL_BLOCK,
        };
    }
    if scheme == "http" {
        return SubscriptionFetchPolicy::Warn { reason: HTTP_WARN };
    }
    SubscriptionFetchPolicy::Allow
}

/// Ticket 27: true for loopback/private/link-local/unspecified hosts (IP
/// literals only; hostnames are never internal without DNS).
fn is_internal_host(host: &str) -> bool {
    let lower = host.trim().to_ascii_lowercase();
    if lower == "localhost" {
        return true;
    }
    if let Ok(ip) = lower.parse::<IpAddr>() {
        match ip {
            IpAddr::V4(v4) => {
                let octets = v4.octets();
                v4.is_loopback()
                    || v4.is_unspecified()
                    || v4.is_link_local()
                    || octets[0] == 10
                    || (octets[0] == 172 && (16..=31).contains(&octets[1]))
                    || (octets[0] == 192 && octets[1] == 168)
                    || octets[0] == 127
            }
            IpAddr::V6(v6) => {
                v6.is_loopback() || v6.is_unspecified() || v6.is_multicast()
                // fe80::/10 link-local, fc00::/7 unique-local
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                || (v6.segments()[0] & 0xfe00) == 0xfc00
            }
        }
    } else {
        false
    }
}

/// A stored proxy configuration (ticket 02 owns identity, naming, grouping,
/// enable flag, and local bind; protocol params are opaque until the
/// import/custom-core tickets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyConfig {
    /// Stable string id, never reused (AUTOINCREMENT rowid rendered as text).
    pub id: String,
    pub name: String,
    pub protocol: String,
    /// Opaque protocol params; schema-checked by a later ticket.
    pub params: String,
    pub group: String,
    pub enabled: bool,
    pub bind_ip: String,
    pub port: u16,
    /// Owning subscription id (ticket 04), if this row is subscription-managed.
    pub subscription_id: Option<String>,
    /// Normalized subscription identity (ticket 04), if owned.
    pub identity_key: Option<String>,
    /// Attached routing preset id (ticket 37), if any. `None` means no
    /// preset: the engine serves the row's isolation rule alone (unattached
    /// rows are byte-identical to before presets existed).
    pub preset_id: Option<String>,
}

/// Raw input for creating a config. All strings are unvalidated; empty
/// protocol/group/bind fall back to defaults, empty port means auto-assign.
#[derive(Debug, Clone)]
pub struct NewConfig {
    pub name: String,
    pub protocol: String,
    pub params: String,
    pub group: String,
    pub bind_ip: String,
    /// Raw port text; `""` (or whitespace) means auto-assign.
    pub port: String,
}

/// Raw patch for editing a config. `None` means unchanged; `Some("")` port
/// means auto-assign a fresh port.
#[derive(Debug, Clone, Default)]
pub struct ConfigPatch {
    pub name: Option<String>,
    pub protocol: Option<String>,
    pub params: Option<String>,
    pub group: Option<String>,
    pub bind_ip: Option<String>,
    pub port: Option<String>,
}

/// One field-level validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub field: &'static str,
    pub message: String,
}

impl FieldError {
    fn new(field: &'static str, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
        }
    }
}

/// Store-level failures.
#[derive(Debug)]
pub enum StoreError {
    /// One or more fields rejected; nothing was written.
    Invalid(Vec<FieldError>),
    /// No config with this id.
    NotFound(String),
    /// Ticket 14: filesystem failure (export writes — never the database).
    /// Carries the destination path so the UI names it instead of
    /// mislabeling the failure "database error".
    Io {
        path: PathBuf,
        message: String,
    },
    Sql(rusqlite::Error),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Invalid(errors) => {
                let parts: Vec<String> = errors
                    .iter()
                    .map(|e| format!("{}: {}", e.field, e.message))
                    .collect();
                write!(f, "invalid config ({})", parts.join("; "))
            }
            StoreError::NotFound(id) => write!(f, "config not found: {id}"),
            StoreError::Io { path, message } => {
                write!(f, "cannot write {}: {}", path.display(), message)
            }
            StoreError::Sql(error) => write!(f, "database error: {error}"),
        }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        StoreError::Sql(error)
    }
}

/// Ticket 25: full-fidelity payload behind [`Store::export_library_json`].
/// `outbounds` holds each import-shaped row's params object verbatim (the
/// import grammar reads only `outbounds`, so a re-import commits duplicates,
/// never adds); `library` carries every row's full params,
/// enabled flag, identity, sync baseline, adoptable flag, and row id;
/// `subscriptions` carries each record's id plus refresh metadata
/// (`last_refresh_at`/`last_error`/`failures`); `tombstones` rides along so
/// user-deleted rows stay deleted after a restore. This is the store half
/// of restorable export: the import/UI side reads these sections (ticket 26
/// territory) or [`Store::restore_library_json`] rebuilds them directly.
/// Ticket 28: the old slim `export_payload` wrapper is gone (zero callers;
/// every consumer takes the full payload).
pub fn export_payload_full(
    configs: &[ProxyConfig],
    subscriptions: &[Subscription],
    tombstones: &[(String, String)],
) -> String {
    let mut outbounds: Vec<serde_json::Value> = Vec::with_capacity(configs.len());
    for config in configs {
        if let Ok(serde_json::Value::Object(map)) =
            serde_json::from_str::<serde_json::Value>(&config.params)
        {
            if value_is_import_shaped(&map) {
                outbounds.push(serde_json::Value::Object(map.clone()));
            }
        }
    }
    let library: Vec<serde_json::Value> = configs
        .iter()
        .map(|config| {
            serde_json::json!({
                "id": config.id,
                "name": config.name,
                "protocol": config.protocol,
                "params": config.params,
                "group": config.group,
                "bind_ip": config.bind_ip,
                "port": config.port,
                "enabled": config.enabled,
                "subscription_id": config.subscription_id,
                "identity_key": config.identity_key,
            })
        })
        .collect();
    let subs: Vec<serde_json::Value> = subscriptions
        .iter()
        .map(|sub| {
            let failures: serde_json::Value = serde_json::from_str(&encode_failures(&sub.failures))
                .unwrap_or_else(|_| serde_json::Value::Array(Vec::new()));
            serde_json::json!({
                "id": sub.id,
                "label": sub.label,
                "url": sub.url,
                "group": sub.group,
                "auto_refresh": sub.auto_refresh,
                "last_refresh_at": sub.last_refresh_at,
                "last_error": sub.last_error,
                "failures": failures,
            })
        })
        .collect();
    let tombs: Vec<serde_json::Value> = tombstones
        .iter()
        .map(|(subscription_id, identity_key)| {
            serde_json::json!({
                "subscription_id": subscription_id,
                "identity_key": identity_key,
            })
        })
        .collect();
    serde_json::json!({
        "outbounds": outbounds,
        "library": library,
        "subscriptions": subs,
        "tombstones": tombs,
    })
    .to_string()
}

/// Ticket 12: durable latest-per-config health sample. The status view
/// (`HealthStore`) stays in memory for the session, but the latest sample
/// per row — latency plus the dead flag (an `error` present means dead) —
/// survives restart via the `health_snapshots` table (one row per config).
/// Logs stay session-only by ruling (never persisted here).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthSnapshot {
    pub config_id: String,
    pub at: i64,
    /// `"url-test"` or `"tcp-ping"` (see `health::HealthKind::as_str`).
    pub kind: String,
    pub latency_ms: Option<u64>,
    /// `Some` names the dead cause; `None` means the last sample was live.
    pub error: Option<String>,
}

/// Map one health snapshot row to a HealthSnapshot.
/// Ticket 25: negative stored latencies clamp to 0 (never wrap via `as u64`).
fn map_snapshot_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HealthSnapshot> {
    let latency_ms: Option<i64> = row.get(3)?;
    Ok(HealthSnapshot {
        config_id: row.get(0)?,
        at: row.get(1)?,
        kind: row.get(2)?,
        latency_ms: latency_ms.map(|ms| ms.max(0) as u64),
        error: row.get(4)?,
    })
}

/// Parse raw port text from a form field. Empty means auto-assign (`None`).
/// `Some(0)` never validates: it is rejected as out of range.
/// Ticket 28: kept separate from the import grammar's `parse_port` — form
/// input allows blank-means-auto and plain digits, while link/JSON ports
/// enforce the strict grammar (no leading zeros/signs, multi-port
/// first-wins); one parser would change rejections on one path.
pub fn parse_port_field(raw: &str) -> Result<Option<u16>, FieldError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    match trimmed.parse::<u32>() {
        Ok(port) if (1..=65_535).contains(&port) => Ok(Some(port as u16)),
        _ => Err(FieldError::new(
            "port",
            format!("port must be 1-65535, got '{trimmed}'"),
        )),
    }
}

/// Both proxy URL forms for a local endpoint (contract: copy action copies
/// both the `socks5://` and `http://` forms).
/// Ticket 32: IPv6 literals bracket (`socks5://[::1]:port`); IPv4 and
/// hostnames pass through untouched.
pub fn proxy_urls(bind_ip: &str, port: u16) -> (String, String) {
    let trimmed = bind_ip.trim();
    let host = if trimmed.contains(':') && !trimmed.starts_with('[') {
        format!("[{trimmed}]")
    } else {
        trimmed.to_string()
    };
    (
        format!("socks5://{host}:{port}"),
        format!("http://{host}:{port}"),
    )
}

/// Confirm-dialog description for removing a config. Names the config.
pub fn delete_confirm_text(name: &str) -> String {
    format!("Remove \"{name}\"? The stored configuration is deleted. Nothing else changes.")
}

/// Ticket 09: explicit warning copy for any user-owned plaintext export.
/// Ticket 11: the export confirm dialog shows this text verbatim (see
/// [`export_confirm_description`]) so private keys/tokens/passwords never
/// leave silently.
pub fn export_warning_text() -> &'static str {
    "Export carries private keys, tokens, and passwords in plaintext. Anyone with this file can use your proxies — store it somewhere safe."
}

/// Ticket 11: the export confirm dialog description. This indirection locks
/// the verbatim binding: the dialog must show [`export_warning_text`]
/// exactly (no paraphrase, no trimming).
pub fn export_confirm_description() -> &'static str {
    export_warning_text()
}

/// Map one result row to a ProxyConfig. All columns are bound by position
/// with explicit types (STRICT table guarantees the shapes).
/// Ticket 25: out-of-range ports fail loudly (never silent truncation via
/// `as u16`); a CHECK constraint guards new tables, this rejects drift in
/// old ones.
fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProxyConfig> {
    let id: i64 = row.get(0)?;
    let enabled: i64 = row.get(5)?;
    let port: i64 = row.get(7)?;
    if !(1..=65_535).contains(&port) {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            7,
            rusqlite::types::Type::Integer,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("stored port out of range: {port}"),
            )),
        ));
    }
    Ok(ProxyConfig {
        id: id.to_string(),
        name: row.get(1)?,
        protocol: row.get(2)?,
        // Ticket 35: the params column is sealed at rest; open it here —
        // the single read funnel, so every consumer sees plaintext.
        params: {
            let stored: String = row.get(3)?;
            crate::dpapi::unseal_params(&stored).map_err(|message| {
                rusqlite::Error::FromSqlConversionFailure(
                    3,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        message,
                    )),
                )
            })?
        },
        group: row.get(4)?,
        enabled: enabled != 0,
        bind_ip: row.get(6)?,
        port: port as u16,
        subscription_id: row.get(8)?,
        identity_key: row.get(9)?,
        preset_id: row.get(10)?,
    })
}

/// A stored routing preset (ticket 37): a named, ordered list of sing-box
/// route-rule objects applied per attached row. `rules_json` is the raw
/// rules array (routing policy, not credentials — DPAPI sealing stays on
/// `proxy_configs.params` only, ticket-35 behavior preserved).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutePreset {
    /// Stable string id, never reused (AUTOINCREMENT rowid rendered as text).
    pub id: String,
    pub name: String,
    /// Intake origin: `link` (`throne://route`), `url` (explicit fetch of a
    /// preset URL / `throne://remoteroute`), `file` (preset JSON file), or
    /// `manual` (hand-created in the manager).
    pub source: String,
    /// Origin URL for `url` presets (fetch target, explicit refresh only —
    /// never auto-fetched). `None` otherwise.
    pub source_url: Option<String>,
    /// Raw JSON array of route-rule objects.
    pub rules_json: String,
    /// Unix seconds of the last write (create / rule edit / refresh fill).
    pub updated_at: Option<i64>,
}

impl RoutePreset {
    /// Decode the stored rules array. Corrupt rows fail with a static
    /// message (never rule content).
    pub fn rules(&self) -> Result<Vec<serde_json::Value>, String> {
        match serde_json::from_str::<serde_json::Value>(&self.rules_json) {
            Ok(serde_json::Value::Array(rules)) => Ok(rules),
            _ => Err(format!(
                "preset \"{}\" holds invalid rules (re-import it)",
                self.name
            )),
        }
    }
}

/// Map one result row to a RoutePreset (STRICT table guarantees the shapes).
fn map_preset_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RoutePreset> {
    let id: i64 = row.get(0)?;
    Ok(RoutePreset {
        id: id.to_string(),
        name: row.get(1)?,
        source: row.get(2)?,
        source_url: row.get(3)?,
        rules_json: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

/// Fully validated + defaulted record content, ready for INSERT/UPDATE.
struct ValidatedConfig {
    name: String,
    protocol: String,
    params: String,
    group: String,
    bind_ip: String,
    port: u16,
}

impl ProxyConfig {
    /// Integer rowid behind the string id; -1 when the id is foreign
    /// (only used for UPDATE targets already loaded from this store).
    fn rowid(&self) -> i64 {
        self.id.parse().unwrap_or(-1)
    }
}

/// Non-empty after trimming, else push a `name` error and return None.
fn validated_name(raw: &str, errors: &mut Vec<FieldError>) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        errors.push(FieldError::new("name", "name must not be empty"));
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Blank input falls back to `default`; otherwise the trimmed value.
fn defaulted(raw: &str, default: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Blank means loopback default; otherwise the text must parse as an IP
/// literal (spec: loopback default; LAN binds gated at activation time).
fn validated_bind(raw: &str, errors: &mut Vec<FieldError>) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Some(DEFAULT_BIND.to_string());
    }
    match trimmed.parse::<IpAddr>() {
        Ok(_) => Some(trimmed.to_string()),
        Err(_) => {
            errors.push(FieldError::new(
                "bind_ip",
                format!("bind address must be an IP literal, got '{trimmed}'"),
            ));
            None
        }
    }
}

/// Ticket 16: canonical protocol for manual add/edit. Trims, lowercases, and
/// folds well-known aliases to the stored form (mirrors the import grammar's
/// `normalize_outbound_type` + Clash folds). Unknown names yield `None` so
/// the caller can raise a `protocol` field error.
/// Ticket 28: kept parallel to the grammar table on purpose — the grammar
/// folds link-only aliases (`hy2`, `wg`, `naive+https`) and raises taxonomy
/// errors, while this folds dialog input to stored form; sharing one table
/// would newly accept (or reject) protocols on one path.
fn canonical_protocol(raw: &str) -> Option<String> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "vless" => Some("vless".to_string()),
        "vmess" => Some("vmess".to_string()),
        "trojan" => Some("trojan".to_string()),
        "shadowsocks" | "ss" => Some("shadowsocks".to_string()),
        "hysteria2" => Some("hysteria2".to_string()),
        "hysteria" => Some("hysteria".to_string()),
        "tuic" => Some("tuic".to_string()),
        "wireguard" | "wg" => Some("wireguard".to_string()),
        "amneziawg" | "awg" | "amnezia-wg" => Some("amneziawg".to_string()),
        "socks" | "socks5" => Some("socks".to_string()),
        "anytls" => Some("anytls".to_string()),
        // Ticket 27: native SSH outbound (sing-box `ssh`) + standalone Noise
        // (fork sidecar; AWG-vs-Noise mismatch rejected in the engine).
        "ssh" => Some("ssh".to_string()),
        "noise" => Some("noise".to_string()),
        "http" => Some("http".to_string()),
        "https" => Some("https".to_string()),
        "naive" => Some("naive".to_string()),
        "xray" | "xray-vless" | "xray_vless" | "vless-xray" => Some("xray".to_string()),
        "custom"
        | "custom-out"
        | "custom-outbound"
        | "custom_outbound"
        | "custom-outbound-json" => Some("custom-outbound".to_string()),
        "custom-config" | "custom_config" | "custom-full" | "custom_full"
        | "custom-full-config" => Some("custom-config".to_string()),
        "extra" | "extra-core" | "extra_core" | "extracore" => Some("extra-core".to_string()),
        "chain" | "chained" | "chain-outbound" | "chain_outbound" => Some("chain".to_string()),
        _ => None,
    }
}

/// Ticket 31: per-protocol starter params for the guided add dialog (never
/// raw JSON first). Every template is a JSON object with the upstream
/// `server`/`server_port` slots plus the protocol's credential slot(s) left
/// blank — submitting it untouched fails in [`validate_protocol_params`]
/// naming the missing field, never silently. Unknown input falls back to a
/// bare server/port object (the commit then names the valid protocols).
pub fn protocol_params_template(protocol: &str) -> String {
    fn blank(params: &mut serde_json::Map<String, serde_json::Value>, key: &str) {
        params.insert(key.to_string(), serde_json::Value::String(String::new()));
    }
    let canonical = canonical_protocol(protocol).unwrap_or_else(|| protocol.trim().to_string());
    let port: u16 = match canonical.as_str() {
        "wireguard" | "amneziawg" => 51820,
        "ssh" => 22,
        "socks" => 1080,
        _ => 443,
    };
    let mut params = serde_json::Map::new();
    params.insert(
        "type".to_string(),
        serde_json::Value::String(canonical.clone()),
    );
    params.insert(
        "server".to_string(),
        serde_json::Value::String(String::new()),
    );
    params.insert(
        "server_port".to_string(),
        serde_json::Value::Number(port.into()),
    );
    match canonical.as_str() {
        "vless" | "vmess" | "xray" => blank(&mut params, "uuid"),
        "trojan" | "hysteria2" | "anytls" => blank(&mut params, "password"),
        "hysteria" => blank(&mut params, "auth"),
        "shadowsocks" => {
            blank(&mut params, "method");
            blank(&mut params, "password");
        }
        "tuic" => {
            blank(&mut params, "uuid");
            blank(&mut params, "password");
        }
        "wireguard" | "amneziawg" => {
            blank(&mut params, "private_key");
            blank(&mut params, "public_key");
        }
        "naive" | "ssh" => {
            blank(&mut params, "username");
            blank(&mut params, "password");
        }
        "noise" => blank(&mut params, "key"),
        _ => {}
    }
    serde_json::Value::Object(params).to_string()
}

/// Ticket 16: true when a params JSON object carries the import shape
/// (`type`/`protocol` + `server`/`address` + `server_port`/`port`). Same
/// predicate as [`export_payload`]: only these rows round-trip through the
/// import grammar; opaque rows (e.g. `"{}"`) stay metadata-only.
fn value_is_import_shaped(obj: &serde_json::Map<String, serde_json::Value>) -> bool {
    let has_kind = obj.contains_key("type") || obj.contains_key("protocol");
    let has_host = obj.contains_key("server") || obj.contains_key("address");
    let has_port = obj.contains_key("server_port") || obj.contains_key("port");
    has_kind && has_host && has_port
}

/// Ticket 16: true when `params` parses as a JSON object with the import
/// shape. Used by the export toast (re-importable count) and by manual
/// validation (only import-shaped rows get per-protocol credential checks,
/// so legacy opaque rows like `"{}"` keep working).
pub fn is_import_shaped_params(params: &str) -> bool {
    match serde_json::from_str::<serde_json::Value>(params) {
        Ok(serde_json::Value::Object(map)) => value_is_import_shaped(&map),
        _ => false,
    }
}

/// Ticket 16: number of rows whose params re-import through the grammar.
pub fn count_importable(configs: &[ProxyConfig]) -> usize {
    configs
        .iter()
        .filter(|config| is_import_shaped_params(&config.params))
        .count()
}

/// Ticket 28: kept case-insensitive on purpose — manual dialog params have
/// no canonical key casing, while the import side (`json_string`) matches
/// exact format keys; folding both to one lookup would newly accept or miss
/// keys on one path.
fn lookup_case_insensitive(
    obj: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    for wanted in keys {
        for (key, value) in obj {
            if key.eq_ignore_ascii_case(wanted) {
                match value {
                    serde_json::Value::String(text) if !text.trim().is_empty() => {
                        return Some(text.clone());
                    }
                    serde_json::Value::Number(number) => return Some(number.to_string()),
                    _ => {}
                }
            }
        }
    }
    None
}

/// Ticket 16: validate a manual protocol + params pair like the import path
/// does. Unknown protocols and non-JSON params are `protocol`/`params` field
/// errors (row not created). Import-shaped params additionally require the
/// per-protocol credential from research 05 (uuid/password/keys/…) plus a
/// sane server/port; opaque params (e.g. `"{}"`) pass through so legacy
/// hand rows and the `subscription_refresh_removes_only_owned` seed keep
/// working. Reality without `pbk` is rejected like the link parsers.
fn validate_protocol_params(
    protocol_raw: &str,
    params_raw: &str,
    errors: &mut Vec<FieldError>,
) -> Option<(String, String)> {
    let protocol_trimmed = protocol_raw.trim();
    let canonical = if protocol_trimmed.is_empty() {
        DEFAULT_PROTOCOL.to_string()
    } else {
        match canonical_protocol(protocol_trimmed) {
            Some(protocol) => protocol,
            None => {
                errors.push(FieldError::new(
                    "protocol",
                    format!(
                        "unknown protocol '{protocol_trimmed}' (valid: {})",
                        VALID_PROTOCOLS.join(", ")
                    ),
                ));
                return None;
            }
        }
    };
    let params_trimmed = params_raw.trim();
    let params_text = if params_trimmed.is_empty() {
        "{}".to_string()
    } else {
        params_trimmed.to_string()
    };
    let value: serde_json::Value = match serde_json::from_str(&params_text) {
        Ok(value) => value,
        Err(error) => {
            errors.push(FieldError::new(
                "params",
                format!("params must be JSON object ({error})"),
            ));
            return None;
        }
    };
    let obj = match value.as_object() {
        Some(obj) => obj,
        None => {
            errors.push(FieldError::new(
                "params",
                "params must be JSON object".to_string(),
            ));
            return None;
        }
    };
    if value_is_import_shaped(obj) {
        let server = lookup_case_insensitive(obj, &["server", "address"]).unwrap_or_default();
        if server.trim().is_empty() || server.chars().any(|c| c.is_whitespace() || c.is_control()) {
            errors.push(FieldError::new("params", "params need a valid server"));
            return None;
        }
        let port_ok = match obj
            .get("server_port")
            .or_else(|| obj.get("port"))
            .or_else(|| obj.get("serverPort"))
        {
            Some(serde_json::Value::Number(number)) => number
                .as_u64()
                .is_some_and(|port| (1..=65_535).contains(&port)),
            Some(serde_json::Value::String(text)) => text
                .trim()
                .parse::<u32>()
                .is_ok_and(|port| (1..=65_535).contains(&port)),
            _ => false,
        };
        if !port_ok {
            errors.push(FieldError::new(
                "params",
                "params need a valid server_port (1-65535)",
            ));
            return None;
        }
        let missing =
            |what: &str| FieldError::new("params", format!("{canonical} params need {what}"));
        let has = |keys: &[&str]| lookup_case_insensitive(obj, keys).is_some();
        let credential_ok = match canonical.as_str() {
            "vless" | "xray" | "vmess" => has(&["uuid", "id"]),
            "trojan" => has(&["password"]),
            "shadowsocks" => has(&["method", "cipher"]) && has(&["password"]),
            "hysteria2" => has(&["password", "auth"]),
            "hysteria" => has(&["auth", "password"]),
            "tuic" => has(&["uuid", "id"]) && has(&["password"]),
            "wireguard" | "amneziawg" => {
                has(&["private_key", "privatekey"]) && has(&["public_key", "publickey"])
            }
            "anytls" => has(&["password"]),
            "naive" => has(&["username", "user"]) && has(&["password"]),
            // Ticket 27: SSH needs a user plus one secret (password or
            // private key); standalone Noise needs one noise secret
            // (password or key) so rows never connect nowhere silently.
            "ssh" => has(&["username", "user"]) && has(&["password", "private_key", "privatekey"]),
            "noise" => has(&["password", "key"]),
            _ => true,
        };
        if !credential_ok {
            let what = match canonical.as_str() {
                "vless" | "xray" | "vmess" => "uuid",
                "trojan" => "password",
                "shadowsocks" => "method and password",
                "hysteria2" => "password",
                "hysteria" => "auth",
                "tuic" => "uuid and password",
                "wireguard" | "amneziawg" => "private_key and public_key",
                "anytls" => "password",
                "naive" => "username and password",
                "ssh" => "username and a password or private_key",
                "noise" => "password or key",
                _ => "credentials",
            };
            errors.push(missing(what));
            return None;
        }
        if lookup_case_insensitive(obj, &["security"])
            .is_some_and(|security| security.eq_ignore_ascii_case("reality"))
            && !has(&["pbk"])
        {
            errors.push(missing("pbk (required for reality)"));
            return None;
        }
    }
    Some((canonical, params_text))
}
/// Ticket 04: a subscription refresh source. Refresh upserts ProxyConfigs in
/// `group` keyed by normalized identity (never the remark); it never deletes
/// hand-created configs and never enables or starts anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscription {
    /// Stable string id, never reused.
    pub id: String,
    pub label: String,
    pub url: String,
    pub group: String,
    pub auto_refresh: bool,
    /// Unix seconds of the last successful refresh, if any.
    pub last_refresh_at: Option<i64>,
    /// Last fetch/parse failure, cleared on the next success.
    pub last_error: Option<String>,
    /// Ticket 17: latest per-item refresh failures (stored vec behind the
    /// per-subscription failures view; cleared on a clean refresh, kept on a
    /// fetch error which parsed nothing new).
    pub failures: Vec<crate::import::ImportFailure>,
}

/// Ticket 04: outcome of one refresh merge. `error` is `Some` when the body
/// failed to parse, in which case prior members were kept untouched.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefreshOutcome {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub duplicates: usize,
    /// Desired rows skipped because the user deleted them (tombstoned).
    pub skipped: usize,
    /// Ticket 25: owned rows where the user changed params/protocol AND
    /// upstream changed them too (or rotated identity under a user edit).
    /// The user's version is kept in place — never silently reverted — and
    /// each such row counts here exactly once so the UI can surface it.
    pub conflicts: usize,
    pub failures: Vec<crate::import::ImportFailure>,
    pub error: Option<String>,
}

/// Default auto-refresh interval for subscriptions (ticket 04 scheduling
/// hook): daily. The hook is [`due_for_refresh`]; the host decides when to
/// evaluate it (v1: at startup for due subscriptions, plus manual refresh).
pub const DEFAULT_AUTO_REFRESH_SECS: i64 = 86_400;

/// Current Unix time in seconds (refresh bookkeeping).
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// Auto-refresh scheduling hook: true when the subscription wants automatic
/// refresh and has never succeeded or its last success is older than
/// `interval_secs`.
pub fn due_for_refresh(subscription: &Subscription, now: i64, interval_secs: i64) -> bool {
    if !subscription.auto_refresh {
        return false;
    }
    match subscription.last_refresh_at {
        None => true,
        Some(last) => interval_secs <= 0 || now.saturating_sub(last) >= interval_secs,
    }
}

/// Map one subscription row to a Subscription.
fn map_subscription_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Subscription> {
    let id: i64 = row.get(0)?;
    let auto_refresh: i64 = row.get(4)?;
    let failures_raw: Option<String> = row.get(7)?;
    Ok(Subscription {
        id: id.to_string(),
        label: row.get(1)?,
        url: row.get(2)?,
        group: row.get(3)?,
        auto_refresh: auto_refresh != 0,
        last_refresh_at: row.get(5)?,
        last_error: row.get(6)?,
        failures: decode_failures(failures_raw),
    })
}

/// Ticket 17: encode per-item refresh failures for the
/// `subscriptions.failures` column (JSON array of
/// `{index, label, reason, code}`; labels/reasons arrive redacted from the
/// import path, so no secrets reach the database).
fn encode_failures(failures: &[crate::import::ImportFailure]) -> String {
    let items: Vec<serde_json::Value> = failures
        .iter()
        .map(|failure| {
            serde_json::json!({
                "index": failure.index,
                "label": failure.label,
                "reason": failure.reason,
                "code": failure.code.as_str(),
            })
        })
        .collect();
    serde_json::Value::Array(items).to_string()
}

/// Ticket 17: decode the `subscriptions.failures` column. Any corruption
/// (foreign rows, hand edits, older databases) yields no failures — never a
/// crash, never an error.
fn decode_failures(raw: Option<String>) -> Vec<crate::import::ImportFailure> {
    use crate::import::{ImportErrorKind, ImportFailure};
    let text = raw.unwrap_or_default();
    if text.trim().is_empty() {
        return Vec::new();
    }
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    if let Some(items) = value.as_array() {
        for item in items {
            // Ticket 25: saturating conversion (never truncate a hostile u64
            // on 32-bit targets); import.rs owns its own half.
            let index = item
                .get("index")
                .and_then(serde_json::Value::as_u64)
                .map(|index| usize::try_from(index).unwrap_or(usize::MAX))
                .unwrap_or(0);
            let label = item
                .get("label")
                .and_then(|value| value.as_str())
                .unwrap_or("item")
                .to_string();
            let reason = item
                .get("reason")
                .and_then(|value| value.as_str())
                .unwrap_or("refresh failed")
                .to_string();
            let code = item
                .get("code")
                .and_then(|value| value.as_str())
                .and_then(ImportErrorKind::from_str)
                .unwrap_or(ImportErrorKind::InvalidJson);
            out.push(ImportFailure {
                index,
                label,
                reason,
                code,
            });
        }
    }
    out
}

/// Ticket 25: widened subscription identity. The import grammar's
/// [`crate::import::normalized_identity`] deliberately excludes
/// transport/security/flow/fingerprint fields (same server with a flow-only
/// change is one row for paste dedupe). Refresh merging needs the opposite:
/// two upstream nodes that share the narrow identity but differ in
/// transport/security/flow/fp/pbk/alpn/plugin are two different nodes and
/// must not over-merge into one row. This key is the narrow key plus those
/// fields (case-insensitive lookup, enum-like values lowercased, secrets
/// verbatim like the narrow key). Non-JSON params fall back exactly like the
/// narrow key. The cutover of `commit_batch` to this key needs an import.rs
/// change (ticket 26 territory); refresh uses it today.
pub fn widened_identity(protocol: &str, params: &str) -> String {
    fn get(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<String> {
        keys.iter().find_map(|wanted| {
            obj.iter().find_map(|(key, value)| {
                if key.eq_ignore_ascii_case(wanted) {
                    match value {
                        serde_json::Value::String(text) if !text.trim().is_empty() => {
                            Some(text.clone())
                        }
                        serde_json::Value::Number(number) => Some(number.to_string()),
                        serde_json::Value::Bool(flag) => Some(flag.to_string()),
                        _ => None,
                    }
                } else {
                    None
                }
            })
        })
    }
    let narrow = crate::import::normalized_identity(protocol, params);
    let parsed: Option<serde_json::Map<String, serde_json::Value>> =
        serde_json::from_str::<serde_json::Value>(params)
            .ok()
            .and_then(|value| value.as_object().cloned());
    let mut extras: Vec<String> = match &parsed {
        Some(map) => [
            (["transport", "network", "net"].as_slice(), true),
            (["security"].as_slice(), true),
            (["flow"].as_slice(), true),
            (["fp", "fingerprint"].as_slice(), true),
            (["pbk"].as_slice(), false),
            (["alpn"].as_slice(), false),
            (["plugin"].as_slice(), false),
        ]
        .iter()
        .map(|(keys, lower)| {
            get(map, keys)
                .map(|value| {
                    let trimmed = value.trim().to_string();
                    if *lower {
                        trimmed.to_ascii_lowercase()
                    } else {
                        trimmed
                    }
                })
                .unwrap_or_default()
        })
        .collect(),
        None => Vec::new(),
    };
    // Ticket 32: AmneziaWG obfuscation keys join the identity — two rows
    // sharing server+creds but a different `jc` (or s/h/i junk tune) are
    // two nodes, never over-merged into one. Appended only for AWG-like
    // rows (protocol alias or any AWG-layer key present), so every other
    // protocol's key is byte-identical to before (no re-key churn for
    // stored identities). Values stay verbatim like the narrow key.
    if let Some(map) = &parsed {
        if is_awg_like(protocol, map) {
            // Declared version first (`awg_version`, else legacy `version`),
            // then every obfuscation slot in matrix order.
            extras.push(
                get(map, &["awg_version", "version"])
                    .map(|value| value.trim().to_string())
                    .unwrap_or_default(),
            );
            for key in AWG_IDENTITY_KEYS {
                extras.push(
                    get(map, &[key])
                        .map(|value| value.trim().to_string())
                        .unwrap_or_default(),
                );
            }
        }
    }
    format!("{narrow}\0{}", extras.join("\0"))
}

/// Ticket 32: AWG obfuscation slots behind [`widened_identity`] — the v2
/// junk/length/header/CPS matrix plus the 3.x additions (same key sets the
/// engine's version matrix validates).
const AWG_IDENTITY_KEYS: &[&str] = &[
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
    "i1",
    "i2",
    "i3",
    "i4",
    "i5",
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

/// Ticket 32: true for AmneziaWG-like rows — the protocol alias, or any
/// AWG-layer key carried in the params (case-insensitive; links and conf
/// files spell them `Jc`, `S1`, …).
fn is_awg_like(protocol: &str, params: &serde_json::Map<String, serde_json::Value>) -> bool {
    match protocol.trim().to_ascii_lowercase().as_str() {
        "amneziawg" | "awg" | "amnezia-wg" => return true,
        _ => {}
    }
    params.keys().any(|key| {
        let lower = key.to_ascii_lowercase();
        lower == "awg_version" || AWG_IDENTITY_KEYS.contains(&lower.as_str())
    })
}

/// Ticket 25: rotation-link core for in-place identity rotation: normalized
/// server + numeric port, WITHOUT protocol or credentials. When upstream
/// rotates identity (flow/security/password/protocol change on the same
/// server:port), the vanished owned row and the unmatched desired item share
/// this core, so the merge updates in place (keeps id/port/enabled) instead
/// of delete + re-add. `None` for non-JSON params (never rotation-matched).
fn rotation_core(params: &str) -> Option<String> {
    let map = serde_json::from_str::<serde_json::Value>(params)
        .ok()?
        .as_object()?
        .clone();
    let get = |keys: &[&str]| -> Option<String> {
        keys.iter().find_map(|wanted| {
            map.iter().find_map(|(key, value)| {
                if key.eq_ignore_ascii_case(wanted) {
                    match value {
                        serde_json::Value::String(text) if !text.trim().is_empty() => {
                            Some(text.clone())
                        }
                        serde_json::Value::Number(number) => Some(number.to_string()),
                        _ => None,
                    }
                } else {
                    None
                }
            })
        })
    };
    let server = get(&["server", "address"]).map(|server| {
        let trimmed = server.trim();
        let unbracketed = trimmed
            .strip_prefix('[')
            .and_then(|h| h.strip_suffix(']'))
            .unwrap_or(trimmed);
        unbracketed.trim_end_matches('.').to_ascii_lowercase()
    })?;
    if server.is_empty() {
        return None;
    }
    let port = get(&["server_port", "port"])?;
    if port.trim().is_empty() {
        return None;
    }
    Some(format!("{server}\0{}", port.trim()))
}

/// Ticket 25: outcome of [`Store::restore_library_json`].
/// (Test-driven API for ticket 26; see the callee's note.)
#[allow(dead_code)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RestoreOutcome {
    pub configs: usize,
    pub subscriptions: usize,
    pub tombstones: usize,
}

/// Ticket 25 (L4): same-directory tmp sibling for atomic export writes.
fn tmp_export_path(path: &Path) -> PathBuf {
    let tmp_name = format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("manyproxy-export.json"),
        std::process::id()
    );
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(tmp_name),
        _ => PathBuf::from(tmp_name),
    }
}

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open a throwaway store for unit tests.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::open(Connection::open_in_memory()?)
    }

    /// Ticket 14 test seam: drop the main tables so reads fail with
    /// `StoreError::Sql` (lets UI tests trigger the DB-error paths with a
    /// failing store instead of a silent empty list).
    #[cfg(test)]
    pub fn break_schema_for_test(&self) -> Result<(), StoreError> {
        self.conn.execute_batch(
            "DROP TABLE proxy_configs; DROP TABLE subscriptions; DROP TABLE route_presets;",
        )?;
        Ok(())
    }

    /// Ticket 14 test seam: read-only mode so writes fail with
    /// `StoreError::Sql` while reads keep working (lets UI tests select a
    /// row first, then fail the toggle write).
    #[cfg(test)]
    pub fn set_query_only_for_test(&self) -> Result<(), StoreError> {
        self.conn.execute_batch("PRAGMA query_only = ON;")?;
        Ok(())
    }

    /// Open (creating parents as needed) a file-backed store.
    /// Ticket 27: the DB file holds secrets at rest — user-only permissions
    /// apply on open (best-effort, never fails the open).
    pub fn open_file(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    StoreError::Sql(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
                })?;
            }
        }
        let store = Self::open(Connection::open(path)?)?;
        harden_file_permissions(path);
        // WAL sidecars (`<db>-wal`, `<db>-shm`) hold the same secrets;
        // tighten them too when present.
        for sibling in [
            PathBuf::from(format!("{}-wal", path.display())),
            PathBuf::from(format!("{}-shm", path.display())),
        ] {
            if sibling.exists() {
                harden_file_permissions(&sibling);
            }
        }
        Ok(store)
    }

    /// Default file location: `%APPDATA%/ManyProxy/library.db` on Windows,
    /// `$HOME/.manyproxy/library.db` elsewhere. `MANYPROXY_DB` overrides.
    pub fn default_path() -> PathBuf {
        if let Ok(path) = std::env::var("MANYPROXY_DB") {
            if !path.trim().is_empty() {
                return PathBuf::from(path);
            }
        }
        #[cfg(windows)]
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("ManyProxy").join("library.db");
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".manyproxy").join("library.db")
    }

    fn open(conn: Connection) -> Result<Self, StoreError> {
        let mut store = Self { conn };
        store.apply_pragmas()?;
        store.create_schema()?;
        Ok(store)
    }

    /// sqlite-skill setup: must run on every connection.
    fn apply_pragmas(&self) -> Result<(), StoreError> {
        self.conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;
             PRAGMA temp_store = MEMORY;",
        )?;
        Ok(())
    }

    /// Ticket 25: schema version behind `PRAGMA user_version`. Version 1 is
    /// every table/column through ticket 17; version 2 adds the ticket-25
    /// ownership columns (`adoptable`, `synced_params`); version 3 seals
    /// params at rest (ticket 35, DPAPI); version 4 adds routing presets
    /// (ticket 37: `route_presets` table + `proxy_configs.preset_id`). Fresh
    /// and legacy (`user_version = 0`) databases migrate forward
    /// idempotently; a database from a NEWER build fails loudly (never a
    /// silent empty).
    const SCHEMA_VERSION: i64 = 4;
    /// Minimum SQLite for this schema: STRICT tables need 3.37+.
    const MIN_SQLITE: (i64, i64, i64) = (3, 37, 0);

    /// Ticket 35: seal params for storage. A DPAPI failure aborts the
    /// write loudly (never silent plaintext).
    fn seal_or_fail(plaintext: &str) -> Result<String, StoreError> {
        crate::dpapi::seal_params(plaintext).map_err(|message| {
            StoreError::Sql(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::new(std::io::ErrorKind::InvalidData, message),
            )))
        })
    }

    /// Ticket 35: open stored params. Sealed values decrypt; legacy JSON
    /// passes through; anything else fails loudly.
    fn unseal_or_fail(stored: &str) -> Result<String, StoreError> {
        crate::dpapi::unseal_params(stored).map_err(|message| {
            StoreError::Sql(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::new(std::io::ErrorKind::InvalidData, message),
            )))
        })
    }

    /// Ticket 25: loud schema failure (newer-than-supported version, too-old
    /// SQLite, post-migration drift). Reuses `StoreError::Sql` so the public
    /// API stays additive; the message names the versions and the fix.
    fn schema_error(message: String) -> StoreError {
        StoreError::Sql(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::new(std::io::ErrorKind::InvalidData, message),
        )))
    }

    /// Ticket 25: assert the runtime SQLite meets [`Store::MIN_SQLITE`].
    fn assert_sqlite_version(&self) -> Result<(), StoreError> {
        let text: String = self
            .conn
            .query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
        let mut parts = text.split('.').map(|part| part.parse::<i64>().unwrap_or(0));
        let version = (
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
        );
        if version < Self::MIN_SQLITE {
            return Err(Self::schema_error(format!(
                "SQLite {text} is too old: ManyProxy needs {}.{}.{}+ (delete or upgrade the runtime, not the database)",
                Self::MIN_SQLITE.0,
                Self::MIN_SQLITE.1,
                Self::MIN_SQLITE.2,
            )));
        }
        Ok(())
    }

    fn column_exists(&self, table: &str, column: &str) -> Result<bool, StoreError> {
        // Table names are hardcoded at the call sites (never user input).
        let mut stmt = self.conn.prepare(&format!(
            "SELECT name FROM pragma_table_info('{table}') WHERE name = ?1"
        ))?;
        Ok(stmt.exists(params![column])?)
    }

    fn ensure_column(&self, table: &str, column: &str, ddl: &str) -> Result<(), StoreError> {
        if !self.column_exists(table, column)? {
            self.conn
                .execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {ddl};"))?;
        }
        Ok(())
    }

    fn create_schema(&mut self) -> Result<(), StoreError> {
        self.assert_sqlite_version()?;
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > Self::SCHEMA_VERSION {
            return Err(Self::schema_error(format!(
                "library database schema version {version} is newer than this build supports ({}): open it with a newer ManyProxy instead of wiping it",
                Self::SCHEMA_VERSION,
            )));
        }
        // `grp` avoids the reserved word GROUP. Ports are intentionally NOT
        // unique: two stored configs may share a port; the conflict error
        // naming both parties is raised at activation time (ticket 03).
        // Ticket 25: CHECK pins the valid range on new tables (old tables
        // are covered by the read-time reject in `map_row`).
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS proxy_configs (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               name TEXT NOT NULL,
               protocol TEXT NOT NULL DEFAULT 'vless',
               params TEXT NOT NULL DEFAULT '{}',
               grp TEXT NOT NULL DEFAULT 'default',
               enabled INTEGER NOT NULL DEFAULT 0,
               bind_ip TEXT NOT NULL DEFAULT '127.0.0.1',
               port INTEGER NOT NULL CHECK (port BETWEEN 1 AND 65535),
               subscription_id TEXT,
               identity_key TEXT,
               adoptable INTEGER NOT NULL DEFAULT 0,
               synced_params TEXT
             ) STRICT;
             CREATE INDEX IF NOT EXISTS idx_proxy_configs_enabled
               ON proxy_configs(enabled);
             CREATE INDEX IF NOT EXISTS idx_proxy_configs_subscription
               ON proxy_configs(subscription_id);
             CREATE INDEX IF NOT EXISTS idx_proxy_configs_identity
               ON proxy_configs(subscription_id, identity_key);
             CREATE TABLE IF NOT EXISTS subscriptions (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               label TEXT NOT NULL,
               url TEXT NOT NULL,
               grp TEXT NOT NULL DEFAULT 'default',
               auto_refresh INTEGER NOT NULL DEFAULT 1,
               last_refresh_at INTEGER,
               last_error TEXT,
               failures TEXT NOT NULL DEFAULT '[]'
             ) STRICT;
             CREATE TABLE IF NOT EXISTS subscription_tombstones (
               subscription_id TEXT NOT NULL,
               identity_key TEXT NOT NULL,
               PRIMARY KEY (subscription_id, identity_key)
             ) STRICT;
             CREATE TABLE IF NOT EXISTS health_snapshots (
               config_id TEXT PRIMARY KEY,
               at INTEGER NOT NULL,
               kind TEXT NOT NULL DEFAULT 'url-test',
               latency_ms INTEGER,
               error TEXT
             ) STRICT;
              CREATE TABLE IF NOT EXISTS route_presets (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                source TEXT NOT NULL DEFAULT 'manual',
                source_url TEXT,
                rules_json TEXT NOT NULL DEFAULT '[]',
                updated_at INTEGER
              ) STRICT;
              CREATE INDEX IF NOT EXISTS idx_route_presets_name
                ON route_presets(name);",
        )?;
        // Ticket 37: per-row preset attachment (null = none; validated on
        // attach, so no FK — a dangling id fails loudly at merge time).
        self.ensure_column("proxy_configs", "preset_id", "TEXT")?;
        // Migrate pre-ticket-04 databases that lack the new columns.
        for (column, ddl) in [("subscription_id", "TEXT"), ("identity_key", "TEXT")] {
            self.ensure_column("proxy_configs", column, ddl)?;
        }
        // Ticket 17: migrate pre-ticket-17 databases that lack the per-item
        // refresh failures column.
        self.ensure_column("subscriptions", "failures", "TEXT NOT NULL DEFAULT '[]'")?;
        // Ticket 25 (version 2): ownership columns for the refresh merge —
        // `adoptable` marks rows released by `remove_subscription` so a
        // second subscription can adopt them; `synced_params` is the
        // last-synced upstream baseline behind the user-vs-upstream compare.
        self.ensure_column("proxy_configs", "adoptable", "INTEGER NOT NULL DEFAULT 0")?;
        self.ensure_column("proxy_configs", "synced_params", "TEXT")?;
        // Ticket 36: cumulative data-plane byte counters (clash-API
        // sourced; probe bytes never land here). Plain ADD COLUMN expand —
        // no version bump (same precedent as the ticket-25 columns).
        self.ensure_column("proxy_configs", "traffic_rx", "INTEGER NOT NULL DEFAULT 0")?;
        self.ensure_column("proxy_configs", "traffic_tx", "INTEGER NOT NULL DEFAULT 0")?;
        // Ticket 35 (version 3): seal params at rest. Legacy JSON values
        // seal in one transaction; already-sealed rows are untouched, so the
        // migration is idempotent and reruns are no-ops.
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < 3 {
            let tx = self.conn.transaction()?;
            for column in ["params", "synced_params"] {
                let mut stmt = tx.prepare(&format!(
                    "SELECT id, {column} FROM proxy_configs WHERE {column} IS NOT NULL"
                ))?;
                let rows = stmt
                    .query_map([], |row| {
                        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                for (id, value) in rows {
                    if value.trim_start().starts_with('{') {
                        let sealed = Self::seal_or_fail(&value)?;
                        tx.execute(
                            &format!("UPDATE proxy_configs SET {column} = ?1 WHERE id = ?2"),
                            rusqlite::params![sealed, id],
                        )?;
                    }
                }
            }
            tx.commit()?;
        }
        // Unknown drift fails loudly: every column the merge relies on must
        // exist after migration, never a silent empty read.
        for (table, column) in [
            ("proxy_configs", "subscription_id"),
            ("proxy_configs", "identity_key"),
            ("proxy_configs", "adoptable"),
            ("proxy_configs", "synced_params"),
            ("proxy_configs", "preset_id"),
            ("subscriptions", "failures"),
        ] {
            if !self.column_exists(table, column)? {
                return Err(Self::schema_error(format!(
                    "library database is missing {table}.{column} after migration: restore from an export instead of running on a half-migrated file"
                )));
            }
        }
        self.conn
            .execute_batch(&format!("PRAGMA user_version = {};", Self::SCHEMA_VERSION))?;
        Ok(())
    }

    /// Create a disabled config; invalid input is rejected with field errors.
    /// Ticket 25: implemented on [`Store::add_batch`] (single-row batch —
    /// one transaction, identical validation).
    pub fn add(&mut self, input: NewConfig) -> Result<ProxyConfig, StoreError> {
        let mut created = self.add_batch(vec![input])?;
        created
            .pop()
            .ok_or_else(|| StoreError::NotFound("batch was empty".to_string()))
    }

    /// Ticket 25: create many disabled configs in ONE transaction (the store
    /// half of "batch inserts off the UI thread": one tx instead of ~1000
    /// statements; rewiring `commit_batch` and the `spawn_blocking` move is
    /// import.rs/main.rs territory, ticket 26). Validation runs fully before
    /// any write, so one bad item rejects the whole batch and writes nothing.
    /// Auto ports assign sequentially from a single taken-set snapshot.
    pub fn add_batch(&mut self, inputs: Vec<NewConfig>) -> Result<Vec<ProxyConfig>, StoreError> {
        struct Fields {
            name: String,
            protocol: String,
            params: String,
            group: String,
            bind_ip: String,
            port: Option<u16>,
        }
        let mut fields: Vec<Fields> = Vec::with_capacity(inputs.len());
        for input in &inputs {
            let mut errors = Vec::new();
            let name = validated_name(&input.name, &mut errors);
            let proto_params =
                validate_protocol_params(&input.protocol, &input.params, &mut errors);
            let group = defaulted(&input.group, DEFAULT_GROUP);
            let bind_ip = validated_bind(&input.bind_ip, &mut errors);
            let port = match parse_port_field(&input.port) {
                Ok(port) => Some(port),
                Err(error) => {
                    errors.push(error);
                    None
                }
            };
            if !errors.is_empty() {
                return Err(StoreError::Invalid(errors));
            }
            let (protocol, params) =
                proto_params.unwrap_or((DEFAULT_PROTOCOL.to_string(), "{}".to_string()));
            fields.push(Fields {
                name: name.unwrap_or_default(),
                protocol,
                params,
                group,
                bind_ip: bind_ip.unwrap_or_default(),
                port: port.unwrap_or(None),
            });
        }
        let mut taken: HashSet<i64> = HashSet::new();
        {
            let mut stmt = self.conn.prepare("SELECT port FROM proxy_configs")?;
            let mapped = stmt.query_map([], |row| row.get(0))?;
            for port in mapped {
                taken.insert(port?);
            }
        }
        let mut assigned: Vec<ValidatedConfig> = Vec::with_capacity(fields.len());
        for item in fields {
            let port = match item.port {
                Some(port) => port,
                None => {
                    // Ticket 27: primary then spillover; honest exhaustion
                    // only past both ranges (message unchanged).
                    let mut free = None;
                    for candidate in auto_port_candidates() {
                        if !taken.contains(&(candidate as i64)) {
                            free = Some(candidate);
                            break;
                        }
                    }
                    match free {
                        Some(port) => port,
                        None => {
                            return Err(StoreError::Invalid(vec![FieldError::new(
                                "port",
                                "no automatic port left in the auto range",
                            )]));
                        }
                    }
                }
            };
            taken.insert(port as i64);
            assigned.push(ValidatedConfig {
                name: item.name,
                protocol: item.protocol,
                params: item.params,
                group: item.group,
                bind_ip: item.bind_ip,
                port,
            });
        }
        let tx = self.conn.transaction()?;
        let mut ids: Vec<i64> = Vec::with_capacity(assigned.len());
        for item in &assigned {
            // Ticket 35: seal before the write; a DPAPI failure returns
            // before commit, so the transaction rolls back with nothing
            // written.
            let sealed = Self::seal_or_fail(&item.params)?;
            tx.execute(
                "INSERT INTO proxy_configs
                   (name, protocol, params, grp, enabled, bind_ip, port)
                 VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
                params![
                    item.name,
                    item.protocol,
                    sealed,
                    item.group,
                    item.bind_ip,
                    item.port as i64,
                ],
            )?;
            ids.push(tx.last_insert_rowid());
        }
        tx.commit()?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            out.push(
                self.get(&id.to_string())?
                    .ok_or_else(|| StoreError::NotFound(id.to_string()))?,
            );
        }
        Ok(out)
    }

    /// All configs in stable id order.
    pub fn list(&self) -> Result<Vec<ProxyConfig>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, protocol, params, grp, enabled, bind_ip, port,
              subscription_id, identity_key, preset_id
             FROM proxy_configs ORDER BY id",
        )?;
        let rows = stmt
            .query_map([], map_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Fetch one config by id.
    pub fn get(&self, id: &str) -> Result<Option<ProxyConfig>, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Ok(None),
        };
        let mut stmt = self.conn.prepare(
            "SELECT id, name, protocol, params, grp, enabled, bind_ip, port,
              subscription_id, identity_key, preset_id
             FROM proxy_configs WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![rowid], map_row)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Edit a stopped-config record. Validation runs before any write, so
    /// invalid input leaves the record unchanged.
    /// Ticket 25: editing a subscription-owned row's protocol/params
    /// refreshes its `identity_key` to the widened identity of the new
    /// values (never a stale key that a later refresh would silently
    /// revert), and any edit claims an adoptable (released) row so a later
    /// subscription refresh cannot adopt it away. The last-synced upstream
    /// baseline (`synced_params`) is deliberately left alone: the merge
    /// compares current params against it to tell user-changed from
    /// upstream-changed.
    pub fn update(&mut self, id: &str, patch: ConfigPatch) -> Result<ProxyConfig, StoreError> {
        let current = self
            .get(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))?;
        let merged = self.validate_patch(&current, &patch)?;
        let identity_key = if current.subscription_id.is_some() {
            Some(widened_identity(&merged.protocol, &merged.params))
        } else {
            current.identity_key.clone()
        };
        self.conn.execute(
            "UPDATE proxy_configs
             SET name = ?1, protocol = ?2, params = ?3, grp = ?4, bind_ip = ?5, port = ?6,
                 identity_key = ?7, adoptable = 0
             WHERE id = ?8",
            params![
                merged.name,
                merged.protocol,
                // Ticket 35: sealed at rest.
                Self::seal_or_fail(&merged.params)?,
                merged.group,
                merged.bind_ip,
                merged.port as i64,
                identity_key,
                current.rowid(),
            ],
        )?;
        self.get(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
    }

    /// Remove a config. Returns false when the id does not exist.
    /// Ticket 04: removing a subscription-owned row records a tombstone so
    /// the next refresh does not resurrect a row the user deleted on purpose.
    /// Ticket 12: the row's health snapshot goes with it (bounded table, no
    /// stale resurrection after restart).
    /// Ticket 25: the delete + tombstone + snapshot prune apply in a single
    /// transaction (never a half-removed row).
    pub fn remove(&mut self, id: &str) -> Result<bool, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Ok(false),
        };
        let current = self.get(id)?;
        let tx = self.conn.transaction()?;
        let removed = tx.execute("DELETE FROM proxy_configs WHERE id = ?1", params![rowid])?;
        if removed > 0 {
            if let Some(row) = current {
                let key = row
                    .identity_key
                    .unwrap_or_else(|| widened_identity(&row.protocol, &row.params));
                if let Some(subscription_id) = row.subscription_id {
                    tx.execute(
                        "INSERT OR IGNORE INTO subscription_tombstones (subscription_id, identity_key)
                         VALUES (?1, ?2)",
                        params![subscription_id, key],
                    )?;
                }
            }
            tx.execute(
                "DELETE FROM health_snapshots WHERE config_id = ?1",
                params![id],
            )?;
        }
        tx.commit()?;
        Ok(removed > 0)
    }

    /// Flip the enabled flag.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<ProxyConfig, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Err(StoreError::NotFound(id.to_string())),
        };
        let changed = self.conn.execute(
            "UPDATE proxy_configs SET enabled = ?1 WHERE id = ?2",
            params![enabled as i64, rowid],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(id.to_string()));
        }
        self.get(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
    }

    // -- Ticket 37: routing presets --------------------------------------

    /// Attach (`Some`) or detach (`None`) a routing preset on one row.
    /// Attaching validates the preset exists (`NotFound` otherwise);
    /// detaching always succeeds on a live row. `NotFound` for ghost rows.
    pub fn set_config_preset(
        &mut self,
        config_id: &str,
        preset_id: Option<&str>,
    ) -> Result<ProxyConfig, StoreError> {
        if let Some(preset) = preset_id {
            if self.get_preset(preset)?.is_none() {
                return Err(StoreError::NotFound(preset.to_string()));
            }
        }
        let rowid: i64 = match config_id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Err(StoreError::NotFound(config_id.to_string())),
        };
        let changed = self.conn.execute(
            "UPDATE proxy_configs SET preset_id = ?1 WHERE id = ?2",
            params![preset_id, rowid],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(config_id.to_string()));
        }
        self.get(config_id)?
            .ok_or_else(|| StoreError::NotFound(config_id.to_string()))
    }

    /// Preset rules for the engine merge: `preset_id` → ordered rule
    /// objects. Corrupt stored rules fail loudly (never a silent empty
    /// merge); rows without presets contribute nothing.
    pub fn preset_rules_map(&self) -> Result<HashMap<String, Vec<serde_json::Value>>, StoreError> {
        let mut map = HashMap::new();
        for preset in self.list_presets()? {
            let rules = preset.rules().map_err(|message| {
                StoreError::Sql(rusqlite::Error::ToSqlConversionFailure(Box::new(
                    std::io::Error::new(std::io::ErrorKind::InvalidData, message),
                )))
            })?;
            map.insert(preset.id, rules);
        }
        Ok(map)
    }

    /// Create a preset from validated rules. Names are non-empty and unique;
    /// rules validate field-by-field (rejections name rule index + field).
    pub fn create_preset(
        &mut self,
        name: &str,
        source: &str,
        source_url: Option<&str>,
        rules: &[serde_json::Value],
    ) -> Result<RoutePreset, StoreError> {
        let name = Self::validated_preset_name(name)?;
        Self::check_preset_rules(rules)?;
        let source = Self::validated_preset_source(source);
        let rules_json = serde_json::Value::Array(rules.to_vec()).to_string();
        let now = now_unix();
        match self.conn.execute(
            "INSERT INTO route_presets (name, source, source_url, rules_json, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![name, source, source_url, rules_json, now],
        ) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(error, _))
                if error.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE =>
            {
                return Err(StoreError::Invalid(vec![FieldError::new(
                    "name",
                    format!("preset \"{name}\" already exists"),
                )]));
            }
            Err(error) => return Err(StoreError::Sql(error)),
        }
        let id = self.conn.last_insert_rowid();
        self.get_preset(&id.to_string())?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
    }

    /// Import a `throne://route/...` link as a named preset (source
    /// `link`). Replaces the v1 named error: route links now store.
    pub fn import_route_link(&mut self, line: &str) -> Result<RoutePreset, StoreError> {
        let parsed = crate::preset::decode_route_link(line).map_err(Self::preset_parse_error)?;
        self.create_preset(&parsed.name, "link", None, &parsed.rules)
    }

    /// Import preset JSON (preset URL bodies after an explicit fetch, or
    /// dropped/pasted JSON files) as a named preset.
    pub fn import_preset_json(
        &mut self,
        text: &str,
        source: &str,
        source_url: Option<&str>,
    ) -> Result<RoutePreset, StoreError> {
        let parsed =
            crate::preset::parse_preset_json(text, "").map_err(Self::preset_parse_error)?;
        self.create_preset(&parsed.name, source, source_url, &parsed.rules)
    }

    /// Record a fetched remote preset URL as an empty-rules shell (source
    /// `url`); the explicit fetch fills it via [`Store::set_preset_rules`].
    /// Lets a `throne://remoteroute/...` paste land without any silent fetch.
    pub fn import_remote_preset_shell(
        &mut self,
        name: &str,
        url: &str,
    ) -> Result<RoutePreset, StoreError> {
        self.create_preset(name, "url", Some(url), &[])
    }

    /// Replace a preset's rules wholesale (the explicit-refresh write path).
    /// Validates field-by-field like create.
    pub fn set_preset_rules(
        &mut self,
        id: &str,
        rules: &[serde_json::Value],
    ) -> Result<RoutePreset, StoreError> {
        Self::check_preset_rules(rules)?;
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Err(StoreError::NotFound(id.to_string())),
        };
        let rules_json = serde_json::Value::Array(rules.to_vec()).to_string();
        let changed = self.conn.execute(
            "UPDATE route_presets SET rules_json = ?1, updated_at = ?2 WHERE id = ?3",
            params![rules_json, now_unix(), rowid],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(id.to_string()));
        }
        self.get_preset(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
    }

    /// Append one rule object to a preset (manager add-rule). The rule
    /// validates in place: the rejection names the (new) rule index + field.
    /// Test-covered store seam for the manager add-rule scenario (the v1
    /// manager UI covers list/create/delete/detach; rule edits go through
    /// create-with-JSON there).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn add_preset_rule(
        &mut self,
        id: &str,
        rule: serde_json::Value,
    ) -> Result<RoutePreset, StoreError> {
        let mut preset = self
            .get_preset(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))?;
        let mut rules = preset.rules().map_err(|message| {
            StoreError::Sql(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::new(std::io::ErrorKind::InvalidData, message),
            )))
        })?;
        rules.push(rule);
        if let Err(error) = crate::preset::validate_preset_rules(&rules) {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "rule",
                error.message.clone(),
            )]));
        }
        self.set_preset_rules(id, &rules)?;
        preset = self
            .get_preset(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))?;
        Ok(preset)
    }

    /// Remove one rule by index (manager remove-rule). Out-of-range indices
    /// fail typed; nothing is written. Test-covered store seam (see
    /// [`Store::add_preset_rule`]).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn remove_preset_rule(
        &mut self,
        id: &str,
        index: usize,
    ) -> Result<RoutePreset, StoreError> {
        let preset = self
            .get_preset(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))?;
        let mut rules = preset.rules().map_err(|message| {
            StoreError::Sql(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::new(std::io::ErrorKind::InvalidData, message),
            )))
        })?;
        if index >= rules.len() {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "rule",
                format!(
                    "rule {index} does not exist (preset has {} rule(s))",
                    rules.len()
                ),
            )]));
        }
        rules.remove(index);
        self.set_preset_rules(id, &rules)
    }

    /// All presets in stable id order.
    pub fn list_presets(&self) -> Result<Vec<RoutePreset>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, source, source_url, rules_json, updated_at
             FROM route_presets ORDER BY id",
        )?;
        let rows = stmt
            .query_map([], map_preset_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Fetch one preset by id.
    pub fn get_preset(&self, id: &str) -> Result<Option<RoutePreset>, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Ok(None),
        };
        let mut stmt = self.conn.prepare(
            "SELECT id, name, source, source_url, rules_json, updated_at
             FROM route_presets WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![rowid], map_preset_row)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Delete a preset. Refused (`Invalid`) while any row attaches it — the
    /// refusal names the attached rows, so the UI can offer the explicit
    /// detach-then-delete path ([`Store::delete_preset_detaching`]).
    pub fn delete_preset(&mut self, id: &str) -> Result<bool, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Ok(false),
        };
        if self.get_preset(id)?.is_none() {
            return Ok(false);
        }
        let attached = self.preset_attachments(id)?;
        if !attached.is_empty() {
            let names: Vec<String> = attached
                .iter()
                .map(|row| format!("\"{}\" (id {})", row.name, row.id))
                .collect();
            return Err(StoreError::Invalid(vec![FieldError::new(
                "preset",
                format!(
                    "preset is still attached to {} row(s): {} — detach first or delete explicitly with detach",
                    attached.len(),
                    names.join(", ")
                ),
            )]));
        }
        let removed = self
            .conn
            .execute("DELETE FROM route_presets WHERE id = ?1", params![rowid])?;
        Ok(removed > 0)
    }

    /// Delete a preset, detaching every attached row first (the explicit
    /// path: one transaction, never a half-detached delete).
    pub fn delete_preset_detaching(&mut self, id: &str) -> Result<bool, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Ok(false),
        };
        if self.get_preset(id)?.is_none() {
            return Ok(false);
        }
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE proxy_configs SET preset_id = NULL WHERE preset_id = ?1",
            params![id],
        )?;
        let removed = tx.execute("DELETE FROM route_presets WHERE id = ?1", params![rowid])?;
        tx.commit()?;
        Ok(removed > 0)
    }

    /// Rows currently attaching a preset (stable id order).
    pub fn preset_attachments(&self, id: &str) -> Result<Vec<ProxyConfig>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, protocol, params, grp, enabled, bind_ip, port,
              subscription_id, identity_key, preset_id
             FROM proxy_configs WHERE preset_id = ?1 ORDER BY id",
        )?;
        let rows = stmt
            .query_map(params![id], map_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    fn validated_preset_name(raw: &str) -> Result<String, StoreError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "name",
                "preset name must not be empty",
            )]));
        }
        let capped: String = if trimmed.chars().count() > crate::preset::MAX_PRESET_NAME_CHARS {
            trimmed
                .chars()
                .take(crate::preset::MAX_PRESET_NAME_CHARS)
                .collect()
        } else {
            trimmed.to_string()
        };
        Ok(capped)
    }

    fn validated_preset_source(raw: &str) -> String {
        match raw.trim() {
            "link" | "url" | "file" | "manual" => raw.trim().to_string(),
            _ => "manual".to_string(),
        }
    }

    /// Field-level rule validation behind create/set/add. Rejections name
    /// rule index + field (never values).
    fn check_preset_rules(rules: &[serde_json::Value]) -> Result<(), StoreError> {
        if let Err(error) = crate::preset::validate_preset_rules(rules) {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "rules",
                error.message.clone(),
            )]));
        }
        Ok(())
    }

    /// Map a preset parse failure into [`StoreError::Invalid`] (field
    /// `preset`; messages never echo payloads by construction of the parser).
    fn preset_parse_error(error: crate::preset::PresetError) -> StoreError {
        StoreError::Invalid(vec![FieldError::new("preset", error.to_string())])
    }

    // -- Ticket 04: subscriptions -----------------------------------------

    fn validated_subscription_label(raw: &str) -> Result<String, StoreError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "label",
                "label must not be empty",
            )]));
        }
        Ok(trimmed.to_string())
    }

    fn validated_subscription_url(raw: &str) -> Result<String, StoreError> {
        // Error text never echoes the URL: subscription URLs often carry
        // tokens in the query string (boundary: no secrets in logs).
        // Ticket 25: byte-safe scheme prefix (never slice at a non-char
        // boundary on non-ASCII input — the import-side half is ticket 24).
        let trimmed = raw.trim();
        let after_scheme = if trimmed
            .get(..7)
            .is_some_and(|p| p.eq_ignore_ascii_case("http://"))
        {
            &trimmed[7..]
        } else if trimmed
            .get(..8)
            .is_some_and(|p| p.eq_ignore_ascii_case("https://"))
        {
            &trimmed[8..]
        } else {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "url",
                "subscription url must use http(s)",
            )]));
        };
        let host = after_scheme.split(['/', '?']).next().unwrap_or("");
        if host.trim().is_empty() || trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "url",
                "subscription url must use http(s)",
            )]));
        }
        // Ticket 27: internal hosts never fetch (SSRF guard). Plain http
        // stays allowed here — the Warn surfaces at fetch/policy time, not
        // as an add-time reject (LAN providers use http). Reasons never
        // echo the URL.
        if matches!(
            subscription_fetch_policy(trimmed),
            SubscriptionFetchPolicy::Block { .. }
        ) {
            // Distinguish scheme failures (above) from the internal-host
            // block: only hosts that parsed as http(s) reach here.
            return Err(StoreError::Invalid(vec![FieldError::new(
                "url",
                "subscription url must not target internal hosts",
            )]));
        }
        Ok(trimmed.to_string())
    }

    /// Add a subscription record. The URL must be `http(s)`; blank groups
    /// fall back to the default. Members appear on the next refresh, never
    /// enabled or started.
    pub fn add_subscription(
        &mut self,
        label: &str,
        url: &str,
        group: &str,
        auto_refresh: bool,
    ) -> Result<Subscription, StoreError> {
        let label = Self::validated_subscription_label(label)?;
        let url = Self::validated_subscription_url(url)?;
        let group = defaulted(group, DEFAULT_GROUP);
        self.conn.execute(
            "INSERT INTO subscriptions (label, url, grp, auto_refresh)
             VALUES (?1, ?2, ?3, ?4)",
            params![label, url, group, auto_refresh as i64],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get_subscription(&id.to_string())?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
    }

    /// Ticket 31: edit a subscription's label/url/group in place. Blank
    /// groups fall back to the default (like add); the auto-refresh flag is
    /// untouched. Invalid input fails typed with nothing written (the same
    /// validators as add, so errors never echo the URL); unknown ids are
    /// `NotFound`.
    pub fn update_subscription(
        &mut self,
        id: &str,
        label: &str,
        url: &str,
        group: &str,
    ) -> Result<Subscription, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Err(StoreError::NotFound(id.to_string())),
        };
        if self.get_subscription(id)?.is_none() {
            return Err(StoreError::NotFound(id.to_string()));
        }
        let label = Self::validated_subscription_label(label)?;
        let url = Self::validated_subscription_url(url)?;
        let group = defaulted(group, DEFAULT_GROUP);
        self.conn.execute(
            "UPDATE subscriptions SET label = ?1, url = ?2, grp = ?3 WHERE id = ?4",
            params![label, url, group, rowid],
        )?;
        self.get_subscription(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
    }

    /// Ticket 31: find a subscription by exact (trimmed) URL — the friendly
    /// `already subscribed` path. A missing row is `None`, never an error.
    pub fn find_subscription_by_url(&self, url: &str) -> Result<Option<Subscription>, StoreError> {
        let trimmed = url.trim();
        let mut stmt = self.conn.prepare(
            "SELECT id, label, url, grp, auto_refresh, last_refresh_at, last_error, failures
             FROM subscriptions WHERE url = ?1",
        )?;
        let mut rows = stmt.query_map(params![trimmed], map_subscription_row)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// All subscriptions in stable id order.
    pub fn list_subscriptions(&self) -> Result<Vec<Subscription>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, label, url, grp, auto_refresh, last_refresh_at, last_error, failures
             FROM subscriptions ORDER BY id",
        )?;
        let rows = stmt
            .query_map([], map_subscription_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Fetch one subscription by id.
    pub fn get_subscription(&self, id: &str) -> Result<Option<Subscription>, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Ok(None),
        };
        let mut stmt = self.conn.prepare(
            "SELECT id, label, url, grp, auto_refresh, last_refresh_at, last_error, failures
             FROM subscriptions WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![rowid], map_subscription_row)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Flip a subscription's auto-refresh flag.
    pub fn set_subscription_auto_refresh(
        &mut self,
        id: &str,
        auto_refresh: bool,
    ) -> Result<Subscription, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Err(StoreError::NotFound(id.to_string())),
        };
        let changed = self.conn.execute(
            "UPDATE subscriptions SET auto_refresh = ?1 WHERE id = ?2",
            params![auto_refresh as i64, rowid],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(id.to_string()));
        }
        self.get_subscription(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
    }

    /// Remove a subscription. Owned member rows are released (kept, unowned),
    /// never deleted; tombstones for this subscription are dropped.
    /// Ticket 25: the release + tombstone drop + subscription delete apply
    /// in a single transaction. Released rows keep their params but lose
    /// their identity, baseline, and ownership — and are marked adoptable so
    /// a second subscription carrying the same node adopts them in place
    /// instead of double-counting a cross-subscription shadow.
    pub fn remove_subscription(&mut self, id: &str) -> Result<bool, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Ok(false),
        };
        if self.get_subscription(id)?.is_none() {
            return Ok(false);
        }
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE proxy_configs
             SET subscription_id = NULL, identity_key = NULL,
                 synced_params = NULL, adoptable = 1
             WHERE subscription_id = ?1",
            params![id],
        )?;
        tx.execute(
            "DELETE FROM subscription_tombstones WHERE subscription_id = ?1",
            params![id],
        )?;
        let removed = tx.execute("DELETE FROM subscriptions WHERE id = ?1", params![rowid])?;
        tx.commit()?;
        Ok(removed > 0)
    }

    /// Record a fetch-level failure (e.g. network error from the intake
    /// path): prior members are kept, only `last_error` changes.
    pub fn record_subscription_error(
        &mut self,
        id: &str,
        error: &str,
    ) -> Result<Subscription, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Err(StoreError::NotFound(id.to_string())),
        };
        let changed = self.conn.execute(
            "UPDATE subscriptions SET last_error = ?1 WHERE id = ?2",
            params![error, rowid],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(id.to_string()));
        }
        self.get_subscription(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
    }

    /// Refresh a subscription from an already-fetched body (the refresh-merge
    /// seam: tests inject body strings, never real network). The body is
    /// parsed with the ticket-03 parsers via
    /// [`crate::import::import_subscription_body`]; the merge upserts owned
    /// rows keyed by [`widened_identity`], preserves the hand-edit overlay
    /// (name/enabled/bind/port), pins user-edited params/protocol against
    /// silent upstream reverts (versioned against `synced_params`, conflicts
    /// counted), rotates identity in place (id/port/enabled kept), adopts
    /// released rows, tombstones user-deleted rows, removes only
    /// subscription-owned vanished rows (pruning their health snapshots),
    /// and applies atomically per group. Unparseable bodies keep the
    /// last-good members and record `last_error`. Refresh never enables or
    /// starts configs.
    pub fn refresh_with_body(
        &mut self,
        sub_id: &str,
        body: &str,
        now: i64,
    ) -> Result<RefreshOutcome, StoreError> {
        use crate::import::{import_subscription_body, ImportedConfig};
        let subscription = self
            .get_subscription(sub_id)?
            .ok_or_else(|| StoreError::NotFound(sub_id.to_string()))?;
        let batch = import_subscription_body(body);
        if batch.configs.is_empty() && (!batch.failures.is_empty() || body.trim().is_empty()) {
            let reason = batch
                .failures
                .first()
                .map(|failure| failure.reason.clone())
                .unwrap_or_else(|| "empty subscription body".to_string());
            self.record_subscription_error(sub_id, &reason)?;
            self.save_subscription_failures(sub_id, &batch.failures)?;
            return Ok(RefreshOutcome {
                failures: batch.failures,
                error: Some(reason),
                ..Default::default()
            });
        }
        // Desired set, FIRST-wins (ticket 25 L1: the code matches the
        // comment); extras count as duplicates. A BTreeMap keeps the
        // allocation order deterministic (ticket 25 L2). Keys are the
        // widened identity (ticket 25: no transport/security/flow over-merge).
        // Values carry the batch position so port-exhaustion failures report
        // real per-item indices (ticket 25 L3).
        let mut desired: BTreeMap<String, (usize, ImportedConfig)> = BTreeMap::new();
        let mut duplicates = 0usize;
        for (index, config) in batch.configs.into_iter().enumerate() {
            let key = widened_identity(&config.protocol, &config.params);
            match desired.entry(key) {
                std::collections::btree_map::Entry::Occupied(_) => {
                    duplicates += 1;
                }
                std::collections::btree_map::Entry::Vacant(slot) => {
                    slot.insert((index, config));
                }
            }
        }
        let rows = self.list()?;
        let mut tombstones: HashSet<String> = HashSet::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT identity_key FROM subscription_tombstones WHERE subscription_id = ?1",
            )?;
            let keys = stmt.query_map(params![subscription.id], |row| row.get::<_, String>(0))?;
            for key in keys {
                tombstones.insert(key?);
            }
        }
        // Ticket 25: ownership side-channels live in side queries, never in
        // `ProxyConfig` (its shape is frozen: main.rs builds it literally).
        // `adoptable` marks rows released by `remove_subscription` so a
        // second subscription adopts them; `synced_params` is the last-synced
        // upstream baseline behind the user-vs-upstream compare.
        let mut adoptable: HashSet<String> = HashSet::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT id FROM proxy_configs WHERE adoptable = 1")?;
            let ids = stmt.query_map([], |row| row.get::<_, i64>(0))?;
            for id in ids {
                adoptable.insert(id?.to_string());
            }
        }
        let mut synced: HashMap<String, Option<String>> = HashMap::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT id, synced_params FROM proxy_configs WHERE subscription_id = ?1",
            )?;
            let pairs = stmt.query_map(params![subscription.id], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
            })?;
            for pair in pairs {
                let (id, baseline) = pair?;
                // Ticket 35: the baseline column is sealed at rest; open it
                // once here so the versioned compare below stays plaintext.
                let baseline = baseline
                    .map(|sealed| Self::unseal_or_fail(&sealed))
                    .transpose()?;
                synced.insert(id.to_string(), baseline);
            }
        }
        struct RowView<'a> {
            row: &'a ProxyConfig,
            stored: Option<String>,
            fresh: String,
            owned: bool,
        }
        let views: Vec<RowView<'_>> = rows
            .iter()
            .map(|row| RowView {
                stored: row.identity_key.clone(),
                fresh: widened_identity(&row.protocol, &row.params),
                owned: row.subscription_id.as_deref() == Some(subscription.id.as_str()),
                row,
            })
            .collect();
        // Primary match key: the stored (last-synced or user-refreshed)
        // identity, falling back to the fresh key for legacy rows.
        let mut owned_by_key: HashMap<String, &RowView<'_>> = HashMap::new();
        for view in &views {
            if view.owned {
                owned_by_key
                    .entry(view.stored.clone().unwrap_or_else(|| view.fresh.clone()))
                    .or_insert(view);
            }
        }
        let mut global_by_key: HashMap<String, &RowView<'_>> = HashMap::new();
        for view in &views {
            global_by_key.entry(view.fresh.clone()).or_insert(view);
        }
        // Ticket 25: narrow fallback behind the widened shadow. Hand-made
        // rows (dialog JSON, legacy seeds) omit the transport/security
        // defaults the link parsers emit, so the same node serializes to
        // different widened keys. A narrow hit on an UNOWNED row still
        // dedupes exactly like before (adopt when released, duplicate when
        // hand-made); owned rows never shadow (vanished/rotation resolves
        // them) so widened precision survives where it matters.
        let mut narrow_by_key: HashMap<String, &RowView<'_>> = HashMap::new();
        for view in &views {
            narrow_by_key
                .entry(crate::import::normalized_identity(
                    &view.row.protocol,
                    &view.row.params,
                ))
                .or_insert(view);
        }
        enum Op {
            Insert {
                key: String,
                config: ImportedConfig,
                port: u16,
            },
            UpstreamUpdate {
                id: String,
                protocol: String,
                params: String,
                key: String,
            },
            GroupFollow {
                id: String,
            },
            Adopt {
                id: String,
                key: String,
                params: String,
            },
            Delete {
                id: String,
            },
        }
        let mut ops: Vec<Op> = Vec::new();
        let mut outcome = RefreshOutcome {
            duplicates,
            failures: batch.failures,
            ..Default::default()
        };
        // Rotation cores of tombstoned desired items: a vanished owned row
        // sharing one is user-owned now (the user edited identity away from
        // the upstream key) and is pinned, never deleted, never reverted.
        let mut tomb_cores: HashSet<String> = HashSet::new();
        for (key, (_, config)) in &desired {
            if tombstones.contains(key) {
                if let Some(core) = rotation_core(&config.params) {
                    tomb_cores.insert(core);
                }
            }
        }
        let mut matched_owned: HashSet<String> = HashSet::new();
        let mut candidates: Vec<(String, usize, ImportedConfig)> = Vec::new();
        for (key, (index, config)) in &desired {
            if tombstones.contains(key) {
                outcome.skipped += 1;
                continue;
            }
            if let Some(view) = owned_by_key.get(key) {
                matched_owned.insert(view.row.id.clone());
                // Ticket 25 versioned compare: the baseline is the
                // last-synced upstream params. User-changed rows keep their
                // params/protocol (pinned); upstream-only changes apply in
                // place; both changing surfaces an explicit conflict.
                match synced.get(&view.row.id).and_then(|base| base.clone()) {
                    None => {
                        // Legacy row (pre-ticket-25, no baseline): upstream
                        // wins, exactly the old behavior.
                        if view.row.protocol != config.protocol
                            || view.row.params != config.params
                            || view.row.group != subscription.group
                        {
                            ops.push(Op::UpstreamUpdate {
                                id: view.row.id.clone(),
                                protocol: config.protocol.clone(),
                                params: config.params.clone(),
                                key: key.clone(),
                            });
                            outcome.updated += 1;
                        }
                    }
                    Some(base) => {
                        let upstream_changed = config.params != base;
                        let user_changed = view.row.params != base;
                        let follow_group = view.row.group != subscription.group;
                        if view.row.protocol != config.protocol
                            || (upstream_changed && !user_changed)
                        {
                            ops.push(Op::UpstreamUpdate {
                                id: view.row.id.clone(),
                                protocol: config.protocol.clone(),
                                params: config.params.clone(),
                                key: key.clone(),
                            });
                            outcome.updated += 1;
                        } else {
                            // Pinned: user fix kept (params/protocol never
                            // overwritten); only the group still follows.
                            if follow_group {
                                ops.push(Op::GroupFollow {
                                    id: view.row.id.clone(),
                                });
                                outcome.updated += 1;
                            }
                            if upstream_changed && user_changed {
                                outcome.conflicts += 1;
                            }
                        }
                    }
                }
                continue;
            }
            if let Some(view) = global_by_key.get(key) {
                // Ticket 25 cross-subscription fix: an unowned ADOPTABLE
                // (released) row with the same node is adopted in place;
                // hand-made collisions stay duplicates, never duplicated.
                // Owned rows never shadow here (a stale-key twin falls
                // through to candidate duty; vanished/rotation resolves it).
                if view.owned {
                    candidates.push((key.clone(), *index, config.clone()));
                } else if adoptable.contains(&view.row.id) {
                    ops.push(Op::Adopt {
                        id: view.row.id.clone(),
                        key: key.clone(),
                        params: config.params.clone(),
                    });
                    outcome.added += 1;
                } else {
                    outcome.duplicates += 1;
                }
                continue;
            }
            if let Some(view) = narrow_by_key.get(&crate::import::normalized_identity(
                &config.protocol,
                &config.params,
            )) {
                if view.owned {
                    candidates.push((key.clone(), *index, config.clone()));
                } else if adoptable.contains(&view.row.id) {
                    // Released row, upstream tweaked an extra since release:
                    // adopt in place onto the new key + params.
                    ops.push(Op::Adopt {
                        id: view.row.id.clone(),
                        key: key.clone(),
                        params: config.params.clone(),
                    });
                    outcome.added += 1;
                } else {
                    outcome.duplicates += 1;
                }
                continue;
            }
            candidates.push((key.clone(), *index, config.clone()));
        }
        // Vanished owned rows, paired deterministically by numeric id.
        let mut vanished: Vec<&RowView<'_>> = views
            .iter()
            .filter(|view| view.owned && !matched_owned.contains(&view.row.id))
            .collect();
        vanished.sort_by_key(|view| view.row.id.parse::<i64>().unwrap_or(i64::MAX));
        let mut consumed: HashSet<String> = HashSet::new();
        let mut deleted_ids: Vec<String> = Vec::new();
        for view in vanished {
            let user_changed = synced
                .get(&view.row.id)
                .and_then(|base| base.as_ref())
                .map(|base| view.row.params != *base)
                .unwrap_or(false);
            let core = rotation_core(&view.row.params);
            if let Some(core) = core.as_deref() {
                if tomb_cores.contains(core) {
                    continue;
                }
            }
            let mut paired = false;
            if let Some(core) = core.as_deref() {
                for (candidate_key, _, candidate) in &candidates {
                    if consumed.contains(candidate_key) {
                        continue;
                    }
                    if rotation_core(&candidate.params).as_deref() == Some(core) {
                        consumed.insert(candidate_key.clone());
                        if user_changed {
                            // Upstream rotated identity under a user edit:
                            // keep the user's version, swallow the twin.
                            outcome.conflicts += 1;
                        } else {
                            // Ticket 25 rotation in place: same id, same
                            // port, same enabled — never delete + re-add.
                            ops.push(Op::UpstreamUpdate {
                                id: view.row.id.clone(),
                                protocol: candidate.protocol.clone(),
                                params: candidate.params.clone(),
                                key: candidate_key.clone(),
                            });
                            outcome.updated += 1;
                        }
                        paired = true;
                        break;
                    }
                }
            }
            if !paired {
                ops.push(Op::Delete {
                    id: view.row.id.clone(),
                });
                deleted_ids.push(view.row.id.clone());
                outcome.removed += 1;
            }
        }
        // Ticket 25 L2: same-pass deletions free their ports first; BTreeMap
        // order makes multi-insert allocation deterministic.
        let deleted_set: HashSet<&str> = deleted_ids.iter().map(String::as_str).collect();
        let mut taken: HashSet<i64> = rows
            .iter()
            .filter(|row| !deleted_set.contains(row.id.as_str()))
            .map(|row| row.port as i64)
            .collect();
        let mut next_free_port = || -> Option<u16> {
            for port in auto_port_candidates() {
                if !taken.contains(&(port as i64)) {
                    taken.insert(port as i64);
                    return Some(port);
                }
            }
            None
        };
        for (key, index, config) in &candidates {
            if consumed.contains(key) {
                continue;
            }
            match next_free_port() {
                Some(port) => {
                    ops.push(Op::Insert {
                        key: key.clone(),
                        config: config.clone(),
                        port,
                    });
                    outcome.added += 1;
                }
                None => {
                    outcome.failures.push(crate::import::ImportFailure {
                        index: *index,
                        label: crate::import::redact_link(&config.name),
                        reason: "port-exhausted: no automatic port left in the auto range"
                            .to_string(),
                        code: crate::import::ImportErrorKind::BadPort,
                    });
                }
            }
        }
        let tx = self.conn.transaction()?;
        for op in ops {
            match op {
                Op::Insert { key, config, port } => {
                    // Ticket 35: params and the sync baseline seal together.
                    let sealed = Self::seal_or_fail(&config.params)?;
                    tx.execute(
                        "INSERT INTO proxy_configs
                           (name, protocol, params, grp, enabled, bind_ip, port,
                            subscription_id, identity_key, synced_params, adoptable)
                         VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?7, ?8, ?9, 0)",
                        params![
                            config.name,
                            config.protocol,
                            sealed,
                            subscription.group,
                            DEFAULT_BIND,
                            port as i64,
                            subscription.id,
                            key,
                            sealed,
                        ],
                    )?;
                }
                Op::UpstreamUpdate {
                    id,
                    protocol,
                    params,
                    key,
                } => {
                    let rowid: i64 = id.parse().map_err(|_| {
                        StoreError::Invalid(vec![FieldError::new("id", "bad config id")])
                    })?;
                    // Ticket 25: refreshes the identity key and the upstream
                    // baseline together; id/port/enabled/name/bind survive.
                    // Ticket 35: sealed at rest.
                    let sealed = Self::seal_or_fail(&params)?;
                    tx.execute(
                        "UPDATE proxy_configs
                         SET protocol = ?1, params = ?2, grp = ?3,
                             identity_key = ?4, synced_params = ?5
                         WHERE id = ?6",
                        params![protocol, sealed, subscription.group, key, sealed, rowid],
                    )?;
                }
                Op::GroupFollow { id } => {
                    let rowid: i64 = id.parse().map_err(|_| {
                        StoreError::Invalid(vec![FieldError::new("id", "bad config id")])
                    })?;
                    tx.execute(
                        "UPDATE proxy_configs SET grp = ?1 WHERE id = ?2",
                        params![subscription.group, rowid],
                    )?;
                }
                Op::Adopt { id, key, params } => {
                    let rowid: i64 = id.parse().map_err(|_| {
                        StoreError::Invalid(vec![FieldError::new("id", "bad config id")])
                    })?;
                    // Ticket 25 adoption: a released row joins the
                    // subscription in place (id/port/enabled/name kept); the
                    // row was untouched since release, so upstream params win.
                    // Ticket 35: sealed at rest.
                    let sealed = Self::seal_or_fail(&params)?;
                    tx.execute(
                        "UPDATE proxy_configs
                         SET subscription_id = ?1, identity_key = ?2,
                             synced_params = ?3, params = ?4, grp = ?5,
                             adoptable = 0
                         WHERE id = ?6",
                        params![
                            subscription.id,
                            key,
                            sealed,
                            sealed,
                            subscription.group,
                            rowid
                        ],
                    )?;
                }
                Op::Delete { id } => {
                    let rowid: i64 = id.parse().map_err(|_| {
                        StoreError::Invalid(vec![FieldError::new("id", "bad config id")])
                    })?;
                    tx.execute("DELETE FROM proxy_configs WHERE id = ?1", params![rowid])?;
                    // Ticket 25: refresh-deletes prune like remove() does.
                    tx.execute(
                        "DELETE FROM health_snapshots WHERE config_id = ?1",
                        params![id],
                    )?;
                }
            }
        }
        {
            let rowid: i64 = subscription.id.parse().map_err(|_| {
                StoreError::Invalid(vec![FieldError::new("id", "bad subscription id")])
            })?;
            tx.execute(
                "UPDATE subscriptions SET last_refresh_at = ?1, last_error = NULL, failures = ?2
                 WHERE id = ?3",
                params![now, encode_failures(&outcome.failures), rowid],
            )?;
        }
        tx.commit()?;
        Ok(outcome)
    }

    /// Ticket 17: persist the latest per-item failures for one subscription
    /// (the per-subscription failures view reads them back via
    /// [`Store::get_subscription`]). A fetch-level failure parses nothing new,
    /// so [`Store::record_subscription_error`] intentionally leaves this
    /// column untouched.
    pub fn save_subscription_failures(
        &mut self,
        id: &str,
        failures: &[crate::import::ImportFailure],
    ) -> Result<(), StoreError> {
        let rowid: i64 = id
            .parse()
            .map_err(|_| StoreError::Invalid(vec![FieldError::new("id", "bad subscription id")]))?;
        self.conn.execute(
            "UPDATE subscriptions SET failures = ?1 WHERE id = ?2",
            params![encode_failures(failures), rowid],
        )?;
        Ok(())
    }

    // -- Ticket 11: user-owned JSON export ----------------------------------

    /// Default export destination: `MANYPROXY_EXPORT` overrides; otherwise a
    /// `manyproxy-export.json` sibling of the library file. The dialog writes
    /// here only after explicit confirm; cancel writes nothing.
    pub fn default_export_path() -> PathBuf {
        if let Ok(path) = std::env::var("MANYPROXY_EXPORT") {
            if !path.trim().is_empty() {
                return PathBuf::from(path);
            }
        }
        let sibling = Self::default_path();
        if let Some(parent) = sibling.parent() {
            if !parent.as_os_str().is_empty() {
                return parent.join("manyproxy-export.json");
            }
        }
        PathBuf::from("manyproxy-export.json")
    }

    /// Serialize the library (configs + subscriptions, no health samples)
    /// for the user-owned export path. The payload round-trips through the
    /// ticket-03 import grammar ([`crate::import::import_text`]): `outbounds`
    /// carries each import-shaped row's params verbatim so an identical
    /// re-import commits 0 added / N duplicates; `library`/`subscriptions`
    /// metadata rides along for backup and is ignored by the import path.
    /// Secrets stay plaintext by design (the confirm dialog shows
    /// [`export_warning_text`] verbatim): redacting here would break
    /// re-import, so redaction never applies to this payload.
    /// Ticket 25: the `library`/`subscriptions`/`tombstones` sections are
    /// full-fidelity (params, enabled, identity, sync baseline, adoptable
    /// flag, refresh metadata) so [`Store::restore_library_json`] rebuilds
    /// identical rows/groups/enabled with deletes staying deleted.
    pub fn export_library_json(&self) -> Result<String, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT subscription_id, identity_key FROM subscription_tombstones
             ORDER BY subscription_id, identity_key",
        )?;
        let tombstones = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let payload = export_payload_full(&self.list()?, &self.list_subscriptions()?, &tombstones);
        // Ticket 25: ownership side-channels (`adoptable`, `synced_params`)
        // live outside `ProxyConfig`; patch them into the library section
        // here so restores preserve them.
        let mut side: HashMap<String, (bool, Option<String>)> = HashMap::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT id, adoptable, synced_params FROM proxy_configs")?;
            let entries = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })?;
            for entry in entries {
                let (id, adoptable, synced) = entry?;
                // Ticket 35: the export carries plaintext (decrypt-then-write
                // behind the verbatim warning); unseal the baseline here.
                let synced = synced
                    .map(|sealed| Self::unseal_or_fail(&sealed))
                    .transpose()?;
                side.insert(id.to_string(), (adoptable != 0, synced));
            }
        }
        let mut value: serde_json::Value = serde_json::from_str(&payload)
            .map_err(|error| Self::schema_error(format!("export serialization broke ({error})")))?;
        if let Some(entries) = value.get_mut("library").and_then(|v| v.as_array_mut()) {
            for entry in entries {
                if let Some(id) = entry.get("id").and_then(|v| v.as_str()) {
                    let (adoptable, synced) = side.get(id).cloned().unwrap_or((false, None));
                    entry["adoptable"] = serde_json::Value::Bool(adoptable);
                    entry["synced_params"] = match synced {
                        Some(text) => serde_json::Value::String(text),
                        None => serde_json::Value::Null,
                    };
                }
            }
        }
        Ok(value.to_string())
    }

    /// Write the export file only when `confirmed` is true. Cancel (`false`)
    /// performs no I/O and returns `Ok(None)`; confirm writes the payload
    /// and returns the path. Errors never echo file contents (no secrets in
    /// logs); only the destination path is named.
    /// Ticket 25 (L4): the payload lands via tmp-file + rename in the same
    /// directory, so a crash never leaves a half-written export behind.
    /// Ticket 32: the payload holds secrets at rest, so both the tmp
    /// sibling and the final file land user-only hardened
    /// ([`harden_file_permissions`], like the library DB).
    pub fn commit_export(
        &self,
        path: &Path,
        confirmed: bool,
    ) -> Result<Option<PathBuf>, StoreError> {
        if !confirmed {
            return Ok(None);
        }
        let payload = self.export_library_json()?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|error| StoreError::Io {
                    path: path.to_path_buf(),
                    message: error.to_string(),
                })?;
            }
        }
        let tmp = tmp_export_path(path);
        let io_error = |error: std::io::Error| StoreError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        };
        if let Err(error) = std::fs::write(&tmp, payload).map_err(io_error) {
            let _ = std::fs::remove_file(&tmp);
            return Err(error);
        }
        harden_file_permissions(&tmp);
        if let Err(error) = std::fs::rename(&tmp, path).map_err(io_error) {
            let _ = std::fs::remove_file(&tmp);
            return Err(error);
        }
        harden_file_permissions(path);
        Ok(Some(path.to_path_buf()))
    }

    /// Ticket 25: rebuild a [`Store::export_library_json`] payload verbatim —
    /// same row ids, same groups, same enabled flags, same subscription
    /// records with refresh metadata, same tombstones (deletes stay
    /// deleted). Rows land exactly as exported (no re-validation, no port
    /// reassignment); anything malformed fails loudly with an `export`
    /// field error before anything is written (single transaction). The
    /// import grammar keeps reading only `outbounds` (ticket 24 territory);
    /// this is the store half that consumes the `library`/`subscriptions`/
    /// `tombstones` sections ticket 25 exposes. No in-tree prod caller yet
    /// (UI/import wiring is ticket 26); kept live by tests.
    #[allow(dead_code)]
    pub fn restore_library_json(&mut self, payload: &str) -> Result<RestoreOutcome, StoreError> {
        fn invalid(message: String) -> StoreError {
            StoreError::Invalid(vec![FieldError::new("export", message)])
        }
        let value: serde_json::Value = serde_json::from_str(payload)
            .map_err(|error| invalid(format!("not JSON ({error})")))?;
        let obj = value
            .as_object()
            .ok_or_else(|| invalid("top level must be an object".to_string()))?;
        let library = obj
            .get("library")
            .and_then(|v| v.as_array())
            .ok_or_else(|| invalid("missing library section".to_string()))?;
        let subs = obj
            .get("subscriptions")
            .and_then(|v| v.as_array())
            .ok_or_else(|| invalid("missing subscriptions section".to_string()))?;
        let empty_section: Vec<serde_json::Value> = Vec::new();
        let tombs = obj
            .get("tombstones")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty_section);
        let get_str = |entry: &serde_json::Value, key: &str| -> Option<String> {
            entry.get(key).and_then(|v| v.as_str()).map(str::to_string)
        };
        struct LibraryRow {
            id: i64,
            name: String,
            protocol: String,
            params: String,
            group: String,
            enabled: bool,
            bind_ip: String,
            port: i64,
            subscription_id: Option<String>,
            identity_key: Option<String>,
            synced_params: Option<String>,
            adoptable: bool,
        }
        let mut rows: Vec<LibraryRow> = Vec::with_capacity(library.len());
        for (index, entry) in library.iter().enumerate() {
            let what = format!("library[{index}]");
            let id: i64 = entry
                .get("id")
                .and_then(|v| v.as_str().and_then(|s| s.parse().ok()))
                .ok_or_else(|| invalid(format!("{what} needs a string id")))?;
            if id <= 0 {
                return Err(invalid(format!("{what} has a bad id")));
            }
            let port: i64 = entry
                .get("port")
                .and_then(serde_json::Value::as_i64)
                .filter(|port| (1..=65_535).contains(port))
                .ok_or_else(|| invalid(format!("{what} needs port 1-65535")))?;
            rows.push(LibraryRow {
                id,
                name: get_str(entry, "name").unwrap_or_default(),
                protocol: get_str(entry, "protocol").unwrap_or_else(|| "vless".to_string()),
                params: get_str(entry, "params").unwrap_or_else(|| "{}".to_string()),
                group: get_str(entry, "group").unwrap_or_else(|| "default".to_string()),
                enabled: entry
                    .get("enabled")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                bind_ip: get_str(entry, "bind_ip").unwrap_or_else(|| "127.0.0.1".to_string()),
                port,
                subscription_id: get_str(entry, "subscription_id"),
                identity_key: get_str(entry, "identity_key"),
                synced_params: get_str(entry, "synced_params"),
                adoptable: entry
                    .get("adoptable")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
            });
        }
        struct SubRow {
            id: i64,
            label: String,
            url: String,
            group: String,
            auto_refresh: bool,
            last_refresh_at: Option<i64>,
            last_error: Option<String>,
            failures: String,
        }
        let mut sub_rows: Vec<SubRow> = Vec::with_capacity(subs.len());
        for (index, entry) in subs.iter().enumerate() {
            let what = format!("subscriptions[{index}]");
            let id: i64 = entry
                .get("id")
                .and_then(|v| v.as_str().and_then(|s| s.parse().ok()))
                .ok_or_else(|| invalid(format!("{what} needs a string id")))?;
            if id <= 0 {
                return Err(invalid(format!("{what} has a bad id")));
            }
            let failures = entry
                .get("failures")
                .map(|v| v.to_string())
                .unwrap_or_else(|| "[]".to_string());
            // Loud on corrupt per-item metadata: never silently drop it.
            if serde_json::from_str::<serde_json::Value>(&failures).is_err() {
                return Err(invalid(format!("{what} has corrupt failures")));
            }
            sub_rows.push(SubRow {
                id,
                label: get_str(entry, "label").unwrap_or_default(),
                url: get_str(entry, "url").unwrap_or_default(),
                group: get_str(entry, "group").unwrap_or_else(|| "default".to_string()),
                auto_refresh: entry
                    .get("auto_refresh")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
                last_refresh_at: entry
                    .get("last_refresh_at")
                    .and_then(serde_json::Value::as_i64),
                last_error: get_str(entry, "last_error"),
                failures,
            });
        }
        let mut tomb_rows: Vec<(String, String)> = Vec::new();
        for (index, entry) in tombs.iter().enumerate() {
            let what = format!("tombstones[{index}]");
            match (
                get_str(entry, "subscription_id"),
                get_str(entry, "identity_key"),
            ) {
                (Some(subscription_id), Some(identity_key)) => {
                    tomb_rows.push((subscription_id, identity_key));
                }
                _ => {
                    return Err(invalid(format!(
                        "{what} needs subscription_id + identity_key"
                    )))
                }
            }
        }
        let tx = self.conn.transaction()?;
        for sub in &sub_rows {
            tx.execute(
                "INSERT INTO subscriptions
                   (id, label, url, grp, auto_refresh, last_refresh_at, last_error, failures)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    sub.id,
                    sub.label,
                    sub.url,
                    sub.group,
                    sub.auto_refresh as i64,
                    sub.last_refresh_at,
                    sub.last_error,
                    sub.failures,
                ],
            )?;
        }
        for row in &rows {
            // Ticket 35: the export carries plaintext; seal on the way back
            // in (same at-rest invariant as live rows).
            // Ticket 37: `preset_id` is intentionally not restored — backups
            // cover endpoints, not routing policy. Restored rows rebuild
            // unattached (the `route_presets` table itself is untouched by
            // restore, so presets survive for re-attach in the manager).
            // Rebuilding an attachment from a bare id would risk dangling
            // references on cross-database restores; a dropped attachment
            // is visible in the manager, a dangling one would fail starts.
            let sealed_params = Self::seal_or_fail(&row.params)?;
            let sealed_synced = row
                .synced_params
                .as_deref()
                .map(Self::seal_or_fail)
                .transpose()?;
            tx.execute(
                "INSERT INTO proxy_configs
                   (id, name, protocol, params, grp, enabled, bind_ip, port,
                    subscription_id, identity_key, synced_params, adoptable)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    row.id,
                    row.name,
                    row.protocol,
                    sealed_params,
                    row.group,
                    row.enabled as i64,
                    row.bind_ip,
                    row.port,
                    row.subscription_id,
                    row.identity_key,
                    sealed_synced,
                    row.adoptable as i64,
                ],
            )?;
        }
        for (subscription_id, identity_key) in &tomb_rows {
            tx.execute(
                "INSERT INTO subscription_tombstones (subscription_id, identity_key)
                 VALUES (?1, ?2)",
                params![subscription_id, identity_key],
            )?;
        }
        tx.commit()?;
        Ok(RestoreOutcome {
            configs: rows.len(),
            subscriptions: sub_rows.len(),
            tombstones: tomb_rows.len(),
        })
    }

    // -- Ticket 36: data-plane traffic counters ----------------------------

    /// Add data-plane byte deltas for one config. Totals persist (restart
    /// resumes from these, never zeroed) and saturate at `i64::MAX`
    /// (SQLite integers are 64-bit signed).
    pub fn add_traffic(&mut self, id: &str, rx: u64, tx: u64) -> Result<(), StoreError> {
        let rowid: i64 = id
            .parse()
            .map_err(|_| StoreError::Invalid(vec![FieldError::new("id", "bad config id")]))?;
        let (cur_rx, cur_tx): (i64, i64) = match self.conn.query_row(
            "SELECT traffic_rx, traffic_tx FROM proxy_configs WHERE id = ?1",
            params![rowid],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ) {
            Ok(pair) => pair,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return Err(StoreError::NotFound(id.to_string()));
            }
            Err(other) => return Err(other.into()),
        };
        let rx = cur_rx.saturating_add(rx.min(i64::MAX as u64) as i64);
        let tx = cur_tx.saturating_add(tx.min(i64::MAX as u64) as i64);
        self.conn.execute(
            "UPDATE proxy_configs SET traffic_rx = ?1, traffic_tx = ?2 WHERE id = ?3",
            params![rx, tx, rowid],
        )?;
        Ok(())
    }

    /// All persisted data-plane totals keyed by config id (for display
    /// hydration after restart).
    pub fn traffic_totals(&self) -> Result<HashMap<String, (u64, u64)>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, traffic_rx, traffic_tx FROM proxy_configs")?;
        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let rx: i64 = row.get(1)?;
            let tx: i64 = row.get(2)?;
            Ok((id.to_string(), (rx.max(0) as u64, tx.max(0) as u64)))
        })?;
        let mut totals = HashMap::new();
        for row in rows {
            let (id, pair) = row?;
            totals.insert(id, pair);
        }
        Ok(totals)
    }

    // -- Ticket 12: health snapshot persistence ------------------------------

    /// Persist one config's latest sample (upsert keyed by `config_id`).
    /// Called after every recorded probe outcome; restart rehydrates from
    /// [`Store::load_health_snapshots`]. Error text here is a probe cause
    /// (already redacted at the health seam), never raw secrets.
    /// Ticket 25: absurd latencies saturate at `i64::MAX` (never wrap);
    /// implemented on [`Store::save_health_snapshots`].
    pub fn save_health_snapshot(&mut self, snapshot: &HealthSnapshot) -> Result<(), StoreError> {
        self.save_health_snapshots(std::slice::from_ref(snapshot))
    }

    /// Ticket 25: persist a whole probe round in ONE transaction (the
    /// store half of "health saves off the UI thread": one tx instead of N
    /// statements; the `spawn_blocking` move is main.rs territory, ticket 26).
    pub fn save_health_snapshots(
        &mut self,
        snapshots: &[HealthSnapshot],
    ) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        for snapshot in snapshots {
            let latency_ms: Option<i64> = snapshot
                .latency_ms
                .map(|ms| i64::try_from(ms).unwrap_or(i64::MAX));
            tx.execute(
                "INSERT INTO health_snapshots (config_id, at, kind, latency_ms, error)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(config_id) DO UPDATE SET
                   at = excluded.at, kind = excluded.kind,
                   latency_ms = excluded.latency_ms, error = excluded.error",
                params![
                    snapshot.config_id,
                    snapshot.at,
                    snapshot.kind,
                    latency_ms,
                    snapshot.error,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// All persisted latest-per-config samples (callers filter to known ids
    /// so snapshots of removed configs never resurrect rows).
    pub fn load_health_snapshots(&self) -> Result<Vec<HealthSnapshot>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT config_id, at, kind, latency_ms, error
             FROM health_snapshots ORDER BY config_id",
        )?;
        let rows = stmt
            .query_map([], map_snapshot_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// First free port in the auto range, given the set of taken ports.
    /// Ticket 27: primary then spillover (see [`auto_port_candidates`]).
    fn auto_port(&self) -> Result<u16, StoreError> {
        let mut stmt = self.conn.prepare("SELECT port FROM proxy_configs")?;
        let taken: HashSet<i64> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<HashSet<_>, _>>()?;
        for port in auto_port_candidates() {
            if !taken.contains(&(port as i64)) {
                return Ok(port);
            }
        }
        Err(StoreError::Invalid(vec![FieldError::new(
            "port",
            "no automatic port left in the auto range",
        )]))
    }

    /// Validate a patch against the current record; pure until the write.
    /// Ticket 16: blank protocol/params/group/bind means "keep current"
    /// (never silent defaults); blank name still errors; blank port still
    /// means auto-assign (explicit in the dialog placeholder).
    fn validate_patch(
        &self,
        current: &ProxyConfig,
        patch: &ConfigPatch,
    ) -> Result<ValidatedConfig, StoreError> {
        let mut errors = Vec::new();
        let name = match &patch.name {
            Some(raw) => validated_name(raw, &mut errors),
            None => Some(current.name.clone()),
        };
        let protocol_raw = match &patch.protocol {
            None => current.protocol.clone(),
            Some(raw) if raw.trim().is_empty() => current.protocol.clone(),
            Some(raw) => raw.clone(),
        };
        let params_raw = match &patch.params {
            None => current.params.clone(),
            Some(raw) if raw.trim().is_empty() => current.params.clone(),
            Some(raw) => raw.clone(),
        };
        let proto_params = validate_protocol_params(&protocol_raw, &params_raw, &mut errors);
        let group = match &patch.group {
            None => Some(current.group.clone()),
            Some(raw) if raw.trim().is_empty() => Some(current.group.clone()),
            Some(raw) => Some(raw.trim().to_string()),
        };
        let bind_ip = match &patch.bind_ip {
            None => Some(current.bind_ip.clone()),
            Some(raw) if raw.trim().is_empty() => Some(current.bind_ip.clone()),
            Some(raw) => validated_bind(raw, &mut errors),
        };
        let port = match &patch.port {
            Some(raw) => match parse_port_field(raw) {
                Ok(None) => match self.auto_port() {
                    Ok(port) => Some(port),
                    Err(StoreError::Invalid(mut auto_errors)) => {
                        errors.append(&mut auto_errors);
                        None
                    }
                    Err(error) => return Err(error),
                },
                Ok(port) => port,
                Err(error) => {
                    errors.push(error);
                    None
                }
            },
            None => Some(current.port),
        };
        if !errors.is_empty() {
            return Err(StoreError::Invalid(errors));
        }
        let (protocol, params) =
            proto_params.unwrap_or((current.protocol.clone(), current.params.clone()));
        Ok(ValidatedConfig {
            name: name.unwrap_or_else(|| current.name.clone()),
            protocol,
            params,
            group: group.unwrap_or_else(|| current.group.clone()),
            bind_ip: bind_ip.unwrap_or_else(|| current.bind_ip.clone()),
            port: port.unwrap_or(current.port),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_input(name: &str) -> NewConfig {
        NewConfig {
            name: name.to_string(),
            protocol: "vless".to_string(),
            params: String::new(),
            group: String::new(),
            bind_ip: String::new(),
            port: String::new(),
        }
    }

    /// Issue 02, scenario 1: empty library + add with loopback bind and auto
    /// port yields a disabled record that persists (visible via list).
    #[test]
    fn add_config_with_defaults_persists_disabled() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        assert!(store.list().expect("list works").is_empty());

        let created = store.add(valid_input("Work")).expect("valid add works");

        assert_eq!(created.name, "Work");
        assert!(!created.enabled);
        assert_eq!(created.bind_ip, DEFAULT_BIND);
        assert_eq!(created.group, DEFAULT_GROUP);
        assert_eq!(created.protocol, DEFAULT_PROTOCOL);
        assert!(
            (AUTO_PORT_START..=AUTO_PORT_END).contains(&created.port),
            "auto port in range, got {}",
            created.port
        );
        assert!(!created.id.is_empty());

        let rows = store.list().expect("list works");
        assert_eq!(rows, vec![created]);
    }

    /// Invalid add input is rejected per field and writes nothing.
    #[test]
    fn add_invalid_reports_fields_and_writes_nothing() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let error = store
            .add(NewConfig {
                name: "   ".to_string(),
                protocol: String::new(),
                params: String::new(),
                group: String::new(),
                bind_ip: "not-an-ip".to_string(),
                port: "99999".to_string(),
            })
            .expect_err("invalid add must fail");
        let StoreError::Invalid(fields) = error else {
            panic!("expected Invalid, got {error}");
        };
        let names: Vec<&str> = fields.iter().map(|e| e.field).collect();
        assert!(names.contains(&"name"), "name error, got {names:?}");
        assert!(names.contains(&"bind_ip"), "bind error, got {names:?}");
        assert!(names.contains(&"port"), "port error, got {names:?}");
        assert!(
            store.list().expect("list works").is_empty(),
            "failed add writes nothing"
        );
    }

    /// Issue 02, scenario 2: editing with an invalid port keeps the record.
    #[test]
    fn edit_invalid_port_leaves_record_unchanged() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let created = store.add(valid_input("Home")).expect("valid add works");

        let error = store
            .update(
                &created.id,
                ConfigPatch {
                    port: Some("99999".to_string()),
                    ..Default::default()
                },
            )
            .expect_err("invalid port edit must fail");
        let StoreError::Invalid(fields) = error else {
            panic!("expected Invalid, got {error}");
        };
        assert!(fields.iter().any(|e| e.field == "port"));

        let kept = store
            .get(&created.id)
            .expect("get works")
            .expect("record still present");
        assert_eq!(kept, created, "record unchanged after failed edit");
    }

    /// Invalid name edit is rejected and the record is unchanged.
    #[test]
    fn edit_invalid_name_leaves_record_unchanged() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let created = store.add(valid_input("Home")).expect("valid add works");

        store
            .update(
                &created.id,
                ConfigPatch {
                    name: Some("  ".to_string()),
                    ..Default::default()
                },
            )
            .expect_err("blank name edit must fail");

        let kept = store
            .get(&created.id)
            .expect("get works")
            .expect("record still present");
        assert_eq!(kept, created);
    }

    /// Valid edits apply; unknown ids report NotFound.
    #[test]
    fn edit_valid_fields_applies() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let created = store.add(valid_input("Home")).expect("valid add works");

        let updated = store
            .update(
                &created.id,
                ConfigPatch {
                    name: Some("Cabin".to_string()),
                    bind_ip: Some("127.0.0.2".to_string()),
                    port: Some("10821".to_string()),
                    ..Default::default()
                },
            )
            .expect("valid edit works");
        assert_eq!(updated.id, created.id);
        assert_eq!(updated.name, "Cabin");
        assert_eq!(updated.bind_ip, "127.0.0.2");
        assert_eq!(updated.port, 10821);

        let missing = store.update(
            "no-such-id",
            ConfigPatch {
                name: Some("X".to_string()),
                ..Default::default()
            },
        );
        assert!(
            matches!(missing, Err(StoreError::NotFound(_))),
            "unknown id is NotFound, got {missing:?}"
        );
    }

    /// Issue 02, scenario 3 (store half): remove deletes exactly one record.
    #[test]
    fn remove_deletes_only_target() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let first = store.add(valid_input("One")).expect("add works");
        let second = store.add(valid_input("Two")).expect("add works");

        assert!(store.remove(&first.id).expect("remove works"));
        assert!(!store.remove(&first.id).expect("repeat remove works"));

        let rows = store.list().expect("list works");
        assert_eq!(rows, vec![second]);
    }

    /// Enabled flag flips; unknown ids report NotFound.
    #[test]
    fn set_enabled_flips_flag() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let created = store.add(valid_input("Work")).expect("add works");
        assert!(!created.enabled);

        let on = store.set_enabled(&created.id, true).expect("enable works");
        assert!(on.enabled);
        let off = store
            .set_enabled(&created.id, false)
            .expect("disable works");
        assert!(!off.enabled);

        assert!(matches!(
            store.set_enabled("no-such-id", true),
            Err(StoreError::NotFound(_))
        ));
    }

    /// Ids are stable and never reused after removal.
    #[test]
    fn ids_are_stable_and_never_reused() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let first = store.add(valid_input("One")).expect("add works");
        let second = store.add(valid_input("Two")).expect("add works");
        assert_ne!(first.id, second.id);

        assert!(store.remove(&first.id).expect("remove works"));
        let third = store.add(valid_input("Three")).expect("add works");
        assert_ne!(third.id, first.id, "removed id is never reused");
        assert_ne!(third.id, second.id);

        let kept = store
            .get(&second.id)
            .expect("get works")
            .expect("second still present");
        assert_eq!(kept, second, "surviving record is untouched");
    }

    /// Auto ports do not collide across adds.
    #[test]
    fn auto_ports_are_unique() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let mut ports = HashSet::new();
        for index in 0..5 {
            let created = store
                .add(valid_input(&format!("Box-{index}")))
                .expect("add works");
            assert!(ports.insert(created.port), "port reused: {}", created.port);
        }
    }

    /// Stored configs may share a port; the conflict error naming both
    /// parties is raised at activation time (ticket 03), not at rest.
    #[test]
    fn duplicate_ports_allowed_at_rest() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let first = store
            .add(NewConfig {
                port: "10821".to_string(),
                ..valid_input("One")
            })
            .expect("add works");
        let second = store
            .add(NewConfig {
                port: "10821".to_string(),
                ..valid_input("Two")
            })
            .expect("duplicate port add works");
        assert_eq!(first.port, second.port);
    }

    /// Port field parsing covers the issue's 99999 example plus blanks.
    #[test]
    fn port_field_parsing() {
        assert_eq!(parse_port_field("").expect("blank is auto"), None);
        assert_eq!(parse_port_field("   ").expect("spaces are auto"), None);
        assert_eq!(
            parse_port_field("10821").expect("valid parses"),
            Some(10821)
        );
        assert_eq!(parse_port_field("1").expect("low edge parses"), Some(1));
        assert_eq!(
            parse_port_field("65535").expect("high edge parses"),
            Some(65535)
        );
        for raw in ["0", "99999", "abc", "10.5", "-1"] {
            let error = parse_port_field(raw).expect_err(&format!("{raw} is invalid"));
            assert_eq!(error.field, "port", "field name for {raw}");
        }
    }

    /// Bind validation: loopback default, any parseable IP allowed at rest.
    #[test]
    fn bind_validation() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let loopback = store.add(valid_input("A")).expect("default bind works");
        assert_eq!(loopback.bind_ip, DEFAULT_BIND);

        let lan = store
            .add(NewConfig {
                bind_ip: "0.0.0.0".to_string(),
                ..valid_input("B")
            })
            .expect("explicit bind stores");
        assert_eq!(lan.bind_ip, "0.0.0.0");
    }

    /// Issue 02, scenario 4 (store half): URL pair formats.
    #[test]
    fn proxy_url_pair_format() {
        let (socks, http) = proxy_urls("127.0.0.1", 10801);
        assert_eq!(socks, "socks5://127.0.0.1:10801");
        assert_eq!(http, "http://127.0.0.1:10801");
    }

    /// Delete confirm text names the config.
    #[test]
    fn delete_confirm_names_config() {
        let text = delete_confirm_text("Work VPN");
        assert!(
            text.contains("Work VPN"),
            "confirm names the config, got {text}"
        );
    }

    /// Ticket 09: the export warning names every secret class it protects
    /// (keys, tokens, passwords) and states the plaintext consequence.
    #[test]
    fn export_warning_names_secret_classes() {
        let text = export_warning_text();
        for class in ["private keys", "tokens", "passwords", "plaintext"] {
            assert!(text.contains(class), "warning names {class}: {text}");
        }
    }

    /// File-backed stores persist across reopen (restart scenario).
    #[test]
    fn file_store_persists_across_reopen() {
        let path =
            std::env::temp_dir().join(format!("manyproxy-ticket02-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let created = {
            let mut store = Store::open_file(&path).expect("file store opens");
            store.add(valid_input("Persisted")).expect("add works")
        };
        let reopened = Store::open_file(&path).expect("reopen works");
        let rows = reopened.list().expect("list works");
        assert_eq!(rows, vec![created]);
        let _ = std::fs::remove_file(&path);
    }

    /// IpAddr import is used (keeps the dependency honest if refactored).
    #[test]
    fn localhost_parses_as_loopback() {
        let ip: IpAddr = DEFAULT_BIND.parse().expect("default bind parses");
        assert!(ip.is_loopback());
    }

    // -- Ticket 04: subscriptions + refresh (merge seam; injected bodies) --

    const SUB_UUID: &str = "11111111-2222-4333-8444-555555555555";

    fn sub_body_two() -> String {
        format!("vless://{SUB_UUID}@a.example.com:443#one\ntrojan://pw@b.example.com:443#two")
    }

    fn add_sub(store: &mut Store, group: &str) -> Subscription {
        store
            .add_subscription("Nightly", "https://example.com/sub.txt", group, true)
            .expect("add subscription works")
    }

    /// Subscription intake validates at the boundary: blank labels and
    /// non-http(s) URLs are rejected per field, writing nothing.
    #[test]
    fn subscription_add_validates_label_and_url() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        for (label, url) in [
            ("", "https://example.com/sub.txt"),
            ("   ", "https://example.com/sub.txt"),
            ("Nightly", "not-a-url"),
            ("Nightly", "ftp://example.com/sub.txt"),
            ("Nightly", "https://"),
        ] {
            let error = store
                .add_subscription(label, url, "travel", true)
                .expect_err(&format!("{label:?} + {url:?} must fail"));
            let StoreError::Invalid(fields) = error else {
                panic!("expected Invalid, got {error}");
            };
            assert_eq!(fields.len(), 1, "one field, got {fields:?}");
            assert!(
                fields[0].field == "label" || fields[0].field == "url",
                "field name, got {fields:?}"
            );
            assert!(
                !fields[0].message.contains("token")
                    && !fields[0].message.contains("example.com/sub"),
                "error hides URL contents, got {}",
                fields[0].message
            );
        }
        assert!(
            store.list_subscriptions().expect("list works").is_empty(),
            "failed adds write nothing"
        );
        let created = store
            .add_subscription("Nightly", "https://example.com/sub.txt", "  ", true)
            .expect("blank group falls back");
        assert_eq!(created.group, DEFAULT_GROUP);
        assert!(created.auto_refresh);
        assert_eq!(created.last_refresh_at, None);
        assert_eq!(created.last_error, None);
    }

    /// Issue 04, scenario 1: new subscription + fetch → group created,
    /// members appear disabled, fetch recorded.
    #[test]
    fn subscription_refresh_creates_disabled_members() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        let outcome = store
            .refresh_with_body(&sub.id, &sub_body_two(), 1_700_000_000)
            .expect("refresh works");
        assert_eq!(outcome.added, 2);
        assert_eq!(outcome.updated, 0);
        assert_eq!(outcome.removed, 0);
        assert!(outcome.failures.is_empty());
        assert_eq!(outcome.error, None);
        let rows = store.list().expect("list works");
        assert_eq!(rows.len(), 2);
        for row in &rows {
            assert!(!row.enabled, "refresh never enables");
            assert_eq!(row.group, "travel");
            assert_eq!(row.subscription_id.as_deref(), Some(sub.id.as_str()));
            assert!(row.identity_key.is_some());
        }
        let kept = store
            .get_subscription(&sub.id)
            .expect("get works")
            .expect("sub present");
        assert_eq!(kept.last_refresh_at, Some(1_700_000_000));
        assert_eq!(kept.last_error, None);
    }

    /// Issue 04, scenario 2: hand-renamed member + unchanged upstream body →
    /// no duplicates, rename preserved (effective no-op).
    #[test]
    fn subscription_refresh_preserves_hand_rename_no_duplicates() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("refresh works");
        let first = store.list().expect("list works").remove(0);
        store
            .update(
                &first.id,
                ConfigPatch {
                    name: Some("My favourite".to_string()),
                    ..Default::default()
                },
            )
            .expect("rename works");
        let outcome = store
            .refresh_with_body(&sub.id, &sub_body_two(), 200)
            .expect("refresh works");
        assert_eq!(outcome.added, 0);
        assert_eq!(outcome.updated, 0);
        assert_eq!(outcome.removed, 0);
        let rows = store.list().expect("list works");
        assert_eq!(rows.len(), 2, "no duplicates, got {rows:?}");
        let renamed = store.get(&first.id).expect("get works").expect("kept");
        assert_eq!(renamed.name, "My favourite", "hand edit wins");
    }

    /// Changed upstream params with the same identity update in place (name
    /// still preserved); a remark-only change is a no-op.
    #[test]
    fn subscription_refresh_updates_params_in_place() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(
                &sub.id,
                &format!("vless://{SUB_UUID}@a.example.com:443#one"),
                100,
            )
            .expect("refresh works");
        let before = store.list().expect("list works").remove(0);
        // `flow` rides in params but is outside the identity key.
        let outcome = store
            .refresh_with_body(
                &sub.id,
                &format!("vless://{SUB_UUID}@a.example.com:443?flow=xtls-rprx-vision#one"),
                200,
            )
            .expect("refresh works");
        assert_eq!(outcome.updated, 1);
        assert_eq!(outcome.added, 0);
        let after = store.get(&before.id).expect("get works").expect("kept");
        assert_eq!(after.id, before.id, "updated in place, not re-added");
        assert_ne!(after.params, before.params);
        assert_eq!(after.name, before.name, "remark preserved");
        // Remark-only change: identical params, identical key → no-op.
        let noop = store
            .refresh_with_body(
                &sub.id,
                &format!(
                    "vless://{SUB_UUID}@a.example.com:443?flow=xtls-rprx-vision#renamed-upstream"
                ),
                300,
            )
            .expect("refresh works");
        assert_eq!(noop.added, 0);
        assert_eq!(noop.updated, 0);
        assert_eq!(store.list().expect("list works").len(), 1);
    }

    /// Issue 04, scenario 3: upstream removes a member → only the
    /// subscription-owned row is removed; hand-created configs are untouched.
    #[test]
    fn subscription_refresh_removes_only_owned() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("refresh works");
        let hand = store
            .add(NewConfig {
                name: "Hand made".to_string(),
                protocol: "vless".to_string(),
                params: "{}".to_string(),
                group: "travel".to_string(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("hand add works");
        let outcome = store
            .refresh_with_body(
                &sub.id,
                &format!("vless://{SUB_UUID}@a.example.com:443#one"),
                200,
            )
            .expect("refresh works");
        assert_eq!(outcome.removed, 1);
        let rows = store.list().expect("list works");
        assert_eq!(rows.len(), 2, "one owned row + hand row, got {rows:?}");
        let kept = store
            .get(&hand.id)
            .expect("get works")
            .expect("hand row kept");
        assert_eq!(kept, hand, "hand-created config untouched");
        assert_eq!(kept.subscription_id, None);
    }

    /// An upstream that empties cleanly (`[]`) removes owned members; that is
    /// a valid refresh, not a failure.
    #[test]
    fn subscription_refresh_empty_array_clears_owned() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("refresh works");
        let outcome = store
            .refresh_with_body(&sub.id, "[]", 200)
            .expect("refresh works");
        assert_eq!(outcome.error, None);
        assert_eq!(outcome.removed, 2);
        assert!(store.list().expect("list works").is_empty());
    }

    /// Issue 04, scenario 4: fetch/parse failure → prior members kept, error
    /// recorded on the subscription.
    #[test]
    fn subscription_refresh_failure_keeps_last_good() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("refresh works");
        let before = store.list().expect("list works");
        let outcome = store
            .refresh_with_body(&sub.id, "bogus://x\nbogus://y", 200)
            .expect("failure is an outcome, not an error");
        assert!(outcome.error.is_some(), "failure recorded");
        assert_eq!(outcome.added, 0);
        assert_eq!(outcome.removed, 0);
        assert!(!outcome.failures.is_empty(), "per-item failures counted");
        assert_eq!(store.list().expect("list works"), before, "members kept");
        let kept = store
            .get_subscription(&sub.id)
            .expect("get works")
            .expect("sub present");
        assert_eq!(kept.last_refresh_at, Some(100), "success stamp untouched");
        assert!(kept.last_error.is_some(), "error recorded");
        // Next success clears the error.
        store
            .refresh_with_body(&sub.id, &sub_body_two(), 300)
            .expect("refresh works");
        let cleared = store
            .get_subscription(&sub.id)
            .expect("get works")
            .expect("sub present");
        assert_eq!(cleared.last_error, None);
        assert_eq!(cleared.last_refresh_at, Some(300));
    }

    /// Prohibited side effect: refresh never enables/starts configs and never
    /// overwrites hand-edited overlay fields (name/enabled/bind/port).
    #[test]
    fn subscription_refresh_never_enables_or_clobbers_overlay() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("refresh works");
        let first = store.list().expect("list works").remove(0);
        store
            .set_enabled(&first.id, true)
            .expect("user enables a member");
        store
            .update(
                &first.id,
                ConfigPatch {
                    bind_ip: Some("127.0.0.2".to_string()),
                    port: Some("10821".to_string()),
                    ..Default::default()
                },
            )
            .expect("user edits bind");
        // Upstream adds a third member; the refresh must not touch the
        // user-enabled row's overlay and the newcomer stays disabled.
        let grown = format!("{}\ntrojan://pw@c.example.com:443#three", sub_body_two());
        let outcome = store
            .refresh_with_body(&sub.id, &grown, 200)
            .expect("refresh works");
        assert_eq!(outcome.added, 1);
        let edited = store.get(&first.id).expect("get works").expect("kept");
        assert!(
            edited.enabled,
            "refresh never disables user-enabled rows either"
        );
        assert_eq!(edited.bind_ip, "127.0.0.2");
        assert_eq!(edited.port, 10821);
        let newcomer = store
            .list()
            .expect("list works")
            .into_iter()
            .find(|row| row.name == "three")
            .expect("new member added");
        assert!(!newcomer.enabled, "new rows land disabled");
    }

    /// User-deleted owned rows are tombstoned: refresh does not resurrect.
    #[test]
    fn subscription_tombstone_prevents_resurrect() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("refresh works");
        let victim = store
            .list()
            .expect("list works")
            .into_iter()
            .find(|row| row.name == "two")
            .expect("member present");
        assert!(store.remove(&victim.id).expect("remove works"));
        let outcome = store
            .refresh_with_body(&sub.id, &sub_body_two(), 200)
            .expect("refresh works");
        assert_eq!(outcome.added, 0);
        assert_eq!(outcome.skipped, 1, "tombstoned row skipped");
        let rows = store.list().expect("list works");
        assert_eq!(rows.len(), 1);
        assert!(rows.iter().all(|row| row.name != "two"));
    }

    /// Upstream rows colliding with hand-created (or foreign) identity count
    /// as duplicates instead of duplicating.
    #[test]
    fn subscription_refresh_counts_foreign_identity_as_duplicate() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store
            .add(NewConfig {
                name: "Mine".to_string(),
                protocol: "trojan".to_string(),
                params: "{\"password\":\"pw\",\"server\":\"b.example.com\",\"server_port\":443,\"type\":\"trojan\",\"sni\":\"b.example.com\"}"
                    .to_string(),
                group: "other".to_string(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("hand add works");
        let sub = add_sub(&mut store, "travel");
        let outcome = store
            .refresh_with_body(&sub.id, "trojan://pw@b.example.com:443#two", 100)
            .expect("refresh works");
        assert_eq!(outcome.added, 0);
        assert_eq!(outcome.duplicates, 1);
        let rows = store.list().expect("list works");
        assert_eq!(rows.len(), 1, "no duplicate created, got {rows:?}");
    }

    /// Per-item failures skip + count while valid siblings still merge.
    #[test]
    fn subscription_refresh_per_item_failures_skip_and_count() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        let outcome = store
            .refresh_with_body(
                &sub.id,
                &format!("vless://{SUB_UUID}@a.example.com:443#one\nbogus://x"),
                100,
            )
            .expect("refresh works");
        assert_eq!(outcome.added, 1);
        assert_eq!(outcome.error, None, "partial success is not a failure");
        assert_eq!(outcome.failures.len(), 1);
        assert_eq!(store.list().expect("list works").len(), 1);
    }

    /// Ticket 17: per-item failures persist on the subscription (the stored
    /// vec behind the per-subscription failures view): partial refreshes
    /// store their failures, a clean refresh clears them, and a total
    /// failure stores its failures alongside `last_error`.
    #[test]
    fn subscription_failures_persist_and_clear() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        let kept = store
            .get_subscription(&sub.id)
            .expect("get works")
            .expect("sub present");
        assert!(
            kept.failures.is_empty(),
            "new subscriptions store no failures"
        );

        store
            .refresh_with_body(
                &sub.id,
                &format!("vless://{SUB_UUID}@a.example.com:443#one\nbogus://x"),
                100,
            )
            .expect("refresh works");
        let partial = store
            .get_subscription(&sub.id)
            .expect("get works")
            .expect("sub present");
        assert_eq!(partial.failures.len(), 1, "partial failures stored");
        assert!(
            partial.failures[0].reason.contains("unknown-scheme"),
            "reason kept, got {}",
            partial.failures[0].reason
        );
        assert_eq!(
            partial.failures[0].label, "bogus://x",
            "label kept without secrets"
        );

        // A clean refresh clears the stored vec.
        store
            .refresh_with_body(
                &sub.id,
                &format!("vless://{SUB_UUID}@a.example.com:443#one"),
                200,
            )
            .expect("refresh works");
        let clean = store
            .get_subscription(&sub.id)
            .expect("get works")
            .expect("sub present");
        assert!(clean.failures.is_empty(), "clean refresh clears failures");

        // A total failure stores its failures with the error.
        let outcome = store
            .refresh_with_body(&sub.id, "bogus://x\nbogus://y", 300)
            .expect("failure is an outcome");
        assert!(outcome.error.is_some());
        let failed = store
            .get_subscription(&sub.id)
            .expect("get works")
            .expect("sub present");
        assert_eq!(failed.failures.len(), 2, "total failures stored");
        assert!(failed.last_error.is_some(), "error still recorded");
    }

    /// Ticket 17: corrupted failures JSON decodes to no failures (never a
    /// crash, never an error).
    #[test]
    fn subscription_failures_decode_is_forgiving() {
        assert!(decode_failures(None).is_empty());
        assert!(decode_failures(Some(String::new())).is_empty());
        assert!(decode_failures(Some("not-json".to_string())).is_empty());
        assert!(decode_failures(Some("[{\"index\": 2}]".to_string())).len() == 1);
    }

    /// Removing a subscription releases (never deletes) its rows and drops
    /// its tombstones; unknown ids report NotFound/false.
    #[test]
    fn remove_subscription_releases_rows() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("refresh works");
        assert!(store.remove_subscription(&sub.id).expect("remove works"));
        assert!(!store.remove_subscription(&sub.id).expect("repeat works"));
        let rows = store.list().expect("list works");
        assert_eq!(rows.len(), 2, "released rows are kept");
        for row in &rows {
            assert_eq!(row.subscription_id, None);
            assert_eq!(row.identity_key, None);
        }
        assert!(store.list_subscriptions().expect("list works").is_empty());
        assert!(matches!(
            store.refresh_with_body(&sub.id, &sub_body_two(), 200),
            Err(StoreError::NotFound(_))
        ));
        assert!(matches!(
            store.set_subscription_auto_refresh("no-such-id", false),
            Err(StoreError::NotFound(_))
        ));
    }

    /// Ticket 14, scenario 5: export I/O failures name the destination path
    /// and never mislabel as a database error.
    #[test]
    fn export_io_error_names_path_never_database() {
        let store = Store::open_in_memory().expect("in-memory store opens");
        // A regular file where a directory is needed: dir creation must fail.
        let blocker =
            std::env::temp_dir().join(format!("manyproxy-t14-blocker-{}.tmp", std::process::id()));
        std::fs::write(&blocker, b"blocker").expect("blocker writes");
        let bad = blocker.join("manyproxy-export.json");
        let error = store
            .commit_export(&bad, true)
            .expect_err("export must fail");
        let _ = std::fs::remove_file(&blocker);
        let text = error.to_string();
        assert!(
            matches!(error, StoreError::Io { .. }),
            "Io variant, got {text}"
        );
        assert!(
            text.contains(&bad.display().to_string()),
            "names the path: {text}"
        );
        assert!(
            !text.contains("database error"),
            "never database-labeled: {text}"
        );
    }

    /// Ticket 14, scenario 6 (store seam): a broken database reports `Err`,
    /// never `Ok(false)`, so the UI can tell "already removed" from a real
    /// failure.
    #[test]
    fn remove_subscription_db_error_is_err_never_already_removed() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = store
            .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
            .expect("seed works");
        store.break_schema_for_test().expect("break works");
        assert!(
            matches!(store.remove_subscription(&sub.id), Err(StoreError::Sql(_))),
            "broken store reports Err, never Ok(false)"
        );
    }

    /// Ticket 14, scenario 2 (store seam): reads on a broken database report
    /// `Err` so the UI notifies instead of rendering a silent empty list.
    #[test]
    fn list_db_error_is_err_after_schema_break() {
        let store = Store::open_in_memory().expect("in-memory store opens");
        store.break_schema_for_test().expect("break works");
        assert!(matches!(store.list(), Err(StoreError::Sql(_))));
        assert!(matches!(
            store.get_subscription("1"),
            Err(StoreError::Sql(_))
        ));
        assert!(matches!(
            store.list_subscriptions(),
            Err(StoreError::Sql(_))
        ));
    }

    /// Auto-refresh scheduling hook: manual gate is `auto_refresh`; timing
    /// compares `last_refresh_at` against the interval.
    #[test]
    fn due_for_refresh_hook() {
        let scheduled = Subscription {
            id: "1".to_string(),
            label: "Nightly".to_string(),
            url: "https://example.com/sub.txt".to_string(),
            group: "travel".to_string(),
            auto_refresh: true,
            last_refresh_at: None,
            last_error: None,
            failures: Vec::new(),
        };
        assert!(
            due_for_refresh(&scheduled, 1_000, 86_400),
            "never refreshed is due"
        );
        let fresh = Subscription {
            last_refresh_at: Some(900),
            ..scheduled.clone()
        };
        assert!(!due_for_refresh(&fresh, 1_000, 86_400));
        assert!(
            due_for_refresh(&fresh, 1_000 + 86_400, 86_400),
            "due at interval"
        );
        let manual = Subscription {
            auto_refresh: false,
            ..scheduled.clone()
        };
        assert!(!due_for_refresh(&manual, 1_000_000, 1), "manual never due");
        let toggled = {
            let mut store = Store::open_in_memory().expect("in-memory store opens");
            let sub = add_sub(&mut store, "travel");
            store
                .set_subscription_auto_refresh(&sub.id, false)
                .expect("toggle works")
        };
        assert!(!toggled.auto_refresh);
    }

    // -- Ticket 11: user-owned JSON export -----------------------------------

    const EXPORT_UUID: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";

    fn export_seed_body() -> String {
        format!(
            "vless://{EXPORT_UUID}@a.example.com:443#one\ntrojan://pw@b.example.com:443#two\nss://YWVzLTEyOC1nY206dGVzdA@c.example.com:8388#three"
        )
    }

    fn export_seed(store: &mut Store) {
        let batch = crate::import::import_text(&export_seed_body());
        let outcome = crate::import::commit_batch(store, batch, "travel").expect("seed works");
        assert_eq!(outcome.imported, 3);
        store
            .add_subscription("Nightly", "https://example.com/sub.txt", "travel", true)
            .expect("seed subscription works");
    }

    /// The confirm dialog description is the warning text verbatim (no
    /// paraphrase, no trimming): secrets never leave silently.
    #[test]
    fn export_confirm_description_is_warning_verbatim() {
        assert_eq!(export_confirm_description(), export_warning_text());
    }

    /// Scenario 1: library with 3 configs → export with confirm → file
    /// parses as JSON, re-import yields 0 added / 3 duplicates, warning
    /// shown verbatim before write.
    #[test]
    fn export_round_trips_through_import_grammar_as_duplicates() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        export_seed(&mut store);

        let payload = store.export_library_json().expect("export works");
        let value: serde_json::Value =
            serde_json::from_str(&payload).expect("export parses as JSON");
        assert_eq!(
            value
                .get("outbounds")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(3),
            "all three rows are import-shaped"
        );
        assert_eq!(
            value
                .get("subscriptions")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(1),
            "subscriptions ride along"
        );
        // Plaintext by design (redacting here would break re-import): the
        // credential survives the round trip; the warning covers the risk.
        assert!(
            payload.contains("pw"),
            "export keeps fields valid for re-import"
        );

        let batch = crate::import::import_text(&payload);
        assert!(
            batch.failures.is_empty(),
            "no per-item failures, got {:?}",
            batch.failures
        );
        let outcome =
            crate::import::commit_batch(&mut store, batch, "travel").expect("re-import works");
        assert_eq!(outcome.imported, 0, "re-import adds nothing");
        assert_eq!(outcome.duplicates, 3, "re-import yields N duplicates");
        assert_eq!(store.list().expect("list works").len(), 3);
    }

    /// Hand-made rows whose params are not import-shaped stay metadata-only:
    /// they never poison the import path with per-item failures.
    #[test]
    fn export_keeps_non_import_rows_metadata_only() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        export_seed(&mut store);
        store
            .add(NewConfig {
                name: "Hand made".to_string(),
                protocol: "vless".to_string(),
                params: "{}".to_string(),
                group: "travel".to_string(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("hand add works");

        let payload = store.export_library_json().expect("export works");
        let value: serde_json::Value =
            serde_json::from_str(&payload).expect("export parses as JSON");
        assert_eq!(
            value
                .get("outbounds")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(3),
            "hand-made row stays out of the import path"
        );
        assert_eq!(
            value
                .get("library")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(4),
            "hand-made row is still backed up"
        );
        let batch = crate::import::import_text(&payload);
        assert!(
            batch.failures.is_empty(),
            "no failures, got {:?}",
            batch.failures
        );
    }

    /// Scenario: export with confirm → the file is written and parses.
    #[test]
    fn export_confirmed_writes_parseable_file() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        export_seed(&mut store);
        let path = std::env::temp_dir().join(format!(
            "manyproxy-export-confirm-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let written = store
            .commit_export(&path, true)
            .expect("confirmed export works");
        assert_eq!(written, Some(path.clone()));
        let on_disk = std::fs::read_to_string(&path).expect("export file was written");
        let value: serde_json::Value =
            serde_json::from_str(&on_disk).expect("export file parses as JSON");
        assert_eq!(
            value
                .get("outbounds")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(3)
        );
        let _ = std::fs::remove_file(&path);
    }

    /// Scenario: export cancelled → no file is created.
    #[test]
    fn export_cancelled_writes_no_file() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        export_seed(&mut store);
        let path = std::env::temp_dir().join(format!(
            "manyproxy-export-cancel-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let written = store
            .commit_export(&path, false)
            .expect("cancelled export works");
        assert_eq!(written, None);
        assert!(!path.exists(), "dismissing the warning creates no file");
        let _ = std::fs::remove_file(&path);
    }

    // -- Ticket 12: health snapshot persistence ------------------------------

    fn live_snapshot(id: &str, latency_ms: u64) -> HealthSnapshot {
        HealthSnapshot {
            config_id: id.to_string(),
            at: 1_700_000_000,
            kind: "url-test".to_string(),
            latency_ms: Some(latency_ms),
            error: None,
        }
    }

    fn dead_snapshot(id: &str) -> HealthSnapshot {
        HealthSnapshot {
            config_id: id.to_string(),
            at: 1_700_000_001,
            kind: "tcp-ping".to_string(),
            latency_ms: None,
            error: Some("tcp ping failed for \"Beta\": dial refused".to_string()),
        }
    }

    /// Latest-per-config samples upsert by config id (live latency + dead
    /// flag survive as stored).
    #[test]
    fn health_snapshots_save_load_round_trip() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        assert!(store
            .load_health_snapshots()
            .expect("load works")
            .is_empty());
        store
            .save_health_snapshot(&live_snapshot("1", 33))
            .expect("save works");
        store
            .save_health_snapshot(&dead_snapshot("2"))
            .expect("save works");
        // A newer sample for the same row replaces, never appends.
        store
            .save_health_snapshot(&live_snapshot("1", 44))
            .expect("resave works");

        let snaps = store.load_health_snapshots().expect("load works");
        assert_eq!(snaps.len(), 2);
        let first = snaps
            .iter()
            .find(|snap| snap.config_id == "1")
            .expect("row 1");
        assert_eq!(first.latency_ms, Some(44));
        assert_eq!(first.error, None);
        assert_eq!(first.kind, "url-test");
        let second = snaps
            .iter()
            .find(|snap| snap.config_id == "2")
            .expect("row 2");
        assert_eq!(second.latency_ms, None);
        assert!(second
            .error
            .as_deref()
            .unwrap_or("")
            .contains("dial refused"));
    }

    /// Removing a config prunes its snapshot (bounded table, no stale
    /// resurrection after restart).
    #[test]
    fn health_snapshot_pruned_with_config() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let created = store.add(valid_input("Gone")).expect("add works");
        store
            .save_health_snapshot(&live_snapshot(&created.id, 10))
            .expect("save works");
        assert_eq!(store.load_health_snapshots().expect("load works").len(), 1);
        assert!(store.remove(&created.id).expect("remove works"));
        assert!(store
            .load_health_snapshots()
            .expect("load works")
            .is_empty());
    }

    /// File-backed stores keep snapshots across reopen (restart scenario at
    /// the persistence seam).
    #[test]
    fn health_snapshots_persist_across_reopen() {
        let path =
            std::env::temp_dir().join(format!("manyproxy-ticket12-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let mut store = Store::open_file(&path).expect("file store opens");
            store
                .save_health_snapshot(&live_snapshot("7", 87))
                .expect("save works");
            store
                .save_health_snapshot(&dead_snapshot("9"))
                .expect("save works");
        }
        let reopened = Store::open_file(&path).expect("reopen works");
        let snaps = reopened.load_health_snapshots().expect("load works");
        assert_eq!(snaps.len(), 2);
        let _ = std::fs::remove_file(&path);
    }

    // -- Ticket 16: validation coherence + dedupe/group consistency ---------

    fn invalid_fields(error: &StoreError) -> Vec<String> {
        match error {
            StoreError::Invalid(fields) => fields.iter().map(|e| e.field.to_string()).collect(),
            other => panic!("expected Invalid, got {other}"),
        }
    }

    /// Scenario 1: unknown protocol or non-JSON params → field error, nothing
    /// written.
    #[test]
    fn manual_add_unknown_protocol_or_non_json_rejected() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let error = store
            .add(NewConfig {
                name: "Bad proto".to_string(),
                protocol: "bogus".to_string(),
                params: String::new(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect_err("unknown protocol must fail");
        assert!(
            invalid_fields(&error).contains(&"protocol".to_string()),
            "names protocol, got {error}"
        );
        let error = store
            .add(NewConfig {
                name: "Bad params".to_string(),
                protocol: "vless".to_string(),
                params: "not-json".to_string(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect_err("non-JSON params must fail");
        assert!(
            invalid_fields(&error).contains(&"params".to_string()),
            "names params, got {error}"
        );
        assert!(
            store.list().expect("list works").is_empty(),
            "failed adds write nothing"
        );
    }

    /// Scenario 1 (credential half): import-shaped vless without uuid lands
    /// no row — the same rule the link grammar enforces.
    #[test]
    fn manual_add_import_shaped_without_credential_rejected() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let error = store
            .add(NewConfig {
                name: "No uuid".to_string(),
                protocol: "vless".to_string(),
                params: "{\"type\":\"vless\",\"server\":\"a.example.com\",\"server_port\":443}"
                    .to_string(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect_err("vless without uuid must fail");
        let text = error.to_string();
        assert!(
            invalid_fields(&error).contains(&"params".to_string()),
            "names params, got {text}"
        );
        assert!(
            text.contains("uuid"),
            "names the missing credential, got {text}"
        );
        assert!(
            store.list().expect("list works").is_empty(),
            "failed add writes nothing"
        );
        // Opaque legacy rows stay valid (metadata-only, never poison import).
        let ok = store
            .add(NewConfig {
                name: "Opaque".to_string(),
                protocol: "vless".to_string(),
                params: String::new(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("opaque params still valid");
        assert_eq!(ok.params, "{}");
        assert!(!is_import_shaped_params(&ok.params));
    }

    /// Scenario 1 (edit half): bad protocol/params on edit fail pure (record
    /// unchanged).
    #[test]
    fn manual_edit_bad_protocol_or_params_leaves_record_unchanged() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let created = store.add(valid_input("Home")).expect("valid add works");
        let error = store
            .update(
                &created.id,
                ConfigPatch {
                    protocol: Some("bogus".to_string()),
                    ..Default::default()
                },
            )
            .expect_err("bad protocol edit must fail");
        assert!(invalid_fields(&error).contains(&"protocol".to_string()));
        let error = store
            .update(
                &created.id,
                ConfigPatch {
                    params: Some("not-json".to_string()),
                    ..Default::default()
                },
            )
            .expect_err("bad params edit must fail");
        assert!(invalid_fields(&error).contains(&"params".to_string()));
        let kept = store.get(&created.id).expect("get works").expect("kept");
        assert_eq!(kept, created, "record unchanged after failed edits");
    }

    /// Scenario 2: cleared protocol/params/group/bind keep current values;
    /// blank port still means auto (explicit in the dialog placeholder).
    #[test]
    fn edit_blank_preserves_current_values() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let created = store
            .add(NewConfig {
                name: "Keep".to_string(),
                protocol: "trojan".to_string(),
                params: "{\"type\":\"trojan\",\"server\":\"b.example.com\",\"server_port\":443,\"password\":\"pw\",\"sni\":\"b.example.com\"}".to_string(),
                group: "travel".to_string(),
                bind_ip: "127.0.0.2".to_string(),
                port: "10821".to_string(),
            })
            .expect("seed works");
        let kept = store
            .update(
                &created.id,
                ConfigPatch {
                    protocol: Some("   ".to_string()),
                    params: Some(String::new()),
                    group: Some("  ".to_string()),
                    bind_ip: Some(String::new()),
                    ..Default::default()
                },
            )
            .expect("blank edit preserves");
        assert_eq!(kept.protocol, "trojan", "protocol preserved");
        assert_eq!(kept.params, created.params, "params preserved");
        assert_eq!(kept.group, "travel", "group preserved");
        assert_eq!(kept.bind_ip, "127.0.0.2", "bind preserved");
        assert_eq!(kept.port, 10821, "untouched port preserved");
        // Blank port is the documented auto exception.
        let reauto = store
            .update(
                &created.id,
                ConfigPatch {
                    port: Some(String::new()),
                    ..Default::default()
                },
            )
            .expect("blank port re-autos");
        assert!(
            (AUTO_PORT_START..=AUTO_PORT_END).contains(&reauto.port),
            "auto port in range, got {}",
            reauto.port
        );
        assert_eq!(reauto.protocol, "trojan");
        assert_eq!(reauto.group, "travel");
    }

    /// Scenario 7 (store half): import-shaped vs total counts for the honest
    /// export toast.
    #[test]
    fn import_shaped_predicate_and_count() {
        assert!(is_import_shaped_params(
            "{\"type\":\"vless\",\"server\":\"a.example.com\",\"server_port\":443,\"uuid\":\"u\"}"
        ));
        assert!(!is_import_shaped_params("{}"));
        assert!(!is_import_shaped_params("not-json"));
        let rows = vec![
            ProxyConfig {
                id: "1".to_string(),
                name: "A".to_string(),
                protocol: "vless".to_string(),
                params: "{\"type\":\"vless\",\"server\":\"a.example.com\",\"server_port\":443,\"uuid\":\"u\"}".to_string(),
                group: "g".to_string(),
                enabled: false,
                bind_ip: DEFAULT_BIND.to_string(),
                port: 10801,
                subscription_id: None,
                identity_key: None,
                preset_id: None,
            },
            ProxyConfig {
                id: "2".to_string(),
                name: "B".to_string(),
                protocol: "vless".to_string(),
                params: "{}".to_string(),
                group: "g".to_string(),
                enabled: false,
                bind_ip: DEFAULT_BIND.to_string(),
                port: 10802,
                subscription_id: None,
                identity_key: None,
                preset_id: None,
            },
        ];
        assert_eq!(count_importable(&rows), 1);
    }

    // -- Ticket 25: store races, transactions, identity, off-thread halves --

    fn vless_params_with(server: &str, extra: &[(&str, &str)]) -> String {
        let mut map = serde_json::Map::new();
        map.insert(
            "type".to_string(),
            serde_json::Value::String("vless".to_string()),
        );
        map.insert(
            "server".to_string(),
            serde_json::Value::String(server.to_string()),
        );
        map.insert(
            "server_port".to_string(),
            serde_json::Value::Number(serde_json::Number::from(443)),
        );
        map.insert(
            "uuid".to_string(),
            serde_json::Value::String(SUB_UUID.to_string()),
        );
        for (key, value) in extra {
            map.insert(
                (*key).to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }
        serde_json::Value::Object(map).to_string()
    }

    /// update() on an owned row refreshes identity_key to the widened
    /// identity of the new values (never stale for the next refresh).
    #[test]
    fn t25_update_refreshes_identity_key() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("refresh works");
        let row = store
            .list()
            .expect("list works")
            .into_iter()
            .find(|row| row.name == "one")
            .expect("member one");
        let before = row.identity_key.clone().expect("owned rows carry identity");
        let fixed = vless_params_with("a.example.com", &[("flow", "xtls-rprx-vision")]);
        let updated = store
            .update(
                &row.id,
                ConfigPatch {
                    params: Some(fixed.clone()),
                    ..Default::default()
                },
            )
            .expect("edit works");
        let after = updated.identity_key.clone().expect("identity kept");
        assert_eq!(after, widened_identity(&updated.protocol, &fixed));
        assert_ne!(before, after, "flow is part of the widened identity");
    }

    /// Required scenario: a user param fix on an owned row survives refresh
    /// (kept, never silently reverted).
    #[test]
    fn t25_user_param_fix_survives_refresh() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("refresh works");
        let row = store
            .list()
            .expect("list works")
            .into_iter()
            .find(|row| row.name == "one")
            .expect("member one");
        let fixed = vless_params_with("a.example.com", &[("flow", "xtls-rprx-vision")]);
        store
            .update(
                &row.id,
                ConfigPatch {
                    params: Some(fixed.clone()),
                    ..Default::default()
                },
            )
            .expect("user fix works");
        let outcome = store
            .refresh_with_body(&sub.id, &sub_body_two(), 200)
            .expect("refresh works");
        assert_eq!(outcome.added, 0);
        assert_eq!(outcome.updated, 0);
        assert_eq!(outcome.removed, 0);
        assert_eq!(outcome.conflicts, 1, "divergence is explicit");
        let kept = store.get(&row.id).expect("get works").expect("kept");
        assert_eq!(kept.params, fixed, "user fix kept, never reverted");
        assert_eq!(store.list().expect("list works").len(), 2);
    }

    /// A non-identity user fix (extra params outside the key) pins the row:
    /// refresh keeps it with zero churn.
    #[test]
    fn t25_user_nonidentity_fix_pinned_no_churn() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("refresh works");
        let row = store
            .list()
            .expect("list works")
            .into_iter()
            .find(|row| row.name == "one")
            .expect("member one");
        // A true non-identity fix: the row's own params plus an extra the
        // identity ignores (every key field byte-identical).
        let mut value: serde_json::Value = serde_json::from_str(&row.params).expect("params parse");
        value["mux"] = serde_json::Value::String("on".to_string());
        let fixed = value.to_string();
        store
            .update(
                &row.id,
                ConfigPatch {
                    params: Some(fixed.clone()),
                    ..Default::default()
                },
            )
            .expect("user fix works");
        let outcome = store
            .refresh_with_body(&sub.id, &sub_body_two(), 200)
            .expect("refresh works");
        assert_eq!(outcome.added, 0);
        assert_eq!(outcome.updated, 0);
        assert_eq!(outcome.removed, 0);
        assert_eq!(outcome.conflicts, 0, "upstream did not move");
        let kept = store.get(&row.id).expect("get works").expect("kept");
        assert_eq!(kept.params, fixed);
    }

    /// Upstream identity rotation (password change, same server) updates in
    /// place: same id, same port, same enabled — never delete + re-add.
    #[test]
    fn t25_upstream_rotation_updates_in_place() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&sub.id, "trojan://pw1@s.example.com:443#n", 100)
            .expect("refresh works");
        let before = store.list().expect("list works").remove(0);
        store
            .set_enabled(&before.id, true)
            .expect("user enables the row");
        let outcome = store
            .refresh_with_body(&sub.id, "trojan://pw2@s.example.com:443#n", 200)
            .expect("refresh works");
        assert_eq!(outcome.updated, 1);
        assert_eq!(outcome.added, 0);
        assert_eq!(outcome.removed, 0);
        let after = store.get(&before.id).expect("get works").expect("kept");
        assert_eq!(after.id, before.id, "same row");
        assert_eq!(after.port, before.port, "same port");
        assert!(after.enabled, "enabled survives rotation");
        assert_ne!(after.params, before.params, "new credentials applied");
    }

    /// In-body duplicates are first-wins (the code matches the comment).
    #[test]
    fn t25_first_wins_in_body_dupes() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        let body = format!(
            "vless://{SUB_UUID}@a.example.com:443#one\nvless://{SUB_UUID}@a.example.com:443#two"
        );
        let outcome = store
            .refresh_with_body(&sub.id, &body, 100)
            .expect("refresh works");
        assert_eq!(outcome.added, 1);
        assert_eq!(outcome.duplicates, 1);
        let rows = store.list().expect("list works");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "one", "first wins");
    }

    /// Same-pass deletions free their ports, and multi-insert allocation is
    /// deterministic (key-sorted).
    #[test]
    fn t25_ports_reused_and_deterministic() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("refresh works");
        let before = store.list().expect("list works");
        let freed = before
            .iter()
            .find(|row| row.name == "one")
            .expect("member one")
            .port;
        let kept_port = before
            .iter()
            .find(|row| row.name == "two")
            .expect("member two")
            .port;
        let grown = "trojan://pw@b.example.com:443#two\ntrojan://pw@c.example.com:443#three\ntrojan://pw@d.example.com:443#four".to_string();
        let outcome = store
            .refresh_with_body(&sub.id, &grown, 200)
            .expect("refresh works");
        assert_eq!(outcome.removed, 1);
        assert_eq!(outcome.added, 2);
        let rows = store.list().expect("list works");
        let three = rows.iter().find(|row| row.name == "three").expect("three");
        let four = rows.iter().find(|row| row.name == "four").expect("four");
        assert_eq!(three.port, freed, "freed port is reused");
        assert_ne!(four.port, kept_port);
        assert_ne!(three.port, four.port);
        // Deterministic: the lower-sorted key takes the lower free port.
        let key_three = widened_identity(&three.protocol, &three.params);
        let key_four = widened_identity(&four.protocol, &four.params);
        assert_eq!(
            three.port < four.port,
            key_three < key_four,
            "allocation follows key order"
        );
    }

    /// Port exhaustion reports real per-item indices (never a blanket 0).
    /// Ticket 27: the range under test is the widened capacity (primary +
    /// spillover); exhaustion is honest only past both.
    #[test]
    fn t25_exhaustion_reports_real_indices() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let mut inputs = Vec::new();
        for (index, port) in auto_port_candidates().enumerate() {
            inputs.push(NewConfig {
                name: format!("fill-{index}"),
                protocol: "vless".to_string(),
                params: vless_params_with(&format!("s{index}.example.com"), &[]),
                group: "fill".to_string(),
                bind_ip: String::new(),
                port: port.to_string(),
            });
        }
        let created = store.add_batch(inputs).expect("fill works");
        assert_eq!(created.len(), AUTO_PORT_CAPACITY);
        let sub = add_sub(&mut store, "travel");
        let outcome = store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("refresh works");
        assert_eq!(outcome.added, 0);
        assert_eq!(outcome.failures.len(), 2);
        let mut indices: Vec<usize> = outcome
            .failures
            .iter()
            .map(|failure| failure.index)
            .collect();
        indices.sort_unstable();
        assert_eq!(indices, vec![0, 1], "real batch positions, got {indices:?}");
    }

    /// Released rows are adoptable: a second subscription adopts them in
    /// place (same id/port) instead of shadowing them as duplicates.
    #[test]
    fn t25_released_rows_adopted_by_second_subscription() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let first = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&first.id, &sub_body_two(), 100)
            .expect("refresh works");
        let before = store.list().expect("list works");
        assert!(store.remove_subscription(&first.id).expect("release works"));
        let second = store
            .add_subscription("Other", "https://example.com/other.txt", "travel", true)
            .expect("second sub works");
        let outcome = store
            .refresh_with_body(&second.id, &sub_body_two(), 200)
            .expect("refresh works");
        assert_eq!(outcome.added, 2, "adoptions count as gained members");
        assert_eq!(outcome.duplicates, 0, "no cross-sub shadowing");
        let after = store.list().expect("list works");
        assert_eq!(after.len(), 2);
        for row in &after {
            assert_eq!(row.subscription_id.as_deref(), Some(second.id.as_str()));
            assert!(row.identity_key.is_some());
        }
        let before_ids: Vec<&str> = before.iter().map(|row| row.id.as_str()).collect();
        let after_ids: Vec<&str> = after.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(before_ids, after_ids, "adopted in place, same ids");
    }

    /// Editing a released row claims it: a later subscription cannot adopt
    /// it away (the collision stays a duplicate).
    #[test]
    fn t25_update_claims_released_row_against_adoption() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let first = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&first.id, "trojan://pw@b.example.com:443#two", 100)
            .expect("refresh works");
        let row = store.list().expect("list works").remove(0);
        assert!(store.remove_subscription(&first.id).expect("release works"));
        store
            .update(
                &row.id,
                ConfigPatch {
                    name: Some("Mine now".to_string()),
                    ..Default::default()
                },
            )
            .expect("user claims the row");
        let second = store
            .add_subscription("Other", "https://example.com/other.txt", "travel", true)
            .expect("second sub works");
        let outcome = store
            .refresh_with_body(&second.id, "trojan://pw@b.example.com:443#two", 200)
            .expect("refresh works");
        assert_eq!(outcome.added, 0);
        assert_eq!(outcome.duplicates, 1);
        let kept = store.get(&row.id).expect("get works").expect("kept");
        assert_eq!(kept.subscription_id, None, "still unowned");
        assert_eq!(kept.name, "Mine now");
    }

    /// Refresh-deletes prune health snapshots (no stale resurrection).
    #[test]
    fn t25_refresh_delete_prunes_snapshot() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("refresh works");
        let row = store.list().expect("list works").remove(0);
        store
            .save_health_snapshot(&HealthSnapshot {
                config_id: row.id.clone(),
                at: 1_700_000_000,
                kind: "url-test".to_string(),
                latency_ms: Some(12),
                error: None,
            })
            .expect("save works");
        assert_eq!(store.load_health_snapshots().expect("load works").len(), 1);
        let outcome = store
            .refresh_with_body(
                &sub.id,
                &format!("vless://{SUB_UUID}@a.example.com:443#one"),
                200,
            )
            .expect("refresh works");
        assert_eq!(outcome.removed, 1);
        assert!(store
            .load_health_snapshots()
            .expect("load works")
            .is_empty());
    }

    /// Required scenario: export → wipe → restore is identical (rows, groups,
    /// enabled, subscriptions, tombstones); deletes stay deleted.
    #[test]
    fn t25_export_restore_round_trip() {
        let mut first = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut first, "travel");
        first
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("refresh works");
        let rows = first.list().expect("list works");
        let victim = rows
            .iter()
            .find(|row| row.name == "two")
            .expect("member two");
        let keeper = rows
            .iter()
            .find(|row| row.name == "one")
            .expect("member one");
        first
            .set_enabled(&keeper.id, true)
            .expect("user enables a member");
        assert!(first.remove(&victim.id).expect("user deletes a member"));
        let payload = first.export_library_json().expect("export works");

        let mut second = Store::open_in_memory().expect("fresh store opens");
        let outcome = second
            .restore_library_json(&payload)
            .expect("restore works");
        assert_eq!(
            outcome,
            RestoreOutcome {
                configs: 1,
                subscriptions: 1,
                tombstones: 1,
            }
        );
        assert_eq!(
            second.export_library_json().expect("re-export works"),
            payload,
            "restore is byte-identical"
        );
        let kept = second.get(&keeper.id).expect("get works").expect("kept");
        assert!(kept.enabled, "enabled survives");
        assert_eq!(kept.group, "travel");
        assert_eq!(kept.subscription_id.as_deref(), Some(sub.id.as_str()));
        let subs = second.list_subscriptions().expect("subs work");
        assert_eq!(subs.len(), 1);
        assert_eq!(
            subs[0].last_refresh_at,
            Some(100),
            "refresh metadata survives"
        );
        // No resurrection: the tombstoned member stays skipped.
        let refresh = second
            .refresh_with_body(&sub.id, &sub_body_two(), 200)
            .expect("refresh works");
        assert_eq!(refresh.added, 0);
        assert_eq!(refresh.skipped, 1);
        assert_eq!(second.list().expect("list works").len(), 1);
    }

    /// Restore fails loudly on garbage (never a silent empty library).
    #[test]
    fn t25_restore_rejects_garbage_loudly() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        for payload in [
            "not-json",
            "{}",
            "{\"library\": [{\"id\": \"1\", \"port\": 99999}], \"subscriptions\": []}",
        ] {
            let error = store
                .restore_library_json(payload)
                .expect_err(&format!("must fail: {payload}"));
            assert!(
                matches!(error, StoreError::Invalid(_)),
                "Invalid, got {error}"
            );
            assert!(store.list().expect("list works").is_empty());
        }
        // An honest empty export restores to an honest empty library.
        let outcome = store
            .restore_library_json("{\"library\": [], \"subscriptions\": []}")
            .expect("empty restore works");
        assert_eq!(
            outcome,
            RestoreOutcome {
                configs: 0,
                subscriptions: 0,
                tombstones: 0,
            }
        );
    }

    /// Atomic export: tmp + rename leaves a parseable file and no tmp litter.
    #[test]
    fn t25_export_is_atomic_no_tmp_litter() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        export_seed(&mut store);
        let dir = std::env::temp_dir();
        let path = dir.join(format!("manyproxy-t25-atomic-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        store.commit_export(&path, true).expect("export works");
        let on_disk = std::fs::read_to_string(&path).expect("file written");
        serde_json::from_str::<serde_json::Value>(&on_disk).expect("parses");
        // Ticket 39: scan only this export's own tmp sibling
        // (`.{filename}.tmp-{pid}`, see `tmp_export_path`) — never the
        // whole shared temp dir, where a concurrent test's in-flight
        // export tmp is legitimate work, not our litter.
        let own_tmp_prefix = format!(
            ".{}.tmp-",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
        );
        let litter: Vec<_> = std::fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(|entry| entry.ok().map(|entry| entry.file_name()))
            .filter(|name| name.to_string_lossy().starts_with(&own_tmp_prefix))
            .collect();
        assert!(litter.is_empty(), "no tmp litter, got {litter:?}");
        let _ = std::fs::remove_file(&path);
    }

    /// Batch insert: 500 rows in one transaction; one bad item writes nothing.
    #[test]
    fn t25_add_batch_is_atomic() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let inputs: Vec<NewConfig> = (0..500)
            .map(|index| NewConfig {
                name: format!("bulk-{index}"),
                protocol: "vless".to_string(),
                params: vless_params_with(&format!("h{index}.example.com"), &[]),
                group: "bulk".to_string(),
                bind_ip: String::new(),
                // Explicit unique ports: duplicates stay legal at rest, so
                // pin distinct ones outside the auto ranges.
                port: (20_000 + index).to_string(),
            })
            .collect();
        let created = store.add_batch(inputs).expect("500-row batch works");
        assert_eq!(created.len(), 500);
        let mut ports: Vec<u16> = created.iter().map(|row| row.port).collect();
        ports.sort_unstable();
        ports.dedup();
        assert_eq!(ports.len(), 500, "auto ports unique in one batch");

        let mut bad: Vec<NewConfig> = (0..10)
            .map(|index| NewConfig {
                name: format!("ok-{index}"),
                protocol: "vless".to_string(),
                params: vless_params_with(&format!("k{index}.example.com"), &[]),
                group: "bulk".to_string(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .collect();
        bad.push(NewConfig {
            name: "   ".to_string(),
            protocol: "vless".to_string(),
            params: String::new(),
            group: String::new(),
            bind_ip: String::new(),
            port: String::new(),
        });
        let before = store.list().expect("list works").len();
        assert!(matches!(store.add_batch(bad), Err(StoreError::Invalid(_))));
        assert_eq!(
            store.list().expect("list works").len(),
            before,
            "failed batch writes nothing"
        );
    }

    /// Health batch: one call persists a whole probe round; absurd latencies
    /// saturate instead of wrapping.
    #[test]
    fn t25_health_batch_saves_round() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store
            .save_health_snapshots(&[
                live_snapshot("1", 10),
                dead_snapshot("2"),
                HealthSnapshot {
                    config_id: "3".to_string(),
                    at: 1_700_000_002,
                    kind: "url-test".to_string(),
                    latency_ms: Some(u64::MAX),
                    error: None,
                },
            ])
            .expect("batch save works");
        let snaps = store.load_health_snapshots().expect("load works");
        assert_eq!(snaps.len(), 3);
        let huge = snaps
            .iter()
            .find(|snap| snap.config_id == "3")
            .expect("row 3");
        assert_eq!(
            huge.latency_ms,
            Some(i64::MAX as u64),
            "saturates, never wraps"
        );
    }

    /// Legacy databases (user_version 0, no ticket-25 columns) migrate
    /// forward on open: ownership columns reappear and refresh works.
    #[test]
    fn t25_legacy_database_migrates_forward() {
        let path =
            std::env::temp_dir().join(format!("manyproxy-t25-legacy-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let store = Store::open_file(&path).expect("file store opens");
            store
                .conn
                .execute_batch(
                    "ALTER TABLE proxy_configs DROP COLUMN adoptable;
                     ALTER TABLE proxy_configs DROP COLUMN synced_params;
                     PRAGMA user_version = 0;",
                )
                .expect("legacy shape simulated");
        }
        let mut store = Store::open_file(&path).expect("legacy file reopens");
        let version: i64 = store
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version reads");
        assert_eq!(version, Store::SCHEMA_VERSION);
        // Ownership merge works on the migrated file (release + adopt).
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&sub.id, "trojan://pw@b.example.com:443#two", 100)
            .expect("refresh works");
        assert!(store.remove_subscription(&sub.id).expect("release works"));
        let second = store
            .add_subscription("Other", "https://example.com/other.txt", "travel", true)
            .expect("second sub works");
        let outcome = store
            .refresh_with_body(&second.id, "trojan://pw@b.example.com:443#two", 200)
            .expect("refresh works");
        assert_eq!(outcome.added, 1, "adopted after migration");
        assert_eq!(outcome.duplicates, 0);
        let _ = std::fs::remove_file(&path);
    }

    /// A database from a NEWER build fails loudly on open (never silent).
    #[test]
    fn t25_newer_schema_fails_loudly() {
        let path =
            std::env::temp_dir().join(format!("manyproxy-t25-newer-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let store = Store::open_file(&path).expect("file store opens");
            store
                .conn
                .execute_batch("PRAGMA user_version = 999;")
                .expect("newer stamp simulated");
        }
        let error = match Store::open_file(&path) {
            Ok(_) => panic!("newer schema must fail"),
            Err(error) => error,
        };
        let text = error.to_string();
        assert!(text.contains("999"), "names the version, got {text}");
        assert!(
            matches!(error, StoreError::Sql(_)),
            "loud database error, never a silent empty: {text}"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// Non-ASCII subscription URLs never panic (valid ones still accepted).
    #[test]
    fn t25_non_ascii_subscription_urls_never_panic() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let ok = store
            .add_subscription("Intl", "https://éxample.com/sub.txt", "g", true)
            .expect("non-ascii host accepted");
        assert_eq!(ok.group, "g");
        for url in [
            "http:/é",
            "https://",
            "https://exa mple.com/sub",
            "ftp://éxample.com/sub",
        ] {
            assert!(
                matches!(
                    store.add_subscription("L", url, "g", true),
                    Err(StoreError::Invalid(_))
                ),
                "rejected without panic: {url}"
            );
        }
    }

    /// The widened identity splits transport/security variants that the
    /// narrow identity merges (no over-merge); remark never matters.
    #[test]
    fn t25_widened_identity_splits_variants() {
        let base = vless_params_with("a.example.com", &[]);
        let flow = vless_params_with("a.example.com", &[("flow", "xtls-rprx-vision")]);
        assert_eq!(
            crate::import::normalized_identity("vless", &base),
            crate::import::normalized_identity("vless", &flow),
            "narrow merges flow variants"
        );
        assert_ne!(
            widened_identity("vless", &base),
            widened_identity("vless", &flow),
            "widened splits them"
        );
        let upper = vless_params_with("A.EXAMPLE.COM", &[("flow", "XTLS-RPRX-VISION")]);
        assert_eq!(
            widened_identity("vless", &flow),
            widened_identity("vless", &upper),
            "enum-like fields compare case-insensitively"
        );
    }

    // -- Ticket 32: store correctness (IPv6 URLs, AWG identity, hardened
    // export) ------------------------------------------------------------

    /// Ticket 32: IPv6 binds bracket in copied URLs (`[::1]:port`).
    #[test]
    fn t32_proxy_urls_bracket_ipv6() {
        let (socks, http) = proxy_urls("::1", 10801);
        assert_eq!(socks, "socks5://[::1]:10801");
        assert_eq!(http, "http://[::1]:10801");
        let (socks, http) = proxy_urls("127.0.0.1", 10801);
        assert_eq!(socks, "socks5://127.0.0.1:10801");
        assert_eq!(http, "http://127.0.0.1:10801");
        let (socks, _) = proxy_urls("2001:db8::1", 10802);
        assert_eq!(socks, "socks5://[2001:db8::1]:10802");
    }

    /// Ticket 32: two AWG rows sharing server+creds but a different `jc`
    /// are two rows (identity includes AWG keys); identical rows dedupe.
    #[test]
    fn t32_awg_identity_includes_obfuscation_keys() {
        fn awg_params_with(jc: &str) -> String {
            serde_json::json!({
                "type": "amneziawg", "server": "192.0.2.90", "server_port": 51820,
                "private_key": "LOCAL-PRIVATE", "public_key": "REMOTE-PUBLIC",
                "awg_version": "2.0", "jc": jc, "jmin": "30", "jmax": "1000",
            })
            .to_string()
        }
        let first = awg_params_with("5");
        let second = awg_params_with("9");
        assert_eq!(
            crate::import::normalized_identity("amneziawg", &first),
            crate::import::normalized_identity("amneziawg", &second),
            "narrow identity ignores obfuscation"
        );
        assert_ne!(
            widened_identity("amneziawg", &first),
            widened_identity("amneziawg", &second),
            "widened identity splits jc variants"
        );
        assert_eq!(
            widened_identity("amneziawg", &first),
            widened_identity("amneziawg", &first.clone()),
            "identical rows share identity"
        );
        // End to end: refresh/import keeps both rows.
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let batch = crate::import::BatchResult {
            configs: vec![
                crate::import::ImportedConfig {
                    name: "AwgFive".to_string(),
                    protocol: "amneziawg".to_string(),
                    params: first,
                    host: "192.0.2.90".to_string(),
                    port: 51820,
                },
                crate::import::ImportedConfig {
                    name: "AwgNine".to_string(),
                    protocol: "amneziawg".to_string(),
                    params: second,
                    host: "192.0.2.90".to_string(),
                    port: 51820,
                },
            ],
            failures: Vec::new(),
        };
        let outcome = crate::import::commit_batch(&mut store, batch, "awg").expect("import works");
        assert_eq!(outcome.imported, 2, "two rows, got {outcome:?}");
        assert_eq!(outcome.duplicates, 0);
        assert_eq!(store.list().expect("list works").len(), 2);
    }

    /// Ticket 32: the export file lands 0600/ACL-hardened like the DB (user
    /// only; the payload holds secrets at rest).
    #[test]
    fn t32_export_file_hardened_like_db() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store.add(valid_input("ExportMe")).expect("add works");
        let path =
            std::env::temp_dir().join(format!("manyproxy-t32-export-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let written = store
            .commit_export(&path, true)
            .expect("export writes")
            .expect("confirm returns the path");
        assert_eq!(written, path);
        assert!(path.is_file(), "export file exists");
        let payload: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("export reads"))
                .expect("export is JSON");
        assert!(payload.get("library").is_some(), "full-fidelity payload");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(&path)
                .expect("metadata reads")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "user-only export, got {mode:o}");
        }
        let _ = std::fs::remove_file(&path);
    }

    // -- Ticket 27: caps lift + secrets hardening + fetch policy -----------

    /// 250 auto-port configs all assign (primary then spillover); exhaustion
    /// stays honest only past the widened capacity.
    #[test]
    fn ticket27_auto_ports_assign_250_with_spillover_and_honest_exhaustion() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let inputs: Vec<NewConfig> = (0..250)
            .map(|index| NewConfig {
                name: format!("auto-{index}"),
                protocol: "vless".to_string(),
                params: vless_params_with(&format!("h{index}.example.com"), &[]),
                group: "bulk".to_string(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .collect();
        let created = store.add_batch(inputs).expect("250 auto ports assign");
        assert_eq!(created.len(), 250);
        let mut ports: Vec<u16> = created.iter().map(|row| row.port).collect();
        ports.sort_unstable();
        ports.dedup();
        assert_eq!(ports.len(), 250, "all unique");
        assert!(
            ports.iter().all(|port| is_auto_port(*port)),
            "every port from an auto range"
        );
        assert!(
            ports.iter().any(|port| *port >= AUTO_PORT_SPILL_START),
            "spillover serves overflow past the 199-port primary"
        );
        // Fill to capacity, then prove honest exhaustion (message unchanged).
        let mut fill: Vec<NewConfig> = Vec::new();
        for port in auto_port_candidates() {
            if ports.contains(&port) {
                continue;
            }
            fill.push(NewConfig {
                name: format!("fill-{port}"),
                protocol: "vless".to_string(),
                params: vless_params_with(&format!("f{port}.example.com"), &[]),
                group: "bulk".to_string(),
                bind_ip: String::new(),
                port: port.to_string(),
            });
        }
        let filled = store.add_batch(fill).expect("fill to capacity works");
        assert_eq!(filled.len(), AUTO_PORT_CAPACITY - 250);
        let error = store
            .add(NewConfig {
                name: "one-too-many".to_string(),
                protocol: "vless".to_string(),
                params: vless_params_with("last.example.com", &[]),
                group: "bulk".to_string(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect_err("past capacity exhausts honestly");
        let text = error.to_string();
        assert!(
            text.contains("no automatic port left"),
            "honest exhaustion, got {text}"
        );
    }

    /// Fresh file DBs carry user-only permissions; the backlog note records
    /// keychain/DPAPI as a non-goal (never shipped silently).
    #[test]
    fn ticket27_db_file_has_user_only_permissions_and_backlog_note() {
        let path =
            std::env::temp_dir().join(format!("manyproxy-t27-perms-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let _store = Store::open_file(&path).expect("file store opens");
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
        }
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(PathBuf::from(format!("{}-wal", path.display())));
        let _ = std::fs::remove_file(PathBuf::from(format!("{}-shm", path.display())));
        assert!(
            SECRETS_AT_REST_NOTE.contains("keychain") && SECRETS_AT_REST_NOTE.contains("DPAPI"),
            "backlog recorded, got {SECRETS_AT_REST_NOTE}"
        );
        // Spot permissions are best-effort on hardened files.
        let probe =
            std::env::temp_dir().join(format!("manyproxy-t27-harden-{}.tmp", std::process::id()));
        std::fs::write(&probe, "secrets").expect("probe writes");
        harden_file_permissions(&probe);
        assert!(probe.exists());
        let _ = std::fs::remove_file(&probe);
    }

    /// Subscription-fetch policy: https/public allows, http warns, internal
    /// blocks (reasons never echo the URL); add rejects internal only.
    #[test]
    fn ticket27_subscription_fetch_policy_warn_block_allow() {
        use SubscriptionFetchPolicy::{Allow, Block, Warn};
        assert!(matches!(
            subscription_fetch_policy("https://example.com/sub.txt"),
            Allow
        ));
        assert!(matches!(
            subscription_fetch_policy("http://example.com/sub.txt"),
            Warn { .. }
        ));
        for url in [
            "http://127.0.0.1/sub.txt",
            "https://127.0.0.1/sub.txt",
            "https://10.0.0.5/sub",
            "https://192.168.1.20/sub",
            "https://172.16.0.9/sub",
            "https://localhost/sub",
            "https://[::1]/sub",
            "http://[::]/sub",
        ] {
            assert!(
                matches!(subscription_fetch_policy(url), Block { .. }),
                "blocked: {url}"
            );
        }
        // Reasons never echo hosts/tokens.
        let blocked = subscription_fetch_policy("http://127.0.0.1/sub?token=hunter2");
        let reason = match blocked {
            Block { reason } => reason,
            other => panic!("expected Block, got {other:?}"),
        };
        assert!(!reason.contains("127.0.0.1"), "no host echo: {reason}");
        assert!(!reason.contains("hunter2"), "no token echo: {reason}");

        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let error = store
            .add_subscription("Local", "http://127.0.0.1/sub.txt", "g", true)
            .expect_err("internal add blocked");
        assert!(
            error.to_string().contains("internal"),
            "names block, got {error}"
        );
        // Plain http stays addable (Warn at fetch, LAN-provider compat).
        store
            .add_subscription("Plain", "http://example.com/sub.txt", "g", true)
            .expect("http add allowed");
    }

    /// Manual ssh/noise rows validate like the import path (credentials
    /// required; opaque rows still pass through).
    #[test]
    fn ticket27_manual_ssh_noise_rows_validate() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store
            .add(NewConfig {
                name: "Ssh".to_string(),
                protocol: "ssh".to_string(),
                params: "{\"type\":\"ssh\",\"server\":\"a.example.com\",\"server_port\":22,\"username\":\"u\",\"password\":\"p\"}".to_string(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("ssh with creds works");
        store
            .add(NewConfig {
                name: "Noise".to_string(),
                protocol: "noise".to_string(),
                params: "{\"type\":\"noise\",\"server\":\"a.example.com\",\"server_port\":51820,\"key\":\"k\"}".to_string(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("noise with key works");
        for (protocol, params) in [
            (
                "ssh",
                "{\"type\":\"ssh\",\"server\":\"a.example.com\",\"server_port\":22,\"username\":\"u\"}",
            ),
            (
                "noise",
                "{\"type\":\"noise\",\"server\":\"a.example.com\",\"server_port\":51820}",
            ),
        ] {
            let error = store
                .add(NewConfig {
                    name: "Bad".to_string(),
                    protocol: protocol.to_string(),
                    params: params.to_string(),
                    group: String::new(),
                    bind_ip: String::new(),
                    port: String::new(),
                })
                .expect_err(&format!("{protocol} without creds fails"));
            assert!(
                matches!(error, StoreError::Invalid(_)),
                "typed, got {error}"
            );
        }
    }

    // -- Ticket 35: DPAPI secrets at rest ------------------------------

    /// Ticket 35: DPAPI round trip — sealed bytes carry no plaintext and
    /// open back to the exact input.
    #[test]
    fn ticket35_dpapi_round_trip() {
        let plain = br#"{"private_key":"SECRET-PRIVATE-KEY-MATERIAL"}"#;
        let sealed = crate::dpapi::protect(plain).expect("protects");
        assert!(
            !sealed.windows(6).any(|w| w == b"SECRET"),
            "sealed bytes carry no plaintext"
        );
        let opened = crate::dpapi::unprotect(&sealed).expect("unprotects");
        assert_eq!(opened, plain, "round trip is exact");
    }

    /// Ticket 35: tampered ciphertext fails loudly (never silent plaintext).
    #[test]
    fn ticket35_dpapi_tamper_fails_loud() {
        let mut sealed = crate::dpapi::protect(b"hello").expect("protects");
        let last = sealed.len() - 1;
        sealed[last] ^= 0x01;
        let error = crate::dpapi::unprotect(&sealed).expect_err("tamper must fail");
        assert!(!error.contains("hello"), "no secret echo: {error}");
    }

    /// Ticket 35: a stored row's params column carries no credential
    /// plaintext at rest, and reads back identically.
    #[test]
    fn ticket35_stored_params_have_no_plaintext() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let created = store
            .add(NewConfig {
                name: "Sealed".to_string(),
                protocol: "shadowsocks".to_string(),
                params: r#"{"server":"198.51.100.9","server_port":8388,"method":"aes-128-gcm","password":"SECRET-PASSWORD"}"#.to_string(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("add works");
        let raw: String = store
            .conn
            .query_row(
                "SELECT params FROM proxy_configs WHERE id = ?1",
                rusqlite::params![created.rowid()],
                |row| row.get(0),
            )
            .expect("raw read works");
        assert!(
            !raw.contains("SECRET-PASSWORD"),
            "params column sealed at rest"
        );
        let back = store.list().expect("list works");
        assert_eq!(back.len(), 1);
        assert!(
            back[0].params.contains("SECRET-PASSWORD"),
            "reads decrypt transparently"
        );
    }

    /// Ticket 35: a v2 plaintext database migrates to sealed params on open
    /// with zero data loss (file-backed migration fixture).
    #[test]
    fn ticket35_v2_plaintext_migrates_to_sealed() {
        let dir = std::env::temp_dir().join(format!(
            "manyproxy-t35-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock works")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir works");
        let path = dir.join("library.db");
        {
            let mut store = Store::open_file(&path).expect("v3 store opens");
            store
                .add(NewConfig {
                    name: "Legacy".to_string(),
                    protocol: "shadowsocks".to_string(),
                    params: r#"{"server":"198.51.100.9","server_port":8388,"method":"aes-128-gcm","password":"LEGACY-SECRET"}"#.to_string(),
                    group: String::new(),
                    bind_ip: String::new(),
                    port: String::new(),
                })
                .expect("add works");
            // Rewind to a v2 plaintext shape: raw params + user_version 2.
            store
                .conn
                .execute(
                    "UPDATE proxy_configs SET params = ?1",
                    rusqlite::params![r#"{"server":"198.51.100.9","server_port":8388,"method":"aes-128-gcm","password":"LEGACY-SECRET"}"#],
                )
                .expect("rewind works");
            store
                .conn
                .execute_batch("PRAGMA user_version = 2;")
                .expect("version rewind works");
        }
        {
            let store = Store::open_file(&path).expect("migrated open works");
            let rows = store.list().expect("list works");
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].name, "Legacy");
            assert!(
                rows[0].params.contains("LEGACY-SECRET"),
                "row survives migration"
            );
            let raw: String = store
                .conn
                .query_row("SELECT params FROM proxy_configs", [], |row| row.get(0))
                .expect("raw read works");
            assert!(
                !raw.contains("LEGACY-SECRET"),
                "migration sealed the column"
            );
            let version: i64 = store
                .conn
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .expect("version reads");
            assert_eq!(version, 4, "schema advanced to 4");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    // -- Ticket 36: data-plane traffic counters ------------------------

    /// Ticket 36: traffic deltas accumulate per row and read back as a map.
    #[test]
    fn ticket36_traffic_accumulates_per_row() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let created = store
            .add(NewConfig {
                name: "Flow".to_string(),
                protocol: "shadowsocks".to_string(),
                params: r#"{"server":"198.51.100.9","server_port":8388,"method":"aes-128-gcm","password":"p"}"#.to_string(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("add works");
        store
            .add_traffic(&created.id, 1_048_782, 88)
            .expect("add works");
        store.add_traffic(&created.id, 100, 10).expect("add works");
        let totals = store.traffic_totals().expect("totals read");
        assert_eq!(totals.get(&created.id), Some(&(1_048_882, 98)));
    }

    // -- Ticket 31: dialogs + import + subscriptions ---------------------

    /// Ticket 31: a typo protocol error names the valid protocol list (never
    /// just "unknown").
    #[test]
    fn ticket31_unknown_protocol_names_valid_protocols() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let error = store
            .add(NewConfig {
                name: "Typo".to_string(),
                protocol: "vles".to_string(),
                params: String::new(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect_err("typo protocol must fail");
        let text = error.to_string();
        assert!(text.contains("vles"), "echoes the typo, got {text}");
        for valid in ["vless", "vmess", "trojan", "wireguard", "shadowsocks"] {
            assert!(text.contains(valid), "names {valid}, got {text}");
        }
        assert!(
            store.list().expect("list works").is_empty(),
            "failed add writes nothing"
        );
    }

    /// Ticket 31: per-protocol templates carry the credential slot; a
    /// template with the server filled but the credential blank fails naming
    /// the credential (guided add, never raw JSON first).
    #[test]
    fn ticket31_protocol_templates_guide_and_reject_blank_credentials() {
        for (protocol, credential) in [
            ("vless", "uuid"),
            ("trojan", "password"),
            ("shadowsocks", "password"),
            ("wireguard", "private_key"),
            ("hysteria2", "password"),
            ("tuic", "uuid"),
            ("naive", "username"),
            ("ssh", "username"),
            ("noise", "key"),
        ] {
            let template = protocol_params_template(protocol);
            let value: serde_json::Value =
                serde_json::from_str(&template).expect("template is JSON");
            assert!(value.is_object(), "{protocol} template is an object");
            assert!(
                is_import_shaped_params(&template),
                "{protocol} template is import-shaped: {template}"
            );
            assert!(
                template.contains(credential),
                "{protocol} template names {credential}: {template}"
            );
        }
        // Server filled, credential blank → the named credential error.
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let params = protocol_params_template("vless")
            .replace("\"server\":\"\"", "\"server\":\"a.example.com\"");
        let error = store
            .add(NewConfig {
                name: "Half".to_string(),
                protocol: "vless".to_string(),
                params,
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect_err("blank credential must fail");
        let text = error.to_string();
        assert!(text.contains("uuid"), "names the credential, got {text}");
        // Unknown protocol falls back to a bare server/port object.
        let fallback = protocol_params_template("bogus");
        assert!(
            fallback.contains("server"),
            "fallback still guides, got {fallback}"
        );
    }

    /// Ticket 31: edit a subscription's label/url/group — persists in place,
    /// no double row, auto flag untouched.
    #[test]
    fn ticket31_update_subscription_persists_label_url_group() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = store
            .add_subscription("Nightly", "https://example.com/a.txt", "g1", true)
            .expect("seed works");
        let updated = store
            .update_subscription(&sub.id, "Daily", "https://example.com/b.txt", "g2")
            .expect("edit works");
        assert_eq!(updated.label, "Daily");
        assert_eq!(updated.url, "https://example.com/b.txt");
        assert_eq!(updated.group, "g2");
        assert!(updated.auto_refresh, "edit keeps the auto flag");
        let rows = store.list_subscriptions().expect("list works");
        assert_eq!(rows.len(), 1, "edit refreshes in place, no double row");
        assert_eq!(rows[0].label, "Daily");
        assert_eq!(rows[0].url, "https://example.com/b.txt");
        assert_eq!(rows[0].group, "g2");
    }

    /// Ticket 31: bad subscription edits fail typed with the record
    /// unchanged; unknown ids are NotFound. Errors never echo the URL.
    #[test]
    fn ticket31_update_subscription_rejects_bad_input_and_ghosts() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = store
            .add_subscription("Nightly", "https://example.com/a.txt", "g", true)
            .expect("seed works");
        for (label, url) in [
            ("", "https://example.com/b.txt"),
            ("   ", "https://example.com/b.txt"),
            ("Daily", "ftp://example.com/b.txt"),
            ("Daily", "http://127.0.0.1/b.txt"),
        ] {
            let error = store
                .update_subscription(&sub.id, label, url, "g2")
                .expect_err("bad edit must fail");
            assert!(
                matches!(error, StoreError::Invalid(_)),
                "typed, got {error}"
            );
            assert!(
                !error.to_string().contains("ftp://") && !error.to_string().contains("127.0.0.1"),
                "no URL in error, got {error}"
            );
        }
        let kept = store
            .get_subscription(&sub.id)
            .expect("get works")
            .expect("kept");
        assert_eq!(kept.label, "Nightly", "record unchanged");
        assert_eq!(kept.url, "https://example.com/a.txt");
        assert_eq!(kept.group, "g");
        assert!(
            matches!(
                store.update_subscription("9999", "x", "https://example.com/x.txt", "g"),
                Err(StoreError::NotFound(_))
            ),
            "ghost edit is NotFound"
        );
    }

    /// Ticket 31: duplicate-URL lookup finds the existing row (the friendly
    /// `already subscribed` path); other URLs find nothing.
    #[test]
    fn ticket31_find_subscription_by_url_matches_trimmed() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = store
            .add_subscription("Nightly", "https://example.com/a.txt", "g", true)
            .expect("seed works");
        let found = store
            .find_subscription_by_url("  https://example.com/a.txt  ")
            .expect("lookup works")
            .expect("dup found");
        assert_eq!(found.id, sub.id);
        assert_eq!(found.label, "Nightly");
        assert!(
            store
                .find_subscription_by_url("https://example.com/other.txt")
                .expect("lookup works")
                .is_none(),
            "other URL finds nothing"
        );
    }

    /// Ticket 31: no silent dedup on bind/port — two rows may share an
    /// endpoint at rest (the conflict error naming both parties stays an
    /// activation-time concern); saves are never blocked here.
    #[test]
    fn ticket31_shared_bind_port_saves_without_silent_dedup() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let first = store
            .add(NewConfig {
                name: "One".to_string(),
                protocol: "vless".to_string(),
                params: String::new(),
                group: String::new(),
                bind_ip: "127.0.0.1".to_string(),
                port: "12001".to_string(),
            })
            .expect("first saves");
        let second = store
            .add(NewConfig {
                name: "Two".to_string(),
                protocol: "vless".to_string(),
                params: String::new(),
                group: String::new(),
                bind_ip: "127.0.0.1".to_string(),
                port: "12001".to_string(),
            })
            .expect("second saves too — warning, never a block");
        assert_ne!(first.id, second.id, "two rows, no silent dedup");
        assert_eq!(store.list().expect("list works").len(), 2);
    }

    // -- Ticket 37: routing presets --------------------------------------

    fn t37_seed_row(store: &mut Store, name: &str) -> ProxyConfig {
        store
            .add(NewConfig {
                name: name.to_string(),
                protocol: "vless".to_string(),
                params: String::new(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("seed row saves")
    }

    fn t37_rules_json() -> Vec<serde_json::Value> {
        serde_json::from_str::<Vec<serde_json::Value>>(
            r#"[{"action":"route","domain":["example.com"]},{"action":"direct","ip_cidr":["10.0.0.0/8"]},{"action":"block","port":[25]}]"#,
        )
        .expect("fixture rules parse")
    }

    /// Ticket 37: `throne://route` link imports a named preset with its rules
    /// (replaces the v1 named error).
    #[test]
    fn ticket37_route_link_imports_named_preset() {
        use base64::Engine as _;
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let payload = base64::engine::general_purpose::STANDARD.encode(
            r#"{"name":"ads","rules":[{"action":"block","domain_suffix":["ads.example"]}]}"#,
        );
        let preset = store
            .import_route_link(&format!("throne://route/{payload}"))
            .expect("route link imports");
        assert_eq!(preset.name, "ads");
        assert_eq!(preset.rules().expect("rules decode").len(), 1);
        let listed = store.list_presets().expect("list works");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "ads");
    }

    /// Ticket 37: preset URL/JSON bodies import; malformed rules are rejected
    /// naming the rule index and the field (never values).
    #[test]
    fn ticket37_preset_json_import_and_malformed_rule_error() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let preset = store
            .import_preset_json(
                r#"{"name":"office","rules":[{"action":"direct"}]}"#,
                "file",
                Some("office.json"),
            )
            .expect("valid JSON imports");
        assert_eq!(preset.name, "office");
        assert_eq!(preset.source, "file");
        assert_eq!(preset.source_url.as_deref(), Some("office.json"));
        let error = store
            .import_preset_json(
                r#"{"name":"bad","rules":[{"action":"route"},{"nope":true}]}"#,
                "file",
                None,
            )
            .expect_err("malformed rule must fail");
        let text = error.to_string();
        assert!(text.contains("rule 1"), "names rule index, got {text}");
        assert!(text.contains("\"action\""), "names field, got {text}");
        assert!(
            !text.contains("nope"),
            "never echoes rule content, got {text}"
        );
        assert_eq!(store.list_presets().expect("list works").len(), 1);
    }

    /// Ticket 37: preset manager — create / add rule / remove rule / detach
    /// persist and validate; delete of an attached preset is refused.
    #[test]
    fn ticket37_preset_manager_crud_and_refused_delete() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let row = t37_seed_row(&mut store, "Row");
        assert_eq!(row.preset_id, None, "rows start unattached");
        let preset = store
            .create_preset("empty", "manual", None, &[])
            .expect("empty create works");
        assert_eq!(preset.rules().expect("rules decode").len(), 0);
        store
            .add_preset_rule(&preset.id, serde_json::json!({"action": "direct"}))
            .expect("add rule works");
        let back = store
            .get_preset(&preset.id)
            .expect("get works")
            .expect("preset kept");
        assert_eq!(back.rules().expect("rules decode").len(), 1);
        let bad = store.add_preset_rule(&preset.id, serde_json::json!({"nope": true}));
        assert!(bad.is_err(), "bad rule rejected");
        store
            .remove_preset_rule(&preset.id, 0)
            .expect("remove rule works");
        let back = store
            .get_preset(&preset.id)
            .expect("get works")
            .expect("preset kept");
        assert_eq!(back.rules().expect("rules decode").len(), 0);
        // Attach, then delete must refuse.
        store
            .set_config_preset(&row.id, Some(&preset.id))
            .expect("attach works");
        let attached = store.get(&row.id).expect("get works").expect("row kept");
        assert_eq!(attached.preset_id.as_deref(), Some(preset.id.as_str()));
        let refused = store
            .delete_preset(&preset.id)
            .expect_err("attached delete refused");
        assert!(
            refused.to_string().contains(&row.id) || refused.to_string().contains("attached"),
            "refusal names the attachment, got {refused}"
        );
        assert!(
            store.get_preset(&preset.id).expect("get works").is_some(),
            "refused delete keeps the preset"
        );
        // Explicit detach, then delete succeeds.
        store
            .set_config_preset(&row.id, None)
            .expect("detach works");
        let detached = store.get(&row.id).expect("get works").expect("row kept");
        assert_eq!(detached.preset_id, None);
        assert!(store.delete_preset(&preset.id).expect("delete works"));
        assert!(
            store.get_preset(&preset.id).expect("get works").is_none(),
            "preset gone"
        );
    }

    /// Ticket 37: explicit delete-detaches-then-deletes in one call.
    #[test]
    fn ticket37_delete_detaching_clears_attachments() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let row = t37_seed_row(&mut store, "Row");
        let preset = store
            .create_preset("p", "manual", None, &t37_rules_json())
            .expect("create works");
        store
            .set_config_preset(&row.id, Some(&preset.id))
            .expect("attach works");
        assert!(store
            .delete_preset_detaching(&preset.id)
            .expect("explicit delete works"));
        let back = store.get(&row.id).expect("get works").expect("row kept");
        assert_eq!(back.preset_id, None, "attachment cleared");
        assert!(
            store.get_preset(&preset.id).expect("get works").is_none(),
            "preset gone"
        );
    }

    /// Ticket 37: attaching a ghost preset is NotFound; attaching to a ghost
    /// row is NotFound; removing a ghost rule index fails typed.
    #[test]
    fn ticket37_preset_ghost_references_fail_typed() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let row = t37_seed_row(&mut store, "Row");
        assert!(
            matches!(
                store.set_config_preset(&row.id, Some("9999")),
                Err(StoreError::NotFound(_))
            ),
            "ghost preset attach is NotFound"
        );
        assert!(
            matches!(
                store.set_config_preset("9999", None),
                Err(StoreError::NotFound(_))
            ),
            "ghost row attach is NotFound"
        );
        let preset = store
            .create_preset("p", "manual", None, &[])
            .expect("create works");
        assert!(
            store.remove_preset_rule(&preset.id, 3).is_err(),
            "ghost rule index fails"
        );
    }

    /// Ticket 37: fresh rows carry no preset and survive a restart-shaped
    /// reopen with their attachment intact (isolated temp DB, synthetic row).
    #[test]
    fn ticket37_preset_attachment_survives_reopen() {
        let dir = std::env::temp_dir().join(format!(
            "manyproxy-t37-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock works")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir works");
        let path = dir.join("library.db");
        let (row_id, preset_id) = {
            let mut store = Store::open_file(&path).expect("file store opens");
            let row = t37_seed_row(&mut store, "Row");
            let preset = store
                .create_preset("p", "manual", None, &t37_rules_json())
                .expect("create works");
            store
                .set_config_preset(&row.id, Some(&preset.id))
                .expect("attach works");
            (row.id, preset.id)
        };
        {
            let store = Store::open_file(&path).expect("reopen works");
            let back = store.get(&row_id).expect("get works").expect("row kept");
            assert_eq!(back.preset_id.as_deref(), Some(preset_id.as_str()));
            let preset = store
                .get_preset(&preset_id)
                .expect("get works")
                .expect("preset kept");
            assert_eq!(preset.rules().expect("rules decode").len(), 3);
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Ticket 37: restore rebuilds rows unattached while the presets table
    /// itself survives (backups cover endpoints, not routing policy). The
    /// preset stays listed for re-attach — never a dangling reference.
    #[test]
    fn ticket37_restore_rebuilds_rows_unattached_presets_survive() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let row = t37_seed_row(&mut store, "Row");
        let preset = store
            .create_preset("p", "manual", None, &t37_rules_json())
            .expect("create works");
        store
            .set_config_preset(&row.id, Some(&preset.id))
            .expect("attach works");
        let payload = store.export_library_json().expect("export works");
        // Restore runs against a wiped library in the same database (the
        // production path wipes rows + subscriptions, never route_presets).
        for row in store.list().expect("list works") {
            store.remove(&row.id).expect("wipe works");
        }
        let outcome = store.restore_library_json(&payload).expect("restore works");
        assert_eq!(outcome.configs, 1);
        let back = store.get(&row.id).expect("get works").expect("row kept");
        assert_eq!(back.preset_id, None, "restored rows rebuild unattached");
        let kept = store
            .get_preset(&preset.id)
            .expect("get works")
            .expect("preset survives restore");
        assert_eq!(kept.rules().expect("rules decode").len(), 3);
    }
}
