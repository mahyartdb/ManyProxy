//! Ticket 03: Throne-compatible import grammar pack.
//!
//! Pure parsers turning share links, JSON bodies, WireGuard conf files and
//! `throne://add` payloads into [`ImportedConfig`] records. This module does
//! no I/O, writes nothing, starts nothing, and never panics on malformed
//! input: [`import_text`] / [`import_file_content`] split a body into items
//! and collect per-item [`ImportFailure`]s (with the item index) instead of
//! failing the batch. [`commit_batch`] appends parsed items to the library as
//! disabled records in the chosen group and skips exact re-imports
//! (normalized identity: protocol + params).
//!
//! Field validation follows research 05
//! (`.planning/research/05-import-grammars.md`): base64 (standard + urlsafe,
//! padded or raw) with wrong-alphabet rejection, unknown schemes/commands,
//! bad host/port (hysteria2 omits port → 443), missing credentials
//! (uuid/password/keys/pbk-for-reality), invalid JSON, empty payloads, bad ss
//! plugins, duplicate tags (renamed `-2`, `-3`, …), non-UTF8 payloads.
//! Failure reasons never echo credentials.

use std::collections::{HashMap, HashSet};

use base64::Engine as _;

use crate::store::{NewConfig, Store, StoreError, DEFAULT_GROUP};

/// One parsed share link / outbound, ready to become a disabled record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedConfig {
    /// Remark/tag, or `{protocol}-{host}:{port}` when the link has none.
    pub name: String,
    /// Normalized outbound type (`vless`, `vmess`, `trojan`, `shadowsocks`,
    /// `hysteria2`, `hysteria`, `tuic`, `wireguard`, `amneziawg` (ticket 07:
    /// `awg`/`amnezia-wg` aliases fold here), `socks`, `anytls`,
    /// `http`, `https`, `naive`, `xray` (ticket 08: Xray-only VLESS),
    /// `custom-outbound` / `custom-config` (ticket 08: custom JSON rows),
    /// `extra-core` (ticket 08: user executable slot),
    /// `chain` (ticket 08: chained outbounds)).
    pub protocol: String,
    /// Deterministic JSON object with sing-box outbound field names
    /// (`type`, `server`, `server_port`, …).
    pub params: String,
    /// Display server.
    pub host: String,
    /// Display server port.
    pub port: u16,
}

/// Machine-readable failure codes (research 05 error taxonomy).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportErrorKind {
    Empty,
    UnknownScheme,
    BadBase64,
    NonUtf8,
    BadHost,
    BadPort,
    MissingCredential,
    InvalidJson,
    SsPlugin,
}

impl ImportErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ImportErrorKind::Empty => "empty",
            ImportErrorKind::UnknownScheme => "unknown-scheme",
            ImportErrorKind::BadBase64 => "bad-base64",
            ImportErrorKind::NonUtf8 => "non-utf8",
            ImportErrorKind::BadHost => "bad-host",
            ImportErrorKind::BadPort => "bad-port",
            ImportErrorKind::MissingCredential => "missing-credential",
            ImportErrorKind::InvalidJson => "invalid-json",
            ImportErrorKind::SsPlugin => "ss-plugin",
        }
    }

    /// Ticket 17: parse the [`ImportErrorKind::as_str`] form back (the store
    /// persists per-subscription item failures as JSON and decodes them for
    /// the failures view). Unknown input yields `None` (callers fall back,
    /// never crash).
    pub fn from_str(raw: &str) -> Option<Self> {
        match raw {
            "empty" => Some(ImportErrorKind::Empty),
            "unknown-scheme" => Some(ImportErrorKind::UnknownScheme),
            "bad-base64" => Some(ImportErrorKind::BadBase64),
            "non-utf8" => Some(ImportErrorKind::NonUtf8),
            "bad-host" => Some(ImportErrorKind::BadHost),
            "bad-port" => Some(ImportErrorKind::BadPort),
            "missing-credential" => Some(ImportErrorKind::MissingCredential),
            "invalid-json" => Some(ImportErrorKind::InvalidJson),
            "ss-plugin" => Some(ImportErrorKind::SsPlugin),
            _ => None,
        }
    }
}

/// One rejected item. `message` never contains credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportError {
    pub kind: ImportErrorKind,
    pub message: String,
}

impl ImportError {
    fn new(kind: ImportErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind.as_str(), self.message)
    }
}

/// One skipped item in a batch result, with its 0-based item index.
/// `label` identifies the item without secrets (share links redacted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportFailure {
    pub index: usize,
    pub label: String,
    pub reason: String,
    pub code: ImportErrorKind,
}

impl ImportFailure {
    fn new(index: usize, label: impl Into<String>, error: ImportError) -> Self {
        Self {
            index,
            label: label.into(),
            reason: error.to_string(),
            code: error.kind,
        }
    }
}

/// Outcome of parsing a body: successes plus per-item failures.
#[derive(Debug, Clone, Default)]
pub struct BatchResult {
    pub configs: Vec<ImportedConfig>,
    pub failures: Vec<ImportFailure>,
}

impl BatchResult {
    fn fail(index: usize, kind: ImportErrorKind, message: impl Into<String>) -> Self {
        Self {
            configs: Vec::new(),
            failures: vec![ImportFailure::new(
                index,
                "body",
                ImportError::new(kind, message),
            )],
        }
    }
}

/// Outcome of writing a batch into the store.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportOutcome {
    pub imported: usize,
    pub duplicates: usize,
    pub failures: Vec<ImportFailure>,
}

/// Ticket 14: render every batch failure as
/// `item {1-based index} ({label}): {reason}`, joined for notifications.
/// Labels arrive redacted from the import path (see [`redact_link`]); reasons
/// never echo credentials by construction of the error taxonomy.
pub fn format_import_failures(failures: &[ImportFailure]) -> String {
    failures
        .iter()
        .map(|failure| {
            format!(
                "item {} ({}): {}",
                failure.index + 1,
                failure.label,
                failure.reason
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

// ---------------------------------------------------------------------------
// Small codecs
// ---------------------------------------------------------------------------

/// Percent-decode `raw`. `plus_as_space` applies form semantics (query
/// values); fragments/userinfo keep `+` literally. Invalid `%` sequences are
/// kept verbatim (lenient: never a hard error).
fn percent_decode(raw: &str, plus_as_space: bool) -> String {
    fn hex_val(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = match (bytes.get(i + 1), bytes.get(i + 2)) {
                (Some(hi), Some(lo)) => match (hex_val(*hi), hex_val(*lo)) {
                    (Some(hi), Some(lo)) => Some(hi * 16 + lo),
                    _ => None,
                },
                _ => None,
            };
            if let Some(byte) = hex {
                out.push(byte);
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        } else if bytes[i] == b'+' && plus_as_space {
            out.push(b' ');
            i += 1;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Base64 with every alphabet Throne-compatible clients emit: standard and
/// urlsafe, padded or raw. A payload undecodable in all four is rejected
/// (covers the "wrong alphabet" failure mode).
fn decode_b64(text: &str) -> Result<Vec<u8>, ImportError> {
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.is_empty() {
        return Err(ImportError::new(
            ImportErrorKind::Empty,
            "empty payload".to_string(),
        ));
    }
    STANDARD
        .decode(&compact)
        .or_else(|_| STANDARD_NO_PAD.decode(&compact))
        .or_else(|_| URL_SAFE.decode(&compact))
        .or_else(|_| URL_SAFE_NO_PAD.decode(&compact))
        .map_err(|_| ImportError::new(ImportErrorKind::BadBase64, "bad base64 payload".to_string()))
}

fn decode_b64_utf8(text: &str) -> Result<String, ImportError> {
    let bytes = decode_b64(text)?;
    String::from_utf8(bytes)
        .map_err(|_| ImportError::new(ImportErrorKind::NonUtf8, "payload is not UTF-8".to_string()))
}

fn unknown_scheme(message: impl Into<String>) -> ImportError {
    ImportError::new(ImportErrorKind::UnknownScheme, message)
}

fn bad_host(message: impl Into<String>) -> ImportError {
    ImportError::new(ImportErrorKind::BadHost, message)
}

fn bad_port(message: impl Into<String>) -> ImportError {
    ImportError::new(ImportErrorKind::BadPort, message)
}

fn missing_credential(message: impl Into<String>) -> ImportError {
    ImportError::new(ImportErrorKind::MissingCredential, message)
}

fn invalid_json(message: impl Into<String>) -> ImportError {
    ImportError::new(ImportErrorKind::InvalidJson, message)
}

/// Strip a leading UTF-8 BOM, if present. `str::trim` leaves U+FEFF alone
/// (it is not White_Space), so BOM-prefixed bodies must strip it before any
/// `{`/`[`/section sniffing.
fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{FEFF}').unwrap_or(text)
}

// ---------------------------------------------------------------------------
// URI splitting (hand-rolled: full control over the error taxonomy)
// ---------------------------------------------------------------------------

/// Split `host[:port]` with bracketed-IPv6 support. Bare IPv6 without
/// brackets is rejected (taxonomy: bad host with IPv6 rules).
fn split_host_port(text: &str) -> Result<(String, Option<String>), ImportError> {
    if text.is_empty() {
        return Err(bad_host("missing host"));
    }
    if let Some(rest) = text.strip_prefix('[') {
        let end = rest
            .find(']')
            .ok_or_else(|| bad_host("bad IPv6 host (missing ']')"))?;
        let host = rest[..end].to_string();
        if host.trim().is_empty() {
            return Err(bad_host("missing host"));
        }
        let after = &rest[end + 1..];
        if after.is_empty() {
            return Ok((host, None));
        }
        let port = after
            .strip_prefix(':')
            .ok_or_else(|| bad_host("bad host"))?;
        if port.is_empty() {
            return Err(bad_port("bad port ''"));
        }
        return Ok((host, Some(port.to_string())));
    }
    match text.rfind(':') {
        Some(i) => {
            let (host, port) = (&text[..i], &text[i + 1..]);
            if host.contains(':') {
                return Err(bad_host("bare IPv6 must use [brackets]"));
            }
            if host.is_empty() || host.chars().any(char::is_whitespace) {
                return Err(bad_host("missing host"));
            }
            if port.is_empty() {
                return Err(bad_port("bad port ''"));
            }
            Ok((host.to_string(), Some(port.to_string())))
        }
        None => {
            if text.chars().any(|c| c.is_whitespace() || c.is_control()) {
                return Err(bad_host("bad host"));
            }
            Ok((text.to_string(), None))
        }
    }
}

/// Port text → 1..=65535. Hysteria-style multi-port (`mport`/comma lists)
/// takes the first entry (first-wins).
fn parse_port(text: &str) -> Result<u16, ImportError> {
    let first = text.split([',', ';']).next().unwrap_or("").trim();
    match first.parse::<u32>() {
        Ok(port) if (1..=65_535).contains(&port) => Ok(port as u16),
        _ => Err(bad_port(format!("bad port '{text}'"))),
    }
}

fn require_port(port_text: Option<&str>) -> Result<u16, ImportError> {
    match port_text {
        Some(text) => parse_port(text),
        None => Err(bad_port("missing port")),
    }
}

/// Generic `scheme://…` split for grammars whose payload never carries `/`
/// inside base64 (uuid/password userinfo forms). vmess/ss/throne use custom
/// entry points below because their base64 blobs may contain `/`.
struct UriParts {
    userinfo: Option<String>,
    host: String,
    port_text: Option<String>,
    query: HashMap<String, String>,
    fragment: String,
}

fn split_uri(scheme: &str, rest: &str) -> Result<UriParts, ImportError> {
    let (before_frag, fragment) = match rest.find('#') {
        Some(i) => (&rest[..i], rest[i + 1..].to_string()),
        None => (rest, String::new()),
    };
    let (before_query, query_raw) = match before_frag.find('?') {
        Some(i) => (&before_frag[..i], &before_frag[i + 1..]),
        None => (before_frag, ""),
    };
    let authority = match before_query.find('/') {
        Some(i) => &before_query[..i],
        None => before_query,
    };
    let (userinfo, host_port) = match authority.rfind('@') {
        Some(i) => (Some(authority[..i].to_string()), &authority[i + 1..]),
        None => (None, authority),
    };
    let (host, port_text) = split_host_port(host_port).map_err(|mut error| {
        if scheme == "hysteria2" || scheme == "hy2" {
            // Port omission is legal for hy2 (→ 443); other host errors stand.
            let _ = &mut error;
        }
        error
    })?;
    Ok(UriParts {
        userinfo,
        host,
        port_text,
        query: query_map(query_raw),
        fragment,
    })
}

/// Query pairs, percent-decoded with form (`+` → space) semantics, unordered
/// with unique names (first wins), case-sensitive names.
fn query_map(raw: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if raw.is_empty() {
        return map;
    }
    for pair in raw.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = match pair.find('=') {
            Some(i) => (&pair[..i], &pair[i + 1..]),
            None => (pair, ""),
        };
        map.entry(percent_decode(key, true))
            .or_insert_with(|| percent_decode(value, true));
    }
    map
}

fn display_name(fragment: &str, protocol: &str, host: &str, port: u16) -> String {
    let remark = percent_decode(fragment, false);
    if remark.trim().is_empty() {
        format!("{protocol}-{host}:{port}")
    } else {
        remark
    }
}

// ---------------------------------------------------------------------------
// Params builders (sing-box outbound field names, deterministic key order)
// ---------------------------------------------------------------------------

type Params = serde_json::Map<String, serde_json::Value>;

fn base_params(protocol: &str, host: &str, port: u16) -> Params {
    let mut params = Params::new();
    params.insert(
        "type".to_string(),
        serde_json::Value::String(protocol.to_string()),
    );
    params.insert(
        "server".to_string(),
        serde_json::Value::String(host.to_string()),
    );
    params.insert(
        "server_port".to_string(),
        serde_json::Value::Number(port.into()),
    );
    params
}

fn insert_str(params: &mut Params, key: &str, value: String) {
    params.insert(key.to_string(), serde_json::Value::String(value));
}

fn insert_opt(params: &mut Params, query: &HashMap<String, String>, key: &str) {
    if let Some(value) = query.get(key) {
        if !value.trim().is_empty() {
            insert_str(params, key, value.clone());
        }
    }
}

/// Remaining query keys after the consumed ones, passed through verbatim.
fn insert_extras(params: &mut Params, query: &HashMap<String, String>, consumed: &[&str]) {
    for (key, value) in query {
        if !consumed.contains(&key.as_str()) {
            insert_str(params, key, value.clone());
        }
    }
}

fn finish(params: Params) -> String {
    serde_json::Value::Object(params).to_string()
}

/// VLESS transport names; unknown values fall back to `tcp` (research 05).
fn normalize_network(raw: Option<&String>) -> String {
    const KNOWN: [&str; 9] = [
        "tcp",
        "raw",
        "kcp",
        "ws",
        "http",
        "grpc",
        "httpupgrade",
        "xhttp",
        "quic",
    ];
    match raw {
        Some(value) if KNOWN.contains(&value.as_str()) => value.clone(),
        _ => "tcp".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Per-protocol parsers
// ---------------------------------------------------------------------------

/// VLESS / VMess-URI / Trojan userinfo forms:
/// `proto://credential@host:port?params#remark`.
fn parse_vless_like(
    protocol: &str,
    parts: &UriParts,
    credential_name: &str,
) -> Result<ImportedConfig, ImportError> {
    let credential = percent_decode(parts.userinfo.as_deref().unwrap_or(""), false);
    if credential.trim().is_empty() {
        return Err(missing_credential(format!("missing {credential_name}")));
    }
    let port = require_port(parts.port_text.as_deref())?;
    let query = &parts.query;
    let default_security = if protocol == "trojan" { "tls" } else { "none" };
    let security = query
        .get("security")
        .cloned()
        .unwrap_or_else(|| default_security.to_string());
    if security.eq_ignore_ascii_case("reality")
        && !query.get("pbk").is_some_and(|pbk| !pbk.trim().is_empty())
    {
        return Err(missing_credential("missing pbk (required for reality)"));
    }
    let sni = query
        .get("sni")
        .cloned()
        .unwrap_or_else(|| parts.host.clone());

    let mut params = base_params(protocol, &parts.host, port);
    match protocol {
        "vless" | "xray" => {
            insert_str(&mut params, "uuid", credential);
            insert_str(
                &mut params,
                "encryption",
                query
                    .get("encryption")
                    .cloned()
                    .unwrap_or_else(|| "none".to_string()),
            );
        }
        "trojan" => {
            insert_str(&mut params, "password", credential);
        }
        _ => {
            // New-style vmess URI: `encryption` carries the security set.
            insert_str(&mut params, "uuid", credential);
            insert_str(
                &mut params,
                "encryption",
                query
                    .get("encryption")
                    .cloned()
                    .unwrap_or_else(|| "auto".to_string()),
            );
        }
    }
    insert_str(&mut params, "network", normalize_network(query.get("type")));
    insert_str(&mut params, "security", security);
    insert_str(&mut params, "sni", sni);
    insert_opt(&mut params, query, "flow");
    insert_opt(&mut params, query, "fp");
    insert_opt(&mut params, query, "alpn");
    insert_opt(&mut params, query, "pbk");
    insert_opt(&mut params, query, "sid");
    insert_extras(
        &mut params,
        query,
        &[
            "security",
            "sni",
            "type",
            "encryption",
            "flow",
            "fp",
            "alpn",
            "pbk",
            "sid",
        ],
    );
    Ok(ImportedConfig {
        name: display_name(&parts.fragment, protocol, &parts.host, port),
        protocol: protocol.to_string(),
        params: finish(params),
        host: parts.host.clone(),
        port,
    })
}

/// Legacy `vmess://base64(json)` (dominant form): `add`/`port`/`id` required.
fn parse_vmess_legacy(payload: &str) -> Result<ImportedConfig, ImportError> {
    let text = decode_b64_utf8(payload)?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| invalid_json(format!("invalid vmess JSON: {error}")))?;
    let obj = value
        .as_object()
        .ok_or_else(|| invalid_json("invalid vmess JSON: expected an object"))?;
    fn field(obj: &Params, key: &str) -> Option<String> {
        obj.get(key).and_then(|value| match value {
            serde_json::Value::String(text) => Some(text.clone()),
            serde_json::Value::Number(number) => Some(number.to_string()),
            _ => None,
        })
    }
    let host = field(obj, "add")
        .filter(|h| !h.trim().is_empty())
        .ok_or_else(|| {
            ImportError::new(ImportErrorKind::BadHost, "vmess link is missing \"add\"")
        })?;
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(&host)
        .to_string();
    let port_raw = field(obj, "port").unwrap_or_default();
    let port = parse_port(&port_raw)
        .map_err(|_| bad_port(format!("vmess link has a bad \"port\" ({port_raw})")))?;
    let uuid = field(obj, "id")
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| missing_credential("vmess link is missing \"id\""))?;
    let name = field(obj, "ps").unwrap_or_default();
    let name = if name.trim().is_empty() {
        format!("vmess-{host}:{port}")
    } else {
        name
    };

    let mut params = base_params("vmess", &host, port);
    insert_str(&mut params, "uuid", uuid);
    insert_str(
        &mut params,
        "alter_id",
        field(obj, "aid").unwrap_or_else(|| "0".to_string()),
    );
    insert_str(
        &mut params,
        "security",
        field(obj, "scy").unwrap_or_else(|| "auto".to_string()),
    );
    insert_str(
        &mut params,
        "network",
        normalize_network(field(obj, "net").as_ref()),
    );
    params.insert(
        "tls".to_string(),
        serde_json::Value::Bool(field(obj, "tls").as_deref() == Some("tls")),
    );
    for key in ["sni", "fp", "alpn", "path", "host"] {
        if let Some(value) = field(obj, key) {
            if !value.trim().is_empty() {
                insert_str(&mut params, key, value);
            }
        }
    }
    if let Some(header) = field(obj, "type") {
        if !header.trim().is_empty() {
            insert_str(&mut params, "header_type", header);
        }
    }
    Ok(ImportedConfig {
        name,
        protocol: "vmess".to_string(),
        params: finish(params),
        host,
        port,
    })
}

/// Shadowsocks triple grammar: legacy `ss://base64(method:password@host:port)`,
/// SIP002 `ss://userinfo@host:port?plugin=`, modern-split
/// `ss://base64(method:password)@host:port`.
fn parse_ss_rest(rest: &str) -> Result<ImportedConfig, ImportError> {
    let (before_frag, fragment) = match rest.find('#') {
        Some(i) => (&rest[..i], rest[i + 1..].to_string()),
        None => (rest, String::new()),
    };
    let (payload, query_raw) = match before_frag.find('?') {
        Some(i) => (&before_frag[..i], &before_frag[i + 1..]),
        None => (before_frag, ""),
    };
    let query = query_map(query_raw);
    if let Some(plugin) = query.get("plugin") {
        if plugin.trim().is_empty() {
            return Err(ImportError::new(
                ImportErrorKind::SsPlugin,
                "bad ss plugin (empty value)",
            ));
        }
    }
    let (method, password, host, port) = match payload.rfind('@') {
        Some(i) => {
            let (userinfo_raw, host_port) = (&payload[..i], &payload[i + 1..]);
            let (method, password) = if let Some(j) = userinfo_raw.find(':') {
                // Plain `method:password` (AEAD-2022 style).
                (
                    percent_decode(&userinfo_raw[..j], false),
                    percent_decode(&userinfo_raw[j + 1..], false),
                )
            } else {
                // `base64(method:password)`.
                let text = decode_b64_utf8(userinfo_raw)?;
                let j = text
                    .find(':')
                    .ok_or_else(|| missing_credential("ss link is missing password"))?;
                (text[..j].to_string(), text[j + 1..].to_string())
            };
            // Real-world clients append a trailing slash to the authority
            // (`ss://method:pw@host:8388/`); tolerate it like the legacy
            // branch below (research 05: trailing-slash ss → trim/lenient).
            let (host, port_text) = split_host_port(host_port.trim_end_matches('/'))?;
            (method, password, host, require_port(port_text.as_deref())?)
        }
        None => {
            // Legacy: the whole authority is base64.
            let payload = payload.trim_end_matches('/');
            let text = decode_b64_utf8(payload)?;
            let i = text
                .rfind('@')
                .ok_or_else(|| bad_host("bad ss link (missing host)"))?;
            let (userinfo, host_port) = (&text[..i], &text[i + 1..]);
            let j = userinfo
                .find(':')
                .ok_or_else(|| missing_credential("ss link is missing password"))?;
            let (host, port_text) = split_host_port(host_port)?;
            (
                userinfo[..j].to_string(),
                userinfo[j + 1..].to_string(),
                host,
                require_port(port_text.as_deref())?,
            )
        }
    };
    if method.trim().is_empty() {
        return Err(missing_credential("ss link is missing method"));
    }
    if password.is_empty() {
        return Err(missing_credential("ss link is missing password"));
    }
    let mut params = base_params("shadowsocks", &host, port);
    insert_str(&mut params, "method", method);
    insert_str(&mut params, "password", password);
    insert_opt(&mut params, &query, "plugin");
    insert_extras(&mut params, &query, &["plugin"]);
    Ok(ImportedConfig {
        name: display_name(&fragment, "shadowsocks", &host, port),
        protocol: "shadowsocks".to_string(),
        params: finish(params),
        host,
        port,
    })
}

/// Hysteria2 (`hysteria2://`, `hy2://`, `hysteria2+realm://`): auth required,
/// port omitted → 443, multi-port first-wins.
fn parse_hy2(scheme: &str, parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    let auth = percent_decode(parts.userinfo.as_deref().unwrap_or(""), false);
    if auth.trim().is_empty() {
        return Err(missing_credential(
            "hysteria2 link is missing auth password",
        ));
    }
    let port = match parts.port_text.as_deref() {
        Some(text) => parse_port(text)?,
        None => 443,
    };
    let query = &parts.query;
    let sni = query
        .get("sni")
        .cloned()
        .unwrap_or_else(|| parts.host.clone());
    let mut params = base_params("hysteria2", &parts.host, port);
    insert_str(&mut params, "password", auth);
    insert_str(&mut params, "sni", sni);
    if scheme == "hysteria2+realm" {
        insert_str(&mut params, "variant", "realm".to_string());
    }
    insert_extras(&mut params, query, &["sni"]);
    Ok(ImportedConfig {
        name: display_name(&parts.fragment, "hysteria2", &parts.host, port),
        protocol: "hysteria2".to_string(),
        params: finish(params),
        host: parts.host.clone(),
        port,
    })
}

/// Legacy Hysteria v1 (`hysteria://auth@host:port?…`): auth and port required.
fn parse_hy1(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    let auth = percent_decode(parts.userinfo.as_deref().unwrap_or(""), false);
    if auth.trim().is_empty() {
        return Err(missing_credential("hysteria link is missing auth"));
    }
    let port = require_port(parts.port_text.as_deref())?;
    let mut params = base_params("hysteria", &parts.host, port);
    insert_str(&mut params, "auth", auth);
    insert_extras(&mut params, &parts.query, &[]);
    Ok(ImportedConfig {
        name: display_name(&parts.fragment, "hysteria", &parts.host, port),
        protocol: "hysteria".to_string(),
        params: finish(params),
        host: parts.host.clone(),
        port,
    })
}

/// TUIC (`tuic://uuid:password@host:port?…`): userinfo splits on the first
/// colon so passwords may contain `:`.
fn parse_tuic(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    let raw = parts.userinfo.as_deref().unwrap_or("");
    let i = raw
        .find(':')
        .ok_or_else(|| missing_credential("tuic link is missing password"))?;
    let (uuid, password) = (
        percent_decode(&raw[..i], false),
        percent_decode(&raw[i + 1..], false),
    );
    if uuid.trim().is_empty() {
        return Err(missing_credential("tuic link is missing uuid"));
    }
    if password.is_empty() {
        return Err(missing_credential("tuic link is missing password"));
    }
    let port = require_port(parts.port_text.as_deref())?;
    let mut params = base_params("tuic", &parts.host, port);
    insert_str(&mut params, "uuid", uuid);
    insert_str(&mut params, "password", password);
    insert_extras(&mut params, &parts.query, &[]);
    Ok(ImportedConfig {
        name: display_name(&parts.fragment, "tuic", &parts.host, port),
        protocol: "tuic".to_string(),
        params: finish(params),
        host: parts.host.clone(),
        port,
    })
}

/// WireGuard URI: private key in userinfo, `publickey` required.
fn parse_wg_uri(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    let private_key = percent_decode(parts.userinfo.as_deref().unwrap_or(""), false);
    if private_key.trim().is_empty() {
        return Err(missing_credential("wireguard link is missing private key"));
    }
    let query = &parts.query;
    let public_key = query.get("publickey").cloned().unwrap_or_default();
    if public_key.trim().is_empty() {
        return Err(missing_credential("wireguard link is missing publickey"));
    }
    let port = require_port(parts.port_text.as_deref())?;
    let mut params = base_params("wireguard", &parts.host, port);
    insert_str(&mut params, "private_key", private_key);
    insert_str(&mut params, "public_key", public_key);
    insert_extras(&mut params, query, &["publickey"]);
    Ok(ImportedConfig {
        name: display_name(&parts.fragment, "wireguard", &parts.host, port),
        protocol: "wireguard".to_string(),
        params: finish(params),
        host: parts.host.clone(),
        port,
    })
}

/// Ticket 07: AmneziaWG-variant URI (`awg://`, `amneziawg://`,
/// `amnezia-wg://`): same authority shape as WireGuard (private key in
/// userinfo, `publickey` required); every other query key — `awg_version`,
/// `jc`/`jmin`/`jmax`, `s/h/i` ranges, 3.x additions — is preserved verbatim
/// into params for the fork-core version-matrix validator (engine.rs).
fn parse_awg_uri(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    let private_key = percent_decode(parts.userinfo.as_deref().unwrap_or(""), false);
    if private_key.trim().is_empty() {
        return Err(missing_credential("amneziawg link is missing private key"));
    }
    let query = &parts.query;
    let public_key = query.get("publickey").cloned().unwrap_or_default();
    if public_key.trim().is_empty() {
        return Err(missing_credential("amneziawg link is missing publickey"));
    }
    let port = require_port(parts.port_text.as_deref())?;
    let mut params = base_params("amneziawg", &parts.host, port);
    insert_str(&mut params, "private_key", private_key);
    insert_str(&mut params, "public_key", public_key);
    insert_extras(&mut params, query, &["publickey"]);
    Ok(ImportedConfig {
        name: display_name(&parts.fragment, "amneziawg", &parts.host, port),
        protocol: "amneziawg".to_string(),
        params: finish(params),
        host: parts.host.clone(),
        port,
    })
}

fn parse_socks_like(
    protocol: &str,
    parts: &UriParts,
    default_port: Option<u16>,
) -> Result<ImportedConfig, ImportError> {
    let port = match (parts.port_text.as_deref(), default_port) {
        (Some(text), _) => parse_port(text)?,
        (None, Some(fallback)) => fallback,
        (None, None) => return Err(bad_port("missing port")),
    };
    let mut params = base_params(protocol, &parts.host, port);
    if let Some(raw) = parts.userinfo.as_deref() {
        match raw.find(':') {
            Some(i) => {
                insert_str(&mut params, "username", percent_decode(&raw[..i], false));
                insert_str(
                    &mut params,
                    "password",
                    percent_decode(&raw[i + 1..], false),
                );
            }
            None if !raw.is_empty() => {
                insert_str(&mut params, "username", percent_decode(raw, false));
            }
            _ => {}
        }
    }
    if protocol == "socks" {
        insert_str(&mut params, "version", "5".to_string());
    }
    insert_extras(&mut params, &parts.query, &[]);
    Ok(ImportedConfig {
        name: display_name(&parts.fragment, protocol, &parts.host, port),
        protocol: protocol.to_string(),
        params: finish(params),
        host: parts.host.clone(),
        port,
    })
}

fn parse_anytls(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    let password = percent_decode(parts.userinfo.as_deref().unwrap_or(""), false);
    if password.is_empty() {
        return Err(missing_credential("anytls link is missing password"));
    }
    let port = require_port(parts.port_text.as_deref())?;
    let mut params = base_params("anytls", &parts.host, port);
    insert_str(&mut params, "password", password);
    insert_extras(&mut params, &parts.query, &[]);
    Ok(ImportedConfig {
        name: display_name(&parts.fragment, "anytls", &parts.host, port),
        protocol: "anytls".to_string(),
        params: finish(params),
        host: parts.host.clone(),
        port,
    })
}

fn parse_naive(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    let raw = parts.userinfo.as_deref().unwrap_or("");
    let i = raw
        .find(':')
        .ok_or_else(|| missing_credential("naive link is missing password"))?;
    let (username, password) = (
        percent_decode(&raw[..i], false),
        percent_decode(&raw[i + 1..], false),
    );
    if username.trim().is_empty() || password.is_empty() {
        return Err(missing_credential("naive link is missing credentials"));
    }
    let port = match parts.port_text.as_deref() {
        Some(text) => parse_port(text)?,
        None => 443,
    };
    let mut params = base_params("naive", &parts.host, port);
    insert_str(&mut params, "username", username);
    insert_str(&mut params, "password", password);
    insert_extras(&mut params, &parts.query, &[]);
    Ok(ImportedConfig {
        name: display_name(&parts.fragment, "naive", &parts.host, port),
        protocol: "naive".to_string(),
        params: finish(params),
        host: parts.host.clone(),
        port,
    })
}

/// `throne://add/<base64-outbound-json>` (case-insensitive command, single
/// outbound, no confirm). Query/fragment bytes corrupt the payload and fail
/// as bad base64; unknown commands are skipped+counted.
fn parse_throne_rest(rest: &str) -> Result<ImportedConfig, ImportError> {
    if rest.trim().is_empty() {
        return Err(ImportError::new(
            ImportErrorKind::Empty,
            "empty payload".to_string(),
        ));
    }
    let (command, payload) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, ""),
    };
    if !command.eq_ignore_ascii_case("add") {
        return Err(unknown_scheme(format!(
            "unknown throne command '{command}'"
        )));
    }
    if payload.trim().is_empty() {
        return Err(ImportError::new(
            ImportErrorKind::Empty,
            "throne link is missing its payload",
        ));
    }
    let text = decode_b64_utf8(payload)?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| invalid_json(format!("invalid throne JSON: {error}")))?;
    json_outbound(0, &value)
}

// ---------------------------------------------------------------------------
// JSON bodies: sing-box outbounds arrays, full configs, Xray arrays
// ---------------------------------------------------------------------------

/// Normalize an outbound `type`/`protocol` value to a library protocol name.
fn normalize_outbound_type(raw: &str) -> Result<String, ImportError> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "vless" | "vmess" | "trojan" | "shadowsocks" | "hysteria2" | "hysteria" | "tuic"
        | "wireguard" | "amneziawg" | "socks" | "anytls" | "http" | "https" | "naive" => Ok(lower),
        "ss" => Ok("shadowsocks".to_string()),
        "socks5" => Ok("socks".to_string()),
        // Ticket 07: AWG aliases fold to the canonical stored form.
        "awg" | "amnezia-wg" => Ok("amneziawg".to_string()),
        // Ticket 08: Xray/custom/extra/chain rows keep their kind so the
        // engine can route them to the sidecar bridge, the owning core,
        // the user slot, or the chain builder.
        "xray" | "xray-vless" | "xray_vless" | "vless-xray" => Ok("xray".to_string()),
        "custom"
        | "custom-out"
        | "custom-outbound"
        | "custom_outbound"
        | "custom-outbound-json" => Ok("custom-outbound".to_string()),
        "custom-config" | "custom_config" | "custom-full" | "custom_full"
        | "custom-full-config" => Ok("custom-config".to_string()),
        "extra" | "extra-core" | "extra_core" | "extracore" => Ok("extra-core".to_string()),
        "chain" | "chained" | "chain-outbound" | "chain_outbound" => Ok("chain".to_string()),
        "" => Err(invalid_json("item is missing outbound \"type\"")),
        other => Err(unknown_scheme(format!("unknown outbound type '{other}'"))),
    }
}

fn json_string(obj: &Params, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| obj.get(*key))
        .and_then(|value| match value {
            serde_json::Value::String(text) => {
                if text.trim().is_empty() {
                    None
                } else {
                    Some(text.clone())
                }
            }
            serde_json::Value::Number(number) => Some(number.to_string()),
            _ => None,
        })
}

fn json_port(obj: &Params) -> Result<u16, ImportError> {
    match obj.get("server_port").or_else(|| obj.get("port")) {
        Some(serde_json::Value::Number(number)) => number
            .as_u64()
            .filter(|port| (1..=65_535).contains(port))
            .map(|port| port as u16)
            .ok_or_else(|| bad_port("item has a bad \"server_port\"")),
        Some(serde_json::Value::String(text)) => parse_port(text)
            .map_err(|_| bad_port(format!("item has a bad \"server_port\" ({text})"))),
        _ => Err(bad_port("item is missing \"server_port\"")),
    }
}

/// Convert one outbound object (sing-box `type` or Xray `protocol`).
fn json_outbound(index: usize, value: &serde_json::Value) -> Result<ImportedConfig, ImportError> {
    let obj = value
        .as_object()
        .ok_or_else(|| invalid_json("item is not a JSON object"))?;
    let type_raw = obj
        .get("type")
        .or_else(|| obj.get("protocol"))
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let protocol = normalize_outbound_type(type_raw)?;
    let host = json_string(obj, &["server", "address"])
        .ok_or_else(|| ImportError::new(ImportErrorKind::BadHost, "item is missing \"server\""))?;
    if host.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(bad_host("item has a bad \"server\""));
    }
    let port = json_port(obj)?;
    let name = json_string(obj, &["tag", "remark", "remarks", "ps", "email", "name"])
        .unwrap_or_else(|| format!("{protocol}-{host}:{port}"));
    let mut params = obj.clone();
    params.insert(
        "type".to_string(),
        serde_json::Value::String(protocol.clone()),
    );
    let _ = index;
    Ok(ImportedConfig {
        name,
        protocol,
        params: finish(params),
        host,
        port,
    })
}

/// Limited SIP008 server item: explicit `type`/`protocol` goes through the
/// standard outbound path; a typeless item with `method` + `password` (+
/// `server`/`server_port`) is treated as Shadowsocks. Anything else is a
/// per-item failure so refresh skips + counts it.
fn sip008_server(value: &serde_json::Value) -> Result<ImportedConfig, ImportError> {
    let obj = value
        .as_object()
        .ok_or_else(|| invalid_json("item is not a JSON object"))?;
    if obj.contains_key("type") || obj.contains_key("protocol") {
        return json_outbound(0, value);
    }
    let mut synthesized = obj.clone();
    synthesized.insert(
        "type".to_string(),
        serde_json::Value::String("shadowsocks".to_string()),
    );
    if !synthesized.contains_key("server") && synthesized.contains_key("address") {
        if let Some(server) = synthesized.get("address").cloned() {
            synthesized.insert("server".to_string(), server);
        }
    }
    if !synthesized.contains_key("server_port") && synthesized.contains_key("port") {
        if let Some(port) = synthesized.get("port").cloned() {
            synthesized.insert("server_port".to_string(), port);
        }
    }
    let has_method = synthesized
        .get("method")
        .and_then(|value| value.as_str())
        .is_some_and(|method| !method.trim().is_empty());
    let has_password = synthesized
        .get("password")
        .and_then(|value| value.as_str())
        .is_some_and(|password| !password.is_empty());
    if !has_method || !has_password {
        return Err(invalid_json(
            "SIP008 item needs an outbound \"type\" or Shadowsocks \"method\"+\"password\"",
        ));
    }
    json_outbound(0, &serde_json::Value::Object(synthesized))
}

/// Duplicate tags inside one JSON body are deduped with `-2`, `-3`, … suffixes.
fn dedupe_names(configs: &mut [ImportedConfig]) {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for config in configs {
        let count = seen.entry(config.name.clone()).or_insert(0);
        *count += 1;
        if *count > 1 {
            config.name = format!("{}-{count}", config.name);
        }
    }
}

fn import_json_value(value: &serde_json::Value) -> BatchResult {
    // SIP008-style `{"version": N, "servers": [...]}` (limited): items with
    // an explicit type go through the standard outbound path; method+password
    // items without a type are treated as Shadowsocks; anything else is a
    // per-item failure (skip + count).
    if let serde_json::Value::Object(map) = value {
        let is_outbound_shape = map.contains_key("outbounds")
            || map.contains_key("type")
            || map.contains_key("protocol");
        if !is_outbound_shape {
            if let Some(servers) = map.get("servers") {
                let mut out = BatchResult::default();
                match servers {
                    serde_json::Value::Array(items) => {
                        for (index, item) in items.iter().enumerate() {
                            let label = item
                                .get("tag")
                                .or_else(|| item.get("remark"))
                                .or_else(|| item.get("remarks"))
                                .and_then(|value| value.as_str())
                                .unwrap_or("sip008");
                            match sip008_server(item) {
                                Ok(config) => out.configs.push(config),
                                Err(error) => {
                                    out.failures.push(ImportFailure::new(index, label, error))
                                }
                            }
                        }
                        dedupe_names(&mut out.configs);
                        return out;
                    }
                    _ => {
                        return BatchResult::fail(
                            0,
                            ImportErrorKind::InvalidJson,
                            "invalid JSON: \"servers\" is not an array",
                        );
                    }
                }
            }
        }
    }
    let mut out = BatchResult::default();
    let items: Vec<(usize, &serde_json::Value)> = match value {
        serde_json::Value::Array(items) => items.iter().enumerate().collect(),
        serde_json::Value::Object(map) => match map.get("outbounds") {
            Some(serde_json::Value::Array(items)) => items.iter().enumerate().collect(),
            Some(_) => {
                return BatchResult::fail(
                    0,
                    ImportErrorKind::InvalidJson,
                    "invalid JSON: \"outbounds\" is not an array",
                );
            }
            None => {
                if map.contains_key("type") || map.contains_key("protocol") {
                    vec![(0, value)]
                } else {
                    return BatchResult::fail(
                        0,
                        ImportErrorKind::InvalidJson,
                        "unsupported JSON: expected an outbounds array or a full config with \"outbounds\"",
                    );
                }
            }
        },
        _ => {
            return BatchResult::fail(
                0,
                ImportErrorKind::InvalidJson,
                "unsupported JSON: expected an outbounds array or a full config with \"outbounds\"",
            );
        }
    };
    for (index, item) in items {
        let label = item
            .get("tag")
            .or_else(|| item.get("remark"))
            .and_then(|value| value.as_str())
            .unwrap_or("json");
        match json_outbound(index, item) {
            Ok(config) => out.configs.push(config),
            Err(error) => out.failures.push(ImportFailure::new(index, label, error)),
        }
    }
    dedupe_names(&mut out.configs);
    out
}

// ---------------------------------------------------------------------------
// WireGuard `.conf` files (one config per `[Peer]`)
// ---------------------------------------------------------------------------

fn conf_value(section: &HashMap<String, String>, key: &str) -> String {
    section.get(key).cloned().unwrap_or_default()
}

fn import_wg_conf(content: &str, stem: &str) -> BatchResult {
    // Ticket 16: each peer keeps the Interface snapshot that precedes it, so
    // concatenated multi-file bodies never bleed one file's PrivateKey (or
    // address/dns/mtu fallback) into another file's peers.
    let mut active_iface: HashMap<String, String> = HashMap::new();
    let mut peers: Vec<(HashMap<String, String>, HashMap<String, String>)> = Vec::new();
    let mut current: Option<(bool, HashMap<String, String>)> = None;
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            if let Some((is_peer, section)) = current.take() {
                if is_peer {
                    peers.push((section, active_iface.clone()));
                } else {
                    active_iface = section;
                }
            }
            let name = line[1..line.len() - 1].trim().to_ascii_lowercase();
            if name == "peer" {
                current = Some((true, HashMap::new()));
            } else if name == "interface" {
                current = Some((false, HashMap::new()));
            } else {
                current = None;
            }
            continue;
        }
        if let (Some(entry), Some(i)) = (current.as_mut(), line.find('=')) {
            entry
                .1
                .entry(line[..i].trim().to_ascii_lowercase())
                .or_insert_with(|| line[i + 1..].trim().to_string());
        }
    }
    if let Some((is_peer, section)) = current {
        if is_peer {
            peers.push((section, active_iface.clone()));
        }
        // A trailing [Interface] with no peers binds nothing: drop it instead
        // of updating state no peer will ever read.
    }
    if peers.is_empty() {
        return BatchResult::fail(
            0,
            ImportErrorKind::InvalidJson,
            "wireguard conf has no [Peer] sections",
        );
    }
    let mut out = BatchResult::default();
    for (peer_index, (peer, iface)) in peers.iter().enumerate() {
        let index = peer_index;
        let endpoint = conf_value(peer, "endpoint");
        let parsed = (|| -> Result<ImportedConfig, ImportError> {
            if endpoint.trim().is_empty() {
                return Err(bad_host("wireguard peer is missing Endpoint"));
            }
            let (host, port_text) = split_host_port(endpoint.trim())?;
            let port = match port_text {
                Some(text) => parse_port(&text)?,
                None => return Err(bad_port("wireguard peer Endpoint is missing its port")),
            };
            let public_key = conf_value(peer, "publickey");
            if public_key.trim().is_empty() {
                return Err(missing_credential("wireguard peer is missing PublicKey"));
            }
            let private_key = conf_value(iface, "privatekey");
            if private_key.trim().is_empty() {
                return Err(missing_credential("wireguard conf is missing PrivateKey"));
            }
            let mut params = base_params("wireguard", &host, port);
            insert_str(&mut params, "private_key", private_key);
            insert_str(&mut params, "public_key", public_key);
            for (conf_key, json_key) in [
                ("presharedkey", "pre_shared_key"),
                ("allowedips", "allowed_ips"),
                ("persistentkeepalive", "persistent_keepalive"),
                ("address", "address"),
                ("dns", "dns"),
                ("mtu", "mtu"),
            ] {
                let value = conf_value(peer, conf_key);
                if !value.trim().is_empty() {
                    insert_str(&mut params, json_key, value);
                }
            }
            for (conf_key, json_key) in [("address", "address"), ("dns", "dns"), ("mtu", "mtu")] {
                let value = conf_value(iface, conf_key);
                if !value.trim().is_empty() && !params.contains_key(json_key) {
                    insert_str(&mut params, json_key, value);
                }
            }
            let name = format!("{stem}-p{}", peer_index + 1);
            Ok(ImportedConfig {
                name,
                protocol: "wireguard".to_string(),
                params: finish(params),
                host,
                port,
            })
        })();
        match parsed {
            Ok(config) => out.configs.push(config),
            Err(error) => out.failures.push(ImportFailure::new(
                index,
                format!("{stem} peer {}", peer_index + 1),
                error,
            )),
        }
    }
    out
}

/// Ticket 15: case-insensitive, comment-aware conf detection. Section
/// markers match in any case (`[Interface]`, `[interface]`, … — the parser
/// itself already lowercases them); `://` only disqualifies when it appears
/// on a non-comment line, so a `# see https://…` comment no longer defeats
/// detection. Paste (`import_text`) and drop (`import_file_content`) share
/// this detector, so the same bytes parse the same way on both paths.
fn looks_like_wg_conf(trimmed: &str) -> bool {
    let mut has_section = false;
    for raw_line in trimmed.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.contains("://") {
            return false;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let name = line[1..line.len() - 1].trim();
            if name.eq_ignore_ascii_case("interface") || name.eq_ignore_ascii_case("peer") {
                has_section = true;
            }
        }
    }
    has_section
}

fn file_stem(file_name: &str) -> String {
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    let unhidden = base.trim_start_matches('.');
    if unhidden.is_empty() {
        return String::new();
    }
    if base.starts_with('.') && !unhidden.contains('.') {
        // Dotfile without a real stem (`.conf`): the leading dot marks a
        // hidden file, not an extension separator, so there is no stem to
        // derive and the caller falls back to "wireguard".
        return String::new();
    }
    match unhidden.rfind('.') {
        Some(i) => unhidden[..i].to_string(),
        None => unhidden.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

fn valid_scheme_chars(scheme: &str) -> bool {
    let mut chars = scheme.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
}

/// Parse one share link. Never panics; every failure mode maps to the error
/// taxonomy.
pub fn parse_link(line: &str) -> Result<ImportedConfig, ImportError> {
    let text = line.trim();
    if text.is_empty() {
        return Err(ImportError::new(
            ImportErrorKind::Empty,
            "empty payload".to_string(),
        ));
    }
    let sep = text
        .find("://")
        .ok_or_else(|| unknown_scheme("not a share link (missing '://')"))?;
    let scheme_raw = &text[..sep];
    if !valid_scheme_chars(scheme_raw) {
        return Err(unknown_scheme(format!("unknown scheme '{scheme_raw}'")));
    }
    let scheme = scheme_raw.to_ascii_lowercase();
    let rest = &text[sep + 3..];
    match scheme.as_str() {
        "vmess" => {
            // New URI form carries `@` before any `?`/`#`; otherwise legacy.
            let authority_end = rest.find(['?', '#']).unwrap_or(rest.len());
            if rest[..authority_end].contains('@') {
                parse_vless_like("vmess", &split_uri(&scheme, rest)?, "uuid")
            } else {
                let payload = &rest[..authority_end];
                parse_vmess_legacy(payload)
            }
        }
        "ss" | "shadowsocks" => parse_ss_rest(rest),
        "throne" => parse_throne_rest(rest),
        "vless" => parse_vless_like("vless", &split_uri(&scheme, rest)?, "uuid"),
        "trojan" => parse_vless_like("trojan", &split_uri(&scheme, rest)?, "password"),
        // Ticket 08: Xray-only VLESS links share the vless userinfo shape;
        // the row keeps the `xray` kind so the engine bridges it to the
        // Xray sidecar (Xray-only fields pass through verbatim).
        "xray" => parse_vless_like("xray", &split_uri(&scheme, rest)?, "uuid"),
        "hysteria2" | "hy2" | "hysteria2+realm" => parse_hy2(&scheme, &split_uri(&scheme, rest)?),
        "hysteria" => parse_hy1(&split_uri(&scheme, rest)?),
        "tuic" => parse_tuic(&split_uri(&scheme, rest)?),
        "wireguard" | "wg" => parse_wg_uri(&split_uri(&scheme, rest)?),
        "awg" | "amneziawg" | "amnezia-wg" => parse_awg_uri(&split_uri(&scheme, rest)?),
        "socks" | "socks5" => parse_socks_like("socks", &split_uri(&scheme, rest)?, None),
        "anytls" => parse_anytls(&split_uri(&scheme, rest)?),
        "http" => parse_socks_like("http", &split_uri(&scheme, rest)?, Some(80)),
        "https" => parse_socks_like("https", &split_uri(&scheme, rest)?, Some(443)),
        "naive+https" => parse_naive(&split_uri(&scheme, rest)?),
        _ => Err(unknown_scheme(format!("unknown scheme '{scheme}'"))),
    }
}

/// Parse pasted/clipboard text: a WireGuard conf (any marker case), a JSON
/// body (outbounds array or full config, BOM-prefixed or not), limited Clash
/// YAML (`proxies:` — same support as subscription bodies), or one share
/// link per line. Blank lines are skipped silently; every other failure is
/// reported with its item index.
pub fn import_text(body: &str) -> BatchResult {
    let trimmed = strip_bom(body.trim()).trim();
    if trimmed.is_empty() {
        return BatchResult::fail(0, ImportErrorKind::Empty, "nothing to import");
    }
    if looks_like_wg_conf(trimmed) {
        return import_wg_conf(trimmed, "wireguard");
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return match serde_json::from_str::<serde_json::Value>(trimmed) {
            Ok(value) => import_json_value(&value),
            Err(error) => BatchResult::fail(
                0,
                ImportErrorKind::InvalidJson,
                format!("invalid JSON: {error}"),
            ),
        };
    }
    if has_clash_proxies(trimmed) {
        return import_clash_proxies(trimmed);
    }
    let mut out = BatchResult::default();
    let mut index = 0;
    for raw_line in body.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        match parse_link(line) {
            Ok(config) => out.configs.push(config),
            Err(error) => out
                .failures
                .push(ImportFailure::new(index, redact_link(line), error)),
        }
        index += 1;
    }
    out
}

/// Parse dropped/picked file content. `.conf` files go through the
/// WireGuard parser only when the content actually looks like a conf;
/// JSON / link-line / Clash bodies inside a `.conf` file are content-sniffed
/// down the [`import_text`] path instead (never forced through the wg parser
/// with a wrong-taxonomy error). Everything else follows [`import_text`].
pub fn import_file_content(file_name: &str, content: &str) -> BatchResult {
    if file_name.to_ascii_lowercase().ends_with(".conf") {
        let trimmed = strip_bom(content.trim()).trim();
        if looks_like_wg_conf(trimmed) {
            let stem = file_stem(file_name);
            let stem = if stem.trim().is_empty() {
                "wireguard".to_string()
            } else {
                stem
            };
            return import_wg_conf(trimmed, &stem);
        }
    }
    import_text(content)
}

/// Write a batch into the store as disabled records in `group` (blank falls
/// back to the default group). Ticket 16: dedupe uses
/// [`normalized_identity`] — the same key as subscription refresh — so the
/// same server with different case/whitespace or a flow-only change commits
/// a single row instead of duplicating. Identical re-imports never duplicate.
/// Import never enables or starts configs: it only calls [`Store::add`],
/// which creates disabled records.
pub fn commit_batch(
    store: &mut Store,
    batch: BatchResult,
    group: &str,
) -> Result<ImportOutcome, StoreError> {
    let group = group.trim();
    let group = if group.is_empty() {
        DEFAULT_GROUP.to_string()
    } else {
        group.to_string()
    };
    let mut seen: HashSet<String> = store
        .list()?
        .iter()
        .map(|config| normalized_identity(&config.protocol, &config.params))
        .collect();
    let mut outcome = ImportOutcome {
        imported: 0,
        duplicates: 0,
        failures: batch.failures,
    };
    for config in batch.configs {
        if !seen.insert(normalized_identity(&config.protocol, &config.params)) {
            outcome.duplicates += 1;
            continue;
        }
        store.add(NewConfig {
            name: config.name,
            protocol: config.protocol,
            params: config.params,
            group: group.clone(),
            bind_ip: String::new(),
            port: String::new(),
        })?;
        outcome.imported += 1;
    }
    Ok(outcome)
}

/// Ticket 16: parse several dropped files, each through [`import_file_content`]
/// with its own name, so per-file `.conf` stems and PrivateKeys are preserved
/// (no cross-file bleed from merged-text parsing). Failure indices are offset
/// by the number of items already seen so every failure keeps a unique index.
pub fn import_multi_file_contents(files: &[(&str, &str)]) -> BatchResult {
    let mut out = BatchResult::default();
    let mut offset = 0usize;
    for (name, content) in files {
        let mut batch = import_file_content(name, content);
        for config in batch.configs.drain(..) {
            out.configs.push(config);
            offset += 1;
        }
        for mut failure in batch.failures.drain(..) {
            failure.index += offset;
            out.failures.push(failure);
            offset += 1;
        }
    }
    out
}

/// Ticket 16: stale-hint guard. A `.conf` hint with non-conf text means the
/// dialog text was edited after the drop — the hint is cleared so the edited
/// text sniffs as its own type instead of parsing as the old file type.
pub fn effective_hint_for_text(hint: Option<&str>, text: &str) -> Option<String> {
    match hint {
        Some(name) if name.to_ascii_lowercase().ends_with(".conf") => {
            let trimmed = strip_bom(text.trim()).trim();
            if looks_like_wg_conf(trimmed) {
                Some(name.to_string())
            } else {
                None
            }
        }
        Some(name) => Some(name.to_string()),
        None => None,
    }
}

// ---------------------------------------------------------------------------
// Ticket 04: subscriptions — normalized identity, addsub intake, bodies
// ---------------------------------------------------------------------------

/// Normalized subscription identity for refresh upserts: protocol + server +
/// port + credential + method + sni/path, NOT the remark. Server is trimmed
/// and lowercased (hostnames are case-insensitive, surrounding whitespace is
/// not identity); credential values stay verbatim. Non-JSON params fall back
/// to the raw string so hand-made rows still get a stable key.
/// Ticket 16: [`commit_batch`] uses this same key so paste/import dedupes
/// exactly like subscription refresh.
pub fn normalized_identity(protocol: &str, params: &str) -> String {
    fn get(obj: &Params, keys: &[&str]) -> Option<String> {
        keys.iter().find_map(|wanted| {
            obj.iter().find_map(|(key, value)| {
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
    }
    let protocol = protocol.trim().to_ascii_lowercase();
    match serde_json::from_str::<serde_json::Value>(params) {
        Ok(serde_json::Value::Object(map)) => {
            let server = get(&map, &["server", "address"])
                .map(|server| server.trim().to_ascii_lowercase())
                .unwrap_or_default();
            let port = get(&map, &["server_port", "port"]).unwrap_or_default();
            let creds: Vec<String> = [
                "uuid",
                "password",
                "auth",
                "private_key",
                "privatekey",
                "public_key",
                "publickey",
                "method",
                "username",
                "token",
            ]
            .iter()
            .filter_map(|key| get(&map, &[key]))
            .collect();
            let sni: Vec<String> = ["sni", "peer", "servername"]
                .iter()
                .filter_map(|key| get(&map, &[key]))
                .map(|value| value.trim().to_ascii_lowercase())
                .collect();
            let path: Vec<String> = ["path", "service_name", "servicename", "serviceName"]
                .iter()
                .filter_map(|key| get(&map, &[key]))
                .collect();
            format!(
                "{protocol}\0{server}\0{port}\0{}\0{}\0{}",
                creds.join(","),
                sni.join(","),
                path.join(",")
            )
        }
        _ => format!("{protocol}\0{params}"),
    }
}

/// Parsed `throne://addsub/...` intake: a subscription URL plus its group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddSubRequest {
    pub url: String,
    pub group: String,
}

/// Ticket 14: single-line `throne://addsub/...` routing check (scheme and
/// command both case-insensitive). Only these lines divert to the
/// subscription intake; a malformed one reports its own
/// [`parse_addsub_link`] reason instead of falling through to the generic
/// unknown-command failure.
pub fn is_addsub_attempt(line: &str) -> bool {
    let trimmed = line.trim();
    let Some(sep) = trimmed.find("://") else {
        return false;
    };
    if !trimmed[..sep].eq_ignore_ascii_case("throne") {
        return false;
    }
    trimmed[sep + 3..]
        .split('/')
        .next()
        .unwrap_or("")
        .eq_ignore_ascii_case("addsub")
}

/// Parse `throne://addsub/<standard-base64 of "<url>[#group]">`
/// (case-insensitive command). Unlike `throne://add`, the payload alphabet is
/// standard base64 ONLY: urlsafe bytes (`-`, `_`) are rejected as bad base64.
/// The URL must be `http(s)`; a missing group falls back to the default.
pub fn parse_addsub_link(line: &str) -> Result<AddSubRequest, ImportError> {
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
    let text = line.trim();
    let sep = text
        .find("://")
        .ok_or_else(|| unknown_scheme("not a share link (missing '://')"))?;
    if !text[..sep].eq_ignore_ascii_case("throne") {
        return Err(unknown_scheme(format!("unknown scheme '{}'", &text[..sep])));
    }
    let rest = &text[sep + 3..];
    let (command, payload) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, ""),
    };
    if !command.eq_ignore_ascii_case("addsub") {
        return Err(unknown_scheme(format!(
            "unknown throne command '{command}'"
        )));
    }
    if payload.trim().is_empty() {
        return Err(ImportError::new(
            ImportErrorKind::Empty,
            "throne link is missing its payload",
        ));
    }
    let compact: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.chars().any(|c| c == '-' || c == '_') {
        return Err(ImportError::new(
            ImportErrorKind::BadBase64,
            "addsub payload must be standard base64",
        ));
    }
    let bytes = STANDARD
        .decode(&compact)
        .or_else(|_| STANDARD_NO_PAD.decode(&compact))
        .map_err(|_| ImportError::new(ImportErrorKind::BadBase64, "bad base64 payload"))?;
    let inner = String::from_utf8(bytes)
        .map_err(|_| ImportError::new(ImportErrorKind::NonUtf8, "payload is not UTF-8"))?;
    let inner = inner.trim();
    if inner.is_empty() {
        return Err(ImportError::new(
            ImportErrorKind::Empty,
            "addsub payload is empty",
        ));
    }
    let (url, group) = match inner.find('#') {
        Some(i) => (inner[..i].trim(), inner[i + 1..].trim().to_string()),
        None => (inner, DEFAULT_GROUP.to_string()),
    };
    let after_scheme = if url.len() >= 7 && url[..7].eq_ignore_ascii_case("http://") {
        &url[7..]
    } else if url.len() >= 8 && url[..8].eq_ignore_ascii_case("https://") {
        &url[8..]
    } else {
        return Err(bad_host("subscription url must use http(s)"));
    };
    let host = after_scheme.split(['/', '?']).next().unwrap_or("");
    if host.trim().is_empty() || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(bad_host("subscription url must use http(s)"));
    }
    Ok(AddSubRequest {
        url: url.to_string(),
        group: if group.is_empty() {
            DEFAULT_GROUP.to_string()
        } else {
            group
        },
    })
}

/// Parse a subscription response body, reusing the ticket-03 parsers:
/// JSON (arrays, full configs, limited SIP008 `servers[]`), limited Clash
/// YAML (`proxies:` with `type`/`server`/`port`), base64 blobs of share links
/// (single-line, `data:…;base64,…`, or per-line v2rayN style), WireGuard conf,
/// or plain share-link lines. Per-item failures skip + count, as everywhere.
pub fn import_subscription_body(body: &str) -> BatchResult {
    let trimmed = strip_bom(body.trim()).trim();
    if trimmed.is_empty() {
        return BatchResult::fail(0, ImportErrorKind::Empty, "nothing to import");
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return match serde_json::from_str::<serde_json::Value>(trimmed) {
            Ok(value) => import_json_value(&value),
            Err(error) => BatchResult::fail(
                0,
                ImportErrorKind::InvalidJson,
                format!("invalid JSON: {error}"),
            ),
        };
    }
    if looks_like_wg_conf(trimmed) {
        return import_wg_conf(trimmed, "wireguard");
    }
    if has_clash_proxies(trimmed) {
        return import_clash_proxies(trimmed);
    }
    let lines: Vec<&str> = trimmed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.len() > 1 {
        // v2rayN-style multi-line base64: decode the lines that decode,
        // keep the rest verbatim, then parse the rebuilt body.
        let mut rebuilt = Vec::with_capacity(lines.len());
        let mut decoded_any = false;
        for line in &lines {
            if line.contains("://") {
                rebuilt.push((*line).to_string());
            } else if let Ok(text) = decode_b64_utf8(line) {
                let text = text.trim().to_string();
                if text.contains("://") {
                    rebuilt.push(text);
                    decoded_any = true;
                } else {
                    rebuilt.push((*line).to_string());
                }
            } else {
                rebuilt.push((*line).to_string());
            }
        }
        if decoded_any {
            return import_text(&rebuilt.join("\n"));
        }
        return import_text(body);
    }
    let single = lines.first().copied().unwrap_or("");
    if single.contains("://") && !single.to_ascii_lowercase().starts_with("data:") {
        return import_text(body);
    }
    // `data:…;base64,…` single payloads (V2Fly DataURLSingle).
    if let Some(comma) = single.find(";base64,").or_else(|| single.find(";BASE64,")) {
        let payload = &single[comma + 8..];
        return match decode_b64_utf8(payload) {
            Ok(text) if text.contains("://") => import_text(&text),
            Ok(_) => BatchResult::fail(
                0,
                ImportErrorKind::InvalidJson,
                "subscription payload has no share links",
            ),
            Err(error) => BatchResult {
                configs: Vec::new(),
                failures: vec![ImportFailure::new(0, "body", error)],
            },
        };
    }
    // Single base64 blob of share links.
    match decode_b64_utf8(single) {
        Ok(text) if text.trim().contains("://") => import_text(&text),
        Ok(_) => import_text(body),
        Err(error) => match error.kind {
            ImportErrorKind::Empty => import_text(body),
            ImportErrorKind::NonUtf8 => BatchResult {
                configs: Vec::new(),
                failures: vec![ImportFailure::new(0, "body", error)],
            },
            // Not base64 at all: let the link parser report it per line.
            _ => import_text(body),
        },
    }
}

/// A Clash body has a top-level `proxies:` mapping. Share-link bodies never
/// start a line with `proxies:`.
fn has_clash_proxies(trimmed: &str) -> bool {
    trimmed.lines().any(|raw_line| {
        let indent = raw_line.len() - raw_line.trim_start().len();
        indent == 0 && {
            let line = raw_line.trim();
            line == "proxies:" || line.starts_with("proxies:")
        }
    })
}

/// Limited Clash `proxies:` parser (block style only): each `- ` entry needs
/// `name` (default `{type}-{server}:{port}`), `type`, `server`, `port`.
/// Entries are converted to sing-box-shaped params; unknown types and missing
/// fields are per-item failures. Groups/rules are ignored.
fn import_clash_proxies(content: &str) -> BatchResult {
    fn clash_type(raw: &str) -> Result<String, ImportError> {
        let lower = raw.trim().to_ascii_lowercase();
        match lower.as_str() {
            "vless" | "vmess" | "trojan" | "shadowsocks" | "hysteria2" | "hysteria" | "tuic"
            | "wireguard" | "amneziawg" | "socks" | "anytls" | "http" | "https" | "naive" => {
                Ok(lower)
            }
            "ss" => Ok("shadowsocks".to_string()),
            "socks5" => Ok("socks".to_string()),
            // Ticket 07: AWG aliases fold to the canonical stored form.
            "awg" | "amnezia-wg" => Ok("amneziawg".to_string()),
            // Ticket 08: same kind-preserving fold as the JSON path.
            "xray" | "xray-vless" | "xray_vless" | "vless-xray" => Ok("xray".to_string()),
            "custom"
            | "custom-out"
            | "custom-outbound"
            | "custom_outbound"
            | "custom-outbound-json" => Ok("custom-outbound".to_string()),
            "custom-config" | "custom_config" | "custom-full" | "custom_full"
            | "custom-full-config" => Ok("custom-config".to_string()),
            "extra" | "extra-core" | "extra_core" | "extracore" => Ok("extra-core".to_string()),
            "chain" | "chained" | "chain-outbound" | "chain_outbound" => Ok("chain".to_string()),
            "" => Err(invalid_json("clash entry is missing \"type\"")),
            other => Err(unknown_scheme(format!("unknown clash type '{other}'"))),
        }
    }
    fn clash_value(raw: &str) -> String {
        let mut value = raw.trim().to_string();
        if value.len() >= 2
            && ((value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\'')))
        {
            value = value[1..value.len() - 1].to_string();
        }
        value
    }
    let mut out = BatchResult::default();
    let lines: Vec<&str> = content.lines().collect();
    let start = lines
        .iter()
        .position(|raw_line| {
            let line = raw_line.trim();
            line == "proxies:" || line.starts_with("proxies:")
        })
        .unwrap_or(0);
    // Entries: (fields, source index). Indents track the `- ` entry line
    // separately from the first `key: value` line so nested mapping detail
    // is ignored instead of ending the entry.
    struct Pending {
        fields: Vec<(String, String)>,
        entry_indent: usize,
        key_indent: Option<usize>,
    }
    let mut entries: Vec<(Vec<(String, String)>, usize)> = Vec::new();
    let mut current: Option<Pending> = None;
    let flush = |current: &mut Option<Pending>,
                 entries: &mut Vec<(Vec<(String, String)>, usize)>| {
        if let Some(pending) = current.take() {
            let index = entries.len();
            entries.push((pending.fields, index));
        }
    };
    for raw_line in lines.iter().skip(start + 1) {
        if raw_line.trim().is_empty() || raw_line.trim_start().starts_with('#') {
            continue;
        }
        let indent = raw_line.len() - raw_line.trim_start().len();
        if indent == 0 {
            break; // next top-level section ends `proxies:`.
        }
        let line = raw_line.trim();
        if line.starts_with("- ") || line == "-" {
            flush(&mut current, &mut entries);
            let after = line.strip_prefix("- ").unwrap_or("").trim();
            let mut fields = Vec::new();
            if !after.is_empty() {
                // `- {flow}` mappings are out of scope for the limited parser.
                if after.starts_with('{') {
                    let index = entries.len();
                    entries.push((vec![("__flow__".to_string(), after.to_string())], index));
                    continue;
                }
                if let Some(i) = after.find(':') {
                    let (key, value) = (after[..i].trim(), after[i + 1..].trim());
                    if !key.is_empty() {
                        fields.push((key.to_ascii_lowercase(), clash_value(value)));
                    }
                }
            }
            current = Some(Pending {
                fields,
                entry_indent: indent,
                key_indent: None,
            });
        } else if let Some(pending) = current.as_mut() {
            if indent <= pending.entry_indent {
                // Dedented non-entry line ends the entry; the line itself is
                // not a field.
                flush(&mut current, &mut entries);
                continue;
            }
            match pending.key_indent {
                None => {
                    if let Some(i) = line.find(':') {
                        let (key, value) = (line[..i].trim(), line[i + 1..].trim());
                        if !key.is_empty() && !key.contains(' ') {
                            pending.key_indent = Some(indent);
                            pending
                                .fields
                                .push((key.to_ascii_lowercase(), clash_value(value)));
                        }
                    }
                }
                Some(key_indent) => {
                    if indent != key_indent {
                        continue; // nested mapping detail: ignored.
                    }
                    if let Some(i) = line.find(':') {
                        let (key, value) = (line[..i].trim(), line[i + 1..].trim());
                        if !key.is_empty() && !key.contains(' ') {
                            pending
                                .fields
                                .push((key.to_ascii_lowercase(), clash_value(value)));
                        }
                    }
                }
            }
        }
    }
    flush(&mut current, &mut entries);
    if entries.is_empty() {
        return BatchResult::fail(
            0,
            ImportErrorKind::InvalidJson,
            "clash body has no proxy entries",
        );
    }
    for (entry_index, (fields, _)) in entries.iter().enumerate() {
        let parsed = (|| -> Result<ImportedConfig, ImportError> {
            if fields.iter().any(|(key, _)| key == "__flow__") {
                return Err(invalid_json("clash flow mappings are not supported"));
            }
            let raw_type = fields
                .iter()
                .find(|(key, _)| key == "type")
                .map(|(_, value)| value.clone())
                .unwrap_or_default();
            let protocol = clash_type(&raw_type)?;
            let host = fields
                .iter()
                .find(|(key, _)| key == "server")
                .map(|(_, value)| value.clone())
                .filter(|host| !host.trim().is_empty())
                .ok_or_else(|| {
                    ImportError::new(
                        ImportErrorKind::BadHost,
                        "clash entry is missing \"server\"",
                    )
                })?;
            if host.chars().any(|c| c.is_whitespace() || c.is_control()) {
                return Err(bad_host("clash entry has a bad \"server\""));
            }
            let port_raw = fields
                .iter()
                .find(|(key, _)| key == "port")
                .map(|(_, value)| value.clone())
                .unwrap_or_default();
            let port = parse_port(&port_raw)
                .map_err(|_| bad_port(format!("clash entry has a bad \"port\" ({port_raw})")))?;
            let name = fields
                .iter()
                .find(|(key, _)| key == "name")
                .map(|(_, value)| value.clone())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| format!("{protocol}-{host}:{port}"));
            let mut params = base_params(&protocol, &host, port);
            for (key, value) in fields {
                if key == "name" || key == "type" || key == "server" || key == "port" {
                    continue;
                }
                insert_str(&mut params, key, value.clone());
            }
            Ok(ImportedConfig {
                name,
                protocol,
                params: finish(params),
                host,
                port,
            })
        })();
        match parsed {
            Ok(config) => out.configs.push(config),
            Err(error) => {
                let label = entries[entry_index]
                    .0
                    .iter()
                    .find(|(key, _)| key == "name")
                    .map(|(_, value)| value.clone())
                    .unwrap_or_else(|| "clash".to_string());
                out.failures
                    .push(ImportFailure::new(entry_index, label, error));
            }
        }
    }
    dedupe_names(&mut out.configs);
    out
}

/// Redact credentials (`scheme://…@`) and query values, then truncate, for
/// notifications and logs (boundary: secrets never reach logs/clipboard).
/// Query keys stay (they diagnose failures); values go (some schemes carry
/// key material like reality `pbk` in the query). Fragments (remarks) stay:
/// remarks are display names by design.
pub fn redact_link(line: &str) -> String {
    let mut text = line.trim().to_string();
    if let Some(scheme_end) = text.find("://") {
        if let Some(at) = text[scheme_end + 3..].find('@') {
            text.replace_range(scheme_end + 3..scheme_end + 3 + at, "…");
        }
    }
    if let Some(query_start) = text.find('?') {
        let frag_start = text.find('#').unwrap_or(text.len());
        if query_start < frag_start {
            let mut pairs = Vec::new();
            for pair in text[query_start + 1..frag_start].split('&') {
                match pair.find('=') {
                    Some(eq) => pairs.push(format!("{}=…", &pair[..eq])),
                    None => pairs.push(pair.to_string()),
                }
            }
            text = format!(
                "{}?{}{}",
                &text[..query_start],
                pairs.join("&"),
                &text[frag_start..]
            );
        }
    }
    const LIMIT: usize = 96;
    if text.chars().count() > LIMIT {
        let kept: String = text.chars().take(LIMIT).collect();
        format!("{kept}…")
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    fn b64(text: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(text)
    }

    fn params_of(config: &ImportedConfig) -> serde_json::Value {
        serde_json::from_str(&config.params).expect("params are JSON")
    }

    // -- fixture corpus: one per grammar -----------------------------------

    const UUID: &str = "11111111-2222-4333-8444-555555555555";

    #[test]
    fn vless_full_link() {
        let link = format!(
            "vless://{UUID}@example.com:443?encryption=none&security=tls&sni=example.com&fp=chrome&type=ws&path=%2Fws&host=example.com#vless-one"
        );
        let config = parse_link(&link).expect("vless parses");
        assert_eq!(config.protocol, "vless");
        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, 443);
        assert_eq!(config.name, "vless-one");
        let params = params_of(&config);
        assert_eq!(params["uuid"], UUID);
        assert_eq!(params["network"], "ws");
        assert_eq!(params["security"], "tls");
        assert_eq!(params["path"], "/ws");
    }

    #[test]
    fn vless_reality_requires_pbk() {
        let link =
            format!("vless://{UUID}@example.com:443?security=reality&sni=example.com&fp=chrome#r");
        let error = parse_link(&link).expect_err("reality without pbk fails");
        assert_eq!(error.kind, ImportErrorKind::MissingCredential);
        assert!(error.message.contains("pbk"), "names pbk, got {error}");

        let link = format!(
            "vless://{UUID}@example.com:443?security=reality&sni=example.com&fp=chrome&pbk=abcdef&sid=1234#r"
        );
        let config = parse_link(&link).expect("reality with pbk parses");
        assert_eq!(params_of(&config)["pbk"], "abcdef");
    }

    #[test]
    fn vmess_legacy_json() {
        let json = serde_json::json!({
            "v": "2", "ps": "vmess-old", "add": "example.com", "port": "10086",
            "id": UUID, "aid": "0", "scy": "auto", "net": "tcp",
            "type": "none", "host": "", "path": "", "tls": ""
        });
        let config = parse_link(&format!("vmess://{}", b64(&json.to_string())))
            .expect("legacy vmess parses");
        assert_eq!(config.protocol, "vmess");
        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, 10086);
        assert_eq!(config.name, "vmess-old");
        assert_eq!(params_of(&config)["uuid"], UUID);
    }

    #[test]
    fn vmess_new_uri() {
        let link = format!("vmess://{UUID}@example.com:443?encryption=auto#vmess-uri");
        let config = parse_link(&link).expect("vmess URI parses");
        assert_eq!(config.protocol, "vmess");
        assert_eq!(config.host, "example.com");
        assert_eq!(params_of(&config)["encryption"], "auto");
    }

    #[test]
    fn trojan_link() {
        let config = parse_link("trojan://secret@example.com:443?sni=example.com#tj")
            .expect("trojan parses");
        assert_eq!(config.protocol, "trojan");
        assert_eq!(config.port, 443);
        let params = params_of(&config);
        assert_eq!(params["password"], "secret");
        assert_eq!(params["security"], "tls");
    }

    #[test]
    fn ss_legacy_blob() {
        // ss://base64(aes-256-gcm:hunter2@example.com:8388)
        let config = parse_link(&format!(
            "ss://{}",
            b64("aes-256-gcm:hunter2@example.com:8388")
        ))
        .expect("legacy ss parses");
        assert_eq!(config.protocol, "shadowsocks");
        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, 8388);
        let params = params_of(&config);
        assert_eq!(params["method"], "aes-256-gcm");
        assert_eq!(params["password"], "hunter2");
    }

    #[test]
    fn ss_sip002_userinfo() {
        // Canonical SIP002 example: aes-128-gcm:test@192.168.100.1:8888.
        let config = parse_link("ss://YWVzLTEyOC1nY206dGVzdA@192.168.100.1:8888#sip002")
            .expect("sip002 ss parses");
        assert_eq!(config.protocol, "shadowsocks");
        assert_eq!(config.host, "192.168.100.1");
        assert_eq!(config.port, 8888);
        assert_eq!(config.name, "sip002");
        let params = params_of(&config);
        assert_eq!(params["method"], "aes-128-gcm");
        assert_eq!(params["password"], "test");
    }

    #[test]
    fn ss_modern_split_plain() {
        let config = parse_link("ss://2022-blake3-aes-128-gcm:YWJjMTIz@example.com:8388#split")
            .expect("split ss parses");
        assert_eq!(params_of(&config)["method"], "2022-blake3-aes-128-gcm");
        assert_eq!(params_of(&config)["password"], "YWJjMTIz");
    }

    #[test]
    fn hysteria2_with_and_without_port() {
        let with_port = parse_link("hysteria2://letmein@example.com:8443?sni=example.com#hy2")
            .expect("hy2 parses");
        assert_eq!(with_port.port, 8443);
        let omitted = parse_link("hy2://letmein@example.com?sni=example.com#hy2")
            .expect("hy2 without port parses");
        assert_eq!(omitted.port, 443, "hy2 omits port → 443");
        assert_eq!(omitted.protocol, "hysteria2");
    }

    #[test]
    fn hysteria2_realm_variant() {
        let config = parse_link("hysteria2+realm://letmein@example.com:8443#r")
            .expect("realm variant parses");
        assert_eq!(config.protocol, "hysteria2");
        assert_eq!(params_of(&config)["variant"], "realm");
    }

    #[test]
    fn hysteria_v1() {
        let config = parse_link("hysteria://auth123@example.com:18443?peer=example.com#hy1")
            .expect("hysteria v1 parses");
        assert_eq!(config.protocol, "hysteria");
        assert_eq!(config.port, 18443);
        assert_eq!(params_of(&config)["auth"], "auth123");
    }

    #[test]
    fn tuic_link() {
        let config = parse_link(&format!(
            "tuic://{UUID}:tuicpass@example.com:4433?congestion_control=bbr#tu"
        ))
        .expect("tuic parses");
        assert_eq!(config.protocol, "tuic");
        let params = params_of(&config);
        assert_eq!(params["uuid"], UUID);
        assert_eq!(params["password"], "tuicpass");
        assert_eq!(params["congestion_control"], "bbr");
    }

    #[test]
    fn wireguard_uri() {
        let link = "wireguard://cHJpdmF0ZWtleQ@example.com:51820?publickey=cHVia2V5&address=10.0.0.2%2F32&mtu=1420#wg";
        let config = parse_link(link).expect("wireguard URI parses");
        assert_eq!(config.protocol, "wireguard");
        assert_eq!(config.port, 51820);
        let params = params_of(&config);
        assert_eq!(params["private_key"], "cHJpdmF0ZWtleQ");
        assert_eq!(params["public_key"], "cHVia2V5");
        assert_eq!(params["address"], "10.0.0.2/32");
    }

    #[test]
    fn wireguard_conf_two_peers() {
        let conf = "[Interface]\nPrivateKey = cHJpdmF0ZWtleQ\nAddress = 10.0.0.2/32\nDNS = 1.1.1.1\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\nAllowedIPs = 0.0.0.0/0\n\n[Peer]\nPublicKey = cHVia2V5Mg\nEndpoint = 203.0.113.2:51820\n";
        let out = import_file_content("home.conf", conf);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 2);
        assert_eq!(out.configs[0].name, "home-p1");
        assert_eq!(out.configs[0].host, "203.0.113.1");
        assert_eq!(out.configs[1].host, "203.0.113.2");
        assert_eq!(params_of(&out.configs[0])["dns"], "1.1.1.1");
    }

    // -- ticket 07: AmneziaWG-variant links (params preserved verbatim) -----

    #[test]
    fn awg_link_preserves_version_matrix_params_verbatim() {
        let link = "awg://cHJpdmF0ZWtleQ@example.com:51820?publickey=cHVia2V5&awg_version=2.0&jc=4&jmin=40&jmax=70&s1=20&h1=1-100&i1=%3Cb%200xd100%3E%3Cr%2050%3E#awg-one";
        let config = parse_link(link).expect("awg parses");
        assert_eq!(config.protocol, "amneziawg");
        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, 51820);
        assert_eq!(config.name, "awg-one");
        let params = params_of(&config);
        assert_eq!(params["private_key"], "cHJpdmF0ZWtleQ");
        assert_eq!(params["public_key"], "cHVia2V5");
        assert_eq!(params["awg_version"], "2.0");
        assert_eq!(params["jc"], "4");
        assert_eq!(params["jmin"], "40");
        assert_eq!(params["jmax"], "70");
        assert_eq!(params["s1"], "20");
        assert_eq!(params["h1"], "1-100");
        assert_eq!(params["i1"], "<b 0xd100><r 50>");
    }

    #[test]
    fn awg_alias_schemes_fold_to_canonical() {
        for scheme in ["awg", "amneziawg", "amnezia-wg", "AWG"] {
            let link = format!(
                "{scheme}://cHJpdmF0ZWtleQ@example.com:51820?publickey=cHVia2V5&awg_version=3.1&HeaderProtectionKey=K#x"
            );
            let config = parse_link(&link).unwrap_or_else(|_| panic!("{scheme} parses"));
            assert_eq!(config.protocol, "amneziawg", "scheme {scheme}");
            assert_eq!(params_of(&config)["HeaderProtectionKey"], "K");
        }
    }

    #[test]
    fn awg_missing_keys_fail_without_echoing() {
        let no_priv = parse_link("awg://@example.com:51820?publickey=x")
            .expect_err("missing private key fails");
        assert_eq!(no_priv.kind, ImportErrorKind::MissingCredential);
        assert!(no_priv.message.contains("private key"), "got {no_priv}");

        let no_pub = parse_link("awg://cHJpdmF0ZWtleQ@example.com:51820")
            .expect_err("missing publickey fails");
        assert_eq!(no_pub.kind, ImportErrorKind::MissingCredential);
        assert!(no_pub.message.contains("publickey"), "got {no_pub}");
        assert!(
            !no_pub.message.contains("cHJpdmF0ZWtleQ"),
            "no key echo: {no_pub}"
        );
    }

    #[test]
    fn json_outbound_awg_types_fold_to_canonical() {
        for raw in ["amneziawg", "awg", "amnezia-wg"] {
            let body = serde_json::json!([{
                "type": raw, "tag": "awg-json",
                "server": "a.example.com", "server_port": 51820,
                "private_key": "p", "public_key": "q", "awg_version": "2.0", "jc": 2
            }]);
            let out = import_text(&body.to_string());
            assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
            assert_eq!(out.configs.len(), 1);
            assert_eq!(out.configs[0].protocol, "amneziawg", "type {raw}");
            assert_eq!(params_of(&out.configs[0])["awg_version"], "2.0");
        }
    }

    #[test]
    fn socks_anytls_http_naive() {
        let socks = parse_link("socks://user:pass@example.com:1080#s").expect("socks parses");
        assert_eq!(socks.protocol, "socks");
        assert_eq!(socks.port, 1080);

        let anytls = parse_link("anytls://secret@example.com:8443?sni=example.com#a")
            .expect("anytls parses");
        assert_eq!(anytls.protocol, "anytls");

        let http = parse_link("http://user:pass@example.com:8080#h").expect("http parses");
        assert_eq!(http.protocol, "http");
        assert_eq!(http.port, 8080);
        let https = parse_link("https://example.com#hs").expect("https parses");
        assert_eq!(https.port, 443, "https defaults to 443");

        let naive = parse_link("naive+https://user:pass@example.com:8443#n").expect("naive parses");
        assert_eq!(naive.protocol, "naive");
    }

    #[test]
    fn sing_box_array_with_dup_tags() {
        let body = serde_json::json!([
            {"type": "vless", "tag": "dup", "server": "a.example.com", "server_port": 443, "uuid": UUID},
            {"type": "trojan", "tag": "dup", "server": "b.example.com", "server_port": 443, "password": "x"},
            {"type": "wireguard", "tag": "wg", "server": "c.example.com", "server_port": 51820}
        ]);
        let out = import_text(&body.to_string());
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 3);
        assert_eq!(out.configs[0].name, "dup");
        assert_eq!(
            out.configs[1].name, "dup-2",
            "clash-style dup names deduped"
        );
    }

    #[test]
    fn sing_box_full_config() {
        let body = serde_json::json!({
            "log": {"level": "info"},
            "outbounds": [
                {"type": "vmess", "tag": "full", "server": "v.example.com", "server_port": 10086, "uuid": UUID}
            ]
        });
        let out = import_text(&body.to_string());
        assert!(out.failures.is_empty());
        assert_eq!(out.configs.len(), 1);
        assert_eq!(out.configs[0].name, "full");
    }

    #[test]
    fn throne_add_payload() {
        let outbound = serde_json::json!({
            "type": "vless", "tag": "throne-one",
            "server": "t.example.com", "server_port": 443, "uuid": UUID
        });
        let link = format!("throne://add/{}", b64(&outbound.to_string()));
        let config = parse_link(&link).expect("throne add parses");
        assert_eq!(config.protocol, "vless");
        assert_eq!(config.name, "throne-one");
        assert_eq!(config.host, "t.example.com");
    }

    // -- failure modes -------------------------------------------------------

    #[test]
    fn failure_bad_base64() {
        let error = parse_link("vmess://!!!not-base64!!!").expect_err("bad base64 fails");
        assert_eq!(error.kind, ImportErrorKind::BadBase64);
    }

    #[test]
    fn failure_wrong_alphabet_base64() {
        // `!` is in neither alphabet; whitespace is tolerated, `!` is not.
        let error = parse_link("vmess://ab!cdefgh").expect_err("wrong alphabet fails");
        assert_eq!(error.kind, ImportErrorKind::BadBase64);
    }

    #[test]
    fn failure_unknown_scheme() {
        let error = parse_link("gopher://example.com:70").expect_err("unknown scheme fails");
        assert_eq!(error.kind, ImportErrorKind::UnknownScheme);
        assert!(
            error.message.contains("gopher"),
            "names scheme, got {error}"
        );
    }

    #[test]
    fn failure_throne_unknown_command() {
        let error = parse_link("throne://addsub/aGk").expect_err("unknown throne cmd fails");
        assert_eq!(error.kind, ImportErrorKind::UnknownScheme);
        assert!(
            error.message.contains("addsub"),
            "names command, got {error}"
        );
    }

    #[test]
    fn failure_throne_missing_payload() {
        let error = parse_link("throne://add/").expect_err("missing payload fails");
        assert_eq!(error.kind, ImportErrorKind::Empty);
    }

    #[test]
    fn failure_bad_host_and_port() {
        let error = parse_link(&format!("vless://{UUID}@:443")).expect_err("empty host fails");
        assert_eq!(error.kind, ImportErrorKind::BadHost);
        for bad in ["0", "99999", "abc", ""] {
            let link = format!("vless://{UUID}@example.com:{bad}");
            let error = parse_link(&link).expect_err(&format!("port {bad} fails"));
            assert_eq!(error.kind, ImportErrorKind::BadPort, "port {bad}");
        }
        let error = parse_link(&format!("vless://{UUID}@::1:443")).expect_err("bare IPv6 fails");
        assert_eq!(error.kind, ImportErrorKind::BadHost);
        let bracketed =
            parse_link(&format!("vless://{UUID}@[::1]:443#v6")).expect("bracketed IPv6 parses");
        assert_eq!(bracketed.host, "::1");
    }

    #[test]
    fn failure_missing_credentials() {
        let cases = [
            "vless://@example.com:443",
            "trojan://@example.com:443",
            "tuic://example.com:443",
            "wireguard://@example.com:51820?publickey=x",
            "wireguard://priv@example.com:51820",
            "hysteria2://example.com:8443",
            "anytls://@example.com:8443",
        ];
        for link in cases {
            let error = parse_link(link).expect_err(&format!("{link} fails"));
            assert_eq!(
                error.kind,
                ImportErrorKind::MissingCredential,
                "link {link}"
            );
        }
    }

    #[test]
    fn failure_invalid_json() {
        let out = import_text("[{\"type\": \"vless\", ");
        assert_eq!(out.configs.len(), 0);
        assert_eq!(out.failures.len(), 1);
        assert_eq!(out.failures[0].code, ImportErrorKind::InvalidJson);

        let error = parse_link(&format!("vmess://{}", b64("{not json")))
            .expect_err("vmess with bad JSON fails");
        assert_eq!(error.kind, ImportErrorKind::InvalidJson);
    }

    #[test]
    fn failure_empty_payload() {
        assert_eq!(
            parse_link("   ").expect_err("blank line fails").kind,
            ImportErrorKind::Empty
        );
        let out = import_text("  \n ");
        assert_eq!(out.failures.len(), 1);
        assert_eq!(out.failures[0].code, ImportErrorKind::Empty);
    }

    #[test]
    fn failure_ss_plugin() {
        let error = parse_link("ss://YWVzLTEyOC1nY206dGVzdA@example.com:8388?plugin=")
            .expect_err("empty plugin fails");
        assert_eq!(error.kind, ImportErrorKind::SsPlugin);
    }

    #[test]
    fn failure_non_utf8() {
        // base64 of 0xFF 0xFE: decodes but is not UTF-8.
        let error = parse_link("vmess:////4=").expect_err("non-UTF8 fails");
        assert_eq!(error.kind, ImportErrorKind::NonUtf8);
    }

    #[test]
    fn failure_reasons_hide_credentials() {
        // Every failure message across the corpus must not echo userinfo.
        let bad = [
            "vless://secret-uuid@example.com:0",
            "trojan://s3cr3t@example.com:99999",
            "tuic://abc:def@example.com:notaport",
            "throne://add/%%%",
        ];
        for link in bad {
            let error = parse_link(link).expect_err(&format!("{link} fails"));
            assert!(
                !error.message.contains("s3cr3t") && !error.message.contains("secret-uuid"),
                "leak in {error}"
            );
        }
    }

    // -- batch + commit semantics --------------------------------------------

    /// Ticket 17: failure codes round-trip through their stored form (the
    /// subscription failures column persists codes as text).
    #[test]
    fn error_kind_from_str_round_trips() {
        for kind in [
            ImportErrorKind::Empty,
            ImportErrorKind::UnknownScheme,
            ImportErrorKind::BadBase64,
            ImportErrorKind::NonUtf8,
            ImportErrorKind::BadHost,
            ImportErrorKind::BadPort,
            ImportErrorKind::MissingCredential,
            ImportErrorKind::InvalidJson,
            ImportErrorKind::SsPlugin,
        ] {
            assert_eq!(ImportErrorKind::from_str(kind.as_str()), Some(kind));
        }
        assert_eq!(ImportErrorKind::from_str("bogus"), None);
        assert_eq!(ImportErrorKind::from_str(""), None);
    }

    #[test]
    fn batch_five_mixed_links() {
        let body = [
            &format!("vless://{UUID}@a.example.com:443#one"),
            &format!(
                "vmess://{}",
                b64(
                    &serde_json::json!({"ps":"two","add":"b.example.com","port":"10086","id":UUID})
                        .to_string()
                )
            ),
            "trojan://pw@c.example.com:443#three",
            "ss://YWVzLTEyOC1nY206dGVzdA@d.example.com:8388#four",
            "hy2://auth@e.example.com#five",
        ]
        .join("\n");
        let out = import_text(&body);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 5);
        assert_eq!(out.configs[4].port, 443);
    }

    #[test]
    fn batch_two_broken_items_reported_with_index() {
        let body = [
            &format!("vless://{UUID}@a.example.com:443#ok"),
            "vless://not-a-uuid@:0",
            "bogus://x",
            "trojan://pw@b.example.com:443#ok2",
        ]
        .join("\n");
        let out = import_text(&body);
        assert_eq!(out.configs.len(), 2);
        assert_eq!(out.failures.len(), 2);
        assert_eq!(out.failures[0].index, 1);
        assert_eq!(out.failures[1].index, 2);
        assert!(!out.failures[0].reason.is_empty());
        assert!(!out.failures[1].reason.is_empty());
        assert!(
            !out.failures[0].label.contains("not-a-uuid"),
            "label redacts userinfo, got {}",
            out.failures[0].label
        );
    }

    #[test]
    fn commit_creates_disabled_records_in_group() {
        let mut store = Store::open_in_memory().expect("store opens");
        let batch = import_text(&format!(
            "vless://{UUID}@a.example.com:443#one\ntrojan://pw@b.example.com:443#two"
        ));
        let outcome = commit_batch(&mut store, batch, "travel").expect("commit works");
        assert_eq!(outcome.imported, 2);
        assert_eq!(outcome.duplicates, 0);
        assert!(outcome.failures.is_empty());
        let rows = store.list().expect("list works");
        assert_eq!(rows.len(), 2);
        for row in &rows {
            assert!(!row.enabled, "import never enables");
            assert_eq!(row.group, "travel");
        }
        assert_eq!(rows[0].protocol, "vless");
    }

    #[test]
    fn commit_identical_reimport_never_duplicates() {
        let mut store = Store::open_in_memory().expect("store opens");
        let body = format!("vless://{UUID}@a.example.com:443#one");
        let first = commit_batch(&mut store, import_text(&body), "g").expect("commit works");
        assert_eq!(first.imported, 1);
        let second = commit_batch(&mut store, import_text(&body), "g").expect("recommit works");
        assert_eq!(second.imported, 0);
        assert_eq!(second.duplicates, 1);
        assert_eq!(store.list().expect("list works").len(), 1);
    }

    #[test]
    fn commit_blank_group_falls_back_to_default() {
        let mut store = Store::open_in_memory().expect("store opens");
        let batch = import_text(&format!("vless://{UUID}@a.example.com:443#one"));
        commit_batch(&mut store, batch, "   ").expect("commit works");
        let rows = store.list().expect("list works");
        assert_eq!(rows[0].group, DEFAULT_GROUP);
    }

    #[test]
    fn redact_link_hides_userinfo() {
        let redacted = redact_link("vless://my-secret-uuid@example.com:443#x");
        assert!(!redacted.contains("my-secret-uuid"), "got {redacted}");
        assert!(redacted.contains("example.com"), "got {redacted}");
    }

    /// Ticket 09: failure labels scrub query values (reality `pbk` and
    /// friends) while keeping the keys; a broken link carrying key material
    /// in the query exposes nothing via the import notification path.
    #[test]
    fn redact_link_scrubs_query_values_keeps_keys() {
        let redacted = redact_link(
            "vless://11111111-2222-4333-8444-555555555555@example.com:notaport?security=reality&pbk=SECRET-PBK&sid=1234#x",
        );
        assert!(!redacted.contains("SECRET-PBK"), "got {redacted}");
        assert!(!redacted.contains("sid=1234"), "got {redacted}");
        assert!(redacted.contains("pbk="), "key kept: {redacted}");
        assert!(redacted.contains("example.com"), "host kept: {redacted}");
    }

    /// Ticket 09: end-to-end over the notification path — a batch whose
    /// broken item carries secrets yields failure display strings
    /// (`item {index} ({label}): {reason}`) with no plaintext secret.
    #[test]
    fn failure_display_strings_carry_no_plaintext_secrets() {
        let body = [
            &format!("vless://{UUID}@a.example.com:443#ok"),
            "vless://not-a-uuid@b.example.com:notaport?security=reality&pbk=hunter2-public-key#bad",
            "trojan://s3cret-pw@c.example.com:443#also-ok",
        ]
        .join("\n");
        let out = import_text(&body);
        assert_eq!(out.failures.len(), 1);
        let failure = &out.failures[0];
        let display = format!(
            "item {} ({}): {}",
            failure.index + 1,
            failure.label,
            failure.reason
        );
        for canary in ["hunter2-public-key", "not-a-uuid", "s3cret-pw"] {
            assert!(!display.contains(canary), "leak in {display}");
        }
        assert!(display.contains("b.example.com"), "host kept: {display}");
    }

    // -- ticket 04: subscription bodies, identity, addsub --------------------

    fn b64_standard(text: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(text)
    }

    #[test]
    fn normalized_identity_ignores_remark() {
        let first =
            parse_link(&format!("vless://{UUID}@a.example.com:443#one")).expect("link parses");
        let second =
            parse_link(&format!("vless://{UUID}@a.example.com:443#two")).expect("link parses");
        assert_eq!(
            normalized_identity(&first.protocol, &first.params),
            normalized_identity(&second.protocol, &second.params),
            "remark is not identity"
        );
        let other =
            parse_link(&format!("vless://{UUID}@b.example.com:443#one")).expect("link parses");
        assert_ne!(
            normalized_identity(&first.protocol, &first.params),
            normalized_identity(&other.protocol, &other.params),
            "server is identity"
        );
    }

    #[test]
    fn subscription_body_base64_blob() {
        let inner =
            format!("vless://{UUID}@a.example.com:443#one\ntrojan://pw@b.example.com:443#two");
        let out = import_subscription_body(&b64_standard(&inner));
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 2);
    }

    #[test]
    fn subscription_body_json_array_and_sip008() {
        let array = serde_json::json!([
            {"type": "vless", "tag": "one", "server": "a.example.com", "server_port": 443, "uuid": UUID}
        ]);
        let out = import_subscription_body(&array.to_string());
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1);

        // Limited SIP008: typeless method+password items become Shadowsocks.
        let sip008 = serde_json::json!({
            "version": 2,
            "servers": [
                {"remarks": "ss-one", "server": "s.example.com", "server_port": 8388,
                 "method": "aes-256-gcm", "password": "hunter2"},
                {"remarks": "typed", "type": "trojan", "server": "t.example.com",
                 "server_port": 443, "password": "pw"},
                {"remarks": "broken", "server": "b.example.com"}
            ]
        });
        let out = import_subscription_body(&sip008.to_string());
        assert_eq!(out.configs.len(), 2, "failures: {:?}", out.failures);
        assert_eq!(out.failures.len(), 1, "typeless item without creds skips");
        assert_eq!(out.configs[0].protocol, "shadowsocks");
        assert_eq!(out.configs[0].name, "ss-one");
    }

    #[test]
    fn subscription_body_clash_limited() {
        let body = "proxies:\n  - name: clash-one\n    type: trojan\n    server: t.example.com\n    port: 443\n    password: pw\n  - name: clash-two\n    type: ss\n    server: s.example.com\n    port: 8388\n    cipher: aes-256-gcm\n    password: hunter2\n  - name: broken\n    type: wireguard\n";
        let out = import_subscription_body(body);
        assert_eq!(out.configs.len(), 2, "failures: {:?}", out.failures);
        assert_eq!(out.failures.len(), 1, "member without server skips");
        assert_eq!(out.configs[0].protocol, "trojan");
        assert_eq!(out.configs[1].protocol, "shadowsocks");
    }

    #[test]
    fn subscription_body_garbage_is_failure() {
        let out = import_subscription_body("bogus://x\nbogus://y");
        assert!(out.configs.is_empty());
        assert_eq!(out.failures.len(), 2);
        let empty = import_subscription_body("   \n ");
        assert!(empty.configs.is_empty());
        assert_eq!(empty.failures.len(), 1);
        assert_eq!(empty.failures[0].code, ImportErrorKind::Empty);
    }

    #[test]
    fn addsub_valid_with_group() {
        let link = format!(
            "throne://addsub/{}",
            b64_standard("https://example.com/sub.txt#travel")
        );
        let request = parse_addsub_link(&link).expect("addsub parses");
        assert_eq!(request.url, "https://example.com/sub.txt");
        assert_eq!(request.group, "travel");
    }

    #[test]
    fn addsub_missing_group_falls_back_to_default() {
        let link = format!(
            "throne://addsub/{}",
            b64_standard("https://example.com/sub.txt")
        );
        let request = parse_addsub_link(&link).expect("addsub parses");
        assert_eq!(request.group, DEFAULT_GROUP);
    }

    #[test]
    fn addsub_rejects_urlsafe_alphabet() {
        // Bytes 0xFB 0xFF encode with `-`/`_` in urlsafe: standard-only must fail.
        let urlsafe = base64::engine::general_purpose::URL_SAFE.encode([0xFBu8, 0xFF]);
        assert!(
            urlsafe.contains('-') || urlsafe.contains('_'),
            "fixture uses the urlsafe alphabet, got {urlsafe}"
        );
        let error = parse_addsub_link(&format!("throne://addsub/{urlsafe}"))
            .expect_err("urlsafe addsub fails");
        assert_eq!(error.kind, ImportErrorKind::BadBase64);
    }

    #[test]
    fn addsub_rejects_non_http_and_unknown_command() {
        let error = parse_addsub_link(&format!(
            "throne://addsub/{}",
            b64_standard("ftp://example.com/sub.txt#travel")
        ))
        .expect_err("non-http url fails");
        assert_eq!(error.kind, ImportErrorKind::BadHost);
        assert!(
            !error.message.contains("ftp://example.com"),
            "error hides the URL, got {error}"
        );
        let error = parse_addsub_link("throne://route/aGk").expect_err("route cmd fails here");
        assert_eq!(error.kind, ImportErrorKind::UnknownScheme);
    }

    // -- ticket 08: Xray/custom/extra/chain intake --------------------------

    #[test]
    fn xray_link_keeps_xray_kind() {
        let link = format!(
            "xray://{UUID}@example.com:443?encryption=none&security=tls&sni=example.com&flow=xtls-rprx-vision#xray-one"
        );
        let config = parse_link(&link).expect("xray parses");
        assert_eq!(config.protocol, "xray");
        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, 443);
        assert_eq!(config.name, "xray-one");
        let params = params_of(&config);
        assert_eq!(params["uuid"], UUID);
        assert_eq!(params["flow"], "xtls-rprx-vision");
    }

    #[test]
    fn json_outbound_ticket08_types_keep_kind() {
        for (raw, canonical) in [
            ("xray", "xray"),
            ("xray-vless", "xray"),
            ("custom-outbound", "custom-outbound"),
            ("custom", "custom-outbound"),
            ("custom-config", "custom-config"),
            ("custom-full", "custom-config"),
            ("extra-core", "extra-core"),
            ("extracore", "extra-core"),
            ("chain", "chain"),
        ] {
            let body = serde_json::json!([{
                "type": raw, "tag": "k",
                "server": "a.example.com", "server_port": 443, "uuid": UUID
            }]);
            let out = import_text(&body.to_string());
            assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
            assert_eq!(out.configs.len(), 1);
            assert_eq!(out.configs[0].protocol, canonical, "type {raw}");
        }
    }

    #[test]
    fn throne_add_custom_json_imports_as_custom_row() {
        let outbound = serde_json::json!({
            "type": "custom-outbound", "tag": "custom-one",
            "server": "c.example.com", "server_port": 443,
            "fancy": "kept", "uuid": UUID
        });
        let link = format!("throne://add/{}", b64(&outbound.to_string()));
        let config = parse_link(&link).expect("throne custom add parses");
        assert_eq!(config.protocol, "custom-outbound");
        assert_eq!(config.name, "custom-one");
        assert_eq!(params_of(&config)["fancy"], "kept");
    }

    #[test]
    fn clash_ticket08_types_fold_to_kind() {
        let body = "proxies:\n  - name: x-one\n    type: xray\n    server: x.example.com\n    port: 443\n    uuid: u\n  - name: e-one\n    type: extra-core\n    server: e.example.com\n    port: 443\n    exe: core.exe\n";
        let out = import_subscription_body(body);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 2);
        assert_eq!(out.configs[0].protocol, "xray");
        assert_eq!(out.configs[1].protocol, "extra-core");
    }

    // -- ticket 14: import-safety ------------------------------------------

    /// Only single-line `throne://addsub/...` routes to the subscription
    /// intake; every other single line keeps the generic import path.
    #[test]
    fn is_addsub_attempt_routes_only_addsub_lines() {
        assert!(is_addsub_attempt("throne://addsub/aGk"));
        assert!(is_addsub_attempt("  THRONE://ADDSUB/aGk  "));
        assert!(is_addsub_attempt("throne://addsub"));
        assert!(!is_addsub_attempt("throne://add/aGk"));
        assert!(!is_addsub_attempt("throne://route/aGk"));
        assert!(!is_addsub_attempt("vless://x@y.example.com:443"));
        assert!(!is_addsub_attempt("not a link"));
        assert!(!is_addsub_attempt(""));
    }

    /// Ticket 15: trailing-slash ss links tolerate the slash in every
    /// userinfo branch (plain, base64-split, legacy blob).
    #[test]
    fn ss_trailing_slash_tolerated() {
        let cases = [
            "ss://aes-256-gcm:hunter2@example.com:8388/".to_string(),
            "ss://aes-256-gcm:hunter2@example.com:8388/#with-remark".to_string(),
            format!("ss://{}@example.com:8388/", b64("aes-256-gcm:hunter2")),
            format!("ss://{}/", b64("aes-256-gcm:hunter2@example.com:8388")),
        ];
        for link in &cases {
            let config = parse_link(link).unwrap_or_else(|_| panic!("{link} parses"));
            assert_eq!(config.protocol, "shadowsocks", "link {link}");
            assert_eq!(config.host, "example.com", "link {link}");
            assert_eq!(config.port, 8388, "link {link}");
            assert_eq!(params_of(&config)["method"], "aes-256-gcm", "link {link}");
            assert_eq!(params_of(&config)["password"], "hunter2", "link {link}");
        }
    }

    /// Ticket 15: pasted lowercase `[interface]`/`[peer]` markers parse as
    /// WireGuard conf, agreeing with the drop path (stem aside).
    #[test]
    fn lowercase_interface_conf_parses_like_canonical() {
        let conf = "[interface]\nPrivateKey = cHJpdmF0ZWtleQ\n\n[peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\n";
        let pasted = import_text(conf);
        assert!(
            pasted.failures.is_empty(),
            "failures: {:?}",
            pasted.failures
        );
        assert_eq!(pasted.configs.len(), 1);
        assert_eq!(pasted.configs[0].protocol, "wireguard");
        assert_eq!(pasted.configs[0].host, "203.0.113.1");
        let dropped = import_file_content("tun.conf", conf);
        assert_eq!(dropped.configs.len(), pasted.configs.len());
        assert_eq!(dropped.configs[0].host, pasted.configs[0].host);
        assert_eq!(dropped.configs[0].name, "tun-p1");
    }

    /// Ticket 15: a `://` inside a conf comment no longer defeats detection.
    #[test]
    fn wg_conf_comment_with_url_still_detected() {
        let conf = "# see https://example.com/docs\n[Interface]\nPrivateKey = cHJpdmF0ZWtleQ\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\n";
        let out = import_text(conf);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1);
        assert_eq!(out.configs[0].protocol, "wireguard");
    }

    /// Ticket 15: `.conf` files holding JSON or link lines are content-sniffed
    /// (never forced through the wg parser with a wrong-taxonomy error),
    /// while real conf content still parses as conf.
    #[test]
    fn conf_file_holding_json_or_links_is_sniffed() {
        let array = serde_json::json!([
            {"type": "vless", "tag": "json-one", "server": "a.example.com", "server_port": 443, "uuid": UUID}
        ]);
        let out = import_file_content("odd.conf", &array.to_string());
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1);
        assert_eq!(out.configs[0].name, "json-one");

        let links =
            format!("vless://{UUID}@a.example.com:443#one\ntrojan://pw@b.example.com:443#two");
        let out = import_file_content("odd.conf", &links);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 2);

        let conf = "[Interface]\nPrivateKey = cHJpdmF0ZWtleQ\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\n";
        let out = import_file_content("tun.conf", conf);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1);
        assert_eq!(out.configs[0].name, "tun-p1");
    }

    /// Ticket 15: pasted Clash YAML imports with the same limited support as
    /// subscription bodies.
    #[test]
    fn pasted_clash_yaml_imports_like_subscription() {
        let body = "proxies:\n  - name: clash-one\n    type: trojan\n    server: t.example.com\n    port: 443\n    password: pw\n";
        let pasted = import_text(body);
        assert!(
            pasted.failures.is_empty(),
            "failures: {:?}",
            pasted.failures
        );
        assert_eq!(pasted.configs.len(), 1);
        assert_eq!(pasted.configs[0].protocol, "trojan");
        assert_eq!(pasted.configs[0].name, "clash-one");
        let via_sub = import_subscription_body(body);
        assert_eq!(via_sub.configs.len(), pasted.configs.len());
    }

    /// Ticket 15: BOM-prefixed JSON parses after the BOM is stripped.
    #[test]
    fn bom_prefixed_json_parses() {
        let array = serde_json::json!([
            {"type": "vless", "tag": "bom-one", "server": "a.example.com", "server_port": 443, "uuid": UUID}
        ]);
        let body = format!("\u{FEFF}{}", array);
        let out = import_text(&body);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1);
        assert_eq!(out.configs[0].name, "bom-one");
        let via_sub = import_subscription_body(&body);
        assert_eq!(via_sub.configs.len(), 1);
    }

    /// Ticket 15: dotfile `.conf` gets a sensible stem, never `.conf-p1`.
    #[test]
    fn dotfile_conf_gets_sensible_stem() {
        let conf = "[Interface]\nPrivateKey = cHJpdmF0ZWtleQ\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\n";
        let out = import_file_content(".conf", conf);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1);
        assert_eq!(out.configs[0].name, "wireguard-p1");
    }

    /// Ticket 15: paste and drop agree on the same conf bytes (stem aside).
    #[test]
    fn paste_and_drop_agree_on_conf_bytes() {
        let conf = "; lan endpoint\n[Interface]\nPrivateKey = cHJpdmF0ZWtleQ\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.9:51820\n";
        let pasted = import_text(conf);
        let dropped = import_file_content("office.conf", conf);
        assert_eq!(pasted.configs.len(), 1);
        assert_eq!(dropped.configs.len(), 1);
        assert_eq!(dropped.configs[0].host, pasted.configs[0].host);
        assert_eq!(dropped.configs[0].port, pasted.configs[0].port);
        assert_eq!(dropped.configs[0].name, "office-p1");
    }

    /// Total failure renders every item with index/label/reason and no
    /// credentials (labels arrive redacted from the import path).
    #[test]
    fn format_import_failures_lists_every_item_credential_free() {
        let body = "vless://not-a-uuid@b.example.com:notaport?security=reality&pbk=hunter2-public-key#bad\nbogus://x\ntrojan://s3cret-pw@c.example.com:99999";
        let out = import_text(body);
        assert_eq!(out.configs.len(), 0);
        assert_eq!(out.failures.len(), 3);
        let text = format_import_failures(&out.failures);
        for (n, failure) in out.failures.iter().enumerate() {
            assert!(
                text.contains(&format!("item {}", n + 1)),
                "index in: {text}"
            );
            assert!(text.contains(&failure.label), "label in: {text}");
            assert!(text.contains(&failure.reason), "reason in: {text}");
        }
        for canary in ["not-a-uuid", "hunter2-public-key", "s3cret-pw"] {
            assert!(!text.contains(canary), "leak in {text}");
        }
        assert!(text.contains("b.example.com"), "host kept: {text}");
    }

    // -- ticket 16: validation coherence + dedupe/group consistency --------

    /// Scenario 3: same server with different case/whitespace, or a flow-only
    /// change, shares the normalized identity — commit dedupes like refresh.
    #[test]
    fn commit_dedupes_on_normalized_identity() {
        let mut store = Store::open_in_memory().expect("store opens");
        let first = import_text(&format!("vless://{UUID}@a.example.com:443#one"));
        let outcome = commit_batch(&mut store, first, "g").expect("commit works");
        assert_eq!(outcome.imported, 1);

        // Different case + remark: same identity, counted as duplicate.
        let upper = import_text(&format!("vless://{UUID}@A.EXAMPLE.COM:443#two"));
        assert_eq!(
            normalized_identity(&upper.configs[0].protocol, &upper.configs[0].params),
            normalized_identity("vless", &store.list().expect("list").remove(0).params),
            "case is not identity"
        );
        let outcome = commit_batch(&mut store, upper, "g").expect("recommit works");
        assert_eq!(outcome.imported, 0, "case variant adds nothing");
        assert_eq!(outcome.duplicates, 1);

        // Flow-only change rides outside the identity key: still one row.
        let flowed = import_text(&format!(
            "vless://{UUID}@a.example.com:443?flow=xtls-rprx-vision#three"
        ));
        let outcome = commit_batch(&mut store, flowed, "g").expect("recommit works");
        assert_eq!(outcome.imported, 0, "flow variant adds nothing");
        assert_eq!(outcome.duplicates, 1);
        assert_eq!(store.list().expect("list works").len(), 1);
    }

    /// Normalized identity trims surrounding whitespace before lowercasing.
    #[test]
    fn normalized_identity_trims_server_whitespace() {
        let a = normalized_identity(
            "vless",
            "{\"type\":\"vless\",\"server\":\"  A.Example.COM  \",\"server_port\":443,\"uuid\":\"u\"}",
        );
        let b = normalized_identity(
            "VLESS",
            "{\"type\":\"vless\",\"server\":\"a.example.com\",\"server_port\":443,\"uuid\":\"u\"}",
        );
        assert_eq!(a, b, "whitespace/case are not identity");
    }

    /// Scenario 5: a `.conf` hint with edited link text is stale — cleared so
    /// the edited text sniffs as its own type.
    #[test]
    fn stale_conf_hint_cleared_for_link_text() {
        let links = format!("vless://{UUID}@a.example.com:443#one");
        assert_eq!(effective_hint_for_text(Some("drop.conf"), &links), None);
        let conf = "[Interface]\nPrivateKey = cHJpdmF0ZWtleQ\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\n";
        assert_eq!(
            effective_hint_for_text(Some("drop.conf"), conf),
            Some("drop.conf".to_string())
        );
        assert_eq!(
            effective_hint_for_text(Some("notes.txt"), &links),
            Some("notes.txt".to_string())
        );
        assert_eq!(effective_hint_for_text(None, &links), None);
    }

    /// Scenario 6: two `.conf` files keep their own stems and PrivateKeys.
    #[test]
    fn multi_file_confs_keep_per_file_stems_and_keys() {
        let a = "[Interface]\nPrivateKey = QUJD\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\n";
        let b = "[Interface]\nPrivateKey = WFl6\n\n[Peer]\nPublicKey = cHVia2V5Mg\nEndpoint = 203.0.113.2:51820\n";
        let out = import_multi_file_contents(&[("a.conf", a), ("b.conf", b)]);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 2);
        assert_eq!(out.configs[0].name, "a-p1");
        assert_eq!(out.configs[1].name, "b-p1");
        let params_a: serde_json::Value =
            serde_json::from_str(&out.configs[0].params).expect("params JSON");
        let params_b: serde_json::Value =
            serde_json::from_str(&out.configs[1].params).expect("params JSON");
        assert_eq!(params_a["private_key"], "QUJD");
        assert_eq!(params_b["private_key"], "WFl6");
        assert_ne!(params_a["private_key"], params_b["private_key"]);
    }

    /// Scenario 6 (bleed guard): concatenated conf text without the multi
    /// helper still binds each peer to its preceding Interface key.
    #[test]
    fn concatenated_conf_text_does_not_bleed_private_keys() {
        let merged = "[Interface]\nPrivateKey = QUJD\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\n[Interface]\nPrivateKey = WFl6\n\n[Peer]\nPublicKey = cHVia2V5Mg\nEndpoint = 203.0.113.2:51820\n";
        let out = import_text(merged);
        // Paste path uses the single "wireguard" stem, but keys stay bound.
        assert_eq!(out.configs.len(), 2, "failures: {:?}", out.failures);
        let params: Vec<serde_json::Value> = out
            .configs
            .iter()
            .map(|config| serde_json::from_str(&config.params).expect("params JSON"))
            .collect();
        assert_eq!(params[0]["private_key"], "QUJD");
        assert_eq!(params[1]["private_key"], "WFl6");
    }
}
