//! Ticket 02: ProxyConfig library store.
//!
//! rusqlite-backed persistence for the config library. Every connection gets
//! the sqlite-skill PRAGMAs (WAL, busy_timeout, FK, synchronous, temp_store),
//! all queries are parameterized, and unit tests run on `:memory:` databases.
//! The store owns all validation so UI dialogs pass raw strings straight
//! through and render the returned field-level errors.

use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::path::{Path, PathBuf};

/// Default loopback bind for new configs (spec: loopback default, LAN only
/// behind an explicit warning at activation time).
pub const DEFAULT_BIND: &str = "127.0.0.1";
/// Default group for new configs.
pub const DEFAULT_GROUP: &str = "default";
/// Default protocol when the add form leaves it blank.
pub const DEFAULT_PROTOCOL: &str = "vless";
/// Inclusive auto-port range for configs created without an explicit port.
pub const AUTO_PORT_START: u16 = 10801;
pub const AUTO_PORT_END: u16 = 10999;

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

/// Ticket 11: pure payload builder behind [`Store::export_library_json`]
/// (tests build payloads without a store). `outbounds` holds each
/// import-shaped row's params object verbatim (object with
/// `type`/`protocol` + `server`/`address` + `server_port`/`port`); rows
/// whose params are not import-shaped stay metadata-only so they never
/// poison the import path with per-item failures. `library` keeps every
/// row's display/endpoint metadata for backup; `subscriptions` keeps the
/// subscription records. The import grammar reads only `outbounds`, so a
/// re-import of this payload commits duplicates, never adds.
pub fn export_payload(configs: &[ProxyConfig], subscriptions: &[Subscription]) -> String {
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
                "name": config.name,
                "protocol": config.protocol,
                "group": config.group,
                "bind_ip": config.bind_ip,
                "port": config.port,
                "subscription_id": config.subscription_id,
            })
        })
        .collect();
    let subs: Vec<serde_json::Value> = subscriptions
        .iter()
        .map(|sub| {
            serde_json::json!({
                "label": sub.label,
                "url": sub.url,
                "group": sub.group,
                "auto_refresh": sub.auto_refresh,
            })
        })
        .collect();
    serde_json::json!({
        "outbounds": outbounds,
        "library": library,
        "subscriptions": subs,
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
fn map_snapshot_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HealthSnapshot> {
    let latency_ms: Option<i64> = row.get(3)?;
    Ok(HealthSnapshot {
        config_id: row.get(0)?,
        at: row.get(1)?,
        kind: row.get(2)?,
        latency_ms: latency_ms.map(|ms| ms as u64),
        error: row.get(4)?,
    })
}

/// Parse raw port text from a form field. Empty means auto-assign (`None`).
/// `Some(0)` never validates: it is rejected as out of range.
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
pub fn proxy_urls(bind_ip: &str, port: u16) -> (String, String) {
    (
        format!("socks5://{bind_ip}:{port}"),
        format!("http://{bind_ip}:{port}"),
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
fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProxyConfig> {
    let id: i64 = row.get(0)?;
    let enabled: i64 = row.get(5)?;
    let port: i64 = row.get(7)?;
    Ok(ProxyConfig {
        id: id.to_string(),
        name: row.get(1)?,
        protocol: row.get(2)?,
        params: row.get(3)?,
        group: row.get(4)?,
        enabled: enabled != 0,
        bind_ip: row.get(6)?,
        port: port as u16,
        subscription_id: row.get(8)?,
        identity_key: row.get(9)?,
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
                    format!("unknown protocol '{protocol_trimmed}'"),
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
            let index = item
                .get("index")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize;
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
        self.conn
            .execute_batch("DROP TABLE proxy_configs; DROP TABLE subscriptions;")?;
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
    pub fn open_file(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    StoreError::Sql(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
                })?;
            }
        }
        Self::open(Connection::open(path)?)
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
        let store = Self { conn };
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

    fn create_schema(&self) -> Result<(), StoreError> {
        // `grp` avoids the reserved word GROUP. Ports are intentionally NOT
        // unique: two stored configs may share a port; the conflict error
        // naming both parties is raised at activation time (ticket 03).
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS proxy_configs (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               name TEXT NOT NULL,
               protocol TEXT NOT NULL DEFAULT 'vless',
               params TEXT NOT NULL DEFAULT '{}',
               grp TEXT NOT NULL DEFAULT 'default',
               enabled INTEGER NOT NULL DEFAULT 0,
               bind_ip TEXT NOT NULL DEFAULT '127.0.0.1',
               port INTEGER NOT NULL,
               subscription_id TEXT,
               identity_key TEXT
             ) STRICT;
             CREATE INDEX IF NOT EXISTS idx_proxy_configs_enabled
               ON proxy_configs(enabled);
             CREATE INDEX IF NOT EXISTS idx_proxy_configs_subscription
               ON proxy_configs(subscription_id);
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
             ) STRICT;",
        )?;
        // Migrate pre-ticket-04 databases that lack the new columns.
        for (column, ddl) in [("subscription_id", "TEXT"), ("identity_key", "TEXT")] {
            let mut stmt = self
                .conn
                .prepare("SELECT name FROM pragma_table_info('proxy_configs') WHERE name = ?1")?;
            let exists: bool = stmt.exists(params![column])?;
            if !exists {
                self.conn.execute_batch(&format!(
                    "ALTER TABLE proxy_configs ADD COLUMN {column} {ddl};"
                ))?;
            }
        }
        // Ticket 17: migrate pre-ticket-17 databases that lack the per-item
        // refresh failures column.
        {
            let mut stmt = self.conn.prepare(
                "SELECT name FROM pragma_table_info('subscriptions') WHERE name = 'failures'",
            )?;
            if !stmt.exists(params![])? {
                self.conn.execute_batch(
                    "ALTER TABLE subscriptions ADD COLUMN failures TEXT NOT NULL DEFAULT '[]';",
                )?;
            }
        }
        Ok(())
    }

    /// Create a disabled config; invalid input is rejected with field errors.
    pub fn add(&mut self, input: NewConfig) -> Result<ProxyConfig, StoreError> {
        let validated = self.validate_new(&input)?;
        self.conn.execute(
            "INSERT INTO proxy_configs
               (name, protocol, params, grp, enabled, bind_ip, port)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
            params![
                validated.name,
                validated.protocol,
                validated.params,
                validated.group,
                validated.bind_ip,
                validated.port as i64,
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get(&id.to_string())?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
    }

    /// All configs in stable id order.
    pub fn list(&self) -> Result<Vec<ProxyConfig>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, protocol, params, grp, enabled, bind_ip, port,
              subscription_id, identity_key
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
              subscription_id, identity_key
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
    pub fn update(&mut self, id: &str, patch: ConfigPatch) -> Result<ProxyConfig, StoreError> {
        let current = self
            .get(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))?;
        let merged = self.validate_patch(&current, &patch)?;
        self.conn.execute(
            "UPDATE proxy_configs
             SET name = ?1, protocol = ?2, params = ?3, grp = ?4, bind_ip = ?5, port = ?6
             WHERE id = ?7",
            params![
                merged.name,
                merged.protocol,
                merged.params,
                merged.group,
                merged.bind_ip,
                merged.port as i64,
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
    pub fn remove(&mut self, id: &str) -> Result<bool, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Ok(false),
        };
        let current = self.get(id)?;
        let removed = self
            .conn
            .execute("DELETE FROM proxy_configs WHERE id = ?1", params![rowid])?;
        if removed > 0 {
            if let Some(row) = current {
                if let (Some(subscription_id), Some(identity_key)) =
                    (row.subscription_id, row.identity_key)
                {
                    self.conn.execute(
                        "INSERT OR IGNORE INTO subscription_tombstones (subscription_id, identity_key)
                         VALUES (?1, ?2)",
                        params![subscription_id, identity_key],
                    )?;
                }
            }
            self.conn.execute(
                "DELETE FROM health_snapshots WHERE config_id = ?1",
                params![id],
            )?;
        }
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
        let trimmed = raw.trim();
        let after_scheme = if trimmed.len() >= 7 && trimmed[..7].eq_ignore_ascii_case("http://") {
            &trimmed[7..]
        } else if trimmed.len() >= 8 && trimmed[..8].eq_ignore_ascii_case("https://") {
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
    pub fn remove_subscription(&mut self, id: &str) -> Result<bool, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Ok(false),
        };
        if self.get_subscription(id)?.is_none() {
            return Ok(false);
        }
        self.conn.execute(
            "UPDATE proxy_configs SET subscription_id = NULL, identity_key = NULL
             WHERE subscription_id = ?1",
            params![id],
        )?;
        self.conn.execute(
            "DELETE FROM subscription_tombstones WHERE subscription_id = ?1",
            params![id],
        )?;
        let removed = self
            .conn
            .execute("DELETE FROM subscriptions WHERE id = ?1", params![rowid])?;
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
    /// rows keyed by [`crate::import::normalized_identity`], preserves the
    /// hand-edit overlay (name/enabled/bind/port), tombstones user-deleted
    /// rows, removes only subscription-owned vanished rows, and applies
    /// atomically per group. Unparseable bodies keep the last-good members
    /// and record `last_error`. Refresh never enables or starts configs.
    pub fn refresh_with_body(
        &mut self,
        sub_id: &str,
        body: &str,
        now: i64,
    ) -> Result<RefreshOutcome, StoreError> {
        use crate::import::{import_subscription_body, normalized_identity, ImportedConfig};
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
        // Desired set, first-wins; extras count as duplicates.
        let mut desired: HashMap<String, ImportedConfig> = HashMap::new();
        let mut duplicates = 0usize;
        for config in batch.configs {
            let key = normalized_identity(&config.protocol, &config.params);
            if desired.insert(key, config).is_some() {
                duplicates += 1;
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
        let row_key = |row: &ProxyConfig| -> String {
            row.identity_key
                .clone()
                .unwrap_or_else(|| normalized_identity(&row.protocol, &row.params))
        };
        let mut global_by_key: HashMap<String, &ProxyConfig> = HashMap::new();
        for row in &rows {
            global_by_key.entry(row_key(row)).or_insert(row);
        }
        let owned: Vec<&ProxyConfig> = rows
            .iter()
            .filter(|row| row.subscription_id.as_deref() == Some(subscription.id.as_str()))
            .collect();
        let mut owned_by_key: HashMap<String, &ProxyConfig> = HashMap::new();
        for row in &owned {
            owned_by_key.entry(row_key(row)).or_insert(row);
        }
        enum Op {
            Insert {
                key: String,
                config: ImportedConfig,
                port: u16,
            },
            Update {
                id: String,
                protocol: String,
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
        let mut taken: HashSet<i64> = rows.iter().map(|row| row.port as i64).collect();
        let mut next_free_port = || -> Option<u16> {
            for port in AUTO_PORT_START..=AUTO_PORT_END {
                if !taken.contains(&(port as i64)) {
                    taken.insert(port as i64);
                    return Some(port);
                }
            }
            None
        };
        for (key, config) in &desired {
            if tombstones.contains(key) {
                outcome.skipped += 1;
                continue;
            }
            if let Some(current) = owned_by_key.get(key) {
                if current.protocol != config.protocol
                    || current.params != config.params
                    || current.group != subscription.group
                {
                    ops.push(Op::Update {
                        id: current.id.clone(),
                        protocol: config.protocol.clone(),
                        params: config.params.clone(),
                    });
                    outcome.updated += 1;
                }
                continue;
            }
            if global_by_key.contains_key(key) {
                outcome.duplicates += 1;
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
                        index: 0,
                        label: crate::import::redact_link(&config.name),
                        reason: "port-exhausted: no automatic port left in the auto range"
                            .to_string(),
                        code: crate::import::ImportErrorKind::BadPort,
                    });
                }
            }
        }
        for row in &owned {
            if !desired.contains_key(&row_key(row)) {
                ops.push(Op::Delete { id: row.id.clone() });
                outcome.removed += 1;
            }
        }
        let tx = self.conn.transaction()?;
        for op in ops {
            match op {
                Op::Insert { key, config, port } => {
                    tx.execute(
                        "INSERT INTO proxy_configs
                           (name, protocol, params, grp, enabled, bind_ip, port,
                            subscription_id, identity_key)
                         VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?7, ?8)",
                        params![
                            config.name,
                            config.protocol,
                            config.params,
                            subscription.group,
                            DEFAULT_BIND,
                            port as i64,
                            subscription.id,
                            key,
                        ],
                    )?;
                }
                Op::Update {
                    id,
                    protocol,
                    params,
                } => {
                    let rowid: i64 = id.parse().map_err(|_| {
                        StoreError::Invalid(vec![FieldError::new("id", "bad config id")])
                    })?;
                    tx.execute(
                        "UPDATE proxy_configs SET protocol = ?1, params = ?2, grp = ?3
                         WHERE id = ?4",
                        params![protocol, params, subscription.group, rowid],
                    )?;
                }
                Op::Delete { id } => {
                    let rowid: i64 = id.parse().map_err(|_| {
                        StoreError::Invalid(vec![FieldError::new("id", "bad config id")])
                    })?;
                    tx.execute("DELETE FROM proxy_configs WHERE id = ?1", params![rowid])?;
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
    pub fn export_library_json(&self) -> Result<String, StoreError> {
        Ok(export_payload(&self.list()?, &self.list_subscriptions()?))
    }

    /// Write the export file only when `confirmed` is true. Cancel (`false`)
    /// performs no I/O and returns `Ok(None)`; confirm writes the payload
    /// and returns the path. Errors never echo file contents (no secrets in
    /// logs); only the destination path is named.
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
        std::fs::write(path, payload).map_err(|error| StoreError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        Ok(Some(path.to_path_buf()))
    }

    // -- Ticket 12: health snapshot persistence ------------------------------

    /// Persist one config's latest sample (upsert keyed by `config_id`).
    /// Called after every recorded probe outcome; restart rehydrates from
    /// [`Store::load_health_snapshots`]. Error text here is a probe cause
    /// (already redacted at the health seam), never raw secrets.
    pub fn save_health_snapshot(&mut self, snapshot: &HealthSnapshot) -> Result<(), StoreError> {
        let latency_ms: Option<i64> = snapshot.latency_ms.map(|ms| ms as i64);
        self.conn.execute(
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
    fn auto_port(&self) -> Result<u16, StoreError> {
        let mut stmt = self.conn.prepare("SELECT port FROM proxy_configs")?;
        let taken: HashSet<i64> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<HashSet<_>, _>>()?;
        for port in AUTO_PORT_START..=AUTO_PORT_END {
            if !taken.contains(&(port as i64)) {
                return Ok(port);
            }
        }
        Err(StoreError::Invalid(vec![FieldError::new(
            "port",
            "no automatic port left in the auto range",
        )]))
    }

    /// Validate fresh input, applying defaults and auto port assignment.
    /// Ticket 16: protocol/params validate like the import grammar (unknown
    /// protocol / non-JSON / missing per-protocol credential → field error).
    fn validate_new(&self, input: &NewConfig) -> Result<ValidatedConfig, StoreError> {
        let mut errors = Vec::new();
        let name = validated_name(&input.name, &mut errors);
        let proto_params = validate_protocol_params(&input.protocol, &input.params, &mut errors);
        let group = defaulted(&input.group, DEFAULT_GROUP);
        let bind_ip = validated_bind(&input.bind_ip, &mut errors);
        let port = match parse_port_field(&input.port) {
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
        };
        if !errors.is_empty() {
            return Err(StoreError::Invalid(errors));
        }
        let (protocol, params) =
            proto_params.unwrap_or((DEFAULT_PROTOCOL.to_string(), "{}".to_string()));
        Ok(ValidatedConfig {
            name: name.unwrap_or_default(),
            protocol,
            params,
            group,
            bind_ip: bind_ip.unwrap_or_default(),
            port: port.unwrap_or(AUTO_PORT_START),
        })
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
            },
        ];
        assert_eq!(count_importable(&rows), 1);
    }
}
