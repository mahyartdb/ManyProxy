//! Ticket 03: Throne-compatible import grammar pack.
//!
//! Pure parsers turning share links, JSON bodies, WireGuard conf files and
//! `throne://add` payloads into [`ImportedConfig`] records. This module does
//! no I/O, writes nothing, starts nothing, and never panics on malformed
//! input: [`import_text`] / [`import_file_content`] split a body into items
//! and collect per-item [`ImportFailure`]s (with the item index) instead of
//! failing the batch. [`commit_batch`] appends parsed items to the library as
//! disabled records in the chosen group and skips exact re-imports
//! (widened identity: protocol + params plus transport/security extras).
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
    /// Ticket 28: single source of truth is [`ImportErrorKind::as_str`]
    /// (one variant list instead of two parallel match tables).
    pub fn from_str(raw: &str) -> Option<Self> {
        use ImportErrorKind::*;
        const ALL: &[ImportErrorKind] = &[
            Empty,
            UnknownScheme,
            BadBase64,
            NonUtf8,
            BadHost,
            BadPort,
            MissingCredential,
            InvalidJson,
            SsPlugin,
        ];
        ALL.iter().copied().find(|kind| kind.as_str() == raw)
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
        // Ticket 24 (SEC L1, LOW caps): labels are bounded display strings —
        // never a paste of unbounded user input.
        let label = label.into();
        let label = if label.chars().count() > 96 {
            format!("{}…", label.chars().take(96).collect::<String>())
        } else {
            label
        };
        Self {
            index,
            label,
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

/// Ticket 24: reject absurdly large base64 payloads before decoding (cap on
/// the compacted text; error is static, never echoes input).
const MAX_B64_COMPACT: usize = 1_000_000;

/// Ticket 24: query keys/values longer than this are dropped (caps on
/// query strings; overlong material is never a credential).
const MAX_QUERY_ITEM: usize = 4096;

/// Ticket 24: display names (remarks/fragments/tags) truncate here.
const MAX_NAME_CHARS: usize = 256;

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
    if compact.len() > MAX_B64_COMPACT {
        return Err(ImportError::new(
            ImportErrorKind::BadBase64,
            "bad base64 payload".to_string(),
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

/// Strip leading UTF-8 BOMs, if present. `str::trim` leaves U+FEFF alone
/// (it is not White_Space), so BOM-prefixed bodies must strip it before any
/// `{`/`[`/section sniffing. Ticket 24: loops (several BOMs collapse) —
/// every entry point funnels through here or [`parse_link`].
fn strip_bom(text: &str) -> &str {
    let mut rest = text;
    while let Some(stripped) = rest.strip_prefix('\u{FEFF}') {
        rest = stripped;
    }
    rest
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
/// takes the first entry (first-wins). Ticket 24: strict — leading zeros
/// (`0443`) and explicit signs (`+443`) are rejected, not normalized.
fn parse_port(text: &str) -> Result<u16, ImportError> {
    let first = text.split([',', ';']).next().unwrap_or("").trim();
    if first.len() > 1 && first.starts_with('0') {
        return Err(bad_port(format!("bad port '{text}'")));
    }
    // Rust's unsigned parse accepts a leading `+`: reject explicit signs.
    if first.starts_with('+') || first.starts_with('-') {
        return Err(bad_port(format!("bad port '{text}'")));
    }
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
    /// Ticket 24 (M1): the URI path is no longer silently dropped — parsers
    /// preserve it into transport opts (or reject it explicitly).
    path: String,
}

fn split_uri(_scheme: &str, rest: &str) -> Result<UriParts, ImportError> {
    let (before_frag, fragment) = match rest.find('#') {
        Some(i) => (&rest[..i], rest[i + 1..].to_string()),
        None => (rest, String::new()),
    };
    let (before_query, query_raw) = match before_frag.find('?') {
        Some(i) => (&before_frag[..i], &before_frag[i + 1..]),
        None => (before_frag, ""),
    };
    let authority_end = before_query.find('/').unwrap_or(before_query.len());
    let authority = &before_query[..authority_end];
    // Ticket 24 (M2): percent-decode the host so link/JSON identities agree
    // (`exam%70le.com` and `example.com` are the same server).
    let (userinfo, host_port) = match authority.rfind('@') {
        Some(i) => (Some(authority[..i].to_string()), &authority[i + 1..]),
        None => (None, authority),
    };
    let (raw_host, port_text) = split_host_port(host_port)?;
    // Ticket 28: the old `.map_err` here returned its error unchanged on
    // every path (the hy2 branch was a `let _ =` no-op) — deleted, behavior
    // identical. Port omission stays legal for hy2 via `parse_hy2`'s 443
    // default downstream.
    let host = percent_decode(raw_host.trim(), false);
    if host.trim().is_empty() {
        return Err(bad_host("missing host"));
    }
    Ok(UriParts {
        userinfo,
        host,
        port_text,
        query: query_map(query_raw),
        fragment,
        path: before_query[authority_end..].to_string(),
    })
}

/// Query pairs, percent-decoded with form (`+` → space) semantics, unordered
/// with unique names (first wins). Ticket 24: names are case-sensitive
/// (spec #716 — `PBK` never satisfies a `pbk` check; the reality error says
/// so); empty keys are dropped (no empty-key pollution); overlong items are
/// dropped (length caps).
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
        if key.len() > MAX_QUERY_ITEM || value.len() > MAX_QUERY_ITEM {
            continue;
        }
        let key = percent_decode(key, true);
        if key.is_empty() {
            continue;
        }
        map.entry(key)
            .or_insert_with(|| percent_decode(value, true));
    }
    map
}

fn display_name(fragment: &str, protocol: &str, host: &str, port: u16) -> String {
    let remark = percent_decode(fragment, false);
    let remark = remark.trim();
    if remark.is_empty() {
        format!("{protocol}-{host}:{port}")
    } else {
        cap_name(remark.to_string())
    }
}

/// Ticket 28: one display-name length cap (link remarks and JSON tags share
/// `MAX_NAME_CHARS`; the truncation itself never carries secrets).
fn cap_name(name: String) -> String {
    if name.chars().count() > MAX_NAME_CHARS {
        name.chars().take(MAX_NAME_CHARS).collect()
    } else {
        name
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
/// Ticket 24 (HIGH extras-overwrite): core fields always win — a key the
/// parser already set, or one of the credential/authority [`CORE_KEYS`], is
/// never overwritten by `?password=fake`/`?server=evil`/`?uuid=` extras.
const CORE_KEYS: &[&str] = &[
    "uuid",
    "password",
    "auth",
    "username",
    "token",
    "server",
    "server_port",
    "port",
    "private_key",
    "privatekey",
    "public_key",
    "publickey",
    "method",
    "type",
    "protocol",
];

fn insert_extras(params: &mut Params, query: &HashMap<String, String>, consumed: &[&str]) {
    for (key, value) in query {
        if consumed.contains(&key.as_str()) || CORE_KEYS.contains(&key.as_str()) {
            continue;
        }
        if params.contains_key(key) {
            continue;
        }
        insert_str(params, key, value.clone());
    }
}

fn finish(params: Params) -> String {
    serde_json::Value::Object(params).to_string()
}

/// VLESS transport names. Ticket 24 (M13): a `type` that is absent defaults
/// to `tcp`; a present-but-unknown value is preserved verbatim (never
/// silently rewritten) so link/JSON verdicts agree and nothing is hidden.
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
        Some(value) if !value.trim().is_empty() => value.clone(),
        _ => "tcp".to_string(),
    }
}

/// Ticket 24 (M12): an empty/blank `sni` query value falls back to the host,
/// exactly like an absent one.
fn query_sni(query: &HashMap<String, String>, host: &str) -> String {
    query
        .get("sni")
        .filter(|sni| !sni.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| host.to_string())
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
        // Ticket 24 (M11): query keys are case-sensitive — a `PBK=` that
        // misses the `pbk` check says so instead of just "missing pbk".
        if query.keys().any(|key| key.eq_ignore_ascii_case("pbk")) {
            return Err(missing_credential(
                "missing pbk (required for reality; query keys are case-sensitive)",
            ));
        }
        return Err(missing_credential("missing pbk (required for reality)"));
    }
    let sni = query_sni(query, &parts.host);

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
    // Ticket 24 (M1): a URI path the grammar cannot express elsewhere rides
    // into transport opts instead of being silently dropped.
    let uri_path = parts.path.trim_matches('/');
    if !uri_path.is_empty() && !query.contains_key("path") {
        insert_str(&mut params, "path", uri_path.to_string());
    }
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
            "uuid",
            "password",
            "server",
            "server_port",
            "port",
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
/// Ticket 24 (M3): a `#remark` on the link overrides the embedded `ps`.
fn parse_vmess_legacy(payload: &str, fragment: &str) -> Result<ImportedConfig, ImportError> {
    let text = decode_b64_utf8(payload)?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| invalid_json("invalid vmess JSON"))?;
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
    // Ticket 24 (HIGH empty host, M16 host whitespace): trim first, reject
    // empty post-strip (`"add":"[]"`), and reject unclosed brackets.
    let host_raw = field(obj, "add")
        .filter(|h| !h.trim().is_empty())
        .ok_or_else(|| {
            ImportError::new(ImportErrorKind::BadHost, "vmess link is missing \"add\"")
        })?;
    let host_trimmed = host_raw.trim();
    let host = if let Some(inner) = host_trimmed.strip_prefix('[') {
        let inner = inner
            .strip_suffix(']')
            .ok_or_else(|| bad_host("vmess link has a bad \"add\" (unclosed '[')"))?;
        if inner.trim().is_empty() {
            return Err(bad_host("vmess link has a bad \"add\" (empty host)"));
        }
        inner.to_string()
    } else {
        host_trimmed.to_string()
    };
    let port_raw = field(obj, "port").unwrap_or_default();
    let port = parse_port(&port_raw)
        .map_err(|_| bad_port(format!("vmess link has a bad \"port\" ({port_raw})")))?;
    let uuid = field(obj, "id")
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| missing_credential("vmess link is missing \"id\""))?;
    let name = if !fragment.trim().is_empty() {
        display_name(fragment, "vmess", &host, port)
    } else {
        let ps = field(obj, "ps").unwrap_or_default();
        if ps.trim().is_empty() {
            format!("vmess-{host}:{port}")
        } else {
            ps
        }
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
    // Ticket 24 (M16): boolean `"tls": true` counts, not just `"tls": "tls"`.
    let tls_raw = match obj.get("tls") {
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(serde_json::Value::Bool(flag)) => flag.to_string(),
        _ => String::new(),
    };
    params.insert(
        "tls".to_string(),
        serde_json::Value::Bool(tls_raw == "tls" || tls_raw == "true"),
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
                // `base64(method:password)`: SIP002 percent-encodes before
                // base64, so decode after (parity with the plain branch).
                let text = decode_b64_utf8(userinfo_raw)?;
                let j = text
                    .find(':')
                    .ok_or_else(|| missing_credential("ss link is missing password"))?;
                (
                    percent_decode(&text[..j], false),
                    percent_decode(&text[j + 1..], false),
                )
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
            // Ticket 24 (M14): inner-slash trim parity with the split
            // branch, plus the same percent-decode parity.
            let (host, port_text) = split_host_port(host_port.trim_end_matches('/'))?;
            (
                percent_decode(&userinfo[..j], false),
                percent_decode(&userinfo[j + 1..], false),
                host,
                require_port(port_text.as_deref())?,
            )
        }
    };
    if method.trim().is_empty() {
        return Err(missing_credential("ss link is missing method"));
    }
    // Ticket 24 (M14): whitespace-only passwords are missing, not empty-ok.
    if password.trim().is_empty() {
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

/// Hysteria2 (`hysteria2://`, `hy2://`): auth required, port omitted → 443,
/// multi-port first-wins. Ticket 42: the `hysteria2+realm` rendezvous scheme
/// never reaches here — [`parse_link`] rejects it with guidance (the realm
/// block conflicts with server addressing and has no link mapping).
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
    let sni = query_sni(query, &parts.host);
    let mut params = base_params("hysteria2", &parts.host, port);
    insert_str(&mut params, "password", auth);
    insert_str(&mut params, "sni", sni);
    if scheme == "hysteria2+realm" {
        insert_str(&mut params, "variant", "realm".to_string());
    }
    insert_extras(
        &mut params,
        query,
        &["sni", "password", "server", "server_port", "port"],
    );
    Ok(ImportedConfig {
        name: display_name(&parts.fragment, "hysteria2", &parts.host, port),
        protocol: "hysteria2".to_string(),
        params: finish(params),
        host: parts.host.clone(),
        port,
    })
}

/// Ticket 42: snell link (`snell://psk@host:port?version=4&…`). The psk
/// rides in the userinfo (or `?psk=`); `version` validates here (4|6,
/// default 4) so a bad version rejects at import, never at start. Known
/// option spellings fold to sing-box names; anything else passes through
/// for the engine's strict per-protocol check (typed bad-option, never a
/// silent mis-serve).
fn parse_snell(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    let mut psk = percent_decode(parts.userinfo.as_deref().unwrap_or(""), false);
    if psk.trim().is_empty() {
        psk = parts.query.get("psk").cloned().unwrap_or_default();
    }
    if psk.trim().is_empty() {
        return Err(missing_credential("snell link is missing psk"));
    }
    let version = parts
        .query
        .get("version")
        .map(|version| version.trim().to_string())
        .filter(|version| !version.is_empty())
        .unwrap_or_else(|| "4".to_string());
    if version != "4" && version != "6" {
        return Err(invalid_json(format!(
            "snell link has a bad \"version\" ({version})"
        )));
    }
    let port = require_port(parts.port_text.as_deref())?;
    let mut params = base_params("snell", &parts.host, port);
    insert_str(&mut params, "psk", psk);
    insert_str(&mut params, "version", version);
    for (query_key, json_key) in [
        ("obfs", "obfs_mode"),
        ("obfs-mode", "obfs_mode"),
        ("obfs_mode", "obfs_mode"),
        ("obfs-host", "obfs_host"),
        ("obfs_host", "obfs_host"),
        ("userkey", "userkey"),
        ("reuse", "reuse"),
        ("mode", "mode"),
        ("network", "network"),
    ] {
        if let Some(value) = parts.query.get(query_key) {
            if !value.trim().is_empty() && !params.contains_key(json_key) {
                insert_str(&mut params, json_key, value.clone());
            }
        }
    }
    insert_extras(
        &mut params,
        &parts.query,
        &[
            "psk",
            "version",
            "obfs",
            "obfs-mode",
            "obfs_mode",
            "obfs-host",
            "obfs_host",
            "userkey",
            "reuse",
            "mode",
            "network",
            "server",
            "server_port",
            "port",
        ],
    );
    Ok(ImportedConfig {
        name: display_name(&parts.fragment, "snell", &parts.host, port),
        protocol: "snell".to_string(),
        params: finish(params),
        host: parts.host.clone(),
        port,
    })
}

/// Ticket 42: shadowTLS link (`shadowtls://password@host:port?version=3&…`,
/// `shadow-tls://` alias). The password rides in the userinfo (empty only
/// for v1); `version` validates here (1|2|3) so a bad version rejects at
/// import. With a password but no version the row defaults to 3 (the
/// passwordless v1 shape omits the key and takes the schema default 1);
/// `sni` falls back to the host exactly like the other TLS protocols.
fn parse_shadowtls(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    let password = percent_decode(parts.userinfo.as_deref().unwrap_or(""), false);
    let version = match parts
        .query
        .get("version")
        .map(|version| version.trim().to_string())
        .filter(|version| !version.is_empty())
    {
        None => {
            if password.trim().is_empty() {
                None
            } else {
                Some("3".to_string())
            }
        }
        Some(version) if ["1", "2", "3"].contains(&version.as_str()) => Some(version),
        Some(version) => {
            return Err(invalid_json(format!(
                "shadowtls link has a bad \"version\" ({version})"
            )));
        }
    };
    let port = require_port(parts.port_text.as_deref())?;
    let sni = query_sni(&parts.query, &parts.host);
    let mut params = base_params("shadowtls", &parts.host, port);
    if let Some(version) = version {
        insert_str(&mut params, "version", version);
    }
    if !password.trim().is_empty() {
        insert_str(&mut params, "password", password);
    }
    insert_str(&mut params, "sni", sni);
    insert_extras(
        &mut params,
        &parts.query,
        &[
            "version",
            "password",
            "sni",
            "server",
            "server_port",
            "port",
        ],
    );
    Ok(ImportedConfig {
        name: display_name(&parts.fragment, "shadowtls", &parts.host, port),
        protocol: "shadowtls".to_string(),
        params: finish(params),
        host: parts.host.clone(),
        port,
    })
}

/// Ticket 42: protocols with no sing-box 1.14.2 outbound schema reject at
/// import with per-protocol guidance (never parse-ok/start-fail). Messages
/// name the scheme and the gap only — never userinfo or hosts.
fn unsupported_protocol_guidance(scheme: &str) -> ImportError {
    let detail = match scheme {
        "mieru" => "mieru has no sing-box 1.14.2 outbound schema (only the third-party mbox fork serves mieru) — import rejected; serve it via a custom full-config row on a fork core instead",
        "juicity" => "juicity has no sing-box outbound schema (upstream feature request, never merged) — import rejected; serve it via a custom full-config row instead",
        "openvpn" => "openvpn is a sing-box endpoint (openvpn-client), not a 1.14.2 outbound — import rejected; serve it via a custom full-config row instead",
        "masque" => "masque-client is a sing-box 1.15.0+ endpoint, not a 1.14.2 outbound — import rejected; serve it via a custom full-config row on a newer core instead",
        "trusttunnel" => "trusttunnel has no sing-box 1.14.2 outbound schema — import rejected; serve it via a custom full-config row instead",
        _ => "this protocol has no sing-box 1.14.2 outbound schema — import rejected",
    };
    unknown_scheme(format!("unsupported scheme '{scheme}' ({detail})"))
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
    insert_extras(
        &mut params,
        &parts.query,
        &["auth", "server", "server_port", "port"],
    );
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
    insert_extras(
        &mut params,
        &parts.query,
        &["uuid", "password", "server", "server_port", "port"],
    );
    Ok(ImportedConfig {
        name: display_name(&parts.fragment, "tuic", &parts.host, port),
        protocol: "tuic".to_string(),
        params: finish(params),
        host: parts.host.clone(),
        port,
    })
}

/// WireGuard-family URI: private key in userinfo, `publickey` required.
/// Ticket 28: one parser for `wireguard`/`wg` and the AmneziaWG variants
/// (`awg://`, `amneziawg://`, `amnezia-wg://`) — same authority shape; every
/// other query key is preserved verbatim into params for the fork-core
/// version-matrix validator (engine.rs). Only the protocol name (params,
/// display name, and credential errors) varies.
fn parse_wg_like(protocol: &str, parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    let private_key = percent_decode(parts.userinfo.as_deref().unwrap_or(""), false);
    if private_key.trim().is_empty() {
        return Err(missing_credential(format!(
            "{protocol} link is missing private key"
        )));
    }
    let query = &parts.query;
    let public_key = query.get("publickey").cloned().unwrap_or_default();
    if public_key.trim().is_empty() {
        return Err(missing_credential(format!(
            "{protocol} link is missing publickey"
        )));
    }
    let port = require_port(parts.port_text.as_deref())?;
    let mut params = base_params(protocol, &parts.host, port);
    insert_str(&mut params, "private_key", private_key);
    insert_str(&mut params, "public_key", public_key);
    insert_extras(
        &mut params,
        query,
        &[
            "publickey",
            "private_key",
            "privatekey",
            "public_key",
            "server",
            "server_port",
            "port",
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

/// WireGuard URI: private key in userinfo, `publickey` required.
fn parse_wg_uri(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    parse_wg_like("wireguard", parts)
}

/// Ticket 07: AmneziaWG-variant URI (`awg://`, `amneziawg://`,
/// `amnezia-wg://`): same authority shape as WireGuard (private key in
/// userinfo, `publickey` required); every other query key — `awg_version`,
/// `jc`/`jmin`/`jmax`, `s/h/i` ranges, 3.x additions — is preserved verbatim
/// into params for the fork-core version-matrix validator (engine.rs).
fn parse_awg_uri(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    parse_wg_like("amneziawg", parts)
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
                // Ticket 24 (M15): a present-but-empty user or password is
                // rejected, never stored as an empty credential.
                let username = percent_decode(&raw[..i], false);
                let password = percent_decode(&raw[i + 1..], false);
                if username.trim().is_empty() || password.is_empty() {
                    return Err(missing_credential(format!(
                        "{protocol} link has empty credentials"
                    )));
                }
                insert_str(&mut params, "username", username);
                insert_str(&mut params, "password", password);
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
    // Ticket 42: `https` links are TLS by definition — the engine serves
    // them through the same `http` outbound with TLS on.
    if protocol == "https" {
        insert_str(&mut params, "security", "tls".to_string());
    }
    insert_extras(
        &mut params,
        &parts.query,
        &[
            "username",
            "password",
            "server",
            "server_port",
            "port",
            "version",
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

fn parse_anytls(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    let password = percent_decode(parts.userinfo.as_deref().unwrap_or(""), false);
    if password.is_empty() {
        return Err(missing_credential("anytls link is missing password"));
    }
    let port = require_port(parts.port_text.as_deref())?;
    let mut params = base_params("anytls", &parts.host, port);
    insert_str(&mut params, "password", password);
    insert_extras(
        &mut params,
        &parts.query,
        &["password", "server", "server_port", "port"],
    );
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
    insert_extras(
        &mut params,
        &parts.query,
        &["username", "password", "server", "server_port", "port"],
    );
    Ok(ImportedConfig {
        name: display_name(&parts.fragment, "naive", &parts.host, port),
        protocol: "naive".to_string(),
        params: finish(params),
        host: parts.host.clone(),
        port,
    })
}

/// Ticket 27: SSH link (`ssh://user[:password]@host[:port]?…`).
/// Native sing-box `ssh` outbound (user + password or private key). Port
/// omitted → 22. A `private_key`/`privatekey` query value also satisfies the
/// secret half (never echoed in errors).
fn parse_ssh(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    let raw = parts.userinfo.as_deref().unwrap_or("");
    let (user, password) = match raw.find(':') {
        Some(i) => (
            percent_decode(&raw[..i], false),
            percent_decode(&raw[i + 1..], false),
        ),
        None => (percent_decode(raw, false), String::new()),
    };
    if user.trim().is_empty() {
        return Err(missing_credential("ssh link is missing user"));
    }
    let query_key = parts
        .query
        .get("private_key")
        .or_else(|| parts.query.get("privatekey"))
        .map(|key| !key.trim().is_empty())
        .unwrap_or(false);
    if password.is_empty() && !query_key {
        return Err(missing_credential(
            "ssh link is missing password (or ?private_key=)",
        ));
    }
    let port = match parts.port_text.as_deref() {
        Some(text) => parse_port(text)?,
        None => 22,
    };
    let mut params = base_params("ssh", &parts.host, port);
    insert_str(&mut params, "user", user.clone());
    insert_str(&mut params, "username", user);
    if !password.is_empty() {
        insert_str(&mut params, "password", password);
    }
    // Ticket 24 (HIGH extras-overwrite): `private_key` is a CORE_KEY, so
    // extras never carry it — read it explicitly like WireGuard's publickey.
    for key in ["private_key", "privatekey"] {
        if let Some(value) = parts.query.get(key) {
            if !value.trim().is_empty() {
                insert_str(&mut params, "private_key", value.clone());
                break;
            }
        }
    }
    insert_extras(
        &mut params,
        &parts.query,
        &[
            "user",
            "username",
            "password",
            "private_key",
            "privatekey",
            "server",
            "server_port",
            "port",
        ],
    );
    Ok(ImportedConfig {
        name: display_name(&parts.fragment, "ssh", &parts.host, port),
        protocol: "ssh".to_string(),
        params: finish(params),
        host: parts.host.clone(),
        port,
    })
}

/// Ticket 27: standalone Noise link (`noise://key@host:port?…`).
/// Fork-sidecar `noise` rows (never stock): the key rides as `password`
/// with a `key` alias so JSON rows and links agree. Port required.
/// AWG-vs-Noise confusion is rejected in the engine (wrong-layer), not here.
fn parse_noise(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    let key = percent_decode(parts.userinfo.as_deref().unwrap_or(""), false);
    if key.trim().is_empty() {
        return Err(missing_credential("noise link is missing key"));
    }
    let port = require_port(parts.port_text.as_deref())?;
    let mut params = base_params("noise", &parts.host, port);
    insert_str(&mut params, "password", key.clone());
    insert_str(&mut params, "key", key);
    insert_extras(
        &mut params,
        &parts.query,
        &["password", "key", "server", "server_port", "port"],
    );
    Ok(ImportedConfig {
        name: display_name(&parts.fragment, "noise", &parts.host, port),
        protocol: "noise".to_string(),
        params: finish(params),
        host: parts.host.clone(),
        port,
    })
}

/// `throne://add/<base64-outbound-json>` (case-insensitive command, single
/// outbound, no confirm). Query/fragment bytes corrupt the payload and fail
/// as bad base64; unknown commands are skipped+counted.
/// Ticket 37: `throne://route/...` (routing presets) gets a named error, not
/// the generic unknown-command text — the pure batch path cannot store
/// presets; the single-line import path stores them (see `preset.rs`).
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
    if command.eq_ignore_ascii_case("route") {
        return Err(unknown_scheme(
            "routing preset link: paste it as a single line to import the preset (throne://route links carry no importable endpoint)",
        ));
    }
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
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| invalid_json("invalid throne JSON"))?;
    json_outbound(0, &value)
}

// ---------------------------------------------------------------------------
// JSON bodies: sing-box outbounds arrays, full configs, Xray arrays
// ---------------------------------------------------------------------------

/// Normalize an outbound `type`/`protocol` value to a library protocol name.
/// Ticket 24 (M8): link-scheme aliases (`hy2`, `wg`, `hysteria2+realm`,
/// `naive+https`) fold here too, so JSON/Clash agree with links.
/// Ticket 28: kept parallel to the store `canonical_protocol` on purpose
/// (see its note) — the alias sets and error taxonomy differ by path.
/// Ticket 31: typo errors name [`VALID_OUTBOUND_TYPES`].
pub const VALID_OUTBOUND_TYPES: &[&str] = &[
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

/// Ticket 31: accepted link schemes (lowercase, as matched in
/// [`parse_link`]). Typo scheme errors name this list; stored/JSON types
/// have their own [`VALID_OUTBOUND_TYPES`] (different namespace: `hy2` is a
/// scheme, never a stored type).
pub const VALID_LINK_SCHEMES: &[&str] = &[
    "vless",
    "vmess",
    "trojan",
    "ss",
    "shadowsocks",
    "throne",
    "xray",
    "hysteria2",
    "hy2",
    "hysteria2+realm",
    "hysteria",
    "tuic",
    "wireguard",
    "wg",
    "awg",
    "amneziawg",
    "amnezia-wg",
    "socks",
    "socks5",
    "snell",
    "shadowtls",
    "shadow-tls",
    "v2rayn",
    "anytls",
    "ssh",
    "noise",
    "http",
    "https",
    "naive+https",
];
fn normalize_outbound_type(raw: &str) -> Result<String, ImportError> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "vless" | "vmess" | "trojan" | "shadowsocks" | "hysteria2" | "hysteria" | "tuic"
        | "wireguard" | "amneziawg" | "socks" | "snell" | "anytls" | "ssh" | "noise" | "http"
        | "https" | "naive" => Ok(lower),
        "ss" => Ok("shadowsocks".to_string()),
        "socks5" => Ok("socks".to_string()),
        // Ticket 42: link-scheme aliases shared with the link grammar.
        "v2rayn" => Ok("vmess".to_string()),
        "shadow-tls" | "shadow_tls" => Ok("shadowtls".to_string()),
        // Ticket 07: AWG aliases fold to the canonical stored form.
        "awg" | "amnezia-wg" => Ok("amneziawg".to_string()),
        "hy2" | "hysteria2+realm" => Ok("hysteria2".to_string()),
        "wg" => Ok("wireguard".to_string()),
        "naive+https" | "naive_https" => Ok("naive".to_string()),
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
        // Ticket 24 (LOW scheme casing): echo the input's case, not the
        // lowered form, and cap the echo (no unbounded input in errors).
        // Ticket 31: the typo names the valid types.
        _ => Err(unknown_scheme(format!(
            "unknown outbound type '{}' (valid: {})",
            raw.trim().chars().take(64).collect::<String>(),
            VALID_OUTBOUND_TYPES.join(", ")
        ))),
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
/// Ticket 24 (M4): credential-needing types must carry credentials, exactly
/// like links do — only the Custom/extra/chain kinds pass through unchecked.
fn json_outbound(_index: usize, value: &serde_json::Value) -> Result<ImportedConfig, ImportError> {
    let obj = value
        .as_object()
        .ok_or_else(|| invalid_json("item is not a JSON object"))?;
    let type_raw = obj
        .get("type")
        .or_else(|| obj.get("protocol"))
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let protocol = normalize_outbound_type(type_raw)?;
    // Ticket 24 (M2): trim/bracket-strip so JSON servers agree with decoded
    // link hosts before identity.
    let host_raw = json_string(obj, &["server", "address"])
        .ok_or_else(|| ImportError::new(ImportErrorKind::BadHost, "item is missing \"server\""))?;
    let host_trimmed = host_raw.trim();
    let host = host_trimmed
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host_trimmed);
    if host.is_empty() || host.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(bad_host("item has a bad \"server\""));
    }
    let host = host.to_string();
    let port = json_port(obj)?;
    // Ticket 24 (M7): `email` is an account identifier, never a display name.
    let name = json_string(obj, &["tag", "remark", "remarks", "ps", "name"])
        .unwrap_or_else(|| format!("{protocol}-{host}:{port}"));
    let name = cap_name(name);
    require_json_credentials(&protocol, obj)?;
    let mut params = obj.clone();
    // Ticket 24 (M6): one canonical value under both keys — the engine reads
    // `protocol` (engine.rs), sing-box reads `type`; they must never disagree.
    params.insert(
        "type".to_string(),
        serde_json::Value::String(protocol.clone()),
    );
    params.insert(
        "protocol".to_string(),
        serde_json::Value::String(protocol.clone()),
    );
    Ok(ImportedConfig {
        name,
        protocol,
        params: finish(params),
        host,
        port,
    })
}

/// Ticket 24 (M4): per-type credential requirements for JSON rows, mirroring
/// the link parsers. Messages name the missing field only (never values).
fn require_json_credentials(protocol: &str, obj: &Params) -> Result<(), ImportError> {
    fn present(obj: &Params, keys: &[&str], trim: bool) -> bool {
        keys.iter().any(|key| match obj.get(*key) {
            Some(serde_json::Value::String(text)) => {
                if trim {
                    !text.trim().is_empty()
                } else {
                    !text.is_empty()
                }
            }
            Some(serde_json::Value::Number(_)) => true,
            _ => false,
        })
    }
    let ok = match protocol {
        "vless" | "vmess" | "xray" => present(obj, &["uuid"], true),
        "trojan" | "anytls" => present(obj, &["password"], false),
        // Ticket 27: ssh (user + password/private_key), noise (password/key).
        "ssh" => {
            present(obj, &["username", "user"], true)
                && present(obj, &["password", "private_key", "privatekey"], false)
        }
        "noise" => present(obj, &["password", "key"], false),
        "shadowsocks" => present(obj, &["method"], true) && present(obj, &["password"], false),
        "hysteria2" => present(obj, &["password", "auth"], true),
        "hysteria" => present(obj, &["auth"], true),
        "tuic" => present(obj, &["uuid"], true) && present(obj, &["password"], false),
        "wireguard" | "amneziawg" => {
            present(obj, &["private_key", "privatekey"], true)
                && present(obj, &["public_key", "publickey"], true)
        }
        "naive" => present(obj, &["username"], true) && present(obj, &["password"], false),
        // Ticket 42: snell needs its psk; shadowtls/socks/http serve
        // credential-less (shadowtls v1 omits the password, socks/http
        // allow anonymous).
        "snell" => present(obj, &["psk"], false),
        _ => true,
    };
    if !ok {
        return Err(missing_credential(format!(
            "item is missing credentials for \"{protocol}\""
        )));
    }
    // Ticket 42: reality without pbk rejects at import exactly like links
    // do (never parse-ok/start-fail). Native `tls.reality` objects are a
    // separate intake shape and are judged by the engine.
    let reality_flat = obj
        .get("security")
        .and_then(|value| value.as_str())
        .is_some_and(|security| security.eq_ignore_ascii_case("reality"));
    if reality_flat && !present(obj, &["pbk"], true) {
        return Err(missing_credential(
            "item is missing pbk (required for reality)",
        ));
    }
    Ok(())
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

/// Duplicate tags inside one body are deduped with `-2`, `-3`, … suffixes.
/// Ticket 24 (M9/M10): suffixes skip already-taken names, so a real `dup-2`
/// never collides with a generated one.
fn dedupe_names(configs: &mut [ImportedConfig]) {
    let mut seen: HashSet<String> = HashSet::new();
    for config in configs {
        let base = config.name.clone();
        if seen.insert(base.clone()) {
            continue;
        }
        let mut n = 2usize;
        loop {
            let candidate = format!("{base}-{n}");
            if seen.insert(candidate.clone()) {
                config.name = candidate;
                break;
            }
            n += 1;
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
            Err(error) => {
                // Ticket 42 (US-27): bare SIP008 server lists carry no
                // `type`/`protocol` (just server + method + password) —
                // synthesize Shadowsocks instead of failing the item.
                let bare_sip008 = item
                    .as_object()
                    .is_some_and(|obj| !obj.contains_key("type") && !obj.contains_key("protocol"));
                if bare_sip008 {
                    match sip008_server(item) {
                        Ok(config) => out.configs.push(config),
                        Err(_) => out.failures.push(ImportFailure::new(index, label, error)),
                    }
                } else {
                    out.failures.push(ImportFailure::new(index, label, error));
                }
            }
        }
    }
    dedupe_names(&mut out.configs);
    out
}

// ---------------------------------------------------------------------------
// WireGuard `.conf` files (one config per `[Peer]`)
// ---------------------------------------------------------------------------

/// AmneziaWG obfuscation keys a `.conf` may carry (official docs: junk-train
/// Jc/Jmin/Jmax, length prefixes S1-S4, header ranges H1-H4, CPS chains
/// I1-I5). Conf keys arrive lowercased by the parser below; values pass
/// through verbatim. Mirrors the engine version matrix (`engine.rs`
/// `AWG_V2_KEYS`); the fork's own `check` stays authoritative for shapes.
const CONF_AWG_V2_KEYS: &[&str] = &[
    "jc", "jmin", "jmax", "s1", "s2", "s3", "s4", "h1", "h2", "h3", "h4", "i1", "i2", "i3", "i4",
    "i5",
];

/// AmneziaWG 3.x-only `.conf` keys (mirrors `engine.rs` `AWG_V3_ONLY_KEYS`).
const CONF_AWG_V3_KEYS: &[&str] = &[
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

/// Classify one peer (+ its Interface snapshot) as AmneziaWG when either
/// section carries an AWG marker key (`awg_version`, a v2 junk/length/header
/// key, or a v3-only key). A bare `version` key alone never reclassifies a
/// plain WireGuard row.
fn conf_peer_is_awg(peer: &HashMap<String, String>, iface: &HashMap<String, String>) -> bool {
    peer.keys().chain(iface.keys()).any(|key| {
        key == "awg_version"
            || CONF_AWG_V2_KEYS.contains(&key.as_str())
            || CONF_AWG_V3_KEYS.contains(&key.as_str())
    })
}

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
        // Ticket 24 (LOW taxonomy): a conf with no peers is empty, not JSON.
        return BatchResult::fail(
            0,
            ImportErrorKind::Empty,
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
            // A conf carrying AmneziaWG obfuscation fields is an AmneziaWG
            // row (fork-core `wireguard` endpoint), not plain WireGuard.
            let protocol = if conf_peer_is_awg(peer, iface) {
                "amneziawg"
            } else {
                "wireguard"
            };
            let mut params = base_params(protocol, &host, port);
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
            // Ticket 42 (US-25): parity with URI intake — endpoint-meaningful
            // conf keys the mapper above does not rename (reserved bytes,
            // fwmark spellings, keepalive spellings) ride verbatim under
            // their lowercased name, peer wins with the Interface snapshot
            // as fallback. Anything else keeps the old drop behavior (a
            // conf-only scripting key like Table must never become a
            // start-fail the URI path would not produce either).
            const CONF_VERBATIM: &[&str] = &[
                "reserved",
                "fwmark",
                "fmark",
                "fm",
                "keepalive",
                "keep_alive",
            ];
            for verbatim in CONF_VERBATIM {
                if params.contains_key(*verbatim) {
                    continue;
                }
                let value = conf_value(peer, verbatim);
                let value = if value.trim().is_empty() {
                    conf_value(iface, verbatim)
                } else {
                    value
                };
                if !value.trim().is_empty() {
                    insert_str(&mut params, verbatim, value);
                }
            }
            if protocol == "amneziawg" {
                // AmneziaWG obfuscation params ride verbatim (peer wins, the
                // Interface snapshot is the fallback — stock AmneziaWG confs
                // keep Jc/Jmin/Jmax in [Interface]). An explicit version is
                // kept as-is; without one the fork matrix needs a declared
                // version, so v3-only keys default to "3.x" and the v2 junk
                // train to "2.0" (the engine's `awg_version_of` also accepts
                // a `version` fallback, which is preserved below when set).
                for key in CONF_AWG_V2_KEYS.iter().chain(CONF_AWG_V3_KEYS.iter()) {
                    let value = conf_value(peer, key);
                    let value = if value.trim().is_empty() {
                        conf_value(iface, key)
                    } else {
                        value
                    };
                    if !value.trim().is_empty() {
                        insert_str(&mut params, key, value);
                    }
                }
                for key in ["awg_version", "version"] {
                    let value = conf_value(peer, key);
                    let value = if value.trim().is_empty() {
                        conf_value(iface, key)
                    } else {
                        value
                    };
                    if !value.trim().is_empty() {
                        insert_str(&mut params, key, value);
                    }
                }
                if !params.contains_key("awg_version") && !params.contains_key("version") {
                    let v3 = CONF_AWG_V3_KEYS.iter().any(|key| params.contains_key(*key));
                    insert_str(
                        &mut params,
                        "awg_version",
                        if v3 { "3.x" } else { "2.0" }.to_string(),
                    );
                }
            }
            let name = format!("{stem}-p{}", peer_index + 1);
            Ok(ImportedConfig {
                name,
                protocol: protocol.to_string(),
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
    let base = file_name
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(file_name);
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
/// taxonomy. Ticket 24 (M19): a leading BOM is stripped here too, so pasted
/// lines parse identically whether or not the body sniffer saw them first.
pub fn parse_link(line: &str) -> Result<ImportedConfig, ImportError> {
    let text = strip_bom(line.trim());
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
        // Ticket 42: v2rayn links are legacy-vmess blobs under another
        // scheme (base64 alphabet has no `@`, so the legacy branch owns
        // them exactly like `vmess://`).
        "vmess" | "v2rayn" => {
            // New URI form carries `@` before any `?`/`#`; otherwise legacy.
            let authority_end = rest.find(['?', '#']).unwrap_or(rest.len());
            if rest[..authority_end].contains('@') {
                parse_vless_like("vmess", &split_uri(&scheme, rest)?, "uuid")
            } else {
                let payload = &rest[..authority_end];
                // Ticket 24 (M3): honor `#remark` on legacy blobs at minimum.
                let fragment = match rest[authority_end..].find('#') {
                    Some(i) => &rest[authority_end + i + 1..],
                    None => "",
                };
                parse_vmess_legacy(payload, fragment)
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
        "hysteria2" | "hy2" => parse_hy2(&scheme, &split_uri(&scheme, rest)?),
        // Ticket 42 (US-22): realm rendezvous has no link mapping (the
        // realm block conflicts with server addressing) — loud import
        // reject with guidance, never parse-ok/start-fail. Native `realm`
        // objects inside JSON intake still serve (engine passthrough).
        "hysteria2+realm" => Err(unknown_scheme(
            "hysteria2+realm rendezvous links carry no importable endpoint mapping (realm conflicts with server addressing in sing-box 1.14.2) — import rejected; serve it via a custom full-config row with a native \"realm\" block instead",
        )),
        "hysteria" => parse_hy1(&split_uri(&scheme, rest)?),
        "tuic" => parse_tuic(&split_uri(&scheme, rest)?),
        "wireguard" | "wg" => parse_wg_uri(&split_uri(&scheme, rest)?),
        "awg" | "amneziawg" | "amnezia-wg" => parse_awg_uri(&split_uri(&scheme, rest)?),
        "socks" | "socks5" => parse_socks_like("socks", &split_uri(&scheme, rest)?, None),
        "anytls" => parse_anytls(&split_uri(&scheme, rest)?),
        // Ticket 27: native SSH + standalone Noise (fork sidecar) links.
        "ssh" => parse_ssh(&split_uri(&scheme, rest)?),
        "noise" => parse_noise(&split_uri(&scheme, rest)?),
        "http" => parse_socks_like("http", &split_uri(&scheme, rest)?, Some(80)),
        "https" => parse_socks_like("https", &split_uri(&scheme, rest)?, Some(443)),
        "naive+https" => parse_naive(&split_uri(&scheme, rest)?),
        // Ticket 42 (US-24): native 1.14.x outbounds.
        "snell" => parse_snell(&split_uri(&scheme, rest)?),
        "shadowtls" | "shadow-tls" => parse_shadowtls(&split_uri(&scheme, rest)?),
        // Ticket 42 (US-24): no upstream 1.14.2 schema (or endpoint-only /
        // 1.15+-only) — loud import reject with per-protocol guidance.
        "mieru" | "juicity" | "openvpn" | "masque" | "trusttunnel" => {
            Err(unsupported_protocol_guidance(&scheme))
        }
        // Ticket 31: a typo scheme names the valid schemes.
        _ => Err(unknown_scheme(format!(
            "unknown scheme '{scheme}' (valid: {})",
            VALID_LINK_SCHEMES.join(", ")
        ))),
    }
}

/// Ticket 26: true when pasted/dropped text is a ManyProxy library export
/// (a JSON object carrying the `library` and `subscriptions` sections from
/// [`crate::store::export_payload_full`]) rather than import-shaped input.
/// Export payloads restore verbatim via [`Store::restore_library_json`] on
/// the UI confirm path; the import grammar keeps reading only `outbounds`,
/// so this predicate — not the grammar — owns the branch. A full sing-box
/// config has `outbounds` but never both export sections, so it stays on the
/// import path (duplicates, never a restore).
pub fn is_library_export(text: &str) -> bool {
    let trimmed = text.trim();
    if !trimmed.starts_with('{') {
        return false;
    }
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(serde_json::Value::Object(map)) => {
            matches!(map.get("library"), Some(serde_json::Value::Array(_)))
                && matches!(map.get("subscriptions"), Some(serde_json::Value::Array(_)))
        }
        _ => false,
    }
}

/// Parse pasted/clipboard text: JSON (outbounds array or full config,
/// BOM-prefixed or not), a WireGuard conf (any marker case), limited Clash
/// YAML (`proxies:` — same support as subscription bodies), or one share
/// link per line. Ticket 24 (M18): the sniff order matches
/// [`import_subscription_body`] (JSON → conf → Clash). Blank lines are
/// skipped silently; every other failure is reported with its item index.
pub fn import_text(body: &str) -> BatchResult {
    let trimmed = strip_bom(body.trim()).trim();
    if trimmed.is_empty() {
        return BatchResult::fail(0, ImportErrorKind::Empty, "nothing to import");
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        match serde_json::from_str::<serde_json::Value>(trimmed) {
            Ok(value) => return import_json_value(&value),
            Err(_) => {
                // A `[`-led conf still parses as conf; anything else that
                // looked like JSON but is not fails statically (SEC L2: no
                // serde error display, which can carry input excerpts).
                if looks_like_wg_conf(trimmed) {
                    return import_wg_conf(trimmed, "wireguard");
                }
                if has_clash_proxies(trimmed) {
                    return import_clash_proxies(trimmed);
                }
                return BatchResult::fail(0, ImportErrorKind::InvalidJson, "invalid JSON");
            }
        }
    }
    if looks_like_wg_conf(trimmed) {
        return import_wg_conf(trimmed, "wireguard");
    }
    if has_clash_proxies(trimmed) {
        return import_clash_proxies(trimmed);
    }
    let mut out = BatchResult::default();
    let mut index = 0;
    for raw_line in body.lines() {
        let line = strip_bom(raw_line.trim());
        if line.is_empty() {
            continue;
        }
        // Ticket 42 (US-27): a pasted bare v2rayn line (base64 of vmess
        // JSON, no scheme) parses as a legacy vmess blob.
        let owned;
        let effective = if !line.contains("://") {
            match decode_b64_utf8(line) {
                Ok(text) if looks_like_vmess_json(&text) => {
                    owned = format!("vmess://{line}");
                    owned.as_str()
                }
                _ => line,
            }
        } else {
            line
        };
        match parse_link(effective) {
            Ok(config) => out.configs.push(config),
            Err(error) => out
                .failures
                .push(ImportFailure::new(index, redact_link(line), error)),
        }
        index += 1;
    }
    // Ticket 24 (M9): link batches dedupe exactly like JSON/Clash batches.
    dedupe_names(&mut out.configs);
    out
}

/// Parse dropped/picked file content. `.conf` files go through the
/// WireGuard parser only when the content actually looks like a conf;
/// JSON / link-line / Clash bodies inside a `.conf` file are content-sniffed
/// down the [`import_text`] path instead (never forced through the wg parser
/// with a wrong-taxonomy error). Everything else follows [`import_text`].
pub fn import_file_content(file_name: &str, content: &str) -> BatchResult {
    // Ticket 24 (LOW): the `.conf` suffix matches after trimming, so
    // `"tun.conf "` still sniffs as conf.
    if file_name.trim().to_ascii_lowercase().ends_with(".conf") {
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
/// back to the default group). Ticket 26: dedupe uses
/// [`crate::store::widened_identity`] — the same key as subscription refresh
/// — so transport/security/flow-only variants commit as distinct rows instead
/// of over-merging, while identical re-imports never duplicate. The write is
/// one [`Store::add_batch`] transaction: validation runs fully before any
/// write, so one bad item rejects the whole batch and writes nothing (never a
/// half-committed import). Import never enables or starts configs: it only
/// creates disabled records.
pub fn commit_batch(
    store: &mut Store,
    batch: BatchResult,
    group: &str,
) -> Result<ImportOutcome, StoreError> {
    use crate::store::widened_identity;
    let group = group.trim();
    let group = if group.is_empty() {
        DEFAULT_GROUP.to_string()
    } else {
        group.to_string()
    };
    let mut seen: HashSet<String> = store
        .list()?
        .iter()
        .map(|config| widened_identity(&config.protocol, &config.params))
        .collect();
    let mut outcome = ImportOutcome {
        imported: 0,
        duplicates: 0,
        failures: batch.failures,
    };
    let mut inputs: Vec<NewConfig> = Vec::with_capacity(batch.configs.len());
    for config in batch.configs {
        if !seen.insert(widened_identity(&config.protocol, &config.params)) {
            outcome.duplicates += 1;
            continue;
        }
        inputs.push(NewConfig {
            name: config.name,
            protocol: config.protocol,
            params: config.params,
            group: group.clone(),
            bind_ip: String::new(),
            port: String::new(),
        });
    }
    if inputs.is_empty() {
        return Ok(outcome);
    }
    let added = store.add_batch(inputs)?;
    outcome.imported = added.len();
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
/// port + credential + method + sni/path, NOT the remark. Server is trimmed,
/// bracket-stripped, trailing-dot-trimmed and lowercased (hostnames are
/// case-insensitive, surrounding whitespace/brackets/a root dot are not
/// identity); credential values stay verbatim. Ticket 24 (LOW): `flow` and
/// `security` are deliberately excluded from identity (same server with a
/// flow-only change is the same row); ports are numeric (no trim needed).
/// Ticket 16: [`commit_batch`] uses the widened key
/// ([`crate::store::widened_identity`], same base plus transport/security
/// extras) so paste/import dedupes exactly like subscription refresh without
/// over-merging distinct nodes.
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
                .map(|server| {
                    let trimmed = server.trim();
                    let unbracketed = trimmed
                        .strip_prefix('[')
                        .and_then(|h| h.strip_suffix(']'))
                        .unwrap_or(trimmed);
                    unbracketed.trim_end_matches('.').to_ascii_lowercase()
                })
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
    // Ticket 24 (CRITICAL byte panic): `url[..7]`/`url[..8]` split UTF-8 when
    // the payload holds multibyte chars near the boundary. `get` + ASCII
    // comparison keeps every slice on a char boundary (a matched ASCII
    // prefix is ASCII bytes, so the split after it is safe).
    // NOTE (ticket 25): store.rs `validated_subscription_url` has the same
    // `trimmed[..7]`/`[..8]` class — import-side half is fixed here; the
    // store-side half is recorded as a Ruling + ledger note for ticket 25.
    let after_scheme = if url
        .get(..7)
        .is_some_and(|p| p.eq_ignore_ascii_case("http://"))
    {
        &url[7..]
    } else if url
        .get(..8)
        .is_some_and(|p| p.eq_ignore_ascii_case("https://"))
    {
        &url[8..]
    } else {
        return Err(bad_host("subscription url must use http(s)"));
    };
    let host = after_scheme.split(['/', '?']).next().unwrap_or("");
    if host.trim().is_empty() || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(bad_host("subscription url must use http(s)"));
    }
    // Ticket 27: same internal-host block as the store intake (SSRF guard;
    // see `crate::store::subscription_fetch_policy`). Plain http stays
    // allowed (Warn at fetch, not a parse reject).
    if matches!(
        crate::store::subscription_fetch_policy(url),
        crate::store::SubscriptionFetchPolicy::Block { .. }
    ) {
        return Err(bad_host("subscription url must not target internal hosts"));
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

/// Ticket 42 (US-27): true when `text` is a legacy-vmess JSON object
/// (v2rayn subscription lines are per-line base64 of exactly this shape,
/// with no scheme). Both `add` and `id` are required — the legacy parser
/// rejects anything less, so this predicate never claims non-vmess JSON.
fn looks_like_vmess_json(text: &str) -> bool {
    match serde_json::from_str::<serde_json::Value>(text.trim()) {
        Ok(serde_json::Value::Object(map)) => {
            let has_add = map
                .get("add")
                .and_then(|value| value.as_str())
                .is_some_and(|add| !add.trim().is_empty());
            let has_id = map
                .get("id")
                .and_then(|value| value.as_str())
                .is_some_and(|id| !id.trim().is_empty());
            has_add && has_id
        }
        _ => false,
    }
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
            // Ticket 24 (SEC L2): static message, never the serde display.
            Err(_) => BatchResult::fail(0, ImportErrorKind::InvalidJson, "invalid JSON"),
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
        // keep the rest verbatim, then parse the rebuilt body. Ticket 42:
        // v2rayn lines decode to schemeless vmess JSON — they re-enter as
        // `vmess://` legacy blobs instead of failing as non-links.
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
                } else if looks_like_vmess_json(&text) {
                    rebuilt.push(format!("vmess://{line}"));
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
    // `data:…;base64,…` single payloads (V2Fly DataURLSingle). Ticket 24
    // (LOW data: case): the marker matches case-insensitively.
    let lowered = single.to_ascii_lowercase();
    if let Some(pos) = lowered.find(";base64,") {
        let payload = &single[pos + 8..];
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
        // Ticket 42 (US-27): a blob that decodes to schemeless vmess JSON
        // (one v2rayn line as the whole body) re-enters as a legacy blob.
        Ok(text) if looks_like_vmess_json(&text) => import_text(&format!("vmess://{single}")),
        // Ticket 24 (M17): a blob that decodes to JSON (no `://`) is tried
        // as JSON before falling back to per-line link parsing.
        Ok(text) => match serde_json::from_str::<serde_json::Value>(text.trim()) {
            Ok(value) => import_json_value(&value),
            Err(_) => import_text(body),
        },
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
/// start a line with `proxies:`. Ticket 24 (LOW strictness): `proxies:foo`
/// (no space) is not a header — only a bare header or one followed by
/// whitespace/comment qualifies.
fn is_proxies_header(line: &str) -> bool {
    line == "proxies:"
        || line
            .strip_prefix("proxies:")
            .is_some_and(|rest| rest.starts_with([' ', '\t']) || rest.starts_with('#'))
}

fn has_clash_proxies(trimmed: &str) -> bool {
    trimmed.lines().any(|raw_line| {
        let indent = raw_line.len() - raw_line.trim_start().len();
        indent == 0 && is_proxies_header(raw_line.trim())
    })
}

/// Limited Clash `proxies:` parser (block style only): each `- ` entry needs
/// `name` (default `{type}-{server}:{port}`), `type`, `server`, `port`.
/// Entries are converted to sing-box-shaped params; unknown types and missing
/// fields are per-item failures. Groups/rules are ignored.
fn import_clash_proxies(content: &str) -> BatchResult {
    fn clash_type(raw: &str) -> Result<String, ImportError> {
        let lower = raw.trim().to_ascii_lowercase();
        // Ticket 42: ShadowsocksR has no sing-box mapping — per-item skip
        // with guidance, never a bare unknown-type text.
        if ["ssr", "shadowsocksr", "ssr-shadowsocks"].contains(&lower.as_str()) {
            return Err(unknown_scheme(
                "clash type 'ssr' has no sing-box mapping (ShadowsocksR is not served by sing-box 1.14.2)",
            ));
        }
        match lower.as_str() {
            "vless" | "vmess" | "trojan" | "shadowsocks" | "hysteria2" | "hysteria" | "tuic"
            | "wireguard" | "amneziawg" | "socks" | "snell" | "shadowtls" | "anytls" | "ssh"
            | "noise" | "http" | "https" | "naive" => Ok(lower),
            "ss" => Ok("shadowsocks".to_string()),
            "socks5" => Ok("socks".to_string()),
            // Ticket 07: AWG aliases fold to the canonical stored form.
            "awg" | "amnezia-wg" => Ok("amneziawg".to_string()),
            // Ticket 24 (M8): link-scheme aliases fold here too.
            "hy2" | "hysteria2+realm" => Ok("hysteria2".to_string()),
            "wg" => Ok("wireguard".to_string()),
            "naive+https" | "naive_https" => Ok("naive".to_string()),
            // Ticket 42: shadowTLS hyphen spelling.
            "shadow-tls" | "shadow_tls" => Ok("shadowtls".to_string()),
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
            // Ticket 24 (LOW scheme casing): echo the input's case, capped.
            _ => Err(unknown_scheme(format!(
                "unknown clash type '{}'",
                raw.trim().chars().take(64).collect::<String>()
            ))),
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
        .position(|raw_line| is_proxies_header(raw_line.trim()))
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
                // Ticket 42 (US-27): `- {"name": …, "type": …}` inline-JSON
                // entries land (parsed as JSON objects); anything else in
                // flow braces stays a named per-item skip.
                if after.starts_with('{') {
                    match serde_json::from_str::<serde_json::Value>(after) {
                        Ok(serde_json::Value::Object(map)) => {
                            for (key, value) in &map {
                                let value = match value {
                                    serde_json::Value::String(text) => text.clone(),
                                    serde_json::Value::Number(_) | serde_json::Value::Bool(_) => {
                                        value.to_string()
                                    }
                                    _ => continue,
                                };
                                fields.push((key.to_ascii_lowercase(), clash_value(&value)));
                            }
                            let index = entries.len();
                            entries.push((fields, index));
                        }
                        _ => {
                            let index = entries.len();
                            entries
                                .push((vec![("__flow__".to_string(), after.to_string())], index));
                        }
                    }
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
            // Ticket 42: Clash/Meta spellings fold to sing-box names so the
            // rows serve instead of dying in `check` (nested option blocks
            // are ignored upstream, so anything they alone could provide —
            // e.g. reality keys — fails loudly below via the credential
            // check rather than at start).
            let mut security_tls = false;
            for (key, value) in fields {
                if key == "name" || key == "type" || key == "server" || key == "port" {
                    continue;
                }
                // Ticket 24 (M5): Clash `cipher` is sing-box `method`, so
                // Clash SS rows dedupe with link SS rows.
                let key = if key == "cipher" { "method" } else { key };
                let key = match key {
                    "servername" => "sni",
                    "client-fingerprint" => "fp",
                    "alterid" => "alter_id",
                    "private-key" => "private_key",
                    "public-key" => "public_key",
                    "allowed-ips" => "allowed_ips",
                    "skip-cert-verify" => "allowInsecure",
                    _ => key,
                };
                if key == "tls" {
                    if matches!(
                        value.trim().to_ascii_lowercase().as_str(),
                        "1" | "true" | "yes" | "on"
                    ) {
                        security_tls = true;
                    }
                    continue;
                }
                insert_str(&mut params, key, value.clone());
            }
            if security_tls && !params.contains_key("security") {
                insert_str(&mut params, "security", "tls".to_string());
            }
            // Ticket 42: credential-needing types fail per-item here (never
            // parse-ok/start-fail), mirroring the JSON path.
            require_json_credentials(&protocol, &params)?;
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

/// Redact credentials and query values, then truncate, for notifications and
/// logs (boundary: secrets never reach logs/clipboard). Query keys stay
/// (they diagnose failures); values go (some schemes carry key material like
/// reality `pbk` in the query). Fragments (remarks) stay: remarks are display
/// names by design. Ticket 24: masks to the LAST `@` (H7), masks opaque
/// no-`@` payloads of vmess/ss/throne links instead of echoing them (H8),
/// and scrubs `?bare-key` pairs instead of keeping them verbatim (H9).
pub fn redact_link(line: &str) -> String {
    let mut text = line.trim().to_string();
    if let Some(scheme_end) = text.find("://") {
        let after = scheme_end + 3;
        let auth_end = text[after..]
            .find(['?', '#'])
            .map(|i| after + i)
            .unwrap_or(text.len());
        let at = text[after..auth_end].rfind('@');
        match at {
            // H7: mask to the LAST `@` in the authority region (query and
            // fragment are excluded from the search).
            Some(i) => {
                text.replace_range(after..after + i, "…");
            }
            // H8: opaque payloads (vmess/ss-blob/throne base64) carry the
            // credential with no `@` — mask the payload, never echo it.
            // Plain host-only links (no userinfo, other schemes) keep the
            // host for diagnostics.
            None => {
                let scheme = text[..scheme_end].to_ascii_lowercase();
                if ["vmess", "ss", "shadowsocks", "throne"].contains(&scheme.as_str()) {
                    text.replace_range(after..auth_end, "…");
                }
            }
        }
    }
    if let Some(query_start) = text.find('?') {
        let frag_start = text.find('#').unwrap_or(text.len());
        if query_start < frag_start {
            let mut pairs = Vec::new();
            for pair in text[query_start + 1..frag_start].split('&') {
                match pair.find('=') {
                    Some(eq) => pairs.push(format!("{}=…", &pair[..eq])),
                    // H9: a `?bare-key` pair is scrubbed, not kept verbatim
                    // (the token sits in the key position).
                    None if pair.is_empty() => pairs.push(String::new()),
                    None => pairs.push("…".to_string()),
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
    // Ticket 28: length cap is health::truncate (same 96-char rule as the
    // subscription-URL scrubber; one truncator for every display path).
    crate::health::truncate(&text, 96)
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

    /// Ticket 26: export payloads route to restore (both sections present);
    /// sing-box configs, links, and partial shapes stay on the import path.
    #[test]
    fn is_library_export_routes_only_full_payloads() {
        use crate::store::export_payload_full;
        let full = export_payload_full(&[], &[], &[]);
        assert!(is_library_export(&full), "real export restores");
        assert!(
            !is_library_export("{\"outbounds\": [], \"library\": []}"),
            "missing subscriptions stays import"
        );
        assert!(
            !is_library_export("{\"subscriptions\": []}"),
            "missing library stays import"
        );
        assert!(
            !is_library_export("{\"outbounds\": [{\"type\": \"vless\"}]}"),
            "sing-box stays import"
        );
        assert!(
            !is_library_export(&format!("vless://{UUID}@a.example.com:443#one")),
            "links stay import"
        );
        assert!(!is_library_export(""), "empty stays import");
        assert!(!is_library_export("[1, 2]"), "arrays stay import");
        assert!(
            !is_library_export("{\"library\": {}, \"subscriptions\": []}"),
            "non-array library stays import"
        );
    }

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

    /// Ticket 42 (US-22): `hysteria2+realm://` rejects at import with
    /// guidance (rendezvous has no link mapping) — never parse-ok/start-fail.
    /// (Replaces the parse-ok expectation: the variant marker used to ride
    /// into params and die at generate time.)
    #[test]
    fn hysteria2_realm_variant() {
        let error = parse_link("hysteria2+realm://letmein@example.com:8443#r")
            .expect_err("realm variant rejects at import");
        assert_eq!(error.kind, ImportErrorKind::UnknownScheme);
        assert!(error.message.contains("realm"), "names realm: {error}");
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

    // -- live review (2026-09-26): user-supplied AmneziaWG conf -------------
    // These exact bytes are the user's live config (already pasted in chat);
    // they are the single regression test allowed to carry real values.
    // A conf carrying AmneziaWG junk-train fields (Jc/Jmin/Jmax, …) is an
    // AmneziaWG row — not plain WireGuard — with the obfuscation params
    // preserved verbatim and a defaulted `awg_version` so the fork-core
    // version matrix (engine.rs) accepts it.

    #[test]
    fn live_amneziawg_conf_imports_single_awg_row_with_junk_params() {
        let conf = "[Interface]\nPrivateKey = +KjXeqsx9goY9hfPNQ0fS31xUtjYrVJ2KVuJnbHkZEM=\nAddress = 100.64.134.1/32\nDNS = 198.18.0.1,198.18.0.2\nJc = 5\nJmin = 30\nJmax = 1000\n\n[Peer]\nPublicKey = KgTUh3KLijVluDvNpzDCJJfrJ7EyLzYLmdHCksG4sRg=\nAllowedIPs = 0.0.0.0/0\nEndpoint = 91.148.248.176:51820\n";
        let out = import_file_content("live-awg.conf", conf);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1, "single peer yields one row");
        let config = &out.configs[0];
        assert_eq!(config.protocol, "amneziawg");
        assert_eq!(config.host, "91.148.248.176");
        assert_eq!(config.port, 51820);
        let params = params_of(config);
        assert_eq!(params["jc"], "5");
        assert_eq!(params["jmin"], "30");
        assert_eq!(params["jmax"], "1000");
        assert_eq!(params["awg_version"], "2.0");
        for key in ["private_key", "public_key"] {
            assert!(
                params
                    .get(key)
                    .and_then(|value| value.as_str())
                    .is_some_and(|value| !value.trim().is_empty()),
                "key {key} flows into params"
            );
        }
        // Paste path agrees with the drop path (ticket 15 parity).
        let pasted = import_text(conf);
        assert!(
            pasted.failures.is_empty(),
            "failures: {:?}",
            pasted.failures
        );
        assert_eq!(pasted.configs.len(), 1);
        assert_eq!(pasted.configs[0].protocol, "amneziawg");
        assert_eq!(params_of(&pasted.configs[0])["jmax"], "1000");
        // Engine seam (public API only): the live row routes to the fork
        // core and generates a fork `wireguard` endpoint with its junk
        // params verbatim. Loopback bind + synthetic local port: no live
        // network, no binary touched — the only missing runtime piece is
        // the `sing-box-awg` binary itself.
        let row = crate::store::ProxyConfig {
            id: "live-awg-1".to_string(),
            name: config.name.clone(),
            protocol: config.protocol.clone(),
            params: config.params.clone(),
            group: "live-review".to_string(),
            enabled: true,
            bind_ip: "127.0.0.1".to_string(),
            port: 10801,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        };
        assert!(
            crate::engine::needs_fork(&row),
            "AWG rows route to the fork core"
        );
        let generated = crate::engine::generate(std::slice::from_ref(&row))
            .expect("live AWG row passes the version matrix");
        assert!(generated.requires_fork);
        let endpoints = generated.value()["endpoints"]
            .as_array()
            .expect("endpoints array")
            .clone();
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0]["type"], serde_json::json!("wireguard"));
        // Live-proven: the fork types jc/jmin/jmax as uint32 — strings are
        // rejected at check time, so generation coerces them.
        assert_eq!(endpoints[0]["jc"], serde_json::json!(5));
        assert_eq!(endpoints[0]["jmin"], serde_json::json!(30));
        assert_eq!(endpoints[0]["jmax"], serde_json::json!(1000));
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
            {"type": "wireguard", "tag": "wg", "server": "c.example.com", "server_port": 51820,
             "private_key": "p", "public_key": "q"}
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

    /// Ticket 31: typo scheme / typo JSON type errors name the valid lists
    /// (never just "unknown").
    #[test]
    fn failure_typos_name_valid_protocols() {
        let error = parse_link("vles://example.com:443").expect_err("typo scheme fails");
        assert_eq!(error.kind, ImportErrorKind::UnknownScheme);
        for valid in ["vless", "vmess", "trojan", "wireguard"] {
            assert!(error.message.contains(valid), "names {valid}, got {error}");
        }
        let error = super::normalize_outbound_type("vles").expect_err("typo type fails");
        assert_eq!(error.kind, ImportErrorKind::UnknownScheme);
        for valid in ["vless", "shadowsocks", "hysteria2", "xray"] {
            assert!(error.message.contains(valid), "names {valid}, got {error}");
        }
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

    /// Ticket 37: `throne://route` links fail with the named preset-intake
    /// error, not the generic unknown-command text (the pure batch path
    /// cannot store presets — the single-line import path does, see the
    /// ticket-37 store tests).
    #[test]
    fn failure_throne_route_names_preset_intake() {
        for link in ["throne://route/abc", "throne://ROUTE/abc"] {
            let error = parse_link(link).expect_err("route command fails");
            assert_eq!(error.kind, ImportErrorKind::UnknownScheme);
            assert!(
                error.message.contains("single line"),
                "named preset-intake error, got {error}"
            );
        }
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

    /// Ticket 26: `commit_batch` dedupes on the widened identity (same key as
    /// subscription refresh): case/whitespace-only variants still dedupe,
    /// but a flow-only change is a DIFFERENT node and commits a second row
    /// instead of over-merging.
    #[test]
    fn commit_dedupes_on_widened_identity() {
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

        // Flow-only change rides INSIDE the widened key: a distinct node.
        let flowed = import_text(&format!(
            "vless://{UUID}@a.example.com:443?flow=xtls-rprx-vision#three"
        ));
        let outcome = commit_batch(&mut store, flowed, "g").expect("recommit works");
        assert_eq!(outcome.imported, 1, "flow variant is a distinct node");
        assert_eq!(outcome.duplicates, 0);
        assert_eq!(store.list().expect("list works").len(), 2);

        // Re-importing the exact flowed row dedupes again.
        let flowed_again = import_text(&format!(
            "vless://{UUID}@a.example.com:443?flow=xtls-rprx-vision#three"
        ));
        let outcome = commit_batch(&mut store, flowed_again, "g").expect("recommit works");
        assert_eq!(outcome.imported, 0, "exact re-import adds nothing");
        assert_eq!(outcome.duplicates, 1);
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

    // -- ticket 24: parser criticals, credential integrity, redaction -------

    #[test]
    fn ticket24_non_ascii_addsub_no_panic() {
        // `https:` + U+00E9: byte slicing at 7/8 would split the multibyte
        // char and panic; must be a typed error instead.
        let link = format!("throne://addsub/{}", b64_standard("https:éxample.com/sub"));
        let error = parse_addsub_link(&link).expect_err("non-ascii url fails typed");
        assert_eq!(error.kind, ImportErrorKind::BadHost);
    }

    #[test]
    fn ticket24_trojan_query_password_never_wins() {
        let config =
            parse_link("trojan://real@example.com:443?password=fake#t").expect("trojan parses");
        assert_eq!(params_of(&config)["password"], "real");
    }

    #[test]
    fn ticket24_core_fields_win_over_query_extras() {
        let vless = parse_link(&format!(
            "vless://{UUID}@a.example.com:443?uuid=fake&server=evil.example&server_port=1&type=ws#x"
        ))
        .expect("vless parses");
        let params = params_of(&vless);
        assert_eq!(params["uuid"], UUID);
        assert_eq!(params["server"], "a.example.com");
        assert_eq!(vless.host, "a.example.com");
        assert_eq!(vless.port, 443);
        let tuic = parse_link(&format!(
            "tuic://{UUID}:realpass@a.example.com:4433?password=fake&uuid=fake#x"
        ))
        .expect("tuic parses");
        assert_eq!(params_of(&tuic)["password"], "realpass");
        let hy2 = parse_link("hy2://real@example.com:8443?password=fake#x").expect("hy2 parses");
        assert_eq!(params_of(&hy2)["password"], "real");
        let wg = parse_link("wireguard://priv@example.com:51820?publickey=pub&private_key=fake#x")
            .expect("wg parses");
        assert_eq!(params_of(&wg)["private_key"], "priv");
        let socks = parse_link("socks://u:p@example.com:1080?username=fake&password=fake#x")
            .expect("socks parses");
        assert_eq!(params_of(&socks)["username"], "u");
    }

    #[test]
    fn ticket24_redact_masks_to_last_at() {
        let redacted = redact_link("vless://uuid@inner@host.example.com:443#x");
        assert!(!redacted.contains("uuid"), "got {redacted}");
        assert!(!redacted.contains("inner"), "got {redacted}");
        assert!(redacted.contains("host.example.com"), "got {redacted}");
    }

    #[test]
    fn ticket24_redact_masks_opaque_payloads() {
        let vmess = format!("vmess://{}", b64("{\"add\":\"h.example.com\"}"));
        let redacted = redact_link(&vmess);
        assert!(
            !redacted.contains(&b64("{\"add\":\"h.example.com\"}")),
            "got {redacted}"
        );
        let ss = format!("ss://{}", b64("aes-256-gcm:hunter2@example.com:8388"));
        assert!(
            !redact_link(&ss).contains("hunter2"),
            "got {}",
            redact_link(&ss)
        );
        let outbound = serde_json::json!({
            "type": "vless", "server": "t.example.com", "server_port": 443, "uuid": UUID
        });
        let throne = format!("throne://add/{}", b64(&outbound.to_string()));
        let redacted = redact_link(&throne);
        assert!(
            !redacted.contains("t.example.com"),
            "payload masked, got {redacted}"
        );
        // Non-opaque scheme without userinfo keeps its host for diagnostics.
        let plain = redact_link("https://example.com#hs");
        assert!(plain.contains("example.com"), "got {plain}");
    }

    #[test]
    fn ticket24_redact_scrubs_bare_keys() {
        let redacted = redact_link("vless://u@h.example.com:443?bare-token-here&sni=#x");
        assert!(!redacted.contains("bare-token-here"), "got {redacted}");
        assert!(redacted.contains("sni="), "got {redacted}");
    }

    #[test]
    fn ticket24_empty_bracket_host_rejected() {
        let json = serde_json::json!({"add": "[]", "port": "10086", "id": UUID});
        let error = parse_link(&format!("vmess://{}", b64(&json.to_string())))
            .expect_err("empty bracket host fails");
        assert_eq!(error.kind, ImportErrorKind::BadHost);
        let json = serde_json::json!({"add": "[::1", "port": "10086", "id": UUID});
        let error = parse_link(&format!("vmess://{}", b64(&json.to_string())))
            .expect_err("unclosed bracket fails");
        assert_eq!(error.kind, ImportErrorKind::BadHost);
    }

    #[test]
    fn ticket24_vless_uri_path_preserved() {
        let config = parse_link(&format!("vless://{UUID}@a.example.com:443/extra-path#x"))
            .expect("vless with path parses");
        assert_eq!(params_of(&config)["path"], "extra-path");
    }

    #[test]
    fn ticket24_host_percent_decoded() {
        let config = parse_link(&format!("vless://{UUID}@exam%70le.com:443#x"))
            .expect("encoded host parses");
        assert_eq!(config.host, "example.com");
    }

    #[test]
    fn ticket24_vmess_legacy_honors_fragment() {
        let json = serde_json::json!({"add": "b.example.com", "port": "10086", "id": UUID});
        let link = format!("vmess://{}#legacy-remark", b64(&json.to_string()));
        let config = parse_link(&link).expect("legacy vmess with remark parses");
        assert_eq!(config.name, "legacy-remark");
    }

    #[test]
    fn ticket24_json_missing_credentials_rejected() {
        let body = serde_json::json!([
            {"type": "vless", "tag": "no-uuid", "server": "a.example.com", "server_port": 443},
            {"type": "wireguard", "tag": "no-keys", "server": "c.example.com", "server_port": 51820},
            {"type": "trojan", "tag": "ok", "server": "t.example.com", "server_port": 443, "password": "pw"}
        ]);
        let out = import_text(&body.to_string());
        assert_eq!(out.configs.len(), 1);
        assert_eq!(out.configs[0].name, "ok");
        assert_eq!(out.failures.len(), 2);
        for failure in &out.failures {
            assert_eq!(failure.code, ImportErrorKind::MissingCredential);
        }
        // Custom kinds keep full-config passthrough without credential checks.
        let body = serde_json::json!([
            {"type": "custom-outbound", "tag": "c", "server": "a.example.com", "server_port": 443}
        ]);
        let out = import_text(&body.to_string());
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1);
    }

    #[test]
    fn ticket24_clash_cipher_maps_to_method() {
        let body = "proxies:\n  - name: clash-ss\n    type: ss\n    server: s.example.com\n    port: 8388\n    cipher: aes-256-gcm\n    password: hunter2\n";
        let out = import_subscription_body(body);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        let params = params_of(&out.configs[0]);
        assert_eq!(params["method"], "aes-256-gcm");
        assert!(
            params.get("cipher").is_none(),
            "cipher folded, got {params}"
        );
    }

    #[test]
    fn ticket24_json_type_protocol_single_canonical() {
        let body = serde_json::json!([
            {"protocol": "SS", "tag": "k", "server": "a.example.com", "server_port": 8388,
             "method": "aes-256-gcm", "password": "pw"}
        ]);
        let out = import_text(&body.to_string());
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        let params = params_of(&out.configs[0]);
        assert_eq!(params["type"], "shadowsocks");
        assert_eq!(params["protocol"], "shadowsocks");
    }

    #[test]
    fn ticket24_json_email_is_not_a_name() {
        let body = serde_json::json!([
            {"type": "trojan", "email": "someone@example.com",
             "server": "a.example.com", "server_port": 443, "password": "pw"}
        ]);
        let out = import_text(&body.to_string());
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs[0].name, "trojan-a.example.com:443");
    }

    #[test]
    fn ticket24_json_and_clash_aliases_fold() {
        for (raw, canonical) in [
            ("hy2", "hysteria2"),
            ("hysteria2+realm", "hysteria2"),
            ("wg", "wireguard"),
            ("naive+https", "naive"),
        ] {
            let body = serde_json::json!([{
                "type": raw, "tag": "k",
                "server": "a.example.com", "server_port": 443,
                "password": "p", "username": "u", "uuid": UUID,
                "private_key": "p", "public_key": "q", "auth": "a"
            }]);
            let out = import_text(&body.to_string());
            assert!(out.failures.is_empty(), "type {raw}: {:?}", out.failures);
            assert_eq!(out.configs[0].protocol, canonical, "type {raw}");
        }
        let body = "proxies:\n  - name: w\n    type: wg\n    server: a.example.com\n    port: 51820\n    private_key: p\n    public_key: q\n  - name: h\n    type: hy2\n    server: b.example.com\n    port: 8443\n    password: p\n";
        let out = import_subscription_body(body);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs[0].protocol, "wireguard");
        assert_eq!(out.configs[1].protocol, "hysteria2");
    }

    #[test]
    fn ticket24_link_batch_dedupes_names_without_collision() {
        let body = [
            &format!("vless://{UUID}@a.example.com:443#same"),
            "trojan://pw@b.example.com:443#same",
            "anytls://pw@c.example.com:8443#same",
            "anytls://pw@d.example.com:8443#same-2",
            "anytls://pw@e.example.com:8443#same",
        ]
        .join("\n");
        let out = import_text(&body);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        let names: Vec<&str> = out.configs.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["same", "same-2", "same-3", "same-2-2", "same-4"]);
    }

    #[test]
    fn ticket24_reality_pbk_case_hint() {
        let link =
            format!("vless://{UUID}@example.com:443?security=reality&sni=example.com&PBK=abcdef#r");
        let error = parse_link(&link).expect_err("uppercase PBK still fails");
        assert_eq!(error.kind, ImportErrorKind::MissingCredential);
        assert!(error.message.contains("case-sensitive"), "got {error}");
    }

    #[test]
    fn ticket24_empty_sni_falls_back_to_host() {
        let config = parse_link(&format!("vless://{UUID}@a.example.com:443?sni=#x"))
            .expect("empty sni parses");
        assert_eq!(params_of(&config)["sni"], "a.example.com");
        let hy2 = parse_link("hy2://pw@b.example.com:8443?sni=#x").expect("hy2 parses");
        assert_eq!(params_of(&hy2)["sni"], "b.example.com");
    }

    #[test]
    fn ticket24_unknown_network_preserved() {
        let config = parse_link(&format!("vless://{UUID}@a.example.com:443?type=foobar#x"))
            .expect("unknown network parses");
        assert_eq!(params_of(&config)["network"], "foobar");
        let defaulted =
            parse_link(&format!("vless://{UUID}@a.example.com:443#x")).expect("no network parses");
        assert_eq!(params_of(&defaulted)["network"], "tcp");
    }

    #[test]
    fn ticket24_ss_legacy_slash_parity() {
        let config = parse_link(&format!(
            "ss://{}/",
            b64("aes-256-gcm:hunter2@example.com:8388/")
        ))
        .expect("legacy ss with inner slash parses");
        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, 8388);
    }

    #[test]
    fn ticket24_ss_userinfo_percent_decoded() {
        let config = parse_link(&format!(
            "ss://{}@example.com:8388#x",
            b64("aes-256-gcm:hunter%202")
        ))
        .expect("encoded ss password parses");
        assert_eq!(params_of(&config)["password"], "hunter 2");
    }

    #[test]
    fn ticket24_ss_whitespace_password_rejected() {
        let error = parse_link("ss://aes-256-gcm:%20%20@example.com:8388#x")
            .expect_err("blank ss password fails");
        assert_eq!(error.kind, ImportErrorKind::MissingCredential);
    }

    #[test]
    fn ticket24_socks_empty_user_or_pass_rejected() {
        for link in [
            "socks://:pass@example.com:1080#x",
            "socks://user:@example.com:1080#x",
        ] {
            let error = parse_link(link).expect_err(&format!("{link} fails"));
            assert_eq!(
                error.kind,
                ImportErrorKind::MissingCredential,
                "link {link}"
            );
        }
        parse_link("socks://user@example.com:1080#x").expect("bare username ok");
        parse_link("socks://example.com:1080#x").expect("no userinfo ok");
    }

    #[test]
    fn ticket24_vmess_tls_bool_and_trimmed_host() {
        let json = serde_json::json!({
            "add": "  example.com  ", "port": "10086", "id": UUID, "tls": true
        });
        let config =
            parse_link(&format!("vmess://{}", b64(&json.to_string()))).expect("bool tls parses");
        assert_eq!(config.host, "example.com");
        assert_eq!(params_of(&config)["tls"], true);
    }

    #[test]
    fn ticket24_single_blob_json_decoded() {
        let array = serde_json::json!([
            {"type": "trojan", "tag": "blob", "server": "t.example.com", "server_port": 443, "password": "pw"}
        ]);
        let out = import_subscription_body(&b64(&array.to_string()));
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1);
        assert_eq!(out.configs[0].name, "blob");
    }

    #[test]
    fn ticket24_bom_per_line_stripped() {
        let body = format!(
            "vless://{UUID}@a.example.com:443#one\n\u{FEFF}trojan://pw@b.example.com:443#two"
        );
        let out = import_text(&body);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 2);
    }

    #[test]
    fn ticket24_port_strictness() {
        for bad in ["0443", "+443", "-1"] {
            let link = format!("vless://{UUID}@example.com:{bad}");
            let error = parse_link(&link).expect_err(&format!("port {bad:?} fails"));
            assert_eq!(error.kind, ImportErrorKind::BadPort, "port {bad:?}");
        }
    }

    #[test]
    fn ticket24_conf_trailing_space_name() {
        let conf = "[Interface]\nPrivateKey = cHJpdmF0ZWtleQ\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\n";
        let out = import_file_content("tun.conf ", conf);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1);
        assert_eq!(out.configs[0].name, "tun-p1");
    }

    #[test]
    fn ticket24_data_base64_case_insensitive() {
        let inner = "trojan://pw@b.example.com:443#two".to_string();
        for marker in [";base64,", ";Base64,", ";BASE64,"] {
            let body = format!("data:text/plain{marker}{}", b64(&inner));
            let out = import_subscription_body(&body);
            assert!(
                out.failures.is_empty(),
                "marker {marker}: {:?}",
                out.failures
            );
            assert_eq!(out.configs.len(), 1, "marker {marker}");
        }
    }

    #[test]
    fn ticket24_peerless_conf_taxonomy() {
        let out = import_text("[Interface]\nPrivateKey = cHJpdmF0ZWtleQ\n");
        assert!(out.configs.is_empty());
        assert_eq!(out.failures.len(), 1);
        assert_eq!(out.failures[0].code, ImportErrorKind::Empty);
    }

    #[test]
    fn ticket24_invalid_json_message_is_static() {
        let out = import_text("[{\"type\": \"vless\", ");
        assert_eq!(out.failures.len(), 1);
        assert_eq!(out.failures[0].reason, "invalid-json: invalid JSON");
        let error = parse_link(&format!("vmess://{}", b64("{not json")))
            .expect_err("vmess with bad JSON fails");
        assert_eq!(error.kind, ImportErrorKind::InvalidJson);
        assert_eq!(error.to_string(), "invalid-json: invalid vmess JSON");
    }

    #[test]
    fn ticket24_query_empty_key_ignored() {
        let config = parse_link(&format!("vless://{UUID}@a.example.com:443?=oops&sni=#x"))
            .expect("empty query key ignored");
        let params = params_of(&config);
        assert!(params.get("").is_none(), "got {params}");
        assert_eq!(params["sni"], "a.example.com");
    }

    #[test]
    fn ticket24_scheme_message_keeps_input_case() {
        let body = serde_json::json!([
            {"type": "Gopher", "tag": "k", "server": "a.example.com", "server_port": 70}
        ]);
        let out = import_text(&body.to_string());
        assert_eq!(out.failures.len(), 1);
        assert!(
            out.failures[0].reason.contains("Gopher"),
            "got {}",
            out.failures[0].reason
        );
    }

    #[test]
    fn ticket24_identity_trims_brackets_trailing_dot() {
        let a = normalized_identity(
            "vless",
            "{\"type\":\"vless\",\"server\":\"Example.COM.\",\"server_port\":443,\"uuid\":\"u\"}",
        );
        let b = normalized_identity(
            "vless",
            "{\"type\":\"vless\",\"server\":\"example.com\",\"server_port\":443,\"uuid\":\"u\"}",
        );
        assert_eq!(a, b, "trailing dot is not identity");
        let c = normalized_identity(
            "vless",
            "{\"type\":\"vless\",\"server\":\"[::1]\",\"server_port\":443,\"uuid\":\"u\"}",
        );
        let d = normalized_identity(
            "vless",
            "{\"type\":\"vless\",\"server\":\"::1\",\"server_port\":443,\"uuid\":\"u\"}",
        );
        assert_eq!(c, d, "brackets are not identity");
    }

    #[test]
    fn ticket24_long_names_truncated() {
        let long = "n".repeat(500);
        let config = parse_link(&format!("vless://{UUID}@a.example.com:443#{long}"))
            .expect("long remark parses");
        assert!(
            config.name.chars().count() <= 256,
            "len {}",
            config.name.len()
        );
    }

    #[test]
    fn ticket24_huge_base64_rejected() {
        let big = format!("vmess://{}", "A".repeat(3_000_000));
        let error = parse_link(&big).expect_err("huge payload fails");
        assert_eq!(error.kind, ImportErrorKind::BadBase64);
    }

    // -- Ticket 27: SSH + Noise links -------------------------------------

    #[test]
    fn ticket27_ssh_link_parses_user_password_default_port() {
        let config = parse_link("ssh://alice:hunter2@example.com:2222#office").expect("ssh parses");
        assert_eq!(config.protocol, "ssh");
        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, 2222);
        assert_eq!(config.name, "office");
        let params = params_of(&config);
        assert_eq!(params["user"], "alice");
        assert_eq!(params["username"], "alice");
        assert_eq!(params["password"], "hunter2");

        let defaulted = parse_link("ssh://bob:secret@example.com#d").expect("port defaults");
        assert_eq!(defaulted.port, 22, "ssh omits port like hy2 omits to 443");
    }

    #[test]
    fn ticket27_ssh_link_rejects_missing_credentials() {
        let error = parse_link("ssh://:pw@example.com:22#x").expect_err("user required");
        assert_eq!(error.kind, ImportErrorKind::MissingCredential);
        assert!(!error.message.contains("pw"), "no secrets: {error}");
        let error = parse_link("ssh://alice@example.com:22#x")
            .expect_err("password or private_key required");
        assert_eq!(error.kind, ImportErrorKind::MissingCredential);
        // ?private_key= satisfies the secret half.
        let keyed = parse_link("ssh://alice@example.com:22?private_key=KEYMAT#x")
            .expect("private_key satisfies secret");
        assert_eq!(params_of(&keyed)["private_key"], "KEYMAT");
    }

    #[test]
    fn ticket27_noise_link_parses_key_requires_port() {
        let config = parse_link("noise://NOISE-KEY@example.com:51820#n").expect("noise parses");
        assert_eq!(config.protocol, "noise");
        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, 51820);
        let params = params_of(&config);
        assert_eq!(params["password"], "NOISE-KEY");
        assert_eq!(params["key"], "NOISE-KEY");

        let error = parse_link("noise://@example.com:51820#x").expect_err("key required");
        assert_eq!(error.kind, ImportErrorKind::MissingCredential);
        let error = parse_link("noise://k@example.com#x").expect_err("noise port required");
        assert_eq!(error.kind, ImportErrorKind::BadPort);
    }

    #[test]
    fn ticket27_ssh_noise_json_rows_validate_credentials() {
        for (body, ok) in [
            (
                serde_json::json!([{"type": "ssh", "server": "a.example.com",
                    "server_port": 22, "username": "u", "password": "p"}]),
                true,
            ),
            (
                serde_json::json!([{"type": "ssh", "server": "a.example.com",
                    "server_port": 22, "username": "u"}]),
                false,
            ),
            (
                serde_json::json!([{"type": "noise", "server": "a.example.com",
                    "server_port": 51820, "key": "k"}]),
                true,
            ),
            (
                serde_json::json!([{"type": "noise", "server": "a.example.com",
                    "server_port": 51820}]),
                false,
            ),
        ] {
            let out = import_text(&body.to_string());
            if ok {
                assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
            } else {
                assert_eq!(out.failures.len(), 1, "one credential failure");
                assert!(
                    out.failures[0].reason.contains("missing"),
                    "got {}",
                    out.failures[0].reason
                );
            }
        }
    }

    #[test]
    fn ticket27_internal_addsub_blocked_without_url_echo() {
        use base64::Engine as _;
        let payload =
            base64::engine::general_purpose::STANDARD.encode("http://127.0.0.1/sub.txt#g");
        let error = parse_addsub_link(&format!("throne://addsub/{payload}"))
            .expect_err("internal addsub blocked");
        assert_eq!(error.kind, ImportErrorKind::BadHost);
        assert!(
            !error.message.contains("127.0.0.1"),
            "no host echo: {error}"
        );
    }

    // -- Ticket 42: serve-everything intake --------------------------------
    // New native protocols parse; schema-less protocols reject loudly at
    // import (never parse-ok/start-fail); conf/URI parity; sub-payload gaps.

    /// Ticket 42 (US-22): `hysteria2+realm://` rejects at import with
    /// guidance (rendezvous has no link mapping) — never parse-ok/start-fail.
    #[test]
    fn t42_hy2_realm_rejects_at_import_with_guidance() {
        let error = parse_link("hysteria2+realm://letmein@example.com:8443#r")
            .expect_err("realm variant rejects at import");
        assert_eq!(error.kind, ImportErrorKind::UnknownScheme);
        assert!(error.message.contains("realm"), "names realm: {error}");
    }

    /// Ticket 42: `v2rayn://base64(json)` parses exactly like legacy vmess.
    #[test]
    fn t42_v2rayn_link_parses_as_vmess() {
        let json = serde_json::json!({
            "v": "2", "ps": "v2rayn-one", "add": "example.com", "port": "10086",
            "id": UUID, "aid": "0", "scy": "auto", "net": "tcp",
            "type": "none", "host": "", "path": "", "tls": ""
        });
        let config =
            parse_link(&format!("v2rayn://{}", b64(&json.to_string()))).expect("v2rayn parses");
        assert_eq!(config.protocol, "vmess");
        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, 10086);
        assert_eq!(config.name, "v2rayn-one");
        assert_eq!(params_of(&config)["uuid"], UUID);
    }

    /// Ticket 42 (US-24): snell links parse (psk required, version defaults
    /// to 4, bad versions reject at import — never start-fail).
    #[test]
    fn t42_snell_link_parses() {
        let config =
            parse_link("snell://psk-secret@example.com:8388#snell-one").expect("snell parses");
        assert_eq!(config.protocol, "snell");
        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, 8388);
        let params = params_of(&config);
        assert_eq!(params["psk"], "psk-secret");
        assert_eq!(params["version"], "4");
        let error = parse_link("snell://psk@example.com:8388?version=5#x")
            .expect_err("bad snell version rejects");
        assert_eq!(error.kind, ImportErrorKind::InvalidJson);
        assert!(error.message.contains("version"), "names version: {error}");
        let error = parse_link("snell://@example.com:8388#x").expect_err("missing psk rejects");
        assert_eq!(error.kind, ImportErrorKind::MissingCredential);
    }

    /// Ticket 42 (US-24): shadowtls links parse (sni rides along, version
    /// validated at import; v1 rows may omit the password).
    #[test]
    fn t42_shadowtls_link_parses() {
        let config = parse_link("shadowtls://shadowpw@example.com:443?sni=example.com#st")
            .expect("shadowtls parses");
        assert_eq!(config.protocol, "shadowtls");
        assert_eq!(params_of(&config)["sni"], "example.com");
        let v1 = parse_link("shadowtls://@example.com:443?version=1#v1")
            .expect("v1 without password parses");
        assert_eq!(params_of(&v1)["version"], "1");
        let error = parse_link("shadowtls://pw@example.com:443?version=9#x")
            .expect_err("bad shadowtls version rejects");
        assert_eq!(error.kind, ImportErrorKind::InvalidJson);
    }

    /// Ticket 42 (US-24): mieru/juicity have no upstream sing-box schema —
    /// import fails loudly naming the gap, never at start.
    #[test]
    fn t42_mieru_juicity_reject_with_guidance() {
        for link in [
            "mieru://user:pass@example.com:1080#m",
            "juicity://uuid:pass@example.com:23142#j",
            "MIERU://user:pass@example.com:1080#m",
        ] {
            let error = parse_link(link).expect_err(&format!("{link} rejects"));
            assert_eq!(error.kind, ImportErrorKind::UnknownScheme, "link {link}");
            assert!(
                error.message.contains("no sing-box"),
                "names the schema gap: {error}"
            );
        }
    }

    /// Ticket 42 (US-24): endpoint-only / 1.15+-only protocols reject at
    /// import with guidance (they are not 1.14.2 outbounds).
    #[test]
    fn t42_endpoint_protocols_reject_with_guidance() {
        for link in [
            "openvpn://user:pass@example.com:1194#o",
            "masque://user@example.com:443#m",
            "trusttunnel://x@example.com:443#t",
        ] {
            let error = parse_link(link).expect_err(&format!("{link} rejects"));
            assert_eq!(error.kind, ImportErrorKind::UnknownScheme, "link {link}");
            assert!(
                error.message.contains("endpoint") || error.message.contains("1.14.2"),
                "names the gap: {error}"
            );
        }
    }

    /// Ticket 42 (US-25): `.conf` key parity with URI intake — reserved and
    /// every other mapped key survive identically on both paths.
    #[test]
    fn t42_wg_conf_key_parity_with_uri() {
        let conf = "[Interface]\nPrivateKey = cHJpdmF0ZWtleQ\nAddress = 10.0.0.2/32\nDNS = 1.1.1.1\nMTU = 1420\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\nAllowedIPs = 0.0.0.0/0\nPersistentKeepalive = 25\nPresharedKey = cHJlc2hhcmVk\nReserved = 1,2,3\nFwMark = 100\n";
        let out = import_file_content("parity.conf", conf);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1);
        let params = params_of(&out.configs[0]);
        for key in [
            "private_key",
            "public_key",
            "address",
            "dns",
            "mtu",
            "allowed_ips",
            "persistent_keepalive",
            "pre_shared_key",
            "reserved",
            "fwmark",
        ] {
            assert!(
                params.get(key).is_some_and(|v| {
                    v.as_str().is_some_and(|s| !s.trim().is_empty()) || v.is_number()
                }),
                "conf key {key} survives: {params}"
            );
        }
        // URI intake with the same extras carries the same key set.
        let uri = parse_link("wireguard://cHJpdmF0ZWtleQ@203.0.113.1:51820?publickey=cHVia2V5&address=10.0.0.2%2F32&dns=1.1.1.1&mtu=1420&allowedips=0.0.0.0%2F0&reserved=1%2C2%2C3&fwmark=100#u")
            .expect("uri parses");
        let uri_params = params_of(&uri);
        for key in ["address", "dns", "mtu", "reserved", "fwmark"] {
            assert!(
                uri_params.get(key).is_some(),
                "uri key {key} survives: {uri_params}"
            );
        }
    }

    /// Ticket 42 (US-27): bare SIP008 server lists (typeless objects with
    /// method+password) land as Shadowsocks rows instead of failing the
    /// whole body.
    #[test]
    fn t42_bare_sip008_array_lands() {
        let body = serde_json::json!([
            {"server": "a.example.com", "server_port": 8388,
             "method": "aes-128-gcm", "password": "pass1", "remarks": "bare-one"},
            {"server": "b.example.com", "server_port": 8389,
             "method": "chacha20-poly1305", "password": "pass2"},
        ]);
        let out = import_subscription_body(&body.to_string());
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 2);
        assert_eq!(out.configs[0].protocol, "shadowsocks");
        assert_eq!(out.configs[0].name, "bare-one");
        assert_eq!(params_of(&out.configs[1])["method"], "chacha20-poly1305");
    }

    /// Ticket 42 (US-27): v2rayn subscription lines (per-line base64 of
    /// vmess JSON, no scheme) land as vmess rows.
    #[test]
    fn t42_v2rayn_subscription_lines_land() {
        let line = b64(
            &serde_json::json!({"v": "2", "ps": "sub-one", "add": "sub.example.com",
                "port": "10086", "id": UUID, "aid": "0"})
            .to_string(),
        );
        let out = import_subscription_body(&format!("{line}\n{line}"));
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 2);
        assert!(out.configs.iter().all(|c| c.protocol == "vmess"));
        assert_eq!(out.configs[0].name, "sub-one");
        assert_eq!(out.configs[1].name, "sub-one-2");
    }

    /// Ticket 42 (US-27): Clash inline-JSON entries land; Clash-native
    /// snell/shadowtls types land; ssr and flow mappings skip per-item with
    /// a named reason.
    #[test]
    fn t42_clash_inline_json_meta_and_skips() {
        let body = "proxies:\n  - {\"name\": \"inline-one\", \"type\": \"vmess\", \"server\": \"i.example.com\", \"port\": 10086, \"uuid\": \"UUID\", \"alterId\": 0}\n  - name: snell-one\n    type: snell\n    server: s.example.com\n    port: 8388\n    psk: snellpsk\n    version: 4\n  - name: ssr-one\n    type: ssr\n    server: r.example.com\n    port: 8388\n    password: x\n  - {flow: bad}\n";
        let body = body.replace("UUID", UUID);
        let out = import_subscription_body(&body);
        assert_eq!(
            out.configs.len(),
            2,
            "inline + snell land: {:?}",
            out.failures
        );
        assert!(
            out.configs
                .iter()
                .any(|c| c.name == "inline-one" && c.protocol == "vmess"),
            "inline JSON lands"
        );
        assert!(
            out.configs
                .iter()
                .any(|c| c.name == "snell-one" && c.protocol == "snell"),
            "Meta snell lands"
        );
        assert_eq!(out.failures.len(), 2, "ssr + flow skip: {:?}", out.failures);
        assert!(
            out.failures
                .iter()
                .any(|f| f.label == "ssr-one" && f.reason.contains("ssr")),
            "ssr skip names the type: {:?}",
            out.failures
        );
    }

    /// Ticket 42 (US-27): Clash entries without credentials fail per-item at
    /// import (never parse-ok/start-fail).
    #[test]
    fn t42_clash_missing_credentials_rejected_per_item() {
        let body = "proxies:\n  - name: nouuid\n    type: vless\n    server: v.example.com\n    port: 443\n";
        let out = import_subscription_body(body);
        assert!(out.configs.is_empty());
        assert_eq!(out.failures.len(), 1);
        assert!(
            out.failures[0].reason.contains("missing"),
            "got {}",
            out.failures[0].reason
        );
    }

    /// Ticket 42 (US-20/US-24): the full import→commit→serve chain for the
    /// new natives — parsed links commit to the store (save-gate accepts
    /// them) and generate native outbounds (no parse-ok/commit-fail and no
    /// parse-ok/start-fail).
    #[test]
    fn t42_new_natives_commit_and_serve() {
        let body = [
            "socks://user:pass@192.0.2.20:1080#socks-one",
            "http://user:pass@192.0.2.21:8080#http-one",
            "snell://psk-secret@192.0.2.23:8388?version=4#snell-one",
            "shadowtls://shadowpw@192.0.2.24:443?sni=example.com&version=3#st-one",
            "naive+https://user:pass@192.0.2.25:8443#naive-one",
            "hysteria2://letmein@192.0.2.26:8443?sni=example.com#hy2-one",
        ]
        .join("\n");
        let batch = import_text(&body);
        assert!(batch.failures.is_empty(), "failures: {:?}", batch.failures);
        assert_eq!(batch.configs.len(), 6);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let outcome = commit_batch(&mut store, batch, "t42").expect("batch commits");
        assert_eq!(outcome.imported, 6, "all six commit");
        let rows = store.list().expect("list works");
        assert_eq!(rows.len(), 6);
        let generated = crate::engine::generate(&rows).expect("all six generate");
        let outbounds = generated.value()["outbounds"]
            .as_array()
            .expect("outbounds")
            .clone();
        for want in ["socks", "http", "snell", "shadowtls", "naive", "hysteria2"] {
            assert!(
                outbounds.iter().any(|node| node["type"] == want),
                "native {want} outbound present"
            );
        }
        assert!(
            !generated.json.contains("certificate_sha256"),
            "no 1.15-only fields"
        );
    }

    /// Ticket 42 (US-23): JSON rows with reality but no pbk reject at import
    /// (mirrors the link grammar — never parse-ok/start-fail).
    #[test]
    fn t42_json_reality_without_pbk_rejected() {
        let body = serde_json::json!([{
            "type": "vless", "server": "a.example.com", "server_port": 443,
            "uuid": UUID, "security": "reality", "sni": "a.example.com",
        }]);
        let out = import_text(&body.to_string());
        assert!(out.configs.is_empty());
        assert_eq!(out.failures.len(), 1);
        assert!(
            out.failures[0].reason.contains("pbk"),
            "got {}",
            out.failures[0].reason
        );
    }

    /// Empty addsub payloads reject as `Empty` with stable copy: a missing
    /// payload names the link, a decoded-but-blank payload names the payload.
    #[test]
    fn addsub_empty_payload_rejects_as_empty() {
        for link in ["throne://addsub", "throne://addsub/", "throne://addsub/   "] {
            let error = parse_addsub_link(link).expect_err(&format!("{link} rejects"));
            assert_eq!(error.kind, ImportErrorKind::Empty);
            assert_eq!(error.message, "throne link is missing its payload");
        }
        let link = format!("throne://addsub/{}", b64_standard("   "));
        let error = parse_addsub_link(&link).expect_err("blank inner rejects");
        assert_eq!(error.kind, ImportErrorKind::Empty);
        assert_eq!(error.message, "addsub payload is empty");
    }

    /// Mixed multi-file drops keep configs plus offset failures;
    /// re-committing the same files is all-duplicate and writes nothing.
    #[test]
    fn commit_all_duplicates_writes_nothing_and_multi_file_mixes_results() {
        let good_a = format!("vless://{UUID}@a.example.com:443#one");
        let files = [
            ("a.txt", good_a.as_str()),
            ("b.txt", "trojan://pw@b.example.com:443#two"),
            ("bad.txt", "not a link"),
        ];
        let mixed = import_multi_file_contents(&files);
        assert_eq!(mixed.configs.len(), 2);
        assert_eq!(mixed.failures.len(), 1);
        assert_eq!(mixed.failures[0].index, 2);
        let mut store = Store::open_in_memory().expect("store opens");
        let first = commit_batch(&mut store, mixed, "mixed").expect("commit works");
        assert_eq!(first.imported, 2);
        assert_eq!(first.duplicates, 0);
        assert_eq!(first.failures.len(), 1);
        assert_eq!(first.failures[0].index, 2);
        assert_eq!(store.list().expect("list works").len(), 2);
        let retry = import_multi_file_contents(&files);
        let second = commit_batch(&mut store, retry, "mixed").expect("recommit works");
        assert_eq!(second.imported, 0);
        assert_eq!(second.duplicates, 2);
        assert_eq!(second.failures.len(), 1);
        assert_eq!(second.failures[0].index, 2);
        assert_eq!(store.list().expect("list works").len(), 2);
    }
}
