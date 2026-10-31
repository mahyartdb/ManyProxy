//! The SQLite store: schema, queries, DPAPI-sealed secrets and the data dir.
//! Belongs: every row/subscription/settings read and write plus their error types.
//! Not here: UI state, engine processes, dialog strings.
use crate::secret::Secret;
use rusqlite::{params, Connection};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};

pub const DEFAULT_BIND: &str = "127.0.0.1";

pub const DEFAULT_GROUP: &str = "default";

pub const DEFAULT_PROTOCOL: &str = "vless";

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
    "direct",
    "mieru",
    "juicity",
    "trusttunnel",
];

pub const AUTO_PORT_START: u16 = 10801;
pub const AUTO_PORT_END: u16 = 10999;

pub const AUTO_PORT_SPILL_START: u16 = 18081;
pub const AUTO_PORT_SPILL_END: u16 = 18380;

/// T-04: the probe timeout's default, in seconds. It mirrors
/// `health::URL_TEST_TIMEOUT`, and a test pins the two together.
pub const DEFAULT_PROBE_TIMEOUT_SECS: u64 = 3;
/// T-04: the shortest probe timeout a user may pick. One second is the floor
/// below which a slow endpoint could never answer at all.
pub const MIN_PROBE_TIMEOUT_SECS: u64 = 1;
/// T-04: the longest probe timeout a user may pick.
pub const MAX_PROBE_TIMEOUT_SECS: u64 = 60;

/// T-04: what the window's close button does.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CloseAction {
    /// Hide the window to the tray; the proxies keep running.
    #[default]
    Tray,
    /// Stop every running profile and exit the app.
    Quit,
}

impl CloseAction {
    /// The value written to the store's meta table.
    pub fn stored(self) -> &'static str {
        match self {
            CloseAction::Tray => "tray",
            CloseAction::Quit => "quit",
        }
    }

    /// Reads a stored value back. Anything unknown is `None`, so the caller
    /// falls back to the default instead of guessing.
    pub fn from_stored(raw: &str) -> Option<Self> {
        match raw.trim() {
            "tray" => Some(CloseAction::Tray),
            "quit" => Some(CloseAction::Quit),
            _ => None,
        }
    }

    /// The label the Settings radio shows.
    pub fn label(self) -> &'static str {
        match self {
            CloseAction::Tray => "Close to tray",
            CloseAction::Quit => "Quit the app",
        }
    }

    /// The one-line explanation under the selected radio.
    pub fn explanation(self) -> &'static str {
        match self {
            CloseAction::Tray => "The window closes and the proxies keep running in the tray.",
            CloseAction::Quit => "Quitting stops every running profile and exits ManyProxy.",
        }
    }
}

/// T-04: which rows a "Test all" run covers.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TestAllScope {
    /// Every running endpoint — the behaviour the app shipped with.
    #[default]
    Running,
    /// Only the endpoints whose rows are checked.
    Checked,
}

impl TestAllScope {
    pub fn stored(self) -> &'static str {
        match self {
            TestAllScope::Running => "running",
            TestAllScope::Checked => "checked",
        }
    }

    pub fn from_stored(raw: &str) -> Option<Self> {
        match raw.trim() {
            "running" => Some(TestAllScope::Running),
            "checked" => Some(TestAllScope::Checked),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            TestAllScope::Running => "Every running endpoint",
            TestAllScope::Checked => "Checked endpoints only",
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
pub const AUTO_PORT_CAPACITY: usize = (AUTO_PORT_END - AUTO_PORT_START + 1) as usize
    + (AUTO_PORT_SPILL_END - AUTO_PORT_SPILL_START + 1) as usize;

pub fn auto_port_candidates() -> impl Iterator<Item = u16> {
    (AUTO_PORT_START..=AUTO_PORT_END).chain(AUTO_PORT_SPILL_START..=AUTO_PORT_SPILL_END)
}

/// T-05: how many bind attempts the auto-assignment's probe makes. The
/// engine's own bind probe retries TEN times with a sleep between them; the
/// store cannot afford that — a slow add is its own defect — so it tries
/// ONCE and moves to the next candidate.
pub const AUTO_PORT_PROBE_ATTEMPTS: u32 = 1;

/// T-05: the auto-assignment asks the SOCKET, not only the DB. The DB knows
/// which ports are ASSIGNED; the socket knows which are HELD. A port
/// another process holds is skipped, so a new row never lands on a port
/// that is already busy.
///
/// `taken` carries the ports the DB already assigns (the picked port joins
/// it). The walk is bounded by the auto range itself, and every candidate
/// costs one bind attempt.
fn take_free_auto_port(
    candidates: impl Iterator<Item = u16>,
    taken: &mut HashSet<i64>,
) -> Result<u16, &'static str> {
    let mut held = false;
    for port in candidates {
        if taken.contains(&(port as i64)) {
            continue;
        }
        if crate::engine::probe_bind_available_with(DEFAULT_BIND, port, AUTO_PORT_PROBE_ATTEMPTS)
            .is_err()
        {
            held = true;
            continue;
        }
        taken.insert(port as i64);
        return Ok(port);
    }
    Err(if held {
        "no automatic port left in the auto range (every free port is held by another process)"
    } else {
        "no automatic port left in the auto range"
    })
}

pub fn is_auto_port(port: u16) -> bool {
    (AUTO_PORT_START..=AUTO_PORT_END).contains(&port)
        || (AUTO_PORT_SPILL_START..=AUTO_PORT_SPILL_END).contains(&port)
}

#[cfg(any(test, feature = "test-support"))]
pub const SECRETS_AT_REST_NOTE: &str = "secrets at rest: row params DPAPI-sealed (user scope) plus user-only file permissions (0600/ACL); OS-keychain portability beyond DPAPI user scope is backlog";

pub fn harden_file_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(windows)]
    {
        let principal = current_user_sid()
            .map(|sid| format!("*{sid}"))
            .unwrap_or_else(|| "OWNER RIGHTS".to_string());
        let mut revoke = std::process::Command::new("icacls");
        revoke.arg(path).arg("/inheritance:r");
        crate::spawn_hidden(&mut revoke);
        let _ = revoke.output();
        let mut grant = std::process::Command::new("icacls");
        grant
            .arg(path)
            .arg("/grant:r")
            .arg(format!("{principal}:(R,W)"));
        crate::spawn_hidden(&mut grant);
        let _ = grant.output();
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
    }
}

#[cfg(windows)]
fn current_user_sid() -> Option<String> {
    let mut cmd = std::process::Command::new("whoami");
    cmd.arg("/user").arg("/fo").arg("csv");
    crate::spawn_hidden(&mut cmd);
    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.split(|c: char| c == '"' || c == ',' || c.is_whitespace())
        .map(str::trim)
        .find(|token| is_sid_shape(token))
        .map(str::to_string)
}

#[cfg(windows)]
fn is_sid_shape(token: &str) -> bool {
    token.len() > 4
        && token.starts_with("S-1-")
        && token[4..].chars().all(|c| c.is_ascii_digit() || c == '-')
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscriptionFetchPolicy {
    Allow,
    Warn { reason: &'static str },
    Block { reason: &'static str },
}

pub fn subscription_url_host(url: &str) -> Option<String> {
    subscription_url_parts(url.trim()).map(|(_, host)| host.to_string())
}

fn subscription_url_parts(trimmed: &str) -> Option<(&'static str, &str)> {
    let (scheme, after) = match trimmed.get(..7) {
        Some(prefix) if prefix.eq_ignore_ascii_case("http://") => ("http", &trimmed[7..]),
        _ => match trimmed.get(..8) {
            Some(prefix) if prefix.eq_ignore_ascii_case("https://") => ("https", &trimmed[8..]),
            _ => return None,
        },
    };
    let authority = after.split('/').next().unwrap_or(after);
    let host_part = authority.rsplit('@').next().unwrap_or(authority);
    let host = if let Some(rest) = host_part.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        host_part.split(':').next().unwrap_or(host_part)
    };
    let host = host.trim();
    if host.is_empty() {
        return None;
    }
    Some((scheme, host))
}

pub fn subscription_fetch_policy(url: &str) -> SubscriptionFetchPolicy {
    const HTTP_WARN: &str =
        "subscription uses plaintext http (tokens in the URL leak in plaintext; prefer https)";
    const INTERNAL_BLOCK: &str =
        "subscription targets an internal host (loopback/private/link-local are never fetched; import local files manually)";
    let Some((scheme, host)) = subscription_url_parts(url.trim()) else {
        return SubscriptionFetchPolicy::Block {
            reason: "subscription url must use http(s)",
        };
    };
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

pub const SUBSCRIPTION_MAX_BYTES: usize = 8 * 1024 * 1024;

pub const SUBSCRIPTION_TIMEOUT_SECS: u64 = 20;

pub const SUBSCRIPTION_MAX_REDIRECTS: usize = 5;

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
    if resolved.iter().any(|ip| is_internal_ip(*ip)) {
        return SubscriptionFetchPolicy::Block {
            reason: "subscription resolves to an internal host (loopback/private/link-local are never fetched; import local files manually)",
        };
    }
    base
}

fn is_internal_host(host: &str) -> bool {
    let lower = host.trim().to_ascii_lowercase();
    let bare = lower.strip_suffix('.').unwrap_or(&lower);
    if bare == "localhost" || bare.ends_with(".localhost") {
        return true;
    }
    if let Ok(ip) = bare.parse::<IpAddr>() {
        return is_internal_ip(ip);
    }
    match parse_bare_ipv4(bare) {
        Some(short) => is_internal_ip(IpAddr::V4(short)),
        None => false,
    }
}

fn is_internal_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_internal_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_internal_v4(mapped);
            }
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                || (v6.segments()[0] & 0xfe00) == 0xfc00
        }
    }
}

fn is_internal_v4(v4: Ipv4Addr) -> bool {
    let octets = v4.octets();
    v4.is_loopback()
        || v4.is_unspecified()
        || v4.is_link_local()
        || octets[0] == 10
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 168)
        || octets[0] == 127
}

fn parse_bare_ipv4(host: &str) -> Option<Ipv4Addr> {
    if host.is_empty()
        || !host
            .chars()
            .all(|c| c.is_ascii_hexdigit() || matches!(c, '.' | 'x' | 'X'))
    {
        return None;
    }
    let parts: Vec<&str> = host.split('.').collect();
    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    let count = parts.len();
    let mut addr: u32 = 0;
    for (index, part) in parts.iter().enumerate() {
        let value = parse_ipv4_part(part)?;
        let (shift, bits) = if index + 1 == count {
            (0, 8 * (4 - index))
        } else {
            (8 * (3 - index), 8)
        };
        let max = if bits >= 32 {
            u32::MAX
        } else {
            (1u32 << bits) - 1
        };
        if value > max {
            return None;
        }
        addr |= value << shift;
    }
    Some(Ipv4Addr::from(addr))
}

fn parse_ipv4_part(part: &str) -> Option<u32> {
    if part.is_empty() {
        return None;
    }
    if let Some(hex) = part.strip_prefix("0x").or_else(|| part.strip_prefix("0X")) {
        if hex.is_empty() || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        return u32::from_str_radix(hex, 16).ok();
    }
    if part.len() > 1 && part.starts_with('0') {
        if !part.chars().all(|c| ('0'..='7').contains(&c)) {
            return None;
        }
        return u32::from_str_radix(part, 8).ok();
    }
    if !part.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    part.parse::<u32>().ok()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyConfig {
    pub id: String,
    pub name: String,
    pub protocol: String,

    pub params: Secret,
    pub group: String,
    pub enabled: bool,
    pub bind_ip: String,
    pub port: u16,

    pub subscription_id: Option<String>,

    pub identity_key: Option<String>,

    pub preset_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewConfig {
    pub name: String,
    pub protocol: String,
    pub params: Secret,
    pub group: String,
    pub bind_ip: String,

    pub port: String,
}

#[derive(Debug, Clone, Default)]
pub struct ConfigPatch {
    pub name: Option<String>,
    pub protocol: Option<String>,
    pub params: Option<Secret>,
    pub group: Option<String>,
    pub bind_ip: Option<String>,
    pub port: Option<String>,
}

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

#[derive(Debug)]
pub enum StoreError {
    Invalid(Vec<FieldError>),

    NotFound(String),

    Io { path: PathBuf, message: String },

    Crypto { message: String },
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

/// T-22: export envelope identity. A future build reads these two fields to
/// decide whether an export is ingestible instead of deep-failing inside
/// restore. The format stays explicit-confirm JSON.
pub const LIBRARY_EXPORT_SCHEMA: &str = "manyproxy-library";
pub const LIBRARY_SCHEMA_VERSION: i64 = 1;

const LIBRARY_ENVELOPE_KEYS: [&str; 4] = ["schema", "schema_version", "created", "checksum"];

/// T-22: the envelope keys are metadata, not payload, so the checksum covers
/// the payload object without them. Rebuilt key by key because `serde_json`'s
/// `preserve_order` feature is live in this build (gpui-kit pulls it in) and
/// `Map::remove` there is a swap-remove that would reshuffle the payload.
fn envelope_free_payload(obj: &serde_json::Map<String, serde_json::Value>) -> serde_json::Value {
    let payload = obj
        .iter()
        .filter(|(key, _)| !LIBRARY_ENVELOPE_KEYS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    serde_json::Value::Object(payload)
}

/// T-22: canonical form of a payload — object keys sorted at every level, so
/// the checksum does not depend on key order or whitespace. `serde_json` is
/// *not* order-stable here (`preserve_order` is active), so the payload is
/// canonicalized explicitly instead of trusting its serializer.
fn canonical_json(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            out.push('{');
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::Value::String((*key).clone()).to_string());
                out.push(':');
                canonical_json(&map[key.as_str()], out);
            }
            out.push('}');
        }
        serde_json::Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                canonical_json(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// T-22: FNV-1a 64 over the canonical payload bytes, rendered as 16 hex digits.
/// An integrity cue, not a crypto claim — same constitution as the cap-name
/// hash in `preset.rs`.
fn export_checksum(payload: &serde_json::Value) -> String {
    let mut text = String::new();
    canonical_json(payload, &mut text);
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// T-22: seal a payload object. The envelope keys lead the JSON so intake can
/// recognize an export by its first field, and the checksum is computed after
/// the payload is materialized. Sealing is idempotent: envelope keys already
/// present are replaced rather than folded into the payload.
pub fn seal_library_export(payload: &serde_json::Value) -> String {
    let payload = match payload.as_object() {
        Some(obj) => envelope_free_payload(obj),
        None => payload.clone(),
    };
    let checksum = export_checksum(&payload);
    let payload_text = payload.to_string();
    let body = payload_text
        .strip_prefix('{')
        .unwrap_or(payload_text.as_str());
    format!(
        "{{\"schema\":\"{LIBRARY_EXPORT_SCHEMA}\",\"schema_version\":{LIBRARY_SCHEMA_VERSION},\
         \"created\":{},\"checksum\":\"{checksum}\",{body}",
        now_unix()
    )
}

fn envelope_refusal(message: String) -> StoreError {
    StoreError::Invalid(vec![FieldError::new("export", message)])
}

/// T-22: pre-write envelope negotiation for a parsed export object. A payload
/// with no `schema` is a pre-T-22 export and passes through exactly as today
/// (backward-compat tolerance). A present `schema` must match,
/// `schema_version` must be ≤ current, and a present `checksum` must match the
/// payload bytes. Every refusal is returned before any write.
fn negotiate_library_envelope(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), StoreError> {
    let Some(schema) = obj.get("schema").and_then(serde_json::Value::as_str) else {
        return Ok(());
    };
    if schema != LIBRARY_EXPORT_SCHEMA {
        return Err(envelope_refusal(format!(
            "library export schema not supported (got \"{schema}\", supported {LIBRARY_EXPORT_SCHEMA})"
        )));
    }
    if let Some(raw_version) = obj.get("schema_version") {
        let version = raw_version.as_i64().ok_or_else(|| {
            envelope_refusal(format!(
                "library export schema not supported (schema_version is not a number, supported ≤{LIBRARY_SCHEMA_VERSION})"
            ))
        })?;
        if version > LIBRARY_SCHEMA_VERSION {
            return Err(envelope_refusal(format!(
                "library export schema not supported (got version {version}, supported ≤{LIBRARY_SCHEMA_VERSION})"
            )));
        }
    }
    if let Some(stored) = obj.get("checksum").and_then(serde_json::Value::as_str) {
        let computed = export_checksum(&envelope_free_payload(obj));
        if stored != computed {
            return Err(envelope_refusal(format!(
                "library export checksum mismatch (envelope {stored}, payload {computed})"
            )));
        }
    }
    Ok(())
}

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
                "quota_used": sub.quota_used,
                "quota_total": sub.quota_total,
                "quota_expire": sub.quota_expire,
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
    let payload = serde_json::json!({
        "outbounds": outbounds,
        "library": library,
        "subscriptions": subs,
        "tombstones": tombs,
    });
    seal_library_export(&payload)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthSnapshot {
    pub config_id: String,
    pub at: i64,

    pub kind: String,
    pub latency_ms: Option<u64>,

    pub error: Option<String>,
}

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

/// T-19: per-group opt-in for auto-clear-unavailable. Groups are free-text
/// field values (no groups table) and rename_group remaps them, so the flags
/// live in one JSON meta map keyed by normalized group name and
/// rename_group migrates its keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoClearGroupConfig {
    pub enabled: bool,
    pub on_date: i64,
    pub note_seen: bool,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrafficPeriod {
    Day,
    Week,
    Month,
    Quarter,
}

impl TrafficPeriod {
    #[cfg(any(test, feature = "test-support"))]
    pub const ALL: [TrafficPeriod; 4] = [
        TrafficPeriod::Day,
        TrafficPeriod::Week,
        TrafficPeriod::Month,
        TrafficPeriod::Quarter,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TrafficPeriod::Day => "24h",
            TrafficPeriod::Week => "7d",
            TrafficPeriod::Month => "30d",
            TrafficPeriod::Quarter => "90d",
        }
    }
}

pub const PRUNE_PREVIEW_LIMIT: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupPruneKind {
    Duplicates,
    Unavailable,
    Invalid,
    ClearTests,
}

impl GroupPruneKind {
    pub const ALL: [GroupPruneKind; 4] = [
        GroupPruneKind::Duplicates,
        GroupPruneKind::Unavailable,
        GroupPruneKind::Invalid,
        GroupPruneKind::ClearTests,
    ];

    pub fn label(self) -> &'static str {
        match self {
            GroupPruneKind::Duplicates => "Remove duplicates",
            GroupPruneKind::Unavailable => "Remove unavailable",
            GroupPruneKind::Invalid => "Remove invalid",
            GroupPruneKind::ClearTests => "Clear tests",
        }
    }
}

pub fn is_valid_config(protocol: &str, params: &str) -> bool {
    let mut errors = Vec::new();
    validate_protocol_params(protocol, params, &mut errors).is_some() && errors.is_empty()
}

pub fn prune_duplicates_confirm_text(group: &str, total: usize, preview: &[String]) -> String {
    let mut text = format!(
        "Remove {total} duplicate(s) in group \"{group}\"? The first copy of each \
         configuration is kept. This cannot be undone."
    );
    if !preview.is_empty() {
        text.push_str(&format!(" Duplicates: {}.", preview.join(", ")));
        if total > preview.len() {
            text.push_str(&format!(" (+{} more)", total - preview.len()));
        }
    }
    text
}

pub fn prune_unavailable_confirm_text(group: &str, count: usize) -> String {
    format!(
        "Remove {count} unavailable row(s) in group \"{group}\"? Rows whose last test failed \
         are deleted. This cannot be undone."
    )
}

pub fn prune_invalid_confirm_text(group: &str, count: usize) -> String {
    format!(
        "Remove {count} invalid row(s) in group \"{group}\"? Rows failing validation are \
         deleted. This cannot be undone."
    )
}

pub fn clear_group_tests_confirm_text(group: &str, count: usize) -> String {
    format!(
        "Clear {count} test result(s) in group \"{group}\"? Rows are kept and read untested \
         until the next probe. This cannot be undone."
    )
}

pub fn row_security_label(protocol: &str, params_raw: &str) -> &'static str {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(params_raw.trim()) {
        if let Some(obj) = value.as_object() {
            if let Some(security) = lookup_case_insensitive(obj, &["security"]) {
                if security.eq_ignore_ascii_case("reality") {
                    return "reality";
                }
                if security.eq_ignore_ascii_case("tls") {
                    return "tls";
                }
            }
        }
    }
    match protocol.trim().to_ascii_lowercase().as_str() {
        "anytls" | "naive" | "tuic" | "hysteria" | "hysteria2" | "hy2" | "shadowtls" => "tls",
        _ => "none",
    }
}

pub fn security_rank(label: &str) -> u8 {
    match label {
        "reality" => 3,
        "tls" => 2,
        "none" => 1,
        _ => 0,
    }
}

pub fn compare_type_security(a: &ProxyConfig, b: &ProxyConfig) -> std::cmp::Ordering {
    a.protocol
        .to_ascii_lowercase()
        .cmp(&b.protocol.to_ascii_lowercase())
        .then_with(|| {
            security_rank(row_security_label(&b.protocol, &b.params))
                .cmp(&security_rank(row_security_label(&a.protocol, &a.params)))
        })
        .then_with(|| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        })
}

/// T-27: the keys the library list can rank by. `None` is the pre-feature
/// order, byte for byte; `TypeSecurity` is the retired `type_security_sort`
/// toggle and keeps that toggle's fixed direction (protocol ascending, then
/// transport security descending, then name).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LibrarySortKey {
    #[default]
    None,
    Latency,
    Download,
    Upload,
    OutIp,
    TrafficTotal,
    TrafficDown,
    TrafficUp,
    TypeSecurity,
}

impl LibrarySortKey {
    /// Every key, in the order the pickers list them. The traffic keys live at
    /// the end because only the traffic header menu offers them.
    pub const ALL: [LibrarySortKey; 9] = [
        LibrarySortKey::None,
        LibrarySortKey::Latency,
        LibrarySortKey::Download,
        LibrarySortKey::Upload,
        LibrarySortKey::OutIp,
        LibrarySortKey::TypeSecurity,
        LibrarySortKey::TrafficTotal,
        LibrarySortKey::TrafficDown,
        LibrarySortKey::TrafficUp,
    ];

    /// Canonical token: both the meta JSON value and the picker item id suffix.
    pub fn token(self) -> &'static str {
        match self {
            LibrarySortKey::None => "none",
            LibrarySortKey::Latency => "latency",
            LibrarySortKey::Download => "download",
            LibrarySortKey::Upload => "upload",
            LibrarySortKey::OutIp => "out_ip",
            LibrarySortKey::TrafficTotal => "traffic_total",
            LibrarySortKey::TrafficDown => "traffic_down",
            LibrarySortKey::TrafficUp => "traffic_up",
            LibrarySortKey::TypeSecurity => "type_security",
        }
    }

    pub fn from_token(token: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|key| key.token() == token)
    }

    pub fn label(self) -> &'static str {
        match self {
            LibrarySortKey::None => "None (library order)",
            LibrarySortKey::Latency => "Latency (latest URL test)",
            LibrarySortKey::Download => "Download (current session)",
            LibrarySortKey::Upload => "Upload (current session)",
            LibrarySortKey::OutIp => "Out-IP (text)",
            LibrarySortKey::TrafficTotal => "Traffic total (counting window)",
            LibrarySortKey::TrafficDown => "Traffic down (counting window)",
            LibrarySortKey::TrafficUp => "Traffic up (counting window)",
            LibrarySortKey::TypeSecurity => "Type | Security (fixed)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortDirection {
    #[default]
    Ascending,
    Descending,
}

impl SortDirection {
    pub fn token(self) -> &'static str {
        match self {
            SortDirection::Ascending => "asc",
            SortDirection::Descending => "desc",
        }
    }

    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "asc" => Some(SortDirection::Ascending),
            "desc" => Some(SortDirection::Descending),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortDirection::Ascending => "ascending",
            SortDirection::Descending => "descending",
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            SortDirection::Ascending => SortDirection::Descending,
            SortDirection::Descending => SortDirection::Ascending,
        }
    }
}

/// T-27: one sort concept for the library list — a single key plus a direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LibrarySort {
    pub key: LibrarySortKey,
    pub direction: SortDirection,
}

impl LibrarySort {
    pub fn new(key: LibrarySortKey, direction: SortDirection) -> Self {
        Self { key, direction }
    }

    pub fn is_none(&self) -> bool {
        self.key == LibrarySortKey::None
    }

    /// Canonical meta payload: `{"key":"latency","direction":"asc"}`.
    pub fn to_json(self) -> String {
        format!(
            "{{\"key\":\"{}\",\"direction\":\"{}\"}}",
            self.key.token(),
            self.direction.token()
        )
    }

    /// Tolerant read: a payload that is not a sort object reads as the default,
    /// and an unknown key or direction falls back field by field, so a
    /// hand-edited meta row can never wedge the list.
    pub fn from_json(raw: &str) -> Self {
        let Ok(serde_json::Value::Object(map)) = serde_json::from_str(raw.trim()) else {
            return Self::default();
        };
        let key = map
            .get("key")
            .and_then(|value| value.as_str())
            .and_then(LibrarySortKey::from_token)
            .unwrap_or_default();
        let direction = map
            .get("direction")
            .and_then(|value| value.as_str())
            .and_then(SortDirection::from_token)
            .unwrap_or_default();
        Self { key, direction }
    }
}

/// T-22: the one clause both the export warning and the restore confirm carry.
pub const RESTORE_REQUIRES_CLAUSE: &str =
    "Restore requires a matching ManyProxy export (schema manyproxy-library, version 1).";

pub fn export_warning_text() -> &'static str {
    "Export carries private keys, tokens, and passwords in plaintext. Anyone with this file can use your proxies — store it somewhere safe. Restore requires a matching ManyProxy export (schema manyproxy-library, version 1)."
}

pub fn export_confirm_description() -> &'static str {
    export_warning_text()
}

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

        params: {
            let stored: String = row.get(3)?;
            crate::secret::Secret::open(&stored).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    3,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        error.to_string(),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutePreset {
    pub id: String,
    pub name: String,

    pub source: String,

    pub source_url: Option<String>,

    pub rules_json: String,

    pub updated_at: Option<i64>,

    pub description: Option<String>,

    pub is_builtin: bool,

    pub last_used_at: Option<i64>,

    /// Prevent-modifications lock: protects the *rules* (readonly editor),
    /// never the attach relationship.
    pub locked: bool,
}

impl RoutePreset {
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

fn map_preset_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RoutePreset> {
    let id: i64 = row.get(0)?;

    let description: Option<String> = row.get(6).ok().flatten();
    let is_builtin: Option<i64> = row.get(7).ok();
    let last_used_at: Option<i64> = row.get(8).ok().flatten();
    let locked: Option<i64> = row.get(9).ok();
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
        locked: locked.unwrap_or(0) != 0,
    })
}

fn map_rule_set_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RuleSetRow> {
    let id: i64 = row.get(0)?;
    let bytes: i64 = row.get(4)?;
    Ok(RuleSetRow {
        id: id.to_string(),
        name: row.get(1)?,
        url: row.get(2)?,
        file_name: row.get(3)?,
        bytes: bytes.max(0) as u64,
        downloaded_at: row.get(5)?,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuarantinedRow {
    pub id: String,
    pub cause: String,
}

/// T-12: one staged rule set. `file_name` mirrors the sanitized name
/// (`data\rule-sets\<file_name>`); rows never hold the binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSetRow {
    pub id: String,
    pub name: String,

    pub url: String,

    pub file_name: String,

    pub bytes: u64,

    pub downloaded_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthHistoryEntry {
    pub config_id: String,
    pub at: i64,

    pub kind: String,
    pub latency_ms: Option<u64>,

    pub error: Option<String>,
}

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

struct ValidatedConfig {
    name: String,
    protocol: String,
    params: Secret,
    group: String,
    bind_ip: String,
    port: u16,
}

impl ProxyConfig {
    fn rowid(&self) -> i64 {
        self.id.parse().unwrap_or(-1)
    }
}

fn validated_name(raw: &str, errors: &mut Vec<FieldError>) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        errors.push(FieldError::new("name", "name must not be empty"));
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn defaulted(raw: &str, default: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalized_group(raw: &str) -> String {
    raw.trim().to_string()
}

fn validated_bind(raw: &str, errors: &mut Vec<FieldError>) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Some(DEFAULT_BIND.to_string());
    }
    if trimmed.eq_ignore_ascii_case("localhost") {
        return Some("localhost".to_string());
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

/// T-33: one canonical literal, or `None` for a hostname/malformed value.
fn canonical_bind_host(raw: &str) -> Option<String> {
    raw.trim().parse::<IpAddr>().ok().map(|ip| ip.to_string())
}

/// T-33: trims a typed value to its canonical literal — the list's only gate.
fn normalize_bind_host(raw: &str) -> Result<String, StoreError> {
    canonical_bind_host(raw)
        .ok_or_else(|| bind_host_error(Store::BIND_HOST_LITERAL_ONLY.to_string()))
}

fn bind_host_error(message: String) -> StoreError {
    StoreError::Invalid(vec![FieldError::new("bind_hosts", message)])
}

fn bind_host_in_use_message(host: &str, rows: &[String]) -> String {
    let named: Vec<String> = rows.iter().map(|name| format!("\"{name}\"")).collect();
    format!(
        "{host} is used by {} — rebind those rows first",
        named.join(", ")
    )
}

/// T-33: stored extras only — constants, duplicates, blanks, and non-literals
/// are dropped so a hand-edited meta row can never widen what may bind.
fn sanitized_bind_host_extras(entries: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for entry in entries {
        let Some(host) = canonical_bind_host(&entry) else {
            continue;
        };
        if Store::BIND_HOST_CONSTANTS.contains(&host.as_str()) || out.contains(&host) {
            continue;
        }
        out.push(host);
    }
    out
}

fn decode_bind_host_extras(raw: &str) -> Vec<String> {
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

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

        "snell" => Some("snell".to_string()),
        "shadowtls" | "shadow-tls" | "shadow_tls" => Some("shadowtls".to_string()),
        "anytls" => Some("anytls".to_string()),

        "ssh" => Some("ssh".to_string()),
        "noise" => Some("noise".to_string()),

        "mieru" => Some("mieru".to_string()),
        "juicity" => Some("juicity".to_string()),
        "trusttunnel" => Some("trusttunnel".to_string()),
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

        "direct" => Some("direct".to_string()),
        _ => None,
    }
}

pub fn protocol_params_template(protocol: &str) -> String {
    fn blank(params: &mut serde_json::Map<String, serde_json::Value>, key: &str) {
        params.insert(key.to_string(), serde_json::Value::String(String::new()));
    }
    let canonical = canonical_protocol(protocol).unwrap_or_else(|| protocol.trim().to_string());

    if matches!(
        canonical.as_str(),
        "direct" | "chain" | "custom-outbound" | "custom-config" | "extra-core"
    ) {
        return format!("{{\"type\":\"{canonical}\"}}");
    }
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

        "mieru" | "trusttunnel" => {
            blank(&mut params, "username");
            blank(&mut params, "password");
        }
        "juicity" => {
            blank(&mut params, "uuid");
            blank(&mut params, "password");
        }
        "noise" => blank(&mut params, "key"),
        _ => {}
    }
    serde_json::Value::Object(params).to_string()
}

pub fn is_virtual_protocol(protocol: &str) -> bool {
    matches!(
        canonical_protocol(protocol).as_deref(),
        Some("chain" | "custom-outbound" | "custom-config" | "extra-core" | "direct")
    )
}

pub fn is_pristine_template(params_raw: &str, protocol: &str) -> bool {
    params_raw == protocol_params_template(protocol)
}

pub fn pristine_template_rewrite(
    params_raw: &str,
    old_protocol: &str,
    new_protocol: &str,
) -> String {
    if is_pristine_template(params_raw, old_protocol) {
        protocol_params_template(new_protocol)
    } else {
        params_raw.to_string()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransportFold {
    pub network: String,
    pub path: String,
    pub host: String,
    pub method: String,
    pub service_name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TlsOptionalFold {
    pub enable: bool,
    pub sni: String,
    pub alpn: String,
    pub fp: String,
    pub insecure: bool,
    pub pbk: String,
    pub sid: String,
}

fn fold_tls_security(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    fold: &TlsOptionalFold,
) -> bool {
    if fold.enable {
        let want = if fold.pbk.trim().is_empty() {
            "tls"
        } else {
            "reality"
        };
        match obj.get("security") {
            Some(serde_json::Value::String(current)) if current.eq_ignore_ascii_case(want) => false,
            _ => {
                obj.insert(
                    "security".to_string(),
                    serde_json::Value::String(want.to_string()),
                );
                true
            }
        }
    } else {
        obj.remove("security").is_some()
    }
}

fn fold_tls_details(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    fold: &TlsOptionalFold,
) -> bool {
    let mut changed = false;
    changed |= fold_text_aliases(obj, "sni", &["sni", "peer"], &fold.sni);
    changed |= fold_text_aliases(obj, "alpn", &["alpn"], &fold.alpn);
    changed |= fold_text_aliases(obj, "fp", &["fp", "fingerprint"], &fold.fp);
    changed |= fold_bool_aliases(
        obj,
        "insecure",
        &[
            "insecure",
            "allowInsecure",
            "allow_insecure",
            "allowinsecure",
        ],
        fold.insecure,
    );
    changed |= fold_text_aliases(obj, "pbk", &["pbk"], &fold.pbk);
    changed |= fold_text_aliases(obj, "sid", &["sid"], &fold.sid);
    changed
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MuxFold {
    pub mode: String,
    pub brutal_enable: bool,
    pub brutal_up_mbps: String,
    pub brutal_down_mbps: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DialFold {
    pub reuse_addr: bool,
    pub tcp_fast_open: bool,
    pub udp_fragment: bool,
    pub tcp_multi_path: bool,
    pub connect_timeout: String,
    pub bind_interface: String,
    pub inet4_bind_address: String,
    pub inet6_bind_address: String,
}

fn fold_select_key(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: &str,
    default_value: &str,
) -> bool {
    let value = value.trim();
    let current = match obj.get(key) {
        Some(serde_json::Value::String(current)) => Some(current.clone()),
        _ => None,
    };
    if current.as_deref() == Some(value) {
        return false;
    }
    if value.is_empty() || value == default_value {
        return obj.remove(key).is_some();
    }
    obj.insert(
        key.to_string(),
        serde_json::Value::String(value.to_string()),
    );
    true
}

fn fold_bool_aliases(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    canonical: &str,
    aliases: &[&str],
    value: bool,
) -> bool {
    let current = aliases.iter().any(|key| truthy_flat(obj.get(*key)));
    if current == value {
        return false;
    }
    for key in aliases {
        obj.remove(*key);
    }
    if value {
        obj.insert(canonical.to_string(), serde_json::Value::Bool(true));
    }
    true
}

fn truthy_flat(value: Option<&serde_json::Value>) -> bool {
    match value {
        Some(serde_json::Value::Bool(flag)) => *flag,
        Some(serde_json::Value::Number(number)) => number.as_u64().is_some_and(|n| n != 0),
        Some(serde_json::Value::String(text)) => {
            matches!(
                text.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on" | "enabled"
            )
        }
        _ => false,
    }
}

fn fold_text_aliases(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    canonical: &str,
    read_keys: &[&str],
    value: &str,
) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    if lookup_case_insensitive(obj, read_keys).as_deref() == Some(value) {
        return false;
    }
    obj.insert(
        canonical.to_string(),
        serde_json::Value::String(value.to_string()),
    );
    true
}

fn fold_number_text(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: &str,
) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    let next = match value.parse::<u64>() {
        Ok(number) => serde_json::Value::from(number),
        Err(_) => serde_json::Value::String(value.to_string()),
    };
    match obj.get(key) {
        Some(current) if current == &next => false,
        _ => {
            obj.insert(key.to_string(), next);
            true
        }
    }
}

fn get_case_insensitive<'a>(
    obj: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a serde_json::Value> {
    obj.get(key).or_else(|| {
        obj.iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value)
    })
}

pub fn guided_prefill_bool(params_raw: &str, keys: &[&str]) -> bool {
    let obj = serde_json::from_str::<serde_json::Value>(params_raw)
        .ok()
        .and_then(|value| value.as_object().cloned());
    match obj {
        Some(obj) => keys
            .iter()
            .any(|key| truthy_flat(get_case_insensitive(&obj, key))),
        None => false,
    }
}

pub fn fold_transport_into_params(params_raw: &str, fold: &TransportFold) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = false;
    changed |= fold_select_key(obj, "network", &fold.network, "tcp");
    changed |= fold_text_aliases(obj, "path", &["path"], &fold.path);
    changed |= fold_text_aliases(obj, "host", &["host"], &fold.host);
    changed |= fold_text_aliases(obj, "method", &["method"], &fold.method);
    changed |= fold_text_aliases(
        obj,
        "service_name",
        &["service_name", "serviceName"],
        &fold.service_name,
    );
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

pub fn fold_tls_optional_into_params(params_raw: &str, fold: &TlsOptionalFold) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = fold_tls_security(obj, fold);
    changed |= fold_tls_details(obj, fold);
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

pub fn fold_mux_into_params(params_raw: &str, fold: &MuxFold) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = false;

    changed |= fold_select_key(obj, "mux", &fold.mode, "default");
    changed |= fold_bool_aliases(obj, "brutal_enable", &["brutal_enable"], fold.brutal_enable);
    changed |= fold_number_text(obj, "brutal_up_mbps", &fold.brutal_up_mbps);
    changed |= fold_number_text(obj, "brutal_down_mbps", &fold.brutal_down_mbps);
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

pub fn fold_dial_into_params(params_raw: &str, fold: &DialFold) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = false;
    for (key, value) in [
        ("reuse_addr", fold.reuse_addr),
        ("tcp_fast_open", fold.tcp_fast_open),
        ("udp_fragment", fold.udp_fragment),
        ("tcp_multi_path", fold.tcp_multi_path),
    ] {
        changed |= fold_bool_aliases(obj, key, &[key], value);
    }
    changed |= fold_text_aliases(
        obj,
        "connect_timeout",
        &["connect_timeout"],
        &fold.connect_timeout,
    );
    changed |= fold_text_aliases(
        obj,
        "bind_interface",
        &["bind_interface"],
        &fold.bind_interface,
    );
    changed |= fold_text_aliases(
        obj,
        "inet4_bind_address",
        &["inet4_bind_address"],
        &fold.inet4_bind_address,
    );
    changed |= fold_text_aliases(
        obj,
        "inet6_bind_address",
        &["inet6_bind_address"],
        &fold.inet6_bind_address,
    );
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

pub fn fold_vless_inner_into_params(
    params_raw: &str,
    uuid: &str,
    flow: &str,
    packet_encoding: &str,
) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = false;
    changed |= fold_text_aliases(obj, "uuid", &["uuid", "id"], uuid);
    changed |= fold_select_key(obj, "flow", flow, "");
    changed |= fold_select_key(obj, "packet_encoding", packet_encoding, "");
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

pub const VMESS_CIPHERS: [&str; 5] = ["auto", "zero", "none", "chacha20-poly1305", "aes-128-gcm"];

pub const SS_METHODS: [&str; 18] = [
    "2022-blake3-aes-128-gcm",
    "2022-blake3-aes-256-gcm",
    "2022-blake3-chacha20-poly1305",
    "none",
    "aes-128-gcm",
    "aes-192-gcm",
    "aes-256-gcm",
    "aes-128-ctr",
    "aes-192-ctr",
    "aes-256-ctr",
    "aes-128-cfb",
    "aes-192-cfb",
    "aes-256-cfb",
    "chacha20-ietf-poly1305",
    "xchacha20-ietf-poly1305",
    "chacha20-ietf",
    "xchacha20",
    "rc4-md5",
];

fn vmess_has_legacy_tls_raw(obj: &serde_json::Map<String, serde_json::Value>) -> bool {
    obj.get("tls").is_some_and(|tls| tls.is_boolean())
}

pub fn vmess_has_legacy_tls(params_raw: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(params_raw)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .is_some_and(|obj| vmess_has_legacy_tls_raw(&obj))
}

pub fn vmess_cipher_value(params_raw: &str) -> String {
    let encryption = guided_prefill_value(params_raw, &["encryption"]);
    if !encryption.trim().is_empty() {
        return encryption;
    }
    let security = guided_prefill_value(params_raw, &["security"]);
    if VMESS_CIPHERS
        .iter()
        .any(|cipher| security.eq_ignore_ascii_case(cipher))
    {
        return security;
    }
    if vmess_has_legacy_tls(params_raw) {
        return security;
    }
    String::new()
}

pub fn vmess_tls_enabled(params_raw: &str) -> bool {
    let security = guided_prefill_value(params_raw, &["security"]);
    if matches!(
        security.to_ascii_lowercase().as_str(),
        "tls" | "xtls" | "reality"
    ) {
        return true;
    }
    guided_prefill_bool(params_raw, &["tls"])
}

pub fn fold_vmess_inner_into_params(
    params_raw: &str,
    uuid: &str,
    aid: &str,
    cipher: &str,
    packet_encoding: &str,
) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = false;
    changed |= fold_text_aliases(obj, "uuid", &["uuid", "id"], uuid);
    changed |= fold_text_aliases(obj, "alter_id", &["alter_id", "aid"], aid);
    if vmess_has_legacy_tls_raw(obj) {
        changed |= fold_text_aliases(obj, "security", &["security"], cipher);
    } else {
        changed |= fold_select_key(obj, "encryption", cipher, "auto");
    }
    changed |= fold_select_key(obj, "packet_encoding", packet_encoding, "");
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

pub fn fold_trojan_inner_into_params(params_raw: &str, password: &str) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    if fold_text_aliases(obj, "password", &["password"], password) {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

fn fold_select_keep_blank(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: &str,
) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    match obj.get(key) {
        Some(serde_json::Value::String(current)) if current == value => false,
        _ => {
            obj.insert(
                key.to_string(),
                serde_json::Value::String(value.to_string()),
            );
            true
        }
    }
}

pub fn fold_shadowsocks_inner_into_params(
    params_raw: &str,
    method: &str,
    password: &str,
    plugin: &str,
    plugin_opts: &str,
    uot: bool,
) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = false;
    changed |= fold_select_keep_blank(obj, "method", method);
    changed |= fold_text_aliases(obj, "password", &["password"], password);
    changed |= fold_select_keep_blank(obj, "plugin", plugin);
    changed |= fold_text_aliases(
        obj,
        "plugin_opts",
        &["plugin_opts", "plugin-opts", "pluginOpts"],
        plugin_opts,
    );
    changed |= fold_bool_aliases(obj, "udp_over_tcp", &["udp_over_tcp", "uot"], uot);
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

pub fn fold_tls_vmess_into_params(params_raw: &str, fold: &TlsOptionalFold) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = false;
    if vmess_has_legacy_tls_raw(obj) && fold.enable && !fold.pbk.trim().is_empty() {
        obj.remove("tls");
        changed = true;
        if let Some(serde_json::Value::String(security)) = obj.get("security").cloned() {
            if VMESS_CIPHERS
                .iter()
                .any(|cipher| security.eq_ignore_ascii_case(cipher))
            {
                obj.remove("security");
                obj.insert(
                    "encryption".to_string(),
                    serde_json::Value::String(security),
                );
            }
        }
    }
    if vmess_has_legacy_tls_raw(obj) {
        let current = obj
            .get("tls")
            .and_then(|tls| tls.as_bool())
            .unwrap_or(false);
        if current != fold.enable {
            obj.insert("tls".to_string(), serde_json::Value::Bool(fold.enable));
            changed = true;
        }
        changed |= fold_tls_details(obj, fold);
    } else {
        changed |= fold_tls_security(obj, fold);
        changed |= fold_tls_details(obj, fold);
    }
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

pub const AWG_VERSIONS: [&str; 2] = ["2.0", "3.x"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WireguardFold {
    pub private_key: String,
    pub public_key: String,
    pub pre_shared_key: String,
    pub reserved: String,
    pub persistent_keepalive: String,
    pub local_addr: String,
    pub dns: String,
    pub mtu: String,
    pub workers: String,
    pub sys_ifc: bool,
}

pub fn fold_wireguard_into_params(params_raw: &str, fold: &WireguardFold) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = false;
    changed |= fold_text_aliases(
        obj,
        "private_key",
        &["private_key", "privatekey"],
        &fold.private_key,
    );
    changed |= fold_text_aliases(
        obj,
        "public_key",
        &["public_key", "publickey"],
        &fold.public_key,
    );
    changed |= fold_text_aliases(
        obj,
        "pre_shared_key",
        &["pre_shared_key", "presharedkey", "preshared_key"],
        &fold.pre_shared_key,
    );
    changed |= fold_text_aliases(obj, "reserved", &["reserved"], &fold.reserved);
    changed |= fold_text_aliases(
        obj,
        "persistent_keepalive",
        &["persistent_keepalive", "persistent_keepalive_interval"],
        &fold.persistent_keepalive,
    );
    changed |= fold_text_aliases(
        obj,
        "address",
        &["address", "local_address", "local_addr"],
        &fold.local_addr,
    );
    changed |= fold_text_aliases(obj, "dns", &["dns"], &fold.dns);
    changed |= fold_text_aliases(obj, "mtu", &["mtu"], &fold.mtu);
    changed |= fold_text_aliases(obj, "workers", &["workers"], &fold.workers);
    changed |= fold_bool_aliases(obj, "system", &["system", "sys_ifc"], fold.sys_ifc);
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AmneziaFold {
    pub enable_amnezia: bool,
    pub version: String,
    pub jc: String,
    pub jmin: String,
    pub jmax: String,
    pub s1: String,
    pub s2: String,
    pub s3: String,
    pub s4: String,
    pub h1: String,
    pub h2: String,
    pub h3: String,
    pub h4: String,
    pub i1: String,
    pub i2: String,
    pub i3: String,
    pub i4: String,
    pub i5: String,
    pub header_protection_key: String,
    pub content_padding_addition: String,
    pub rekey_after_time: String,
    pub rekey_timeout: String,
    pub reject_after_time: String,
    pub keepalive_timeout: String,
    pub max_handshake_attempts: String,
    pub random_trailers: bool,
    pub disable_cookies: bool,
}

pub fn fold_amneziawg_into_params(params_raw: &str, fold: &AmneziaFold) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = false;
    changed |= fold_bool_aliases(
        obj,
        "enable_amnezia",
        &["enable_amnezia"],
        fold.enable_amnezia,
    );
    changed |= fold_select_keep_blank(obj, "awg_version", &fold.version);
    for (key, value) in [
        ("jc", &fold.jc),
        ("jmin", &fold.jmin),
        ("jmax", &fold.jmax),
        ("s1", &fold.s1),
        ("s2", &fold.s2),
        ("s3", &fold.s3),
        ("s4", &fold.s4),
        ("h1", &fold.h1),
        ("h2", &fold.h2),
        ("h3", &fold.h3),
        ("h4", &fold.h4),
        ("i1", &fold.i1),
        ("i2", &fold.i2),
        ("i3", &fold.i3),
        ("i4", &fold.i4),
        ("i5", &fold.i5),
        ("headerprotectionkey", &fold.header_protection_key),
        ("contentpaddingaddition", &fold.content_padding_addition),
        ("rekeyaftertime", &fold.rekey_after_time),
        ("rekeytimeout", &fold.rekey_timeout),
        ("rejectaftertime", &fold.reject_after_time),
        ("keepalivetimeout", &fold.keepalive_timeout),
        ("maxhandshakeattempts", &fold.max_handshake_attempts),
    ] {
        changed |= fold_text_aliases(obj, key, &[key], value);
    }
    changed |= fold_bool_aliases(
        obj,
        "randomtrailers",
        &["randomtrailers", "random_trailers"],
        fold.random_trailers,
    );
    changed |= fold_bool_aliases(
        obj,
        "disablecookies",
        &["disablecookies", "disable_cookies"],
        fold.disable_cookies,
    );
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

pub const HY_AUTH_TYPES: [&str; 2] = ["STRING", "BASE64"];
pub const TUIC_CC_MODES: [&str; 3] = ["bbr", "cubic", "new_reno"];
pub const TUIC_RELAY_MODES: [&str; 2] = ["native", "quic"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TlsForcedFold {
    pub sni: String,
    pub alpn: String,
    pub fp: String,
    pub insecure: bool,
    pub pbk: String,
    pub sid: String,
}

pub fn fold_tls_forced_into_params(params_raw: &str, fold: &TlsForcedFold) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = false;
    changed |= fold_text_aliases(obj, "sni", &["sni", "peer"], &fold.sni);
    changed |= fold_text_aliases(obj, "alpn", &["alpn"], &fold.alpn);
    changed |= fold_text_aliases(obj, "fp", &["fp", "fingerprint"], &fold.fp);
    changed |= fold_bool_aliases(
        obj,
        "insecure",
        &[
            "insecure",
            "allowInsecure",
            "allow_insecure",
            "allowinsecure",
        ],
        fold.insecure,
    );
    changed |= fold_text_aliases(obj, "pbk", &["pbk"], &fold.pbk);
    changed |= fold_text_aliases(obj, "sid", &["sid"], &fold.sid);
    if !fold.pbk.trim().is_empty() {
        match obj.get("security") {
            Some(serde_json::Value::String(current)) if current.eq_ignore_ascii_case("reality") => {
            }
            _ => {
                obj.insert(
                    "security".to_string(),
                    serde_json::Value::String("reality".to_string()),
                );
                changed = true;
            }
        }
    } else if changed
        && !matches!(
            lookup_case_insensitive(obj, &["security"])
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str(),
            "tls" | "xtls" | "reality"
        )
    {
        obj.insert(
            "security".to_string(),
            serde_json::Value::String("tls".to_string()),
        );
        changed = true;
    }
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HysteriaFold {
    pub server_ports: String,
    pub hop_interval: String,
    pub up: String,
    pub down: String,
    pub obfs: String,
    pub auth_type: String,
    pub auth: String,
    pub recv_window_conn: String,
    pub recv_window: String,
    pub disable_mtu_discovery: bool,
    pub password: String,
}

pub fn fold_hysteria_into_params(params_raw: &str, fold: &HysteriaFold, is_v2: bool) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = false;
    changed |= fold_text_aliases(obj, "server_ports", &["server_ports"], &fold.server_ports);
    changed |= fold_text_aliases(obj, "hop_interval", &["hop_interval"], &fold.hop_interval);
    changed |= fold_text_aliases(obj, "up", &["up", "up_mbps", "upmbps"], &fold.up);
    changed |= fold_text_aliases(obj, "down", &["down", "down_mbps", "downmbps"], &fold.down);
    changed |= fold_text_aliases(obj, "obfs", &["obfs"], &fold.obfs);
    changed |= fold_select_keep_blank(obj, "auth_type", &fold.auth_type);
    if is_v2 {
        changed |= fold_text_aliases(obj, "password", &["password"], &fold.password);
    } else {
        changed |= fold_text_aliases(obj, "auth", &["auth"], &fold.auth);
    }
    changed |= fold_number_text(obj, "recv_window_conn", &fold.recv_window_conn);
    changed |= fold_number_text(obj, "recv_window", &fold.recv_window);
    changed |= fold_bool_aliases(
        obj,
        "disable_mtu_discovery",
        &["disable_mtu_discovery"],
        fold.disable_mtu_discovery,
    );
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TuicFold {
    pub uuid: String,
    pub password: String,
    pub congestion_control: String,
    pub udp_relay_mode: String,
    pub udp_over_stream: bool,
    pub zero_rtt_handshake: bool,
    pub heartbeat: String,
}

pub fn fold_tuic_into_params(params_raw: &str, fold: &TuicFold) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = false;
    changed |= fold_text_aliases(obj, "uuid", &["uuid", "id"], &fold.uuid);
    changed |= fold_text_aliases(obj, "password", &["password"], &fold.password);
    changed |= fold_select_keep_blank(obj, "congestion_control", &fold.congestion_control);
    changed |= fold_select_keep_blank(obj, "udp_relay_mode", &fold.udp_relay_mode);
    changed |= fold_bool_aliases(
        obj,
        "udp_over_stream",
        &["udp_over_stream"],
        fold.udp_over_stream,
    );
    changed |= fold_bool_aliases(
        obj,
        "zero_rtt_handshake",
        &["zero_rtt_handshake", "zero_rtt"],
        fold.zero_rtt_handshake,
    );
    changed |= fold_text_aliases(obj, "heartbeat", &["heartbeat"], &fold.heartbeat);
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

pub const SOCKS_VERSIONS: [&str; 2] = ["5", "4"];
pub const NAIVE_CC_MODES: [&str; 4] = ["bbr", "bbr2", "cubic", "reno"];

pub fn guided_prefill_list(params_raw: &str, keys: &[&str]) -> String {
    let obj = serde_json::from_str::<serde_json::Value>(params_raw)
        .ok()
        .and_then(|value| value.as_object().cloned());
    let Some(obj) = obj else {
        return String::new();
    };
    for wanted in keys {
        let hit = obj
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(wanted));
        if let Some((_, value)) = hit {
            if let Some(items) = value.as_array() {
                let joined = items
                    .iter()
                    .filter_map(|item| item.as_str())
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                if !joined.is_empty() {
                    return joined;
                }
            }
        }
    }
    guided_prefill_value(params_raw, keys)
}

fn fold_comma_list(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    canonical: &str,
    read_keys: &[&str],
    value: &str,
) -> bool {
    let next: Vec<String> = value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect();
    if next.is_empty() {
        return false;
    }
    let current: Option<Vec<String>> = read_keys.iter().find_map(|key| {
        let hit = obj
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value)?;
        match hit {
            serde_json::Value::Array(items) => Some(
                items
                    .iter()
                    .filter_map(|item| item.as_str())
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(str::to_string)
                    .collect(),
            ),
            serde_json::Value::String(text) => Some(
                text.split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(str::to_string)
                    .collect(),
            ),
            serde_json::Value::Number(number) => Some(vec![number.to_string()]),
            _ => None,
        }
    });
    if current.as_deref() == Some(next.as_slice()) {
        return false;
    }
    obj.insert(
        canonical.to_string(),
        serde_json::Value::Array(next.into_iter().map(serde_json::Value::String).collect()),
    );
    true
}

fn fold_select_keep_blank_aliases(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    canonical: &str,
    read_keys: &[&str],
    value: &str,
) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    if lookup_case_insensitive(obj, read_keys).as_deref() == Some(value) {
        return false;
    }
    obj.insert(
        canonical.to_string(),
        serde_json::Value::String(value.to_string()),
    );
    true
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SocksFold {
    pub version: String,
    pub username: String,
    pub password: String,
    pub network: String,
    pub udp_over_tcp: bool,
}

pub fn fold_socks_into_params(params_raw: &str, fold: &SocksFold) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = false;
    changed |= fold_select_keep_blank(obj, "version", &fold.version);
    changed |= fold_text_aliases(obj, "username", &["username"], &fold.username);
    changed |= fold_text_aliases(obj, "password", &["password"], &fold.password);
    changed |= fold_select_key(obj, "network", &fold.network, "tcp");
    changed |= fold_bool_aliases(
        obj,
        "udp_over_tcp",
        &["udp_over_tcp", "uot"],
        fold.udp_over_tcp,
    );
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HttpFold {
    pub username: String,
    pub password: String,
    pub path: String,
    pub headers: String,
}

pub fn fold_http_into_params(params_raw: &str, fold: &HttpFold) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = false;
    changed |= fold_text_aliases(obj, "username", &["username"], &fold.username);
    changed |= fold_text_aliases(obj, "password", &["password"], &fold.password);
    changed |= fold_text_aliases(obj, "path", &["path"], &fold.path);
    changed |= fold_text_aliases(obj, "headers", &["headers"], &fold.headers);
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnytlsFold {
    pub password: String,
    pub interval: String,
    pub timeout: String,
    pub min: String,
}

pub fn fold_anytls_into_params(params_raw: &str, fold: &AnytlsFold) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = false;
    changed |= fold_text_aliases(obj, "password", &["password"], &fold.password);
    changed |= fold_text_aliases(
        obj,
        "idle_session_check_interval",
        &["idle_session_check_interval", "interval"],
        &fold.interval,
    );
    changed |= fold_text_aliases(
        obj,
        "idle_session_timeout",
        &["idle_session_timeout", "timeout"],
        &fold.timeout,
    );
    changed |= fold_number_text(obj, "min_idle_session", &fold.min);
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NaiveFold {
    pub username: String,
    pub password: String,
    pub udp_over_tcp: bool,
    pub quic: bool,
    pub congestion_control: String,
}

pub fn fold_naive_into_params(params_raw: &str, fold: &NaiveFold) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = false;
    changed |= fold_text_aliases(obj, "username", &["username"], &fold.username);
    changed |= fold_text_aliases(obj, "password", &["password"], &fold.password);
    changed |= fold_bool_aliases(
        obj,
        "udp_over_tcp",
        &["udp_over_tcp", "uot"],
        fold.udp_over_tcp,
    );
    changed |= fold_bool_aliases(obj, "quic", &["quic"], fold.quic);
    changed |= fold_select_keep_blank_aliases(
        obj,
        "quic_congestion_control",
        &["quic_congestion_control", "congestion_control"],
        &fold.congestion_control,
    );
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SshFold {
    pub user: String,
    pub password: String,
    pub private_key: String,
    pub private_key_path: String,
    pub private_key_passphrase: String,
    pub host_key: String,
    pub host_key_algorithms: String,
    pub client_version: String,
}

pub fn fold_ssh_into_params(params_raw: &str, fold: &SshFold) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = false;
    changed |= fold_text_aliases(obj, "user", &["user", "username"], &fold.user);
    changed |= fold_text_aliases(obj, "password", &["password"], &fold.password);
    changed |= fold_text_aliases(
        obj,
        "private_key",
        &["private_key", "privatekey"],
        &fold.private_key,
    );
    changed |= fold_text_aliases(
        obj,
        "private_key_path",
        &["private_key_path"],
        &fold.private_key_path,
    );
    changed |= fold_text_aliases(
        obj,
        "private_key_passphrase",
        &["private_key_passphrase", "private_key_pass"],
        &fold.private_key_passphrase,
    );
    changed |= fold_comma_list(obj, "host_key", &["host_key"], &fold.host_key);
    changed |= fold_comma_list(
        obj,
        "host_key_algorithms",
        &["host_key_algorithms"],
        &fold.host_key_algorithms,
    );
    changed |= fold_text_aliases(
        obj,
        "client_version",
        &["client_version"],
        &fold.client_version,
    );
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

pub const SNELL_VERSIONS: [&str; 2] = ["4", "6"];

pub const SHADOWTLS_VERSIONS: [&str; 3] = ["1", "2", "3"];

pub const XRAY_NETWORKS: [&str; 5] = ["raw", "xhttp", "ws", "httpupgrade", "grpc"];

pub const XRAY_FLOWS: [&str; 2] = ["xtls-rprx-vision", "xtls-rprx-vision-udp443"];

pub const XRAY_MUX_MODES: [&str; 2] = ["enabled", "disabled"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SnellFold {
    pub psk: String,
    pub version: String,
    pub obfs_mode: String,
    pub obfs_host: String,
    pub userkey: String,
    pub reuse: bool,
    pub network: String,
    pub mode: String,
}

pub fn fold_snell_into_params(params_raw: &str, fold: &SnellFold) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = false;
    changed |= fold_text_aliases(obj, "psk", &["psk"], &fold.psk);
    changed |= fold_select_keep_blank(obj, "version", &fold.version);
    changed |= fold_text_aliases(obj, "obfs_mode", &["obfs_mode", "obfs"], &fold.obfs_mode);
    changed |= fold_text_aliases(
        obj,
        "obfs_host",
        &["obfs_host", "obfs-host"],
        &fold.obfs_host,
    );
    changed |= fold_text_aliases(obj, "userkey", &["userkey"], &fold.userkey);
    changed |= fold_bool_aliases(obj, "reuse", &["reuse"], fold.reuse);
    changed |= fold_text_aliases(obj, "network", &["network"], &fold.network);
    changed |= fold_text_aliases(obj, "mode", &["mode"], &fold.mode);
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShadowTlsFold {
    pub version: String,
    pub password: String,
}

pub fn fold_shadowtls_into_params(params_raw: &str, fold: &ShadowTlsFold) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = false;
    changed |= fold_select_keep_blank(obj, "version", &fold.version);
    changed |= fold_text_aliases(obj, "password", &["password"], &fold.password);
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct XrayFold {
    pub uuid: String,
    pub encryption: String,
    pub flow: String,
    pub sni: String,
    pub fp: String,
    pub alpn: String,
    pub pbk: String,
    pub sid: String,
    pub spiderx: String,
    pub network: String,
    pub mode: String,
    pub mux: String,
}

pub fn fold_xray_into_params(params_raw: &str, fold: &XrayFold) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = false;
    changed |= fold_text_aliases(obj, "uuid", &["uuid", "id"], &fold.uuid);
    changed |= fold_text_aliases(obj, "encryption", &["encryption"], &fold.encryption);
    changed |= fold_select_keep_blank(obj, "flow", &fold.flow);
    changed |= fold_text_aliases(obj, "sni", &["sni"], &fold.sni);
    changed |= fold_select_keep_blank_aliases(obj, "fp", &["fp", "fingerprint"], &fold.fp);
    changed |= fold_text_aliases(obj, "alpn", &["alpn"], &fold.alpn);
    changed |= fold_text_aliases(obj, "pbk", &["pbk"], &fold.pbk);
    changed |= fold_text_aliases(obj, "sid", &["sid"], &fold.sid);
    changed |= fold_text_aliases(obj, "spiderx", &["spiderx"], &fold.spiderx);
    changed |= fold_select_keep_blank(obj, "network", &fold.network);
    changed |= fold_text_aliases(obj, "mode", &["mode"], &fold.mode);
    changed |= fold_select_keep_blank(obj, "xray_mux", &fold.mux);
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtraFold {
    pub exe: String,
    pub args: String,
    pub config: String,
}

fn fold_extra_config(obj: &mut serde_json::Map<String, serde_json::Value>, value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    let next = serde_json::from_str::<serde_json::Value>(value)
        .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));
    match obj.get("config") {
        Some(current) if current == &next => false,
        _ => {
            obj.insert("config".to_string(), next);
            true
        }
    }
}

pub fn fold_extra_into_params(params_raw: &str, fold: &ExtraFold) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let mut changed = false;
    changed |= fold_text_aliases(obj, "exe", &["exe", "binary"], &fold.exe);
    changed |= fold_text_aliases(obj, "args", &["args"], &fold.args);
    changed |= fold_extra_config(obj, &fold.config);
    if changed {
        value.to_string()
    } else {
        params_raw.to_string()
    }
}

pub fn extra_config_prefill(params_raw: &str) -> String {
    let obj = serde_json::from_str::<serde_json::Value>(params_raw)
        .ok()
        .and_then(|value| value.as_object().cloned());
    let Some(obj) = obj else {
        return String::new();
    };
    let hit = obj
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("config"))
        .map(|(_, value)| value);
    match hit {
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(serde_json::Value::Object(_)) | Some(serde_json::Value::Array(_)) => {
            hit.expect("config value present").to_string()
        }
        _ => String::new(),
    }
}

pub fn validate_extra_params(params_raw: &str) -> Result<(), String> {
    let obj = serde_json::from_str::<serde_json::Value>(params_raw)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or_else(|| "extra-core params must be JSON object".to_string())?;
    let exe_ok = ["exe", "binary"].iter().any(|wanted| {
        obj.iter().any(|(key, value)| {
            key.eq_ignore_ascii_case(wanted)
                && matches!(value, serde_json::Value::String(text) if !text.trim().is_empty())
        })
    });
    if !exe_ok {
        return Err("extra-core params need exe (pick the executable with Browse)".to_string());
    }
    let config = obj
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("config"))
        .map(|(_, value)| value);
    match config {
        None => {
            Err("extra-core params need config (passed to the executable as -c <file>)".to_string())
        }
        Some(serde_json::Value::String(text)) if text.trim().is_empty() => {
            Err("extra-core params need config (passed to the executable as -c <file>)".to_string())
        }
        Some(serde_json::Value::String(text)) => serde_json::from_str::<serde_json::Value>(text)
            .map(|_| ())
            .map_err(|_| "extra-core config string must be valid JSON".to_string()),
        Some(serde_json::Value::Object(_)) | Some(serde_json::Value::Array(_)) => Ok(()),
        Some(_) => Err("extra-core config must be a JSON object or a JSON string".to_string()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainHopSlot {
    Chain,
    Autoselector,
    Extra,
    Other,
}

pub fn chain_hop_rejection(
    existing: &[ChainHopSlot],
    candidate: ChainHopSlot,
    index: usize,
    replacing: bool,
) -> Option<String> {
    if candidate == ChainHopSlot::Chain {
        return Some(
            "chain hops cannot nest a chain (v1 chains are flat — pick a non-chain row)"
                .to_string(),
        );
    }
    if candidate == ChainHopSlot::Autoselector {
        return Some(
            "chain hops cannot take an autoselector (v1 chains are flat — pick a concrete row)"
                .to_string(),
        );
    }
    let len = existing.len();
    if (!replacing && index > len) || (replacing && index >= len) {
        return Some("hop index is out of range (pick the hop to replace)".to_string());
    }
    if candidate == ChainHopSlot::Extra {
        if index != 0 {
            return Some(
                "extracore hops must sit at index 0 (first hop only — off-index-0 rejected)"
                    .to_string(),
            );
        }
        let surviving = existing
            .iter()
            .enumerate()
            .any(|(slot, kind)| *kind == ChainHopSlot::Extra && (!replacing || slot != index));
        if surviving {
            return Some("at most one extracore hop per chain".to_string());
        }
    }
    None
}

pub fn chain_nodes_of(params_raw: &str) -> Vec<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(params_raw)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .and_then(|obj| obj.get("nodes").cloned())
        .and_then(|nodes| nodes.as_array().cloned())
        .unwrap_or_default()
}

pub fn fold_chain_nodes_into_params(params_raw: &str, nodes: &[serde_json::Value]) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(params_raw) {
        Ok(value) => value,
        Err(_) => return params_raw.to_string(),
    };
    let obj = match value.as_object_mut() {
        Some(obj) => obj,
        None => return params_raw.to_string(),
    };
    let current: Vec<serde_json::Value> = obj
        .get("nodes")
        .and_then(|nodes| nodes.as_array().cloned())
        .unwrap_or_default();
    if current == nodes {
        return params_raw.to_string();
    }
    obj.insert(
        "nodes".to_string(),
        serde_json::Value::Array(nodes.to_vec()),
    );
    value.to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuidedIdentity {
    pub key: Option<&'static str>,

    pub read_keys: &'static [&'static str],

    pub needs: &'static str,
}

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

        "mieru" => GuidedIdentity {
            key: Some("password"),
            read_keys: &["password"],
            needs: "username and password",
        },
        "juicity" => GuidedIdentity {
            key: Some("uuid"),
            read_keys: &["uuid", "id"],
            needs: "uuid and password",
        },
        "trusttunnel" => GuidedIdentity {
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
        "socks" | "http" | "https" | "shadowtls" | "direct" => GuidedIdentity {
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

pub fn guided_prefill_value(params_raw: &str, keys: &[&str]) -> String {
    let obj = serde_json::from_str::<serde_json::Value>(params_raw)
        .ok()
        .and_then(|value| value.as_object().cloned());
    match obj {
        Some(obj) => lookup_case_insensitive(&obj, keys).unwrap_or_default(),
        None => String::new(),
    }
}

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

/// F5: Name is the first row in the form, so it is named before any other.
pub fn blank_name(name: &str) -> Option<(&'static str, String)> {
    let mut errors = Vec::new();
    validated_name(name, &mut errors)
        .is_none()
        .then(|| (errors[0].field, errors[0].message.clone()))
}

/// F5: which guided row is blank in the params the form would store.
///
/// `store.add` can only blame the folded `params` JSON, which the guided
/// protocols never show — so the form asks here first and names the input that
/// fills the gap. Only rows the form actually renders are reported: virtual
/// protocols author their params raw, and only vless/vmess/tuic/xray put the
/// credential in a protocol block.
pub fn guided_blank_field(params_raw: &str, protocol: &str) -> Option<(&'static str, String)> {
    let canonical = canonical_protocol(protocol)?;
    if is_virtual_protocol(&canonical) {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(params_raw).ok()?;
    let obj = value.as_object()?;

    let server_ok = lookup_case_insensitive(obj, &["server", "address"])
        .is_some_and(|server| !server.chars().any(|c| c.is_whitespace() || c.is_control()));
    if !server_ok {
        return Some(("guided_server", "Server is required".to_string()));
    }
    let port_ok = ["server_port", "port", "serverPort"].iter().any(|key| {
        let valid = |number: u64| (1..=65_535).contains(&number);
        match obj.get(*key) {
            Some(serde_json::Value::Number(number)) => number.as_u64().is_some_and(valid),
            Some(serde_json::Value::String(text)) => text.trim().parse::<u64>().is_ok_and(valid),
            _ => false,
        }
    });
    if !port_ok {
        return Some(("guided_port", "Server port is required".to_string()));
    }
    if matches!(canonical.as_str(), "vless" | "vmess" | "tuic" | "xray")
        && lookup_case_insensitive(obj, &["uuid", "id"]).is_none()
    {
        return Some(("uuid", "UUID is required".to_string()));
    }
    None
}

fn value_is_import_shaped(obj: &serde_json::Map<String, serde_json::Value>) -> bool {
    let has_kind = obj.contains_key("type") || obj.contains_key("protocol");
    let has_host = obj.contains_key("server") || obj.contains_key("address");
    let has_port = obj.contains_key("server_port") || obj.contains_key("port");
    has_kind && has_host && has_port
}

pub fn is_import_shaped_params(params: &str) -> bool {
    match serde_json::from_str::<serde_json::Value>(params) {
        Ok(serde_json::Value::Object(map)) => value_is_import_shaped(&map),
        _ => false,
    }
}

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

fn validate_protocol_params(
    protocol_raw: &str,
    params_raw: &str,
    errors: &mut Vec<FieldError>,
) -> Option<(String, Secret)> {
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

            "ssh" => has(&["username", "user"]) && has(&["password", "private_key", "privatekey"]),
            "noise" => has(&["password", "key"]),

            "snell" => has(&["psk"]),

            "mieru" => has(&["username", "user"]) && has(&["password"]),
            "juicity" => has(&["uuid", "id"]) && has(&["password"]),
            "trusttunnel" => has(&["username", "user"]) && has(&["password"]),
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

                "mieru" => "username and password",
                "juicity" => "uuid and password",
                "trusttunnel" => "username and password",
                _ => "credentials",
            };
            errors.push(missing(what));
            return None;
        }

        if canonical == "hysteria"
            && !(has(&["up", "up_mbps", "upmbps"]) && has(&["down", "down_mbps", "downmbps"]))
        {
            errors.push(missing("up_mbps and down_mbps"));
            return None;
        }

        for key in [
            "brutal_up_mbps",
            "brutal_down_mbps",
            "brutal_up",
            "brutal_down",
            "brutal_upmbps",
            "brutal_downmbps",
        ] {
            let numeric_ok = match obj.get(key) {
                None | Some(serde_json::Value::Null) => true,
                Some(serde_json::Value::Number(_)) => true,
                Some(serde_json::Value::String(text)) if text.trim().is_empty() => true,
                Some(serde_json::Value::String(text)) => {
                    text.trim().parse::<f64>().is_ok_and(f64::is_finite)
                }
                Some(_) => false,
            };
            if !numeric_ok {
                errors.push(FieldError::new(
                    "params",
                    format!("{canonical} params need {key} as a number (Mbps)"),
                ));
                return None;
            }
        }
        if lookup_case_insensitive(obj, &["security"])
            .is_some_and(|security| security.eq_ignore_ascii_case("reality"))
            && !has(&["pbk"])
        {
            errors.push(missing("pbk (required for reality)"));
            return None;
        }
    }
    Some((canonical, Secret::new(params_text)))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscription {
    pub id: String,
    pub label: String,
    pub url: Secret,
    pub group: String,
    pub auto_refresh: bool,

    pub last_refresh_at: Option<i64>,

    pub last_error: Option<String>,

    pub failures: Vec<crate::import::ImportFailure>,

    pub interval_secs: i64,

    pub quota_used: Option<i64>,
    pub quota_total: Option<i64>,
    pub quota_expire: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefreshOutcome {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub duplicates: usize,

    pub skipped: usize,

    pub conflicts: usize,

    pub in_use: usize,
    /// Row labels collected exactly where the counts increment; every entry is
    /// a display label — never params or URL fragments.
    pub added_names: Vec<String>,
    pub updated_names: Vec<String>,
    pub removed_names: Vec<String>,
    pub kept_in_use_names: Vec<String>,
    pub failures: Vec<crate::import::ImportFailure>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GroupRenameOutcome {
    pub rows: usize,
    pub subscriptions: usize,
}

pub const DEFAULT_AUTO_REFRESH_SECS: i64 = 86_400;

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

pub fn due_for_refresh(subscription: &Subscription, now: i64, interval_secs: i64) -> bool {
    if !subscription.auto_refresh {
        return false;
    }
    match subscription.last_refresh_at {
        None => true,
        Some(last) => interval_secs <= 0 || now.saturating_sub(last) >= interval_secs,
    }
}

pub fn due_for_refresh_for(subscription: &Subscription, now: i64) -> bool {
    due_for_refresh(subscription, now, subscription.interval_secs)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SubscriptionQuota {
    pub used: Option<i64>,
    pub total: Option<i64>,
    pub expire: Option<i64>,
}

pub fn parse_subscription_userinfo(header: &str) -> Option<SubscriptionQuota> {
    fn parse_int(raw: &str) -> Option<i64> {
        let text = raw.trim();
        if text.is_empty() {
            return None;
        }

        if !text.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let digits = text.trim_start_matches('0');
        let digits = if digits.is_empty() { "0" } else { digits };

        if digits.len() > 19 {
            return Some(i64::MAX);
        }
        digits.parse::<i64>().ok()
    }
    let mut upload: Option<i64> = None;
    let mut download: Option<i64> = None;
    let mut total: Option<i64> = None;
    let mut expire: Option<i64> = None;
    let mut seen_any = false;
    for part in header.split(';') {
        let (key, value) = match part.split_once('=') {
            Some(pair) => pair,
            None => continue,
        };
        seen_any = true;
        match key.trim().to_ascii_lowercase().as_str() {
            "upload" => upload = parse_int(value).or(upload),
            "download" => download = parse_int(value).or(download),
            "total" => total = parse_int(value).or(total),
            "expire" => expire = parse_int(value).or(expire),
            _ => {}
        }
    }
    if !seen_any {
        return None;
    }
    let used = match (upload, download) {
        (Some(up), Some(down)) => Some(up.saturating_add(down)),
        (Some(up), None) => Some(up),
        (None, Some(down)) => Some(down),
        (None, None) => None,
    };
    if used.is_none() && total.is_none() && expire.is_none() {
        return None;
    }
    Some(SubscriptionQuota {
        used,
        total,
        expire,
    })
}

fn map_subscription_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Subscription> {
    let id: i64 = row.get(0)?;
    let auto_refresh: i64 = row.get(4)?;
    let failures_raw: Option<String> = row.get(7)?;

    let interval_secs: Option<i64> = row.get(8).ok();

    let quota_used: Option<i64> = row.get(9).ok().flatten();
    let quota_total: Option<i64> = row.get(10).ok().flatten();
    let quota_expire: Option<i64> = row.get(11).ok().flatten();
    Ok(Subscription {
        id: id.to_string(),
        label: row.get(1)?,

        url: {
            let stored: String = row.get(2)?;
            if stored.trim_start().starts_with(crate::secret::MARKER) {
                crate::secret::Secret::open(&stored).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            error.to_string(),
                        )),
                    )
                })?
            } else {
                Secret::new(stored)
            }
        },
        group: row.get(3)?,
        auto_refresh: auto_refresh != 0,
        last_refresh_at: row.get(5)?,
        last_error: row.get(6)?,
        failures: decode_failures(failures_raw),
        interval_secs: interval_secs.unwrap_or(DEFAULT_AUTO_REFRESH_SECS),
        quota_used,
        quota_total,
        quota_expire,
    })
}

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
                .and_then(ImportErrorKind::parse)
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

    if let Some(map) = &parsed {
        if is_awg_like(protocol, map) {
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RestoreOutcome {
    pub configs: usize,
    pub subscriptions: usize,
    pub tombstones: usize,
    pub presets: usize,
}

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

fn sibling_sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut os = path.as_os_str().to_owned();
    os.push(suffix);
    PathBuf::from(os)
}

pub const DATA_DIR_NAME: &str = "data";

pub fn app_data_dir_in(app_dir: Option<&Path>) -> PathBuf {
    match app_dir {
        Some(dir) => dir.join(DATA_DIR_NAME),
        // Unresolvable app dir (edge dev cases): stay cwd-relative — the
        // AppData and home-dot defaults are banned, never read, never recreated.
        None => PathBuf::from(DATA_DIR_NAME),
    }
}

pub fn app_data_dir() -> PathBuf {
    app_data_dir_in(crate::engine::current_app_dir().as_deref())
}

/// T-08: one probe, against one folder. Split out from `Store::write_probe`
/// so the store's in-memory case stays outside the filesystem path entirely.
fn write_probe_in(dir: &Path) -> Result<(), String> {
    use std::io::Write as _;

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let seq = WRITE_PROBE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let stem = format!(".mp-write-probe-{}-{stamp}-{seq}", std::process::id());
    let temp = dir.join(format!("{stem}.tmp"));
    let renamed = dir.join(format!("{stem}.ok"));

    let outcome = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(b"manyproxy write probe\n")?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, &renamed)?;
        std::fs::remove_file(&renamed)?;
        Ok(())
    })();

    // Never leave a probe file behind, whichever step failed.
    let _ = std::fs::remove_file(&temp);
    let _ = std::fs::remove_file(&renamed);

    outcome.map_err(|error| format!("cannot write in {}: {error}", dir.display()))
}

pub const RULE_SETS_DIR_NAME: &str = "rule-sets";

/// T-12: staged `.srs` rule sets live beside the app under `data\rule-sets\`
/// (same T-02 family as the library DB): the folder ships inside the portable
/// ZIP and the sidecar path points at app-dir files only.
pub fn rule_sets_dir_in(app_dir: Option<&Path>) -> PathBuf {
    app_data_dir_in(app_dir).join(RULE_SETS_DIR_NAME)
}

pub fn rule_sets_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("MANYPROXY_RULE_SETS_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    rule_sets_dir_in(crate::engine::current_app_dir().as_deref())
}

/// The staged file for a rule-set name: name sanitized with the same grammar
/// the engine uses for tags (`sanitize_tag`), so the emission path derived
/// from a rule reference lands exactly on the file staging wrote.
pub fn rule_set_file_stem(name: &str) -> String {
    crate::engine::sanitize_tag(name.trim())
}

pub fn rule_set_file_name(name: &str) -> String {
    format!("{}.srs", rule_set_file_stem(name))
}

pub fn staged_rule_set_path(file_name: &str) -> PathBuf {
    rule_sets_dir().join(file_name)
}

/// A tag-sanitizing name can still land on a reserved Windows device name
/// ("con", "nul", …); such a file cannot be created, so staging rejects it
/// loudly at name validation instead of failing at first download.
fn windows_reserved_stem(stem: &str) -> bool {
    matches!(
        stem.to_ascii_lowercase().as_str(),
        "con"
            | "prn"
            | "aux"
            | "nul"
            | "com1"
            | "com2"
            | "com3"
            | "com4"
            | "com5"
            | "com6"
            | "com7"
            | "com8"
            | "com9"
            | "lpt1"
            | "lpt2"
            | "lpt3"
            | "lpt4"
            | "lpt5"
            | "lpt6"
            | "lpt7"
            | "lpt8"
            | "lpt9"
    )
}

/// Keeps `<appdir>\data\rule-sets\<stem>.srs` far away from MAX_PATH trouble.
pub const RULE_SET_STEM_MAX_CHARS: usize = 80;

static NAME_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn unique_name_suffix(ts: i64) -> String {
    let n = NAME_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{ts}-{}-{n}", std::process::id())
}

/// T-32: the persisted window rectangle in gpui logical points — the payload
/// behind [`Store::META_GEOMETRY`]. Absent or corrupt meta reads as `None`, so
/// the caller keeps its own default rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl WindowGeometry {
    /// T-32: the same minimums `app_window_options` hands the window, kept
    /// here so a restored rectangle can never seed a window below them.
    pub const MIN_WIDTH: f32 = 880.;
    pub const MIN_HEIGHT: f32 = 620.;

    /// T-32: the sane position box — anything further out is clamped rather
    /// than dropped, so even a hand-edited row lands somewhere visible.
    pub const POSITION_MIN: f32 = -999.;
    pub const POSITION_MAX: f32 = 9999.;

    /// T-32: the persistence clamp. Non-finite input reads as no geometry at
    /// all; size bottoms out at the window minimums and position at the box.
    pub fn clamped(self) -> Option<Self> {
        if ![self.x, self.y, self.width, self.height]
            .iter()
            .all(|value| value.is_finite())
        {
            return None;
        }
        Some(Self {
            x: self.x.clamp(Self::POSITION_MIN, Self::POSITION_MAX),
            y: self.y.clamp(Self::POSITION_MIN, Self::POSITION_MAX),
            width: self.width.max(Self::MIN_WIDTH),
            height: self.height.max(Self::MIN_HEIGHT),
        })
    }

    /// Canonical meta payload: `{"x":320,"y":180,"width":1280,"height":900}`.
    pub fn to_json(self) -> String {
        format!(
            "{{\"x\":{},\"y\":{},\"width\":{},\"height\":{}}}",
            self.x, self.y, self.width, self.height
        )
    }

    /// Tolerant read: anything that is not a complete numeric object, or that
    /// cannot be made sane, reads as no geometry.
    pub fn from_json(raw: &str) -> Option<Self> {
        let Ok(serde_json::Value::Object(map)) = serde_json::from_str(raw.trim()) else {
            return None;
        };
        let number = |key: &str| {
            map.get(key)
                .and_then(|value| value.as_f64())
                .map(|value| value as f32)
        };
        Self {
            x: number("x")?,
            y: number("y")?,
            width: number("width")?,
            height: number("height")?,
        }
        .clamped()
    }
}

pub struct Store {
    conn: Connection,

    db_path: Option<PathBuf>,
}

/// T-08: probe file names carry a process-wide sequence as well as a clock
/// stamp, so two probes in the same nanosecond (tick plus save) never share a
/// path.
static WRITE_PROBE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

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
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::open_with_path(Connection::open_in_memory()?, None)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn break_schema_for_test(&self) -> Result<(), StoreError> {
        self.conn.execute_batch(
            "DROP TABLE proxy_configs; DROP TABLE subscriptions; DROP TABLE route_presets;",
        )?;
        Ok(())
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn set_query_only_for_test(&self) -> Result<(), StoreError> {
        self.conn.execute_batch("PRAGMA query_only = ON;")?;
        Ok(())
    }

    /// T-32: read-only peek at one meta value for startup, before the real
    /// store opens. Never creates, migrates, or quarantines anything — any
    /// problem (missing file, corrupt library, locked database) reads as
    /// `None`, leaving the app's own open path fully in charge of recovery.
    pub fn peek_meta_readonly(path: &Path, key: &str) -> Option<String> {
        if !path.exists() {
            return None;
        }
        let flags =
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = Connection::open_with_flags(path, flags).ok()?;
        let mut stmt = conn.prepare("SELECT value FROM meta WHERE key = ?1").ok()?;
        stmt.query_row(params![key], |row| row.get::<_, String>(0))
            .ok()
    }

    pub fn open_file(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|error| StoreError::Io {
                    path: parent.to_path_buf(),
                    message: error.to_string(),
                })?;
            }
        }

        if path.exists() {
            if let Ok(meta) = std::fs::metadata(path) {
                if meta.len() > 0 {
                    if let Ok(mut file) = std::fs::File::open(path) {
                        use std::io::Read as _;
                        let mut head = [0u8; 16];

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
            if path.exists() {
                let _ = Self::quarantine_corrupt_file(path);
            }
            StoreError::Sql(e)
        })?;
        let store = match Self::open_with_path(conn, Some(path.to_path_buf())) {
            Ok(store) => store,
            Err(error) => {
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

        for sibling in [sibling_sidecar(path, "-wal"), sibling_sidecar(path, "-shm")] {
            if sibling.exists() {
                harden_file_permissions(&sibling);
            }
        }
        Ok(store)
    }

    pub fn default_path() -> PathBuf {
        if let Ok(path) = std::env::var("MANYPROXY_DB") {
            if !path.trim().is_empty() {
                return PathBuf::from(path);
            }
        }
        app_data_dir().join("library.db")
    }

    /// T-08: the folder the library writes into. `None` for an in-memory
    /// store — it has no folder, so it has nothing to lose.
    pub fn data_dir(&self) -> Option<PathBuf> {
        self.db_path
            .as_ref()
            .and_then(|path| path.parent())
            .map(Path::to_path_buf)
    }

    /// T-08: the only honest test of writability is a write. The probe creates
    /// a temp file inside the data folder, writes bytes, flushes them to the
    /// OS, renames the file, then removes it. Metadata and a successful read
    /// are not evidence: a read-only folder still reads. An in-memory store has
    /// no folder to probe and answers `Ok(())` — the test suite and the
    /// no-persistence fallback run on it, and a false banner there would be a
    /// defect.
    pub fn write_probe(&self) -> Result<(), String> {
        match self.data_dir() {
            Some(dir) => write_probe_in(&dir),
            None => Ok(()),
        }
    }

    fn open_with_path(conn: Connection, db_path: Option<PathBuf>) -> Result<Self, StoreError> {
        let mut store = Self { conn, db_path };
        store.apply_pragmas()?;
        store.create_schema()?;

        let now = now_unix();
        let _ = store.gc_health_history(90, now);
        Ok(store)
    }

    pub fn db_path(&self) -> Option<PathBuf> {
        self.db_path.clone()
    }

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

        for suffix in ["-wal", "-shm", "-journal"] {
            let _ = std::fs::remove_file(sibling_sidecar(path, suffix));
        }
        Ok(quarantined)
    }

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

    fn remove_restore_temp(tmp: &Path) {
        let _ = std::fs::remove_file(tmp);
        for suffix in ["-wal", "-shm", "-journal"] {
            let _ = std::fs::remove_file(sibling_sidecar(tmp, suffix));
        }
    }

    pub fn restore_library_file(
        db_path: &Path,
        payload: &str,
        now: i64,
    ) -> Result<(RestoreOutcome, Option<PathBuf>), StoreError> {
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

        if let Some(parent) = db_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|error| StoreError::Io {
                    path: parent.to_path_buf(),
                    message: error.to_string(),
                })?;
            }
        }

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

        Self::remove_restore_temp(&tmp);
        let outcome = (|| -> Result<RestoreOutcome, StoreError> {
            let mut tmp_store = Store::open_with_path(
                Connection::open(&tmp).map_err(StoreError::from)?,
                Some(tmp.clone()),
            )?;
            let outcome = tmp_store.restore_library_json(payload)?;
            tmp_store.check_quick()?;

            let _ = tmp_store
                .conn
                .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
            drop(tmp_store);
            Ok(outcome)
        })()
        .inspect_err(|_| Self::remove_restore_temp(&tmp))?;
        swap_tmp_over_live(&tmp, db_path).map_err(|error| {
            Self::remove_restore_temp(&tmp);
            StoreError::Io {
                path: db_path.to_path_buf(),
                message: error.to_string(),
            }
        })?;

        for suffix in ["-wal", "-shm", "-journal"] {
            let _ = std::fs::remove_file(sibling_sidecar(db_path, suffix));
        }
        if let Some(parent) = db_path.parent() {
            if !parent.as_os_str().is_empty() {}
        }
        harden_file_permissions(db_path);
        Ok((outcome, backup))
    }

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

    const SCHEMA_VERSION: i64 = 6;

    const MIN_SQLITE: (i64, i64, i64) = (3, 37, 0);

    fn seal_or_fail(plaintext: &str) -> Result<String, StoreError> {
        crate::secret::Secret::new(plaintext)
            .seal()
            .map_err(|error| StoreError::Crypto {
                message: format!("dpapi seal failed: {error}"),
            })
    }

    fn unseal_or_fail(stored: &str) -> Result<String, StoreError> {
        crate::secret::Secret::open(stored)
            .map(|secret| secret.into_plaintext())
            .map_err(|error| StoreError::Crypto {
                message: format!("dpapi open failed: {error}"),
            })
    }

    fn schema_error(message: String) -> StoreError {
        StoreError::Sql(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::new(std::io::ErrorKind::InvalidData, message),
        )))
    }

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
               synced_params TEXT,
                sort_order INTEGER NOT NULL DEFAULT 0
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
               interval_secs INTEGER NOT NULL DEFAULT 86400,
                quota_used INTEGER,
                quota_total INTEGER,
                quota_expire INTEGER
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
                last_used_at INTEGER,
                locked INTEGER NOT NULL DEFAULT 0
              ) STRICT;
             CREATE TABLE IF NOT EXISTS meta (
               key TEXT PRIMARY KEY,
               value TEXT NOT NULL DEFAULT ''
             ) STRICT;
             CREATE TABLE IF NOT EXISTS rule_sets (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               name TEXT NOT NULL UNIQUE,
               url TEXT NOT NULL,
               file_name TEXT NOT NULL UNIQUE,
               bytes INTEGER NOT NULL,
               downloaded_at INTEGER NOT NULL
             ) STRICT;",
        )?;

        self.ensure_column("proxy_configs", "preset_id", "TEXT")?;

        for (column, ddl) in [("subscription_id", "TEXT"), ("identity_key", "TEXT")] {
            self.ensure_column("proxy_configs", column, ddl)?;
        }

        self.ensure_column("subscriptions", "failures", "TEXT NOT NULL DEFAULT '[]'")?;

        self.ensure_column("proxy_configs", "adoptable", "INTEGER NOT NULL DEFAULT 0")?;
        self.ensure_column("proxy_configs", "synced_params", "TEXT")?;

        self.ensure_column("proxy_configs", "traffic_rx", "INTEGER NOT NULL DEFAULT 0")?;
        self.ensure_column("proxy_configs", "traffic_tx", "INTEGER NOT NULL DEFAULT 0")?;

        if !self.column_exists("proxy_configs", "sort_order")? {
            self.ensure_column("proxy_configs", "sort_order", "INTEGER NOT NULL DEFAULT 0")?;
            self.conn
                .execute("UPDATE proxy_configs SET sort_order = id", [])?;
        }

        self.ensure_column(
            "subscriptions",
            "interval_secs",
            "INTEGER NOT NULL DEFAULT 86400",
        )?;
        self.ensure_column("route_presets", "description", "TEXT")?;
        self.ensure_column("route_presets", "is_builtin", "INTEGER NOT NULL DEFAULT 0")?;
        self.ensure_column("route_presets", "last_used_at", "INTEGER")?;
        self.ensure_column("route_presets", "locked", "INTEGER NOT NULL DEFAULT 0")?;
        self.ensure_column("subscription_tombstones", "subscription_url", "TEXT")?;
        self.ensure_column("subscription_tombstones", "rotation_core", "TEXT")?;

        for (column, ddl) in [
            ("quota_used", "INTEGER"),
            ("quota_total", "INTEGER"),
            ("quota_expire", "INTEGER"),
        ] {
            self.ensure_column("subscriptions", column, ddl)?;
        }

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
                let stored: Option<String> = self
                    .conn
                    .query_row(
                        "SELECT url FROM subscriptions WHERE id = ?1",
                        params![sub_id.parse::<i64>().unwrap_or(-1)],
                        |r| r.get(0),
                    )
                    .ok();
                let url = match stored {
                    Some(stored) if stored.trim_start().starts_with(crate::secret::MARKER) => {
                        // A URL sealed for another user/machine must not abort
                        // schema setup: skip this tombstone's backfill (leave
                        // subscription_url NULL) instead of failing the open.
                        Self::unseal_or_fail(&stored).ok()
                    }
                    other => other,
                };
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
                    if value.starts_with(crate::secret::MARKER) {
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
                if url.trim_start().starts_with(crate::secret::MARKER) {
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

        self.conn
            .execute_batch(&format!("PRAGMA user_version = {};", Self::SCHEMA_VERSION))?;
        Ok(())
    }

    pub fn add(&mut self, input: NewConfig) -> Result<ProxyConfig, StoreError> {
        let mut created = self.add_batch(vec![input])?;
        created
            .pop()
            .ok_or_else(|| StoreError::NotFound("batch was empty".to_string()))
    }

    pub fn add_batch(&mut self, inputs: Vec<NewConfig>) -> Result<Vec<ProxyConfig>, StoreError> {
        struct Fields {
            name: String,
            protocol: String,
            params: Secret,
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
                proto_params.unwrap_or((DEFAULT_PROTOCOL.to_string(), Secret::new("{}")));
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
                None => match take_free_auto_port(auto_port_candidates(), &mut taken) {
                    Ok(port) => port,
                    Err(reason) => {
                        return Err(StoreError::Invalid(vec![FieldError::new("port", reason)]));
                    }
                },
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
            let rowid = tx.last_insert_rowid();
            ids.push(rowid);

            tx.execute(
                "UPDATE proxy_configs SET sort_order = ?1 WHERE id = ?1",
                params![rowid],
            )?;
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

    pub fn list(&self) -> Result<Vec<ProxyConfig>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, protocol, params, grp, enabled, bind_ip, port,
              subscription_id, identity_key, preset_id
             FROM proxy_configs ORDER BY sort_order, id",
        )?;
        let rows = stmt
            .query_map([], map_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

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

    pub fn list_with_quarantine(
        &self,
    ) -> Result<(Vec<ProxyConfig>, Vec<QuarantinedRow>), StoreError> {
        let ids: Vec<i64> = {
            let mut id_stmt = self
                .conn
                .prepare("SELECT id FROM proxy_configs ORDER BY sort_order, id")?;
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

    pub fn export_library_json_with_quarantine(&self) -> Result<(String, Vec<String>), StoreError> {
        let (rows, quarantined) = self.list_with_quarantine()?;
        let mut skipped: Vec<String> = quarantined.iter().map(|q| q.id.clone()).collect();
        let (rows, mut bad_synced) = self.without_unreadable_synced(rows);
        skipped.append(&mut bad_synced);

        let subs = self.list_subscriptions()?;
        let tombstones = self.list_tombstones_raw()?;
        let payload = export_payload_full(&rows, &subs, &tombstones);

        let enriched = self.enrich_export_payload(&payload, &rows)?;
        Ok((enriched, skipped))
    }

    fn list_tombstones_raw(&self) -> Result<Vec<(String, String)>, StoreError> {
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

    pub fn remove(&mut self, id: &str) -> Result<bool, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Ok(false),
        };
        let current = self.get(id)?;

        let owner_url: Option<String> = match &current {
            Some(row) => match &row.subscription_id {
                Some(sub_id) => self
                    .get_subscription(sub_id)?
                    .map(|s| s.url.into_plaintext()),
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
                    // shortcut: the tombstone's url stays a plaintext column —
                    // it is an equality key and DPAPI seals are not
                    // deterministic, so matching would mean opening every row.
                    // Upgrade when a sealed column plus a migration is worth the
                    // cost, or when the url becomes a token-bearing secret.
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

            let _ = tx.execute(
                "DELETE FROM health_history WHERE config_id = ?1",
                params![id],
            );
        }
        tx.commit()?;
        Ok(removed > 0)
    }

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

    pub fn rename_group(&mut self, old: &str, new: &str) -> Result<GroupRenameOutcome, StoreError> {
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

        let moved_subs = self.conn.execute(
            "UPDATE subscriptions SET grp = ?1 WHERE grp = ?2",
            params![new, old],
        )?;

        if old != new {
            let old_key = Self::group_skip_key(&old);
            if let Some(value) = self.get_meta(&old_key)? {
                self.set_meta(&Self::group_skip_key(&new), &value)?;
                self.conn
                    .execute("DELETE FROM meta WHERE key = ?1", params![old_key])?;
            }
            // T-19: the auto-clear flag follows the renamed group; when the
            // new name already carries its own entry it wins, and the stale
            // old key never lingers.
            let mut auto_clear_groups = self.auto_clear_groups_map();
            if let Some(migrated) = auto_clear_groups.remove(&old) {
                auto_clear_groups.entry(new.clone()).or_insert(migrated);
                self.auto_clear_groups_write(&auto_clear_groups)?;
            }
        }
        Ok(GroupRenameOutcome {
            rows: changed,
            subscriptions: moved_subs,
        })
    }

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
        let subscriptions: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM subscriptions WHERE grp = ?1",
            params![name],
            |row| row.get(0),
        )?;
        if subscriptions > 0 {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "group",
                format!(
                    "Group \"{name}\" still holds {subscriptions} subscription(s); remove or regroup them first"
                ),
            )]));
        }
        Ok(())
    }

    fn rows_in_group(&self, group: &str) -> Result<Vec<ProxyConfig>, StoreError> {
        let group = normalized_group(group);
        Ok(self
            .list()?
            .into_iter()
            .filter(|row| row.group == group)
            .collect())
    }

    pub fn duplicate_rows_in_group(&self, group: &str) -> Result<Vec<ProxyConfig>, StoreError> {
        let mut seen = HashSet::new();
        let mut dupes = Vec::new();
        for row in self.rows_in_group(group)? {
            let key = widened_identity(&row.protocol, &row.params);
            if !seen.insert(key) {
                dupes.push(row);
            }
        }
        Ok(dupes)
    }

    pub fn duplicate_preview_in_group(
        &self,
        group: &str,
    ) -> Result<(usize, Vec<String>), StoreError> {
        let dupes = self.duplicate_rows_in_group(group)?;
        let total = dupes.len();
        let names = dupes
            .into_iter()
            .take(PRUNE_PREVIEW_LIMIT)
            .map(|row| row.name)
            .collect();
        Ok((total, names))
    }

    pub fn remove_duplicates_in_group(
        &mut self,
        group: &str,
        skip: &HashSet<String>,
    ) -> Result<Vec<ProxyConfig>, StoreError> {
        let dupes = self.duplicate_rows_in_group(group)?;
        let mut removed = Vec::with_capacity(dupes.len());
        for row in dupes {
            if skip.contains(&row.id) {
                continue;
            }
            if self.remove(&row.id)? {
                removed.push(row);
            }
        }
        Ok(removed)
    }

    pub fn unavailable_rows_in_group(&self, group: &str) -> Result<Vec<ProxyConfig>, StoreError> {
        let dead: HashSet<String> = self
            .load_health_snapshots()?
            .into_iter()
            .filter(|snap| snap.error.is_some())
            .map(|snap| snap.config_id)
            .collect();
        Ok(self
            .rows_in_group(group)?
            .into_iter()
            .filter(|row| dead.contains(&row.id))
            .collect())
    }

    pub fn remove_unavailable_in_group(
        &mut self,
        group: &str,
        skip: &HashSet<String>,
    ) -> Result<Vec<ProxyConfig>, StoreError> {
        let dead = self.unavailable_rows_in_group(group)?;
        let mut removed = Vec::with_capacity(dead.len());
        for row in dead {
            if skip.contains(&row.id) {
                continue;
            }
            if self.remove(&row.id)? {
                removed.push(row);
            }
        }
        Ok(removed)
    }

    pub fn invalid_rows_in_group(&self, group: &str) -> Result<Vec<ProxyConfig>, StoreError> {
        Ok(self
            .rows_in_group(group)?
            .into_iter()
            .filter(|row| !is_valid_config(&row.protocol, &row.params))
            .collect())
    }

    pub fn remove_invalid_in_group(
        &mut self,
        group: &str,
        skip: &HashSet<String>,
    ) -> Result<Vec<ProxyConfig>, StoreError> {
        let bad = self.invalid_rows_in_group(group)?;
        let mut removed = Vec::with_capacity(bad.len());
        for row in bad {
            if skip.contains(&row.id) {
                continue;
            }
            if self.remove(&row.id)? {
                removed.push(row);
            }
        }
        Ok(removed)
    }

    pub fn clear_tests_in_group(&mut self, group: &str) -> Result<usize, StoreError> {
        let group = normalized_group(group);
        let cleared = self.conn.execute(
            "DELETE FROM health_snapshots WHERE config_id IN
              (SELECT id FROM proxy_configs WHERE grp = ?1)",
            params![group],
        )?;
        let _ = self.conn.execute(
            "DELETE FROM health_history WHERE config_id IN
              (SELECT id FROM proxy_configs WHERE grp = ?1)",
            params![group],
        );
        Ok(cleared)
    }

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

    pub fn set_config_preset(
        &mut self,
        config_id: &str,
        preset_id: Option<&str>,
    ) -> Result<ProxyConfig, StoreError> {
        let rowid: i64 = match config_id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Err(StoreError::NotFound(config_id.to_string())),
        };
        if let Some(preset) = preset_id {
            let rowid_preset: i64 = preset.parse().unwrap_or(-1);
            let rule_count: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM route_presets WHERE id = ?1",
                params![rowid_preset],
                |row| row.get(0),
            )?;
            if rule_count == 0 {
                return Err(StoreError::NotFound(preset.to_string()));
            }

            let rules_json: String = self.conn.query_row(
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

            // T-12 attachment-time validation: every `rule_set` reference must
            // name a staged set at the moment the preset attaches to a row.
            // Same loudness taste as unknown exits — loud here, loud again at
            // config generation, never silent.
            let staged = self.staged_rule_set_names()?;
            let rules: Vec<serde_json::Value> =
                serde_json::from_str::<serde_json::Value>(&rules_json)
                    .ok()
                    .and_then(|value| value.as_array().cloned())
                    .unwrap_or_default();
            if let Err(error) = crate::preset::validate_rule_set_refs_staged(&rules, &staged) {
                return Err(StoreError::Invalid(vec![FieldError::new(
                    "preset",
                    error.message,
                )]));
            }
        }
        let tx = self.conn.transaction()?;
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

    #[cfg(any(test, feature = "test-support"))]
    pub fn find_orphaned_preset_rows(&self) -> Result<Vec<ProxyConfig>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, protocol, params, grp, enabled, bind_ip, port,
              subscription_id, identity_key, preset_id
             FROM proxy_configs
             WHERE preset_id IS NOT NULL
               AND preset_id NOT IN (SELECT id FROM route_presets)
             ORDER BY id",
        )?;

        let rows = stmt
            .query_map([], map_row)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    #[cfg(any(test, feature = "test-support"))]
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

    pub fn create_preset(
        &mut self,
        name: &str,
        source: &str,
        source_url: Option<&str>,
        rules: &[serde_json::Value],
    ) -> Result<RoutePreset, StoreError> {
        let name = Self::validated_preset_name(name)?;
        let rules = {
            let mut rules = rules.to_vec();
            crate::preset::cap_rule_names(&mut rules);
            rules
        };
        Self::check_preset_rules(&rules)?;
        let source = Self::validated_preset_source(source);
        let rules_json = serde_json::Value::Array(rules).to_string();
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

    pub fn import_route_link(&mut self, line: &str) -> Result<RoutePreset, StoreError> {
        let parsed = crate::preset::decode_route_link(line).map_err(Self::preset_parse_error)?;
        self.create_preset(&parsed.name, "link", None, &parsed.rules)
    }

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

    pub fn import_remote_preset_shell(
        &mut self,
        name: &str,
        url: &str,
    ) -> Result<RoutePreset, StoreError> {
        self.create_preset(name, "url", Some(url), &[])
    }

    pub fn set_preset_rules(
        &mut self,
        id: &str,
        rules: &[serde_json::Value],
    ) -> Result<RoutePreset, StoreError> {
        let rules = {
            let mut rules = rules.to_vec();
            crate::preset::cap_rule_names(&mut rules);
            rules
        };
        Self::check_preset_rules(&rules)?;

        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Err(StoreError::NotFound(id.to_string())),
        };
        let rules_json = serde_json::Value::Array(rules).to_string();
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

    /// T-10 prevent-modifications lock. An additive-column toggle: locking
    /// protects the preset's rules (the editor renders readonly), never the
    /// attach relationship — attach/detach are unaffected.
    pub fn set_preset_locked(&mut self, id: &str, locked: bool) -> Result<RoutePreset, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Err(StoreError::NotFound(id.to_string())),
        };
        let changed = self.conn.execute(
            "UPDATE route_presets SET locked = ?1 WHERE id = ?2",
            params![locked as i64, rowid],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(id.to_string()));
        }
        self.get_preset(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
    }

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

    #[cfg(any(test, feature = "test-support"))]
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

    #[cfg(any(test, feature = "test-support"))]
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

    pub fn list_presets(&self) -> Result<Vec<RoutePreset>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, source, source_url, rules_json, updated_at,
                    description, is_builtin, last_used_at, locked
             FROM route_presets ORDER BY id",
        )?;
        let rows = stmt
            .query_map([], map_preset_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_preset(&self, id: &str) -> Result<Option<RoutePreset>, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Ok(None),
        };
        let mut stmt = self.conn.prepare(
            "SELECT id, name, source, source_url, rules_json, updated_at,
                    description, is_builtin, last_used_at, locked
             FROM route_presets WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![rowid], map_preset_row)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

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

    /// T-12: staged `.srs` rule-set catalog rows. The binary file is cache
    /// state under `data\rule-sets\` (never DPAPI-sealed — a compiled rule
    /// catalog carries no credentials, like a subscription body); this table
    /// only tracks what was staged, from where, and when.
    pub fn list_rule_sets(&self) -> Result<Vec<RuleSetRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, url, file_name, bytes, downloaded_at
             FROM rule_sets ORDER BY id",
        )?;
        let rows = stmt
            .query_map([], map_rule_set_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_rule_set(&self, id: &str) -> Result<Option<RuleSetRow>, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Ok(None),
        };
        let mut stmt = self.conn.prepare(
            "SELECT id, name, url, file_name, bytes, downloaded_at
             FROM rule_sets WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![rowid], map_rule_set_row)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Records a freshly downloaded `.srs` back as a row. The file itself is
    /// written by the confirmed download flow (`main.rs`) BEFORE this call;
    /// `file_name` mirrors the sanitized name so the engine derivation
    /// (`crate::store::rule_set_file_name`) and the table agree.
    pub fn create_rule_set(
        &self,
        name: &str,
        url: &str,
        bytes: u64,
    ) -> Result<RuleSetRow, StoreError> {
        let name = Self::validated_rule_set_name(name)?;
        let url = url.trim().to_string();
        if bytes > SUBSCRIPTION_MAX_BYTES as u64 {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "bytes",
                format!(
                    "rule set is {} bytes — the staging cap is {SUBSCRIPTION_MAX_BYTES} bytes",
                    bytes
                ),
            )]));
        }
        let file_name = rule_set_file_name(&name);
        match self.conn.execute(
            "INSERT INTO rule_sets (name, url, file_name, bytes, downloaded_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![name, url, file_name, bytes as i64, now_unix()],
        ) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(error, _))
                if error.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE =>
            {
                return Err(StoreError::Invalid(vec![FieldError::new(
                    "name",
                    format!("rule set \"{name}\" is already staged"),
                )]));
            }
            Err(error) => return Err(StoreError::Sql(error)),
        }
        let id = self.conn.last_insert_rowid();
        self.get_rule_set(&id.to_string())?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
    }

    /// Explicit re-Download on the same entry: bytes/URL are refreshed, the
    /// file overwrite itself is the user-confirmed step done by the flow.
    pub fn touch_rule_set(
        &self,
        id: &str,
        url: &str,
        bytes: u64,
    ) -> Result<RuleSetRow, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Err(StoreError::NotFound(id.to_string())),
        };
        if bytes > SUBSCRIPTION_MAX_BYTES as u64 {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "bytes",
                format!(
                    "rule set is {} bytes — the staging cap is {SUBSCRIPTION_MAX_BYTES} bytes",
                    bytes
                ),
            )]));
        }
        let changed = self.conn.execute(
            "UPDATE rule_sets SET url = ?1, bytes = ?2, downloaded_at = ?3 WHERE id = ?4",
            params![url.trim(), bytes as i64, now_unix(), rowid],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(id.to_string()));
        }
        self.get_rule_set(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
    }

    /// Rule sets a preset's rules currently reference (the ticket-named
    /// `rule_sets_in_use(preset_id)` helper). Used by the Remove guard and the
    /// list's "used by" column.
    pub fn rule_sets_in_use(&self, preset_id: &str) -> Result<Vec<String>, StoreError> {
        let Some(preset) = self.get_preset(preset_id)? else {
            return Ok(Vec::new());
        };
        let rules = preset.rules().map_err(|message| {
            StoreError::Sql(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::new(std::io::ErrorKind::InvalidData, message),
            )))
        })?;
        Ok(crate::preset::collect_rule_set_refs(&rules))
    }

    /// Presets whose rules reference the given rule set by name. Remove is
    /// blocked while this is non-empty: used sets must be detached first, so
    /// removal never leaves naming rules silently pointing at nothing.
    pub fn rule_set_used_by(&self, name: &str) -> Result<Vec<String>, StoreError> {
        let mut used = Vec::new();
        for preset in self.list_presets()? {
            if self
                .rule_sets_in_use(&preset.id)?
                .iter()
                .any(|set| set == name)
            {
                used.push(preset.name);
            }
        }
        Ok(used)
    }

    /// Removes the row once nothing references it. Returns the staged
    /// `file_name` so the caller deletes the cache file (no silent orphans:
    /// the flow unlinks it right after, and a failed unlink is loud).
    pub fn delete_rule_set(&self, id: &str) -> Result<Option<String>, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Err(StoreError::NotFound(id.to_string())),
        };
        let Some(row) = self.get_rule_set(id)? else {
            return Ok(None);
        };
        let used = self.rule_set_used_by(&row.name)?;
        if !used.is_empty() {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "rule set",
                crate::preset::rule_set_in_use_message(&row.name, &used),
            )]));
        }
        self.conn
            .execute("DELETE FROM rule_sets WHERE id = ?1", params![rowid])?;
        Ok(Some(row.file_name))
    }

    /// Names currently staged — the attachment-time source of truth for the
    /// `rule_set` reference check in `set_config_preset` below.
    fn staged_rule_set_names(&self) -> Result<BTreeSet<String>, StoreError> {
        let mut names = BTreeSet::new();
        for row in self.list_rule_sets()? {
            names.insert(row.name);
        }
        Ok(names)
    }

    fn validated_preset_name(raw: &str) -> Result<String, StoreError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "name",
                "preset name must not be empty",
            )]));
        }

        Ok(crate::preset::cap_name(trimmed.to_string()))
    }

    /// T-12 staging name validation: the name is both the row label and (via
    /// `rule_set_file_name`) the staged file base, so it must survive the tag
    /// sanitizer with at least one letter/digit left.
    pub fn validated_rule_set_name(raw: &str) -> Result<String, StoreError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "name",
                "rule set name must not be empty",
            )]));
        }
        let stem = rule_set_file_stem(trimmed);
        if !stem.chars().any(|c| c.is_ascii_alphanumeric()) {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "name",
                "rule set name must contain a letter or digit",
            )]));
        }
        if windows_reserved_stem(&stem) {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "name",
                format!(
                    "rule set name \"{trimmed}\" sanitizes to the reserved Windows device name \"{stem}\" — pick another name"
                ),
            )]));
        }
        if stem.len() > RULE_SET_STEM_MAX_CHARS {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "name",
                format!(
                    "rule set name must sanitize to at most {RULE_SET_STEM_MAX_CHARS} file-name characters"
                ),
            )]));
        }
        Ok(trimmed.to_string())
    }

    fn validated_preset_source(raw: &str) -> String {
        match raw.trim() {
            "link" | "url" | "file" | "manual" => raw.trim().to_string(),
            _ => "manual".to_string(),
        }
    }

    fn check_preset_rules(rules: &[serde_json::Value]) -> Result<(), StoreError> {
        if let Err(error) = crate::preset::validate_preset_rules(rules) {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "rules",
                error.message.clone(),
            )]));
        }
        Ok(())
    }

    fn preset_parse_error(error: crate::preset::PresetError) -> StoreError {
        StoreError::Invalid(vec![FieldError::new("preset", error.to_string())])
    }

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

        if matches!(
            subscription_fetch_policy(trimmed),
            SubscriptionFetchPolicy::Block { .. }
        ) {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "url",
                "subscription url must not target internal hosts",
            )]));
        }
        Ok(trimmed.to_string())
    }

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

    pub fn find_subscription_by_url(&self, url: &str) -> Result<Option<Subscription>, StoreError> {
        let trimmed = url.trim();
        for sub in self.list_subscriptions()? {
            if sub.url == trimmed {
                return Ok(Some(sub));
            }
        }
        Ok(None)
    }

    pub fn list_subscriptions(&self) -> Result<Vec<Subscription>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, label, url, grp, auto_refresh, last_refresh_at, last_error, failures, interval_secs,
                     quota_used, quota_total, quota_expire
             FROM subscriptions ORDER BY id",
        )?;
        let rows = stmt
            .query_map([], map_subscription_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_subscription(&self, id: &str) -> Result<Option<Subscription>, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Ok(None),
        };
        let mut stmt = self.conn.prepare(
            "SELECT id, label, url, grp, auto_refresh, last_refresh_at, last_error, failures, interval_secs,
                     quota_used, quota_total, quota_expire
             FROM subscriptions WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![rowid], map_subscription_row)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

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

    pub fn set_subscription_quota(
        &mut self,
        id: &str,
        quota: Option<SubscriptionQuota>,
    ) -> Result<Subscription, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Err(StoreError::NotFound(id.to_string())),
        };
        let (used, total, expire) = match quota {
            Some(quota) => (quota.used, quota.total, quota.expire),
            None => (None, None, None),
        };
        let changed = self.conn.execute(
            "UPDATE subscriptions SET quota_used = ?1, quota_total = ?2, quota_expire = ?3
             WHERE id = ?4",
            params![used, total, expire, rowid],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(id.to_string()));
        }
        self.get_subscription(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
    }

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

        let removed = tx.execute("DELETE FROM subscriptions WHERE id = ?1", params![rowid])?;
        tx.commit()?;
        Ok(removed > 0)
    }

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

    #[cfg(any(test, feature = "test-support"))]
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

    #[cfg(any(test, feature = "test-support"))]
    pub fn refresh_with_body(
        &mut self,
        sub_id: &str,
        body: &str,
        now: i64,
    ) -> Result<RefreshOutcome, StoreError> {
        self.refresh_with_body_running(sub_id, body, now, &HashSet::new())
    }

    pub fn refresh_with_body_running(
        &mut self,
        sub_id: &str,
        body: &str,
        now: i64,
        running: &HashSet<String>,
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

        let mut desired: BTreeMap<String, (usize, ImportedConfig)> = BTreeMap::new();

        let mut remote_keys: Vec<String> = Vec::new();
        let mut duplicates = 0usize;
        for (index, config) in batch.configs.into_iter().enumerate() {
            let key = widened_identity(&config.protocol, &config.params);
            match desired.entry(key) {
                std::collections::btree_map::Entry::Occupied(_) => {
                    duplicates += 1;
                }
                std::collections::btree_map::Entry::Vacant(slot) => {
                    remote_keys.push(slot.key().clone());
                    slot.insert((index, config));
                }
            }
        }
        let rows = self.list()?;

        let mut tombstones: HashSet<String> = HashSet::new();
        let mut tomb_cores_by_url: HashSet<String> = HashSet::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT identity_key, rotation_core FROM subscription_tombstones
                 WHERE subscription_id = ?1 OR subscription_url = ?2",
            )?;
            let entries = stmt
                .query_map(params![subscription.id, subscription.url.expose()], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
                })?;
            for entry in entries {
                let (key, core) = entry?;
                tombstones.insert(key.clone());

                let core = core.or_else(|| rotation_core_from_identity_key(&key));
                if let Some(core) = core {
                    tomb_cores_by_url.insert(core);
                }
            }
        }

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
                params: Secret,
                key: String,
            },
            GroupFollow {
                id: String,
            },
            Adopt {
                id: String,
                key: String,
                params: Secret,
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

            if let Some(core) = rotation_core(&config.params) {
                if tomb_cores_by_url.contains(&core) {
                    outcome.skipped += 1;
                    continue;
                }
            }
            if let Some(view) = owned_by_key.get(key) {
                matched_owned.insert(view.row.id.clone());

                match synced.get(&view.row.id).and_then(|base| base.clone()) {
                    None => {
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
                            outcome.updated_names.push(view.row.name.clone());
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
                            outcome.updated_names.push(view.row.name.clone());
                        } else {
                            if follow_group {
                                ops.push(Op::GroupFollow {
                                    id: view.row.id.clone(),
                                });
                                outcome.updated += 1;
                                outcome.updated_names.push(view.row.name.clone());
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
                if view.owned {
                    candidates.push((key.clone(), *index, config.clone()));
                } else if adoptable.contains(&view.row.id) {
                    ops.push(Op::Adopt {
                        id: view.row.id.clone(),
                        key: key.clone(),
                        params: config.params.clone(),
                    });
                    outcome.added += 1;
                    outcome.added_names.push(view.row.name.clone());
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
                    ops.push(Op::Adopt {
                        id: view.row.id.clone(),
                        key: key.clone(),
                        params: config.params.clone(),
                    });
                    outcome.added += 1;
                    outcome.added_names.push(view.row.name.clone());
                } else {
                    outcome.duplicates += 1;
                }
                continue;
            }
            candidates.push((key.clone(), *index, config.clone()));
        }

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
            if running.contains(view.row.id.as_str()) {
                outcome.in_use += 1;
                outcome.kept_in_use_names.push(view.row.name.clone());
                continue;
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
                            outcome.conflicts += 1;
                        } else {
                            ops.push(Op::UpstreamUpdate {
                                id: view.row.id.clone(),
                                protocol: candidate.protocol.clone(),
                                params: candidate.params.clone(),
                                key: candidate_key.clone(),
                            });
                            outcome.updated += 1;
                            outcome.updated_names.push(view.row.name.clone());
                        }
                        paired = true;
                        break;
                    }
                }
            }
            if !paired {
                for (candidate_key, _, candidate) in &candidates {
                    if consumed.contains(candidate_key) {
                        continue;
                    }
                    if candidate.name == view.row.name {
                        consumed.insert(candidate_key.clone());
                        if user_changed {
                            outcome.conflicts += 1;
                        } else {
                            ops.push(Op::UpstreamUpdate {
                                id: view.row.id.clone(),
                                protocol: candidate.protocol.clone(),
                                params: candidate.params.clone(),
                                key: candidate_key.clone(),
                            });
                            outcome.updated += 1;
                            outcome.updated_names.push(view.row.name.clone());
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
                outcome.removed_names.push(view.row.name.clone());
            }
        }

        let deleted_set: HashSet<&str> = deleted_ids.iter().map(String::as_str).collect();
        let mut taken: HashSet<i64> = rows
            .iter()
            .filter(|row| !deleted_set.contains(row.id.as_str()))
            .map(|row| row.port as i64)
            .collect();
        let mut next_free_port = || -> Result<u16, &'static str> {
            take_free_auto_port(auto_port_candidates(), &mut taken)
        };
        for (key, index, config) in &candidates {
            if consumed.contains(key) {
                continue;
            }
            match next_free_port() {
                Ok(port) => {
                    ops.push(Op::Insert {
                        key: key.clone(),
                        config: config.clone(),
                        port,
                    });
                    outcome.added += 1;
                    outcome.added_names.push(config.name.clone());
                }
                Err(reason) => {
                    outcome.failures.push(crate::import::ImportFailure {
                        index: *index,
                        label: crate::import::redact_link(&config.name),
                        reason: format!("port-exhausted: {reason}"),
                        code: crate::import::ImportErrorKind::BadPort,
                    });
                }
            }
        }
        let tx = self.conn.transaction()?;
        for op in ops {
            match op {
                Op::Insert { key, config, port } => {
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

        let remote_ids: Vec<String> = {
            let mut key_to_id: HashMap<String, String> = HashMap::new();
            {
                let mut stmt = tx.prepare(
                    "SELECT id, identity_key FROM proxy_configs WHERE subscription_id = ?1",
                )?;
                let entries = stmt.query_map(params![subscription.id], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
                })?;
                for entry in entries {
                    let (id, key) = entry?;
                    if let Some(key) = key {
                        key_to_id.insert(key, id.to_string());
                    }
                }
            }
            remote_keys
                .iter()
                .filter_map(|key| key_to_id.get(key).cloned())
                .collect()
        };
        {
            let mut current: Vec<String> = {
                let mut stmt =
                    tx.prepare("SELECT id FROM proxy_configs ORDER BY sort_order, id")?;
                let ids = stmt.query_map([], |row| row.get::<_, i64>(0))?;
                let mut out = Vec::new();
                for id in ids {
                    out.push(id?.to_string());
                }
                out
            };
            let remote_set: HashSet<&str> = remote_ids.iter().map(String::as_str).collect();
            let anchor = current
                .iter()
                .position(|id| remote_set.contains(id.as_str()))
                .unwrap_or(current.len());
            current.retain(|id| !remote_set.contains(id.as_str()));
            let at = anchor.min(current.len());
            for (offset, id) in remote_ids.iter().enumerate() {
                current.insert(at + offset, id.clone());
            }
            for (position, id) in current.iter().enumerate() {
                let rowid: i64 = id.parse().map_err(|_| {
                    StoreError::Invalid(vec![FieldError::new("id", "bad config id")])
                })?;
                tx.execute(
                    "UPDATE proxy_configs SET sort_order = ?1 WHERE id = ?2",
                    params![position as i64, rowid],
                )?;
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

    pub fn export_library_json(&self) -> Result<String, StoreError> {
        let (payload, _) = self.export_library_json_with_quarantine()?;
        Ok(payload)
    }

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

    fn enrich_export_payload(
        &self,
        base_payload: &str,
        rows: &[ProxyConfig],
    ) -> Result<String, StoreError> {
        type SideRow = (bool, Option<String>, i64, i64, Option<String>);

        let mut side: HashMap<String, SideRow> = HashMap::new();

        let mut unreadable_synced: HashSet<String> = HashSet::new();
        {
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

            let preset_names: HashMap<String, String> = self
                .list_presets()?
                .into_iter()
                .map(|p| (p.id, p.name))
                .collect();
            for entry in entries {
                let (id, adoptable, synced, rx, tx, preset_id) = entry?;

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
        let parsed: serde_json::Value = serde_json::from_str(base_payload)
            .map_err(|error| Self::schema_error(format!("export serialization broke ({error})")))?;
        // T-22: the base payload arrives sealed; enrichment mutates it, so the
        // envelope is stripped here and re-sealed (fresh checksum) at the end.
        let mut value = match parsed {
            serde_json::Value::Object(obj) => envelope_free_payload(&obj),
            other => other,
        };
        if let Some(entries) = value.get_mut("library").and_then(|v| v.as_array_mut()) {
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
                        "locked": p.locked,
                    })
                })
                .collect();
            if let Some(obj) = value.as_object_mut() {
                obj.insert("route_presets".to_string(), serde_json::Value::Array(items));
            }
        }

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

            for group in self.group_skip_flags()? {
                meta.insert(
                    Self::group_skip_key(&group),
                    serde_json::Value::String("1".to_string()),
                );
            }
            if let Some(obj) = value.as_object_mut() {
                obj.insert("meta".to_string(), serde_json::Value::Object(meta));
            }
        }

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
        Ok(seal_library_export(&value))
    }

    pub fn commit_export(
        &self,
        path: &Path,
        confirmed: bool,
    ) -> Result<Option<PathBuf>, StoreError> {
        if !confirmed {
            return Ok(None);
        }

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
        // T-34/arms: the rename must not depend on an external `icacls` call
        // succeeding — hardening the transient temp file first could strip its
        // inherited ACLs and leave it un-renameable (ACCESS_DENIED), which
        // lost the whole export on the windows-arm64 runner. The shipped file
        // is hardened right after it lands in place.
        if let Err(error) = std::fs::rename(&tmp, path).map_err(io_error) {
            let _ = std::fs::remove_file(&tmp);
            return Err(error);
        }
        harden_file_permissions(path);
        Ok(Some(path.to_path_buf()))
    }

    pub fn restore_library_json(&mut self, payload: &str) -> Result<RestoreOutcome, StoreError> {
        fn invalid(message: String) -> StoreError {
            StoreError::Invalid(vec![FieldError::new("export", message)])
        }
        let value: serde_json::Value = serde_json::from_str(payload)
            .map_err(|error| invalid(format!("not JSON ({error})")))?;
        let obj = value
            .as_object()
            .ok_or_else(|| invalid("top level must be an object".to_string()))?;
        // T-22: envelope negotiation sits between parse and the write pipeline
        // (parse → negotiate → dry-run → backup → swap), so every refusal here
        // leaves the library and any backup untouched.
        negotiate_library_envelope(obj)?;
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

        {
            let mut seen: std::collections::HashSet<(String, i64)> =
                std::collections::HashSet::new();
            for (index, row) in rows.iter().enumerate() {
                let key = (crate::engine::normalize_bind_key(&row.bind_ip), row.port);
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
            quota_used: Option<i64>,
            quota_total: Option<i64>,
            quota_expire: Option<i64>,
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

                quota_used: entry.get("quota_used").and_then(serde_json::Value::as_i64),
                quota_total: entry.get("quota_total").and_then(serde_json::Value::as_i64),
                quota_expire: entry
                    .get("quota_expire")
                    .and_then(serde_json::Value::as_i64),
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
            locked: bool,
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
                locked: entry
                    .get("locked")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
            });
        }

        let mut meta_rows: Vec<(String, String)> = Vec::new();
        for key in [
            Self::META_RESTORE_RUNNING,
            Self::META_RUNNING_IDS,
            Self::META_SUBS_AUTO_REFRESH,
        ] {
            if let Some(v) = meta_json.get(key).and_then(|v| v.as_str()) {
                if key == Self::META_RUNNING_IDS {
                    match serde_json::from_str::<serde_json::Value>(v) {
                        Ok(serde_json::Value::Array(_)) => {}
                        _ => return Err(invalid("meta.running_ids is corrupt".to_string())),
                    }
                }

                if key == Self::META_SUBS_AUTO_REFRESH && v != "0" && v != "1" {
                    return Err(invalid("meta.subs_auto_refresh is corrupt".to_string()));
                }
                meta_rows.push((key.to_string(), v.to_string()));
            }
        }

        for (key, value) in &meta_json {
            if let (Some(group), Some(flag)) =
                (key.strip_prefix("group.skip_refresh."), value.as_str())
            {
                if flag == "1" && !group.trim().is_empty() {
                    meta_rows.push((key.clone(), "1".to_string()));
                }
            }
        }

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

        let mut sealed_sub_urls: Vec<String> = Vec::with_capacity(sub_rows.len());
        for sub in &sub_rows {
            if sub.url.trim_start().starts_with(crate::secret::MARKER) {
                Self::unseal_or_fail(&sub.url).map_err(|error| {
                    let what = if sub.label.trim().is_empty() {
                        format!("subscription {}", sub.id)
                    } else {
                        format!("subscription \"{}\"", sub.label)
                    };
                    invalid(format!("{what} has an unsealable url ({error})"))
                })?;
                sealed_sub_urls.push(sub.url.clone());
            } else {
                sealed_sub_urls.push(Self::seal_or_fail(&sub.url)?);
            }
        }
        let tx = self.conn.transaction()?;

        tx.execute("DELETE FROM proxy_configs", [])?;
        tx.execute("DELETE FROM subscriptions", [])?;
        tx.execute("DELETE FROM subscription_tombstones", [])?;

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
                    description, is_builtin, last_used_at, locked)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
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
                    preset.locked as i64,
                ],
            )?;
        }

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
                   (id, label, url, grp, auto_refresh, last_refresh_at, last_error, failures, interval_secs,
                    quota_used, quota_total, quota_expire)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
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
                    sub.quota_used,
                    sub.quota_total,
                    sub.quota_expire,
                ],
            )?;
        }

        for (position, (row, sealed_params, sealed_synced)) in sealed_rows.iter().enumerate() {
            let preset_id: Option<String> = row
                .preset_name
                .as_deref()
                .and_then(|name| preset_by_name.get(name).cloned());
            tx.execute(
                "INSERT INTO proxy_configs
                   (id, name, protocol, params, grp, enabled, bind_ip, port,
                    subscription_id, identity_key, synced_params, adoptable,
                    preset_id, traffic_rx, traffic_tx, sort_order)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
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
                    position as i64,
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

    #[cfg(any(test, feature = "test-support"))]
    pub fn add_traffic(&mut self, id: &str, rx: u64, tx: u64) -> Result<(), StoreError> {
        let updated = self.add_traffic_batch(&[(id.to_string(), rx, tx)])?;
        if updated == 0 {
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

    pub fn add_traffic_batch(
        &mut self,
        deltas: &[(String, u64, u64)],
    ) -> Result<usize, StoreError> {
        if deltas.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.transaction()?;

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

            let rx = (*rx).min(i64::MAX as u64) as i64;
            let txb = (*tx_bytes).min(i64::MAX as u64) as i64;
            match stmt.execute(params![rx, txb, rowid]) {
                Ok(1..) => updated += 1,
                Ok(_) => {}
                Err(error) => return Err(error.into()),
            }
        }
        drop(stmt);
        tx.commit()?;
        Ok(updated)
    }

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

    pub fn reset_traffic(&mut self, id: &str) -> Result<(), StoreError> {
        self.reset_traffic_counters(id)
    }

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

    pub fn clear_all_traffic(&mut self) -> Result<usize, StoreError> {
        let changed = self.conn.execute(
            "UPDATE proxy_configs SET traffic_rx = 0, traffic_tx = 0",
            [],
        )?;
        Ok(changed)
    }

    pub fn clear_health_snapshots(&mut self) -> Result<usize, StoreError> {
        let changed = self.conn.execute("DELETE FROM health_snapshots", [])?;
        Ok(changed)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn save_health_snapshot(&mut self, snapshot: &HealthSnapshot) -> Result<(), StoreError> {
        self.save_health_snapshots(std::slice::from_ref(snapshot))
    }

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

    fn check_health_sample(
        kind: &str,
        latency_ms: Option<u64>,
        error: Option<&str>,
    ) -> Result<(), String> {
        if kind != "url-test" && kind != "tcp-ping" && kind != "speed-test" {
            return Err(format!(
                "bad health kind '{kind}' (want url-test|tcp-ping|speed-test)"
            ));
        }
        match (latency_ms, error) {
            (Some(_), None) | (None, Some(_)) => Ok(()),
            _ => Err("health sample needs latency_ms xor error".to_string()),
        }
    }

    #[cfg(any(test, feature = "test-support"))]
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

    pub fn prune_health_history_before(&mut self, cutoff: i64) -> Result<usize, StoreError> {
        let tx = self.conn.transaction()?;
        let pruned = tx.execute("DELETE FROM health_history WHERE at < ?1", params![cutoff])?;
        tx.commit()?;
        Ok(pruned)
    }

    pub fn gc_health_history(&mut self, retain_days: i64, now: i64) -> Result<usize, StoreError> {
        let days = retain_days.max(1);
        let cutoff = now.saturating_sub(days.saturating_mul(86_400));
        self.prune_health_history_before(cutoff)
    }

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

    pub const META_RESTORE_RUNNING: &str = "restore_running";

    pub const META_RUNNING_IDS: &str = "running_ids";

    /// T-34: `"1"` only when the previous session reached an orderly exit.
    /// Boot writes `"0"` before anything else, so a kill/crash leaves `"0"`.
    pub const META_LAST_CLEAN_EXIT: &str = "last_clean_exit";

    pub const META_TEST_URL: &str = "test_url";

    pub fn get_meta(&self, key: &str) -> Result<Option<String>, StoreError> {
        let mut stmt = self.conn.prepare("SELECT value FROM meta WHERE key = ?1")?;
        let mut rows = stmt.query_map(params![key], |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn set_meta(&mut self, key: &str, value: &str) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn restore_running(&self) -> bool {
        self.get_meta(Self::META_RESTORE_RUNNING)
            .map(|value| value.as_deref() == Some("1"))
            .unwrap_or(false)
    }

    pub fn set_restore_running(&mut self, on: bool) -> Result<(), StoreError> {
        self.set_meta(Self::META_RESTORE_RUNNING, if on { "1" } else { "0" })
    }

    pub const META_SUBS_AUTO_REFRESH: &str = "subs_auto_refresh";

    pub fn subs_auto_refresh_enabled(&self) -> bool {
        self.get_meta(Self::META_SUBS_AUTO_REFRESH)
            .map(|value| value.as_deref() != Some("0"))
            .unwrap_or(true)
    }

    pub fn set_subs_auto_refresh_enabled(&mut self, on: bool) -> Result<(), StoreError> {
        self.set_meta(Self::META_SUBS_AUTO_REFRESH, if on { "1" } else { "0" })
    }

    fn group_skip_key(group: &str) -> String {
        format!("group.skip_refresh.{}", group.trim())
    }

    pub fn group_skip_refresh(&self, group: &str) -> bool {
        self.get_meta(&Self::group_skip_key(group))
            .map(|value| value.as_deref() == Some("1"))
            .unwrap_or(false)
    }

    pub fn group_skip_flags(&self) -> Result<Vec<String>, StoreError> {
        let prefix = "group.skip_refresh.";
        let mut stmt = self.conn.prepare(
            "SELECT key FROM meta WHERE key LIKE 'group.skip_refresh.%' AND value = '1'
             ORDER BY key",
        )?;
        let keys = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for key in keys {
            let key = key?;
            if let Some(group) = key.strip_prefix(prefix) {
                out.push(group.to_string());
            }
        }
        Ok(out)
    }

    pub fn set_group_skip_refresh(&mut self, group: &str, skip: bool) -> Result<(), StoreError> {
        let key = Self::group_skip_key(group);
        if skip {
            self.set_meta(&key, "1")
        } else {
            self.conn
                .execute("DELETE FROM meta WHERE key = ?1", params![key])?;
            Ok(())
        }
    }

    pub const META_AUTO_CLEAR_GROUPS: &str = "auto_clear_groups";

    /// T-19: normalized group name → opt-in config; absent or malformed meta
    /// reads as off everywhere.
    fn auto_clear_groups_map(&self) -> BTreeMap<String, AutoClearGroupConfig> {
        match self.get_meta(Self::META_AUTO_CLEAR_GROUPS) {
            Ok(Some(raw)) => serde_json::from_str::<serde_json::Value>(&raw)
                .ok()
                .and_then(|value| value.as_object().cloned())
                .map(|entries| {
                    entries
                        .into_iter()
                        .filter_map(|(group, entry)| {
                            Some((
                                group,
                                AutoClearGroupConfig {
                                    enabled: entry.get("enabled")?.as_bool()?,
                                    on_date: entry.get("on_date")?.as_i64()?,
                                    note_seen: entry.get("note_seen")?.as_bool()?,
                                },
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            _ => BTreeMap::new(),
        }
    }

    fn auto_clear_groups_write(
        &mut self,
        map: &BTreeMap<String, AutoClearGroupConfig>,
    ) -> Result<(), StoreError> {
        let entries: serde_json::Map<String, serde_json::Value> = map
            .iter()
            .map(|(group, config)| {
                (
                    group.clone(),
                    serde_json::json!({
                        "enabled": config.enabled,
                        "on_date": config.on_date,
                        "note_seen": config.note_seen,
                    }),
                )
            })
            .collect();
        self.set_meta(
            Self::META_AUTO_CLEAR_GROUPS,
            &serde_json::Value::Object(entries).to_string(),
        )
    }

    pub fn auto_clear_group(&self, group: &str) -> Option<AutoClearGroupConfig> {
        self.auto_clear_groups_map()
            .get(&normalized_group(group))
            .cloned()
    }

    pub fn auto_clear_enabled(&self, group: &str) -> bool {
        self.auto_clear_groups_map()
            .get(&normalized_group(group))
            .is_some_and(|config| config.enabled)
    }

    pub fn auto_clear_enabled_groups(&self) -> Vec<String> {
        self.auto_clear_groups_map()
            .into_iter()
            .filter(|(_, config)| config.enabled)
            .map(|(group, _)| group)
            .collect()
    }

    /// Enabling without `note_seen` is rejected here — skip-delete-confirm
    /// cannot act as a bypass for auto-clear's own acknowledgment.
    pub fn set_auto_clear_group(
        &mut self,
        group: &str,
        config: AutoClearGroupConfig,
    ) -> Result<(), StoreError> {
        let group = normalized_group(group);
        if group.is_empty() {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "group",
                "Group name cannot be empty",
            )]));
        }
        if config.enabled && !config.note_seen {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "auto_clear",
                "auto-clear opt-in note not confirmed",
            )]));
        }
        let mut map = self.auto_clear_groups_map();
        map.insert(group, config);
        self.auto_clear_groups_write(&map)
    }

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

    pub fn clear_running_snapshot(&mut self) -> Result<(), StoreError> {
        self.conn.execute(
            "DELETE FROM meta WHERE key = ?1",
            params![Self::META_RUNNING_IDS],
        )?;
        Ok(())
    }

    /// T-34: did the previous session quit through an orderly path? An absent
    /// key (fresh install, legacy store) or any value but `"1"` is unclean.
    pub fn last_clean_exit(&self) -> bool {
        self.get_meta(Self::META_LAST_CLEAN_EXIT)
            .map(|value| value.as_deref() == Some("1"))
            .unwrap_or(false)
    }

    pub fn set_last_clean_exit(&mut self, clean: bool) -> Result<(), StoreError> {
        self.set_meta(Self::META_LAST_CLEAN_EXIT, if clean { "1" } else { "0" })
    }

    pub fn test_url(&self) -> Option<String> {
        match self.get_meta(Self::META_TEST_URL) {
            Ok(Some(url)) if !url.trim().is_empty() => Some(url),
            _ => None,
        }
    }

    pub fn set_test_url(&mut self, url: &str) -> Result<(), StoreError> {
        self.set_meta(Self::META_TEST_URL, url.trim())
    }

    pub fn clear_test_url(&mut self) -> Result<(), StoreError> {
        self.conn.execute(
            "DELETE FROM meta WHERE key = ?1",
            params![Self::META_TEST_URL],
        )?;
        Ok(())
    }

    fn row_test_url_key(id: &str) -> String {
        format!("{}:{id}", Self::META_TEST_URL)
    }

    pub fn row_test_url(&self, id: &str) -> Option<String> {
        match self.get_meta(&Self::row_test_url_key(id)) {
            Ok(Some(url)) if !url.trim().is_empty() => Some(url),
            _ => None,
        }
    }

    pub fn set_row_test_url(&mut self, id: &str, url: &str) -> Result<(), StoreError> {
        self.set_meta(&Self::row_test_url_key(id), url.trim())
    }

    pub fn clear_row_test_url(&mut self, id: &str) -> Result<(), StoreError> {
        self.conn.execute(
            "DELETE FROM meta WHERE key = ?1",
            params![Self::row_test_url_key(id)],
        )?;
        Ok(())
    }

    pub const META_UPDATE_FEED_URL: &str = "update_feed_url";

    pub fn update_feed_url(&self) -> Option<String> {
        match self.get_meta(Self::META_UPDATE_FEED_URL) {
            Ok(Some(url)) if !url.trim().is_empty() => Some(url),
            _ => None,
        }
    }

    pub fn set_update_feed_url(&mut self, url: &str) -> Result<(), StoreError> {
        self.set_meta(Self::META_UPDATE_FEED_URL, url.trim())
    }

    pub fn clear_update_feed_url(&mut self) -> Result<(), StoreError> {
        self.conn.execute(
            "DELETE FROM meta WHERE key = ?1",
            params![Self::META_UPDATE_FEED_URL],
        )?;
        Ok(())
    }

    pub const META_APPEARANCE: &str = "appearance";

    pub fn appearance(&self) -> String {
        match self.get_meta(Self::META_APPEARANCE) {
            Ok(Some(value)) if value == "dark" || value == "light" || value == "system" => value,
            _ => "dark".to_string(),
        }
    }

    pub fn set_appearance(&mut self, value: &str) -> Result<(), StoreError> {
        if value != "dark" && value != "light" && value != "system" {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "appearance",
                "appearance must be dark, light, or system",
            )]));
        }
        self.set_meta(Self::META_APPEARANCE, value)
    }

    // K17: the old META_SKIP_DELETE_CONFIRM meta and its accessors are gone.
    // The confirm gate they armed no longer exists (spec line 91: delete is
    // immediate, the undo toast is the net), so keeping them would leave a
    // stored value that controls nothing; a legacy library's skip_delete_confirm
    // meta row is simply ignored.

    pub const META_CLOSE_ACTION: &str = "close_action";

    /// T-04: what the window's close button does. `Tray` hides the window and
    /// leaves the proxies running; `Quit` stops every profile and exits. A
    /// missing or corrupt value reads as `Tray` — the long-standing behaviour.
    pub fn close_action(&self) -> CloseAction {
        match self.get_meta(Self::META_CLOSE_ACTION) {
            Ok(Some(raw)) => CloseAction::from_stored(&raw).unwrap_or_default(),
            _ => CloseAction::default(),
        }
    }

    pub fn set_close_action(&mut self, action: CloseAction) -> Result<(), StoreError> {
        self.set_meta(Self::META_CLOSE_ACTION, action.stored())
    }

    pub const META_SEEN_TRAY_HINT: &str = "seen_tray_hint";

    /// T-04: the one-time "still running in the tray" hint. Absent or corrupt
    /// reads as not-seen, so the hint shows exactly once per library.
    pub fn seen_tray_hint(&self) -> bool {
        self.get_meta(Self::META_SEEN_TRAY_HINT)
            .map(|value| value.as_deref() == Some("1"))
            .unwrap_or(false)
    }

    pub fn set_seen_tray_hint(&mut self, seen: bool) -> Result<(), StoreError> {
        self.set_meta(Self::META_SEEN_TRAY_HINT, if seen { "1" } else { "0" })
    }

    pub const META_PROBE_TIMEOUT_SECS: &str = "probe_timeout_secs";

    /// T-04: how long one URL probe may take. A missing or out-of-range value
    /// reads as the default, so a corrupt meta row never stalls a probe.
    pub fn probe_timeout_secs(&self) -> u64 {
        match self.get_meta(Self::META_PROBE_TIMEOUT_SECS) {
            Ok(Some(raw)) => raw
                .trim()
                .parse::<u64>()
                .ok()
                .filter(|secs| (MIN_PROBE_TIMEOUT_SECS..=MAX_PROBE_TIMEOUT_SECS).contains(secs))
                .unwrap_or(DEFAULT_PROBE_TIMEOUT_SECS),
            _ => DEFAULT_PROBE_TIMEOUT_SECS,
        }
    }

    pub fn set_probe_timeout_secs(&mut self, secs: u64) -> Result<(), StoreError> {
        if !(MIN_PROBE_TIMEOUT_SECS..=MAX_PROBE_TIMEOUT_SECS).contains(&secs) {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "probe_timeout_secs",
                format!(
                    "probe timeout must be {MIN_PROBE_TIMEOUT_SECS}-{MAX_PROBE_TIMEOUT_SECS} seconds"
                ),
            )]));
        }
        self.set_meta(Self::META_PROBE_TIMEOUT_SECS, &secs.to_string())
    }

    pub const META_TEST_ALL_SCOPE: &str = "test_all_scope";

    /// T-04: which rows a "Test all" run covers. A missing or corrupt value
    /// reads as `Running` — the behaviour the app shipped with.
    pub fn test_all_scope(&self) -> TestAllScope {
        match self.get_meta(Self::META_TEST_ALL_SCOPE) {
            Ok(Some(raw)) => TestAllScope::from_stored(&raw).unwrap_or_default(),
            _ => TestAllScope::default(),
        }
    }

    pub fn set_test_all_scope(&mut self, scope: TestAllScope) -> Result<(), StoreError> {
        self.set_meta(Self::META_TEST_ALL_SCOPE, scope.stored())
    }

    pub const META_LAST_UPDATE_CHECK: &str = "last_update_check";

    /// T-04: when the update feed was last checked, as a Unix timestamp.
    pub fn last_update_check(&self) -> Option<i64> {
        match self.get_meta(Self::META_LAST_UPDATE_CHECK) {
            Ok(Some(raw)) => raw.trim().parse::<i64>().ok(),
            _ => None,
        }
    }

    pub fn set_last_update_check(&mut self, at: i64) -> Result<(), StoreError> {
        self.set_meta(Self::META_LAST_UPDATE_CHECK, &at.to_string())
    }

    pub const META_LIBRARY_COLUMNS: &str = "library_columns";

    pub fn library_columns_raw(&self) -> Option<String> {
        self.get_meta(Self::META_LIBRARY_COLUMNS).ok().flatten()
    }

    pub fn set_library_columns(&mut self, value: &str) -> Result<(), StoreError> {
        if value.trim().is_empty() {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "library_columns",
                "library_columns must list visible columns",
            )]));
        }
        self.set_meta(Self::META_LIBRARY_COLUMNS, value.trim())
    }

    /// T-27: the persisted library sort, additive beside `library_columns`. A
    /// missing key (or a corrupt payload) reads as `none`/ascending — the
    /// pre-feature order.
    pub const META_LIBRARY_SORT: &str = "library_sort";

    pub fn library_sort(&self) -> LibrarySort {
        match self.get_meta(Self::META_LIBRARY_SORT) {
            Ok(Some(raw)) => LibrarySort::from_json(&raw),
            _ => LibrarySort::default(),
        }
    }

    pub fn set_library_sort(&mut self, sort: LibrarySort) -> Result<(), StoreError> {
        self.set_meta(Self::META_LIBRARY_SORT, &sort.to_json())
    }

    pub const META_LIBRARY_LAST_QUERY: &str = "library_last_query";

    /// T-23: the library search query string of the previous session. A missing
    /// or whitespace-only value reads as "no remembered filter".
    pub fn library_last_query(&self) -> Option<String> {
        match self.get_meta(Self::META_LIBRARY_LAST_QUERY) {
            Ok(Some(value)) if !value.trim().is_empty() => Some(value),
            _ => None,
        }
    }

    /// T-23: remembering an empty query clears the key instead of storing a
    /// blank one, so an untouched search field never writes a meta row.
    pub fn set_library_last_query(&mut self, value: &str) -> Result<(), StoreError> {
        if value.trim().is_empty() {
            return self.clear_library_last_query();
        }
        self.set_meta(Self::META_LIBRARY_LAST_QUERY, value.trim())
    }

    pub fn clear_library_last_query(&mut self) -> Result<(), StoreError> {
        self.conn.execute(
            "DELETE FROM meta WHERE key = ?1",
            params![Self::META_LIBRARY_LAST_QUERY],
        )?;
        Ok(())
    }

    /// T-31: the kept-log ring cap, additive beside the other meta keys.
    pub const META_LOG_MAX_LINES: &str = "log_max_lines";

    /// T-31: absent, unparsable, or out-of-window values read as the 5,000-line
    /// default — the pre-feature shape — and in-window values clamp to
    /// `500..=50_000`.
    pub fn log_max_lines(&self) -> usize {
        match self.get_meta(Self::META_LOG_MAX_LINES) {
            Ok(Some(raw)) => raw
                .trim()
                .parse::<usize>()
                .map(crate::health::clamp_log_max_lines)
                .unwrap_or(crate::health::MAX_LOG_RECORDS),
            _ => crate::health::MAX_LOG_RECORDS,
        }
    }

    pub fn set_log_max_lines(&mut self, cap: usize) -> Result<(), StoreError> {
        self.set_meta(
            Self::META_LOG_MAX_LINES,
            &crate::health::clamp_log_max_lines(cap).to_string(),
        )
    }

    /// T-32: the last clean-exit window rectangle, additive beside the other
    /// meta keys. Absent or corrupt reads as `None` — the pre-feature default.
    pub const META_GEOMETRY: &str = "geometry";

    pub fn window_geometry(&self) -> Option<WindowGeometry> {
        match self.get_meta(Self::META_GEOMETRY) {
            Ok(Some(raw)) => WindowGeometry::from_json(&raw),
            _ => None,
        }
    }

    /// T-32: stores the clamped rectangle; a geometry that cannot be made sane
    /// writes nothing, so the next launch keeps the previous or default shape.
    pub fn set_window_geometry(&mut self, geometry: WindowGeometry) -> Result<(), StoreError> {
        match geometry.clamped() {
            Some(sane) => self.set_meta(Self::META_GEOMETRY, &sane.to_json()),
            None => Ok(()),
        }
    }

    /// T-33: the bind-address menu the per-row picker draws from — a JSON array
    /// of user-added host literals. The constants are implicit: they are never
    /// written here, never removable, and always lead the accessor's result.
    pub const META_BIND_HOSTS: &str = "bind_hosts";

    /// T-33: the two addresses every list carries, in this order, ahead of the
    /// user extras. Removing them is impossible by construction.
    pub const BIND_HOST_CONSTANTS: [&str; 2] = [DEFAULT_BIND, "0.0.0.0"];

    /// T-33: the one reason the list editor refuses a typed value — also the
    /// inline text the Settings field shows for it.
    pub const BIND_HOST_LITERAL_ONLY: &str = "addresses only — no hostnames";

    /// T-33: constants first, then the stored extras in insertion order.
    ///
    /// Seeding is one-time by construction: only a *missing* meta key seeds, and
    /// the seed unions the two constants with every row's current `bind_ip`
    /// before writing the extras. Once the key exists, later rows never widen
    /// the list — additions go through [`Store::add_bind_host`] alone.
    pub fn bind_hosts(&mut self) -> Vec<String> {
        let mut out: Vec<String> = Self::BIND_HOST_CONSTANTS
            .iter()
            .map(|host| host.to_string())
            .collect();
        out.extend(self.bind_host_extras());
        out
    }

    /// T-33: extras only, seeding on a missing key (see [`Store::bind_hosts`]).
    fn bind_host_extras(&mut self) -> Vec<String> {
        match self.get_meta(Self::META_BIND_HOSTS) {
            Ok(Some(raw)) => sanitized_bind_host_extras(decode_bind_host_extras(&raw)),
            Ok(None) => {
                let seeded = sanitized_bind_host_extras(self.current_row_binds());
                let _ = self.write_bind_host_extras(&seeded);
                seeded
            }
            Err(_) => Vec::new(),
        }
    }

    fn current_row_binds(&self) -> Vec<String> {
        match self.list() {
            Ok(rows) => rows.into_iter().map(|row| row.bind_ip).collect(),
            Err(_) => Vec::new(),
        }
    }

    fn write_bind_host_extras(&mut self, extras: &[String]) -> Result<(), StoreError> {
        let value = serde_json::Value::Array(
            extras
                .iter()
                .map(|host| serde_json::Value::from(host.clone()))
                .collect(),
        )
        .to_string();
        self.set_meta(Self::META_BIND_HOSTS, &value)
    }

    /// T-33: which binds the library currently uses, as bind → row names. The
    /// Settings editor consults this before removing an extra.
    ///
    /// Literal binds are canonicalized so `FD00:0::7` and `fd00::7` count as the
    /// same use; anything the store lets through but the list never holds
    /// (`localhost`) keeps its raw text.
    pub fn bind_host_usage(&self) -> Result<BTreeMap<String, Vec<String>>, StoreError> {
        let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for row in self.list()? {
            let bind = row.bind_ip.trim();
            if bind.is_empty() {
                continue;
            }
            let key = canonical_bind_host(bind).unwrap_or_else(|| bind.to_string());
            out.entry(key).or_default().push(row.name);
        }
        for names in out.values_mut() {
            names.sort();
        }
        Ok(out)
    }

    /// T-33: appends one validated literal to the extras and returns the whole
    /// list. Hostnames, malformed literals, and duplicates are refused.
    pub fn add_bind_host(&mut self, raw: &str) -> Result<Vec<String>, StoreError> {
        let host = normalize_bind_host(raw)?;
        let mut extras = self.bind_host_extras();
        if Self::BIND_HOST_CONSTANTS.contains(&host.as_str()) || extras.contains(&host) {
            return Err(bind_host_error(format!("{host} is already in the list")));
        }
        extras.push(host);
        self.write_bind_host_extras(&extras)?;
        Ok(self.bind_hosts())
    }

    /// T-33: removes one extra. The constants and any bind a row still uses are
    /// refused; the in-use refusal names the blocking row(s).
    pub fn remove_bind_host(&mut self, host: &str) -> Result<Vec<String>, StoreError> {
        let host = host.trim();
        if Self::BIND_HOST_CONSTANTS
            .iter()
            .any(|constant| constant.eq_ignore_ascii_case(host))
        {
            return Err(bind_host_error(format!(
                "{host} is built in and cannot be removed"
            )));
        }
        let host = canonical_bind_host(host).unwrap_or_else(|| host.to_string());
        if let Some(names) = self.bind_host_usage()?.get(&host) {
            return Err(bind_host_error(bind_host_in_use_message(&host, names)));
        }
        let mut extras = self.bind_host_extras();
        extras.retain(|entry| entry != &host);
        self.write_bind_host_extras(&extras)?;
        Ok(self.bind_hosts())
    }

    pub const CONN_COLUMNS: [(&str, u32); 5] = [
        ("destination", 220),
        ("exit", 180),
        ("protocol", 90),
        ("traffic", 140),
        ("session", 90),
    ];

    pub const META_CONN_COL_WIDTHS: &str = "conn_col_widths";

    pub fn conn_column_widths(&self) -> [u32; 5] {
        let defaults = Self::default_conn_column_widths();
        let raw = match self.get_meta(Self::META_CONN_COL_WIDTHS) {
            Ok(Some(raw)) => raw,
            _ => return defaults,
        };
        let parsed: Vec<u32> = match serde_json::from_str(&raw) {
            Ok(serde_json::Value::Array(items)) => items
                .iter()
                .filter_map(|item| item.as_u64().map(|v| v as u32))
                .collect(),
            _ => return defaults,
        };
        if parsed.len() != defaults.len() {
            return defaults;
        }
        let mut out = defaults;
        for (slot, value) in out.iter_mut().zip(parsed) {
            *slot = value.clamp(40, 600);
        }
        out
    }

    pub fn default_conn_column_widths() -> [u32; 5] {
        [
            Self::CONN_COLUMNS[0].1,
            Self::CONN_COLUMNS[1].1,
            Self::CONN_COLUMNS[2].1,
            Self::CONN_COLUMNS[3].1,
            Self::CONN_COLUMNS[4].1,
        ]
    }

    pub fn set_conn_column_width(&mut self, index: usize, width_px: u32) -> Result<(), StoreError> {
        if index >= Self::CONN_COLUMNS.len() {
            return Err(StoreError::Invalid(vec![FieldError::new(
                "column",
                format!(
                    "column {index} does not exist (table has {} columns)",
                    Self::CONN_COLUMNS.len()
                ),
            )]));
        }
        let mut widths = self.conn_column_widths();
        widths[index] = width_px.clamp(40, 600);
        let value = serde_json::Value::Array(
            widths
                .iter()
                .map(|width| serde_json::Value::from(*width))
                .collect(),
        )
        .to_string();
        self.set_meta(Self::META_CONN_COL_WIDTHS, &value)
    }

    pub fn reset_conn_column_widths(&mut self) -> Result<(), StoreError> {
        self.conn.execute(
            "DELETE FROM meta WHERE key = ?1",
            params![Self::META_CONN_COL_WIDTHS],
        )?;
        Ok(())
    }

    pub fn traffic_period_summary(
        totals: &std::collections::HashMap<String, (u64, u64)>,
        _period: TrafficPeriod,
    ) -> (usize, u64, u64) {
        let mut exits = 0usize;
        let mut rx = 0u64;
        let mut tx = 0u64;
        for (row_rx, row_tx) in totals.values() {
            if *row_rx > 0 || *row_tx > 0 {
                exits += 1;
            }
            rx = rx.saturating_add(*row_rx);
            tx = tx.saturating_add(*row_tx);
        }
        (exits, rx, tx)
    }

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

    fn auto_port_excluding(&self, exclude: Option<u16>) -> Result<u16, StoreError> {
        let mut stmt = self.conn.prepare("SELECT port FROM proxy_configs")?;
        let mut taken: HashSet<i64> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<HashSet<_>, _>>()?;
        if let Some(own) = exclude {
            taken.remove(&(own as i64));
        }
        take_free_auto_port(auto_port_candidates(), &mut taken)
            .map_err(|reason| StoreError::Invalid(vec![FieldError::new("port", reason)]))
    }

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
        let (protocol, params) = proto_params.unwrap_or((
            current.protocol.clone(),
            Secret::new(current.params.expose()),
        ));
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
            params: Secret::default(),
            group: String::new(),
            bind_ip: String::new(),
            port: String::new(),
        }
    }

    /// T-22: byte-identity assertions compare the payload, because the
    /// envelope's `created` stamp is wall-clock at export time.
    fn payload_without_envelope(json: &str) -> String {
        let value: serde_json::Value = serde_json::from_str(json).expect("export parses");
        envelope_free_payload(value.as_object().expect("export is an object")).to_string()
    }

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
        let renamed = store.rename_group("g1", "team").expect("rename works");
        assert_eq!(renamed.rows, 2);
        assert_eq!(renamed.subscriptions, 0);
        let groups: std::collections::HashMap<String, usize> = store
            .group_counts()
            .expect("counts work")
            .into_iter()
            .collect();
        assert_eq!(groups.get("team"), Some(&2));
        assert_eq!(groups.get("g2"), Some(&1));
        assert!(!groups.contains_key("g1"));
        let missing = store
            .rename_group("missing", "x")
            .expect("missing renames zero");
        assert_eq!(missing, GroupRenameOutcome::default());
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

    #[test]
    fn t08_group_skip_flag_round_trips_and_follows_rename() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        assert!(!store.group_skip_refresh("travel"), "unset means refresh");
        assert!(store.group_skip_flags().expect("flags read").is_empty());
        store
            .set_group_skip_refresh("travel", true)
            .expect("flag sets");
        assert!(store.group_skip_refresh("travel"), "flag reads back");
        assert_eq!(
            store.group_skip_flags().expect("flags read"),
            vec!["travel".to_string()]
        );
        store
            .set_group_skip_refresh("travel", false)
            .expect("flag clears");
        assert!(!store.group_skip_refresh("travel"), "clear deletes the key");
        assert!(store.group_skip_flags().expect("flags read").is_empty());

        store
            .set_group_skip_refresh("travel", true)
            .expect("flag sets");
        store.rename_group("travel", "team").expect("rename works");
        assert!(!store.group_skip_refresh("travel"), "old label unflagged");
        assert!(store.group_skip_refresh("team"), "flag follows rename");
        let moved = store
            .get_subscription(&sub.id)
            .expect("get works")
            .expect("sub kept");
        assert_eq!(moved.group, "team", "subscription follows rename");

        let payload = store.export_library_json().expect("export works");
        let mut other = Store::open_in_memory().expect("in-memory store opens");
        other.restore_library_json(&payload).expect("restore works");
        assert!(
            other.group_skip_refresh("team"),
            "skip flag survives export/restore"
        );
    }

    #[test]
    fn t69_auto_clear_groups_meta_round_trip() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        assert!(!store.auto_clear_enabled("travel"), "absent meta is off");
        assert!(store.auto_clear_enabled_groups().is_empty());
        assert_eq!(store.auto_clear_group("travel"), None);

        // Enabling without the note ACK never reaches the meta map.
        let error = store
            .set_auto_clear_group(
                "travel",
                AutoClearGroupConfig {
                    enabled: true,
                    on_date: 111,
                    note_seen: false,
                },
            )
            .expect_err("note_seen=false cannot enable");
        assert!(
            error.to_string().contains("opt-in note"),
            "guard names the note, got {error}"
        );
        assert_eq!(store.auto_clear_group("travel"), None);

        // The confirmed opt-in persists; lookups trim the key like every
        // other group-level op.
        store
            .set_auto_clear_group(
                "  travel  ",
                AutoClearGroupConfig {
                    enabled: true,
                    on_date: 222,
                    note_seen: true,
                },
            )
            .expect("confirmed opt-in writes");
        assert!(store.auto_clear_enabled("travel"));
        assert_eq!(
            store.auto_clear_enabled_groups(),
            vec!["travel".to_string()]
        );

        // Toggling off keeps the entry (and its audit fields) so the rename
        // migration carries extinguished flags too.
        store
            .set_auto_clear_group(
                "travel",
                AutoClearGroupConfig {
                    enabled: false,
                    on_date: 222,
                    note_seen: true,
                },
            )
            .expect("off writes");
        assert!(!store.auto_clear_enabled("travel"));
        assert!(store.auto_clear_group("travel").is_some());

        store
            .set_auto_clear_group(
                "travel",
                AutoClearGroupConfig {
                    enabled: true,
                    on_date: 333,
                    note_seen: true,
                },
            )
            .expect("re-enable writes");
        let empty = store
            .set_auto_clear_group(
                " ",
                AutoClearGroupConfig {
                    enabled: true,
                    on_date: 1,
                    note_seen: true,
                },
            )
            .expect_err("blank group name rejected");
        let StoreError::Invalid(fields) = empty else {
            panic!("expected Invalid for blank group")
        };
        assert!(fields.iter().any(|e| e.field == "group"));

        // Malformed or incomplete JSON means off, never enabled.
        store
            .set_meta(Store::META_AUTO_CLEAR_GROUPS, "not-json")
            .expect("meta overwrite works");
        assert!(!store.auto_clear_enabled("travel"));
        assert!(store.auto_clear_enabled_groups().is_empty());
        assert_eq!(store.auto_clear_group("travel"), None);
        store
            .set_meta(
                Store::META_AUTO_CLEAR_GROUPS,
                r#"{"travel": {"enabled": true, "on_date": 5}}"#,
            )
            .expect("meta overwrite works");
        assert!(
            !store.auto_clear_enabled("travel"),
            "entries missing note_seen never enable"
        );
    }

    #[test]
    fn t69_group_rename_carries_flag() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store
            .set_auto_clear_group(
                "alpha",
                AutoClearGroupConfig {
                    enabled: true,
                    on_date: 10,
                    note_seen: true,
                },
            )
            .expect("enable alpha");
        store.rename_group("alpha", "beta").expect("rename works");
        assert!(!store.auto_clear_enabled("alpha"), "old label unflagged");
        assert!(store.auto_clear_enabled("beta"), "flag follows rename");
        let config = store.auto_clear_group("beta").expect("entry moved");
        assert_eq!(config.on_date, 10, "opt-in date rides along");
        assert!(config.note_seen, "note ACK rides along");

        // Extinguished flags migrate too, so no stale key lingers and the
        // opt-in history survives the rename.
        store
            .set_auto_clear_group(
                "beta",
                AutoClearGroupConfig {
                    enabled: false,
                    on_date: 10,
                    note_seen: true,
                },
            )
            .expect("disable keeps entry");
        store.rename_group("beta", "gamma").expect("rename works");
        assert!(
            store.auto_clear_group("gamma").is_some(),
            "off entry carries too"
        );
        assert!(!store.auto_clear_enabled("gamma"));
        assert_eq!(store.auto_clear_group("beta"), None);

        // When both names already carry entries the target's own wins and the
        // stale source key is dropped outright.
        store
            .set_auto_clear_group(
                "delta",
                AutoClearGroupConfig {
                    enabled: false,
                    on_date: 7,
                    note_seen: true,
                },
            )
            .expect("delta off entry");
        store
            .set_auto_clear_group(
                "epsilon",
                AutoClearGroupConfig {
                    enabled: true,
                    on_date: 9,
                    note_seen: true,
                },
            )
            .expect("epsilon on entry");
        store
            .rename_group("delta", "epsilon")
            .expect("merge rename works");
        let kept = store
            .auto_clear_group("epsilon")
            .expect("target entry kept");
        assert!(kept.enabled && kept.on_date == 9, "target's own entry wins");
        assert!(!store.auto_clear_enabled("delta"), "source key dropped");
    }

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

    #[test]
    fn bug14_delete_group_refuses_while_subscriptions_remain() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "news");
        let error = store
            .delete_group("news")
            .expect_err("sub-only delete refuses");
        let StoreError::Invalid(fields) = error else {
            panic!("expected Invalid, got {error}");
        };
        assert!(
            fields
                .iter()
                .any(|e| e.message.contains("1 subscription(s)")),
            "refusal names the subscription count: {fields:?}"
        );
        assert_eq!(
            store
                .get_subscription(&sub.id)
                .expect("get works")
                .expect("sub kept")
                .group,
            "news",
            "refused delete leaves the subscription"
        );

        let mut a = valid_input("A");
        a.group = "news".to_string();
        store.add(a).expect("add works");
        assert!(
            store.delete_group("news").is_err(),
            "mixed group (row + subscription) refuses"
        );

        store
            .delete_group("empty-label")
            .expect("truly empty group deletes clean");
        assert_eq!(store.list().expect("list works").len(), 1);
    }

    #[test]
    fn bug19_rename_group_counts_subscriptions() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "news");
        let outcome = store.rename_group("news", "feed").expect("rename works");
        assert_eq!(
            outcome,
            GroupRenameOutcome {
                rows: 0,
                subscriptions: 1
            },
            "sub-only rename reports the moved subscription"
        );
        let moved = store
            .get_subscription(&sub.id)
            .expect("get works")
            .expect("sub kept");
        assert_eq!(moved.group, "feed", "subscription follows rename");

        let mut a = valid_input("A");
        a.group = "feed".to_string();
        store.add(a).expect("add works");
        let mixed = store
            .rename_group("feed", "team")
            .expect("mixed rename works");
        assert_eq!(
            mixed,
            GroupRenameOutcome {
                rows: 1,
                subscriptions: 1
            }
        );
    }

    #[test]
    fn bug23_restore_rejects_unsealable_sealed_subscription_url() {
        use base64::Engine as _;
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sealed = crate::secret::Secret::new("https://example.com/feed.txt")
            .seal()
            .expect("seal works");
        let good = format!(
            r#"{{"outbounds":[],"library":[],"subscriptions":[{{"id":"1","label":"Good","url":"{sealed}"}}],"tombstones":[],"route_presets":[],"meta":{{}}}}"#
        );
        store
            .restore_library_json(&good)
            .expect("valid sealed url restores");
        let subs = store.list_subscriptions().expect("list works");
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].url, "https://example.com/feed.txt", "round-trips");

        let foreign = format!(
            "dpapi1:{}",
            base64::engine::general_purpose::STANDARD.encode([0xabu8; 64])
        );
        let bad = format!(
            r#"{{"outbounds":[],"library":[],"subscriptions":[{{"id":"2","label":"Nightly","url":"{foreign}"}}],"tombstones":[],"route_presets":[],"meta":{{}}}}"#
        );
        let error = store
            .restore_library_json(&bad)
            .expect_err("foreign sealed url refuses");
        let StoreError::Invalid(fields) = error else {
            panic!("expected Invalid, got {error}");
        };
        assert!(
            fields.iter().any(|e| e.message.contains("Nightly")),
            "refusal names the subscription: {fields:?}"
        );
        assert_eq!(
            store.list_subscriptions().expect("list works").len(),
            1,
            "failed restore writes nothing"
        );
    }

    #[test]
    fn bug24_tombstone_backfill_unseals_stored_url() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "news");
        let stored: String = store
            .conn
            .query_row(
                "SELECT url FROM subscriptions WHERE id = ?1",
                rusqlite::params![sub.id.parse::<i64>().expect("id parses")],
                |r| r.get(0),
            )
            .expect("stored url reads");
        assert!(
            stored.trim_start().starts_with(crate::secret::MARKER),
            "v6 stores the url sealed"
        );

        store
            .conn
            .execute(
                "INSERT INTO subscription_tombstones
                   (subscription_id, identity_key, subscription_url, rotation_core)
                 VALUES (?1, 'k', NULL, NULL)",
                rusqlite::params![sub.id],
            )
            .expect("null-url tombstone plants");
        store.create_schema().expect("re-run backfills");

        let backfilled: Option<String> = store
            .conn
            .query_row(
                "SELECT subscription_url FROM subscription_tombstones WHERE identity_key = 'k'",
                [],
                |r| r.get(0),
            )
            .expect("backfilled url reads");
        assert_eq!(
            backfilled.as_deref(),
            Some(sub.url.as_str()),
            "backfill unseals the url for URL-keyed matching"
        );
        let matched: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM subscription_tombstones
                 WHERE subscription_id = ?1 OR subscription_url = ?2",
                rusqlite::params!["99999", sub.url.expose()],
                |r| r.get(0),
            )
            .expect("match query runs");
        assert_eq!(matched, 1, "tombstone matching hits by unsealed url");
    }

    #[test]
    fn bug24_foreign_sealed_url_skips_backfill_without_aborting() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "news");
        store
            .conn
            .execute(
                "UPDATE subscriptions SET url = 'dpapi1:!!!' WHERE id = ?1",
                rusqlite::params![sub.id.parse::<i64>().expect("id parses")],
            )
            .expect("foreign seal planted");
        store
            .conn
            .execute(
                "INSERT INTO subscription_tombstones
                   (subscription_id, identity_key, subscription_url, rotation_core)
                 VALUES (?1, 'foreign', NULL, NULL)",
                rusqlite::params![sub.id],
            )
            .expect("null-url tombstone plants");

        store
            .create_schema()
            .expect("an unreadable url never aborts schema setup");
        let backfilled: Option<String> = store
            .conn
            .query_row(
                "SELECT subscription_url FROM subscription_tombstones WHERE identity_key = 'foreign'",
                [],
                |r| r.get(0),
            )
            .expect("tombstone reads");
        assert!(backfilled.is_none(), "backfill is skipped, url stays NULL");
    }

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

    #[test]
    fn add_invalid_reports_fields_and_writes_nothing() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let error = store
            .add(NewConfig {
                name: "   ".to_string(),
                protocol: String::new(),
                params: Secret::default(),
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

    /// T-05: the auto-assignment asks the SOCKET. A port held by a
    /// listening temp socket is skipped, never handed out.
    #[test]
    fn t05_auto_assignment_skips_a_port_held_by_a_listening_socket() {
        let held = std::net::TcpListener::bind("127.0.0.1:0").expect("ephemeral port");
        let held_port = held.local_addr().expect("addr").port();
        let spare = std::net::TcpListener::bind("127.0.0.1:0").expect("ephemeral port");
        let free_port = spare.local_addr().expect("addr").port();
        drop(spare);
        let mut taken: HashSet<i64> = HashSet::new();
        let picked = take_free_auto_port(
            std::iter::once(held_port).chain(std::iter::once(free_port)),
            &mut taken,
        )
        .expect("a free port is still picked");
        assert_eq!(picked, free_port, "the held port is skipped");
        assert!(
            taken.contains(&(free_port as i64)),
            "the pick joins the taken set"
        );
        assert!(
            !taken.contains(&(held_port as i64)),
            "the held port is never handed out"
        );
    }

    /// T-05: the add path skips a held port too — the row lands on the
    /// next free auto port, never on the one the socket holds.
    #[test]
    fn t05_add_auto_port_skips_the_held_first_candidate() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let held = std::net::TcpListener::bind(format!("127.0.0.1:{AUTO_PORT_START}")).ok();
        let created = store
            .add(valid_input("Auto"))
            .expect("the auto port still assigns");
        assert_ne!(created.port, AUTO_PORT_START, "the held port is skipped");
        assert!(
            is_auto_port(created.port),
            "still an auto port: {}",
            created.port
        );
        drop(held);
    }

    #[test]
    fn a4_auto_pool_is_global_across_binds() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let loopback = store
            .add(NewConfig {
                bind_ip: "127.0.0.1".to_string(),
                port: String::new(),
                ..valid_input("Loop")
            })
            .expect("loopback auto assigns");
        let lan = store
            .add(NewConfig {
                bind_ip: "0.0.0.0".to_string(),
                port: String::new(),
                ..valid_input("Lan")
            })
            .expect("LAN auto assigns");
        assert_ne!(
            loopback.port, lan.port,
            "global pool: no per-bind reuse ({} vs {})",
            loopback.port, lan.port
        );
        assert_eq!(
            AUTO_PORT_CAPACITY,
            (AUTO_PORT_END - AUTO_PORT_START + 1) as usize
                + (AUTO_PORT_SPILL_END - AUTO_PORT_SPILL_START + 1) as usize,
            "exhaustion only past both ranges"
        );
    }
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

    #[test]
    fn proxy_url_pair_format() {
        let (socks, http) = proxy_urls("127.0.0.1", 10801);
        assert_eq!(socks, "socks5://127.0.0.1:10801");
        assert_eq!(http, "http://127.0.0.1:10801");
    }

    #[test]
    fn export_warning_names_secret_classes() {
        let text = export_warning_text();
        for class in ["private keys", "tokens", "passwords", "plaintext"] {
            assert!(text.contains(class), "warning names {class}: {text}");
        }
    }

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

    const SUB_UUID: &str = "11111111-2222-4333-8444-555555555555";

    fn sub_body_two() -> String {
        format!("vless://{SUB_UUID}@a.example.com:443#one\ntrojan://pw@b.example.com:443#two")
    }

    fn add_sub(store: &mut Store, group: &str) -> Subscription {
        store
            .add_subscription("Nightly", "https://example.com/sub.txt", group, true)
            .expect("add subscription works")
    }

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

    #[test]
    fn subscription_refresh_host_change_updates_in_place_keeps_port_traffic() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("refresh works");
        let before: Vec<ProxyConfig> = store.list().expect("list works");
        assert_eq!(before.len(), 2);
        let moved = before
            .iter()
            .find(|row| row.name == "one")
            .expect("seeded member one");
        let moved_id = moved.id.clone();
        let moved_port = moved.port;
        store
            .add_traffic(&moved_id, 1_000_000, 500_000)
            .expect("traffic seeds");

        let outcome = store
            .refresh_with_body(
                &sub.id,
                &format!(
                    "vless://{SUB_UUID}@c.example.com:443#one\ntrojan://pw@b.example.com:443#two"
                ),
                200,
            )
            .expect("refresh works");
        assert_eq!(outcome.updated, 1, "host move counts updated");
        assert_eq!(outcome.added, 0, "no re-add");
        assert_eq!(outcome.removed, 0, "no delete");
        let after = store.get(&moved_id).expect("get works").expect("kept");
        assert_eq!(after.id, moved_id, "same row id kept");
        assert_eq!(after.port, moved_port, "port preserved");
        assert!(!after.enabled, "refresh never enables");
        assert!(after.params.contains("c.example.com"), "params moved");
        let totals = store.traffic_totals().expect("totals read");
        let kept = totals.get(&moved_id).expect("traffic row kept");
        assert_eq!(kept.0, 1_000_000, "rx preserved");
        assert_eq!(kept.1, 500_000, "tx preserved");
    }

    #[test]
    fn subscription_refresh_running_row_stays_in_use_until_stopped() {
        use std::collections::HashSet;
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("refresh works");
        let live_id = store
            .list()
            .expect("list works")
            .iter()
            .find(|row| row.name == "one")
            .expect("seeded member one")
            .id
            .clone();
        let running: HashSet<String> = [live_id.clone()].into_iter().collect();

        let one_body = "trojan://pw@b.example.com:443#two".to_string();
        let outcome = store
            .refresh_with_body_running(&sub.id, &one_body, 200, &running)
            .expect("refresh works");
        assert_eq!(outcome.in_use, 1, "serving row counted still-in-use");
        assert_eq!(outcome.removed, 0, "serving row never deleted");
        let kept = store.get(&live_id).expect("get works").expect("kept");
        assert_eq!(kept.name, "one", "kept verbatim");
        assert_eq!(store.list().expect("list works").len(), 2);

        let outcome = store
            .refresh_with_body(&sub.id, &one_body, 300)
            .expect("refresh works");
        assert_eq!(outcome.in_use, 0);
        assert_eq!(outcome.removed, 1, "stopped row deletes");
        assert_eq!(store.list().expect("list works").len(), 1);
    }

    #[test]
    fn t70_outcome_named_lists_match_counts() {
        use std::collections::HashSet;
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");

        let outcome = store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("refresh works");
        assert_eq!(outcome.added, 2);
        let mut added = outcome.added_names.clone();
        added.sort();
        assert_eq!(
            added,
            vec!["one".to_string(), "two".to_string()],
            "inserts name their rows"
        );
        assert_eq!(outcome.updated_names.len(), outcome.updated);
        assert_eq!(outcome.removed_names.len(), outcome.removed);
        assert_eq!(outcome.kept_in_use_names.len(), outcome.in_use);

        let host_move =
            format!("vless://{SUB_UUID}@c.example.com:443#one\ntrojan://pw@b.example.com:443#two");
        let outcome = store
            .refresh_with_body(&sub.id, &host_move, 200)
            .expect("refresh works");
        assert_eq!(outcome.updated, 1, "host move counts updated");
        assert_eq!(
            outcome.updated_names,
            vec!["one".to_string()],
            "updates name their rows"
        );
        assert_eq!(outcome.added_names.len(), outcome.added);
        assert_eq!(outcome.removed_names.len(), outcome.removed);
        assert_eq!(outcome.kept_in_use_names.len(), outcome.in_use);

        let outcome = store
            .refresh_with_body(&sub.id, &host_move, 250)
            .expect("refresh works");
        assert_eq!(outcome.updated, 0, "same nodes, only order moves");
        assert!(
            outcome.added_names.is_empty()
                && outcome.updated_names.is_empty()
                && outcome.removed_names.is_empty()
                && outcome.kept_in_use_names.is_empty(),
            "no churn, no names"
        );

        let one_id = store
            .list()
            .expect("list works")
            .into_iter()
            .find(|row| row.name == "one")
            .expect("seeded member one")
            .id;
        let running: HashSet<String> = [one_id].into_iter().collect();
        let outcome = store
            .refresh_with_body_running(
                &sub.id,
                "trojan://pw3@c.example.com:443#three",
                300,
                &running,
            )
            .expect("refresh works");
        assert_eq!(outcome.in_use, 1);
        assert_eq!(
            outcome.kept_in_use_names,
            vec!["one".to_string()],
            "running rows land in the kept list"
        );
        assert_eq!(outcome.removed, 1);
        assert_eq!(outcome.removed_names, vec!["two".to_string()]);
        assert_eq!(outcome.added, 1);
        assert_eq!(outcome.added_names, vec!["three".to_string()]);
        assert_eq!(outcome.updated, 0);
        assert!(outcome.updated_names.is_empty());
        assert_eq!(
            outcome.added_names.len(),
            outcome.added,
            "lists match counts as a property"
        );
        assert_eq!(outcome.updated_names.len(), outcome.updated);
        assert_eq!(outcome.removed_names.len(), outcome.removed);
        assert_eq!(outcome.kept_in_use_names.len(), outcome.in_use);
    }

    #[test]
    fn subscription_refresh_reorders_members_to_remote_order() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("refresh works");

        let hand = store
            .add(NewConfig {
                name: "Hand made".to_string(),
                protocol: "vless".to_string(),
                params: Secret::new("{}"),
                group: "travel".to_string(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("hand add works");

        let outcome = store
            .refresh_with_body(
                &sub.id,
                &format!(
                    "trojan://pw@b.example.com:443#two\nvless://{SUB_UUID}@a.example.com:443#one"
                ),
                200,
            )
            .expect("refresh works");
        assert_eq!(outcome.added, 0);
        assert_eq!(outcome.removed, 0);
        assert_eq!(outcome.updated, 0, "same nodes, only order moves");
        let names: Vec<String> = store
            .list()
            .expect("list works")
            .iter()
            .map(|row| row.name.clone())
            .collect();
        assert_eq!(
            names,
            vec!["two".to_string(), "one".to_string(), hand.name.clone()],
            "members follow remote order, hand row keeps its slot: {names:?}"
        );

        store
            .refresh_with_body(&sub.id, &sub_body_two(), 300)
            .expect("refresh works");
        let names: Vec<String> = store
            .list()
            .expect("list works")
            .iter()
            .map(|row| row.name.clone())
            .collect();
        assert_eq!(
            names,
            vec!["one".to_string(), "two".to_string(), hand.name.clone()],
            "order tracks remote both ways: {names:?}"
        );
    }

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
                params: Secret::new("{}"),
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

    #[test]
    fn subscription_userinfo_parses_standard_shape() {
        let quota = parse_subscription_userinfo(
            "upload=1024; download=2048; total=1073741824; expire=1735689600",
        )
        .expect("standard header parses");
        assert_eq!(quota.used, Some(3_072));
        assert_eq!(quota.total, Some(1_073_741_824));
        assert_eq!(quota.expire, Some(1_735_689_600));

        let mixed = parse_subscription_userinfo(" Upload = 7 ;DOWNLOAD=8;foo=bar ")
            .expect("tolerant parse works");
        assert_eq!(mixed.used, Some(15));
        assert_eq!(mixed.total, None);
        assert_eq!(mixed.expire, None);

        assert_eq!(parse_subscription_userinfo(""), None);
        assert_eq!(parse_subscription_userinfo("upload=abc; total=xyz"), None);
        assert_eq!(parse_subscription_userinfo("nothing-here"), None);
        assert_eq!(parse_subscription_userinfo("upload=12.5; total=1GB"), None);

        let partial = parse_subscription_userinfo("total=99999999999999999999999")
            .expect("huge total saturates");
        assert_eq!(partial.total, Some(i64::MAX));
        let expire_only =
            parse_subscription_userinfo("expire=1735689600").expect("expire alone counts");
        assert_eq!(expire_only.used, None);
        assert_eq!(expire_only.expire, Some(1_735_689_600));
    }

    #[test]
    fn subscription_quota_round_trips_and_refresh_keeps_it() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        let seeded = store
            .get_subscription(&sub.id)
            .expect("get works")
            .expect("sub present");
        assert_eq!(seeded.quota_used, None, "no quota until a header lands");
        store
            .set_subscription_quota(
                &sub.id,
                Some(SubscriptionQuota {
                    used: Some(3_072),
                    total: Some(1_073_741_824),
                    expire: Some(1_735_689_600),
                }),
            )
            .expect("quota sets");
        let kept = store
            .get_subscription(&sub.id)
            .expect("get works")
            .expect("sub present");
        assert_eq!(kept.quota_used, Some(3_072));
        assert_eq!(kept.quota_total, Some(1_073_741_824));
        assert_eq!(kept.quota_expire, Some(1_735_689_600));

        store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("refresh works");
        let after_success = store
            .get_subscription(&sub.id)
            .expect("get works")
            .expect("sub present");
        assert_eq!(after_success.quota_used, Some(3_072), "merge keeps quota");
        store
            .refresh_with_body(&sub.id, "bogus://x", 200)
            .expect("failure is an outcome");
        let after_failure = store
            .get_subscription(&sub.id)
            .expect("get works")
            .expect("sub present");
        assert_eq!(after_failure.quota_used, Some(3_072), "failure keeps quota");

        store
            .set_subscription_quota(&sub.id, None)
            .expect("quota clears");
        let cleared = store
            .get_subscription(&sub.id)
            .expect("get works")
            .expect("sub present");
        assert_eq!(cleared.quota_used, None);
        assert_eq!(cleared.quota_total, None);
        assert_eq!(cleared.quota_expire, None);

        assert!(
            store
                .set_subscription_quota("9999", Some(SubscriptionQuota::default()))
                .is_err(),
            "unknown id fails"
        );
    }

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

    #[test]
    fn subscription_refresh_counts_foreign_identity_as_duplicate() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store
            .add(NewConfig {
                name: "Mine".to_string(),
                protocol: "trojan".to_string(),
                params: "{\"password\":\"pw\",\"server\":\"b.example.com\",\"server_port\":443,\"type\":\"trojan\",\"sni\":\"b.example.com\"}"
                    .to_string().into(),
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

    #[test]
    fn subscription_failures_decode_is_forgiving() {
        assert!(decode_failures(None).is_empty());
        assert!(decode_failures(Some(String::new())).is_empty());
        assert!(decode_failures(Some("not-json".to_string())).is_empty());
        assert!(decode_failures(Some("[{\"index\": 2}]".to_string())).len() == 1);
    }

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

        let empty = Store::open_in_memory().expect("fresh store opens");
        let payload = empty.export_library_json().expect("empty export works");
        let (quarantined_payload, skipped) = empty
            .export_library_json_with_quarantine()
            .expect("quarantine export works");
        assert_eq!(
            payload_without_envelope(&payload),
            payload_without_envelope(&quarantined_payload),
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
            payload_without_envelope(&restored.export_library_json().expect("re-export works")),
            payload_without_envelope(&payload),
            "empty round-trip is byte-identical"
        );
    }

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

    #[test]
    fn export_io_error_names_path_never_database() {
        let store = Store::open_in_memory().expect("in-memory store opens");

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

    #[test]
    fn due_for_refresh_hook() {
        let scheduled = Subscription {
            id: "1".to_string(),
            label: "Nightly".to_string(),
            url: Secret::new("https://example.com/sub.txt"),
            group: "travel".to_string(),
            auto_refresh: true,
            last_refresh_at: None,
            last_error: None,
            failures: Vec::new(),
            interval_secs: DEFAULT_AUTO_REFRESH_SECS,
            quota_used: None,
            quota_total: None,
            quota_expire: None,
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

    #[test]
    fn export_confirm_description_is_warning_verbatim() {
        assert_eq!(export_confirm_description(), export_warning_text());
    }

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

    #[test]
    fn export_keeps_non_import_rows_metadata_only() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        export_seed(&mut store);
        store
            .add(NewConfig {
                name: "Hand made".to_string(),
                protocol: "vless".to_string(),
                params: Secret::new("{}"),
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

    #[test]
    fn t09_group_prune_kinds_pinned() {
        assert_eq!(GroupPruneKind::ALL.len(), 4, "four prune kinds");
        let labels: Vec<&str> = GroupPruneKind::ALL
            .iter()
            .map(|kind| kind.label())
            .collect();
        assert_eq!(
            labels,
            [
                "Remove duplicates",
                "Remove unavailable",
                "Remove invalid",
                "Clear tests"
            ]
        );
        for label in &labels {
            assert!(
                !label.to_ascii_lowercase().contains("insecure"),
                "no insecure prune: {label}"
            );
        }
        assert_eq!(PRUNE_PREVIEW_LIMIT, 20, "preview cap pinned");
    }

    fn t09_input(name: &str, group: &str, params: &str) -> NewConfig {
        NewConfig {
            name: name.to_string(),
            protocol: "vless".to_string(),
            params: Secret::new(params),
            group: group.to_string(),
            bind_ip: String::new(),
            port: String::new(),
        }
    }

    #[test]
    fn t09_remove_duplicates_keeps_first_with_preview() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let a1 = store.add(t09_input("A1", "g", "")).expect("add works");
        let a2 = store.add(t09_input("A2", "g", "")).expect("add works");
        let b = store
            .add(t09_input(
                "B",
                "g",
                r#"{"server":"example.com","server_port":443,"uuid":"11111111-2222-4333-8444-555555555555"}"#,
            ))
            .expect("add works");

        store
            .save_health_snapshot(&dead_snapshot(&b.id))
            .expect("save works");
        let dupes = store.duplicate_rows_in_group("g").expect("scan works");
        assert_eq!(dupes.len(), 1, "one later copy");
        assert_eq!(dupes[0].id, a2.id, "first copy kept");
        let (total, preview) = store
            .duplicate_preview_in_group("g")
            .expect("preview works");
        assert_eq!(total, 1);
        assert_eq!(preview, vec!["A2".to_string()]);
        let confirm = prune_duplicates_confirm_text("g", total, &preview);
        assert!(confirm.contains("\"g\""), "names scope: {confirm}");
        assert!(confirm.contains('1'), "names count: {confirm}");
        assert!(confirm.contains("A2"), "previews names: {confirm}");
        let removed = store
            .remove_duplicates_in_group("g", &std::collections::HashSet::new())
            .expect("prune works");
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].id, a2.id);
        let kept: Vec<String> = store
            .list()
            .expect("list works")
            .into_iter()
            .map(|row| row.id)
            .collect();
        assert!(kept.contains(&a1.id), "first copy kept");
        assert!(kept.contains(&b.id), "distinct row kept");
        assert!(!kept.contains(&a2.id), "dupe removed");
        assert!(
            store
                .duplicate_rows_in_group("g")
                .expect("rescan works")
                .is_empty(),
            "idempotent"
        );
    }

    #[test]
    fn t09_duplicate_preview_caps_at_twenty() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let mut inputs = Vec::new();
        for i in 0..25 {
            inputs.push(t09_input(&format!("D{i:02}"), "g", ""));
        }
        store.add_batch(inputs).expect("adds work");
        let (total, preview) = store
            .duplicate_preview_in_group("g")
            .expect("preview works");
        assert_eq!(total, 24, "first copy kept, rest dupes");
        assert_eq!(preview.len(), PRUNE_PREVIEW_LIMIT);
        let confirm = prune_duplicates_confirm_text("g", total, &preview);
        assert!(confirm.contains("24"), "count names all: {confirm}");
        assert!(confirm.contains("+4 more"), "overflow named: {confirm}");
    }

    #[test]
    fn t09_remove_unavailable_drops_dead_only() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let live = store.add(valid_input("Live")).expect("add works");
        let dead = store.add(valid_input("Dead")).expect("add works");
        store.add(valid_input("Fresh")).expect("add works");
        store
            .save_health_snapshot(&live_snapshot(&live.id, 33))
            .expect("save works");
        store
            .save_health_snapshot(&dead_snapshot(&dead.id))
            .expect("save works");
        let found = store
            .unavailable_rows_in_group("default")
            .expect("scan works");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, dead.id);
        let confirm = prune_unavailable_confirm_text("default", found.len());
        assert!(confirm.contains("\"default\""), "names scope: {confirm}");
        let removed = store
            .remove_unavailable_in_group("default", &std::collections::HashSet::new())
            .expect("prune works");
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].id, dead.id);
        let kept: Vec<String> = store
            .list()
            .expect("list works")
            .into_iter()
            .map(|row| row.name)
            .collect();
        assert!(kept.contains(&"Live".to_string()));
        assert!(kept.contains(&"Fresh".to_string()), "untested stays");
        assert!(!kept.contains(&"Dead".to_string()));
    }

    fn t09_insert_invalid_row(store: &mut Store, name: &str, group: &str) -> String {
        let sealed =
            Store::seal_or_fail(r#"{"type":"vless","server":"example.com","server_port":443}"#)
                .expect("seal works");
        store
            .conn
            .execute(
                "INSERT INTO proxy_configs
                   (name, protocol, params, grp, enabled, bind_ip, port, sort_order)
                 VALUES (?1, 'vless', ?2, ?3, 0, '127.0.0.1', 19999, 999999)",
                rusqlite::params![name, sealed, group],
            )
            .expect("raw insert works");
        store.conn.last_insert_rowid().to_string()
    }

    #[test]
    fn t09_remove_invalid_sweeps_failures_only() {
        assert!(is_valid_config("vless", ""), "opaque legacy passes");
        assert!(
            is_valid_config(
                "vless",
                r#"{"server":"example.com","server_port":443,"uuid":"11111111-2222-4333-8444-555555555555"}"#
            ),
            "complete row passes"
        );
        assert!(
            !is_valid_config(
                "vless",
                r#"{"type":"vless","server":"example.com","server_port":443}"#
            ),
            "import-shaped without uuid fails"
        );
        assert!(!is_valid_config("nope", ""), "unknown protocol fails");
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let good = store.add(valid_input("Good")).expect("add works");
        let bad_id = t09_insert_invalid_row(&mut store, "Bad", "default");
        let found = store.invalid_rows_in_group("default").expect("scan works");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, bad_id);
        let confirm = prune_invalid_confirm_text("default", found.len());
        assert!(confirm.contains("\"default\""), "names scope: {confirm}");
        let removed = store
            .remove_invalid_in_group("default", &std::collections::HashSet::new())
            .expect("prune works");
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].id, bad_id);
        assert!(
            store.get(&good.id).expect("get works").is_some(),
            "valid row kept"
        );
    }

    #[test]
    fn t09_clear_tests_in_group_scoped() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let mut a = valid_input("A");
        a.group = "g1".to_string();
        let mut b = valid_input("B");
        b.group = "g2".to_string();
        let a = store.add(a).expect("add works");
        let b = store.add(b).expect("add works");
        store
            .save_health_snapshot(&live_snapshot(&a.id, 10))
            .expect("save works");
        store
            .save_health_snapshot(&dead_snapshot(&b.id))
            .expect("save works");
        let confirm = clear_group_tests_confirm_text("g1", 1);
        assert!(confirm.contains("\"g1\""), "names scope: {confirm}");
        assert_eq!(store.clear_tests_in_group("g1").expect("clear works"), 1);
        let snaps = store.load_health_snapshots().expect("load works");
        assert_eq!(snaps.len(), 1, "other group kept");
        assert_eq!(snaps[0].config_id, b.id);
        assert!(store.get(&a.id).expect("get works").is_some(), "row kept");
        assert_eq!(
            store.clear_tests_in_group("missing").expect("clear works"),
            0,
            "missing group clears nothing"
        );
    }

    #[test]
    fn t09_security_label_and_sort() {
        assert_eq!(row_security_label("vless", r#"{"security":"tls"}"#), "tls");
        assert_eq!(
            row_security_label("vless", r#"{"security":"reality"}"#),
            "reality"
        );
        assert_eq!(row_security_label("shadowsocks", "{}"), "none");
        assert_eq!(row_security_label("anytls", "{}"), "tls");
        assert_eq!(security_rank("reality"), 3);
        assert_eq!(security_rank("tls"), 2);
        assert_eq!(security_rank("none"), 1);
        let row = |name: &str, protocol: &str, params: &str| ProxyConfig {
            id: "0".to_string(),
            name: name.to_string(),
            protocol: protocol.to_string(),
            params: Secret::new(params),
            group: "g".to_string(),
            enabled: true,
            bind_ip: "127.0.0.1".to_string(),
            port: 10801,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        };
        let mut rows = [
            row("c", "vless", r#"{"security":"tls"}"#),
            row("a", "trojan", "{}"),
            row("b", "vless", r#"{"security":"reality"}"#),
        ];
        rows.sort_by(compare_type_security);
        let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c"], "type asc, reality before tls");
    }

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

    fn invalid_fields(error: &StoreError) -> Vec<String> {
        match error {
            StoreError::Invalid(fields) => fields.iter().map(|e| e.field.to_string()).collect(),
            other => panic!("expected Invalid, got {other}"),
        }
    }

    #[test]
    fn manual_add_unknown_protocol_or_non_json_rejected() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let error = store
            .add(NewConfig {
                name: "Bad proto".to_string(),
                protocol: "bogus".to_string(),
                params: Secret::default(),
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
                params: Secret::new("not-json"),
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

    #[test]
    fn manual_add_import_shaped_without_credential_rejected() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let error = store
            .add(NewConfig {
                name: "No uuid".to_string(),
                protocol: "vless".to_string(),
                params: "{\"type\":\"vless\",\"server\":\"a.example.com\",\"server_port\":443}"
                    .to_string()
                    .into(),
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

        let ok = store
            .add(NewConfig {
                name: "Opaque".to_string(),
                protocol: "vless".to_string(),
                params: Secret::default(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("opaque params still valid");
        assert_eq!(ok.params, "{}");
        assert!(!is_import_shaped_params(&ok.params));
    }

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
                    params: Some(Secret::new("not-json")),
                    ..Default::default()
                },
            )
            .expect_err("bad params edit must fail");
        assert!(invalid_fields(&error).contains(&"params".to_string()));
        let kept = store.get(&created.id).expect("get works").expect("kept");
        assert_eq!(kept, created, "record unchanged after failed edits");
    }

    #[test]
    fn edit_blank_preserves_current_values() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let created = store
            .add(NewConfig {
                name: "Keep".to_string(),
                protocol: "trojan".to_string(),
                params: "{\"type\":\"trojan\",\"server\":\"b.example.com\",\"server_port\":443,\"password\":\"pw\",\"sni\":\"b.example.com\"}".to_string().into(),
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
                    params: Some(Secret::default()),
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
                params: "{\"type\":\"vless\",\"server\":\"a.example.com\",\"server_port\":443,\"uuid\":\"u\"}".to_string().into(),
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
                params: Secret::new("{}"),
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

    fn vless_params_with(server: &str, extra: &[(&str, &str)]) -> Secret {
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
        Secret::new(serde_json::Value::Object(map).to_string())
    }

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

        let mut value: serde_json::Value = serde_json::from_str(&row.params).expect("params parse");
        value["mux"] = serde_json::Value::String("on".to_string());
        let fixed = Secret::new(value.to_string());
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

        let key_three = widened_identity(&three.protocol, &three.params);
        let key_four = widened_identity(&four.protocol, &four.params);
        assert_eq!(
            three.port < four.port,
            key_three < key_four,
            "allocation follows key order"
        );
    }

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

    #[test]
    fn t25_refresh_delete_prunes_snapshot() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = add_sub(&mut store, "travel");
        store
            .refresh_with_body(&sub.id, &sub_body_two(), 100)
            .expect("refresh works");

        let row = store
            .list()
            .expect("list works")
            .into_iter()
            .find(|row| row.name == "two")
            .expect("member two doomed by the next refresh");
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
            payload_without_envelope(&second.export_library_json().expect("re-export works")),
            payload_without_envelope(&payload),
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

        let refresh = second
            .refresh_with_body(&sub.id, &sub_body_two(), 200)
            .expect("refresh works");
        assert_eq!(refresh.added, 0);
        assert_eq!(refresh.skipped, 1);
        assert_eq!(second.list().expect("list works").len(), 1);
    }

    #[test]
    fn t72_export_envelope_fields_present() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        export_seed(&mut store);
        let payload = store.export_library_json().expect("export works");

        assert!(
            payload
                .starts_with("{\"schema\":\"manyproxy-library\",\"schema_version\":1,\"created\":"),
            "envelope keys lead the payload: {payload}"
        );
        let value: serde_json::Value = serde_json::from_str(&payload).expect("export parses");
        assert_eq!(value["schema"], "manyproxy-library");
        assert_eq!(value["schema_version"], 1);
        let created = value["created"].as_i64().expect("created is unix seconds");
        assert!(
            created > 1_600_000_000,
            "created is a real stamp, got {created}"
        );
        let checksum = value["checksum"].as_str().expect("checksum is a string");
        assert_eq!(checksum.len(), 16, "fnv-1a 64 as hex, got {checksum}");
        assert!(
            checksum.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "checksum is hex, got {checksum}"
        );
        for key in ["outbounds", "library", "subscriptions", "tombstones"] {
            assert!(value.get(key).is_some(), "payload key {key} survives");
        }
        let after_envelope = payload
            .split_once("\"checksum\":\"")
            .expect("checksum key present")
            .1
            .split_once("\",")
            .expect("checksum value ends")
            .1;
        assert!(
            after_envelope.starts_with("\"outbounds\":["),
            "payload keys follow the envelope in the documented order: {after_envelope}"
        );
        assert_eq!(
            checksum,
            export_checksum(&envelope_free_payload(
                value.as_object().expect("export is an object")
            )),
            "checksum covers the envelope-free payload bytes"
        );
    }

    #[test]
    fn t72_restore_accepts_enveloped_export() {
        let mut source = Store::open_in_memory().expect("source opens");
        export_seed(&mut source);
        let payload = source.export_library_json().expect("export works");
        assert!(payload.contains("manyproxy-library"), "export is enveloped");

        let mut target = Store::open_in_memory().expect("target opens");
        let outcome = target
            .restore_library_json(&payload)
            .expect("enveloped restore works");
        assert_eq!(outcome.configs, 3, "all seeded rows restore");
        assert_eq!(outcome.subscriptions, 1);
        assert_eq!(
            payload_without_envelope(&target.export_library_json().expect("re-export works")),
            payload_without_envelope(&payload),
            "payload survives the enveloped round trip"
        );
    }

    #[test]
    fn t72_checksum_mismatch_refuses_without_writes() {
        let dir = std::env::temp_dir().join(format!(
            "manyproxy-t72sum-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock works")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir works");
        let path = dir.join("library.db");
        let tampered = {
            let mut source = Store::open_in_memory().expect("source opens");
            source.add(valid_input("Work")).expect("row seeds");
            source
                .export_library_json()
                .expect("export works")
                .replace("\"Work\"", "\"W0rk\"")
        };
        {
            let mut live = Store::open_file(&path).expect("live opens");
            live.add(valid_input("Live")).expect("live seeds");
        }
        let before = std::fs::read(&path).expect("live bytes");
        let entries_before = std::fs::read_dir(&dir).expect("dir reads").count();

        let error = Store::restore_library_file(&path, &tampered, now_unix())
            .expect_err("tampered export refuses");
        assert!(
            error.to_string().contains("checksum mismatch"),
            "refusal names the mismatch: {error}"
        );
        assert_eq!(
            std::fs::read(&path).expect("live bytes"),
            before,
            "live database untouched"
        );
        assert_eq!(
            std::fs::read_dir(&dir).expect("dir reads").count(),
            entries_before,
            "no backup and no swap temp written"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn t72_version_mismatch_refuses_without_writes() {
        let dir = std::env::temp_dir().join(format!(
            "manyproxy-t72ver-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock works")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir works");
        let path = dir.join("library.db");
        let enveloped = {
            let mut source = Store::open_in_memory().expect("source opens");
            source.add(valid_input("Work")).expect("row seeds");
            source.export_library_json().expect("export works")
        };
        {
            let mut live = Store::open_file(&path).expect("live opens");
            live.add(valid_input("Live")).expect("live seeds");
        }
        let before = std::fs::read(&path).expect("live bytes");
        let entries_before = std::fs::read_dir(&dir).expect("dir reads").count();

        let future = enveloped.replace("\"schema_version\":1", "\"schema_version\":2");
        let error = Store::restore_library_file(&path, &future, now_unix())
            .expect_err("future schema version refuses");
        let text = error.to_string();
        assert!(
            text.contains("library export schema not supported"),
            "typed refusal: {text}"
        );
        assert!(
            text.contains("supported ≤1"),
            "names the supported version: {text}"
        );
        assert_eq!(
            std::fs::read(&path).expect("live bytes"),
            before,
            "live database untouched"
        );
        assert_eq!(
            std::fs::read_dir(&dir).expect("dir reads").count(),
            entries_before,
            "no backup and no swap temp written"
        );

        let foreign = enveloped.replace(
            "\"schema\":\"manyproxy-library\"",
            "\"schema\":\"thrn-archive\"",
        );
        let error = Store::restore_library_file(&path, &foreign, now_unix())
            .expect_err("foreign schema refuses");
        assert!(
            error
                .to_string()
                .contains("library export schema not supported"),
            "foreign schema refuses loudly: {error}"
        );
        assert_eq!(
            std::fs::read(&path).expect("live bytes"),
            before,
            "still untouched"
        );
        assert_eq!(
            std::fs::read_dir(&dir).expect("dir reads").count(),
            entries_before,
            "still no writes"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn t72_legacy_unenveloped_export_still_restores() {
        let mut source = Store::open_in_memory().expect("source opens");
        export_seed(&mut source);
        let payload = source.export_library_json().expect("export works");
        let legacy = payload_without_envelope(&payload);
        assert!(
            !legacy.contains("\"schema\""),
            "legacy payload carries no envelope: {legacy}"
        );

        let mut target = Store::open_in_memory().expect("target opens");
        let outcome = target
            .restore_library_json(&legacy)
            .expect("legacy export restores as today");
        assert_eq!(outcome.configs, 3);
        assert_eq!(outcome.subscriptions, 1);
        assert!(
            target
                .restore_library_json("{\"library\": [], \"subscriptions\": []}")
                .is_ok(),
            "hand-written pre-envelope payloads still restore"
        );
    }

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

        for alias in ["localhost", "::1", "LOCALHOST", " 127.0.0.1 "] {
            let mut second = valid_entry();
            second["id"] = serde_json::Value::String("2".to_string());
            second["name"] = serde_json::Value::String("two".to_string());
            second["bind_ip"] = serde_json::Value::String(alias.to_string());
            let mut store = Store::open_in_memory().expect("in-memory store opens");
            let payload = payload_with(vec![valid_entry(), second]);
            let error = store
                .restore_library_json(&payload)
                .expect_err(&format!("alias {alias:?} must collide"));
            assert!(error.to_string().contains("duplicates"), "got {error}");
            assert!(store.list().expect("list works").is_empty());
        }

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
            params: Secret::default(),
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

        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let batch = crate::import::BatchResult {
            configs: vec![
                crate::import::ImportedConfig {
                    name: "AwgFive".to_string(),
                    protocol: "amneziawg".to_string(),
                    params: first.into(),
                    host: "192.0.2.90".to_string(),
                    port: 51820,
                },
                crate::import::ImportedConfig {
                    name: "AwgNine".to_string(),
                    protocol: "amneziawg".to_string(),
                    params: second.into(),
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

        let probe =
            std::env::temp_dir().join(format!("manyproxy-t27-harden-{}.tmp", std::process::id()));
        std::fs::write(&probe, "secrets").expect("probe writes");
        harden_file_permissions(&probe);
        assert!(probe.exists());
        let _ = std::fs::remove_file(&probe);
    }

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

        store
            .add_subscription("Plain", "http://example.com/sub.txt", "g", true)
            .expect("http add allowed");
    }

    #[test]
    fn bug03_short_form_ip_literals_classify_as_internal() {
        for host in [
            "127.1",
            "0x7f000001",
            "2130706433",
            "0177.0.0.1",
            "127.0.0.1",
            "0x7f.0.0.1",
            "localhost",
            "app.localhost",
            "0.0.0.0",
            "169.254.9.9",
            "10.1.2.3",
            "192.168.0.9",
            "172.31.255.1",
            "::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(is_internal_host(host), "internal: {host}");
        }
        for host in [
            "8.8.8.8",
            "93.184.216.34",
            "example.com",
            "beef.example",
            "172.32.0.1",
            "192.169.0.1",
        ] {
            assert!(!is_internal_host(host), "public: {host}");
        }
    }

    #[test]
    fn bug03_short_form_literal_urls_rejected_at_storage() {
        for url in [
            "http://127.1:8080/x",
            "http://2130706433/x",
            "http://0x7f000001/x",
            "http://0177.0.0.1/x",
            "https://127.1/x",
        ] {
            assert!(
                Store::validated_subscription_url(url).is_err(),
                "rejected: {url}"
            );
        }
        for url in [
            "https://8.8.8.8/x",
            "https://example.com/x",
            "http://example.com/x",
        ] {
            assert!(
                Store::validated_subscription_url(url).is_ok(),
                "accepted: {url}"
            );
        }
    }

    #[test]
    fn bug03_subscription_url_host_unwraps_brackets_and_userinfo() {
        assert_eq!(
            subscription_url_host("https://user:pw@[::1]:8443/x").as_deref(),
            Some("::1")
        );
        assert_eq!(
            subscription_url_host("https://example.com:443/x").as_deref(),
            Some("example.com")
        );
        assert_eq!(subscription_url_host("ftp://example.com/x"), None);
    }

    #[test]
    fn a5_redirect_rebind_caps_hold() {
        use SubscriptionFetchPolicy::{Allow, Block, Warn};

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
                subscription_fetch_policy_resolved(
                    "https://example.com/sub.txt",
                    &[public, private]
                ),
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

        assert!(
            matches!(
                subscription_fetch_policy_resolved("http://127.0.0.1/sub", &[public]),
                Block { .. }
            ),
            "literal internal never laundered"
        );

        let reason = match subscription_fetch_policy("http://example.com/sub?token=hunter2") {
            Warn { reason } => reason,
            other => panic!("expected Warn, got {other:?}"),
        };
        assert!(!reason.contains("hunter2"), "no token echo: {reason}");
        assert!(!reason.contains("example.com"), "no URL echo: {reason}");

        assert_eq!(SUBSCRIPTION_MAX_BYTES, 8 * 1024 * 1024);
        assert_eq!(SUBSCRIPTION_TIMEOUT_SECS, 20);
        assert_eq!(SUBSCRIPTION_MAX_REDIRECTS, 5);

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

        let outcome = store
            .refresh_with_bytes(&sub.id, sub_body_two().as_bytes(), 400)
            .expect("valid bytes refresh");
        assert!(outcome.error.is_none(), "clean, got {outcome:?}");
    }

    #[test]
    fn ticket27_manual_ssh_noise_rows_validate() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store
            .add(NewConfig {
                name: "Ssh".to_string(),
                protocol: "ssh".to_string(),
                params: "{\"type\":\"ssh\",\"server\":\"a.example.com\",\"server_port\":22,\"username\":\"u\",\"password\":\"p\"}".to_string().into(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("ssh with creds works");
        store
            .add(NewConfig {
                name: "Noise".to_string(),
                protocol: "noise".to_string(),
                params: "{\"type\":\"noise\",\"server\":\"a.example.com\",\"server_port\":51820,\"key\":\"k\"}".to_string().into(),
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
                    params: Secret::new(params),
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

    #[test]
    fn t07_parse_only_rows_validate() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        for (protocol, params) in [
            (
                "mieru",
                "{\"type\":\"mieru\",\"server\":\"a.example.com\",\"server_port\":1080,\"username\":\"u\",\"password\":\"p\"}",
            ),
            (
                "juicity",
                "{\"type\":\"juicity\",\"server\":\"a.example.com\",\"server_port\":23142,\"uuid\":\"11111111-2222-4333-8444-555555555555\",\"password\":\"p\"}",
            ),
            (
                "trusttunnel",
                "{\"type\":\"trusttunnel\",\"server\":\"a.example.com\",\"server_port\":443,\"username\":\"u\",\"password\":\"p\"}",
            ),
        ] {
            store
                .add(NewConfig {
                    name: format!("{protocol} row"),
                    protocol: protocol.to_string(),
                    params: Secret::new(params),
                    group: String::new(),
                    bind_ip: String::new(),
                    port: String::new(),
                })
                .unwrap_or_else(|error| panic!("{protocol} with creds works: {error}"));
        }
        for (protocol, params, wants) in [
            (
                "mieru",
                "{\"type\":\"mieru\",\"server\":\"a.example.com\",\"server_port\":1080,\"username\":\"u\"}",
                "username and password",
            ),
            (
                "juicity",
                "{\"type\":\"juicity\",\"server\":\"a.example.com\",\"server_port\":23142,\"password\":\"p\"}",
                "uuid and password",
            ),
            (
                "trusttunnel",
                "{\"type\":\"trusttunnel\",\"server\":\"a.example.com\",\"server_port\":443,\"password\":\"p\"}",
                "username and password",
            ),
        ] {
            let error = store
                .add(NewConfig {
                    name: "Bad".to_string(),
                    protocol: protocol.to_string(),
                    params: Secret::new(params),
                    group: String::new(),
                    bind_ip: String::new(),
                    port: String::new(),
                })
                .expect_err(&format!("{protocol} without creds fails"));
            let text = error.to_string();
            assert!(text.contains(wants), "names {wants}, got {text}");
        }
    }

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

    #[test]
    fn ticket35_dpapi_tamper_fails_loud() {
        let mut sealed = crate::dpapi::protect(b"hello").expect("protects");
        let last = sealed.len() - 1;
        sealed[last] ^= 0x01;
        let error = crate::dpapi::unprotect(&sealed).expect_err("tamper must fail");
        assert!(!error.contains("hello"), "no secret echo: {error}");
    }

    #[test]
    fn ticket35_stored_params_have_no_plaintext() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let created = store
            .add(NewConfig {
                name: "Sealed".to_string(),
                protocol: "shadowsocks".to_string(),
                params: r#"{"server":"198.51.100.9","server_port":8388,"method":"aes-128-gcm","password":"SECRET-PASSWORD"}"#.to_string().into(),
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
                    params: r#"{"server":"198.51.100.9","server_port":8388,"method":"aes-128-gcm","password":"LEGACY-SECRET"}"#.to_string().into(),
                    group: String::new(),
                    bind_ip: String::new(),
                    port: String::new(),
                })
                .expect("add works");

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

    #[test]
    fn ticket36_traffic_accumulates_per_row() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let created = store
            .add(NewConfig {
                name: "Flow".to_string(),
                protocol: "shadowsocks".to_string(),
                params: r#"{"server":"198.51.100.9","server_port":8388,"method":"aes-128-gcm","password":"p"}"#.to_string().into(),
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

    #[test]
    fn ticket31_unknown_protocol_names_valid_protocols() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let error = store
            .add(NewConfig {
                name: "Typo".to_string(),
                protocol: "vles".to_string(),
                params: Secret::default(),
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

        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let params = Secret::new(
            protocol_params_template("vless")
                .replace("\"server\":\"\"", "\"server\":\"a.example.com\""),
        );
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

        let fallback = protocol_params_template("bogus");
        assert!(
            fallback.contains("server"),
            "fallback still guides, got {fallback}"
        );
    }

    #[test]
    fn review_virtual_templates_commit_and_brutal_gate() {
        for virtual_proto in [
            "chain",
            "custom-outbound",
            "custom-config",
            "extra-core",
            "direct",
        ] {
            let template = protocol_params_template(virtual_proto);
            let value: serde_json::Value =
                serde_json::from_str(&template).expect("virtual template is JSON");
            assert!(
                value.get("server").is_none() && value.get("server_port").is_none(),
                "{virtual_proto} starter is bare: {template}"
            );
            assert!(
                !is_import_shaped_params(&template),
                "{virtual_proto} starter skips the server gate"
            );
        }
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        for virtual_proto in ["chain", "custom-config", "direct"] {
            store
                .add(NewConfig {
                    name: format!("{virtual_proto} row"),
                    protocol: virtual_proto.to_string(),
                    params: protocol_params_template(virtual_proto).into(),
                    group: String::new(),
                    bind_ip: String::new(),
                    port: String::new(),
                })
                .unwrap_or_else(|error| panic!("bare {virtual_proto} starter commits: {error}"));
        }

        let bad = "{\"type\":\"vless\",\"server\":\"s.example.com\",\"server_port\":443,\
            \"uuid\":\"11111111-2222-4333-8444-555555555555\",\
            \"brutal_down_mbps\":\"fast\"}";
        let error = store
            .add(NewConfig {
                name: "Bad brutal".to_string(),
                protocol: "vless".to_string(),
                params: Secret::new(bad),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect_err("garbage brutal speed must fail");
        assert!(
            error.to_string().contains("brutal_down_mbps"),
            "names the field, got {error}"
        );
        let numeric = bad.replace("\"fast\"", "100");
        store
            .add(NewConfig {
                name: "Numeric brutal".to_string(),
                protocol: "vless".to_string(),
                params: numeric.into(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("numeric brutal speed commits");
    }

    #[test]
    fn t01_picker_virtual_and_pristine_rules() {
        assert_eq!(
            VALID_PROTOCOLS.len(),
            27,
            "picker lists exactly the canonical types"
        );
        for virtual_proto in [
            "chain",
            "custom-outbound",
            "custom-config",
            "extra-core",
            "direct",
        ] {
            assert!(
                is_virtual_protocol(virtual_proto),
                "{virtual_proto} hides Server/Port"
            );
        }
        for endpoint_proto in [
            "vless",
            "vmess",
            "trojan",
            "shadowsocks",
            "socks",
            "wireguard",
            "xray",
        ] {
            assert!(
                !is_virtual_protocol(endpoint_proto),
                "{endpoint_proto} keeps Server/Port"
            );
        }
        assert!(!is_virtual_protocol("bogus"), "unknown is not virtual");

        let vless_starter = protocol_params_template("vless");
        assert!(is_pristine_template(&vless_starter, "vless"));
        assert!(!is_pristine_template(&format!("{vless_starter} "), "vless"));
        assert!(!is_pristine_template(&vless_starter, "chain"));

        assert_eq!(
            pristine_template_rewrite(&vless_starter, "vless", "chain"),
            protocol_params_template("chain"),
            "pristine switch rewrites"
        );

        let dirty = vless_starter.replace("\"server\":\"\"", "\"server\":\"a.example.com\"");
        assert_eq!(
            pristine_template_rewrite(&dirty, "vless", "chain"),
            dirty,
            "dirty switch preserves"
        );
        let hand_typed = "{\"type\":\"vless\",\"server\":\"b.example.com\"}";
        assert_eq!(
            pristine_template_rewrite(hand_typed, "vless", "socks"),
            hand_typed,
            "hand-typed JSON stands"
        );
        assert_eq!(
            pristine_template_rewrite("{{{", "vless", "socks"),
            "{{{",
            "garbage stands"
        );
    }

    #[test]
    fn t08_direct_template_saves_with_no_fields() {
        assert_eq!(
            protocol_params_template("direct"),
            "{\"type\":\"direct\"}",
            "direct starter is the bare marker"
        );
        let guided = guided_identity_fields("direct").expect("direct has a guided row");
        assert!(guided.key.is_none(), "direct takes no secret");
        assert_eq!(guided.needs, "no secret");
        assert!(is_virtual_protocol("direct"), "direct hides Server/Port");
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let created = store
            .add(NewConfig {
                name: "Direct".to_string(),
                protocol: "direct".to_string(),
                params: protocol_params_template("direct").into(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("bare direct row saves");
        assert_eq!(created.protocol, "direct");
    }

    #[test]
    fn t02_panel_folds_noop_untouched_and_write_minimal() {
        let shaped = "{\"type\":\"vless\",\"server\":\"a.example.com\",\"server_port\":443,\
            \"uuid\":\"11111111-2222-4333-8444-555555555555\",\"network\":\"tcp\"}";
        let echoed = TransportFold {
            network: "tcp".to_string(),
            path: String::new(),
            host: String::new(),
            method: String::new(),
            service_name: String::new(),
        };
        assert_eq!(fold_transport_into_params(shaped, &echoed), shaped);
        let ws = fold_transport_into_params(
            shaped,
            &TransportFold {
                network: "ws".to_string(),
                path: "/w".to_string(),
                host: "h.example.com".to_string(),
                ..Default::default()
            },
        );
        let ws_value: serde_json::Value = serde_json::from_str(&ws).expect("ws folds to JSON");
        assert_eq!(ws_value["network"], "ws");
        assert_eq!(ws_value["path"], "/w");
        assert_eq!(ws_value["host"], "h.example.com");
        let back = fold_transport_into_params(
            &ws,
            &TransportFold {
                network: "tcp".to_string(),
                ..Default::default()
            },
        );
        let back_value: serde_json::Value = serde_json::from_str(&back).expect("back folds");
        assert!(back_value.get("network").is_none(), "tcp restores default");
        assert_eq!(back_value["path"], "/w", "conditional values preserved");
        assert_eq!(back_value["server"], "a.example.com", "row untouched");

        let bare = "{\"type\":\"vless\",\"server\":\"a.example.com\",\"server_port\":443}";
        let on = fold_tls_optional_into_params(
            bare,
            &TlsOptionalFold {
                enable: true,
                sni: "s.example.com".to_string(),
                ..Default::default()
            },
        );
        let on_value: serde_json::Value = serde_json::from_str(&on).expect("tls on folds");
        assert_eq!(on_value["security"], "tls");
        assert_eq!(on_value["sni"], "s.example.com");
        let reality = fold_tls_optional_into_params(
            &on,
            &TlsOptionalFold {
                enable: true,
                sni: "s.example.com".to_string(),
                pbk: "pubkey".to_string(),
                sid: "short".to_string(),
                ..Default::default()
            },
        );
        let reality_value: serde_json::Value =
            serde_json::from_str(&reality).expect("reality folds");
        assert_eq!(
            reality_value["security"], "reality",
            "non-empty pbk means REALITY"
        );
        let off = fold_tls_optional_into_params(
            &reality,
            &TlsOptionalFold {
                enable: false,
                ..Default::default()
            },
        );
        let off_value: serde_json::Value = serde_json::from_str(&off).expect("tls off folds");
        assert!(
            off_value.get("security").is_none(),
            "disable drops security"
        );
        assert_eq!(off_value["sni"], "s.example.com", "sni preserved");
        assert_eq!(off_value["pbk"], "pubkey", "pbk preserved");

        let aliased = "{\"security\":\"tls\",\"allowInsecure\":true}";
        let kept = fold_tls_optional_into_params(
            aliased,
            &TlsOptionalFold {
                enable: true,
                insecure: true,
                ..Default::default()
            },
        );
        assert_eq!(kept, aliased, "aliased true no-ops");
        let cleared = fold_tls_optional_into_params(
            aliased,
            &TlsOptionalFold {
                enable: true,
                insecure: false,
                ..Default::default()
            },
        );
        let cleared_value: serde_json::Value =
            serde_json::from_str(&cleared).expect("insecure off folds");
        assert!(cleared_value.get("allowInsecure").is_none());
        assert!(cleared_value.get("insecure").is_none());

        let mux_on = fold_mux_into_params(
            bare,
            &MuxFold {
                mode: "on".to_string(),
                brutal_enable: true,
                brutal_up_mbps: "100".to_string(),
                brutal_down_mbps: "fast".to_string(),
            },
        );
        let mux_value: serde_json::Value = serde_json::from_str(&mux_on).expect("mux folds");
        assert_eq!(mux_value["mux"], "on");
        assert_eq!(mux_value["brutal_enable"], true);
        assert_eq!(mux_value["brutal_up_mbps"], 100);
        assert_eq!(mux_value["brutal_down_mbps"], "fast");
        let mux_default = fold_mux_into_params(
            &mux_on,
            &MuxFold {
                mode: String::new(),
                brutal_enable: true,
                brutal_up_mbps: "100".to_string(),
                brutal_down_mbps: "fast".to_string(),
            },
        );
        let default_value: serde_json::Value =
            serde_json::from_str(&mux_default).expect("mux default folds");
        assert!(
            default_value.get("mux").is_none(),
            "default removes the key"
        );
        assert_eq!(default_value["brutal_enable"], true, "brutal preserved");

        let dialed = fold_dial_into_params(
            bare,
            &DialFold {
                reuse_addr: true,
                bind_interface: "eth0".to_string(),
                ..Default::default()
            },
        );
        let dial_value: serde_json::Value = serde_json::from_str(&dialed).expect("dial folds");
        assert_eq!(dial_value["reuse_addr"], true);
        assert_eq!(dial_value["bind_interface"], "eth0");
        let dial_echo = DialFold {
            reuse_addr: true,
            bind_interface: "eth0".to_string(),
            ..Default::default()
        };
        assert_eq!(fold_dial_into_params(&dialed, &dial_echo), dialed);
        let dial_off = fold_dial_into_params(
            &dialed,
            &DialFold {
                bind_interface: "eth0".to_string(),
                ..Default::default()
            },
        );
        let dial_off_value: serde_json::Value =
            serde_json::from_str(&dial_off).expect("dial off folds");
        assert!(dial_off_value.get("reuse_addr").is_none(), "false clears");

        let inner = fold_vless_inner_into_params(
            bare,
            "11111111-2222-4333-8444-555555555555",
            "xtls-rprx-vision",
            "xudp",
        );
        let inner_value: serde_json::Value = serde_json::from_str(&inner).expect("inner folds");
        assert_eq!(inner_value["uuid"], "11111111-2222-4333-8444-555555555555");
        assert_eq!(inner_value["flow"], "xtls-rprx-vision");
        assert_eq!(inner_value["packet_encoding"], "xudp");
        let inner_echo = fold_vless_inner_into_params(
            &inner,
            "11111111-2222-4333-8444-555555555555",
            "xtls-rprx-vision",
            "xudp",
        );
        assert_eq!(inner_echo, inner, "echoed inner folds byte-identical");
        let inner_plain =
            fold_vless_inner_into_params(&inner, "11111111-2222-4333-8444-555555555555", "", "");
        let plain_value: serde_json::Value =
            serde_json::from_str(&inner_plain).expect("inner plain folds");
        assert!(
            plain_value.get("flow").is_none(),
            "blank flow restores default"
        );
        assert!(plain_value.get("packet_encoding").is_none());

        for fold_it in [
            fold_transport_into_params("{{{", &TransportFold::default()),
            fold_mux_into_params("{{{", &MuxFold::default()),
            fold_dial_into_params("{{{", &DialFold::default()),
        ] {
            assert_eq!(fold_it, "{{{");
        }
    }

    #[test]
    fn t03_vmess_inner_fold_keeps_grammar() {
        let uuid = "11111111-2222-4333-8444-555555555555";

        let legacy = format!(
            "{{\"type\":\"vmess\",\"server\":\"a.example.com\",\"server_port\":443,\
            \"uuid\":\"{uuid}\",\"alter_id\":\"0\",\"security\":\"auto\",\"tls\":true}}"
        );
        assert!(vmess_has_legacy_tls(&legacy));
        assert!(!vmess_has_legacy_tls("{\"security\":\"tls\"}"));
        assert_eq!(vmess_cipher_value(&legacy), "auto", "legacy cipher reads");
        assert!(vmess_tls_enabled(&legacy), "legacy boolean enables");
        assert_eq!(
            fold_vmess_inner_into_params(&legacy, uuid, "0", "auto", ""),
            legacy,
            "untouched legacy echoes byte-identical"
        );

        let switched = fold_vmess_inner_into_params(&legacy, uuid, "0", "chacha20-poly1305", "");
        let switched_value: serde_json::Value =
            serde_json::from_str(&switched).expect("switched folds");
        assert_eq!(switched_value["security"], "chacha20-poly1305");
        assert!(
            switched_value.get("encryption").is_none(),
            "no new-style key on legacy rows"
        );
        assert_eq!(switched_value["tls"], true, "boolean untouched");
        assert_eq!(switched_value["alter_id"], "0", "aid stays a string");

        let modern = format!(
            "{{\"type\":\"vmess\",\"server\":\"a.example.com\",\"server_port\":443,\
            \"uuid\":\"{uuid}\",\"alter_id\":\"2\",\"security\":\"reality\",\
            \"encryption\":\"aes-128-gcm\",\"pbk\":\"k\"}}"
        );
        assert!(!vmess_has_legacy_tls(&modern));
        assert_eq!(vmess_cipher_value(&modern), "aes-128-gcm");
        assert!(vmess_tls_enabled(&modern));
        assert_eq!(
            fold_vmess_inner_into_params(&modern, uuid, "2", "aes-128-gcm", ""),
            modern,
            "untouched new-style echoes byte-identical"
        );

        assert_eq!(vmess_cipher_value("{\"security\":\"tls\"}"), "");
        assert_eq!(vmess_cipher_value("{\"security\":\"reality\"}"), "");
        assert!(vmess_tls_enabled("{\"security\":\"tls\"}"));
        assert!(!vmess_tls_enabled("{\"security\":\"auto\"}"));
        assert!(!vmess_tls_enabled("{}"));

        let ambiguous = format!(
            "{{\"type\":\"vmess\",\"server\":\"a.example.com\",\"server_port\":443,\
            \"uuid\":\"{uuid}\",\"security\":\"auto\"}}"
        );
        assert_eq!(vmess_cipher_value(&ambiguous), "auto");
        let staged = fold_vmess_inner_into_params(&ambiguous, uuid, "", "auto", "xudp");
        let staged_value: serde_json::Value = serde_json::from_str(&staged).expect("staged folds");
        assert_eq!(staged_value["packet_encoding"], "xudp");
        let healed = fold_tls_vmess_into_params(
            &staged,
            &TlsOptionalFold {
                enable: false,
                ..Default::default()
            },
        );
        let healed_value: serde_json::Value = serde_json::from_str(&healed).expect("healed folds");
        assert!(
            healed_value.get("security").is_none(),
            "ambiguous tls-ness cleared"
        );
        assert!(
            healed_value.get("encryption").is_none(),
            "default cipher stays absent"
        );
        assert_eq!(healed_value["packet_encoding"], "xudp");

        for fold_it in [
            fold_vmess_inner_into_params("{{{", uuid, "0", "auto", ""),
            fold_trojan_inner_into_params("{{{", "secret"),
            fold_shadowsocks_inner_into_params("{{{", "aes-128-gcm", "pw", "", "", false),
            fold_tls_vmess_into_params("{{{", &TlsOptionalFold::default()),
        ] {
            assert_eq!(fold_it, "{{{");
        }
    }

    #[test]
    fn t03_tls_vmess_fold_mirrors_bool_and_migrates_reality() {
        let uuid = "11111111-2222-4333-8444-555555555555";
        let legacy = format!(
            "{{\"type\":\"vmess\",\"server\":\"a.example.com\",\"server_port\":443,\
            \"uuid\":\"{uuid}\",\"alter_id\":\"0\",\"security\":\"auto\",\"tls\":true}}"
        );

        let echo = fold_tls_vmess_into_params(
            &legacy,
            &TlsOptionalFold {
                enable: true,
                ..Default::default()
            },
        );
        assert_eq!(echo, legacy, "untouched legacy echoes byte-identical");

        let off = fold_tls_vmess_into_params(
            &legacy,
            &TlsOptionalFold {
                enable: false,
                ..Default::default()
            },
        );
        let off_value: serde_json::Value = serde_json::from_str(&off).expect("off folds");
        assert_eq!(off_value["tls"], false, "boolean mirrors the toggle");
        assert_eq!(off_value["security"], "auto", "cipher preserved");

        let migrated = fold_tls_vmess_into_params(
            &legacy,
            &TlsOptionalFold {
                enable: true,
                sni: "a.example.com".to_string(),
                pbk: "pubkey".to_string(),
                sid: "short".to_string(),
                ..Default::default()
            },
        );
        let migrated_value: serde_json::Value =
            serde_json::from_str(&migrated).expect("migrated folds");
        assert!(
            migrated_value.get("tls").is_none(),
            "boolean dropped on migration"
        );
        assert_eq!(migrated_value["encryption"], "auto", "cipher moved");
        assert_eq!(migrated_value["security"], "reality");
        assert_eq!(migrated_value["sni"], "a.example.com");
        assert_eq!(migrated_value["pbk"], "pubkey");

        let twice = fold_tls_vmess_into_params(
            &migrated,
            &TlsOptionalFold {
                enable: true,
                sni: "a.example.com".to_string(),
                pbk: "pubkey".to_string(),
                sid: "short".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(twice, migrated, "migration is idempotent");

        let modern = "{\"type\":\"vmess\",\"server\":\"a.example.com\",\"server_port\":443}";
        for fold in [
            TlsOptionalFold {
                enable: true,
                sni: "s.example.com".to_string(),
                ..Default::default()
            },
            TlsOptionalFold {
                enable: false,
                ..Default::default()
            },
            TlsOptionalFold {
                enable: true,
                pbk: "pubkey".to_string(),
                insecure: true,
                ..Default::default()
            },
        ] {
            assert_eq!(
                fold_tls_vmess_into_params(modern, &fold),
                fold_tls_optional_into_params(modern, &fold),
                "non-legacy matches the standard fold"
            );
        }
    }

    #[test]
    fn t03_ss_and_trojan_inner_folds() {
        let trojan = "{\"type\":\"trojan\",\"server\":\"a.example.com\",\"server_port\":443,\
            \"password\":\"secret\"}";
        assert_eq!(
            fold_trojan_inner_into_params(trojan, "secret"),
            trojan,
            "untouched password echoes"
        );
        let rotated = fold_trojan_inner_into_params(trojan, "rotated");
        let rotated_value: serde_json::Value =
            serde_json::from_str(&rotated).expect("rotated folds");
        assert_eq!(rotated_value["password"], "rotated");
        assert_eq!(
            fold_trojan_inner_into_params(trojan, ""),
            trojan,
            "blank never wipes"
        );
        assert_eq!(
            fold_trojan_inner_into_params(trojan, "  "),
            trojan,
            "whitespace never wipes"
        );

        let template = protocol_params_template("shadowsocks");
        assert_eq!(
            fold_shadowsocks_inner_into_params(&template, "", "", "", "", false),
            template,
            "pristine template stands"
        );

        let ss = "{\"type\":\"shadowsocks\",\"server\":\"a.example.com\",\"server_port\":8388,\
            \"method\":\"aes-128-gcm\",\"password\":\"pw\",\"udp_over_tcp\":true}";
        assert_eq!(
            fold_shadowsocks_inner_into_params(ss, "aes-128-gcm", "pw", "", "", true),
            ss,
            "untouched ss echoes byte-identical"
        );

        let uot_legacy = "{\"type\":\"shadowsocks\",\"server\":\"a.example.com\",\
            \"server_port\":8388,\"method\":\"aes-128-gcm\",\"password\":\"pw\",\"uot\":true}";
        assert_eq!(
            fold_shadowsocks_inner_into_params(uot_legacy, "aes-128-gcm", "pw", "", "", true),
            uot_legacy,
            "matching uot echoes (engine folds the spelling)"
        );
        let uot_off = "{\"type\":\"shadowsocks\",\"server\":\"a.example.com\",\
            \"server_port\":8388,\"method\":\"aes-128-gcm\",\"password\":\"pw\",\"uot\":false}";
        let normalized = fold_shadowsocks_inner_into_params(
            uot_off,
            "chacha20-ietf-poly1305",
            "pw",
            "obfs-local",
            "obfs=http",
            true,
        );
        let normalized_value: serde_json::Value =
            serde_json::from_str(&normalized).expect("normalized folds");
        assert_eq!(normalized_value["method"], "chacha20-ietf-poly1305");
        assert_eq!(normalized_value["plugin"], "obfs-local");
        assert_eq!(normalized_value["plugin_opts"], "obfs=http");
        assert_eq!(normalized_value["udp_over_tcp"], true);
        assert!(
            normalized_value.get("uot").is_none(),
            "alias retired on write"
        );

        let tailed = "{\"type\":\"shadowsocks\",\"server\":\"a.example.com\",\
            \"server_port\":8388,\"method\":\"aes-128-gcm\",\"password\":\"pw\",\
            \"plugin\":\"obfs-local;obfs=http\"}";
        assert_eq!(
            fold_shadowsocks_inner_into_params(tailed, "aes-128-gcm", "pw", "", "", false),
            tailed,
            "unrepresentable plugin preserved"
        );

        let cleared = fold_shadowsocks_inner_into_params(ss, "aes-128-gcm", "pw", "", "", false);
        let cleared_value: serde_json::Value =
            serde_json::from_str(&cleared).expect("cleared folds");
        assert!(cleared_value.get("udp_over_tcp").is_none(), "false clears");
        assert_eq!(cleared_value["method"], "aes-128-gcm", "method kept");
    }

    #[test]
    fn t04_wireguard_inner_fold() {
        let full = WireguardFold {
            private_key: "LOCAL".to_string(),
            public_key: "REMOTE".to_string(),
            pre_shared_key: "PSK".to_string(),
            reserved: "1,2,3".to_string(),
            persistent_keepalive: "25".to_string(),
            local_addr: "10.0.0.2/32".to_string(),
            dns: "1.1.1.1".to_string(),
            mtu: "1420".to_string(),
            workers: "4".to_string(),
            sys_ifc: true,
        };
        let bare = "{\"type\":\"wireguard\",\"server\":\"192.0.2.44\",\"server_port\":51820,\
            \"private_key\":\"\",\"public_key\":\"\"}";
        let folded = fold_wireguard_into_params(bare, &full);
        let folded_value: serde_json::Value = serde_json::from_str(&folded).expect("wg folds");
        assert_eq!(folded_value["private_key"], "LOCAL");
        assert_eq!(folded_value["public_key"], "REMOTE");
        assert_eq!(folded_value["pre_shared_key"], "PSK");
        assert!(
            folded_value.get("preshared_key").is_none(),
            "Throne spelling never written"
        );
        assert_eq!(folded_value["reserved"], "1,2,3", "comma ints stay text");
        assert_eq!(folded_value["persistent_keepalive"], "25");
        assert_eq!(
            folded_value["address"], "10.0.0.2/32",
            "local_addr stores as address"
        );
        assert!(
            folded_value.get("local_addr").is_none(),
            "Throne spelling never written"
        );
        assert_eq!(folded_value["dns"], "1.1.1.1");
        assert_eq!(
            folded_value["mtu"], "1420",
            "mtu stays text (engine coerces)"
        );
        assert_eq!(folded_value["workers"], "4");
        assert_eq!(folded_value["system"], true, "sys_ifc stores as system");
        assert!(
            folded_value.get("sys_ifc").is_none(),
            "Throne spelling never written"
        );

        let echo = WireguardFold {
            private_key: "LOCAL".to_string(),
            public_key: "REMOTE".to_string(),
            pre_shared_key: "PSK".to_string(),
            reserved: "1,2,3".to_string(),
            persistent_keepalive: "25".to_string(),
            local_addr: "10.0.0.2/32".to_string(),
            dns: "1.1.1.1".to_string(),
            mtu: "1420".to_string(),
            workers: "4".to_string(),
            sys_ifc: true,
        };
        assert_eq!(
            fold_wireguard_into_params(&folded, &echo),
            folded,
            "untouched wg echoes byte-identical"
        );

        let throne = "{\"type\":\"wireguard\",\"server\":\"192.0.2.44\",\"server_port\":51820,\
            \"private_key\":\"LOCAL\",\"public_key\":\"REMOTE\",\"preshared_key\":\"PSK\",\
            \"local_addr\":\"10.0.0.2/32\",\"sys_ifc\":true}";
        let throne_fold = WireguardFold {
            private_key: "LOCAL".to_string(),
            public_key: "REMOTE".to_string(),
            pre_shared_key: "PSK".to_string(),
            local_addr: "10.0.0.2/32".to_string(),
            sys_ifc: true,
            ..Default::default()
        };
        assert_eq!(
            fold_wireguard_into_params(throne, &throne_fold),
            throne,
            "matching aliases echo (engine-adjacent spellings retire only on write)"
        );

        let template = protocol_params_template("wireguard");
        assert_eq!(
            fold_wireguard_into_params(&template, &WireguardFold::default()),
            template,
            "pristine template stands"
        );

        let imported = "{\"type\":\"wireguard\",\"server\":\"192.0.2.44\",\"server_port\":51820,\
            \"private_key\":\"LOCAL\",\"public_key\":\"REMOTE\",\"udp_timeout\":\"30\",\
            \"allowed_ips\":\"10.0.0.0/8\",\"fwmark\":\"100\",\"keepalive\":\"25\"}";
        let kept = fold_wireguard_into_params(
            imported,
            &WireguardFold {
                private_key: "LOCAL".to_string(),
                public_key: "REMOTE".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(kept, imported, "import-only keys preserved");
        let kept_value: serde_json::Value = serde_json::from_str(&kept).expect("kept parses");
        assert_eq!(kept_value["udp_timeout"], "30");

        let sys_off = fold_wireguard_into_params(
            &folded,
            &WireguardFold {
                sys_ifc: false,
                ..Default::default()
            },
        );

        let sys_off_value: serde_json::Value =
            serde_json::from_str(&sys_off).expect("sys off folds");
        assert!(sys_off_value.get("system").is_none(), "false clears");
        assert_eq!(
            sys_off_value["private_key"], "LOCAL",
            "blank texts never wipe"
        );
        assert_eq!(fold_wireguard_into_params("{{{", &full), "{{{");
    }

    #[test]
    fn t04_amneziawg_matrix_fold() {
        assert_eq!(AWG_VERSIONS, ["2.0", "3.x"]);
        let matrix = AmneziaFold {
            enable_amnezia: true,
            version: "3.x".to_string(),
            jc: "5".to_string(),
            jmin: "30".to_string(),
            jmax: "1000".to_string(),
            s1: "20".to_string(),
            h1: "1-100".to_string(),
            i1: "<b 0xd100><r 50>".to_string(),
            header_protection_key: "HPK".to_string(),
            content_padding_addition: "PAD".to_string(),
            rekey_after_time: "120".to_string(),
            rekey_timeout: "30".to_string(),
            reject_after_time: "180".to_string(),
            keepalive_timeout: "60".to_string(),
            max_handshake_attempts: "3".to_string(),
            random_trailers: true,
            disable_cookies: true,
            ..Default::default()
        };
        let bare = "{\"type\":\"amneziawg\",\"server\":\"192.0.2.90\",\"server_port\":51820,\
            \"private_key\":\"LOCAL\",\"public_key\":\"REMOTE\"}";
        let folded = fold_amneziawg_into_params(bare, &matrix);
        let folded_value: serde_json::Value = serde_json::from_str(&folded).expect("awg folds");
        assert_eq!(folded_value["enable_amnezia"], true);
        assert_eq!(folded_value["awg_version"], "3.x");
        assert_eq!(folded_value["jc"], "5", "junk stays text (engine coerces)");
        assert_eq!(folded_value["jmin"], "30");
        assert_eq!(folded_value["jmax"], "1000");
        assert_eq!(folded_value["s1"], "20");
        assert_eq!(folded_value["h1"], "1-100", "h-range verbatim");
        assert_eq!(folded_value["i1"], "<b 0xd100><r 50>", "i-chain verbatim");
        assert_eq!(folded_value["headerprotectionkey"], "HPK");
        assert_eq!(folded_value["contentpaddingaddition"], "PAD");
        assert_eq!(folded_value["rekeyaftertime"], "120");
        assert_eq!(folded_value["maxhandshakeattempts"], "3");
        assert_eq!(folded_value["randomtrailers"], true);
        assert_eq!(folded_value["disablecookies"], true);
        assert_eq!(
            fold_amneziawg_into_params(&folded, &matrix),
            folded,
            "untouched matrix echoes byte-identical"
        );

        let noversion = fold_amneziawg_into_params(bare, &AmneziaFold::default());
        let noversion_value: serde_json::Value =
            serde_json::from_str(&noversion).expect("no-version folds");
        assert!(
            noversion_value.get("awg_version").is_none(),
            "blank never fabricates"
        );
        let keeper = AmneziaFold {
            enable_amnezia: true,
            version: "3.x".to_string(),
            random_trailers: true,
            disable_cookies: true,
            ..Default::default()
        };
        assert_eq!(
            fold_amneziawg_into_params(&folded, &keeper),
            folded,
            "echoed toggles+version keep, blank texts never strip"
        );

        let v2row = "{\"type\":\"amneziawg\",\"server\":\"192.0.2.90\",\"server_port\":51820,\
            \"private_key\":\"LOCAL\",\"public_key\":\"REMOTE\",\"awg_version\":\"2.0\",\
            \"headerprotectionkey\":\"HPK\"}";
        assert_eq!(
            fold_amneziawg_into_params(v2row, &AmneziaFold::default()),
            v2row,
            "hidden-version values preserved, never wiped"
        );

        let off = fold_amneziawg_into_params(
            &folded,
            &AmneziaFold {
                version: "3.x".to_string(),
                ..Default::default()
            },
        );
        let off_value: serde_json::Value = serde_json::from_str(&off).expect("off folds");
        assert!(off_value.get("enable_amnezia").is_none(), "marker clears");
        assert!(off_value.get("randomtrailers").is_none(), "trailers clear");
        assert!(off_value.get("disablecookies").is_none(), "cookies clear");
        assert_eq!(off_value["awg_version"], "3.x", "version kept");
        assert_eq!(fold_amneziawg_into_params("{{{", &matrix), "{{{");
    }

    #[test]
    fn t05_hysteria_fold_common_and_version_halves() {
        assert_eq!(HY_AUTH_TYPES, ["STRING", "BASE64"]);
        let v1 = HysteriaFold {
            server_ports: "9000-9010".to_string(),
            hop_interval: "30s".to_string(),
            up: "100".to_string(),
            down: "50 Mbps".to_string(),
            obfs: "obfspw".to_string(),
            auth_type: "STRING".to_string(),
            auth: "secret".to_string(),
            recv_window_conn: "1024".to_string(),
            recv_window: "2048".to_string(),
            disable_mtu_discovery: true,
            password: "NEVER-FOLDS-ON-V1".to_string(),
        };
        let bare_v1 = "{\"type\":\"hysteria\",\"server\":\"192.0.2.14\",\"server_port\":443,\
            \"auth\":\"\"}";
        let folded = fold_hysteria_into_params(bare_v1, &v1, false);
        let value: serde_json::Value = serde_json::from_str(&folded).expect("hy folds");
        assert_eq!(value["server_ports"], "9000-9010");
        assert_eq!(value["hop_interval"], "30s");
        assert_eq!(value["up"], "100", "speeds fold as up/down flats");
        assert_eq!(value["down"], "50 Mbps");
        assert!(
            value.get("up_mbps").is_none() && value.get("down_mbps").is_none(),
            "Throne spellings never written"
        );
        assert_eq!(value["obfs"], "obfspw");
        assert_eq!(value["auth_type"], "STRING");
        assert_eq!(value["auth"], "secret");
        assert_eq!(
            value["recv_window_conn"], 1024,
            "recv windows land as numbers"
        );
        assert_eq!(value["recv_window"], 2048);
        assert_eq!(value["disable_mtu_discovery"], true);
        assert!(
            value.get("password").is_none(),
            "v1 never folds the v2 credential"
        );

        let echo = HysteriaFold {
            server_ports: "9000-9010".to_string(),
            hop_interval: "30s".to_string(),
            up: "100".to_string(),
            down: "50 Mbps".to_string(),
            obfs: "obfspw".to_string(),
            auth_type: "STRING".to_string(),
            auth: "secret".to_string(),
            recv_window_conn: "1024".to_string(),
            recv_window: "2048".to_string(),
            disable_mtu_discovery: true,
            ..Default::default()
        };
        assert_eq!(
            fold_hysteria_into_params(&folded, &echo, false),
            folded,
            "untouched v1 echoes byte-identical"
        );

        let v2 = HysteriaFold {
            up: "100".to_string(),
            down: "100".to_string(),
            auth: "NEVER-FOLDS-ON-V2".to_string(),
            password: "v2secret".to_string(),
            ..Default::default()
        };
        let bare_v2 = "{\"type\":\"hysteria2\",\"server\":\"192.0.2.13\",\"server_port\":443,\
            \"password\":\"\"}";
        let folded2 = fold_hysteria_into_params(bare_v2, &v2, true);
        let value2: serde_json::Value = serde_json::from_str(&folded2).expect("hy2 folds");
        assert_eq!(value2["password"], "v2secret");
        assert_eq!(value2["up"], "100");
        assert!(value2.get("auth").is_none(), "v2 never folds the v1 auth");

        let throne = "{\"type\":\"hysteria\",\"server\":\"192.0.2.14\",\"server_port\":443,\
            \"auth\":\"s\",\"up_mbps\":100,\"down_mbps\":50}";
        let throne_echo = HysteriaFold {
            auth: "s".to_string(),
            up: "100".to_string(),
            down: "50".to_string(),
            ..Default::default()
        };
        assert_eq!(
            fold_hysteria_into_params(throne, &throne_echo, false),
            throne,
            "matching aliases echo"
        );

        for proto in ["hysteria", "hysteria2"] {
            let template = protocol_params_template(proto);
            assert_eq!(
                fold_hysteria_into_params(
                    &template,
                    &HysteriaFold::default(),
                    proto == "hysteria2"
                ),
                template,
                "{proto} pristine template stands"
            );
        }
        assert_eq!(fold_hysteria_into_params("{{{", &v1, false), "{{{");
    }

    #[test]
    fn t05_hysteria_v1_up_down_gate_names_fields() {
        fn add(
            store: &mut Store,
            name: &str,
            protocol: &str,
            params: &str,
        ) -> Result<String, String> {
            store
                .add(NewConfig {
                    name: name.to_string(),
                    protocol: protocol.to_string(),
                    params: Secret::new(params),
                    group: String::new(),
                    bind_ip: String::new(),
                    port: String::new(),
                })
                .map(|created| created.id)
                .map_err(|error| error.to_string())
        }
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let no_speeds = "{\"type\":\"hysteria\",\"server\":\"192.0.2.14\",\"server_port\":443,\
            \"auth\":\"secret\"}";
        let error = add(&mut store, "HyNoSpeed", "hysteria", no_speeds)
            .expect_err("v1 without speeds must fail");
        assert!(
            error.contains("up_mbps") && error.contains("down_mbps"),
            "names the page fields, got {error}"
        );
        let speeds = "{\"type\":\"hysteria\",\"server\":\"192.0.2.14\",\"server_port\":443,\
            \"auth\":\"secret\",\"up\":\"100\",\"down\":\"100\"}";
        add(&mut store, "HySpeed", "hysteria", speeds).expect("v1 with speeds lands");
        let bbr = "{\"type\":\"hysteria2\",\"server\":\"192.0.2.13\",\"server_port\":443,\
            \"password\":\"secret\"}";
        add(&mut store, "Hy2Bbr", "hysteria2", bbr).expect("v2 omits speeds for BBR");
        assert_eq!(
            store.list().expect("list works").len(),
            2,
            "the refused row wrote nothing"
        );
    }

    #[test]
    fn t05_tuic_fold_seven_fields_engine_spellings() {
        assert_eq!(TUIC_CC_MODES, ["bbr", "cubic", "new_reno"]);
        assert_eq!(TUIC_RELAY_MODES, ["native", "quic"]);
        let full = TuicFold {
            uuid: "11111111-2222-4333-8444-555555555555".to_string(),
            password: "secret".to_string(),
            congestion_control: "bbr".to_string(),
            udp_relay_mode: "quic".to_string(),
            udp_over_stream: true,
            zero_rtt_handshake: true,
            heartbeat: "10s".to_string(),
        };
        let bare = "{\"type\":\"tuic\",\"server\":\"192.0.2.12\",\"server_port\":443,\
            \"uuid\":\"\",\"password\":\"\"}";
        let folded = fold_tuic_into_params(bare, &full);
        let value: serde_json::Value = serde_json::from_str(&folded).expect("tuic folds");
        assert_eq!(value["uuid"], "11111111-2222-4333-8444-555555555555");
        assert_eq!(value["password"], "secret");
        assert_eq!(value["congestion_control"], "bbr");
        assert_eq!(value["udp_relay_mode"], "quic");
        assert_eq!(value["udp_over_stream"], true);
        assert_eq!(
            value["zero_rtt_handshake"], true,
            "sing-box spelling written"
        );
        assert!(
            value.get("zero_rtt").is_none(),
            "Throne spelling never written"
        );
        assert_eq!(value["heartbeat"], "10s");
        assert_eq!(
            fold_tuic_into_params(&folded, &full),
            folded,
            "untouched tuic echoes byte-identical"
        );

        let throne = "{\"type\":\"tuic\",\"server\":\"192.0.2.12\",\"server_port\":443,\
            \"uuid\":\"u\",\"password\":\"p\",\"zero_rtt\":true}";
        let throne_echo = TuicFold {
            uuid: "u".to_string(),
            password: "p".to_string(),
            zero_rtt_handshake: true,
            ..Default::default()
        };
        assert_eq!(
            fold_tuic_into_params(throne, &throne_echo),
            throne,
            "matching alias echoes"
        );

        let template = protocol_params_template("tuic");
        assert_eq!(
            fold_tuic_into_params(&template, &TuicFold::default()),
            template,
            "tuic pristine template stands"
        );
        assert_eq!(fold_tuic_into_params("{{{", &full), "{{{");
    }

    #[test]
    fn t05_tls_forced_fold_locks_only_when_touched() {
        let bare = "{\"type\":\"tuic\",\"server\":\"192.0.2.12\",\"server_port\":443,\
            \"uuid\":\"u\",\"password\":\"p\"}";
        assert_eq!(
            fold_tls_forced_into_params(bare, &TlsForcedFold::default()),
            bare,
            "untouched forced page stays byte-identical"
        );
        let sni = TlsForcedFold {
            sni: "example.com".to_string(),
            ..Default::default()
        };
        let sni_value: serde_json::Value =
            serde_json::from_str(&fold_tls_forced_into_params(bare, &sni)).expect("sni folds");
        assert_eq!(sni_value["sni"], "example.com");
        assert_eq!(sni_value["security"], "tls", "touched detail locks tls");
        let pbk = TlsForcedFold {
            pbk: "PUBKEY".to_string(),
            sid: "SID".to_string(),
            ..Default::default()
        };
        let pbk_value: serde_json::Value =
            serde_json::from_str(&fold_tls_forced_into_params(bare, &pbk)).expect("pbk folds");
        assert_eq!(pbk_value["security"], "reality", "public key locks reality");
        assert_eq!(pbk_value["pbk"], "PUBKEY");

        let tls_row =
            "{\"type\":\"tuic\",\"server\":\"s\",\"server_port\":443,\"security\":\"tls\"}";
        let kept = fold_tls_forced_into_params(
            tls_row,
            &TlsForcedFold {
                sni: "example.com".to_string(),
                ..Default::default()
            },
        );
        let kept_value: serde_json::Value = serde_json::from_str(&kept).expect("kept parses");
        assert_eq!(kept_value["security"], "tls");

        let none_row =
            "{\"type\":\"tuic\",\"server\":\"s\",\"server_port\":443,\"security\":\"none\"}";
        assert_eq!(
            fold_tls_forced_into_params(none_row, &TlsForcedFold::default()),
            none_row,
            "broken security never silently repaired"
        );
        assert_eq!(fold_tls_forced_into_params("{{{", &sni), "{{{");
    }

    #[test]
    fn t06_socks_fold_version_network_uot() {
        assert_eq!(SOCKS_VERSIONS, ["5", "4"]);
        let full = SocksFold {
            version: "5".to_string(),
            username: "user".to_string(),
            password: "secret".to_string(),
            network: "udp".to_string(),
            udp_over_tcp: true,
        };
        let bare = "{\"type\":\"socks\",\"server\":\"192.0.2.20\",\"server_port\":1080}";
        let folded = fold_socks_into_params(bare, &full);
        let value: serde_json::Value = serde_json::from_str(&folded).expect("socks folds");
        assert_eq!(value["version"], "5");
        assert_eq!(value["username"], "user");
        assert_eq!(value["password"], "secret");
        assert_eq!(value["network"], "udp");
        assert_eq!(value["udp_over_tcp"], true);
        assert_eq!(
            fold_socks_into_params(&folded, &full),
            folded,
            "untouched socks echoes byte-identical"
        );

        let v4 = "{\"type\":\"socks\",\"server\":\"s\",\"server_port\":1080,\"version\":\"4\"}";
        let v4_echo = SocksFold {
            version: "4".to_string(),
            ..Default::default()
        };
        assert_eq!(
            fold_socks_into_params(v4, &v4_echo),
            v4,
            "v4 echoes for the engine gate"
        );

        let throne = "{\"type\":\"socks\",\"server\":\"s\",\"server_port\":1080,\"uot\":true}";
        let throne_echo = SocksFold {
            udp_over_tcp: true,
            network: "tcp".to_string(),
            ..Default::default()
        };
        assert_eq!(
            fold_socks_into_params(throne, &throne_echo),
            throne,
            "matching alias echoes (tcp removes nothing absent)"
        );

        let template = protocol_params_template("socks");
        assert_eq!(
            fold_socks_into_params(&template, &SocksFold::default()),
            template,
            "socks pristine template stands"
        );
        assert_eq!(fold_socks_into_params("{{{", &full), "{{{");
    }

    #[test]
    fn t06_http_fold_parity_plus() {
        let full = HttpFold {
            username: "user".to_string(),
            password: "secret".to_string(),
            path: "/p".to_string(),
            headers: "Host: h.example.com".to_string(),
        };
        let bare = "{\"type\":\"http\",\"server\":\"192.0.2.21\",\"server_port\":8080}";
        let folded = fold_http_into_params(bare, &full);
        let value: serde_json::Value = serde_json::from_str(&folded).expect("http folds");
        assert_eq!(value["username"], "user");
        assert_eq!(value["password"], "secret");
        assert_eq!(value["path"], "/p");
        assert_eq!(value["headers"], "Host: h.example.com");
        assert_eq!(
            fold_http_into_params(&folded, &full),
            folded,
            "untouched http echoes byte-identical"
        );
        for proto in ["http", "https"] {
            let template = protocol_params_template(proto);
            assert_eq!(
                fold_http_into_params(&template, &HttpFold::default()),
                template,
                "{proto} pristine template stands"
            );
        }
        assert_eq!(fold_http_into_params("{{{", &full), "{{{");
    }

    #[test]
    fn t06_anytls_fold_interval_timeout_min() {
        let full = AnytlsFold {
            password: "secret".to_string(),
            interval: "30s".to_string(),
            timeout: "60s".to_string(),
            min: "5".to_string(),
        };
        let bare = "{\"type\":\"anytls\",\"server\":\"192.0.2.15\",\"server_port\":8443,\
            \"password\":\"\"}";
        let folded = fold_anytls_into_params(bare, &full);
        let value: serde_json::Value = serde_json::from_str(&folded).expect("anytls folds");
        assert_eq!(value["password"], "secret");
        assert_eq!(value["idle_session_check_interval"], "30s");
        assert_eq!(value["idle_session_timeout"], "60s");
        assert_eq!(value["min_idle_session"], 5, "min lands as a number");
        assert!(
            value.get("interval").is_none() && value.get("timeout").is_none(),
            "Throne spellings never written"
        );
        assert_eq!(
            fold_anytls_into_params(&folded, &full),
            folded,
            "untouched anytls echoes byte-identical"
        );

        let throne = "{\"type\":\"anytls\",\"server\":\"s\",\"server_port\":8443,\
            \"password\":\"p\",\"interval\":\"30s\",\"timeout\":\"60s\"}";
        let throne_echo = AnytlsFold {
            password: "p".to_string(),
            interval: "30s".to_string(),
            timeout: "60s".to_string(),
            ..Default::default()
        };
        assert_eq!(
            fold_anytls_into_params(throne, &throne_echo),
            throne,
            "matching aliases echo"
        );
        let template = protocol_params_template("anytls");
        assert_eq!(
            fold_anytls_into_params(&template, &AnytlsFold::default()),
            template,
            "anytls pristine template stands"
        );
        assert_eq!(fold_anytls_into_params("{{{", &full), "{{{");
    }

    #[test]
    fn t06_naive_fold_uot_quic_cc() {
        assert_eq!(NAIVE_CC_MODES, ["bbr", "bbr2", "cubic", "reno"]);
        let full = NaiveFold {
            username: "user".to_string(),
            password: "secret".to_string(),
            udp_over_tcp: true,
            quic: true,
            congestion_control: "bbr2".to_string(),
        };
        let bare = "{\"type\":\"naive\",\"server\":\"192.0.2.22\",\"server_port\":443,\
            \"username\":\"\",\"password\":\"\"}";
        let folded = fold_naive_into_params(bare, &full);
        let value: serde_json::Value = serde_json::from_str(&folded).expect("naive folds");
        assert_eq!(value["username"], "user");
        assert_eq!(value["password"], "secret");
        assert_eq!(value["udp_over_tcp"], true);
        assert_eq!(value["quic"], true);
        assert_eq!(
            value["quic_congestion_control"], "bbr2",
            "sing-box spelling written"
        );
        assert!(
            value.get("uot").is_none() && value.get("congestion_control").is_none(),
            "Throne spellings never written"
        );
        assert_eq!(
            fold_naive_into_params(&folded, &full),
            folded,
            "untouched naive echoes byte-identical"
        );

        let throne = "{\"type\":\"naive\",\"server\":\"s\",\"server_port\":443,\
            \"username\":\"u\",\"password\":\"p\",\"uot\":true,\"congestion_control\":\"cubic\"}";
        let throne_echo = NaiveFold {
            username: "u".to_string(),
            password: "p".to_string(),
            udp_over_tcp: true,
            congestion_control: "cubic".to_string(),
            ..Default::default()
        };
        assert_eq!(
            fold_naive_into_params(throne, &throne_echo),
            throne,
            "matching aliases echo"
        );

        let template = protocol_params_template("naive");
        assert_eq!(
            fold_naive_into_params(&template, &NaiveFold::default()),
            template,
            "naive pristine template stands"
        );
        assert_eq!(fold_naive_into_params("{{{", &full), "{{{");
    }

    #[test]
    fn t06_ssh_fold_user_key_hostlists() {
        let full = SshFold {
            user: "root".to_string(),
            password: "secret".to_string(),
            private_key: "KEY-BLOB".to_string(),
            private_key_path: "/home/u/.ssh/id_rsa".to_string(),
            private_key_passphrase: "pp".to_string(),
            host_key: "ecdsa-sha2-nistp256 AAAA, ssh-ed25519 BBBB".to_string(),
            host_key_algorithms: "ssh-ed25519".to_string(),
            client_version: "SSH-2.0-OpenSSH_7.4p1".to_string(),
        };
        let bare = "{\"type\":\"ssh\",\"server\":\"192.0.2.23\",\"server_port\":22,\
            \"username\":\"\",\"password\":\"\"}";
        let folded = fold_ssh_into_params(bare, &full);
        let value: serde_json::Value = serde_json::from_str(&folded).expect("ssh folds");
        assert_eq!(value["user"], "root", "sing-box spelling written");
        assert_eq!(value["password"], "secret");
        assert_eq!(value["private_key"], "KEY-BLOB");
        assert_eq!(value["private_key_path"], "/home/u/.ssh/id_rsa");
        assert_eq!(value["private_key_passphrase"], "pp");
        assert_eq!(
            value["host_key"],
            serde_json::json!(["ecdsa-sha2-nistp256 AAAA", "ssh-ed25519 BBBB"]),
            "host_key lands as an array"
        );
        assert_eq!(
            value["host_key_algorithms"],
            serde_json::json!(["ssh-ed25519"]),
            "algs land as an array"
        );
        assert_eq!(value["client_version"], "SSH-2.0-OpenSSH_7.4p1");
        assert!(
            value.get("username").is_some_and(|name| name == ""),
            "template username slot untouched (engine drops it when user wins)"
        );

        let echo = SshFold {
            user: "root".to_string(),
            password: "secret".to_string(),
            private_key: "KEY-BLOB".to_string(),
            private_key_path: "/home/u/.ssh/id_rsa".to_string(),
            private_key_passphrase: "pp".to_string(),
            host_key: "ecdsa-sha2-nistp256 AAAA, ssh-ed25519 BBBB".to_string(),
            host_key_algorithms: "ssh-ed25519".to_string(),
            client_version: "SSH-2.0-OpenSSH_7.4p1".to_string(),
        };
        assert_eq!(
            fold_ssh_into_params(&folded, &echo),
            folded,
            "untouched ssh echoes byte-identical"
        );
        assert_eq!(
            guided_prefill_list(&folded, &["host_key"]),
            "ecdsa-sha2-nistp256 AAAA, ssh-ed25519 BBBB",
            "stored arrays echo as comma text"
        );

        let throne = "{\"type\":\"ssh\",\"server\":\"s\",\"server_port\":22,\
            \"username\":\"root\",\"privatekey\":\"K\",\"private_key_pass\":\"pp\"}";
        let throne_echo = SshFold {
            user: "root".to_string(),
            private_key: "K".to_string(),
            private_key_passphrase: "pp".to_string(),
            ..Default::default()
        };
        assert_eq!(
            fold_ssh_into_params(throne, &throne_echo),
            throne,
            "matching aliases echo"
        );
        let template = protocol_params_template("ssh");
        assert_eq!(
            fold_ssh_into_params(&template, &SshFold::default()),
            template,
            "ssh pristine template stands"
        );
        assert_eq!(fold_ssh_into_params("{{{", &full), "{{{");
    }

    #[test]
    fn t07_snell_fold_engine_driven_fields() {
        assert_eq!(SNELL_VERSIONS, ["4", "6"]);
        let full = SnellFold {
            psk: "secret".to_string(),
            version: "6".to_string(),
            obfs_mode: "http".to_string(),
            obfs_host: "example.com".to_string(),
            userkey: "key".to_string(),
            reuse: true,
            network: "tcp".to_string(),
            mode: "multi".to_string(),
        };
        let bare = "{\"type\":\"snell\",\"server\":\"192.0.2.30\",\"server_port\":8080}";
        let folded = fold_snell_into_params(bare, &full);
        let value: serde_json::Value = serde_json::from_str(&folded).expect("snell folds");
        assert_eq!(value["psk"], "secret");
        assert_eq!(value["version"], "6");
        assert_eq!(value["obfs_mode"], "http");
        assert_eq!(value["obfs_host"], "example.com");
        assert_eq!(value["userkey"], "key");
        assert_eq!(value["reuse"], true);
        assert_eq!(value["network"], "tcp");
        assert_eq!(value["mode"], "multi");
        assert_eq!(
            fold_snell_into_params(&folded, &full),
            folded,
            "untouched snell echoes byte-identical"
        );

        let v6 = "{\"type\":\"snell\",\"server\":\"s\",\"server_port\":8080,\
            \"psk\":\"secret\",\"version\":\"6\"}";
        assert_eq!(
            fold_snell_into_params(v6, &SnellFold::default()),
            v6,
            "stored version survives blank picks"
        );

        let throne = "{\"type\":\"snell\",\"server\":\"s\",\"server_port\":8080,\"obfs\":\"http\"}";
        let throne_echo = SnellFold {
            obfs_mode: "http".to_string(),
            ..Default::default()
        };
        assert_eq!(
            fold_snell_into_params(throne, &throne_echo),
            throne,
            "matching alias echoes"
        );
        let template = protocol_params_template("snell");
        assert_eq!(
            fold_snell_into_params(&template, &SnellFold::default()),
            template,
            "snell pristine template stands"
        );
        assert_eq!(fold_snell_into_params("{{{", &full), "{{{");
    }

    #[test]
    fn t07_shadowtls_fold_version_password() {
        assert_eq!(SHADOWTLS_VERSIONS, ["1", "2", "3"]);
        let full = ShadowTlsFold {
            version: "2".to_string(),
            password: "secret".to_string(),
        };
        let bare = "{\"type\":\"shadowtls\",\"server\":\"192.0.2.31\",\"server_port\":443}";
        let folded = fold_shadowtls_into_params(bare, &full);
        let value: serde_json::Value = serde_json::from_str(&folded).expect("shadowtls folds");
        assert_eq!(value["version"], "2");
        assert_eq!(value["password"], "secret");
        assert_eq!(
            fold_shadowtls_into_params(&folded, &full),
            folded,
            "untouched shadowtls echoes byte-identical"
        );

        let v1 = "{\"type\":\"shadowtls\",\"server\":\"s\",\"server_port\":443}";
        assert_eq!(
            fold_shadowtls_into_params(v1, &ShadowTlsFold::default()),
            v1,
            "v1 rows omit the password"
        );
        let template = protocol_params_template("shadowtls");
        assert_eq!(
            fold_shadowtls_into_params(&template, &ShadowTlsFold::default()),
            template,
            "shadowtls pristine template stands"
        );
        assert_eq!(fold_shadowtls_into_params("{{{", &full), "{{{");
    }

    #[test]
    fn t07_xray_fold_ph1_preserves_ph2() {
        assert_eq!(XRAY_NETWORKS, ["raw", "xhttp", "ws", "httpupgrade", "grpc"]);
        assert_eq!(XRAY_FLOWS, ["xtls-rprx-vision", "xtls-rprx-vision-udp443"]);
        assert_eq!(XRAY_MUX_MODES, ["enabled", "disabled"]);
        let full = XrayFold {
            uuid: "11111111-2222-4333-8444-555555555555".to_string(),
            encryption: "none".to_string(),
            flow: "xtls-rprx-vision".to_string(),
            sni: "example.com".to_string(),
            fp: "chrome".to_string(),
            alpn: "h2".to_string(),
            pbk: "pubkey".to_string(),
            sid: "shortid".to_string(),
            spiderx: "/".to_string(),
            network: "xhttp".to_string(),
            mode: "auto".to_string(),
            mux: "enabled".to_string(),
        };
        let bare = "{\"type\":\"xray\",\"server\":\"192.0.2.32\",\"server_port\":443}";
        let folded = fold_xray_into_params(bare, &full);
        let value: serde_json::Value = serde_json::from_str(&folded).expect("xray folds");
        assert_eq!(value["uuid"], "11111111-2222-4333-8444-555555555555");
        assert_eq!(value["encryption"], "none");
        assert_eq!(value["flow"], "xtls-rprx-vision");
        assert_eq!(value["sni"], "example.com");
        assert_eq!(value["fp"], "chrome");
        assert_eq!(value["alpn"], "h2");
        assert_eq!(value["pbk"], "pubkey");
        assert_eq!(value["sid"], "shortid");
        assert_eq!(value["spiderx"], "/");
        assert_eq!(value["network"], "xhttp");
        assert_eq!(value["mode"], "auto");
        assert_eq!(value["xray_mux"], "enabled");
        assert_eq!(
            fold_xray_into_params(&folded, &full),
            folded,
            "untouched xray echoes byte-identical"
        );

        let ph2 = "{\"type\":\"xray\",\"server\":\"s\",\"server_port\":443,\
            \"uuid\":\"11111111-2222-4333-8444-555555555555\",\"encryption\":\"none\",\
            \"flow\":\"xtls-rprx-vision\",\"sni\":\"example.com\",\"fp\":\"chrome\",\
            \"alpn\":\"h2\",\"pbk\":\"pubkey\",\"sid\":\"shortid\",\"spiderx\":\"/\",\
            \"network\":\"xhttp\",\"mode\":\"auto\",\"xray_mux\":\"enabled\",\
            \"xPaddingBytes\":\"100-200\",\"maxConcurrency\":16,\
            \"downloadSettings\":{\"address\":\"d.example.com\"},\
            \"xray_custom_future\":\"kept\"}";
        assert_eq!(
            fold_xray_into_params(ph2, &full),
            ph2,
            "Ph2-only and unknown xray_* keys round-trip untouched (preserve-not-strip)"
        );

        let throne = "{\"type\":\"xray\",\"server\":\"s\",\"server_port\":443,\
            \"fingerprint\":\"chrome\"}";
        let throne_echo = XrayFold {
            fp: "chrome".to_string(),
            ..Default::default()
        };
        assert_eq!(
            fold_xray_into_params(throne, &throne_echo),
            throne,
            "matching alias echoes"
        );
        assert!(
            !folded.contains("xPaddingBytes") && !folded.contains("maxConcurrency"),
            "the fold writes no Ph2 matrix keys"
        );
        assert_eq!(fold_xray_into_params("{{{", &full), "{{{");
    }

    #[test]
    fn t07_chain_hop_rules_name_throne_rejections() {
        use ChainHopSlot::{Autoselector, Chain, Extra, Other};
        let none: &[ChainHopSlot] = &[];
        let one_other: &[ChainHopSlot] = &[Other];
        let extra_first: &[ChainHopSlot] = &[Extra, Other];

        for candidate in [Chain, Autoselector] {
            let rule = chain_hop_rejection(none, candidate, 0, false).expect("rejected");
            assert!(
                rule.contains("chain") || rule.contains("autoselector"),
                "refusal names the flat-chain rule: {rule}"
            );
            assert!(chain_hop_rejection(one_other, candidate, 1, false).is_some());
        }

        let off_index = chain_hop_rejection(one_other, Extra, 1, false).expect("rejected");
        assert!(
            off_index.contains("index 0"),
            "off-index-0 extracore refusal names the rule: {off_index}"
        );
        assert!(chain_hop_rejection(none, Extra, 0, false).is_none());
        assert!(chain_hop_rejection(one_other, Extra, 0, false).is_none());

        let second = chain_hop_rejection(extra_first, Extra, 0, false).expect("rejected");
        assert!(
            second.contains("at most one"),
            "second extracore refusal names the rule: {second}"
        );
        assert!(chain_hop_rejection(extra_first, Extra, 0, true).is_none());
        assert!(chain_hop_rejection(extra_first, Other, 1, true).is_none());

        let oob = chain_hop_rejection(one_other, Other, 1, true).expect("rejected");
        assert!(
            oob.contains("out of range"),
            "out-of-range replace names the rule: {oob}"
        );

        assert!(chain_hop_rejection(none, Other, 0, false).is_none());
        assert!(chain_hop_rejection(extra_first, Other, 2, false).is_none());

        let params = "{\"type\":\"chain\",\"nodes\":[\"a\",\"b\"]}";
        assert_eq!(
            chain_nodes_of(params),
            vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::String("b".to_string())
            ]
        );
        assert!(chain_nodes_of("{\"type\":\"chain\"}").is_empty());
        assert!(chain_nodes_of("{{{").is_empty());
        assert_eq!(
            fold_chain_nodes_into_params(
                params,
                &[
                    serde_json::Value::String("a".to_string()),
                    serde_json::Value::String("b".to_string())
                ]
            ),
            params,
            "untouched hops refold byte-identical"
        );
        let cleared = fold_chain_nodes_into_params(params, &[]);
        let cleared_value: serde_json::Value =
            serde_json::from_str(&cleared).expect("clear-all folds");
        assert_eq!(cleared_value["nodes"], serde_json::json!([]));
        let template = protocol_params_template("chain");
        assert_eq!(
            fold_chain_nodes_into_params(&template, &[]),
            template,
            "empty editor over a nodeless template is a no-op"
        );
        assert_eq!(fold_chain_nodes_into_params("{{{", &[]), "{{{");
    }

    #[test]
    fn t07_extra_fold_validate_never_no_logs() {
        let full = ExtraFold {
            exe: "core.exe".to_string(),
            args: "--fast".to_string(),
            config: "{\"inbounds\":[]}".to_string(),
        };
        let bare = "{\"type\":\"extra-core\"}";
        let folded = fold_extra_into_params(bare, &full);
        let value: serde_json::Value = serde_json::from_str(&folded).expect("extra folds");
        assert_eq!(value["exe"], "core.exe");
        assert_eq!(value["args"], "--fast");
        assert_eq!(value["config"], serde_json::json!({"inbounds":[]}));
        assert!(
            value.get("no_logs").is_none() && !folded.contains("no_logs"),
            "the fold never writes no_logs"
        );
        assert_eq!(
            fold_extra_into_params(&folded, &full),
            folded,
            "untouched extra echoes byte-identical"
        );

        assert_eq!(
            extra_config_prefill(&folded),
            "{\"inbounds\":[]}",
            "stored objects echo as JSON text"
        );
        let with_string = "{\"type\":\"extra-core\",\"config\":\"{\\\"a\\\":1}\"}";
        assert_eq!(extra_config_prefill(with_string), "{\"a\":1}");
        assert_eq!(extra_config_prefill("{\"type\":\"extra-core\"}"), "");
        assert_eq!(extra_config_prefill("{{{"), "");

        let alias = "{\"type\":\"extra-core\",\"binary\":\"core.exe\"}";
        let alias_echo = ExtraFold {
            exe: "core.exe".to_string(),
            ..Default::default()
        };
        assert_eq!(
            fold_extra_into_params(alias, &alias_echo),
            alias,
            "matching alias echoes"
        );

        assert!(validate_extra_params(&folded).is_ok());
        let no_exe = validate_extra_params("{\"config\":\"{}\"}").expect_err("exe required");
        assert!(no_exe.contains("exe"), "missing exe names exe: {no_exe}");
        let no_config =
            validate_extra_params("{\"exe\":\"core.exe\"}").expect_err("config required");
        assert!(
            no_config.contains("config"),
            "missing config names config: {no_config}"
        );
        let bad_config = validate_extra_params("{\"exe\":\"core.exe\",\"config\":\"{oops\"}")
            .expect_err("config must be JSON");
        assert!(
            bad_config.contains("config"),
            "bad config names config: {bad_config}"
        );
        let scalar_config =
            validate_extra_params("{\"exe\":\"core.exe\",\"config\":7}").expect_err("shape gated");
        assert!(
            scalar_config.contains("config"),
            "scalar config names config: {scalar_config}"
        );
        assert!(validate_extra_params("not json").is_err());
        assert_eq!(fold_extra_into_params("{{{", &full), "{{{");
    }

    #[test]
    fn t02_prefill_bool_reads_truthy_spellings() {
        assert!(guided_prefill_bool(
            "{\"reuse_addr\":true}",
            &["reuse_addr"]
        ));
        assert!(guided_prefill_bool("{\"reuse_addr\":1}", &["reuse_addr"]));
        assert!(guided_prefill_bool("{\"a\":\"ON\"}", &["a"]));
        assert!(!guided_prefill_bool(
            "{\"reuse_addr\":false}",
            &["reuse_addr"]
        ));
        assert!(!guided_prefill_bool("{\"reuse_addr\":0}", &["reuse_addr"]));
        assert!(!guided_prefill_bool("{\"other\":true}", &["reuse_addr"]));
        assert!(!guided_prefill_bool("{{{", &["reuse_addr"]));
        assert!(!guided_prefill_bool("", &["reuse_addr"]));
    }

    #[test]
    fn t05_guided_identity_fields_mirror_validation() {
        let vless = guided_identity_fields("vless").expect("vless maps");
        assert_eq!(vless.key, Some("uuid"));
        assert_eq!(vless.needs, "uuid");
        let trojan = guided_identity_fields("trojan").expect("trojan maps");
        assert_eq!(trojan.key, Some("password"));

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

    #[test]
    fn t05_fold_guided_leaves_blank_untouched_and_merges_touched() {
        let template = protocol_params_template("vless");

        assert_eq!(
            fold_guided_into_params(&template, "", "", Some(("uuid", &["uuid", "id"])), ""),
            template
        );

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

        let bad_port = fold_guided_into_params(shaped, "", "abc", None, "");
        assert!(
            bad_port.contains("\"server_port\":\"abc\""),
            "loud for validation, got {bad_port}"
        );

        assert_eq!(
            fold_guided_into_params("{{{", "a.example.com", "", None, ""),
            "{{{"
        );
    }

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

    #[test]
    fn ticket31_shared_bind_port_saves_without_silent_dedup() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let first = store
            .add(NewConfig {
                name: "One".to_string(),
                protocol: "vless".to_string(),
                params: Secret::default(),
                group: String::new(),
                bind_ip: "127.0.0.1".to_string(),
                port: "12001".to_string(),
            })
            .expect("first saves");
        let second = store
            .add(NewConfig {
                name: "Two".to_string(),
                protocol: "vless".to_string(),
                params: Secret::default(),
                group: String::new(),
                bind_ip: "127.0.0.1".to_string(),
                port: "12001".to_string(),
            })
            .expect("second saves too — warning, never a block");
        assert_ne!(first.id, second.id, "two rows, no silent dedup");
        assert_eq!(store.list().expect("list works").len(), 2);
    }

    fn t37_seed_row(store: &mut Store, name: &str) -> ProxyConfig {
        store
            .add(NewConfig {
                name: name.to_string(),
                protocol: "vless".to_string(),
                params: Secret::default(),
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

        assert!(
            store.set_config_preset(&row.id, Some(&preset.id)).is_err(),
            "empty attach refused"
        );
        store
            .add_preset_rule(&preset.id, serde_json::json!({"action": "direct"}))
            .expect("re-add rule works");

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
            params: "{\"a\":1}".to_string().into(),
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

    #[test]
    fn t48_foreign_blob_reads_quarantine() {
        use base64::Engine as _;
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let good = store
            .add(NewConfig {
                name: "Good".to_string(),
                protocol: "vless".to_string(),
                params: "{\"a\":1}".to_string().into(),
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
                    .to_string()
                    .into(),
                group: String::new(),
                bind_ip: "127.0.0.1".to_string(),
                port: "10801".to_string(),
            })
            .expect("seed row saves")
    }

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

    #[test]
    fn appearance_defaults_dark_and_round_trips() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        assert_eq!(store.appearance(), "dark", "default dark");
        for value in ["dark", "light", "system"] {
            store.set_appearance(value).expect("valid choice writes");
            assert_eq!(store.appearance(), value, "{value} reads back");
        }
        assert!(
            store.set_appearance("midnight").is_err(),
            "junk fails loudly"
        );
        store
            .set_meta(Store::META_APPEARANCE, "midnight")
            .expect("raw write");
        assert_eq!(store.appearance(), "dark", "corrupt reads dark");
    }

    #[test]
    fn library_columns_absent_and_round_trips() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        assert_eq!(store.library_columns_raw(), None, "default absent");
        store
            .set_library_columns("upstream,health")
            .expect("valid set writes");
        assert_eq!(
            store.library_columns_raw().as_deref(),
            Some("upstream,health"),
            "round-trips verbatim"
        );
        assert!(
            store.set_library_columns("  ").is_err(),
            "blank fails loudly"
        );
    }

    #[test]
    fn restore_running_defaults_off_and_round_trips() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        assert!(!store.restore_running(), "default off");
        store.set_restore_running(true).expect("opt-in writes");
        assert!(store.restore_running(), "opt-in reads back");
        store.set_restore_running(false).expect("opt-out writes");
        assert!(!store.restore_running(), "opt-out reads back");
    }

    #[test]
    fn t10_conn_column_widths_persist_clamp_and_reset() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        assert_eq!(
            store.conn_column_widths(),
            Store::default_conn_column_widths(),
            "unset reads defaults"
        );
        store.set_conn_column_width(0, 300).expect("width writes");
        let widths = store.conn_column_widths();
        assert_eq!(widths[0], 300, "resize persists");
        assert_eq!(widths[1], Store::default_conn_column_widths()[1]);
        store.set_conn_column_width(1, 5).expect("tiny clamps");
        assert_eq!(store.conn_column_widths()[1], 40, "clamped, not zero");
        store.set_conn_column_width(2, 9999).expect("huge clamps");
        assert_eq!(store.conn_column_widths()[2], 600, "clamped, not huge");
        assert!(
            store.set_conn_column_width(5, 100).is_err(),
            "bad column fails loudly"
        );
        store
            .set_meta(Store::META_CONN_COL_WIDTHS, "[1,2]")
            .expect("raw write");
        assert_eq!(
            store.conn_column_widths(),
            Store::default_conn_column_widths(),
            "short reads defaults"
        );
        store
            .set_meta(Store::META_CONN_COL_WIDTHS, "junk")
            .expect("raw write");
        assert_eq!(
            store.conn_column_widths(),
            Store::default_conn_column_widths(),
            "corrupt reads defaults"
        );
        store.set_conn_column_width(0, 300).expect("width writes");
        store.reset_conn_column_widths().expect("reset works");
        assert_eq!(
            store.conn_column_widths(),
            Store::default_conn_column_widths(),
            "refresh restores defaults"
        );
    }

    #[test]
    fn t10_traffic_period_summary_matches_stored_totals() {
        let mut totals = std::collections::HashMap::new();
        totals.insert("1".to_string(), (1000u64, 50u64));
        totals.insert("2".to_string(), (3000u64, 90u64));
        totals.insert("3".to_string(), (0u64, 0u64));
        for period in TrafficPeriod::ALL {
            let (exits, rx, tx) = Store::traffic_period_summary(&totals, period);
            assert_eq!((exits, rx, tx), (2, 4000, 140), "period {}", period.label());
        }
        assert_eq!(
            Store::traffic_period_summary(&std::collections::HashMap::new(), TrafficPeriod::Day),
            (0, 0, 0),
            "empty stays empty"
        );
    }

    #[test]
    fn running_snapshot_round_trips_and_fails_empty() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        assert!(
            store.load_running_snapshot().is_empty(),
            "missing reads empty"
        );

        store
            .save_running_snapshot(&["3".to_string(), "7".to_string()])
            .expect("snapshot writes");
        assert!(
            store.load_running_snapshot().is_empty(),
            "unknown ids filter to empty"
        );

        let row = store
            .add(NewConfig {
                name: "R".to_string(),
                protocol: "vless".to_string(),
                params: Secret::default(),
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

            let sealed = crate::secret::Secret::new(
                r#"{"server":"198.51.100.9","server_port":8388,"method":"aes-128-gcm","password":"p"}"#,
            )
            .seal()
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

        let subs = store.list_subscriptions().expect("subs list");
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].interval_secs, 86_400, "default interval");

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

        let rows = store.list().expect("rows list");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "V4Row");

        drop(store);
        let store2 = Store::open_file(&v4_path).expect("reopen works");
        let version2: i64 = store2
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .expect("version reads");
        assert_eq!(version2, Store::SCHEMA_VERSION);

        let fresh = Store::open_in_memory().expect("fresh opens");
        for (table, column) in [
            ("subscriptions", "interval_secs"),
            ("route_presets", "description"),
            ("route_presets", "is_builtin"),
            ("route_presets", "last_used_at"),
            ("route_presets", "locked"),
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

    #[test]
    fn t40_corrupt_row_quarantines() {
        let mut store = Store::open_in_memory().expect("in-memory opens");
        let good = store
            .add(NewConfig {
                name: "Good".to_string(),
                protocol: "shadowsocks".to_string(),
                params: r#"{"server":"198.51.100.9","server_port":8388,"method":"aes-128-gcm","password":"p"}"#.to_string().into(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("good seeds");

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

        assert!(store.list().is_err(), "strict list fails on corrupt row");

        let (rows, quarantined) = store.list_with_quarantine().expect("tolerant lists");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, good.id);
        assert_eq!(quarantined.len(), 1);
        assert_eq!(quarantined[0].id, bad_id_str);

        let (payload, skipped) = store
            .export_library_json_with_quarantine()
            .expect("tolerant export works");
        assert_eq!(skipped, vec![bad_id_str.clone()]);
        assert!(payload.contains("Good"), "good row exported");
        assert!(!payload.contains("\"Bad\""), "bad row skipped");

        assert!(store.get(&bad_id_str).is_err(), "strict get rejects");
    }

    #[test]
    fn t40_traffic_batch_is_atomic_and_saturating() {
        let mut store = Store::open_in_memory().expect("in-memory opens");
        let a = store
            .add(NewConfig {
                name: "A".to_string(),
                protocol: "shadowsocks".to_string(),
                params: r#"{"server":"198.51.100.9","server_port":8388,"method":"aes-128-gcm","password":"p"}"#.to_string().into(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("seed A");
        let b = store
            .add(NewConfig {
                name: "B".to_string(),
                protocol: "shadowsocks".to_string(),
                params: r#"{"server":"198.51.100.10","server_port":8388,"method":"aes-128-gcm","password":"p"}"#.to_string().into(),
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

        assert!(store.remove_subscription(&sub.id).expect("unsub works"));

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

    #[test]
    fn t40_preset_attach_revalidated_and_orphan_sweep() {
        let mut store = Store::open_in_memory().expect("in-memory opens");
        let row = store
            .add(NewConfig {
                name: "R".to_string(),
                protocol: "vless".to_string(),
                params: Secret::default(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("row seeds");

        assert!(
            matches!(
                store.set_config_preset(&row.id, Some("9999")),
                Err(StoreError::NotFound(_))
            ),
            "dangling attach refuses"
        );

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

    #[test]
    fn t40_snapshot_filters_unknown_ids() {
        let mut store = Store::open_in_memory().expect("in-memory opens");
        let row = store
            .add(NewConfig {
                name: "R".to_string(),
                protocol: "vless".to_string(),
                params: Secret::default(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("row seeds");
        store
            .save_running_snapshot(&[row.id.clone(), "9999".to_string()])
            .expect("snapshot writes");
        assert_eq!(store.load_running_snapshot(), vec![row.id.clone()]);

        store.set_query_only_for_test().expect("readonly sets");
        assert!(
            store
                .save_running_snapshot(std::slice::from_ref(&row.id))
                .is_err(),
            "write error surfaces"
        );
    }

    #[test]
    fn t40_health_history_appends_and_prunes() {
        let mut store = Store::open_in_memory().expect("in-memory opens");
        let row = store
            .add(NewConfig {
                name: "R".to_string(),
                protocol: "vless".to_string(),
                params: Secret::default(),
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

        let pruned = store
            .prune_health_history_before(1_500)
            .expect("prune works");
        assert_eq!(pruned, 1);
        let hist = store
            .load_health_history(&row.id, None)
            .expect("history loads");
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].at, 2_000);

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

    #[test]
    fn t45_sub_interval_honored_by_due() {
        let mut store = Store::open_in_memory().expect("in-memory opens");
        let sub = store
            .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
            .expect("seed works");
        assert_eq!(sub.interval_secs, DEFAULT_AUTO_REFRESH_SECS);
        assert_eq!(sub.last_refresh_at, None);
        let now = 1_700_000_000;

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

        assert!(
            store.set_subscription_interval("9999", 60).is_err(),
            "unknown id fails loudly"
        );
    }

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

    #[test]
    fn t45_reset_traffic_zeroes_counters() {
        let mut store = Store::open_in_memory().expect("in-memory opens");
        let row = store
            .add(NewConfig {
                name: "R".to_string(),
                protocol: "vless".to_string(),
                params: Secret::default(),
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

    #[test]
    fn t45_history_retention_window_keeps_recent() {
        let mut store = Store::open_in_memory().expect("in-memory opens");
        let row = store
            .add(NewConfig {
                name: "R".to_string(),
                protocol: "vless".to_string(),
                params: Secret::default(),
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

    #[test]
    fn t45_logs_path_pattern() {
        let path = Store::default_logs_path();
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("manyproxy-logs.txt"),
            "logs file name is stable"
        );
    }

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
                params: r#"{"server":"198.51.100.9","server_port":8388,"method":"aes-128-gcm","password":"p"}"#.to_string().into(),
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

        let kept: Vec<_> = std::fs::read_dir(&dir)
            .expect("dir reads")
            .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
            .filter(|n| n.contains("corrupt"))
            .collect();
        assert_eq!(kept.len(), 1, "one quarantined file, got {kept:?}");
        std::fs::remove_dir_all(&dir).ok();
    }

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
                    params: Secret::default(),
                    group: String::new(),
                    bind_ip: String::new(),
                    port: String::new(),
                })
                .expect("live seeds");
            store.export_library_json().expect("export works")
        };
        let backup = Store::backup_db_file(&path, now_unix()).expect("backup works");
        assert!(backup.exists(), "timestamped backup exists");

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

        let _ = (payload, backup);
        std::fs::remove_dir_all(&dir).ok();
    }

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

    #[test]
    fn bug22_rejected_restore_leaves_no_restore_temp() {
        let dir = std::env::temp_dir().join(format!(
            "manyproxy-bug22store-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock works")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir works");
        let path = dir.join("library.db");
        Store::open_file(&path).expect("live opens");

        let error = Store::restore_library_file(&path, "not-json", now_unix())
            .expect_err("garbage payload rejected");
        assert!(
            matches!(error, StoreError::Invalid(_)),
            "Invalid, got {error}"
        );

        let leftovers: Vec<String> = std::fs::read_dir(&dir)
            .expect("read dir works")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".restore-"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "no restore temp next to the db, got {leftovers:?}"
        );
        Store::open_file(&path).expect("live db still opens");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn bug22_restore_temp_cleanup_removes_sidecars() {
        let dir = std::env::temp_dir().join(format!(
            "manyproxy-bug22tmp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock works")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir works");
        let tmp = dir.join(".library.db.restore-999-1-0");
        let keep = dir.join("keep.txt");
        std::fs::write(&tmp, "db").expect("tmp written");
        for suffix in ["-wal", "-shm", "-journal"] {
            std::fs::write(sibling_sidecar(&tmp, suffix), "sidecar").expect("sidecar written");
        }
        std::fs::write(&keep, "keep").expect("bystander written");

        Store::remove_restore_temp(&tmp);

        assert!(!tmp.exists(), "restore temp removed");
        for suffix in ["-wal", "-shm", "-journal"] {
            assert!(
                !sibling_sidecar(&tmp, suffix).exists(),
                "sidecar {suffix} removed"
            );
        }
        assert!(keep.exists(), "unrelated file kept");
        std::fs::remove_dir_all(&dir).ok();
    }

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

    #[test]
    fn group_ops_trim_consistently() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let mut a = valid_input("A");
        a.group = "g1".to_string();
        store.add(a).expect("add works");
        assert_eq!(
            store
                .rename_group("  g1  ", "  team  ")
                .expect("padded rename works")
                .rows,
            1,
            "old and new both trim"
        );
        assert!(
            store.delete_group(" team ").is_err(),
            "padded delete still sees the moved row (refuses, never a false-empty Ok)"
        );
    }

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

        let store = Store::open_file(&path).expect("retry works");
        let version: i64 = store
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .expect("version reads");
        assert_eq!(version, Store::SCHEMA_VERSION);
        std::fs::remove_dir_all(&dir).ok();
    }

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

    #[test]
    fn t60_preset_locked_column_round_trip() {
        let dir = t52_temp_root("t60locked");
        let path = dir.join("library.db");
        let preset_id = {
            let mut store = Store::open_file(&path).expect("file store opens");
            let preset = store
                .create_preset(
                    "t60-locked",
                    "manual",
                    None,
                    &[serde_json::json!({"action": "direct"})],
                )
                .expect("preset seeds");
            assert!(
                !preset.locked,
                "presets start unlocked (no auto-lock on import)"
            );
            let locked = store
                .set_preset_locked(&preset.id, true)
                .expect("lock works");
            assert!(locked.locked, "toggle locks in memory");
            assert!(
                store
                    .column_exists("route_presets", "locked")
                    .expect("probe works"),
                "additive ensure_column added route_presets.locked"
            );
            preset.id.clone()
        };
        // The lock survives a restart (column round trip), and attaching a
        // locked preset is allowed: the lock protects the rules, never the
        // attach relationship.
        {
            let mut store = Store::open_file(&path).expect("reopen works");
            let preset = store
                .get_preset(&preset_id)
                .expect("read works")
                .expect("preset kept");
            assert!(preset.locked, "locked persists across restarts");
            let row = store.add(valid_input("Target")).expect("row seeds");
            let attached = store
                .set_config_preset(&row.id, Some(&preset_id))
                .expect("attach works");
            assert_eq!(
                attached.preset_id.as_deref(),
                Some(preset_id.as_str()),
                "locked presets attach"
            );
            let detached = store
                .set_config_preset(&row.id, None)
                .expect("detach works");
            assert!(detached.preset_id.is_none(), "locked presets detach");
        }
        // Export carries the lock as plain additive state...
        let source_store = Store::open_file(&path).expect("source store opens");
        let exported = source_store.export_library_json().expect("export works");
        assert!(exported.contains("\"locked\":true"), "{exported}");
        // ...and restore keeps it without touching the source file.
        {
            let mut store = Store::open_in_memory().expect("in-memory store opens");
            store
                .restore_library_json(&exported)
                .expect("restore works");
            let restored = store
                .get_preset(&preset_id)
                .expect("read works")
                .expect("preset restored");
            assert!(restored.locked, "restore keeps the lock");
            let unlocked = store
                .set_preset_locked(&preset_id, false)
                .expect("unlock works");
            assert!(!unlocked.locked, "toggle unlocks");
        }
        let preset = source_store
            .get_preset(&preset_id)
            .expect("read works")
            .expect("preset kept");
        assert!(preset.locked, "the source file was never touched");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn t60_store_caps_rule_names() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        // create_preset condenses over-long rule names with the cap_name
        // taste; short names stay byte-exact.
        let preset = store
            .create_preset(
                "t60-caps",
                "manual",
                None,
                &[
                    serde_json::json!({"action": "route", "name": "a".repeat(300)}),
                    serde_json::json!({"action": "direct", "name": "Office exit"}),
                ],
            )
            .expect("preset seeds");
        let rules = preset.rules().expect("rules decode");
        let long = rules[0]["name"].as_str().expect("name stays a string");
        assert_eq!(
            long.chars().count(),
            crate::preset::MAX_RULE_NAME_CHARS,
            "stored name capped to 128 chars"
        );
        assert!(long.starts_with(&"a".repeat(100)), "{long}");
        assert_eq!(
            rules[1]["name"],
            serde_json::json!("Office exit"),
            "short name byte-exact"
        );
        let set = store
            .set_preset_rules(
                &preset.id,
                &[serde_json::json!({"action": "direct", "name": "b".repeat(300)})],
            )
            .expect("set works");
        let rules = set.rules().expect("rules decode");
        assert_eq!(
            rules[0]["name"]
                .as_str()
                .expect("name stays a string")
                .chars()
                .count(),
            crate::preset::MAX_RULE_NAME_CHARS,
            "set_preset_rules caps too"
        );
    }

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

    fn t52_temp_root(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "manyproxy-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock works")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp root works");
        dir
    }

    #[test]
    fn t52_default_path_is_exe_relative_data() {
        let path = Store::default_path();
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("library.db"),
            "db file name unchanged, got {path:?}"
        );
        let exe_dir = std::env::current_exe()
            .expect("test exe resolves")
            .parent()
            .map(Path::to_path_buf)
            .expect("exe dir resolves");
        assert_eq!(
            path.parent().map(Path::to_path_buf),
            Some(app_data_dir()),
            "db default is the shared app-data helper"
        );
        assert_eq!(
            app_data_dir(),
            exe_dir.join(DATA_DIR_NAME),
            "data folder sits beside the exe, never elsewhere"
        );

        let base = t52_temp_root("t52apps");
        let a = base.join("install-a");
        let b = base.join("install-b");
        assert_eq!(
            app_data_dir_in(Some(&a))
                .file_name()
                .map(|n| n.to_os_string()),
            Some(std::ffi::OsString::from(DATA_DIR_NAME)),
            "every app dir gets a data subfolder"
        );
        assert_ne!(
            app_data_dir_in(Some(&a)),
            app_data_dir_in(Some(&b)),
            "two app dirs yield independent defaults"
        );
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn t52_export_and_logs_defaults_inside_data() {
        let data = app_data_dir();
        assert_eq!(
            Store::default_export_path(),
            data.join("manyproxy-export.json"),
            "export default derives into the data folder"
        );
        assert_eq!(
            Store::default_logs_path(),
            data.join("manyproxy-logs.txt"),
            "logs default derives into the data folder"
        );
        assert_eq!(
            Store::default_export_path().parent().map(Path::to_path_buf),
            Store::default_path().parent().map(Path::to_path_buf),
            "export and db share one parent"
        );
        assert_eq!(
            Store::default_logs_path().parent().map(Path::to_path_buf),
            Store::default_path().parent().map(Path::to_path_buf),
            "logs and db share one parent"
        );
    }

    #[test]
    fn t52_backup_and_quarantine_beside_db() {
        let base = t52_temp_root("t52backup");
        let data = app_data_dir_in(Some(&base));
        let db = data.join("library.db");
        std::fs::create_dir_all(&data).expect("data dir works");
        std::fs::write(&db, b"this is not sqlite at all").expect("garbage writes");

        let error = match Store::open_file(&db) {
            Ok(_) => panic!("corrupt open must fail"),
            Err(err) => err,
        };
        let text = error.to_string();
        assert!(
            text.contains("corrupt") || text.contains("integrity"),
            "loud failure at the new location, got {text}"
        );

        let kept: Vec<_> = std::fs::read_dir(&data)
            .expect("data dir reads")
            .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
            .filter(|n| n.contains("corrupt"))
            .collect();
        assert_eq!(
            kept.len(),
            1,
            "quarantine lands beside the db, got {kept:?}"
        );

        let store = Store::open_file(&db).expect("fresh db opens after quarantine");
        drop(store);
        let backup = Store::backup_db_file(&db, 12345).expect("backup works");
        assert_eq!(
            backup.parent().map(Path::to_path_buf),
            Some(data.clone()),
            "backup lands beside the db"
        );
        assert!(
            backup
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("library.db.bak-")),
            "backup file name is the .bak sibling, got {backup:?}"
        );
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn t52_fresh_default_store_at_exe_relative_data() {
        let base = t52_temp_root("t52fresh");
        let data = app_data_dir_in(Some(&base));
        let db = data.join("library.db");
        assert!(!data.exists(), "fresh install starts without a data folder");

        let payload = {
            let mut store = Store::open_file(&db).expect("first open creates the data folder");
            assert!(data.exists(), "<appdir> data folder created on first use");
            store.add(valid_input("t52-fresh")).expect("row seeds");
            store.export_library_json().expect("export works")
        };
        let reopened = Store::open_file(&db).expect("second open works");
        let rows = reopened.list().expect("list works");
        assert_eq!(rows.len(), 1, "open twice keeps the library, no migration");
        assert_eq!(rows[0].name, "t52-fresh", "row survives the reopen");

        let mut dry = Store::open_in_memory().expect("in-memory opens");
        dry.restore_library_json(&payload)
            .expect("restore round trip works");
        let restored = dry.list().expect("restored list works");
        assert_eq!(restored.len(), 1, "row survives export/restore in memory");
        assert_eq!(restored[0].name, "t52-fresh", "restored row intact");

        let other = t52_temp_root("t52fresh-second");
        let other_db = app_data_dir_in(Some(&other)).join("library.db");
        let fresh = Store::open_file(&other_db).expect("second app dir opens");
        assert!(
            fresh.list().expect("fresh list works").is_empty(),
            "another app dir starts a new library, nothing carried over"
        );
        std::fs::remove_dir_all(&base).ok();
        std::fs::remove_dir_all(&other).ok();
    }

    #[test]
    fn t52_no_default_paths_reference_appdata() {
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        // Needles are assembled at runtime so this guard never matches its own source.
        let appdata = ["APP", "DATA"].concat();
        let dot = '.';
        let legacy_home = format!("{dot}{}{}", "many", "proxy");

        let mut offenders = Vec::new();
        for entry in std::fs::read_dir(&src_dir).expect("src dir reads") {
            let path = entry.expect("entry reads").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let text = std::fs::read_to_string(&path).expect("source reads");
            if text.contains(&appdata) {
                offenders.push(format!("{name}: {appdata}"));
            }
            if text.contains(&legacy_home) {
                offenders.push(format!("{name}: {legacy_home}"));
            }
        }
        assert!(
            offenders.is_empty(),
            "default storage must never reference the legacy homes, got {offenders:?}"
        );
    }

    #[test]
    fn t62_rule_set_staged_under_data_rule_sets() {
        // The staging root is the T-02 family: `<appdir>\data\rule-sets\`,
        // derived from the same app-dir helper as the DB — nothing else.
        let base = t52_temp_root("t62staged");
        let app = base.join("install");
        assert_eq!(
            rule_sets_dir_in(Some(&app)),
            app.join(DATA_DIR_NAME).join(RULE_SETS_DIR_NAME),
            "rule sets stage under the command data folder"
        );

        let store = Store::open_in_memory().expect("in-memory store opens");
        assert!(store.list_rule_sets().expect("list works").is_empty());

        let row = store
            .create_rule_set("Block Ads", "https://s.example.com/z.srs", 512)
            .expect("stage records");
        assert_eq!(row.name, "Block Ads");
        assert_eq!(row.url, "https://s.example.com/z.srs");
        assert_eq!(
            row.file_name,
            crate::store::rule_set_file_name("Block Ads"),
            "the file name mirrors the sanitized name the engine derives"
        );
        assert_eq!(row.file_name, "Block-Ads.srs");
        assert_eq!(row.bytes, 512);
        assert!(row.downloaded_at > 0);

        // The same entry again is the UNIQUE guard: overwriting happens only
        // through the confirmed re-Download flow (touch), never at intake.
        let error = store
            .create_rule_set("Block Ads", "https://s.example.com/z.srs", 512)
            .expect_err("duplicate name is refused");
        let StoreError::Invalid(fields) = error else {
            panic!("expected Invalid, got {error:?}");
        };
        assert!(
            fields.iter().any(|e| e.message.contains("already staged")),
            "{fields:?}"
        );

        // Re-download refreshes bytes/URL and shifts downloaded_at forward.
        let first_touched = row.downloaded_at;
        let touched = store
            .touch_rule_set(&row.id, "https://mirror.example.com/z.srs", 640)
            .expect("touch refreshes");
        assert_eq!(touched.bytes, 640);
        assert_eq!(touched.url, "https://mirror.example.com/z.srs");
        assert!(touched.downloaded_at >= first_touched);

        let error = store
            .create_rule_set(
                "Too much",
                "https://s.example.com/z.srs",
                (SUBSCRIPTION_MAX_BYTES + 1) as u64,
            )
            .expect_err("over-cap refuse");
        assert!(error.to_string().contains("staging cap"), "{error}");

        // Name validation: the name doubles as the staged file base, so a
        // name must survive the tag sanitizer with a letter/digit, never be a
        // Windows device name, and stay bounded.
        for (bad, cause) in [
            ("   ", "empty"),
            ("///", "no letter or digit"),
            ("名", "no letter or digit"),
            ("con", "reserved Windows device name"),
        ] {
            let error = store
                .create_rule_set(bad, "https://s.example.com/z.srs", 10)
                .expect_err("this name must not stage");
            let StoreError::Invalid(fields) = error else {
                panic!("expected Invalid for {bad}, got {error:?}");
            };
            let field = fields.first().expect("field error present");
            match cause {
                "empty" => assert_eq!(field.message, "rule set name must not be empty"),
                "no letter or digit" => assert_eq!(
                    field.message, "rule set name must contain a letter or digit",
                    "raw name {bad} sanitizes to no usable stem"
                ),
                "reserved Windows device name" => {
                    let message = field.message.clone();
                    assert!(
                        message.contains("reserved"),
                        "device-name refusal: {message}"
                    );
                }
                other => panic!("unknown cause {other}"),
            }
        }

        // An unused row deletes clean and hands back the file name so the flow
        // can unlink the cache bytes (no silent orphan cache survives).
        let file_name = store
            .delete_rule_set(&row.id)
            .expect("delete works")
            .expect("the staged file name comes back");
        assert_eq!(file_name, "Block-Ads.srs");
        assert!(store.list_rule_sets().expect("list works").is_empty());
        assert_eq!(
            store.delete_rule_set(&row.id).expect("ghost delete"),
            None,
            "deleting twice is a no-op, not an error"
        );
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn t62_rule_set_used_guard_blocks_remove() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let row = store.add(valid_input("Row")).expect("row seeds");
        let ads = store
            .create_rule_set("ads", "https://s.example.com/ads.srs", 11)
            .expect("stage records");
        let preset = store
            .create_preset(
                "t62-guard",
                "manual",
                None,
                &[serde_json::json!({"action": "proxy", "rule_set": ["ads"]})],
            )
            .expect("preset seeds");
        store
            .set_config_preset(&row.id, Some(&preset.id))
            .expect("attach works — the set is staged");
        assert_eq!(
            store.rule_sets_in_use(&preset.id).expect("in-use works"),
            vec!["ads".to_string()],
            "the helper names the sets a preset references"
        );
        assert_eq!(
            store.rule_set_used_by("ads").expect("used-by works"),
            vec![preset.name.clone()],
            "the guard names every preset still using the set"
        );
        assert!(store.rule_set_used_by("none").expect("works").is_empty());

        // Remove is blocked while the preset still references the set — the
        // inline error names both the count and the preset names.
        let error = store
            .delete_rule_set(&ads.id)
            .expect_err("used set refuses removal");
        let StoreError::Invalid(fields) = error else {
            panic!("expected Invalid, got {error:?}");
        };
        assert!(
            fields.iter().any(|e| {
                e.message
                    == crate::preset::rule_set_in_use_message(
                        "ads",
                        std::slice::from_ref(&preset.name),
                    )
            }),
            "{fields:?}"
        );
        assert!(store.get_rule_set(&ads.id).expect("get works").is_some());

        // Detaching frees removal — and the detach surface is the preset's
        // rules, not the row link: the guard counts rule_set references that
        // presets still name.
        store
            .set_preset_rules(
                &preset.id,
                &[serde_json::json!({"action": "proxy", "ip_cidr": ["10.0.0.0/8"]})],
            )
            .expect("rule rewrite works");
        assert!(store
            .rule_sets_in_use(&preset.id)
            .expect("works")
            .is_empty());
        let file_name = store
            .delete_rule_set(&ads.id)
            .expect("delete works")
            .expect("file name comes back");
        assert_eq!(file_name, "ads.srs", "the file name can be unlinked");
        assert!(store.list_rule_sets().expect("list works").is_empty());
    }

    #[test]
    fn t62_attach_rejects_unstaged_rule_set() {
        // Attachment-time validation is at the row attach (not preset save):
        // the T-61 anchor saves a `ruleset:` ref unstaged — saving stays
        // tolerant, attaching to a row is where the loud name check lives.
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let row = store.add(valid_input("Row")).expect("row seeds");
        let preset = store
            .create_preset(
                "t62-attach",
                "manual",
                None,
                &[serde_json::json!({"action": "proxy", "rule_set": ["ghost-set"]})],
            )
            .expect("preset save stays tolerant");
        store
            .create_preset("plain", "manual", None, &[])
            .expect("empty preset seeds");
        let error = store
            .set_config_preset(&row.id, Some(&preset.id))
            .expect_err("unstaged ref must fail the attach");
        let StoreError::Invalid(fields) = error else {
            panic!("expected Invalid, got {error:?}");
        };
        let unstaged = fields
            .iter()
            .any(|e| e.message.contains("\"ghost-set\"") && e.message.contains("is not staged"));
        assert!(unstaged, "names the missing set: {fields:?}");
        assert!(
            store
                .get(&row.id)
                .expect("get works")
                .expect("kept")
                .preset_id
                .is_none(),
            "a refused attach leaves the row unattached"
        );

        // Staging the named set clears the gate.
        store
            .create_rule_set("ghost-set", "https://s.example.com/g.srs", 5)
            .expect("stage records");
        let attached = store
            .set_config_preset(&row.id, Some(&preset.id))
            .expect("attach works once the set is staged");
        assert_eq!(attached.preset_id.as_deref(), Some(preset.id.as_str()));

        // And attach refuses a named-but-empty reference too.
        let empty = store
            .create_preset(
                "t62-blank",
                "manual",
                None,
                &[serde_json::json!({"action": "direct", "rule_set": [" "]})],
            )
            .expect("tolerant save");
        let error = store
            .set_config_preset(&row.id, Some(&empty.id))
            .expect_err("blank ref must fail");
        assert!(error.to_string().contains("empty set name"), "{error}");
    }

    /// T-27: `library_sort` rides the meta family beside `library_columns`: a
    /// fresh store reads the default, every key/direction round-trips through a
    /// reopen, and the `none` choice persists as the untouched order.
    #[test]
    fn t77_sort_model_meta_round_trip() {
        let path = std::env::temp_dir().join("manyproxy-t77-sort.db");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
        {
            let mut store = Store::open_file(&path).expect("file store opens");
            assert_eq!(
                store.library_sort(),
                LibrarySort::default(),
                "a fresh store sorts by nothing"
            );
            assert_eq!(
                store.get_meta(Store::META_LIBRARY_SORT).expect("meta read"),
                None,
                "the default writes no meta row"
            );

            for key in LibrarySortKey::ALL {
                for direction in [SortDirection::Ascending, SortDirection::Descending] {
                    let sort = LibrarySort::new(key, direction);
                    store.set_library_sort(sort).expect("sort writes");
                    assert_eq!(store.library_sort(), sort, "{} round-trips", key.token());
                }
            }

            assert_eq!(
                LibrarySort::new(LibrarySortKey::Latency, SortDirection::Descending).to_json(),
                r#"{"key":"latency","direction":"desc"}"#,
                "the meta payload has one canonical shape"
            );
            for key in LibrarySortKey::ALL {
                assert_eq!(
                    LibrarySortKey::from_token(key.token()),
                    Some(key),
                    "{} parses back from its token",
                    key.token()
                );
            }

            // A hand-edited payload never wedges the list: the object as a whole
            // falls back to the default, and one unknown field keeps the other.
            store
                .set_meta(Store::META_LIBRARY_SORT, "not json at all")
                .expect("corrupt meta writes");
            assert_eq!(
                store.library_sort(),
                LibrarySort::default(),
                "garbage reads as the untouched order"
            );
            store
                .set_meta(
                    Store::META_LIBRARY_SORT,
                    r#"{"key":"nope","direction":"desc"}"#,
                )
                .expect("unknown key writes");
            assert_eq!(
                store.library_sort(),
                LibrarySort::new(LibrarySortKey::None, SortDirection::Descending),
                "an unknown key falls back to none, the direction survives"
            );
            store
                .set_meta(
                    Store::META_LIBRARY_SORT,
                    r#"{"key":"latency","direction":"sideways"}"#,
                )
                .expect("unknown direction writes");
            assert_eq!(
                store.library_sort(),
                LibrarySort::new(LibrarySortKey::Latency, SortDirection::Ascending),
                "an unknown direction falls back to ascending"
            );

            store
                .set_library_sort(LibrarySort::default())
                .expect("none writes");
        }

        let store = Store::open_file(&path).expect("reopen works");
        assert_eq!(
            store.library_sort(),
            LibrarySort::default(),
            "the none choice survives a restart"
        );
        assert_eq!(
            store
                .get_meta(Store::META_LIBRARY_SORT)
                .expect("meta read")
                .as_deref(),
            Some(r#"{"key":"none","direction":"asc"}"#),
            "none persists explicitly instead of clearing the key"
        );
        drop(store);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    /// T-31: `log_max_lines` rides the meta family with a `500..=50_000` window
    /// and today's 5,000-line default for an absent, corrupt, or reset key.
    #[test]
    fn t81_log_caps_round_trip_reset_and_default() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        assert_eq!(
            store.log_max_lines(),
            crate::health::MAX_LOG_RECORDS,
            "absent meta reads as today's 5,000-line ring"
        );
        assert_eq!(
            store
                .get_meta(Store::META_LOG_MAX_LINES)
                .expect("meta read"),
            None,
            "the default writes no meta row"
        );

        for cap in [500usize, 1_234, 50_000] {
            store.set_log_max_lines(cap).expect("cap writes");
            assert_eq!(store.log_max_lines(), cap, "{cap} round-trips");
        }

        store.set_log_max_lines(1).expect("low cap writes");
        assert_eq!(store.log_max_lines(), 500, "low caps clamp to 500");
        store
            .set_log_max_lines(usize::MAX)
            .expect("high cap writes");
        assert_eq!(store.log_max_lines(), 50_000, "high caps clamp to 50,000");

        store
            .set_meta(Store::META_LOG_MAX_LINES, "not-a-number")
            .expect("junk writes");
        assert_eq!(store.log_max_lines(), 5_000, "junk reads as the default");

        store
            .conn
            .execute(
                "DELETE FROM meta WHERE key = ?1",
                params![Store::META_LOG_MAX_LINES],
            )
            .expect("meta row clears");
        assert_eq!(store.log_max_lines(), 5_000, "a reset reads as the default");

        let path =
            std::env::temp_dir().join(format!("manyproxy-t81-cap-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
        {
            let mut store = Store::open_file(&path).expect("file store opens");
            assert_eq!(store.log_max_lines(), 5_000, "a fresh file store defaults");
            store.set_log_max_lines(777).expect("cap writes");
        }
        let store = Store::open_file(&path).expect("reopen works");
        assert_eq!(store.log_max_lines(), 777, "cap survives a restart");
        drop(store);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    /// T-32: the window rectangle rides the meta family as
    /// `{x,y,width,height}` logical points; an absent key writes nothing.
    #[test]
    fn t82_geometry_meta_round_trip() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        assert_eq!(
            store.window_geometry(),
            None,
            "absent meta reads as no geometry"
        );
        assert_eq!(
            store.get_meta(Store::META_GEOMETRY).expect("meta read"),
            None,
            "the default writes no meta row"
        );

        let geometry = WindowGeometry {
            x: 320.,
            y: 180.,
            width: 1280.,
            height: 900.,
        };
        store
            .set_window_geometry(geometry)
            .expect("geometry writes");
        assert_eq!(
            store.window_geometry(),
            Some(geometry),
            "geometry round-trips"
        );
        assert_eq!(
            store.window_geometry().expect("geometry reads").to_json(),
            r#"{"x":320,"y":180,"width":1280,"height":900}"#,
            "the payload is the plain JSON rectangle"
        );

        let path =
            std::env::temp_dir().join(format!("manyproxy-t82-geometry-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
        {
            let mut store = Store::open_file(&path).expect("file store opens");
            assert_eq!(store.window_geometry(), None, "a fresh file store defaults");
            store
                .set_window_geometry(geometry)
                .expect("geometry writes");
        }
        let store = Store::open_file(&path).expect("reopen works");
        assert_eq!(
            store.window_geometry(),
            Some(geometry),
            "geometry survives a restart"
        );
        drop(store);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    /// T-32: width/height never read back below the 880×620 window minimum and
    /// x/y land inside the -999..9999 sane box, whichever side wrote the row.
    #[test]
    fn t82_geometry_clamps_respect_mins() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");

        let tiny_far_away = WindowGeometry {
            x: -50_000.,
            y: 42_000.,
            width: 100.,
            height: 50.,
        };
        store
            .set_window_geometry(tiny_far_away)
            .expect("geometry writes");
        assert_eq!(
            store.window_geometry(),
            Some(WindowGeometry {
                x: WindowGeometry::POSITION_MIN,
                y: WindowGeometry::POSITION_MAX,
                width: WindowGeometry::MIN_WIDTH,
                height: WindowGeometry::MIN_HEIGHT,
            }),
            "the setter clamps before writing"
        );

        store
            .set_meta(
                Store::META_GEOMETRY,
                r#"{"x":-99999,"y":99999,"width":10,"height":10}"#,
            )
            .expect("hand-edited row writes");
        assert_eq!(
            store.window_geometry(),
            Some(WindowGeometry {
                x: WindowGeometry::POSITION_MIN,
                y: WindowGeometry::POSITION_MAX,
                width: WindowGeometry::MIN_WIDTH,
                height: WindowGeometry::MIN_HEIGHT,
            }),
            "the getter clamps a row that bypassed the setter"
        );

        store
            .set_meta(Store::META_GEOMETRY, r#"{"x":0,"y":0,"width":1280}"#)
            .expect("partial row writes");
        assert_eq!(
            store.window_geometry(),
            None,
            "a partial rectangle reads as no geometry"
        );

        store
            .conn
            .execute(
                "DELETE FROM meta WHERE key = ?1",
                params![Store::META_GEOMETRY],
            )
            .expect("meta row clears");
        store
            .set_window_geometry(WindowGeometry {
                x: f32::NAN,
                y: 0.,
                width: 1280.,
                height: 900.,
            })
            .expect("a non-finite write is refused, not stored");
        assert_eq!(
            store.get_meta(Store::META_GEOMETRY).expect("meta read"),
            None,
            "a non-finite rectangle leaves no row behind"
        );
    }

    // T-33: the bind-address list the per-row picker draws from.

    fn bind_host_seed(name: &str, bind_ip: &str, port: &str) -> NewConfig {
        NewConfig {
            name: name.to_string(),
            protocol: "vless".to_string(),
            params: Secret::default(),
            group: String::new(),
            bind_ip: bind_ip.to_string(),
            port: port.to_string(),
        }
    }

    fn stored_bind_hosts(store: &Store) -> serde_json::Value {
        let raw = store
            .get_meta(Store::META_BIND_HOSTS)
            .expect("meta read")
            .expect("seed wrote the key");
        serde_json::from_str(&raw).expect("extras decode")
    }

    #[test]
    fn t83_bind_hosts_meta_round_trip_and_seed() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store
            .add(bind_host_seed("Loop", "127.0.0.1", "10951"))
            .expect("seed works");
        store
            .add(bind_host_seed("Lan", "192.168.1.20", "10952"))
            .expect("seed works");

        assert_eq!(
            store.bind_hosts(),
            vec![
                "127.0.0.1".to_string(),
                "0.0.0.0".to_string(),
                "192.168.1.20".to_string(),
            ],
            "constants lead, then every row bind, loopback deduped"
        );
        assert_eq!(
            stored_bind_hosts(&store),
            serde_json::json!(["192.168.1.20"]),
            "constants are implicit — only extras are written"
        );

        store
            .add(bind_host_seed("Late", "10.9.9.9", "10953"))
            .expect("add works");
        assert!(
            !store.bind_hosts().contains(&"10.9.9.9".to_string()),
            "seeding happened once; a later row does not widen the list"
        );

        store.add_bind_host("fd00::7").expect("extra accepted");
        assert_eq!(
            store.bind_hosts(),
            vec![
                "127.0.0.1".to_string(),
                "0.0.0.0".to_string(),
                "192.168.1.20".to_string(),
                "fd00::7".to_string(),
            ],
            "extras keep insertion order after the constants"
        );
        assert_eq!(
            stored_bind_hosts(&store),
            serde_json::json!(["192.168.1.20", "fd00::7"]),
            "round trip keeps extras only"
        );

        store
            .set_meta(Store::META_BIND_HOSTS, "not json")
            .expect("meta write works");
        assert_eq!(
            store.bind_hosts(),
            vec!["127.0.0.1".to_string(), "0.0.0.0".to_string()],
            "a corrupt payload reads as no extras and never re-seeds"
        );
    }

    #[test]
    fn t83_constants_never_removable() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store.add_bind_host("192.168.1.20").expect("extra accepted");
        for constant in Store::BIND_HOST_CONSTANTS {
            let error = store
                .remove_bind_host(constant)
                .expect_err("constants cannot be removed");
            assert!(
                error.to_string().contains(constant),
                "refusal names {constant}: {error}"
            );
            let duplicate = store
                .add_bind_host(constant)
                .expect_err("constants are already listed");
            assert!(
                duplicate.to_string().contains("already in the list"),
                "duplicate refusal: {duplicate}"
            );
        }
        assert_eq!(
            store.bind_hosts(),
            vec![
                "127.0.0.1".to_string(),
                "0.0.0.0".to_string(),
                "192.168.1.20".to_string(),
            ],
            "constants stay present and first"
        );
        assert_eq!(
            store.remove_bind_host("192.168.1.20").expect("extra goes"),
            vec!["127.0.0.1".to_string(), "0.0.0.0".to_string()],
            "an unused extra removes"
        );
    }

    #[test]
    fn t83_remove_blocked_while_used_by_rows() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store
            .add(bind_host_seed("Alpha", "127.0.0.1", "10951"))
            .expect("seed works");
        store
            .add(bind_host_seed("Beta", "127.0.0.1", "10952"))
            .expect("seed works");
        store
            .add(bind_host_seed("Gamma", "10.1.2.3", "10953"))
            .expect("seed works");
        store.add_bind_host("10.9.9.9").expect("extra accepted");

        assert_eq!(
            store
                .bind_host_usage()
                .expect("usage reads")
                .get("127.0.0.1")
                .cloned(),
            Some(vec!["Alpha".to_string(), "Beta".to_string()]),
            "the used-hosts query names every row on the bind"
        );

        let error = store
            .remove_bind_host("127.0.0.1")
            .expect_err("constants never remove, even when unused");
        assert!(error.to_string().contains("built in"), "{error}");
        let error = store
            .remove_bind_host("10.1.2.3")
            .expect_err("a used extra cannot be removed");
        let text = error.to_string();
        assert!(
            text.contains("10.1.2.3") && text.contains("\"Gamma\""),
            "the refusal names the blocking row: {text}"
        );
        assert!(
            store.bind_hosts().contains(&"10.1.2.3".to_string()),
            "a refused remove leaves the list untouched"
        );

        assert_eq!(
            store
                .remove_bind_host("10.9.9.9")
                .expect("unused extra goes"),
            vec![
                "127.0.0.1".to_string(),
                "0.0.0.0".to_string(),
                "10.1.2.3".to_string(),
            ],
            "only the unused extra leaves"
        );
    }

    fn probe_temp(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "manyproxy-t08-{tag}-{}-{stamp}",
            std::process::id()
        ))
    }

    /// T-08: the probe is a write, not a metadata guess. A writable folder
    /// answers clean and the probe file never survives the round trip.
    #[test]
    fn t08_write_probe_writes_and_cleans_up() {
        let in_memory = Store::open_in_memory().expect("in-memory store opens");
        assert_eq!(
            in_memory.data_dir(),
            None,
            "an in-memory store has no data folder"
        );
        assert!(
            in_memory.write_probe().is_ok(),
            "an in-memory store probes clean — a false banner would be a defect"
        );

        let dir = probe_temp("probe");
        std::fs::create_dir_all(&dir).expect("temp dir created");
        let store = Store::open_file(&dir.join("library.db")).expect("file store opens");
        assert_eq!(
            store.data_dir().as_deref(),
            Some(dir.as_path()),
            "the probe folder is the database's own folder"
        );
        store
            .write_probe()
            .expect("a writable data folder probes clean");

        let leftovers: Vec<String> = std::fs::read_dir(&dir)
            .expect("temp dir reads")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with(".mp-write-probe"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "the probe removes its temp and renamed files: {leftovers:?}"
        );

        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// T-08: a folder that refuses a write must say so, with the path, and must
    /// not be inferred writable from anything else. A regular file is the
    /// cheapest reliable refusal on every platform.
    #[test]
    fn t08_write_probe_reports_a_refused_write_with_the_path() {
        let file = probe_temp("probe-file").with_extension("bin");
        std::fs::write(&file, b"not a folder").expect("temp file writes");
        let error = write_probe_in(&file).expect_err("a file path refuses the probe");
        assert!(
            error.contains(&file.display().to_string()),
            "the reason names the folder path: {error}"
        );
        assert!(
            file.is_file(),
            "a failed probe never deletes anything it did not create"
        );
        let _ = std::fs::remove_file(&file);
    }
}
