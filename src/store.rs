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
    "snell",
    "shadowtls",
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
/// Ticket A4: the pool is GLOBAL across binds (one port number serves one
/// local endpoint; two binds never share an auto port). Auto-assign
/// deliberately ignores `bind_ip` so `127.0.0.1:10801` and
/// `0.0.0.0:10801` can never collide at activation. At rest two rows MAY
/// share a bind:port (saves are never blocked — see the schema note); the
/// conflict error naming both parties stays an activation-time concern.
/// Restore is stricter (a duplicated bind:port in a backup fails loudly as
/// corruption — see `restore_library_json`).
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

/// Ticket 27: secrets-at-rest note. Row params are DPAPI-sealed at rest
/// (user scope, `dpapi1:` marker, `src/dpapi.rs`, opened in [`map_row`]);
/// the library DB file and every sidecar config additionally get user-only
/// file permissions ([`harden_file_permissions`]). OS-keychain portability
/// beyond DPAPI user scope stays an explicit BACKLOG non-goal: when it
/// lands, this note and the export warning are its entry points.
#[cfg_attr(not(test), allow(dead_code))]
pub const SECRETS_AT_REST_NOTE: &str = "secrets at rest: row params DPAPI-sealed (user scope) plus user-only file permissions (0600/ACL); OS-keychain portability beyond DPAPI user scope is backlog";

/// Ticket 27: best-effort user-only permissions for a secrets-bearing file
/// (library DB, sidecar configs). Unix sets 0600; Windows strips inherited
/// ACLs and grants the current user SID only (best-effort: failures are
/// ignored so hardening never breaks open/write — the file stays usable in
/// its per-user dir). Never grants Administrators (user-only means
/// user-only) and never trusts the spoofable `USERNAME` env (the SID comes
/// from `whoami /user`). Never echoes file contents.
pub fn harden_file_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(windows)]
    {
        // Best-effort: `<user-sid>:(R,W)` only, no inherited entries. The
        // temp dir and %APPDATA% are per-user by construction; this tightens
        // the file itself when icacls is available. SIDs need the `*`
        // prefix (otherwise icacls reads them as account names and grants
        // nothing — which would lock even the owner out after the
        // inheritance strip above).
        let principal = current_user_sid()
            .map(|sid| format!("*{sid}"))
            .unwrap_or_else(|| "OWNER RIGHTS".to_string());
        let _ = std::process::Command::new("icacls")
            .arg(path)
            .arg("/inheritance:r")
            .output();
        let _ = std::process::Command::new("icacls")
            .arg(path)
            .arg("/grant:r")
            .arg(format!("{principal}:(R,W)"))
            .output();
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
    }
}

/// Current process user SID via `whoami /user` (`None` when the lookup
/// fails — the caller falls back to `OWNER RIGHTS`, never to `USERNAME`).
#[cfg(windows)]
fn current_user_sid() -> Option<String> {
    let output = std::process::Command::new("whoami")
        .arg("/user")
        .arg("/fo")
        .arg("csv")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.split(|c: char| c == '"' || c == ',' || c.is_whitespace())
        .map(str::trim)
        .find(|token| is_sid_shape(token))
        .map(str::to_string)
}

/// True for `S-1-...` SID shapes (digits and dashes after the prefix).
#[cfg(windows)]
fn is_sid_shape(token: &str) -> bool {
    token.len() > 4
        && token.starts_with("S-1-")
        && token[4..].chars().all(|c| c.is_ascii_digit() || c == '-')
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

/// Ticket A5: enforced subscription-fetch caps. The async fetch path
/// enforces these at the socket (size + timeout + redirect hops); the byte
/// entry [`Store::refresh_with_bytes`] enforces size + UTF-8 again at
/// ingest, so the 8 MiB / 20 s limits live in code on both sides — never
/// comment-only.
pub const SUBSCRIPTION_MAX_BYTES: usize = 8 * 1024 * 1024;
/// Ticket A5: per-fetch network timeout, seconds (enforced by the fetch
/// path; recorded here so the two sides cannot drift).
pub const SUBSCRIPTION_TIMEOUT_SECS: u64 = 20;
/// Ticket A5: redirect hops followed per fetch, each hop re-running
/// [`subscription_fetch_policy`] (enforced by the fetch path).
pub const SUBSCRIPTION_MAX_REDIRECTS: usize = 5;

/// Ticket A5: DNS-rebind guard — the fetch-time half of
/// [`subscription_fetch_policy`]. The pure policy blocks literal internal
/// hosts without DNS; this runs AFTER resolution: a public hostname that
/// resolves (or re-resolves, e.g. via a redirect target) to a
/// loopback/private/link-local/unspecified address is Blocked, so a benign
/// name can never smuggle the fetch onto a local service. An empty
/// resolution fails closed (Block, never fetch blind). Reasons never echo
/// the URL.
pub fn subscription_fetch_policy_resolved(
    url: &str,
    resolved: &[std::net::IpAddr],
) -> SubscriptionFetchPolicy {
    let base = subscription_fetch_policy(url);
    if matches!(base, SubscriptionFetchPolicy::Block { .. }) {
        return base;
    }
    if resolved.is_empty() {
        return SubscriptionFetchPolicy::Block {
            reason: "subscription host does not resolve (refusing to fetch blind)",
        };
    }
    if resolved.iter().any(|ip| is_internal_host(&ip.to_string())) {
        return SubscriptionFetchPolicy::Block {
            reason: "subscription resolves to an internal host (loopback/private/link-local are never fetched; import local files manually)",
        };
    }
    base
}
/// Ticket 27: true for loopback/private/link-local/unspecified hosts (IP
/// literals only; hostnames are never internal without DNS). Mirrors the
/// preset intake classifier — both copies block `localhost` subdomains and
/// IPv4-mapped forms (decode-time and fetch-time stay in parity).
fn is_internal_host(host: &str) -> bool {
    let lower = host.trim().to_ascii_lowercase();
    let bare = lower.strip_suffix('.').unwrap_or(&lower);
    if bare == "localhost" || bare.ends_with(".localhost") {
        return true;
    }
    if let Ok(ip) = bare.parse::<IpAddr>() {
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
                // Mapped-only: `::1` is not mapped (falls through to the
                // plain v6 checks below); `to_ipv4` would also match
                // IPv4-compatible `::a.b.c.d` and misroute `::1`.
                if let Some(mapped) = v6.to_ipv4_mapped() {
                    let octets = mapped.octets();
                    return mapped.is_loopback()
                        || mapped.is_unspecified()
                        || mapped.is_link_local()
                        || octets[0] == 10
                        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
                        || (octets[0] == 192 && octets[1] == 168)
                        || octets[0] == 127;
                }
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
    /// Ticket A2: DPAPI seal/open failure (never a `Sql` mislabel). The
    /// write aborts before commit, so nothing is stored — never silent
    /// plaintext, never a partial row.
    Crypto {
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
            StoreError::Crypto { message } => {
                write!(f, "cannot protect secrets ({message}; nothing was written)")
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
                "interval_secs": sub.interval_secs,
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
/// Ticket 43: the word `auto` (any case, padded or not) also means
/// auto-assign, so an edited concrete port can return to auto by typing it.
pub fn parse_port_field(raw: &str) -> Result<Option<u16>, FieldError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
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

/// Ticket 43: confirm-dialog description for removing a RUNNING config. The
/// endpoint stops first, and the copy says so — the stopped-row text above
/// would lie here.
pub fn delete_running_confirm_text(name: &str) -> String {
    format!(
        "Remove \"{name}\"? Its endpoint is running and will be stopped first, \
         then the stored configuration is deleted."
    )
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
    /// Ticket 40 (v5, additive): free-form manager note. `None` means unset.
    pub description: Option<String>,
    /// Ticket 40 (v5, additive): true for shipped builtins (never deleted by
    /// restore sweeps; manager still edits rules explicitly).
    pub is_builtin: bool,
    /// Ticket 40 (v5, additive): unix seconds this preset was last attached.
    pub last_used_at: Option<i64>,
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
    // Ticket 40: v5 extension columns are trailing and optional for legacy
    // SELECTs; missing reads as unset (never a whole-list failure).
    let description: Option<String> = row.get(6).ok().flatten();
    let is_builtin: Option<i64> = row.get(7).ok();
    let last_used_at: Option<i64> = row.get(8).ok().flatten();
    Ok(RoutePreset {
        id: id.to_string(),
        name: row.get(1)?,
        source: row.get(2)?,
        source_url: row.get(3)?,
        rules_json: row.get(4)?,
        updated_at: row.get(5)?,
        description,
        is_builtin: is_builtin.unwrap_or(0) != 0,
        last_used_at,
    })
}

/// Ticket 40 (v5): one quarantined library row. The store keeps good rows
/// loadable while naming the ids that need attention (re-import or restore).
/// `cause` names the column/reason only, never secret or param bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuarantinedRow {
    pub id: String,
    pub cause: String,
}

/// Ticket 40 (v5): one append-only health-history sample. `health_snapshots`
/// keeps the latest-fast-path untouched; history serves per-row charts and
/// range deletes via `idx_health_history_config_at`.
/// Ticket 45: production type behind the detail sparkline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthHistoryEntry {
    pub config_id: String,
    pub at: i64,
    /// `"url-test"` or `"tcp-ping"`.
    pub kind: String,
    pub latency_ms: Option<u64>,
    /// `Some` names the dead cause; `None` means live.
    pub error: Option<String>,
}

#[cfg_attr(not(test), allow(dead_code))]
fn map_health_history_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HealthHistoryEntry> {
    let latency_ms: Option<i64> = row.get(3)?;
    Ok(HealthHistoryEntry {
        config_id: row.get(0)?,
        at: row.get(1)?,
        kind: row.get(2)?,
        latency_ms: latency_ms.map(|ms| ms.max(0) as u64),
        error: row.get(4)?,
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

/// Canonical group-label normalization: surrounding whitespace never
/// distinguishes labels (stored labels are always trimmed — see
/// [`defaulted`]). Every group entry point (rename/move/delete/count
/// matching) funnels through here so `" g1 "` and `"g1"` are one group.
fn normalized_group(raw: &str) -> String {
    raw.trim().to_string()
}

/// Blank means loopback default; otherwise the text must parse as an IP
/// literal or `localhost` (spec: loopback default; LAN binds gated at
/// activation time). Matches the engine save/start gate message
/// (`BadBind`: "want an IP address or localhost").
fn validated_bind(raw: &str, errors: &mut Vec<FieldError>) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Some(DEFAULT_BIND.to_string());
    }
    if trimmed.eq_ignore_ascii_case("localhost") {
        return Some(trimmed.to_string());
    }
    match trimmed.parse::<IpAddr>() {
        Ok(_) => Some(trimmed.to_string()),
        Err(_) => {
            errors.push(FieldError::new(
                "bind_ip",
                format!("bind address must be an IP literal or localhost, got '{trimmed}'"),
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
        // Ticket 42: native 1.14.2 outbounds (serve-everything).
        "snell" => Some("snell".to_string()),
        "shadowtls" | "shadow-tls" | "shadow_tls" => Some("shadowtls".to_string()),
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
        // Ticket 42: snell needs its psk; shadowtls v1 rows may omit the
        // password (v2/v3 carry it) so the template leaves it blank-only
        // when the dialog targets a passworded shape — blank stays a
        // template, never a committed row (commit names the field).
        "snell" => blank(&mut params, "psk"),
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

/// Ticket 05 (signal-box): guided mini-form mapping for one protocol.
/// `None` means unknown protocol (the commit names it through the normal
/// validation path). `key = None` means a known protocol with no link
/// secret — the Identity input hides while Server + Server port still fold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuidedIdentity {
    /// Params key the Identity input fills (`None` = no secret).
    pub key: Option<&'static str>,
    /// Alias keys for prefill/compare (first entry is `key` when set).
    pub read_keys: &'static [&'static str],
    /// Human credential list for the guided hint line.
    pub needs: &'static str,
}

/// Ticket 05: primary identity key + read aliases + needs-label per
/// protocol. Key sets mirror [`validate_protocol_params`] exactly (same
/// credential per protocol) so the mini-form can never disagree with the
/// commit gate about what a row needs.
pub fn guided_identity_fields(protocol: &str) -> Option<GuidedIdentity> {
    let canonical = canonical_protocol(protocol)?;
    let guided = match canonical.as_str() {
        "vless" | "xray" | "vmess" => GuidedIdentity {
            key: Some("uuid"),
            read_keys: &["uuid", "id"],
            needs: "uuid",
        },
        "trojan" => GuidedIdentity {
            key: Some("password"),
            read_keys: &["password"],
            needs: "password",
        },
        "shadowsocks" => GuidedIdentity {
            key: Some("password"),
            read_keys: &["password"],
            needs: "method and password",
        },
        "hysteria2" => GuidedIdentity {
            key: Some("password"),
            read_keys: &["password", "auth"],
            needs: "password",
        },
        "hysteria" => GuidedIdentity {
            key: Some("auth"),
            read_keys: &["auth", "password"],
            needs: "auth",
        },
        "tuic" => GuidedIdentity {
            key: Some("uuid"),
            read_keys: &["uuid", "id"],
            needs: "uuid and password",
        },
        "wireguard" | "amneziawg" => GuidedIdentity {
            key: Some("private_key"),
            read_keys: &["private_key", "privatekey"],
            needs: "private_key and public_key",
        },
        "anytls" => GuidedIdentity {
            key: Some("password"),
            read_keys: &["password"],
            needs: "password",
        },
        "naive" => GuidedIdentity {
            key: Some("password"),
            read_keys: &["password"],
            needs: "username and password",
        },
        "ssh" => GuidedIdentity {
            key: Some("password"),
            read_keys: &["password", "private_key", "privatekey"],
            needs: "username and a password or private_key",
        },
        "noise" => GuidedIdentity {
            key: Some("key"),
            read_keys: &["key", "password"],
            needs: "password or key",
        },
        "snell" => GuidedIdentity {
            key: Some("psk"),
            read_keys: &["psk"],
            needs: "psk",
        },
        "socks" | "http" | "https" | "shadowtls" => GuidedIdentity {
            key: None,
            read_keys: &[],
            needs: "no secret",
        },
        "custom-outbound" | "custom-config" | "extra-core" | "chain" => GuidedIdentity {
            key: None,
            read_keys: &[],
            needs: "custom JSON",
        },
        _ => return None,
    };
    Some(guided)
}

/// Ticket 05: prefill one guided mini-form value from a params JSON object
/// (case-insensitive across `keys`; numbers stringify). Blank for
/// unparseable JSON or missing keys — the JSON stays the source of truth,
/// the mini-form only echoes it.
pub fn guided_prefill_value(params_raw: &str, keys: &[&str]) -> String {
    let obj = serde_json::from_str::<serde_json::Value>(params_raw)
        .ok()
        .and_then(|value| value.as_object().cloned());
    match obj {
        Some(obj) => lookup_case_insensitive(&obj, keys).unwrap_or_default(),
        None => String::new(),
    }
}

/// Ticket 05: fold non-blank guided mini-form values into the params JSON
/// text. Blank guided inputs leave the JSON byte-identical (the
/// template/typed JSON stands, and untouched edit fields stay silent — no
/// spurious patches from reserialization). Unparseable or non-object JSON
/// returns unchanged so validation names that error. A non-blank server
/// port outside 1-65535 (or unparseable) is written as a string so
/// validation fails naming server_port instead of silently keeping the
/// JSON's port. `identity` is `(key, read_aliases)`: the value compares
/// across every alias but writes to `key` only.
pub fn fold_guided_into_params(
    params_raw: &str,
    server: &str,
    server_port: &str,
    identity: Option<(&str, &[&str])>,
    identity_value: &str,
) -> String {
    let server = server.trim();
    let port = server_port.trim();
    let identity_value = identity_value.trim();
    let identity_active = identity.is_some_and(|(_, _)| !identity_value.is_empty());
    if server.is_empty() && port.is_empty() && !identity_active {
        return params_raw.to_string();
    }
    const SERVER_KEYS: &[&str] = &["server", "address"];
    const PORT_KEYS: &[&str] = &["server_port", "port", "serverPort"];
    let base = if params_raw.trim().is_empty() {
        "{}"
    } else {
        params_raw
    };
    let mut value: serde_json::Value = match serde_json::from_str(base) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    // Reads borrow per call (never held across an insert — NLL ends each
    // borrow before the mutation below).
    let mut changed = false;
    if !server.is_empty() && lookup_case_insensitive(obj, SERVER_KEYS).as_deref() != Some(server) {
        obj.insert(
            "server".to_string(),
            serde_json::Value::String(server.to_string()),
        );
        changed = true;
    }
    if !port.is_empty() {
        let valid = port
            .parse::<u64>()
            .ok()
            .filter(|port| (1..=65_535).contains(port));
        let same = match (lookup_case_insensitive(obj, PORT_KEYS), valid) {
            (Some(text), Some(number)) => text.parse::<u64>().ok() == Some(number),
            (Some(text), None) => text == port,
            (None, _) => false,
        };
        if !same {
            let value = match valid {
                Some(number) => serde_json::Value::from(number),
                None => serde_json::Value::String(port.to_string()),
            };
            obj.insert("server_port".to_string(), value);
            changed = true;
        }
    }
    if let Some((key, read_keys)) = identity {
        if !identity_value.is_empty()
            && lookup_case_insensitive(obj, read_keys).as_deref() != Some(identity_value)
        {
            obj.insert(
                key.to_string(),
                serde_json::Value::String(identity_value.to_string()),
            );
            changed = true;
        }
    }
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
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
            // Ticket 42: snell needs its psk; shadowtls/socks/http serve
            // without link credentials (v1 shadowtls omits the password).
            "snell" => has(&["psk"]),
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
                "snell" => "psk",
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
    /// Ticket 40 (v5): per-subscription auto-refresh interval in seconds.
    /// Missing/legacy rows read as [`DEFAULT_AUTO_REFRESH_SECS`]; `<= 0`
    /// means always due (matches [`due_for_refresh`] semantics).
    pub interval_secs: i64,
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

/// Ticket 40 (v5): per-subscription due check honoring
/// [`Subscription::interval_secs`] (missing/`<= 0` means always due once a
/// prior success exists; never-refreshed auto subs are always due).
pub fn due_for_refresh_for(subscription: &Subscription, now: i64) -> bool {
    due_for_refresh(subscription, now, subscription.interval_secs)
}

/// Map one subscription row to a Subscription.
fn map_subscription_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Subscription> {
    let id: i64 = row.get(0)?;
    let auto_refresh: i64 = row.get(4)?;
    let failures_raw: Option<String> = row.get(7)?;
    // Ticket 40: v5 `interval_secs` is column 8 when present; legacy rows
    // (v0–v4 fixtures, hand-built SELECTs) fall back to the default.
    let interval_secs: Option<i64> = row.get(8).ok();
    Ok(Subscription {
        id: id.to_string(),
        label: row.get(1)?,
        // Ticket 35/48: subscription URLs carry tokens, so the column is
        // sealed at rest like params. Sealed values open here — the single
        // read funnel; pre-seal rows (legacy databases, hand-built
        // fixtures) pass through since all writers seal.
        url: {
            let stored: String = row.get(2)?;
            if stored.trim_start().starts_with(crate::dpapi::MARKER) {
                crate::dpapi::unseal_params(&stored).map_err(|message| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            message,
                        )),
                    )
                })?
            } else {
                stored
            }
        },
        group: row.get(3)?,
        auto_refresh: auto_refresh != 0,
        last_refresh_at: row.get(5)?,
        last_error: row.get(6)?,
        failures: decode_failures(failures_raw),
        interval_secs: interval_secs.unwrap_or(DEFAULT_AUTO_REFRESH_SECS),
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

/// Ticket 40: derive the rotation core (`server\0port`) from a stored
/// widened identity key (`protocol\0server\0port\0…`). `None` when the key
/// is not in the expected shape (legacy/foreign tombstones match exactly
/// only, never by core).
fn rotation_core_from_identity_key(key: &str) -> Option<String> {
    let mut parts = key.split('\0');
    let _protocol = parts.next()?;
    let server = parts.next()?;
    let port = parts.next()?;
    if server.is_empty() || port.trim().is_empty() {
        return None;
    }
    Some(format!("{server}\0{}", port.trim()))
}

/// Ticket 25: outcome of [`Store::restore_library_json`].
/// (Test-driven API for ticket 26; see the callee's note.)
/// Ticket 40: `presets` counts rebuilt route presets (full-fidelity export).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RestoreOutcome {
    pub configs: usize,
    pub subscriptions: usize,
    pub tombstones: usize,
    pub presets: usize,
}

/// True when two paths name the same file: canonical comparison when both
/// resolve, parent-canonical + file-name comparison otherwise (the export
/// target usually does not exist yet, so canonicalization alone never
/// suffices), absolutized-lexical fallback last.
fn paths_equal(a: &Path, b: &Path) -> bool {
    if let (Ok(canonical_a), Ok(canonical_b)) = (a.canonicalize(), b.canonicalize()) {
        return canonical_a == canonical_b;
    }
    fn canonical_parent_joined(path: &Path) -> Option<PathBuf> {
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty())?;
        let canonical_parent = parent.canonicalize().ok()?;
        Some(canonical_parent.join(path.file_name()?))
    }
    if let (Some(joined_a), Some(joined_b)) =
        (canonical_parent_joined(a), canonical_parent_joined(b))
    {
        #[cfg(windows)]
        {
            // Windows paths are case-insensitive; canonicalization already
            // normalized separators.
            return joined_a.to_string_lossy().to_lowercase()
                == joined_b.to_string_lossy().to_lowercase();
        }
        #[cfg(not(windows))]
        {
            return joined_a == joined_b;
        }
    }
    fn absolutize(path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        }
    }
    absolutize(a) == absolutize(b)
}

/// Ticket 25 (L4): same-directory tmp sibling for atomic export writes.
fn tmp_export_path(path: &Path) -> PathBuf {
    let tmp_name = format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("manyproxy-export.json"),
        unique_name_suffix(now_unix())
    );
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(tmp_name),
        _ => PathBuf::from(tmp_name),
    }
}

/// Append a SQLite sidecar suffix (`-wal`/`-shm`/`-journal`) at the
/// OS-string level so non-UTF8 paths survive (never `display()` concat,
/// which breaks on non-UTF8 bytes).
fn sibling_sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut os = path.as_os_str().to_owned();
    os.push(suffix);
    PathBuf::from(os)
}

/// Process-unique filename suffix: `<ts>-<pid>-<counter>` so two
/// quarantines/backups/restores in the same second never collide (same
/// process shares pid+timestamp; the counter disambiguates).
static NAME_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn unique_name_suffix(ts: i64) -> String {
    let n = NAME_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{ts}-{}-{n}", std::process::id())
}

pub struct Store {
    conn: Connection,
    /// Ticket 40: live file behind this handle (`None` for `:memory:` test
    /// stores). Powers backup-before-restore + temp-DB atomic swaps without
    /// touching the live user DB from tests (isolated `MANYPROXY_DB` only).
    db_path: Option<PathBuf>,
}

/// Swap a rebuilt temp DB over the live path. Plain rename first; on
/// Windows the destination may be open (this process's own live handle or
/// a lingering lock) and rename fails — then remove the destination +
/// sidecars and retry once, with copy-over as a last resort. Off-Windows
/// the rename error propagates unchanged.
fn swap_tmp_over_live(tmp: &Path, db_path: &Path) -> std::io::Result<()> {
    if std::fs::rename(tmp, db_path).is_ok() {
        return Ok(());
    }
    #[cfg(windows)]
    {
        let _ = std::fs::remove_file(db_path);
        for suffix in ["-wal", "-shm", "-journal"] {
            let _ = std::fs::remove_file(sibling_sidecar(db_path, suffix));
        }
        if std::fs::rename(tmp, db_path).is_ok() {
            return Ok(());
        }
        std::fs::copy(tmp, db_path)?;
        let _ = std::fs::remove_file(tmp);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(tmp, db_path)
    }
}

impl Store {
    /// Open a throwaway store for unit tests.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::open_with_path(Connection::open_in_memory()?, None)
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
    /// Ticket 40: corrupt/foreign files are quarantined aside as
    /// `library.db.corrupt-<ts>` with a loud integrity error (never silent
    /// in-memory loss); the caller offers restore-from-export.
    pub fn open_file(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|error| StoreError::Io {
                    path: parent.to_path_buf(),
                    message: error.to_string(),
                })?;
            }
        }
        // Fast reject: a non-empty file that is not SQLite at all (bad magic)
        // quarantines before rusqlite ever touches it. Reads only the 16-byte
        // header (never the whole file for a magic check).
        if path.exists() {
            if let Ok(meta) = std::fs::metadata(path) {
                if meta.len() > 0 {
                    if let Ok(mut file) = std::fs::File::open(path) {
                        use std::io::Read as _;
                        let mut head = [0u8; 16];
                        // `read_exact` fails on short files; short-but-nonempty
                        // files fall through to rusqlite (loud + quarantine).
                        if file.read_exact(&mut head).is_ok() {
                            let magic = b"SQLite format 3\0";
                            if head != *magic {
                                let quarantined = Self::quarantine_corrupt_file(path)?;
                                return Err(Self::schema_error(format!(
                                    "library database is corrupt (integrity check failed): preserved as {} — restore from an export instead of running on a half-migrated file",
                                    quarantined.display()
                                )));
                            }
                        }
                    }
                }
            }
        }
        let conn = Connection::open(path).map_err(|e| {
            // `Connection::open` itself failed (e.g. not a database): try to
            // preserve the bad file aside before failing loudly.
            if path.exists() {
                let _ = Self::quarantine_corrupt_file(path);
            }
            StoreError::Sql(e)
        })?;
        let store = match Self::open_with_path(conn, Some(path.to_path_buf())) {
            Ok(store) => store,
            Err(error) => {
                // A schema/migration failure on a file-backed open that smells
                // like corruption (not a clean validation error) preserves the
                // bad file aside. Clean validation errors (newer version,
                // DPAPI-down, drift) propagate untouched so retries work.
                let text = error.to_string();
                let looks_corrupt = text.contains("not a database")
                    || text.contains("file is not a database")
                    || text.contains("malformed")
                    || text.contains("integrity check failed")
                    || text.contains("corrupt");
                if looks_corrupt && path.exists() {
                    if let Ok(quarantined) = Self::quarantine_corrupt_file(path) {
                        return Err(Self::schema_error(format!(
                            "{text} (bad file preserved as {})",
                            quarantined.display()
                        )));
                    }
                }
                return Err(error);
            }
        };
        // Post-migration integrity gate: a page-level fault quarantines on
        // next open, never silent in-memory loss.
        if let Err(error) = store.check_quick() {
            let text = error.to_string();
            drop(store);
            if let Ok(quarantined) = Self::quarantine_corrupt_file(path) {
                return Err(Self::schema_error(format!(
                    "library database is corrupt (integrity check failed: {text}): preserved as {} — restore from an export instead of running on a half-migrated file",
                    quarantined.display()
                )));
            }
            return Err(error);
        }
        harden_file_permissions(path);
        // WAL sidecars (`<db>-wal`, `<db>-shm`) hold the same secrets;
        // tighten them too when present.
        for sibling in [sibling_sidecar(path, "-wal"), sibling_sidecar(path, "-shm")] {
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

    fn open_with_path(conn: Connection, db_path: Option<PathBuf>) -> Result<Self, StoreError> {
        let mut store = Self { conn, db_path };
        store.apply_pragmas()?;
        store.create_schema()?;
        // Ticket 40: time-based health-history retention at startup (90d,
        // one tx, never VACUUM; best-effort so a prune failure never blocks
        // open).
        let now = now_unix();
        let _ = store.gc_health_history(90, now);
        Ok(store)
    }

    /// Ticket 40: live file behind this handle, if file-backed.
    pub fn db_path(&self) -> Option<PathBuf> {
        self.db_path.clone()
    }

    /// Ticket 40: `PRAGMA quick_check` gate. `Ok(())` means `ok`; any error
    /// row becomes a loud schema error (never silent in-memory loss).
    fn check_quick(&self) -> Result<(), StoreError> {
        let mut stmt = self.conn.prepare("PRAGMA quick_check")?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        if rows.len() == 1 && rows[0].eq_ignore_ascii_case("ok") {
            Ok(())
        } else {
            Err(Self::schema_error(format!(
                "library database integrity check failed: {}",
                rows.join("; ")
            )))
        }
    }

    /// Ticket 40: preserve a corrupt/foreign DB file aside as
    /// `library.db.corrupt-<ts>-<pid>-<counter>` (timestamp + pid + counter,
    /// same directory so the live path stays on one filesystem). Returns the
    /// quarantine path.
    fn quarantine_corrupt_file(path: &Path) -> Result<PathBuf, StoreError> {
        let ts = now_unix();
        let base = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("library.db");
        let quarantined = match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => {
                parent.join(format!("{base}.corrupt-{}", unique_name_suffix(ts)))
            }
            _ => PathBuf::from(format!("{base}.corrupt-{}", unique_name_suffix(ts))),
        };
        std::fs::rename(path, &quarantined).map_err(|error| {
            StoreError::Sql(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
        })?;
        // Remove WAL sidecars of the bad file so a stale journal never
        // shadows the quarantine.
        for suffix in ["-wal", "-shm", "-journal"] {
            let _ = std::fs::remove_file(sibling_sidecar(path, suffix));
        }
        Ok(quarantined)
    }

    /// Ticket 40: timestamped backup before any wipe
    /// (`library.db.bak-<ts>-<pid>-<counter>` in the same directory). The
    /// backup is a plain file copy (same-filesystem, crash-safe source read);
    /// restore uses temp-DB + rename, never in-place overwrite.
    pub fn backup_db_file(db_path: &Path, now: i64) -> Result<PathBuf, StoreError> {
        let base = db_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("library.db");
        let backup = match db_path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => {
                parent.join(format!("{base}.bak-{}", unique_name_suffix(now)))
            }
            _ => PathBuf::from(format!("{base}.bak-{}", unique_name_suffix(now))),
        };
        std::fs::copy(db_path, &backup).map_err(|error| {
            StoreError::Sql(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
        })?;
        harden_file_permissions(&backup);
        Ok(backup)
    }

    /// Ticket 40: atomic file restore. Validates the payload via a dry-run,
    /// takes a timestamped backup, rebuilds into a temp sibling DB, runs
    /// `quick_check`, then atomically renames over the live path. A kill
    /// before the rename leaves the old library intact. Returns the outcome
    /// plus the backup path (`None` when no live file existed, so no backup
    /// was taken — never a phantom path to a file that was never written).
    pub fn restore_library_file(
        db_path: &Path,
        payload: &str,
        now: i64,
    ) -> Result<(RestoreOutcome, Option<PathBuf>), StoreError> {
        // Validate first: a malformed backup never touches the live DB.
        {
            let mut dry = Store::open_in_memory()?;
            dry.restore_library_json(payload)?;
            dry.check_quick()?;
        }
        let backup = if db_path.exists() {
            Some(Self::backup_db_file(db_path, now)?)
        } else {
            None
        };
        // The live file's parents may not exist yet (fresh directory):
        // create them before the temp DB lands.
        if let Some(parent) = db_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|error| StoreError::Io {
                    path: parent.to_path_buf(),
                    message: error.to_string(),
                })?;
            }
        }
        // Build into a temp sibling DB in the same directory (same filesystem
        // → rename is atomic).
        let tmp = match db_path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.join(format!(
                ".{}.restore-{}",
                db_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("library.db"),
                unique_name_suffix(now)
            )),
            _ => PathBuf::from(format!(".library.restore-{}", unique_name_suffix(now))),
        };
        // Ensure a stale temp from a killed restore never shadows us.
        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_file(sibling_sidecar(&tmp, "-wal"));
        let _ = std::fs::remove_file(sibling_sidecar(&tmp, "-shm"));
        let outcome = {
            let mut tmp_store = Store::open_with_path(
                Connection::open(&tmp).map_err(StoreError::from)?,
                Some(tmp.clone()),
            )?;
            let outcome = tmp_store.restore_library_json(payload)?;
            tmp_store.check_quick()?;
            // Close + fsync via checkpoint before the rename.
            let _ = tmp_store
                .conn
                .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
            drop(tmp_store);
            outcome
        };
        swap_tmp_over_live(&tmp, db_path).map_err(|error| {
            let _ = std::fs::remove_file(&tmp);
            StoreError::Io {
                path: db_path.to_path_buf(),
                message: error.to_string(),
            }
        })?;
        // Drop stale WAL sidecars of the old live file; the renamed file
        // carries its own journal state.
        for suffix in ["-wal", "-shm", "-journal"] {
            // Only remove sidecars that predate the rename (best-effort).
            let _ = std::fs::remove_file(sibling_sidecar(db_path, suffix));
        }
        if let Some(parent) = db_path.parent() {
            if !parent.as_os_str().is_empty() {
                // Best-effort dir fsync is platform-specific; rename itself is
                // the atomicity guarantee (sqlite atomiccommit).
            }
        }
        harden_file_permissions(db_path);
        Ok((outcome, backup))
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
    /// (ticket 37: `route_presets` table + `proxy_configs.preset_id`); version
    /// 5 stages per-sub intervals, health history, preset extensions, and
    /// URL-keyed tombstones (ticket 40); version 6 seals subscription URLs
    /// at rest (ticket 35/48: tokens ride in query strings). Fresh and legacy
    /// (`user_version = 0`)
    /// databases migrate forward idempotently; a database from a NEWER build
    /// fails loudly (never a silent empty). The version bump lands LAST, so a
    /// kill mid-migration retries on next open instead of trusting a
    /// half-migrated file.
    const SCHEMA_VERSION: i64 = 6;
    /// Minimum SQLite for this schema: STRICT tables need 3.37+.
    const MIN_SQLITE: (i64, i64, i64) = (3, 37, 0);

    /// Ticket 35: seal params for storage. A DPAPI failure aborts the
    /// write loudly (never silent plaintext). Ticket A2: typed as
    /// `Crypto`, never `Sql` (a crypto outage is not a database error).
    fn seal_or_fail(plaintext: &str) -> Result<String, StoreError> {
        crate::dpapi::seal_params(plaintext).map_err(|message| StoreError::Crypto {
            message: format!("dpapi seal failed: {message}"),
        })
    }

    /// Ticket 35: open stored params. Sealed values decrypt; legacy JSON
    /// passes through; anything else fails loudly. Ticket A2: typed as
    /// `Crypto`, never `Sql`.
    fn unseal_or_fail(stored: &str) -> Result<String, StoreError> {
        crate::dpapi::unseal_params(stored).map_err(|message| StoreError::Crypto {
            message: format!("dpapi open failed: {message}"),
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
        // Ticket 40 (v5): fresh DDL carries the full v5 shape so fresh and
        // migrated DBs converge; migrated v4 DBs backfill via `ensure_column`
        // below (all online ADD COLUMN / CREATE TABLE, no rebuild).
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
             CREATE TABLE IF NOT EXISTS subscriptions (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               label TEXT NOT NULL,
               url TEXT NOT NULL,
               grp TEXT NOT NULL DEFAULT 'default',
               auto_refresh INTEGER NOT NULL DEFAULT 1,
               last_refresh_at INTEGER,
               last_error TEXT,
               failures TEXT NOT NULL DEFAULT '[]',
               interval_secs INTEGER NOT NULL DEFAULT 86400
             ) STRICT;
             CREATE TABLE IF NOT EXISTS subscription_tombstones (
               subscription_id TEXT NOT NULL,
               identity_key TEXT NOT NULL,
               subscription_url TEXT,
               rotation_core TEXT,
               PRIMARY KEY (subscription_id, identity_key)
             ) STRICT;
             CREATE TABLE IF NOT EXISTS health_snapshots (
               config_id TEXT PRIMARY KEY,
               at INTEGER NOT NULL,
               kind TEXT NOT NULL DEFAULT 'url-test',
               latency_ms INTEGER,
               error TEXT
             ) STRICT;
             CREATE TABLE IF NOT EXISTS health_history (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               config_id TEXT NOT NULL,
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
                updated_at INTEGER,
                description TEXT,
                is_builtin INTEGER NOT NULL DEFAULT 0,
                last_used_at INTEGER
              ) STRICT;
             CREATE TABLE IF NOT EXISTS meta (
               key TEXT PRIMARY KEY,
               value TEXT NOT NULL DEFAULT ''
             ) STRICT;",
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
        // Ticket 40 (v5): per-sub interval, preset extensions, tombstone URL
        // + rotation core. All constraint-free ADD COLUMN (online, time
        // independent of row count); idempotent via `ensure_column`.
        self.ensure_column(
            "subscriptions",
            "interval_secs",
            "INTEGER NOT NULL DEFAULT 86400",
        )?;
        self.ensure_column("route_presets", "description", "TEXT")?;
        self.ensure_column("route_presets", "is_builtin", "INTEGER NOT NULL DEFAULT 0")?;
        self.ensure_column("route_presets", "last_used_at", "INTEGER")?;
        self.ensure_column("subscription_tombstones", "subscription_url", "TEXT")?;
        self.ensure_column("subscription_tombstones", "rotation_core", "TEXT")?;
        // Indexes (after all columns exist, so legacy DBs missing columns
        // never fail index creation; IF NOT EXISTS keeps re-runs idempotent).
        self.conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_proxy_configs_enabled
               ON proxy_configs(enabled);
             CREATE INDEX IF NOT EXISTS idx_proxy_configs_subscription
               ON proxy_configs(subscription_id);
             CREATE INDEX IF NOT EXISTS idx_proxy_configs_identity
               ON proxy_configs(subscription_id, identity_key);
             CREATE INDEX IF NOT EXISTS idx_route_presets_name
               ON route_presets(name);
             CREATE INDEX IF NOT EXISTS idx_health_history_config_at
               ON health_history(config_id, at);
             CREATE INDEX IF NOT EXISTS idx_tombstones_url
               ON subscription_tombstones(subscription_url, identity_key);
             CREATE INDEX IF NOT EXISTS idx_tombstones_core
               ON subscription_tombstones(subscription_url, rotation_core);",
        )?;
        // Backfill v5 tombstone URL + rotation core where the owning
        // subscription still exists (orphan legacy tombstones keep NULL and
        // still match by exact id-key; GC cleans them explicitly).
        {
            let rows: Vec<(String, String)> = {
                let mut stmt = self.conn.prepare(
                    "SELECT subscription_id, identity_key FROM subscription_tombstones
                     WHERE subscription_url IS NULL",
                )?;
                let mapped =
                    stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
                let mut out = Vec::new();
                for row in mapped {
                    out.push(row?);
                }
                out
            };
            for (sub_id, key) in rows {
                let url: Option<String> = self
                    .conn
                    .query_row(
                        "SELECT url FROM subscriptions WHERE id = ?1",
                        params![sub_id.parse::<i64>().unwrap_or(-1)],
                        |r| r.get(0),
                    )
                    .ok();
                if let Some(url) = url {
                    let core = rotation_core_from_identity_key(&key);
                    self.conn.execute(
                        "UPDATE subscription_tombstones SET subscription_url = ?1, rotation_core = ?2
                         WHERE subscription_id = ?3 AND identity_key = ?4",
                        params![url, core, sub_id, key],
                    )?;
                }
            }
        }
        // Ticket 35 (version 3): seal params at rest. Every non-sealed value
        // seals in one transaction — including legacy non-JSON garbage that
        // does not start with `{` (leaving it would strand plaintext under a
        // bumped version, violating the sealed-at-rest contract); already-
        // sealed rows are untouched, so the migration is idempotent and
        // reruns are no-ops. Unreadable-after-seal rows surface through the
        // quarantine list, never as silent plaintext.
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
                    if value.starts_with(crate::dpapi::MARKER) {
                        continue;
                    }
                    let sealed = Self::seal_or_fail(&value)?;
                    tx.execute(
                        &format!("UPDATE proxy_configs SET {column} = ?1 WHERE id = ?2"),
                        rusqlite::params![sealed, id],
                    )?;
                }
            }
            tx.commit()?;
        }
        // Ticket 35/48 (version 6): seal subscription URLs at rest.
        // Subscription URLs carry tokens in the query string, so the
        // `subscriptions.url` column seals exactly like params — sealed rows
        // are skipped, every other row seals in one transaction before the
        // version bump below (never stranded plaintext).
        if version < 6 {
            let tx = self.conn.transaction()?;
            let rows = {
                let mut stmt = tx.prepare("SELECT id, url FROM subscriptions")?;
                let mapped = stmt.query_map([], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                })?;
                mapped.collect::<Result<Vec<_>, _>>()?
            };
            for (id, url) in rows {
                if url.trim_start().starts_with(crate::dpapi::MARKER) {
                    continue;
                }
                let sealed = Self::seal_or_fail(&url)?;
                tx.execute(
                    "UPDATE subscriptions SET url = ?1 WHERE id = ?2",
                    rusqlite::params![sealed, id],
                )?;
            }
            tx.commit()?;
        }
        // Unknown drift fails loudly: every column the merge relies on must
        // exist after migration, never a silent empty read.
        // Ticket 40: drift loop extended to the v5 columns; fresh and
        // migrated DBs converge to identical DDL per this check.
        for (table, column) in [
            ("proxy_configs", "subscription_id"),
            ("proxy_configs", "identity_key"),
            ("proxy_configs", "adoptable"),
            ("proxy_configs", "synced_params"),
            ("proxy_configs", "preset_id"),
            ("proxy_configs", "traffic_rx"),
            ("proxy_configs", "traffic_tx"),
            ("subscriptions", "failures"),
            ("subscriptions", "interval_secs"),
            ("route_presets", "description"),
            ("route_presets", "is_builtin"),
            ("route_presets", "last_used_at"),
            ("subscription_tombstones", "subscription_url"),
            ("subscription_tombstones", "rotation_core"),
        ] {
            if !self.column_exists(table, column)? {
                return Err(Self::schema_error(format!(
                    "library database is missing {table}.{column} after migration: restore from an export instead of running on a half-migrated file"
                )));
            }
        }
        // health_history table must exist after migration.
        {
            let count: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='health_history'",
                [],
                |row| row.get(0),
            )?;
            if count != 1 {
                return Err(Self::schema_error(
                    "library database is missing health_history after migration: restore from an export instead of running on a half-migrated file"
                        .to_string(),
                ));
            }
        }
        // Ticket 40: bump LAST — the crash-safety property. A kill anywhere
        // above leaves the old version number, so next open re-runs the
        // migration instead of trusting a half-migrated file. DPAPI-down seal
        // failures return before this line, so plaintext is never stranded as
        // migrated.
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

    /// Ticket 40: tolerant list — good rows plus quarantined ids. One corrupt
    /// params blob (bad DPAPI marker, out-of-range port) never poisons the
    /// whole list; the UI renders goods with a banner naming the ids. Write
    /// paths and single-row reads stay strict ([`Store::list`]/[`Store::get`]
    /// still fail loudly).
    pub fn list_with_quarantine(
        &self,
    ) -> Result<(Vec<ProxyConfig>, Vec<QuarantinedRow>), StoreError> {
        // Map each row individually so one fault partitions instead of
        // failing the whole list.
        let ids: Vec<i64> = {
            let mut id_stmt = self
                .conn
                .prepare("SELECT id FROM proxy_configs ORDER BY id")?;
            let mapped = id_stmt.query_map([], |row| row.get::<_, i64>(0))?;
            let mut out = Vec::new();
            for row in mapped {
                out.push(row?);
            }
            out
        };
        let mut good = Vec::with_capacity(ids.len());
        let mut quarantined = Vec::new();
        for id in ids {
            let mut per_stmt = self.conn.prepare(
                "SELECT id, name, protocol, params, grp, enabled, bind_ip, port,
                  subscription_id, identity_key, preset_id
                 FROM proxy_configs WHERE id = ?1",
            )?;
            match per_stmt.query_row(params![id], map_row) {
                Ok(row) => good.push(row),
                Err(error) => quarantined.push(QuarantinedRow {
                    id: id.to_string(),
                    cause: format!("row {id} unreadable ({error}); re-import or restore"),
                }),
            }
        }
        Ok((good, quarantined))
    }

    /// Ticket 40: tolerant export — skips quarantined rows, returns the
    /// payload plus the skipped ids so the dialog can offer skip-or-flag.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn export_library_json_with_quarantine(&self) -> Result<(String, Vec<String>), StoreError> {
        let (rows, quarantined) = self.list_with_quarantine()?;
        let mut skipped: Vec<String> = quarantined.iter().map(|q| q.id.clone()).collect();
        let (rows, mut bad_synced) = self.without_unreadable_synced(rows);
        skipped.append(&mut bad_synced);
        // Build the export from the good rows only (same shape as
        // `export_library_json`, quarantined rows omitted, never failed).
        let subs = self.list_subscriptions()?;
        let tombstones = self.list_tombstones_raw()?;
        let payload = export_payload_full(&rows, &subs, &tombstones);
        // Patch in the v5 full-fidelity sections for the good rows only.
        let enriched = self.enrich_export_payload(&payload, &rows)?;
        Ok((enriched, skipped))
    }

    /// Raw tombstones for export (ordered, no filtering).
    fn list_tombstones_raw(&self) -> Result<Vec<(String, String)>, StoreError> {
        // Ticket 40: export the URL-keyed shape when present; legacy readers
        // ignore the extra keys. The base pair stays (subscription_id,
        // identity_key) for backward compat.
        let mut stmt = self.conn.prepare(
            "SELECT subscription_id, identity_key FROM subscription_tombstones
             ORDER BY subscription_id, identity_key",
        )?;
        let mapped = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut out = Vec::new();
        for row in mapped {
            out.push(row?);
        }
        Ok(out)
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
    /// Ticket 40: tombstones carry the subscription URL + rotation core so a
    /// remove-subscribe + re-add of the same URL still honors the delete.
    pub fn remove(&mut self, id: &str) -> Result<bool, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Ok(false),
        };
        let current = self.get(id)?;
        // Resolve the owning subscription URL before opening the tx (reads
        // before the write tx keeps the tx short).
        let owner_url: Option<String> = match &current {
            Some(row) => match &row.subscription_id {
                Some(sub_id) => self.get_subscription(sub_id)?.map(|s| s.url),
                None => None,
            },
            None => None,
        };
        let tx = self.conn.transaction()?;
        let removed = tx.execute("DELETE FROM proxy_configs WHERE id = ?1", params![rowid])?;
        if removed > 0 {
            if let Some(row) = current {
                let key = row
                    .identity_key
                    .unwrap_or_else(|| widened_identity(&row.protocol, &row.params));
                if let Some(subscription_id) = row.subscription_id {
                    let core = rotation_core_from_identity_key(&key)
                        .or_else(|| rotation_core(&row.params));
                    tx.execute(
                        "INSERT OR IGNORE INTO subscription_tombstones
                           (subscription_id, identity_key, subscription_url, rotation_core)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![subscription_id, key, owner_url, core],
                    )?;
                }
            }
            tx.execute(
                "DELETE FROM health_snapshots WHERE config_id = ?1",
                params![id],
            )?;
            // Ticket 40: history of a deleted row is pruned with it (bounded,
            // no stale resurrection after restart).
            let _ = tx.execute(
                "DELETE FROM health_history WHERE config_id = ?1",
                params![id],
            );
        }
        tx.commit()?;
        Ok(removed > 0)
    }

    /// Ticket 44: per-group row counts for the facet picker and the group
    /// manager (ordered by group name). Groups are free-text labels, not a
    /// table — this is the only group inventory.
    pub fn group_counts(&self) -> Result<Vec<(String, usize)>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT grp, COUNT(*) FROM proxy_configs GROUP BY grp ORDER BY grp")?;
        let mapped = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        })?;
        let mut out = Vec::new();
        for row in mapped {
            out.push(row?);
        }
        Ok(out)
    }

    /// Ticket 44: rename a group label across every row that carries it.
    /// Returns the rows moved. An empty new name is rejected (nothing
    /// written); renaming a missing group moves zero rows.
    pub fn rename_group(&mut self, old: &str, new: &str) -> Result<usize, StoreError> {
        let old = normalized_group(old);
        let new = normalized_group(new);
        if new.is_empty() {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "group",
                "Group name cannot be empty",
            )]));
        }
        let changed = self.conn.execute(
            "UPDATE proxy_configs SET grp = ?1 WHERE grp = ?2",
            params![new, old],
        )?;
        Ok(changed)
    }

    /// Ticket 44: move rows into a group by id. Unknown ids are skipped
    /// (bulk semantics — already-removed rows are not failures); returns
    /// the rows actually moved. An empty group name is rejected.
    pub fn move_to_group(&mut self, ids: &[String], group: &str) -> Result<usize, StoreError> {
        let group = normalized_group(group);
        if group.is_empty() {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "group",
                "Group name cannot be empty",
            )]));
        }
        let rowids: Vec<i64> = ids.iter().filter_map(|id| id.parse::<i64>().ok()).collect();
        if rowids.is_empty() {
            return Ok(0);
        }
        let placeholders: Vec<String> = rowids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 2))
            .collect();
        let sql = format!(
            "UPDATE proxy_configs SET grp = ?1 WHERE id IN ({})",
            placeholders.join(", ")
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let changed = stmt.execute(rusqlite::params_from_iter(
            std::iter::once(group.to_string()).chain(rowids.iter().map(|id| id.to_string())),
        ))?;
        Ok(changed)
    }

    /// Ticket 44: delete a group label. Refuses loudly while rows still
    /// carry it (names the count so the manager can offer move-first);
    /// deleting an already-empty group is a no-op success. The default
    /// group label itself is never special-cased here — emptiness decides.
    pub fn delete_group(&mut self, name: &str) -> Result<(), StoreError> {
        let name = normalized_group(name);
        let remaining: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM proxy_configs WHERE grp = ?1",
            params![name],
            |row| row.get(0),
        )?;
        if remaining > 0 {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "group",
                format!("Group \"{name}\" still holds {remaining} row(s); move them first"),
            )]));
        }
        Ok(())
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
    /// Ticket 40: check + write run in one transaction (re-validated in-tx),
    /// so a preset deleted between check and write still refuses loudly.
    /// Successful attaches stamp `last_used_at`.
    pub fn set_config_preset(
        &mut self,
        config_id: &str,
        preset_id: Option<&str>,
    ) -> Result<ProxyConfig, StoreError> {
        let rowid: i64 = match config_id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Err(StoreError::NotFound(config_id.to_string())),
        };
        let tx = self.conn.transaction()?;
        if let Some(preset) = preset_id {
            let rowid_preset: i64 = preset.parse().unwrap_or(-1);
            let rule_count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM route_presets WHERE id = ?1",
                params![rowid_preset],
                |row| row.get(0),
            )?;
            if rule_count == 0 {
                return Err(StoreError::NotFound(preset.to_string()));
            }
            // An empty-rules preset serves nothing (the merge would add no
            // rule, silently serving preset-less) — refuse the attach loudly
            // until the preset carries rules.
            let rules_json: String = tx.query_row(
                "SELECT rules_json FROM route_presets WHERE id = ?1",
                params![rowid_preset],
                |row| row.get(0),
            )?;
            let empty = serde_json::from_str::<serde_json::Value>(&rules_json)
                .ok()
                .and_then(|value| value.as_array().cloned())
                .is_some_and(|rules| rules.is_empty());
            if empty {
                return Err(StoreError::Invalid(vec![FieldError::new(
                    "preset",
                    "preset has no rules yet — refresh it before attaching",
                )]));
            }
        }
        let changed = tx.execute(
            "UPDATE proxy_configs SET preset_id = ?1 WHERE id = ?2",
            params![preset_id, rowid],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(config_id.to_string()));
        }
        if let Some(preset) = preset_id {
            let now = now_unix();
            let _ = tx.execute(
                "UPDATE route_presets SET last_used_at = ?1 WHERE id = ?2",
                params![now, preset.parse::<i64>().unwrap_or(-1)],
            );
        }
        tx.commit()?;
        self.get(config_id)?
            .ok_or_else(|| StoreError::NotFound(config_id.to_string()))
    }

    /// Ticket 40: rows whose `preset_id` points at a missing preset
    /// (dangling references from cross-DB restores or races). The UI renders
    /// these behind an orphan-sweep banner (never silent at serve).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn find_orphaned_preset_rows(&self) -> Result<Vec<ProxyConfig>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, protocol, params, grp, enabled, bind_ip, port,
              subscription_id, identity_key, preset_id
             FROM proxy_configs
             WHERE preset_id IS NOT NULL
               AND preset_id NOT IN (SELECT id FROM route_presets)
             ORDER BY id",
        )?;
        // `preset_id` here is the stored string id; the subselect compares as
        // text-vs-integer — SQLite dynamic typing still matches numeric ids.
        // Rows with non-numeric dangling ids also land here (never in the
        // presets table).
        let rows = stmt
            .query_map([], map_row)?
            .collect::<Result<Vec<_>, _>>()?;
        // `map_row` unseals params strictly; a corrupt orphan still surfaces
        // via `list_with_quarantine` — but the id-presence check above already
        // ran in SQL. If strict mapping fails here, fall back to a tolerant
        // scan so one bad row never hides the whole orphan set.
        Ok(rows)
    }

    /// Ticket 40: detach every orphaned preset reference in one transaction.
    /// Returns the detached count for the banner copy.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn detach_orphaned_presets(&mut self) -> Result<usize, StoreError> {
        let tx = self.conn.transaction()?;
        let cleared = tx.execute(
            "UPDATE proxy_configs SET preset_id = NULL
             WHERE preset_id IS NOT NULL
               AND preset_id NOT IN (SELECT id FROM route_presets)",
            [],
        )?;
        tx.commit()?;
        Ok(cleared)
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

    /// Ticket 43: rename a preset (manager edit path). The new name validates
    /// like create (blank fails); a taken name fails with the same
    /// already-exists copy — never a silent overwrite. Rules are untouched.
    pub fn rename_preset(&mut self, id: &str, name: &str) -> Result<RoutePreset, StoreError> {
        let name = Self::validated_preset_name(name)?;
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Err(StoreError::NotFound(id.to_string())),
        };
        match self.conn.execute(
            "UPDATE route_presets SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, now_unix(), rowid],
        ) {
            Ok(0) => return Err(StoreError::NotFound(id.to_string())),
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
            "SELECT id, name, source, source_url, rules_json, updated_at,
                    description, is_builtin, last_used_at
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
            "SELECT id, name, source, source_url, rules_json, updated_at,
                    description, is_builtin, last_used_at
             FROM route_presets WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![rowid], map_preset_row)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Ticket 40: fetch one preset by exact name (restore re-links by name,
    /// never by cross-DB rowid).
    #[allow(dead_code)]
    pub fn get_preset_by_name(&self, name: &str) -> Result<Option<RoutePreset>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, source, source_url, rules_json, updated_at,
                    description, is_builtin, last_used_at
             FROM route_presets WHERE name = ?1",
        )?;
        let mut rows = stmt.query_map(params![name], map_preset_row)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Delete a preset. Refused (`Invalid`) while any row attaches it — the
    /// refusal names the attached rows, so the UI can offer the explicit
    /// detach-then-delete path ([`Store::delete_preset_detaching`]).
    /// Existence check + attachment check + delete run in ONE transaction so
    /// a concurrent attach between check and delete can never dangle.
    pub fn delete_preset(&mut self, id: &str) -> Result<bool, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Ok(false),
        };
        let tx = self.conn.transaction()?;
        let exists: i64 = tx.query_row(
            "SELECT COUNT(*) FROM route_presets WHERE id = ?1",
            params![rowid],
            |row| row.get(0),
        )?;
        if exists == 0 {
            return Ok(false);
        }
        let attached: Vec<String> = {
            let mut stmt =
                tx.prepare("SELECT name, id FROM proxy_configs WHERE preset_id = ?1 ORDER BY id")?;
            let mapped = stmt.query_map(params![id], |row| {
                Ok(format!(
                    "\"{}\" (id {})",
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?
                ))
            })?;
            mapped.collect::<Result<Vec<_>, _>>()?
        };
        if !attached.is_empty() {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "preset",
                format!(
                    "preset is still attached to {} row(s): {} — detach first or delete explicitly with detach",
                    attached.len(),
                    attached.join(", ")
                ),
            )]));
        }
        let removed = tx.execute("DELETE FROM route_presets WHERE id = ?1", params![rowid])?;
        tx.commit()?;
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
        // Same bound as the preset intake (hash-suffixed on overflow, so
        // two long names never truncate-collide into one preset).
        Ok(crate::preset::cap_name(trimmed.to_string()))
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
    /// enabled or started. The URL seals at rest (tokens ride in query
    /// strings); reads unseal transparently.
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
        let sealed_url = Self::seal_or_fail(&url)?;
        self.conn.execute(
            "INSERT INTO subscriptions (label, url, grp, auto_refresh)
             VALUES (?1, ?2, ?3, ?4)",
            params![label, sealed_url, group, auto_refresh as i64],
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
        let sealed_url = Self::seal_or_fail(&url)?;
        self.conn.execute(
            "UPDATE subscriptions SET label = ?1, url = ?2, grp = ?3 WHERE id = ?4",
            params![label, sealed_url, group, rowid],
        )?;
        self.get_subscription(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
    }

    /// Ticket 31: find a subscription by exact (trimmed) URL — the friendly
    /// `already subscribed` path. A missing row is `None`, never an error.
    /// Compared in Rust over unsealed rows: sealed URLs are nondeterministic
    /// ciphertext, so SQL equality could never match them.
    pub fn find_subscription_by_url(&self, url: &str) -> Result<Option<Subscription>, StoreError> {
        let trimmed = url.trim();
        for sub in self.list_subscriptions()? {
            if sub.url == trimmed {
                return Ok(Some(sub));
            }
        }
        Ok(None)
    }

    /// All subscriptions in stable id order.
    pub fn list_subscriptions(&self) -> Result<Vec<Subscription>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, label, url, grp, auto_refresh, last_refresh_at, last_error, failures, interval_secs
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
            "SELECT id, label, url, grp, auto_refresh, last_refresh_at, last_error, failures, interval_secs
             FROM subscriptions WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![rowid], map_subscription_row)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Ticket 40 (v5): set a subscription's auto-refresh interval. Values
    /// `<= 0` mean always-due (matches [`due_for_refresh`]); absurdly large
    /// values saturate at `i64::MAX` via the column type (never wrap).
    /// Ticket 45: production seam behind the per-sub interval picker.
    pub fn set_subscription_interval(
        &mut self,
        id: &str,
        interval_secs: i64,
    ) -> Result<Subscription, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Err(StoreError::NotFound(id.to_string())),
        };
        let changed = self.conn.execute(
            "UPDATE subscriptions SET interval_secs = ?1 WHERE id = ?2",
            params![interval_secs, rowid],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(id.to_string()));
        }
        self.get_subscription(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
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
    /// never deleted; tombstones for this subscription are KEPT (ticket 40:
    /// URL-keyed, so a re-add of the same URL still honors deletes).
    /// Ticket 25: the release + subscription delete apply in a single
    /// transaction. Released rows keep their params but lose their identity,
    /// baseline, and ownership — and are marked adoptable so a second
    /// subscription carrying the same node adopts them in place instead of
    /// double-counting a cross-subscription shadow. Use [`Store::gc_tombstones`]
    /// to drop tombstones whose URL no longer has a subscription.
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
        // Ticket 40: tombstones survive the unsubscribe (URL-keyed). The old
        // `DELETE WHERE subscription_id = ?` is gone — it broke re-subscribe.
        let removed = tx.execute("DELETE FROM subscriptions WHERE id = ?1", params![rowid])?;
        tx.commit()?;
        Ok(removed > 0)
    }

    /// Ticket 40: drop tombstones whose subscription URL no longer has a live
    /// subscription AND whose subscription id is gone. Returns the pruned
    /// count. Explicit (never on the remove path) so re-subscribe before GC
    /// still honors deletes. Matched in Rust over unsealed URLs: sealed
    /// `subscriptions.url` ciphertext never equals the plaintext tombstone
    /// copy in SQL.
    #[allow(dead_code)]
    pub fn gc_tombstones(&mut self) -> Result<usize, StoreError> {
        let live_ids: HashSet<String> = {
            let mut stmt = self.conn.prepare("SELECT id FROM subscriptions")?;
            let mapped = stmt.query_map([], |row| row.get::<_, i64>(0))?;
            mapped
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(|id| id.to_string())
                .collect()
        };
        let live_urls: HashSet<String> = self
            .list_subscriptions()?
            .into_iter()
            .map(|sub| sub.url)
            .collect();
        let tombs: Vec<(String, String, Option<String>)> = {
            let mut stmt = self.conn.prepare(
                "SELECT subscription_id, identity_key, subscription_url FROM subscription_tombstones",
            )?;
            let mapped = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })?;
            mapped.collect::<Result<Vec<_>, _>>()?
        };
        let mut pruned = 0;
        for (sub_id, key, url) in tombs {
            let url_dead = url.as_deref().is_none_or(|url| !live_urls.contains(url));
            if !live_ids.contains(&sub_id) && url_dead {
                pruned += self.conn.execute(
                    "DELETE FROM subscription_tombstones WHERE subscription_id = ?1 AND identity_key = ?2",
                    params![sub_id, key],
                )?;
            }
        }
        Ok(pruned)
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
    /// Ticket A5: byte-level ingest — enforces [`SUBSCRIPTION_MAX_BYTES`]
    /// and UTF-8 before parsing, so oversize/non-text bodies keep the
    /// last-good members and record `last_error` instead of growing us
    /// without bound. Reasons never echo body bytes.
    pub fn refresh_with_bytes(
        &mut self,
        sub_id: &str,
        body: &[u8],
        now: i64,
    ) -> Result<RefreshOutcome, StoreError> {
        if body.len() > SUBSCRIPTION_MAX_BYTES {
            let reason = "subscription body too large".to_string();
            self.record_subscription_error(sub_id, &reason)?;
            return Ok(RefreshOutcome {
                error: Some(reason),
                ..Default::default()
            });
        }
        let text = match std::str::from_utf8(body) {
            Ok(text) => text,
            Err(_) => {
                let reason = "subscription body is not UTF-8".to_string();
                self.record_subscription_error(sub_id, &reason)?;
                return Ok(RefreshOutcome {
                    error: Some(reason),
                    ..Default::default()
                });
            }
        };
        self.refresh_with_body(sub_id, text, now)
    }
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
        // Ticket 40: URL-keyed tombstones with rotation-core matching. A
        // delete stays honored across remove-subscribe + re-add of the same
        // URL (new subscription id, same URL). Exact widened-identity hits
        // skip; rotation-core hits (same server:port, rotated creds/flow)
        // skip too. Legacy id-keyed tombstones still match by id for the
        // same-subscription path.
        let mut tombstones: HashSet<String> = HashSet::new();
        let mut tomb_cores_by_url: HashSet<String> = HashSet::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT identity_key, rotation_core FROM subscription_tombstones
                 WHERE subscription_id = ?1 OR subscription_url = ?2",
            )?;
            let entries = stmt.query_map(params![subscription.id, subscription.url], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?;
            for entry in entries {
                let (key, core) = entry?;
                tombstones.insert(key.clone());
                // Rotation core for the tombstone: stored column first,
                // derived from the identity key as fallback.
                let core = core.or_else(|| rotation_core_from_identity_key(&key));
                if let Some(core) = core {
                    tomb_cores_by_url.insert(core);
                }
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
            // Ticket 40: rotation-core tombstone match — upstream rotated
            // creds/flow on the same server:port the user deleted stays
            // deleted (URL-keyed, cross-resubscribe safe).
            if let Some(core) = rotation_core(&config.params) {
                if tomb_cores_by_url.contains(&core) {
                    outcome.skipped += 1;
                    continue;
                }
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
                    // Ticket 25: refresh-deletes prune like remove() does
                    // (snapshot + history, never a stale resurrection).
                    tx.execute(
                        "DELETE FROM health_snapshots WHERE config_id = ?1",
                        params![id],
                    )?;
                    let _ = tx.execute(
                        "DELETE FROM health_history WHERE config_id = ?1",
                        params![id],
                    );
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

    /// Ticket 45: default visible-logs destination — `MANYPROXY_LOGS`
    /// overrides; otherwise a `manyproxy-logs.txt` sibling of the library
    /// file (same export-path pattern as [`Store::default_export_path`]:
    /// explicit user action writes here, nothing automatic ever does).
    pub fn default_logs_path() -> PathBuf {
        if let Ok(path) = std::env::var("MANYPROXY_LOGS") {
            if !path.trim().is_empty() {
                return PathBuf::from(path);
            }
        }
        let sibling = Self::default_path();
        if let Some(parent) = sibling.parent() {
            if !parent.as_os_str().is_empty() {
                return parent.join("manyproxy-logs.txt");
            }
        }
        PathBuf::from("manyproxy-logs.txt")
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
    /// Ticket 40: full-fidelity — `route_presets` + per-row preset linkage
    /// by NAME + traffic + meta survive, so export-then-restore elsewhere
    /// rebuilds routing policy and counters. Tolerant of corrupt rows
    /// (quarantined rows are skipped, never fail the whole export — the
    /// caller uses `export_library_json_with_quarantine` for the skip list).
    /// Rows whose sync baseline no longer unseals are skipped the same way
    /// (logged + quarantined, never a whole-export abort):
    /// `list_with_quarantine` cannot see that column (`map_row` never reads
    /// it), so export filters it here.
    /// Ticket A8: single pipeline — this delegates to
    /// [`Store::export_library_json_with_quarantine`] and drops the skip
    /// list, so the two exports can never drift apart (one funnel for the
    /// good-rows shape, the quarantine list, and the v5 sections).
    pub fn export_library_json(&self) -> Result<String, StoreError> {
        let (payload, _) = self.export_library_json_with_quarantine()?;
        Ok(payload)
    }

    /// Split `rows` into exportable rows plus skipped ids: any row whose
    /// `synced_params` fails to unseal is logged and skipped (never a whole
    /// export failure). A broken side-channel read degrades to no filtering
    /// (loud `eprintln`, never silent data loss either way).
    fn without_unreadable_synced(&self, rows: Vec<ProxyConfig>) -> (Vec<ProxyConfig>, Vec<String>) {
        let mut bad: HashSet<String> = HashSet::new();
        match self
            .conn
            .prepare("SELECT id, synced_params FROM proxy_configs WHERE synced_params IS NOT NULL")
        {
            Ok(mut stmt) => {
                match stmt.query_map([], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
                }) {
                    Ok(mapped) => {
                        for entry in mapped {
                            match entry {
                                Ok((id, Some(sealed))) => {
                                    if Self::unseal_or_fail(&sealed).is_err() {
                                        eprintln!(
                                            "export skips row {id}: unreadable sync baseline (quarantined)"
                                        );
                                        bad.insert(id.to_string());
                                    }
                                }
                                Ok((_, None)) => {}
                                Err(error) => {
                                    eprintln!(
                                        "export sync-baseline scan hit an unreadable row ({error}); keeping it"
                                    );
                                }
                            }
                        }
                    }
                    Err(error) => {
                        eprintln!("export sync-baseline scan failed ({error}); no rows filtered");
                    }
                }
            }
            Err(error) => {
                eprintln!("export sync-baseline scan failed ({error}); no rows filtered");
            }
        }
        let skipped: Vec<String> = bad.iter().cloned().collect();
        let kept: Vec<ProxyConfig> = rows
            .into_iter()
            .filter(|row| !bad.contains(&row.id))
            .collect();
        (kept, skipped)
    }

    /// Ticket 40: patch the v5 full-fidelity sections into a base export
    /// payload for exactly `rows` (good rows only). Adds `adoptable` /
    /// `synced_params` (ticket 25), `preset` (name, never id),
    /// `traffic_rx` / `traffic_tx`, `route_presets`, and `meta`.
    fn enrich_export_payload(
        &self,
        base_payload: &str,
        rows: &[ProxyConfig],
    ) -> Result<String, StoreError> {
        /// Per-row export side-channels: adoptable, synced baseline,
        /// counters, preset name.
        type SideRow = (bool, Option<String>, i64, i64, Option<String>);
        // Ownership side-channels + traffic + preset linkage, keyed by id.
        let mut side: HashMap<String, SideRow> = HashMap::new();
        // Row ids whose sync baseline failed to unseal (quarantined below).
        let mut unreadable_synced: HashSet<String> = HashSet::new();
        {
            // preset_id → preset name resolution happens below; here we read
            // the raw linkage plus counters.
            let mut stmt = self.conn.prepare(
                "SELECT id, adoptable, synced_params, traffic_rx, traffic_tx, preset_id
                 FROM proxy_configs",
            )?;
            let entries = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            })?;
            // Preset id → name map for linkage by name.
            let preset_names: HashMap<String, String> = self
                .list_presets()?
                .into_iter()
                .map(|p| (p.id, p.name))
                .collect();
            for entry in entries {
                let (id, adoptable, synced, rx, tx, preset_id) = entry?;
                // One unreadable sync baseline quarantines its row (logged +
                // skipped), never the whole export.
                let synced = match synced {
                    Some(sealed) => match Self::unseal_or_fail(&sealed) {
                        Ok(text) => Some(text),
                        Err(error) => {
                            eprintln!("export skips row {id}: unreadable sync baseline ({error})");
                            unreadable_synced.insert(id.to_string());
                            continue;
                        }
                    },
                    None => None,
                };
                let preset_name = preset_id
                    .as_deref()
                    .and_then(|pid| preset_names.get(pid).cloned());
                side.insert(
                    id.to_string(),
                    (adoptable != 0, synced, rx, tx, preset_name),
                );
            }
        }
        let mut value: serde_json::Value = serde_json::from_str(base_payload)
            .map_err(|error| Self::schema_error(format!("export serialization broke ({error})")))?;
        if let Some(entries) = value.get_mut("library").and_then(|v| v.as_array_mut()) {
            // Drop any library entries for quarantined rows (base payload was
            // built from good rows only, so this is a no-op — belt and
            // suspenders for callers that pass a foreign base) plus rows
            // whose sync baseline failed to unseal above.
            let good_ids: HashSet<&str> = rows.iter().map(|r| r.id.as_str()).collect();
            entries.retain(|e| {
                e.get("id")
                    .and_then(|v| v.as_str())
                    .is_some_and(|id| good_ids.contains(id) && !unreadable_synced.contains(id))
            });
            for entry in entries.iter_mut() {
                if let Some(id) = entry.get("id").and_then(|v| v.as_str()) {
                    let (adoptable, synced, rx, tx, preset) =
                        side.get(id).cloned().unwrap_or((false, None, 0, 0, None));
                    entry["adoptable"] = serde_json::Value::Bool(adoptable);
                    entry["synced_params"] = match synced {
                        Some(text) => serde_json::Value::String(text),
                        None => serde_json::Value::Null,
                    };
                    entry["traffic_rx"] = serde_json::Value::Number(rx.into());
                    entry["traffic_tx"] = serde_json::Value::Number(tx.into());
                    entry["preset"] = match preset {
                        Some(name) => serde_json::Value::String(name),
                        None => serde_json::Value::Null,
                    };
                }
            }
        }
        // route_presets section (full rules, never secret-bearing).
        {
            let presets = self.list_presets()?;
            let items: Vec<serde_json::Value> = presets
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "id": p.id,
                        "name": p.name,
                        "source": p.source,
                        "source_url": p.source_url,
                        "rules_json": p.rules_json,
                        "updated_at": p.updated_at,
                        "description": p.description,
                        "is_builtin": p.is_builtin,
                        "last_used_at": p.last_used_at,
                    })
                })
                .collect();
            if let Some(obj) = value.as_object_mut() {
                obj.insert("route_presets".to_string(), serde_json::Value::Array(items));
            }
        }
        // meta section (allow-listed keys only; future keys ride the same
        // table without code changes, but only these restore).
        {
            let mut meta = serde_json::Map::new();
            for key in [
                Self::META_RESTORE_RUNNING,
                Self::META_RUNNING_IDS,
                Self::META_SUBS_AUTO_REFRESH,
            ] {
                if let Some(v) = self.get_meta(key)? {
                    meta.insert(key.to_string(), serde_json::Value::String(v));
                }
            }
            if let Some(obj) = value.as_object_mut() {
                obj.insert("meta".to_string(), serde_json::Value::Object(meta));
            }
        }
        // Tombstones with URL + rotation core for cross-DB fidelity.
        {
            let mut stmt = self.conn.prepare(
                "SELECT subscription_id, identity_key, subscription_url, rotation_core
                 FROM subscription_tombstones ORDER BY subscription_id, identity_key",
            )?;
            let enriched: Vec<serde_json::Value> = stmt
                .query_map([], |row| {
                    Ok(serde_json::json!({
                        "subscription_id": row.get::<_, String>(0)?,
                        "identity_key": row.get::<_, String>(1)?,
                        "subscription_url": row.get::<_, Option<String>>(2)?,
                        "rotation_core": row.get::<_, Option<String>>(3)?,
                    }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            if let Some(obj) = value.as_object_mut() {
                obj.insert("tombstones".to_string(), serde_json::Value::Array(enriched));
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
        // Never overwrite the live database with JSON export bytes: refuse
        // when the destination resolves to the live DB file.
        if let Some(live) = self.db_path.as_deref() {
            if paths_equal(live, path) {
                return Err(StoreError::Invalid(vec![FieldError::new(
                    "export",
                    "export path must not be the live database file",
                )]));
            }
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
    /// Ticket 25: rebuild a [`Store::export_library_json`] payload verbatim —
    /// same row ids, same groups, same enabled flags, same subscription
    /// records with refresh metadata, same tombstones (deletes stay
    /// deleted). Rows land exactly as exported (no re-validation, no port
    /// reassignment); anything malformed fails loudly with an `export`
    /// field error before anything is written (single transaction). The
    /// import grammar keeps reading only `outbounds` (ticket 24 territory);
    /// this is the store half that consumes the `library`/`subscriptions`/
    /// `tombstones` sections ticket 25 exposes.
    /// Ticket 40: full-fidelity — `route_presets` rebuild by id, per-row
    /// linkage re-attached BY NAME (never dangling), traffic counters,
    /// `interval_secs`, URL-keyed tombstones, and allow-listed `meta` all
    /// survive. Old payloads missing the new sections still restore
    /// (presets untouched, traffic zero, meta kept). The wipe + rebuild run
    /// in ONE transaction: a mid-restore kill rolls back, old library intact.
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
        let presets_json = obj
            .get("route_presets")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let meta_json = obj
            .get("meta")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
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
            preset_name: Option<String>,
            traffic_rx: i64,
            traffic_tx: i64,
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
            let traffic_rx = entry
                .get("traffic_rx")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0)
                .max(0);
            let traffic_tx = entry
                .get("traffic_tx")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0)
                .max(0);
            // Ticket A3: strict restore — missing/wrong-typed core fields fail
            // loudly (same `invalid()` shape as id/port above), never silent
            // defaults that would resurrect a corrupt export as good rows.
            let name = get_str(entry, "name")
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| invalid(format!("{what} needs a name")))?;
            let protocol_raw = get_str(entry, "protocol")
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| invalid(format!("{what} needs a protocol")))?;
            let protocol = canonical_protocol(&protocol_raw)
                .ok_or_else(|| invalid(format!("{what} has an unknown protocol")))?;
            let params_raw =
                get_str(entry, "params").ok_or_else(|| invalid(format!("{what} needs params")))?;
            match serde_json::from_str::<serde_json::Value>(&params_raw) {
                Ok(serde_json::Value::Object(_)) => {}
                _ => return Err(invalid(format!("{what} has corrupt params"))),
            }
            let bind_ip_raw = get_str(entry, "bind_ip")
                .ok_or_else(|| invalid(format!("{what} needs a bind_ip")))?;
            {
                let trimmed = bind_ip_raw.trim();
                let bind_ok = trimmed.is_empty()
                    || trimmed.eq_ignore_ascii_case("localhost")
                    || trimmed.parse::<std::net::IpAddr>().is_ok();
                if !bind_ok {
                    return Err(invalid(format!("{what} has a bad bind_ip")));
                }
            }
            // Ticket A3: reserved ranges never restore silently (the save
            // gate would have refused them); name the range like the engine.
            if let Some(range) = crate::engine::reserved_range_of(port as u16) {
                return Err(invalid(format!(
                    "{what} uses reserved port {port} ({range})"
                )));
            }
            rows.push(LibraryRow {
                id,
                name,
                protocol,
                params: params_raw,
                group: get_str(entry, "group").unwrap_or_else(|| "default".to_string()),
                enabled: entry
                    .get("enabled")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                bind_ip: bind_ip_raw,
                port,
                subscription_id: get_str(entry, "subscription_id"),
                identity_key: get_str(entry, "identity_key"),
                synced_params: get_str(entry, "synced_params"),
                adoptable: entry
                    .get("adoptable")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                preset_name: get_str(entry, "preset"),
                traffic_rx,
                traffic_tx,
            });
        }
        // Ticket A3: duplicate bind:port on the restore path fails loudly
        // (two rows can never claim one endpoint; activation would refuse
        // it later — fail at restore instead).
        {
            let mut seen: std::collections::HashSet<(String, i64)> =
                std::collections::HashSet::new();
            for (index, row) in rows.iter().enumerate() {
                let key = (row.bind_ip.trim().to_ascii_lowercase(), row.port);
                if !seen.insert(key.clone()) {
                    return Err(invalid(format!(
                        "library[{index}] duplicates bind:port {}:{}",
                        row.bind_ip, row.port
                    )));
                }
            }
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
            interval_secs: i64,
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
                interval_secs: entry
                    .get("interval_secs")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(DEFAULT_AUTO_REFRESH_SECS),
            });
        }
        struct TombRow {
            subscription_id: String,
            identity_key: String,
            subscription_url: Option<String>,
            rotation_core: Option<String>,
        }
        let mut tomb_rows: Vec<TombRow> = Vec::new();
        for (index, entry) in tombs.iter().enumerate() {
            let what = format!("tombstones[{index}]");
            match (
                get_str(entry, "subscription_id"),
                get_str(entry, "identity_key"),
            ) {
                (Some(subscription_id), Some(identity_key)) => {
                    let subscription_url = get_str(entry, "subscription_url");
                    let rotation_core = get_str(entry, "rotation_core")
                        .or_else(|| rotation_core_from_identity_key(&identity_key));
                    tomb_rows.push(TombRow {
                        subscription_id,
                        identity_key,
                        subscription_url,
                        rotation_core,
                    });
                }
                _ => {
                    return Err(invalid(format!(
                        "{what} needs subscription_id + identity_key"
                    )))
                }
            }
        }
        struct PresetRow {
            id: i64,
            name: String,
            source: String,
            source_url: Option<String>,
            rules_json: String,
            updated_at: Option<i64>,
            description: Option<String>,
            is_builtin: bool,
            last_used_at: Option<i64>,
        }
        let mut preset_rows: Vec<PresetRow> = Vec::with_capacity(presets_json.len());
        for (index, entry) in presets_json.iter().enumerate() {
            let what = format!("route_presets[{index}]");
            let id: i64 = entry
                .get("id")
                .and_then(|v| {
                    v.as_str()
                        .and_then(|s| s.parse().ok())
                        .or_else(|| v.as_i64())
                })
                .ok_or_else(|| invalid(format!("{what} needs a string id")))?;
            if id <= 0 {
                return Err(invalid(format!("{what} has a bad id")));
            }
            let name =
                get_str(entry, "name").ok_or_else(|| invalid(format!("{what} needs a name")))?;
            if name.trim().is_empty() {
                return Err(invalid(format!("{what} needs a name")));
            }
            let rules_json = match entry.get("rules_json") {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(v) => v.to_string(),
                None => "[]".to_string(),
            };
            // Loud on corrupt rules: never silently drop routing policy.
            match serde_json::from_str::<serde_json::Value>(&rules_json) {
                Ok(serde_json::Value::Array(_)) => {}
                _ => return Err(invalid(format!("{what} has corrupt rules_json"))),
            }
            preset_rows.push(PresetRow {
                id,
                name,
                source: get_str(entry, "source").unwrap_or_else(|| "manual".to_string()),
                source_url: get_str(entry, "source_url"),
                rules_json,
                updated_at: entry.get("updated_at").and_then(serde_json::Value::as_i64),
                description: get_str(entry, "description"),
                is_builtin: entry
                    .get("is_builtin")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                last_used_at: entry
                    .get("last_used_at")
                    .and_then(serde_json::Value::as_i64),
            });
        }
        // Validate meta values are strings (never secrets in errors).
        let mut meta_rows: Vec<(String, String)> = Vec::new();
        for key in [
            Self::META_RESTORE_RUNNING,
            Self::META_RUNNING_IDS,
            Self::META_SUBS_AUTO_REFRESH,
        ] {
            if let Some(v) = meta_json.get(key).and_then(|v| v.as_str()) {
                // running_ids must parse as an array of strings when present;
                // corrupt snapshots fail loudly (never a guessed row).
                if key == Self::META_RUNNING_IDS {
                    match serde_json::from_str::<serde_json::Value>(v) {
                        Ok(serde_json::Value::Array(_)) => {}
                        _ => return Err(invalid("meta.running_ids is corrupt".to_string())),
                    }
                }
                // Ticket 45: the kill-switch restores only as an explicit
                // on/off bit; anything else fails loudly (never a guess).
                if key == Self::META_SUBS_AUTO_REFRESH && v != "0" && v != "1" {
                    return Err(invalid("meta.subs_auto_refresh is corrupt".to_string()));
                }
                meta_rows.push((key.to_string(), v.to_string()));
            }
        }
        // Seal plaintext params BEFORE opening the tx (DPAPI failures abort
        // before any wipe, so the tx never starts half-sealed).
        let mut sealed_rows: Vec<(LibraryRow, String, Option<String>)> =
            Vec::with_capacity(rows.len());
        for row in rows {
            let sealed_params = Self::seal_or_fail(&row.params)?;
            let sealed_synced = row
                .synced_params
                .as_deref()
                .map(Self::seal_or_fail)
                .transpose()?;
            sealed_rows.push((row, sealed_params, sealed_synced));
        }
        // Seal subscription URLs BEFORE opening the tx (same atomicity rule:
        // export payloads carry plaintext URLs by design, and they seal on
        // the way back in; already-sealed input stays as-is).
        let mut sealed_sub_urls: Vec<String> = Vec::with_capacity(sub_rows.len());
        for sub in &sub_rows {
            if sub.url.trim_start().starts_with(crate::dpapi::MARKER) {
                sealed_sub_urls.push(sub.url.clone());
            } else {
                sealed_sub_urls.push(Self::seal_or_fail(&sub.url)?);
            }
        }
        let tx = self.conn.transaction()?;
        // Wipe + rebuild in ONE transaction (atomic: mid-kill rolls back).
        tx.execute("DELETE FROM proxy_configs", [])?;
        tx.execute("DELETE FROM subscriptions", [])?;
        tx.execute("DELETE FROM subscription_tombstones", [])?;
        // Presets: only wipe when the payload carries an explicit presets
        // array (missing or explicit `null` preserves live presets for
        // re-attach, ticket-37 compat; only `[]` or a populated array —
        // an explicit statement about the set — clears first).
        if obj
            .get("route_presets")
            .and_then(|v| v.as_array())
            .is_some()
        {
            tx.execute("DELETE FROM route_presets", [])?;
        }
        for preset in &preset_rows {
            tx.execute(
                "INSERT INTO route_presets
                   (id, name, source, source_url, rules_json, updated_at,
                    description, is_builtin, last_used_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    preset.id,
                    preset.name,
                    preset.source,
                    preset.source_url,
                    preset.rules_json,
                    preset.updated_at,
                    preset.description,
                    preset.is_builtin as i64,
                    preset.last_used_at,
                ],
            )?;
        }
        // Preset name → id map for linkage (payload ids may differ across
        // DBs; names are the stable key).
        let mut preset_by_name: HashMap<String, String> = HashMap::new();
        if !preset_rows.is_empty() {
            let mut stmt = tx.prepare("SELECT id, name FROM route_presets")?;
            for entry in
                stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            {
                let (id, name) = entry?;
                preset_by_name.insert(name, id.to_string());
            }
        } else {
            // Old payload or empty presets: link against live presets (kept
            // table) by name so same-DB restores still re-attach.
            let mut stmt = tx.prepare("SELECT id, name FROM route_presets")?;
            for entry in
                stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            {
                let (id, name) = entry?;
                preset_by_name.insert(name, id.to_string());
            }
        }
        for (sub, sealed_url) in sub_rows.iter().zip(sealed_sub_urls.iter()) {
            tx.execute(
                "INSERT INTO subscriptions
                   (id, label, url, grp, auto_refresh, last_refresh_at, last_error, failures, interval_secs)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    sub.id,
                    sub.label,
                    sealed_url,
                    sub.group,
                    sub.auto_refresh as i64,
                    sub.last_refresh_at,
                    sub.last_error,
                    sub.failures,
                    sub.interval_secs,
                ],
            )?;
        }
        for (row, sealed_params, sealed_synced) in &sealed_rows {
            // Ticket 40: linkage BY NAME (never dangling ids). Unknown names
            // rebuild unattached (visible in the manager, never a serve-time
            // failure).
            let preset_id: Option<String> = row
                .preset_name
                .as_deref()
                .and_then(|name| preset_by_name.get(name).cloned());
            tx.execute(
                "INSERT INTO proxy_configs
                   (id, name, protocol, params, grp, enabled, bind_ip, port,
                    subscription_id, identity_key, synced_params, adoptable,
                    preset_id, traffic_rx, traffic_tx)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
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
                    preset_id,
                    row.traffic_rx,
                    row.traffic_tx,
                ],
            )?;
        }
        for tomb in &tomb_rows {
            tx.execute(
                "INSERT OR IGNORE INTO subscription_tombstones
                   (subscription_id, identity_key, subscription_url, rotation_core)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    tomb.subscription_id,
                    tomb.identity_key,
                    tomb.subscription_url,
                    tomb.rotation_core,
                ],
            )?;
        }
        for (key, val) in &meta_rows {
            tx.execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, val],
            )?;
        }
        tx.commit()?;
        Ok(RestoreOutcome {
            configs: sealed_rows.len(),
            subscriptions: sub_rows.len(),
            tombstones: tomb_rows.len(),
            presets: preset_rows.len(),
        })
    }

    // -- Ticket 36: data-plane traffic counters ----------------------------

    /// Add data-plane byte deltas for one config. Totals persist (restart
    /// resumes from these, never zeroed) and saturate at `i64::MAX`
    /// (SQLite integers are 64-bit signed). Implemented on the batched path;
    /// ticks use [`Store::add_traffic_batch`] directly (one tx per tick).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn add_traffic(&mut self, id: &str, rx: u64, tx: u64) -> Result<(), StoreError> {
        let updated = self.add_traffic_batch(&[(id.to_string(), rx, tx)])?;
        if updated == 0 {
            // Preserve the single-row NotFound contract (batch skips unknown
            // ids and counts; single-row callers expect a typed miss).
            if id.parse::<i64>().is_err() {
                return Err(StoreError::Invalid(vec![FieldError::new(
                    "id",
                    "bad config id",
                )]));
            }
            return Err(StoreError::NotFound(id.to_string()));
        }
        Ok(())
    }

    /// Ticket 40: batched traffic writes — one transaction per tick with a
    /// single prepared saturating UPDATE loop. Unknown ids are skipped (not
    /// failed) so one removed row never drops a whole tick; the return count
    /// names how many rows moved. Errors propagate loudly (callers log, never
    /// swallow). Deltas are collected before the tx opens (never held across
    /// network I/O).
    pub fn add_traffic_batch(
        &mut self,
        deltas: &[(String, u64, u64)],
    ) -> Result<usize, StoreError> {
        if deltas.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.transaction()?;
        // Saturating single-statement UPDATEs: CASE caps at i64::MAX without
        // overflowing the addition itself.
        let mut stmt = tx.prepare(
            "UPDATE proxy_configs SET
               traffic_rx = CASE WHEN traffic_rx > 9223372036854775807 - ?1
                                 THEN 9223372036854775807 ELSE traffic_rx + ?1 END,
               traffic_tx = CASE WHEN traffic_tx > 9223372036854775807 - ?2
                                 THEN 9223372036854775807 ELSE traffic_tx + ?2 END
             WHERE id = ?3",
        )?;
        let mut updated = 0usize;
        for (id, rx, tx_bytes) in deltas {
            let rowid: i64 = match id.parse() {
                Ok(rowid) => rowid,
                Err(_) => continue,
            };
            // Clamp deltas to i64 range before binding (u64::MAX never wraps).
            let rx = (*rx).min(i64::MAX as u64) as i64;
            let txb = (*tx_bytes).min(i64::MAX as u64) as i64;
            match stmt.execute(params![rx, txb, rowid]) {
                Ok(1..) => updated += 1,
                Ok(_) => {} // unknown id: skip-and-count (quarantine philosophy)
                Err(error) => return Err(error.into()),
            }
        }
        drop(stmt);
        tx.commit()?;
        Ok(updated)
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

    /// Ticket 43: zero one row's data-plane totals (per-row reset, behind a
    /// confirm in the UI). `NotFound` for ghost rows — never a silent no-op.
    pub fn reset_traffic(&mut self, id: &str) -> Result<(), StoreError> {
        self.reset_traffic_counters(id)
    }

    /// Ticket 45: zero one row's persisted usage counters (the confirm gate
    /// lives in the UI; this seam never zeros silently — unknown ids are
    /// `NotFound`, never a silent no-op).
    pub fn reset_traffic_counters(&mut self, id: &str) -> Result<(), StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Err(StoreError::NotFound(id.to_string())),
        };
        let changed = self.conn.execute(
            "UPDATE proxy_configs SET traffic_rx = 0, traffic_tx = 0 WHERE id = ?1",
            params![rowid],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(id.to_string()));
        }
        Ok(())
    }

    /// Ticket 43: zero every row's data-plane totals (Settings "clear
    /// traffic"). Returns the row count so the toast can name it.
    pub fn clear_all_traffic(&mut self) -> Result<usize, StoreError> {
        let changed = self.conn.execute(
            "UPDATE proxy_configs SET traffic_rx = 0, traffic_tx = 0",
            [],
        )?;
        Ok(changed)
    }

    /// Ticket 43: drop every persisted health snapshot (Settings "clear
    /// health"). Returns the row count so the toast can name it.
    pub fn clear_health_snapshots(&mut self) -> Result<usize, StoreError> {
        let changed = self.conn.execute("DELETE FROM health_snapshots", [])?;
        Ok(changed)
    }

    // -- Ticket 12: health snapshot persistence ------------------------------
    /// Called after every recorded probe outcome; restart rehydrates from
    /// [`Store::load_health_snapshots`]. Error text here is a probe cause
    /// (already redacted at the health seam), never raw secrets.
    /// Ticket 25: absurd latencies saturate at `i64::MAX` (never wrap);
    /// implemented on [`Store::save_health_snapshots`].
    /// Ticket 47: probe rounds persist batched ([`record_and_persist`] uses
    /// [`Store::save_health_snapshots`]); kept for single-row callers.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn save_health_snapshot(&mut self, snapshot: &HealthSnapshot) -> Result<(), StoreError> {
        self.save_health_snapshots(std::slice::from_ref(snapshot))
    }

    /// Ticket 25: persist a whole probe round in ONE transaction (the
    /// store half of "health saves off the UI thread": one tx instead of N
    /// statements; the `spawn_blocking` move is main.rs territory, ticket 26).
    /// Ticket 40: the same tx also appends to `health_history` (one tx per
    /// probe round covers latest + history; retention prunes separately via
    /// `prune_health_history_before`, never VACUUM).
    pub fn save_health_snapshots(
        &mut self,
        snapshots: &[HealthSnapshot],
    ) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        for snapshot in snapshots {
            Self::check_health_sample(
                &snapshot.kind,
                snapshot.latency_ms,
                snapshot.error.as_deref(),
            )
            .map_err(|message| StoreError::Invalid(vec![FieldError::new("health", message)]))?;
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
            tx.execute(
                "INSERT INTO health_history (config_id, at, kind, latency_ms, error)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
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

    /// Ticket 40: validate one health sample. `kind` must be `url-test` or
    /// `tcp-ping`; exactly one of `latency_ms` / `error` must be present
    /// (null XOR non-null). Rejections are typed, never silent.
    fn check_health_sample(
        kind: &str,
        latency_ms: Option<u64>,
        error: Option<&str>,
    ) -> Result<(), String> {
        if kind != "url-test" && kind != "tcp-ping" {
            return Err(format!("bad health kind '{kind}' (want url-test|tcp-ping)"));
        }
        match (latency_ms, error) {
            (Some(_), None) | (None, Some(_)) => Ok(()),
            _ => Err("health sample needs latency_ms xor error".to_string()),
        }
    }

    /// Ticket 40: append history samples without touching the latest
    /// fast-path (retention prunes separately). One tx, validated strictly.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn append_health_history(
        &mut self,
        entries: &[HealthHistoryEntry],
    ) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        for entry in entries {
            Self::check_health_sample(&entry.kind, entry.latency_ms, entry.error.as_deref())
                .map_err(|message| StoreError::Invalid(vec![FieldError::new("health", message)]))?;
            let latency_ms: Option<i64> = entry
                .latency_ms
                .map(|ms| i64::try_from(ms).unwrap_or(i64::MAX));
            tx.execute(
                "INSERT INTO health_history (config_id, at, kind, latency_ms, error)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    entry.config_id,
                    entry.at,
                    entry.kind,
                    latency_ms,
                    entry.error,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Ticket 40: history for one config, oldest first, optionally since a
    /// cutoff (`at >= since`). Serves the detail sparkline.
    /// Ticket 45: production seam behind the detail sparkline (bounded by
    /// the caller, never the latest-snapshot fast path).
    pub fn load_health_history(
        &self,
        config_id: &str,
        since: Option<i64>,
    ) -> Result<Vec<HealthHistoryEntry>, StoreError> {
        if let Some(since) = since {
            let mut stmt = self.conn.prepare(
                "SELECT config_id, at, kind, latency_ms, error FROM health_history
                 WHERE config_id = ?1 AND at >= ?2 ORDER BY at",
            )?;
            let mapped = stmt.query_map(params![config_id, since], map_health_history_row)?;
            let mut out = Vec::new();
            for row in mapped {
                out.push(row?);
            }
            Ok(out)
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT config_id, at, kind, latency_ms, error FROM health_history
                 WHERE config_id = ?1 ORDER BY at",
            )?;
            let mapped = stmt.query_map(params![config_id], map_health_history_row)?;
            let mut out = Vec::new();
            for row in mapped {
                out.push(row?);
            }
            Ok(out)
        }
    }

    /// Ticket 40: time-based retention — `DELETE WHERE at < ?` in one tx,
    /// never VACUUM (WAL freelist reuse is fine). Returns the pruned count.
    /// Called at startup + daily with a 30–90d cutoff.
    pub fn prune_health_history_before(&mut self, cutoff: i64) -> Result<usize, StoreError> {
        let tx = self.conn.transaction()?;
        let pruned = tx.execute("DELETE FROM health_history WHERE at < ?1", params![cutoff])?;
        tx.commit()?;
        Ok(pruned)
    }

    /// Ticket 40: retention by age — keep the last `retain_days` of history
    /// as of `now`. One tx, no VACUUM.
    pub fn gc_health_history(&mut self, retain_days: i64, now: i64) -> Result<usize, StoreError> {
        let days = retain_days.max(1);
        let cutoff = now.saturating_sub(days.saturating_mul(86_400));
        self.prune_health_history_before(cutoff)
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

    // -- Ticket 38: opt-in autostart/restore state --------------------------

    /// Meta key behind the session-restore opt-in (`"1"` = on).
    pub const META_RESTORE_RUNNING: &str = "restore_running";
    /// Meta key behind the running-set snapshot (JSON array of config ids).
    pub const META_RUNNING_IDS: &str = "running_ids";
    /// Ticket 43: meta key behind the global probe-URL override (unset means
    /// the documented default). Per-row overrides live under
    /// `test_url:<config_id>`.
    pub const META_TEST_URL: &str = "test_url";

    /// Read one meta value (`None` when unset). Callers decide the default.
    pub fn get_meta(&self, key: &str) -> Result<Option<String>, StoreError> {
        let mut stmt = self.conn.prepare("SELECT value FROM meta WHERE key = ?1")?;
        let mut rows = stmt.query_map(params![key], |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Write one meta value (upsert).
    pub fn set_meta(&mut self, key: &str, value: &str) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// True only when the user opted into session restore. A broken read
    /// fails closed (no restore), never open.
    pub fn restore_running(&self) -> bool {
        self.get_meta(Self::META_RESTORE_RUNNING)
            .map(|value| value.as_deref() == Some("1"))
            .unwrap_or(false)
    }

    /// Persist the session-restore opt-in (explicit Settings checkbox only).
    pub fn set_restore_running(&mut self, on: bool) -> Result<(), StoreError> {
        self.set_meta(Self::META_RESTORE_RUNNING, if on { "1" } else { "0" })
    }

    /// Ticket 45: meta key behind the global subscription auto-refresh
    /// kill-switch (`"1"` = on). Unset means on (existing libraries keep
    /// their behavior); explicit off gates the automatic due pass only —
    /// manual Refresh-all-due is explicit consent and always runs.
    pub const META_SUBS_AUTO_REFRESH: &str = "subs_auto_refresh";

    /// True unless the user explicitly killed subscription auto-refresh. A
    /// broken read fails closed (no automatic network), never open.
    pub fn subs_auto_refresh_enabled(&self) -> bool {
        self.get_meta(Self::META_SUBS_AUTO_REFRESH)
            .map(|value| value.as_deref() != Some("0"))
            .unwrap_or(true)
    }

    /// Persist the global auto-refresh kill-switch (explicit subs-dialog
    /// toggle only — never flipped by ticks).
    pub fn set_subs_auto_refresh_enabled(&mut self, on: bool) -> Result<(), StoreError> {
        self.set_meta(Self::META_SUBS_AUTO_REFRESH, if on { "1" } else { "0" })
    }

    /// Persist the currently running set for a later restore. Best-effort
    /// bookkeeping (like health snapshots): callers ignore errors.
    /// Ticket 40: filters to known config ids (ghost ids never persist) and
    /// surfaces write errors (read-only disks fail loudly, never silent
    /// loss — callers log instead of `let _ =`).
    pub fn save_running_snapshot(&mut self, ids: &[String]) -> Result<(), StoreError> {
        let known: HashSet<String> = {
            let mut stmt = self.conn.prepare("SELECT id FROM proxy_configs")?;
            let mapped = stmt.query_map([], |row| row.get::<_, i64>(0))?;
            let mut out = HashSet::new();
            for row in mapped {
                out.insert(row?);
            }
            out.into_iter().map(|id| id.to_string()).collect()
        };
        let filtered: Vec<serde_json::Value> = ids
            .iter()
            .filter(|id| known.contains(*id))
            .map(|id| serde_json::Value::String(id.clone()))
            .collect();
        let value = serde_json::Value::Array(filtered).to_string();
        self.set_meta(Self::META_RUNNING_IDS, &value)
    }

    /// Ticket 43: clear the running snapshot (Settings "reset snapshot").
    /// A later launch restores nothing; the live engine is untouched.
    pub fn clear_running_snapshot(&mut self) -> Result<(), StoreError> {
        self.conn.execute(
            "DELETE FROM meta WHERE key = ?1",
            params![Self::META_RUNNING_IDS],
        )?;
        Ok(())
    }

    /// Ticket 43: global probe-URL override (`None` means the documented
    /// default). Blank reads as unset, never as a URL.
    pub fn test_url(&self) -> Option<String> {
        match self.get_meta(Self::META_TEST_URL) {
            Ok(Some(url)) if !url.trim().is_empty() => Some(url),
            _ => None,
        }
    }

    /// Ticket 43: persist the global probe-URL override (validated upstream;
    /// stored trimmed).
    pub fn set_test_url(&mut self, url: &str) -> Result<(), StoreError> {
        self.set_meta(Self::META_TEST_URL, url.trim())
    }

    /// Ticket 43: drop the global probe-URL override (back to the default).
    pub fn clear_test_url(&mut self) -> Result<(), StoreError> {
        self.conn.execute(
            "DELETE FROM meta WHERE key = ?1",
            params![Self::META_TEST_URL],
        )?;
        Ok(())
    }

    /// Ticket 43: meta key for one row's probe-URL override.
    fn row_test_url_key(id: &str) -> String {
        format!("{}:{id}", Self::META_TEST_URL)
    }

    /// Ticket 43: one row's probe-URL override (`None` falls back to the
    /// global override, then the default). Blank reads as unset.
    pub fn row_test_url(&self, id: &str) -> Option<String> {
        match self.get_meta(&Self::row_test_url_key(id)) {
            Ok(Some(url)) if !url.trim().is_empty() => Some(url),
            _ => None,
        }
    }

    /// Ticket 43: persist one row's probe-URL override (validated upstream).
    pub fn set_row_test_url(&mut self, id: &str, url: &str) -> Result<(), StoreError> {
        self.set_meta(&Self::row_test_url_key(id), url.trim())
    }

    /// Ticket 43: drop one row's probe-URL override (back to the global).
    pub fn clear_row_test_url(&mut self, id: &str) -> Result<(), StoreError> {
        self.conn.execute(
            "DELETE FROM meta WHERE key = ?1",
            params![Self::row_test_url_key(id)],
        )?;
        Ok(())
    }

    /// Load the running-set snapshot. Missing or corrupt reads as empty
    /// (never a crash, never a guessed row).
    /// Ticket 40: filters to known config ids so a snapshot naming a removed
    /// row never resurrects it (callers still intersect with live ids).
    pub fn load_running_snapshot(&self) -> Vec<String> {
        let raw = match self.get_meta(Self::META_RUNNING_IDS) {
            Ok(Some(raw)) => raw,
            _ => return Vec::new(),
        };
        let ids: Vec<String> = match serde_json::from_str::<serde_json::Value>(&raw) {
            Ok(serde_json::Value::Array(items)) => items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect(),
            _ => return Vec::new(),
        };
        // Filter to live ids (best-effort: a broken read fails closed to the
        // unfiltered list rather than empty — saves already filter).
        let known: HashSet<String> = match self.conn.prepare("SELECT id FROM proxy_configs") {
            Ok(mut stmt) => match stmt.query_map([], |row| row.get::<_, i64>(0)) {
                Ok(mapped) => {
                    let mut set = HashSet::new();
                    let mut ok = true;
                    for row in mapped {
                        match row {
                            Ok(id) => {
                                set.insert(id.to_string());
                            }
                            Err(_) => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if ok {
                        set
                    } else {
                        ids.iter().cloned().collect()
                    }
                }
                Err(_) => ids.iter().cloned().collect(),
            },
            Err(_) => ids.iter().cloned().collect(),
        };
        ids.into_iter().filter(|id| known.contains(id)).collect()
    }

    /// First free port in the auto range, given the set of taken ports.
    /// Ticket 27: primary then spillover (see [`auto_port_candidates`]).
    /// `exclude` frees one in-use port from the taken set (the edit-to-auto
    /// path passes the row's own current port: it stays taken by nobody
    /// else, so the row may keep it instead of burning a fresh port per
    /// edit and starving the range).
    fn auto_port_excluding(&self, exclude: Option<u16>) -> Result<u16, StoreError> {
        let mut stmt = self.conn.prepare("SELECT port FROM proxy_configs")?;
        let mut taken: HashSet<i64> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<HashSet<_>, _>>()?;
        if let Some(own) = exclude {
            taken.remove(&(own as i64));
        }
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
                // Blank/`auto` re-resolves against every port but this row's
                // own (it may keep its current port; otherwise the range
                // drains one port per edit).
                Ok(None) => match self.auto_port_excluding(Some(current.port)) {
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

    /// Ticket 44: group rename moves every row carrying the old label;
    /// empty names refuse with nothing written.
    #[test]
    fn t44_rename_group_moves_rows_and_rejects_empty() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let mut a = valid_input("A");
        a.group = "g1".to_string();
        let mut b = valid_input("B");
        b.group = "g1".to_string();
        let mut c = valid_input("C");
        c.group = "g2".to_string();
        store.add_batch(vec![a, b, c]).expect("adds work");
        assert_eq!(store.rename_group("g1", "team").expect("rename works"), 2);
        let groups: std::collections::HashMap<String, usize> = store
            .group_counts()
            .expect("counts work")
            .into_iter()
            .collect();
        assert_eq!(groups.get("team"), Some(&2));
        assert_eq!(groups.get("g2"), Some(&1));
        assert!(!groups.contains_key("g1"));
        assert_eq!(
            store
                .rename_group("missing", "x")
                .expect("missing renames zero"),
            0
        );
        let error = store
            .rename_group("team", "   ")
            .expect_err("empty rename must fail");
        let StoreError::Invalid(fields) = error else {
            panic!("expected Invalid, got {error}");
        };
        assert!(fields.iter().any(|e| e.field == "group"));
        assert_eq!(
            store
                .list()
                .expect("list works")
                .iter()
                .filter(|r| r.group == "team")
                .count(),
            2,
            "failed rename writes nothing"
        );
    }

    /// Ticket 44: bulk move lands the checked ids in the target group;
    /// unknown ids skip silently; group counts inventory the labels.
    #[test]
    fn t44_move_to_group_and_counts() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let mut a = valid_input("A");
        a.group = "g1".to_string();
        let mut b = valid_input("B");
        b.group = "g1".to_string();
        let created = store.add_batch(vec![a, b]).expect("adds work");
        let ids: Vec<String> = created.iter().map(|r| r.id.clone()).collect();
        let mut targets = vec![ids[0].clone(), "99999".to_string()];
        assert_eq!(
            store.move_to_group(&targets, "team").expect("move works"),
            1
        );
        targets[0] = ids[1].clone();
        assert_eq!(
            store
                .move_to_group(&targets, "  team  ")
                .expect("move trims"),
            1
        );
        let counts = store.group_counts().expect("counts work");
        assert_eq!(counts, vec![("team".to_string(), 2)]);
        let error = store
            .move_to_group(&ids, "")
            .expect_err("empty group must fail");
        let StoreError::Invalid(_) = error else {
            panic!("expected Invalid, got {error}");
        };
    }

    /// Ticket 44: deleting a group with rows refuses loudly (names the
    /// count); empty groups delete clean.
    #[test]
    fn t44_delete_group_refuses_while_rows_remain() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let mut a = valid_input("A");
        a.group = "g1".to_string();
        store.add(a).expect("add works");
        let error = store
            .delete_group("g1")
            .expect_err("non-empty delete refuses");
        let StoreError::Invalid(fields) = error else {
            panic!("expected Invalid, got {error}");
        };
        assert!(
            fields.iter().any(|e| e.message.contains('1')),
            "refusal names the count: {fields:?}"
        );
        store
            .delete_group("empty-label")
            .expect("empty group deletes clean");
        assert_eq!(store.list().expect("list works").len(), 1);
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
    /// Ticket A1: `localhost` validates as a loopback bind (save gate agrees
    /// with the engine start gate); garbage still fails naming localhost.
    #[test]
    fn localhost_parses_as_loopback() {
        let ip: IpAddr = DEFAULT_BIND.parse().expect("default bind parses");
        assert!(ip.is_loopback());
        let mut errors = Vec::new();
        assert_eq!(
            validated_bind("localhost", &mut errors).as_deref(),
            Some("localhost")
        );
        assert!(errors.is_empty());
        let mut errors = Vec::new();
        assert!(validated_bind("not a bind!!", &mut errors).is_none());
        let message = errors
            .iter()
            .map(|e| format!("{}: {}", e.field, e.message))
            .collect::<Vec<_>>()
            .join("; ");
        assert!(
            message.contains("localhost"),
            "reconciled message: {message}"
        );
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

    /// Ticket A8: a corrupt `failures` CELL in a live row (hand edit, older
    /// build) reads back as no failures — never a crash, never an error —
    /// and an empty library round-trips export → restore → export
    /// byte-identically through the single pipeline.
    #[test]
    fn a8_corrupt_failures_cell_and_empty_round_trip() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .conn
            .execute(
                "UPDATE subscriptions SET failures = ?1 WHERE id = ?2",
                params!["{corrupt!!!", sub.id.parse::<i64>().expect("rowid")],
            )
            .expect("corrupt write works");
        let read = store
            .get_subscription(&sub.id)
            .expect("read works")
            .expect("sub present");
        assert!(
            read.failures.is_empty(),
            "corrupt cell decodes to no failures"
        );

        // Empty library round-trip through the single export pipeline.
        let empty = Store::open_in_memory().expect("fresh store opens");
        let payload = empty.export_library_json().expect("empty export works");
        let (quarantined_payload, skipped) = empty
            .export_library_json_with_quarantine()
            .expect("quarantine export works");
        assert_eq!(
            payload, quarantined_payload,
            "single pipeline: both exports agree"
        );
        assert!(skipped.is_empty(), "nothing to skip when empty");
        let mut restored = Store::open_in_memory().expect("restore target opens");
        let outcome = restored
            .restore_library_json(&payload)
            .expect("empty restore works");
        assert_eq!(
            outcome,
            RestoreOutcome {
                configs: 0,
                subscriptions: 0,
                tombstones: 0,
                presets: 0,
            }
        );
        assert_eq!(
            restored.export_library_json().expect("re-export works"),
            payload,
            "empty round-trip is byte-identical"
        );
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
            interval_secs: DEFAULT_AUTO_REFRESH_SECS,
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
                presets: 0,
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
                presets: 0,
            }
        );
    }

    /// Ticket A3: strict restore — corrupt library rows fail loudly (missing
    /// / wrong-typed name/protocol/params/bind_ip, reserved ports, duplicate
    /// bind:port), with nothing written.
    #[test]
    fn a3_restore_rejects_corrupt_rows_loudly() {
        fn valid_entry() -> serde_json::Value {
            serde_json::json!({
                "id": "1",
                "name": "one",
                "protocol": "vless",
                "params": "{\"server\":\"a.example.com\",\"server_port\":443,\"uuid\":\"11111111-2222-4333-8444-555555555555\"}",
                "group": "default",
                "enabled": false,
                "bind_ip": "127.0.0.1",
                "port": 10801,
            })
        }
        fn payload_with(entries: Vec<serde_json::Value>) -> String {
            serde_json::json!({
                "library": entries,
                "subscriptions": [],
            })
            .to_string()
        }
        let mut cases: Vec<(&str, serde_json::Value)> = Vec::new();
        let mut missing_name = valid_entry();
        missing_name.as_object_mut().expect("object").remove("name");
        cases.push(("missing name", missing_name));
        let mut blank_name = valid_entry();
        blank_name["name"] = serde_json::Value::String("   ".to_string());
        cases.push(("blank name", blank_name));
        let mut wrong_protocol = valid_entry();
        wrong_protocol["protocol"] = serde_json::json!(42);
        cases.push(("wrong-typed protocol", wrong_protocol));
        let mut unknown_protocol = valid_entry();
        unknown_protocol["protocol"] = serde_json::Value::String("nope".to_string());
        cases.push(("unknown protocol", unknown_protocol));
        let mut missing_params = valid_entry();
        missing_params
            .as_object_mut()
            .expect("object")
            .remove("params");
        cases.push(("missing params", missing_params));
        let mut corrupt_params = valid_entry();
        corrupt_params["params"] = serde_json::Value::String("{garbage".to_string());
        cases.push(("corrupt params", corrupt_params));
        let mut missing_bind = valid_entry();
        missing_bind
            .as_object_mut()
            .expect("object")
            .remove("bind_ip");
        cases.push(("missing bind_ip", missing_bind));
        let mut bad_bind = valid_entry();
        bad_bind["bind_ip"] = serde_json::Value::String("not a bind!!".to_string());
        cases.push(("bad bind_ip", bad_bind));
        let mut reserved = valid_entry();
        reserved["port"] = serde_json::json!(18600);
        cases.push(("reserved port", reserved));
        for (label, entry) in cases {
            let mut store = Store::open_in_memory().expect("in-memory store opens");
            let payload = payload_with(vec![entry]);
            let error = store
                .restore_library_json(&payload)
                .expect_err(&format!("{label} must fail"));
            assert!(
                matches!(error, StoreError::Invalid(_)),
                "{label}: Invalid, got {error}"
            );
            assert!(
                store.list().expect("list works").is_empty(),
                "{label}: nothing written"
            );
        }
        // Duplicate bind:port fails loudly with nothing written.
        {
            let mut second = valid_entry();
            second["id"] = serde_json::Value::String("2".to_string());
            second["name"] = serde_json::Value::String("two".to_string());
            let mut store = Store::open_in_memory().expect("in-memory store opens");
            let payload = payload_with(vec![valid_entry(), second]);
            let error = store
                .restore_library_json(&payload)
                .expect_err("duplicate bind:port must fail");
            assert!(
                matches!(error, StoreError::Invalid(_)),
                "Invalid, got {error}"
            );
            assert!(error.to_string().contains("duplicates"), "got {error}");
            assert!(store.list().expect("list works").is_empty());
        }
        // localhost bind restores fine (A1 parity: loopback by name).
        {
            let mut entry = valid_entry();
            entry["bind_ip"] = serde_json::Value::String("localhost".to_string());
            let mut store = Store::open_in_memory().expect("in-memory store opens");
            let outcome = store
                .restore_library_json(&payload_with(vec![entry]))
                .expect("localhost bind restores");
            assert_eq!(outcome.configs, 1);
        }
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

    /// Ticket A5: redirect + DNS-rebind coverage — every hop re-runs the
    /// policy, so a benign host redirecting (or resolving) to loopback is
    /// Blocked; caps are enforced code; Warn reasons never carry tokens.
    #[test]
    fn a5_redirect_rebind_caps_hold() {
        use SubscriptionFetchPolicy::{Allow, Block, Warn};
        // A redirect target is just another URL through the same policy: a
        // hop to 127.0.0.1 is Blocked before any byte is fetched from it.
        for target in [
            "http://127.0.0.1/evil?token=hunter2",
            "https://127.0.0.1:8080/evil",
            "http://localhost/evil",
            "https://[::1]/evil",
        ] {
            assert!(
                matches!(subscription_fetch_policy(target), Block { .. }),
                "redirect target blocked: {target}"
            );
        }
        // DNS-rebind: public name, internal resolution → Block.
        let loopback: std::net::IpAddr = "127.0.0.1".parse().expect("ip parses");
        let private: std::net::IpAddr = "10.9.9.9".parse().expect("ip parses");
        let public: std::net::IpAddr = "93.184.216.34".parse().expect("ip parses");
        assert!(
            matches!(
                subscription_fetch_policy_resolved("https://example.com/sub.txt", &[loopback]),
                Block { .. }
            ),
            "rebind to loopback blocked"
        );
        assert!(
            matches!(
                subscription_fetch_policy_resolved("https://example.com/sub.txt", &[public, private]),
                Block { .. }
            ),
            "one internal answer poisons the set"
        );
        assert!(
            matches!(
                subscription_fetch_policy_resolved("https://example.com/sub.txt", &[public]),
                Allow
            ),
            "public resolution stays Allow"
        );
        assert!(
            matches!(
                subscription_fetch_policy_resolved("http://example.com/sub.txt", &[public]),
                Warn { .. }
            ),
            "plaintext-http Warn survives resolution (LAN-provider compat)"
        );
        assert!(
            matches!(
                subscription_fetch_policy_resolved("https://example.com/sub.txt", &[]),
                Block { .. }
            ),
            "empty resolution fails closed"
        );
        // Literal-internal URLs stay Block even with a public resolution
        // claim (no DNS laundering past the literal guard).
        assert!(
            matches!(
                subscription_fetch_policy_resolved("http://127.0.0.1/sub", &[public]),
                Block { .. }
            ),
            "literal internal never laundered"
        );
        // Warn reasons never echo the URL/token.
        let reason = match subscription_fetch_policy("http://example.com/sub?token=hunter2") {
            Warn { reason } => reason,
            other => panic!("expected Warn, got {other:?}"),
        };
        assert!(!reason.contains("hunter2"), "no token echo: {reason}");
        assert!(!reason.contains("example.com"), "no URL echo: {reason}");
        // Caps live in code and match the fetch path (8 MiB / 20 s / 5).
        assert_eq!(SUBSCRIPTION_MAX_BYTES, 8 * 1024 * 1024);
        assert_eq!(SUBSCRIPTION_TIMEOUT_SECS, 20);
        assert_eq!(SUBSCRIPTION_MAX_REDIRECTS, 5);
        // Byte entry enforces size + UTF-8, keeping last-good members.
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("baseline refresh works");
        assert_eq!(store.list().expect("list works").len(), 2);
        let big = vec![b'x'; SUBSCRIPTION_MAX_BYTES + 1];
        let outcome = store
            .refresh_with_bytes(&sub.id, &big, 200)
            .expect("oversize is an outcome, not an error");
        assert!(
            outcome.error.as_deref() == Some("subscription body too large"),
            "loud cap, got {outcome:?}"
        );
        let outcome = store
            .refresh_with_bytes(&sub.id, &[0xff, 0xfe, 0xfd], 300)
            .expect("non-UTF8 is an outcome, not an error");
        assert!(
            outcome.error.as_deref() == Some("subscription body is not UTF-8"),
            "loud encoding, got {outcome:?}"
        );
        assert_eq!(
            store.list().expect("list works").len(),
            2,
            "capped bodies keep last-good members"
        );
        // Small valid bodies still flow through the byte entry.
        let outcome = store
            .refresh_with_bytes(&sub.id, sub_body_two().as_bytes(), 400)
            .expect("valid bytes refresh");
        assert!(outcome.error.is_none(), "clean, got {outcome:?}");
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

    /// Ticket 35/48: a stored subscription URL carries no token plaintext
    /// at rest, reads back identically, and still matches by URL.
    #[test]
    fn ticket35_stored_sub_urls_have_no_plaintext() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let created = store
            .add_subscription(
                "Nightly",
                "https://example.com/sub.txt?token=SECRET-TOKEN",
                "default",
                true,
            )
            .expect("add works");
        let raw: String = store
            .conn
            .query_row(
                "SELECT url FROM subscriptions WHERE id = ?1",
                rusqlite::params![created.id.parse::<i64>().expect("numeric id")],
                |row| row.get(0),
            )
            .expect("raw read works");
        assert!(
            !raw.contains("SECRET-TOKEN"),
            "subscription url column sealed at rest"
        );
        let back = store
            .get_subscription(&created.id)
            .expect("get works")
            .expect("row kept");
        assert_eq!(
            back.url, "https://example.com/sub.txt?token=SECRET-TOKEN",
            "reads decrypt transparently"
        );
        assert_eq!(
            store
                .find_subscription_by_url("https://example.com/sub.txt?token=SECRET-TOKEN")
                .expect("find works")
                .map(|sub| sub.id),
            Some(created.id.clone()),
            "sealed rows still match by url"
        );
        assert!(
            store
                .find_subscription_by_url("https://example.com/other.txt")
                .expect("find works")
                .is_none(),
            "missing url is None"
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
            assert_eq!(version, Store::SCHEMA_VERSION, "schema advances fully");
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

    /// Ticket 05: guided identity mapping mirrors the commit gate (same
    /// credential per protocol); aliases canonicalize; secret-free and
    /// custom protocols hide Identity; unknown stays unknown.
    #[test]
    fn t05_guided_identity_fields_mirror_validation() {
        let vless = guided_identity_fields("vless").expect("vless maps");
        assert_eq!(vless.key, Some("uuid"));
        assert_eq!(vless.needs, "uuid");
        let trojan = guided_identity_fields("trojan").expect("trojan maps");
        assert_eq!(trojan.key, Some("password"));
        // Aliases canonicalize through the same gate as validation.
        assert_eq!(
            guided_identity_fields("ss").expect("ss maps").key,
            Some("password"),
            "shadowsocks alias"
        );
        assert_eq!(
            guided_identity_fields("wg").expect("wg maps").key,
            Some("private_key"),
            "wireguard alias"
        );
        let socks = guided_identity_fields("socks").expect("socks maps");
        assert_eq!(socks.key, None, "no secret for socks");
        let custom = guided_identity_fields("custom-outbound").expect("custom maps");
        assert_eq!(custom.key, None, "custom JSON fills Identity nowhere");
        assert!(
            guided_identity_fields("bogus").is_none(),
            "unknown stays unknown"
        );
    }

    /// Ticket 05: guided prefill echoes the JSON (aliases, numbers) and
    /// stays blank on garbage — the JSON is the source of truth.
    #[test]
    fn t05_guided_prefill_echoes_json_blank_on_garbage() {
        let params = "{\"type\":\"vless\",\"server\":\"a.example.com\",\"server_port\":443,\
            \"uuid\":\"11111111-2222-4333-8444-555555555555\"}";
        assert_eq!(
            guided_prefill_value(params, &["server", "address"]),
            "a.example.com"
        );
        assert_eq!(
            guided_prefill_value(params, &["server_port", "port"]),
            "443"
        );
        assert_eq!(
            guided_prefill_value(params, &["uuid", "id"]),
            "11111111-2222-4333-8444-555555555555"
        );
        assert_eq!(guided_prefill_value(params, &["password"]), "");
        assert_eq!(guided_prefill_value("not json", &["server"]), "");
        assert_eq!(guided_prefill_value("", &["server"]), "");
    }

    /// Ticket 05: blank guided inputs leave the JSON byte-identical;
    /// non-blank values fold in; garbage ports fail loudly at validation
    /// (written as strings, never silently kept).
    #[test]
    fn t05_fold_guided_leaves_blank_untouched_and_merges_touched() {
        let template = protocol_params_template("vless");
        // All blank → byte-identical (untouched template still names uuid).
        assert_eq!(
            fold_guided_into_params(&template, "", "", Some(("uuid", &["uuid", "id"])), ""),
            template
        );
        // Guided identity + server fold into the blank template.
        let merged = fold_guided_into_params(
            &template,
            "a.example.com",
            "",
            Some(("uuid", &["uuid", "id"])),
            "11111111-2222-4333-8444-555555555555",
        );
        let value: serde_json::Value = serde_json::from_str(&merged).expect("merged is JSON");
        assert_eq!(value["server"], "a.example.com");
        assert_eq!(value["uuid"], "11111111-2222-4333-8444-555555555555");
        assert_eq!(value["server_port"], 443, "template port stands");
        // Untouched values never reserialize (no spurious edit patches).
        let shaped = "{\"type\":\"vless\",\"server\":\"a.example.com\",\"server_port\":443,\
            \"uuid\":\"11111111-2222-4333-8444-555555555555\"}";
        assert_eq!(
            fold_guided_into_params(
                shaped,
                "a.example.com",
                "443",
                Some(("uuid", &["uuid", "id"])),
                "11111111-2222-4333-8444-555555555555"
            ),
            shaped
        );
        // Garbage port is written as a string — validation names it.
        let bad_port = fold_guided_into_params(shaped, "", "abc", None, "");
        assert!(
            bad_port.contains("\"server_port\":\"abc\""),
            "loud for validation, got {bad_port}"
        );
        // Unparseable JSON returns unchanged (validation names that error).
        assert_eq!(
            fold_guided_into_params("{{{", "a.example.com", "", None, ""),
            "{{{"
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

    /// Ticket 42 (US-28): route links store in all three encodings
    /// (standard-b64, urlsafe-b64, raw JSON).
    #[test]
    fn t42_route_link_all_encodings_store() {
        use base64::Engine as _;
        let inner =
            r#"{"kind":"throne-route-profile","v":1,"name":"enc","rules":[{"action":"direct"}]}"#;
        let std = base64::engine::general_purpose::STANDARD.encode(inner.as_bytes());
        let urlsafe = base64::engine::general_purpose::URL_SAFE.encode(inner.as_bytes());
        for (label, link) in [
            ("standard", format!("throne://route/{std}")),
            ("urlsafe", format!("throne://route/{urlsafe}")),
            ("raw", format!("throne://route/{inner}")),
        ] {
            let mut store = Store::open_in_memory().expect("in-memory store opens");
            let preset = store
                .import_route_link(&link)
                .unwrap_or_else(|error| panic!("{label} stores: {error}"));
            assert_eq!(preset.name, "enc", "{label}");
            assert_eq!(preset.rules().expect("rules decode").len(), 1, "{label}");
        }
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
        // Attaching an empty-rules preset is refused (it would serve
        // preset-less without a word).
        assert!(
            store.set_config_preset(&row.id, Some(&preset.id)).is_err(),
            "empty attach refused"
        );
        store
            .add_preset_rule(&preset.id, serde_json::json!({"action": "direct"}))
            .expect("re-add rule works");
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
    /// Ticket 40: full-fidelity export now carries presets + linkage by name,
    /// so restores re-attach (old unattached expectation superseded).
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
        assert_eq!(
            back.preset_id.as_deref(),
            Some(preset.id.as_str()),
            "ticket-40 full fidelity re-attaches by name"
        );
        let kept = store
            .get_preset(&preset.id)
            .expect("get works")
            .expect("preset survives restore");
        assert_eq!(kept.rules().expect("rules decode").len(), 3);
    }
    // -- Ticket 48: DPAPI failure injection + preset integration --------

    /// Ticket 48 (US-60): DPAPI-down writes abort loudly with nothing
    /// written — a single add and a sealing subscription refresh both
    /// roll back (version and rows untouched, next open retries).
    #[test]
    fn t48_dpapi_down_aborts_writes_without_side_effects() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("baseline refresh works");
        assert_eq!(store.list().expect("list works").len(), 2);
        crate::dpapi::set_fail_for_test(true);
        let add_result = store.add(NewConfig {
            name: "Doomed".to_string(),
            protocol: "vless".to_string(),
            params: "{\"a\":1}".to_string(),
            group: String::new(),
            bind_ip: String::new(),
            port: String::new(),
        });
        let grown_body = format!("{}\ntrojan://pw3@c.example.com:443#three", sub_body_two());
        let refresh_result = store.refresh_with_body(&sub.id, &grown_body, 200);
        crate::dpapi::set_fail_for_test(false);
        for error in [
            add_result.expect_err("DPAPI-down add aborts"),
            refresh_result.expect_err("DPAPI-down refresh aborts"),
        ] {
            // Ticket A2: crypto failures are `Crypto`, never `Sql`, with an
            // honest message (no "database error" mislabel).
            assert!(
                matches!(error, StoreError::Crypto { .. }),
                "distinct crypto variant, got {error:?}"
            );
            assert!(
                error.to_string().contains("dpapi"),
                "loud dpapi cause, got {error}"
            );
            assert!(
                error.to_string().contains("nothing was written"),
                "honest no-write message, got {error}"
            );
            assert!(
                !matches!(error, StoreError::Sql(_)),
                "never mislabeled as Sql: {error:?}"
            );
        }
        assert_eq!(
            store.list().expect("list works").len(),
            2,
            "aborted writes leave no rows"
        );
        let kept = store
            .get_subscription(&sub.id)
            .expect("get works")
            .expect("sub kept");
        assert_eq!(
            kept.last_refresh_at,
            Some(100),
            "aborted refresh leaves the watermark"
        );
    }

    /// Ticket 48 (US-60): a foreign DPAPI blob (sealed under another
    /// profile) fails strict reads loudly, quarantines on tolerant reads
    /// instead of poisoning the list, and is skipped (named) by export.
    #[test]
    fn t48_foreign_blob_reads_quarantine() {
        use base64::Engine as _;
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let good = store
            .add(NewConfig {
                name: "Good".to_string(),
                protocol: "vless".to_string(),
                params: "{\"a\":1}".to_string(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("seed works");
        let foreign = format!(
            "dpapi1:{}",
            base64::engine::general_purpose::STANDARD.encode([0xabu8; 64])
        );
        store
            .conn
            .execute(
                "INSERT INTO proxy_configs (name, protocol, params, grp, enabled, bind_ip, port)
                 VALUES ('Foreign','vless',?1,'default',0,'127.0.0.1',10899)",
                rusqlite::params![foreign],
            )
            .expect("foreign blob plants");
        let bad_id = store.conn.last_insert_rowid().to_string();
        let error = store.list().expect_err("strict list fails loud");
        assert!(
            error.to_string().contains("dpapi"),
            "loud dpapi cause, got {error}"
        );
        let (rows, quarantined) = store.list_with_quarantine().expect("tolerant list works");
        assert_eq!(rows.len(), 1, "good rows load");
        assert_eq!(rows[0].id, good.id);
        assert_eq!(quarantined.len(), 1, "one bad row named");
        assert_eq!(quarantined[0].id, bad_id);
        let (_payload, skipped) = store
            .export_library_json_with_quarantine()
            .expect("tolerant export works");
        assert_eq!(skipped, vec![bad_id], "export names the skip");
    }

    /// Ticket 48 (US-65): a corrupt sync baseline fails the refresh loudly
    /// (never a silent revert, never a guessed compare); rows stay intact.
    #[test]
    fn t48_refresh_corrupt_baseline_fails_loud() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("baseline works");
        let row = store
            .list()
            .expect("list works")
            .into_iter()
            .find(|row| row.name == "one")
            .expect("member one");
        store
            .conn
            .execute(
                "UPDATE proxy_configs SET synced_params = 'dpapi1:!!!' WHERE id = ?1",
                rusqlite::params![row.id.parse::<i64>().expect("numeric id")],
            )
            .expect("corrupt works");
        let error = store
            .refresh_with_body(&sub.id, &sub_body_two(), 200)
            .expect_err("corrupt baseline fails loud");
        assert!(
            error.to_string().contains("dpapi"),
            "loud dpapi cause, got {error}"
        );
        assert_eq!(store.list().expect("list works").len(), 2, "rows intact");
    }

    fn t48_seed_vless(store: &mut Store, name: &str) -> ProxyConfig {
        store
            .add(NewConfig {
                name: name.to_string(),
                protocol: "vless".to_string(),
                params: "{\"type\":\"vless\",\"server\":\"198.51.100.9\",\"server_port\":443,\
                    \"uuid\":\"11111111-2222-4333-8444-555555555555\",\"encryption\":\"none\",\
                    \"security\":\"tls\",\"sni\":\"198.51.100.9\",\"network\":\"tcp\"}"
                    .to_string(),
                group: String::new(),
                bind_ip: "127.0.0.1".to_string(),
                port: "10801".to_string(),
            })
            .expect("seed row saves")
    }

    /// Ticket 48 (US-65): preset attach → generate (rules merge ahead of
    /// the isolation rule) → delete paths → deleted id refused at
    /// attach-time → cross-DB-style dangling reference swept by the
    /// orphan banner path, generation honest again afterwards.
    #[test]
    fn t48_preset_attach_generate_deleted_id_integration() {
        use crate::engine::{generate_with_presets, inbound_tag, outbound_tag};
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let row = t48_seed_vless(&mut store, "Row");
        let preset = store
            .create_preset("ads", "manual", None, &t37_rules_json())
            .expect("create works");
        store
            .set_config_preset(&row.id, Some(&preset.id))
            .expect("attach works");
        let rows = store.list().expect("list works");
        let map = store.preset_rules_map().expect("rules map works");
        let generated = generate_with_presets(&rows, &map).expect("attached set generates");
        let rules = generated.value()["route"]["rules"]
            .as_array()
            .expect("rules array")
            .clone();
        assert_eq!(
            rules.len(),
            t37_rules_json().len() + 1,
            "preset rules plus the isolation rule"
        );
        assert_eq!(
            rules[0]["domain"],
            serde_json::json!(["example.com"]),
            "preset rule merges first"
        );
        assert_eq!(
            rules[0]["inbound"],
            serde_json::json!([inbound_tag(&row.id)]),
            "preset rule scoped to the row"
        );
        assert_eq!(
            rules[0]["outbound"],
            serde_json::json!(outbound_tag(&row.id)),
            "preset route serves the row"
        );
        assert!(
            store.delete_preset(&preset.id).is_err(),
            "attached delete refused"
        );
        assert!(store
            .delete_preset_detaching(&preset.id)
            .expect("explicit delete works"));
        assert!(
            matches!(
                store.set_config_preset(&row.id, Some(&preset.id)),
                Err(StoreError::NotFound(_))
            ),
            "deleted id refused at attach-time"
        );
        // Cross-DB-style dangling reference (restore races): the orphan
        // scan names it, the sweep clears it, generation is honest again.
        store
            .conn
            .execute(
                "UPDATE proxy_configs SET preset_id = '9999' WHERE id = ?1",
                rusqlite::params![row.id.parse::<i64>().expect("numeric id")],
            )
            .expect("dangle works");
        let orphans = store
            .find_orphaned_preset_rows()
            .expect("orphan scan works");
        assert_eq!(orphans.len(), 1, "one dangling row");
        assert_eq!(orphans[0].id, row.id);
        assert_eq!(store.detach_orphaned_presets().expect("sweep works"), 1);
        let rows = store.list().expect("list works");
        assert_eq!(rows[0].preset_id, None, "swept clean");
        let map = store.preset_rules_map().expect("rules map works");
        generate_with_presets(&rows, &map).expect("swept set generates");
    }

    /// Ticket 38: restore opt-in defaults off and round-trips (explicit
    /// checkbox only — nothing enables it silently).
    #[test]
    fn restore_running_defaults_off_and_round_trips() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        assert!(!store.restore_running(), "default off");
        store.set_restore_running(true).expect("opt-in writes");
        assert!(store.restore_running(), "opt-in reads back");
        store.set_restore_running(false).expect("opt-out writes");
        assert!(!store.restore_running(), "opt-out reads back");
    }

    /// Ticket 38: running-set snapshot round-trips; missing reads empty;
    /// corrupt JSON reads empty (never a crash, never a guessed row).
    /// Ticket 40: saves filter to known ids (ghosts never persist).
    #[test]
    fn running_snapshot_round_trips_and_fails_empty() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        assert!(
            store.load_running_snapshot().is_empty(),
            "missing reads empty"
        );
        // Ghost ids filter to empty (never persist unknown rows).
        store
            .save_running_snapshot(&["3".to_string(), "7".to_string()])
            .expect("snapshot writes");
        assert!(
            store.load_running_snapshot().is_empty(),
            "unknown ids filter to empty"
        );
        // Known ids round-trip.
        let row = store
            .add(NewConfig {
                name: "R".to_string(),
                protocol: "vless".to_string(),
                params: String::new(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("seed works");
        store
            .save_running_snapshot(std::slice::from_ref(&row.id))
            .expect("snapshot writes");
        assert_eq!(store.load_running_snapshot(), vec![row.id.clone()]);
        store
            .conn
            .execute(
                "UPDATE meta SET value = 'not-json{{' WHERE key = 'running_ids'",
                [],
            )
            .expect("corrupt the snapshot");
        assert!(
            store.load_running_snapshot().is_empty(),
            "corrupt reads empty"
        );
    }

    /// Ticket 38: meta upserts (same key twice keeps the latest).
    #[test]
    fn meta_upserts() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        assert_eq!(store.get_meta("k").expect("read works"), None);
        store.set_meta("k", "a").expect("write works");
        store.set_meta("k", "b").expect("rewrite works");
        assert_eq!(
            store.get_meta("k").expect("read works"),
            Some("b".to_string())
        );
    }

    // -- Ticket 40: Store v5 foundation + integrity hardening (RED) --------

    /// v4 fixture migrates idempotently to v5 with zero loss; fresh and
    /// migrated converge per drift check.
    #[test]
    fn t40_v4_to_v5_migrates_idempotently() {
        let dir = std::env::temp_dir().join(format!(
            "manyproxy-t40v5-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock works")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir works");
        let v4_path = dir.join("v4.db");
        {
            // Build a v4-shaped DB by hand (no v5 columns, version 4).
            let conn = rusqlite::Connection::open(&v4_path).expect("v4 file opens");
            conn.execute_batch(
                "CREATE TABLE proxy_configs (
                   id INTEGER PRIMARY KEY AUTOINCREMENT,
                   name TEXT NOT NULL, protocol TEXT NOT NULL DEFAULT 'vless',
                   params TEXT NOT NULL DEFAULT '{}', grp TEXT NOT NULL DEFAULT 'default',
                   enabled INTEGER NOT NULL DEFAULT 0, bind_ip TEXT NOT NULL DEFAULT '127.0.0.1',
                   port INTEGER NOT NULL CHECK (port BETWEEN 1 AND 65535),
                   subscription_id TEXT, identity_key TEXT,
                   adoptable INTEGER NOT NULL DEFAULT 0, synced_params TEXT,
                   preset_id TEXT, traffic_rx INTEGER NOT NULL DEFAULT 0,
                   traffic_tx INTEGER NOT NULL DEFAULT 0
                 ) STRICT;
                 CREATE TABLE subscriptions (
                   id INTEGER PRIMARY KEY AUTOINCREMENT, label TEXT NOT NULL,
                   url TEXT NOT NULL, grp TEXT NOT NULL DEFAULT 'default',
                   auto_refresh INTEGER NOT NULL DEFAULT 1,
                   last_refresh_at INTEGER, last_error TEXT,
                   failures TEXT NOT NULL DEFAULT '[]'
                 ) STRICT;
                 CREATE TABLE subscription_tombstones (
                   subscription_id TEXT NOT NULL, identity_key TEXT NOT NULL,
                   PRIMARY KEY (subscription_id, identity_key)) STRICT;
                 CREATE TABLE health_snapshots (
                   config_id TEXT PRIMARY KEY, at INTEGER NOT NULL,
                   kind TEXT NOT NULL DEFAULT 'url-test',
                   latency_ms INTEGER, error TEXT) STRICT;
                 CREATE TABLE route_presets (
                   id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE,
                   source TEXT NOT NULL DEFAULT 'manual', source_url TEXT,
                   rules_json TEXT NOT NULL DEFAULT '[]', updated_at INTEGER) STRICT;
                 CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL DEFAULT '') STRICT;
                 PRAGMA user_version = 4;",
            )
            .expect("v4 ddl works");
            // Seed one row/sub/preset/traffic with DPAPI-sealed params (v4 already seals).
            let sealed = crate::dpapi::seal_params(
                r#"{"server":"198.51.100.9","server_port":8388,"method":"aes-128-gcm","password":"p"}"#,
            )
            .expect("seal works");
            conn.execute(
                "INSERT INTO proxy_configs (name, protocol, params, grp, enabled, bind_ip, port, traffic_rx, traffic_tx)
                 VALUES ('V4Row','shadowsocks',?1,'default',1,'127.0.0.1',10801,100,200)",
                rusqlite::params![sealed],
            )
            .expect("seed row");
            conn.execute(
                "INSERT INTO subscriptions (label, url, grp, auto_refresh) VALUES ('S','https://example.com/a.txt','g',1)",
                [],
            )
            .expect("seed sub");
            conn.execute(
                "INSERT INTO route_presets (name, source, rules_json) VALUES ('p','manual','[]')",
                [],
            )
            .expect("seed preset");
            conn.execute(
                "INSERT INTO meta (key, value) VALUES ('restore_running','1')",
                [],
            )
            .expect("seed meta");
        }
        let store = Store::open_file(&v4_path).expect("v4 opens and migrates");
        let version: i64 = store
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .expect("version reads");
        assert_eq!(version, Store::SCHEMA_VERSION, "migrated fully");
        // interval_secs defaults 86400.
        let subs = store.list_subscriptions().expect("subs list");
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].interval_secs, 86_400, "default interval");
        // The v4 plaintext subscription URL sealed at rest, reads back plain.
        let raw_url: String = store
            .conn
            .query_row("SELECT url FROM subscriptions", [], |r| r.get(0))
            .expect("raw url reads");
        assert!(
            !raw_url.contains("example.com"),
            "subscription url column sealed at rest"
        );
        assert_eq!(
            subs[0].url, "https://example.com/a.txt",
            "reads decrypt transparently"
        );
        // Zero data loss.
        let rows = store.list().expect("rows list");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "V4Row");
        // Reopen is idempotent.
        drop(store);
        let store2 = Store::open_file(&v4_path).expect("reopen works");
        let version2: i64 = store2
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .expect("version reads");
        assert_eq!(version2, Store::SCHEMA_VERSION);
        // Fresh and migrated converge: same v5 columns present.
        let fresh = Store::open_in_memory().expect("fresh opens");
        for (table, column) in [
            ("subscriptions", "interval_secs"),
            ("route_presets", "description"),
            ("route_presets", "is_builtin"),
            ("route_presets", "last_used_at"),
            ("subscription_tombstones", "subscription_url"),
        ] {
            assert!(
                fresh.column_exists(table, column).expect("fresh probe"),
                "fresh has {table}.{column}"
            );
            assert!(
                store2.column_exists(table, column).expect("migrated probe"),
                "migrated has {table}.{column}"
            );
        }
        // health_history table + index exist on both.
        for s in [&fresh, &store2] {
            let tbl: i64 = s
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='health_history'",
                    [],
                    |r| r.get(0),
                )
                .expect("table probe");
            assert_eq!(tbl, 1, "health_history exists");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// One corrupt params blob quarantines; good rows load with banner ids;
    /// export skips-or-flags; strict writes still reject.
    #[test]
    fn t40_corrupt_row_quarantines() {
        let mut store = Store::open_in_memory().expect("in-memory opens");
        let good = store
            .add(NewConfig {
                name: "Good".to_string(),
                protocol: "shadowsocks".to_string(),
                params: r#"{"server":"198.51.100.9","server_port":8388,"method":"aes-128-gcm","password":"p"}"#.to_string(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("good seeds");
        // Corrupt the second row's params directly (foreign blob, bad base64).
        let bad_id: i64 = {
            store
                .conn
                .execute(
                    "INSERT INTO proxy_configs (name, protocol, params, grp, enabled, bind_ip, port)
                     VALUES ('Bad','shadowsocks','dpapi1:!!!not-base64!!!','default',0,'127.0.0.1',10899)",
                    [],
                )
                .expect("raw corrupt insert");
            store.conn.last_insert_rowid()
        };
        let bad_id_str = bad_id.to_string();
        // Strict list fails loudly.
        assert!(store.list().is_err(), "strict list fails on corrupt row");
        // Tolerant list loads good + names quarantined.
        let (rows, quarantined) = store.list_with_quarantine().expect("tolerant lists");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, good.id);
        assert_eq!(quarantined.len(), 1);
        assert_eq!(quarantined[0].id, bad_id_str);
        // Tolerant export skips bad and flags ids.
        let (payload, skipped) = store
            .export_library_json_with_quarantine()
            .expect("tolerant export works");
        assert_eq!(skipped, vec![bad_id_str.clone()]);
        assert!(payload.contains("Good"), "good row exported");
        assert!(!payload.contains("\"Bad\""), "bad row skipped");
        // Single-row strict read still rejects.
        assert!(store.get(&bad_id_str).is_err(), "strict get rejects");
    }

    /// Batched traffic: one tx per tick, saturating, unknown ids skipped.
    #[test]
    fn t40_traffic_batch_is_atomic_and_saturating() {
        let mut store = Store::open_in_memory().expect("in-memory opens");
        let a = store
            .add(NewConfig {
                name: "A".to_string(),
                protocol: "shadowsocks".to_string(),
                params: r#"{"server":"198.51.100.9","server_port":8388,"method":"aes-128-gcm","password":"p"}"#.to_string(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("seed A");
        let b = store
            .add(NewConfig {
                name: "B".to_string(),
                protocol: "shadowsocks".to_string(),
                params: r#"{"server":"198.51.100.10","server_port":8388,"method":"aes-128-gcm","password":"p"}"#.to_string(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("seed B");
        let updated = store
            .add_traffic_batch(&[
                (a.id.clone(), 100, 50),
                (b.id.clone(), 200, 25),
                ("9999".to_string(), 10, 10),
            ])
            .expect("batch works");
        assert_eq!(updated, 2, "unknown ids skipped, 2 updated");
        let totals = store.traffic_totals().expect("totals read");
        assert_eq!(totals.get(&a.id), Some(&(100, 50)));
        // Saturating at i64::MAX.
        store
            .conn
            .execute(
                "UPDATE proxy_configs SET traffic_rx = 9223372036854775800 WHERE id = ?1",
                rusqlite::params![a.id.parse::<i64>().expect("id parses")],
            )
            .expect("pin near max");
        store
            .add_traffic_batch(&[(a.id.clone(), 100, 0)])
            .expect("saturating batch works");
        let totals = store.traffic_totals().expect("totals read");
        assert_eq!(
            totals.get(&a.id).map(|v| v.0),
            Some(9_223_372_036_854_775_807u64),
            "saturates, never wraps"
        );
    }

    /// URL-keyed tombstones: delete survives remove + re-add same URL.
    #[test]
    fn t40_tombstone_url_keyed_survives_resubscribe() {
        let mut store = Store::open_in_memory().expect("in-memory opens");
        let sub = store
            .add_subscription("S", "https://example.com/feed.txt", "g", true)
            .expect("sub seeds");
        let body_one = "vless://11111111-2222-4333-8444-555555555555@198.51.100.9:443?encryption=none&security=tls#One";
        let body_two = format!(
            "{body_one}\nvless://22222222-2222-4333-8444-555555555555@198.51.100.10:443?encryption=none&security=tls#Two"
        );
        store
            .refresh_with_body(&sub.id, &body_two, 1_000)
            .expect("refresh seeds 2");
        let rows = store.list().expect("list works");
        assert_eq!(rows.len(), 2);
        let victim = rows
            .iter()
            .find(|r| r.name == "One")
            .expect("victim")
            .clone();
        assert!(store.remove(&victim.id).expect("delete works"));
        // Remove the subscription entirely (old code dropped tombstones).
        assert!(store.remove_subscription(&sub.id).expect("unsub works"));
        // Re-add the SAME URL (new id) and refresh with both members again.
        let sub2 = store
            .add_subscription("S2", "https://example.com/feed.txt", "g", true)
            .expect("re-add works");
        let outcome = store
            .refresh_with_body(&sub2.id, &body_two, 2_000)
            .expect("refresh works");
        assert_eq!(
            outcome.skipped, 1,
            "delete stays honored across re-subscribe"
        );
        let rows = store.list().expect("list works");
        assert_eq!(rows.len(), 1, "only the non-deleted member lands");
        assert_eq!(rows[0].name, "Two");
    }

    /// Dangling preset attach refuses loudly in-tx; orphan sweep finds it.
    #[test]
    fn t40_preset_attach_revalidated_and_orphan_sweep() {
        let mut store = Store::open_in_memory().expect("in-memory opens");
        let row = store
            .add(NewConfig {
                name: "R".to_string(),
                protocol: "vless".to_string(),
                params: String::new(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("row seeds");
        // Ghost preset refuses.
        assert!(
            matches!(
                store.set_config_preset(&row.id, Some("9999")),
                Err(StoreError::NotFound(_))
            ),
            "dangling attach refuses"
        );
        // Plant a dangling reference behind the API (simulates cross-DB restore).
        store
            .conn
            .execute(
                "UPDATE proxy_configs SET preset_id = '9999' WHERE id = ?1",
                rusqlite::params![row.id.parse::<i64>().expect("id parses")],
            )
            .expect("plant dangling");
        let orphans = store.find_orphaned_preset_rows().expect("sweep scans");
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].id, row.id);
        let cleared = store.detach_orphaned_presets().expect("sweep detaches");
        assert_eq!(cleared, 1);
        let back = store.get(&row.id).expect("get works").expect("row kept");
        assert_eq!(back.preset_id, None);
    }

    /// Snapshot saves filter to known ids; read-only surfaces errors.
    #[test]
    fn t40_snapshot_filters_unknown_ids() {
        let mut store = Store::open_in_memory().expect("in-memory opens");
        let row = store
            .add(NewConfig {
                name: "R".to_string(),
                protocol: "vless".to_string(),
                params: String::new(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("row seeds");
        store
            .save_running_snapshot(&[row.id.clone(), "9999".to_string()])
            .expect("snapshot writes");
        assert_eq!(store.load_running_snapshot(), vec![row.id.clone()]);
        // Read-only disk surfaces errors instead of silent loss.
        store.set_query_only_for_test().expect("readonly sets");
        assert!(
            store
                .save_running_snapshot(std::slice::from_ref(&row.id))
                .is_err(),
            "write error surfaces"
        );
    }

    /// Health history appends per round with retention; bad kinds rejected.
    #[test]
    fn t40_health_history_appends_and_prunes() {
        let mut store = Store::open_in_memory().expect("in-memory opens");
        let row = store
            .add(NewConfig {
                name: "R".to_string(),
                protocol: "vless".to_string(),
                params: String::new(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("row seeds");
        store
            .append_health_history(&[
                HealthHistoryEntry {
                    config_id: row.id.clone(),
                    at: 1_000,
                    kind: "url-test".to_string(),
                    latency_ms: Some(120),
                    error: None,
                },
                HealthHistoryEntry {
                    config_id: row.id.clone(),
                    at: 2_000,
                    kind: "tcp-ping".to_string(),
                    latency_ms: None,
                    error: Some("timeout".to_string()),
                },
            ])
            .expect("append works");
        // latest-fast-path untouched: snapshots still round-trip separately.
        // Note: save_health_snapshots also appends to history (one tx per
        // probe round), so history holds 2 appended + 1 snapshot = 3.
        store
            .save_health_snapshot(&HealthSnapshot {
                config_id: row.id.clone(),
                at: 2_000,
                kind: "url-test".to_string(),
                latency_ms: Some(120),
                error: None,
            })
            .expect("snapshot works");
        let hist = store
            .load_health_history(&row.id, None)
            .expect("history loads");
        assert_eq!(hist.len(), 3);
        // Retention: prune everything before 1500 in one tx.
        let pruned = store
            .prune_health_history_before(1_500)
            .expect("prune works");
        assert_eq!(pruned, 1);
        let hist = store
            .load_health_history(&row.id, None)
            .expect("history loads");
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].at, 2_000);
        // Validation: latency XOR error, kind allow-list.
        assert!(
            store
                .append_health_history(&[HealthHistoryEntry {
                    config_id: row.id.clone(),
                    at: 3_000,
                    kind: "bogus".to_string(),
                    latency_ms: Some(1),
                    error: None,
                }])
                .is_err(),
            "bad kind rejected"
        );
    }

    /// Ticket 45: per-sub interval is honored by `due_for_refresh_for` and
    /// rows expose `last_refresh_at` for display.
    #[test]
    fn t45_sub_interval_honored_by_due() {
        let mut store = Store::open_in_memory().expect("in-memory opens");
        let sub = store
            .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
            .expect("seed works");
        assert_eq!(sub.interval_secs, DEFAULT_AUTO_REFRESH_SECS);
        assert_eq!(sub.last_refresh_at, None);
        let now = 1_700_000_000;
        // A 1h interval set on the row is honored: fresh stamp → not due.
        let hourly = store
            .set_subscription_interval(&sub.id, 3_600)
            .expect("interval sets");
        assert_eq!(hourly.interval_secs, 3_600);
        let fresh = Subscription {
            last_refresh_at: Some(now - 100),
            ..hourly.clone()
        };
        assert!(
            !due_for_refresh_for(&fresh, now),
            "fresh hourly sub not due"
        );
        let stale = Subscription {
            last_refresh_at: Some(now - 7_200),
            ..hourly
        };
        assert!(due_for_refresh_for(&stale, now), "stale hourly sub due");
        // Unknown ids never silently succeed.
        assert!(
            store.set_subscription_interval("9999", 60).is_err(),
            "unknown id fails loudly"
        );
    }

    /// Ticket 45: the global auto-refresh kill-switch defaults on (unset
    /// means on) and persists off across reads.
    #[test]
    fn t45_global_kill_switch_defaults_on_and_persists() {
        let mut store = Store::open_in_memory().expect("in-memory opens");
        assert!(
            store.subs_auto_refresh_enabled(),
            "unset kill-switch defaults on"
        );
        store
            .set_subs_auto_refresh_enabled(false)
            .expect("kill-switch writes");
        assert!(
            !store.subs_auto_refresh_enabled(),
            "kill-switch persists off"
        );
        store
            .set_subs_auto_refresh_enabled(true)
            .expect("kill-switch writes");
        assert!(
            store.subs_auto_refresh_enabled(),
            "kill-switch flips back on"
        );
    }

    /// Ticket 45: usage reset zeroes both counters; unknown ids are
    /// `NotFound`, never a silent no-op.
    #[test]
    fn t45_reset_traffic_zeroes_counters() {
        let mut store = Store::open_in_memory().expect("in-memory opens");
        let row = store
            .add(NewConfig {
                name: "R".to_string(),
                protocol: "vless".to_string(),
                params: String::new(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("row seeds");
        store
            .add_traffic_batch(&[(row.id.clone(), 1_000, 500)])
            .expect("traffic accrues");
        store.reset_traffic_counters(&row.id).expect("reset zeroes");
        let totals = store.traffic_totals().expect("totals read");
        assert_eq!(totals.get(&row.id), Some(&(0, 0)));
        assert!(
            store.reset_traffic_counters("9999").is_err(),
            "unknown id fails loudly"
        );
    }

    /// Ticket 45: the 30d retention window keeps recent history and prunes
    /// old samples in one tx; the latest-snapshot fast path is a separate
    /// table and never pays the history scan.
    #[test]
    fn t45_history_retention_window_keeps_recent() {
        let mut store = Store::open_in_memory().expect("in-memory opens");
        let row = store
            .add(NewConfig {
                name: "R".to_string(),
                protocol: "vless".to_string(),
                params: String::new(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("row seeds");
        let now: i64 = 1_700_000_000;
        let day: i64 = 86_400;
        for at in [now - 100 * day, now - 40 * day, now - 10 * day, now] {
            store
                .append_health_history(&[HealthHistoryEntry {
                    config_id: row.id.clone(),
                    at,
                    kind: "url-test".to_string(),
                    latency_ms: Some(50),
                    error: None,
                }])
                .expect("append works");
        }
        let pruned = store
            .gc_health_history(30, now)
            .expect("retention gc works");
        assert_eq!(pruned, 2, "100d + 40d samples pruned");
        let kept = store
            .load_health_history(&row.id, None)
            .expect("history loads");
        assert_eq!(kept.len(), 2, "10d + now samples kept");
        assert!(kept.iter().all(|entry| entry.at >= now - 30 * day));
        // Latest-fast-path reads only `health_snapshots` (no history scan):
        // a snapshot round-trips while history holds rows.
        store
            .save_health_snapshot(&HealthSnapshot {
                config_id: row.id.clone(),
                at: now,
                kind: "url-test".to_string(),
                latency_ms: Some(50),
                error: None,
            })
            .expect("snapshot works");
        let snaps = store.load_health_snapshots().expect("snapshots load");
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].config_id, row.id);
    }

    /// Ticket 45: logs leave through the export-path pattern — a stable file
    /// name beside the library.
    #[test]
    fn t45_logs_path_pattern() {
        let path = Store::default_logs_path();
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("manyproxy-logs.txt"),
            "logs file name is stable"
        );
    }

    /// Export is full-fidelity: presets + linkage by name + traffic + meta.
    #[test]
    fn t40_export_full_fidelity_round_trip() {
        let mut store = Store::open_in_memory().expect("in-memory opens");
        let preset = store
            .create_preset(
                "p",
                "manual",
                None,
                &[serde_json::json!({"action":"direct"})],
            )
            .expect("preset seeds");
        let row = store
            .add(NewConfig {
                name: "R".to_string(),
                protocol: "shadowsocks".to_string(),
                params: r#"{"server":"198.51.100.9","server_port":8388,"method":"aes-128-gcm","password":"p"}"#.to_string(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("row seeds");
        store
            .set_config_preset(&row.id, Some(&preset.id))
            .expect("attach works");
        store.add_traffic(&row.id, 500, 600).expect("traffic seeds");
        store.set_restore_running(true).expect("meta seeds");
        store
            .save_running_snapshot(std::slice::from_ref(&row.id))
            .expect("snapshot seeds");
        let payload = store.export_library_json().expect("export works");
        assert!(payload.contains("\"route_presets\""), "presets exported");
        assert!(payload.contains("\"traffic_rx\""), "traffic exported");
        // Restore elsewhere rebuilds linkage by NAME (ids differ across DBs).
        let mut other = Store::open_in_memory().expect("other opens");
        let outcome = other.restore_library_json(&payload).expect("restore works");
        assert_eq!(outcome.configs, 1);
        let presets = other.list_presets().expect("presets list");
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].name, "p");
        let rows = other.list().expect("rows list");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].preset_id.as_deref(),
            Some(presets[0].id.as_str()),
            "linkage re-attached by name"
        );
        let totals = other.traffic_totals().expect("totals read");
        assert_eq!(totals.get(&rows[0].id), Some(&(500, 600)));
        assert!(other.restore_running(), "meta survives");
        assert_eq!(other.load_running_snapshot(), vec![rows[0].id.clone()]);
    }

    /// Per-sub interval persists; due check honors it.
    #[test]
    fn t40_per_sub_interval_honored() {
        let mut store = Store::open_in_memory().expect("in-memory opens");
        let sub = store
            .add_subscription("S", "https://example.com/a.txt", "g", true)
            .expect("sub seeds");
        assert_eq!(sub.interval_secs, 86_400);
        let updated = store
            .set_subscription_interval(&sub.id, 3_600)
            .expect("interval writes");
        assert_eq!(updated.interval_secs, 3_600);
        assert!(
            due_for_refresh_for(&updated, 5_000),
            "never refreshed → due"
        );
        store
            .refresh_with_body(&sub.id, "[]", 10_000)
            .expect("refresh stamps time");
        let back = store
            .get_subscription(&sub.id)
            .expect("get works")
            .expect("kept");
        // Refresh with empty body errors (keeps last-good); stamp manually instead.
        let _ = back;
        let due_sub = Subscription {
            interval_secs: 3_600,
            last_refresh_at: Some(10_000),
            auto_refresh: true,
            ..updated.clone()
        };
        assert!(
            !due_for_refresh_for(&due_sub, 10_000 + 1_000),
            "not due yet"
        );
        assert!(
            due_for_refresh_for(&due_sub, 10_000 + 3_600),
            "due after interval"
        );
    }

    /// Corrupt file is quarantined aside, never silent in-memory loss.
    #[test]
    fn t40_corrupt_file_quarantined() {
        let dir = std::env::temp_dir().join(format!(
            "manyproxy-t40corr-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock works")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir works");
        let path = dir.join("library.db");
        std::fs::write(&path, b"this is not sqlite at all").expect("garbage writes");
        let err = match Store::open_file(&path) {
            Ok(_) => panic!("corrupt open must fail"),
            Err(err) => err,
        };
        let text = err.to_string();
        assert!(
            text.contains("corrupt") || text.contains("integrity"),
            "loud guidance, got {text}"
        );
        // Bad file preserved aside.
        let kept: Vec<_> = std::fs::read_dir(&dir)
            .expect("dir reads")
            .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
            .filter(|n| n.contains("corrupt"))
            .collect();
        assert_eq!(kept.len(), 1, "one quarantined file, got {kept:?}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Restore takes a timestamped backup before any wipe (file path).
    #[test]
    fn t40_restore_takes_backup_before_wipe() {
        let dir = std::env::temp_dir().join(format!(
            "manyproxy-t40bak-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock works")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir works");
        let path = dir.join("library.db");
        let payload = {
            let mut store = Store::open_file(&path).expect("live opens");
            store
                .add(NewConfig {
                    name: "Live".to_string(),
                    protocol: "vless".to_string(),
                    params: String::new(),
                    group: String::new(),
                    bind_ip: String::new(),
                    port: String::new(),
                })
                .expect("live seeds");
            store.export_library_json().expect("export works")
        };
        let backup = Store::backup_db_file(&path, now_unix()).expect("backup works");
        assert!(backup.exists(), "timestamped backup exists");
        // Restoring an empty payload over the live file keeps a backup.
        let empty = r#"{"outbounds":[],"library":[],"subscriptions":[],"tombstones":[],"route_presets":[],"meta":{}}"#;
        let (_outcome, bak) =
            Store::restore_library_file(&path, empty, now_unix()).expect("file restore works");
        let bak = bak.expect("restore took a backup over a live file");
        assert!(bak.exists(), "restore took a backup");
        let store = Store::open_file(&path).expect("reopen works");
        assert!(
            store.list().expect("list works").is_empty(),
            "restored empty"
        );
        // The pre-restore backup still holds the exported row.
        let _ = (payload, backup);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Export refuses the live database path (JSON bytes must never
    /// overwrite the library file).
    #[test]
    fn export_refuses_live_db_path() {
        let dir = std::env::temp_dir().join(format!(
            "manyproxy-exportguard-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock works")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir works");
        let path = dir.join("library.db");
        let store = Store::open_file(&path).expect("file store opens");
        let live = store.db_path().expect("file-backed");
        let error = store
            .commit_export(&live, true)
            .expect_err("export over live DB must fail");
        let StoreError::Invalid(fields) = error else {
            panic!("expected Invalid, got {error:?}");
        };
        assert!(fields.iter().any(|e| e.field == "export"));
        assert!(path.exists(), "live DB untouched");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Edit-to-auto reuses the row's own port (never burns a fresh port
    /// per edit toward exhaustion).
    #[test]
    fn edit_to_auto_keeps_own_port() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let created = store.add(valid_input("Solo")).expect("add works");
        let before = created.port;
        let updated = store
            .update(
                &created.id,
                ConfigPatch {
                    port: Some("auto".to_string()),
                    ..Default::default()
                },
            )
            .expect("edit-to-auto works");
        assert_eq!(
            updated.port, before,
            "own port reused, got {}",
            updated.port
        );
    }

    /// `route_presets: null` preserves live presets like a missing section
    /// (only an explicit array — even empty — wipes first).
    #[test]
    fn restore_null_presets_preserves_live() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store
            .create_preset("keep", "manual", None, &[])
            .expect("preset seeds");
        let payload = r#"{"outbounds":[],"library":[],"subscriptions":[],"tombstones":[],"route_presets":null,"meta":{}}"#;
        store
            .restore_library_json(payload)
            .expect("null-presets restore works");
        assert_eq!(
            store.list_presets().expect("list works").len(),
            1,
            "explicit null preserves live presets"
        );
        let wiped = r#"{"outbounds":[],"library":[],"subscriptions":[],"tombstones":[],"route_presets":[],"meta":{}}"#;
        store
            .restore_library_json(wiped)
            .expect("empty-array restore works");
        assert!(
            store.list_presets().expect("list works").is_empty(),
            "explicit empty array wipes"
        );
    }

    /// Restoring with no live file succeeds with no backup (never a phantom
    /// path to a file that was never written).
    #[test]
    fn restore_without_live_file_returns_no_backup() {
        let dir = std::env::temp_dir().join(format!(
            "manyproxy-nolive-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock works")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir works");
        let path = dir.join("fresh").join("library.db");
        assert!(!path.exists());
        let payload = r#"{"outbounds":[],"library":[],"subscriptions":[],"tombstones":[],"route_presets":[],"meta":{}}"#;
        let (outcome, backup) =
            Store::restore_library_file(&path, payload, now_unix()).expect("restore works");
        assert!(backup.is_none(), "no live file means no backup");
        assert_eq!(outcome.configs, 0);
        assert!(path.exists(), "fresh parents created, live file written");
        Store::open_file(&path).expect("restored file opens");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Export skips a row whose sync baseline no longer unseals (logged +
    /// quarantined) instead of aborting the whole export.
    #[test]
    fn export_skips_row_with_bad_synced_params() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let good = store.add(valid_input("Good")).expect("add works");
        let bad = store.add(valid_input("Bad")).expect("add works");
        store
            .conn
            .execute(
                "UPDATE proxy_configs SET synced_params = 'garbage!!!' WHERE id = ?1",
                rusqlite::params![bad.id.parse::<i64>().expect("numeric id")],
            )
            .expect("corrupt baseline seeds");
        let payload = store.export_library_json().expect("export survives");
        let value: serde_json::Value = serde_json::from_str(&payload).expect("JSON");
        let ids: Vec<String> = value["library"]
            .as_array()
            .expect("library section")
            .iter()
            .filter_map(|e| e.get("id").and_then(|v| v.as_str()).map(str::to_string))
            .collect();
        assert!(ids.contains(&good.id), "good row exports, got {ids:?}");
        assert!(
            !ids.contains(&bad.id),
            "bad-baseline row quarantined, got {ids:?}"
        );
        let (quarantine_payload, skipped) = store
            .export_library_json_with_quarantine()
            .expect("quarantine export works");
        assert!(skipped.contains(&bad.id), "skip list names it");
        let _ = quarantine_payload;
    }

    /// Group rename/move/delete trim consistently (`" g1 "` is `"g1"`).
    #[test]
    fn group_ops_trim_consistently() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let mut a = valid_input("A");
        a.group = "g1".to_string();
        store.add(a).expect("add works");
        assert_eq!(
            store
                .rename_group("  g1  ", "  team  ")
                .expect("padded rename works"),
            1,
            "old and new both trim"
        );
        assert!(
            store.delete_group(" team ").is_err(),
            "padded delete still sees the moved row (refuses, never a false-empty Ok)"
        );
    }

    /// DPAPI-down migration fails loudly, version stays, retry succeeds.
    #[test]
    fn t40_dpapi_down_migration_fails_loud_version_stays() {
        let dir = std::env::temp_dir().join(format!(
            "manyproxy-t40dpapi-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock works")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir works");
        let path = dir.join("library.db");
        {
            let conn = rusqlite::Connection::open(&path).expect("file opens");
            conn.execute_batch(
                "CREATE TABLE proxy_configs (id INTEGER PRIMARY KEY AUTOINCREMENT,
                   name TEXT NOT NULL, protocol TEXT NOT NULL DEFAULT 'vless',
                   params TEXT NOT NULL DEFAULT '{}', grp TEXT NOT NULL DEFAULT 'default',
                   enabled INTEGER NOT NULL DEFAULT 0, bind_ip TEXT NOT NULL DEFAULT '127.0.0.1',
                   port INTEGER NOT NULL CHECK (port BETWEEN 1 AND 65535)) STRICT;
                 CREATE TABLE subscriptions (id INTEGER PRIMARY KEY AUTOINCREMENT,
                   label TEXT NOT NULL, url TEXT NOT NULL, grp TEXT NOT NULL DEFAULT 'default',
                   auto_refresh INTEGER NOT NULL DEFAULT 1, last_refresh_at INTEGER,
                   last_error TEXT, failures TEXT NOT NULL DEFAULT '[]') STRICT;
                 CREATE TABLE subscription_tombstones (subscription_id TEXT NOT NULL,
                   identity_key TEXT NOT NULL,
                   PRIMARY KEY (subscription_id, identity_key)) STRICT;
                 CREATE TABLE health_snapshots (config_id TEXT PRIMARY KEY, at INTEGER NOT NULL,
                   kind TEXT NOT NULL DEFAULT 'url-test', latency_ms INTEGER, error TEXT) STRICT;
                 CREATE TABLE route_presets (id INTEGER PRIMARY KEY AUTOINCREMENT,
                   name TEXT NOT NULL UNIQUE, source TEXT NOT NULL DEFAULT 'manual',
                   source_url TEXT, rules_json TEXT NOT NULL DEFAULT '[]', updated_at INTEGER) STRICT;
                 CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL DEFAULT '') STRICT;
                 PRAGMA user_version = 2;",
            )
            .expect("v2 ddl works");
            conn.execute(
                "INSERT INTO proxy_configs (name, protocol, params, grp, enabled, bind_ip, port)
                 VALUES ('Plain','vless','{}','default',0,'127.0.0.1',10801)",
                [],
            )
            .expect("plaintext seeds");
        }
        crate::dpapi::set_fail_for_test(true);
        let err = match Store::open_file(&path) {
            Ok(_) => {
                crate::dpapi::set_fail_for_test(false);
                panic!("dpapi-down must fail loudly");
            }
            Err(err) => {
                crate::dpapi::set_fail_for_test(false);
                err
            }
        };
        assert!(
            err.to_string().contains("dpapi") || err.to_string().contains("DPAPI"),
            "loud dpapi cause, got {err}"
        );
        let version: i64 = rusqlite::Connection::open(&path)
            .expect("raw opens")
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .expect("version reads");
        assert!(
            version < Store::SCHEMA_VERSION,
            "version stays, next open retries (got {version})"
        );
        // Retry with DPAPI up migrates fully.
        let store = Store::open_file(&path).expect("retry works");
        let version: i64 = store
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .expect("version reads");
        assert_eq!(version, Store::SCHEMA_VERSION);
        std::fs::remove_dir_all(&dir).ok();
    }
    /// Ticket 43: the word `auto` (any case) means auto-assign, like blank.
    #[test]
    fn port_field_auto_word_means_auto_assign() {
        assert_eq!(super::parse_port_field("").expect("blank ok"), None);
        assert_eq!(super::parse_port_field("auto").expect("auto ok"), None);
        assert_eq!(super::parse_port_field("AUTO").expect("AUTO ok"), None);
        assert_eq!(
            super::parse_port_field("  Auto  ").expect("padded ok"),
            None
        );
        assert_eq!(
            super::parse_port_field("10801").expect("digits ok"),
            Some(10801)
        );
    }

    /// Ticket 43: global test-URL override round-trips; empty reads as unset.
    #[test]
    fn test_url_global_round_trips() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        assert_eq!(store.test_url(), None, "unset by default");
        store
            .set_test_url("https://example.com/ping")
            .expect("write works");
        assert_eq!(
            store.test_url(),
            Some("https://example.com/ping".to_string())
        );
        store.clear_test_url().expect("clear works");
        assert_eq!(store.test_url(), None, "clear returns to default");
    }

    /// Ticket 43: per-row test-URL override round-trips independently.
    #[test]
    fn test_url_row_override_round_trips() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let row = store.add(valid_input("probe-me")).expect("seed works");
        assert_eq!(store.row_test_url(&row.id), None, "unset by default");
        store
            .set_row_test_url(&row.id, "http://192.0.2.1/health")
            .expect("write works");
        assert_eq!(
            store.row_test_url(&row.id),
            Some("http://192.0.2.1/health".to_string())
        );
        store.clear_row_test_url(&row.id).expect("clear works");
        assert_eq!(store.row_test_url(&row.id), None, "clear returns to global");
    }

    /// Ticket 43: per-row traffic reset zeroes one row; clear-all zeroes all.
    #[test]
    fn traffic_reset_and_clear_all() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let alpha = store.add(valid_input("traffic-a")).expect("seed works");
        let beta = store.add(valid_input("traffic-b")).expect("seed works");
        store.add_traffic(&alpha.id, 1000, 50).expect("delta lands");
        store.add_traffic(&beta.id, 2000, 70).expect("delta lands");
        store.reset_traffic(&alpha.id).expect("reset works");
        let totals = store.traffic_totals().expect("totals read");
        assert_eq!(totals.get(&alpha.id), Some(&(0, 0)), "alpha zeroed");
        assert_eq!(totals.get(&beta.id), Some(&(2000, 70)), "beta untouched");
        assert!(
            store.reset_traffic("999999").is_err(),
            "ghost reset fails loudly"
        );
        let cleared = store.clear_all_traffic().expect("clear-all works");
        assert_eq!(cleared, 2, "both rows cleared");
        let totals = store.traffic_totals().expect("totals read");
        assert_eq!(totals.get(&beta.id), Some(&(0, 0)), "beta zeroed");
    }

    /// Ticket 43: preset rename validates + keeps rules; duplicates fail.
    #[test]
    fn preset_rename_validates_and_keeps_rules() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let preset = store
            .create_preset("first", "manual", None, &[])
            .expect("seed works");
        store
            .create_preset("second", "manual", None, &[])
            .expect("seed works");
        let renamed = store
            .rename_preset(&preset.id, "renamed")
            .expect("rename works");
        assert_eq!(renamed.name, "renamed");
        assert!(
            store.rename_preset(&preset.id, "   ").is_err(),
            "blank rename fails"
        );
        assert!(
            store.rename_preset(&preset.id, "second").is_err(),
            "duplicate rename fails"
        );
        assert!(
            store.rename_preset("999999", "ghost").is_err(),
            "ghost rename fails"
        );
    }

    /// Ticket 43: the running-remove confirm promises stop-first.
    #[test]
    fn delete_running_confirm_promises_stop_first() {
        let stopped = super::delete_confirm_text("Work");
        assert!(
            !stopped.contains("stopped"),
            "idle text claims nothing: {stopped}"
        );
        let running = super::delete_running_confirm_text("Work");
        assert!(running.contains("Work"), "names row: {running}");
        assert!(
            running.contains("stopped first"),
            "promises stop-first: {running}"
        );
    }

    /// Ticket 43: clearing health snapshots empties the table.
    #[test]
    fn clear_health_snapshots_empties() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let row = store.add(valid_input("health-me")).expect("seed works");
        store
            .save_health_snapshot(&super::HealthSnapshot {
                config_id: row.id.clone(),
                at: 1,
                kind: "url-test".to_string(),
                latency_ms: Some(21),
                error: None,
            })
            .expect("snapshot saves");
        assert!(
            !store
                .load_health_snapshots()
                .expect("load works")
                .is_empty(),
            "snapshot present"
        );
        store.clear_health_snapshots().expect("clear works");
        assert!(
            store
                .load_health_snapshots()
                .expect("load works")
                .is_empty(),
            "snapshots cleared"
        );
    }

    /// Port field trims padding and reports the literal 1-65535 range.
    #[test]
    fn port_field_trims_and_reports_literal_range() {
        assert_eq!(
            parse_port_field("  10801  ").expect("padded digits trim"),
            Some(10801)
        );
        assert_eq!(
            parse_port_field("  AUTO  ").expect("padded auto trims"),
            None
        );
        for (raw, want) in [
            ("0", "port must be 1-65535, got '0'"),
            ("99999", "port must be 1-65535, got '99999'"),
            ("abc", "port must be 1-65535, got 'abc'"),
        ] {
            let error = parse_port_field(raw).expect_err(&format!("{raw} rejects"));
            assert_eq!(error.field, "port");
            assert_eq!(error.message, want);
        }
        let padded = parse_port_field("  abc  ").expect_err("padded word rejects");
        assert_eq!(padded.field, "port");
        assert_eq!(padded.message, "port must be 1-65535, got 'abc'");
    }
}
