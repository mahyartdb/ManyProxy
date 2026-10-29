//! Share-link and subscription parsing into `NewConfig`, plus open-with classification.
//! Belongs: turning text and URLs into import candidates and batch results.
//! Not here: dialogs, store commits, engine calls.
use std::collections::{HashMap, HashSet};

use base64::Engine as _;

use crate::store::{NewConfig, Store, StoreError, DEFAULT_GROUP};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedConfig {
    pub name: String,

    pub protocol: String,

    pub params: String,

    pub host: String,

    pub port: u16,
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportFailure {
    pub index: usize,
    pub label: String,
    pub reason: String,
    pub code: ImportErrorKind,
}

impl ImportFailure {
    fn new(index: usize, label: impl Into<String>, error: ImportError) -> Self {
        let label = sanitize_name(&label.into());
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportOutcome {
    pub imported: usize,
    pub imported_ids: Vec<String>,
    pub duplicates: usize,
    pub failures: Vec<ImportFailure>,
}

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

const MAX_B64_COMPACT: usize = 1_000_000;

const MAX_QUERY_ITEM: usize = 4096;

const MAX_QUERY_PAIRS: usize = 256;

const MAX_NAME_CHARS: usize = 256;

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

fn strip_bom(text: &str) -> &str {
    let mut rest = text;
    while let Some(stripped) = rest.strip_prefix('\u{FEFF}') {
        rest = stripped;
    }
    rest
}

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
        if host.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(bad_host("bad host"));
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
                if host.parse::<std::net::Ipv6Addr>().is_ok() {
                    if port.is_empty() {
                        return Err(bad_port("bad port ''"));
                    }
                    return Ok((host.to_string(), Some(port.to_string())));
                }
                if text.parse::<std::net::Ipv6Addr>().is_ok() {
                    return Ok((text.to_string(), None));
                }
                return Err(bad_host("bare IPv6 must use [brackets]"));
            }
            if host.is_empty() || host.chars().any(|c| c.is_whitespace() || c.is_control()) {
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

fn decoded_host(raw: &str) -> Result<String, ImportError> {
    let host = percent_decode(raw.trim(), false);
    if host.trim().is_empty() {
        return Err(bad_host("missing host"));
    }
    if host.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(bad_host("bad host"));
    }
    Ok(host)
}

fn parse_port(text: &str) -> Result<u16, ImportError> {
    let first = text.split([',', ';']).next().unwrap_or("").trim();
    if first.len() > 1 && first.starts_with('0') {
        return Err(bad_port(format!("bad port '{}'", cap_echo(text))));
    }

    if first.starts_with('+') || first.starts_with('-') {
        return Err(bad_port(format!("bad port '{}'", cap_echo(text))));
    }
    match first.parse::<u32>() {
        Ok(port) if (1..=65_535).contains(&port) => Ok(port as u16),
        _ => Err(bad_port(format!("bad port '{}'", cap_echo(text)))),
    }
}

fn require_port(port_text: Option<&str>) -> Result<u16, ImportError> {
    match port_text {
        Some(text) => parse_port(text),
        None => Err(bad_port("missing port")),
    }
}

struct UriParts {
    userinfo: Option<String>,
    host: String,
    port_text: Option<String>,
    query: HashMap<String, String>,
    fragment: String,

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

    let (userinfo, host_port) = match authority.rfind('@') {
        Some(i) => (Some(authority[..i].to_string()), &authority[i + 1..]),
        None => (None, authority),
    };
    let (raw_host, port_text) = split_host_port(host_port)?;

    let host = decoded_host(&raw_host)?;
    Ok(UriParts {
        userinfo,
        host,
        port_text,
        query: query_map(query_raw),
        fragment,
        path: before_query[authority_end..].to_string(),
    })
}

fn reject_unexpected_path(protocol: &str, parts: &UriParts) -> Result<(), ImportError> {
    if !parts.path.trim_matches('/').is_empty() {
        return Err(bad_host(format!("{protocol} link has an unexpected path")));
    }
    Ok(())
}

fn query_map(raw: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if raw.is_empty() {
        return map;
    }
    for pair in raw.split('&').take(MAX_QUERY_PAIRS) {
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

fn cap_name(name: String) -> String {
    let clean = sanitize_name(&name);
    if clean.chars().count() > MAX_NAME_CHARS {
        clean.chars().take(MAX_NAME_CHARS).collect()
    } else {
        clean
    }
}

fn cap_echo(text: &str) -> String {
    const MAX_ECHO_CHARS: usize = 64;
    if text.chars().count() > MAX_ECHO_CHARS {
        format!("{}…", text.chars().take(MAX_ECHO_CHARS).collect::<String>())
    } else {
        text.to_string()
    }
}

fn sanitize_name(text: &str) -> String {
    const CSI_FINAL: std::ops::RangeInclusive<char> = '@'..='~';
    let mut no_ansi = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1B}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for c in chars.by_ref() {
                    if CSI_FINAL.contains(&c) {
                        break;
                    }
                }
            }
            continue;
        }
        no_ansi.push(c);
    }
    no_ansi
        .chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(
                    c,
                    '\u{202A}'..='\u{202E}'
                        | '\u{2066}'..='\u{2069}'
                        | '\u{200E}'
                        | '\u{200F}'
                        | '\u{061C}'
                        | '\u{FEFF}'
                )
        })
        .collect()
}

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

fn query_sni(query: &HashMap<String, String>, host: &str) -> String {
    query
        .get("sni")
        .filter(|sni| !sni.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| host.to_string())
}

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

    let uri_path = percent_decode(&parts.path, true);
    if !uri_path.trim_matches('/').is_empty() && !query.contains_key("path") {
        insert_str(&mut params, "path", uri_path);
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
        if inner.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(bad_host("vmess link has a bad \"add\""));
        }
        inner.to_string()
    } else {
        if host_trimmed
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(bad_host("vmess link has a bad \"add\""));
        }
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
            cap_name(ps)
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

    let tls_on = match obj.get("tls") {
        Some(serde_json::Value::Bool(flag)) => *flag,
        Some(serde_json::Value::String(text)) => matches!(
            text.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on" | "enabled" | "tls"
        ),
        Some(serde_json::Value::Number(number)) => {
            number.as_f64().is_some_and(|number| number != 0.0)
        }
        _ => false,
    };
    params.insert("tls".to_string(), serde_json::Value::Bool(tls_on));
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
                (
                    percent_decode(&userinfo_raw[..j], false),
                    percent_decode(&userinfo_raw[j + 1..], false),
                )
            } else {
                let text = decode_b64_utf8(userinfo_raw)?;
                let j = text
                    .find(':')
                    .ok_or_else(|| missing_credential("ss link is missing password"))?;
                (
                    percent_decode(&text[..j], false),
                    percent_decode(&text[j + 1..], false),
                )
            };

            let (host, port_text) = split_host_port(host_port.trim_end_matches('/'))?;
            (method, password, host, require_port(port_text.as_deref())?)
        }
        None => {
            let payload = payload.trim_end_matches('/');
            let text = decode_b64_utf8(payload)?;
            let i = text
                .rfind('@')
                .ok_or_else(|| bad_host("bad ss link (missing host)"))?;
            let (userinfo, host_port) = (&text[..i], &text[i + 1..]);
            let j = userinfo
                .find(':')
                .ok_or_else(|| missing_credential("ss link is missing password"))?;

            let (host, port_text) = split_host_port(host_port.trim_end_matches('/'))?;
            (
                percent_decode(&userinfo[..j], false),
                userinfo[j + 1..].to_string(),
                host,
                require_port(port_text.as_deref())?,
            )
        }
    };
    if method.trim().is_empty() {
        return Err(missing_credential("ss link is missing method"));
    }

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

fn parse_hy2(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    reject_unexpected_path("hysteria2", parts)?;
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

fn parse_snell(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    reject_unexpected_path("snell", parts)?;
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

fn parse_shadowtls(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    reject_unexpected_path("shadowtls", parts)?;
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

fn parse_mieru(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    reject_unexpected_path("mieru", parts)?;
    let (mut username, mut password) = match parts.userinfo.as_deref() {
        Some(raw) if !raw.is_empty() => match raw.find(':') {
            Some(i) => (
                percent_decode(&raw[..i], false),
                percent_decode(&raw[i + 1..], false),
            ),
            None => (percent_decode(raw, false), String::new()),
        },
        _ => (String::new(), String::new()),
    };
    if username.trim().is_empty() {
        username = parts.query.get("username").cloned().unwrap_or_default();
    }
    if password.trim().is_empty() {
        password = parts.query.get("password").cloned().unwrap_or_default();
    }
    if username.trim().is_empty() {
        return Err(missing_credential("mieru link is missing username"));
    }
    if password.trim().is_empty() {
        return Err(missing_credential("mieru link is missing password"));
    }
    let port = require_port(parts.port_text.as_deref())?;
    let mut params = base_params("mieru", &parts.host, port);
    insert_str(&mut params, "username", username);
    insert_str(&mut params, "password", password);
    insert_extras(
        &mut params,
        &parts.query,
        &["username", "password", "server", "server_port", "port"],
    );
    Ok(ImportedConfig {
        name: display_name(&parts.fragment, "mieru", &parts.host, port),
        protocol: "mieru".to_string(),
        params: finish(params),
        host: parts.host.clone(),
        port,
    })
}

fn parse_juicity(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    reject_unexpected_path("juicity", parts)?;
    let raw = parts.userinfo.as_deref().unwrap_or("");
    let i = raw
        .find(':')
        .ok_or_else(|| missing_credential("juicity link is missing password"))?;
    let (uuid, password) = (
        percent_decode(&raw[..i], false),
        percent_decode(&raw[i + 1..], false),
    );
    if uuid.trim().is_empty() {
        return Err(missing_credential("juicity link is missing uuid"));
    }
    if password.trim().is_empty() {
        return Err(missing_credential("juicity link is missing password"));
    }
    let port = require_port(parts.port_text.as_deref())?;
    let mut params = base_params("juicity", &parts.host, port);
    insert_str(&mut params, "uuid", uuid);
    insert_str(&mut params, "password", password);
    insert_extras(
        &mut params,
        &parts.query,
        &["uuid", "password", "server", "server_port", "port"],
    );
    Ok(ImportedConfig {
        name: display_name(&parts.fragment, "juicity", &parts.host, port),
        protocol: "juicity".to_string(),
        params: finish(params),
        host: parts.host.clone(),
        port,
    })
}

fn parse_trusttunnel(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    reject_unexpected_path("trusttunnel", parts)?;
    let raw = parts.userinfo.as_deref().unwrap_or("");
    let (username, password) = match raw.find(':') {
        Some(i) => (
            percent_decode(&raw[..i], false),
            percent_decode(&raw[i + 1..], false),
        ),
        None => (percent_decode(raw, false), String::new()),
    };
    if username.trim().is_empty() {
        return Err(missing_credential("trusttunnel link is missing username"));
    }
    if password.trim().is_empty() {
        return Err(missing_credential("trusttunnel link is missing password"));
    }
    let port = require_port(parts.port_text.as_deref())?;
    let mut params = base_params("trusttunnel", &parts.host, port);
    insert_str(&mut params, "username", username);
    insert_str(&mut params, "password", password);
    insert_extras(
        &mut params,
        &parts.query,
        &["username", "password", "server", "server_port", "port"],
    );
    Ok(ImportedConfig {
        name: display_name(&parts.fragment, "trusttunnel", &parts.host, port),
        protocol: "trusttunnel".to_string(),
        params: finish(params),
        host: parts.host.clone(),
        port,
    })
}

fn parse_json_link(rest: &str) -> Result<ImportedConfig, ImportError> {
    let (payload, fragment) = match rest.find('#') {
        Some(i) => (&rest[..i], rest[i + 1..].to_string()),
        None => (rest, String::new()),
    };
    if payload.trim().is_empty() {
        return Err(ImportError::new(
            ImportErrorKind::Empty,
            "json link is missing its payload",
        ));
    }
    let text = decode_b64_utf8(payload)?;
    let value: serde_json::Value =
        serde_json::from_str(text.trim()).map_err(|_| invalid_json("invalid json link payload"))?;
    if !value.is_object() {
        return Err(invalid_json(
            "invalid json link payload: expected an object",
        ));
    }
    if let Some(terminal) = xray_terminal_kind(&value) {
        return Err(unknown_scheme(format!(
            "Xray terminal '{terminal}' carries no importable endpoint — dropped"
        )));
    }
    let mut config = json_outbound(0, &value)?;
    if !fragment.trim().is_empty() {
        config.name = display_name(&fragment, &config.protocol, &config.host, config.port);
    }
    Ok(config)
}

fn unsupported_protocol_guidance(scheme: &str) -> ImportError {
    let detail = match scheme {
        "openvpn" => "openvpn is a sing-box endpoint (openvpn-client), not a 1.14.2 outbound — import rejected; serve it via a \"custom-config\" row instead",
        "masque" => "masque-client is a sing-box 1.15.0+ endpoint, not a 1.14.2 outbound — import rejected; serve it via a \"custom-config\" row on a newer core instead",


        "tailscale" => "tailscale is a sing-box endpoint, not a 1.14.2 outbound — import rejected; serve it via a \"custom-config\" row instead",
        "openconnect" => "openconnect has no sing-box 1.14.2 outbound schema — import rejected; serve it via a \"custom-config\" row instead",
        "autoselector" => "autoselector is a group/failover construct, not a sing-box 1.14.2 outbound — import rejected; serve the concrete rows directly or via a \"custom-config\" row instead",
        _ => "this protocol has no sing-box 1.14.2 outbound schema — import rejected",
    };
    unknown_scheme(format!("unsupported scheme '{scheme}' ({detail})"))
}

fn parse_direct_link(rest: &str) -> Result<ImportedConfig, ImportError> {
    let fragment = rest.find('#').map(|i| &rest[i + 1..]).unwrap_or("");
    let remark = percent_decode(fragment, false);
    let name = if remark.trim().is_empty() {
        "direct".to_string()
    } else {
        cap_name(remark)
    };
    let mut params = Params::new();
    params.insert(
        "type".to_string(),
        serde_json::Value::String("direct".to_string()),
    );
    Ok(ImportedConfig {
        name,
        protocol: "direct".to_string(),
        params: finish(params),
        host: String::new(),
        port: 0,
    })
}

fn parse_hy1(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    reject_unexpected_path("hysteria", parts)?;
    let mut auth = percent_decode(parts.userinfo.as_deref().unwrap_or(""), false);
    if auth.trim().is_empty() {
        auth = parts.query.get("auth").cloned().unwrap_or_default();
    }
    if auth.trim().is_empty() {
        return Err(missing_credential("hysteria link is missing auth"));
    }
    let port = require_port(parts.port_text.as_deref())?;
    let mut params = base_params("hysteria", &parts.host, port);
    insert_str(&mut params, "auth", auth);
    for (alias, field) in [("upmbps", "up"), ("downmbps", "down")] {
        if !parts.query.contains_key(field) {
            if let Some(value) = parts
                .query
                .get(alias)
                .filter(|value| !value.trim().is_empty())
            {
                insert_str(&mut params, field, value.clone());
            }
        }
    }
    insert_extras(
        &mut params,
        &parts.query,
        &[
            "auth",
            "server",
            "server_port",
            "port",
            "upmbps",
            "downmbps",
        ],
    );
    Ok(ImportedConfig {
        name: display_name(&parts.fragment, "hysteria", &parts.host, port),
        protocol: "hysteria".to_string(),
        params: finish(params),
        host: parts.host.clone(),
        port,
    })
}

fn parse_tuic(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    reject_unexpected_path("tuic", parts)?;
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
    if password.trim().is_empty() {
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

fn parse_wg_like(protocol: &str, parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    reject_unexpected_path(protocol, parts)?;
    let private_key = percent_decode(parts.userinfo.as_deref().unwrap_or(""), false);
    if private_key.trim().is_empty() {
        return Err(missing_credential(format!(
            "{protocol} link is missing private key"
        )));
    }
    let query = &parts.query;

    let public_key = query
        .get("publickey")
        .or_else(|| query.get("public_key"))
        .cloned()
        .unwrap_or_default();
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

fn parse_wg_uri(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    parse_wg_like("wireguard", parts)
}

fn parse_awg_uri(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    parse_wg_like("amneziawg", parts)
}

fn parse_socks_like(
    protocol: &str,
    parts: &UriParts,
    default_port: Option<u16>,
) -> Result<ImportedConfig, ImportError> {
    reject_unexpected_path(protocol, parts)?;
    let port = match (parts.port_text.as_deref(), default_port) {
        (Some(text), _) => parse_port(text)?,
        (None, Some(fallback)) => fallback,
        (None, None) => return Err(bad_port("missing port")),
    };
    let mut params = base_params(protocol, &parts.host, port);
    if let Some(raw) = parts.userinfo.as_deref() {
        match raw.find(':') {
            Some(i) => {
                let username = percent_decode(&raw[..i], false);
                let password = percent_decode(&raw[i + 1..], false);
                if username.trim().is_empty() || password.trim().is_empty() {
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
        if let Some(version) = parts.query.get("version") {
            let version = version.trim();
            if version == "4" {
                return Err(unknown_scheme("SOCKS v4 is not supported"));
            }
            if !version.is_empty() && version != "5" {
                return Err(unknown_scheme(format!(
                    "SOCKS version '{}' has no mapping (only v5 is served)",
                    cap_echo(version)
                )));
            }
        }
        insert_str(&mut params, "version", "5".to_string());
    }

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
    reject_unexpected_path("anytls", parts)?;
    let password = percent_decode(parts.userinfo.as_deref().unwrap_or(""), false);
    if password.trim().is_empty() {
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
    reject_unexpected_path("naive", parts)?;
    let raw = parts.userinfo.as_deref().unwrap_or("");
    let i = raw
        .find(':')
        .ok_or_else(|| missing_credential("naive link is missing password"))?;
    let (username, password) = (
        percent_decode(&raw[..i], false),
        percent_decode(&raw[i + 1..], false),
    );
    if username.trim().is_empty() || password.trim().is_empty() {
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

fn parse_ssh(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    reject_unexpected_path("ssh", parts)?;
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
    if password.trim().is_empty() && !query_key {
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
    if !password.trim().is_empty() {
        insert_str(&mut params, "password", password);
    }

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

fn parse_noise(parts: &UriParts) -> Result<ImportedConfig, ImportError> {
    reject_unexpected_path("noise", parts)?;
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

fn parse_throne_rest(scheme: &str, rest: &str) -> Result<ImportedConfig, ImportError> {
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
    if command.eq_ignore_ascii_case("addsub") {
        return Err(unknown_scheme(format!(
            "unknown {scheme} command 'addsub' (subscription links add through the single-line intake, never the batch path)"
        )));
    }
    if !command.eq_ignore_ascii_case("add") {
        return Err(unknown_scheme(format!(
            "unknown {scheme} command '{}'",
            cap_echo(command)
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
    if let Some(terminal) = xray_terminal_kind(&value) {
        return Err(unknown_scheme(format!(
            "Xray terminal '{terminal}' carries no importable endpoint — dropped"
        )));
    }
    json_outbound(0, &value)
}

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
    "direct",
    "mieru",
    "juicity",
    "trusttunnel",
];

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
    "shadow_tls",
    "v2rayn",
    "anytls",
    "ssh",
    "noise",
    "http",
    "https",
    "naive",
    "naive+https",
    "direct",
    "mieru",
    "juicity",
    "trusttunnel",
    "json",
];
fn normalize_outbound_type(raw: &str) -> Result<String, ImportError> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "vless" | "vmess" | "trojan" | "shadowsocks" | "hysteria2" | "hysteria" | "tuic"
        | "wireguard" | "amneziawg" | "socks" | "snell" | "shadowtls" | "anytls" | "ssh"
        | "noise" | "http" | "https" | "naive" => Ok(lower),

        "ss" => Ok("shadowsocks".to_string()),
        "socks5" => Ok("socks".to_string()),

        "v2rayn" => Ok("vmess".to_string()),
        "shadow-tls" | "shadow_tls" => Ok("shadowtls".to_string()),

        "awg" | "amnezia-wg" => Ok("amneziawg".to_string()),
        "hy2" | "hysteria2+realm" => Ok("hysteria2".to_string()),
        "wg" => Ok("wireguard".to_string()),
        "naive+https" | "naive_https" => Ok("naive".to_string()),

        "mieru" => Ok("mieru".to_string()),
        "juicity" => Ok("juicity".to_string()),
        "trusttunnel" => Ok("trusttunnel".to_string()),

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

        "direct" => Ok("direct".to_string()),
        "" => Err(invalid_json("item is missing outbound \"type\"")),

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

fn xray_terminal_kind(value: &serde_json::Value) -> Option<&'static str> {
    let raw = value
        .as_object()?
        .get("type")
        .or_else(|| value.as_object()?.get("protocol"))?
        .as_str()?;
    match raw.trim().to_ascii_lowercase().as_str() {
        "freedom" => Some("freedom"),
        "blackhole" => Some("blackhole"),
        _ => None,
    }
}

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

    if protocol == "direct" {
        let name = json_string(obj, &["tag", "remark", "remarks", "ps", "name"])
            .map(cap_name)
            .unwrap_or_else(|| "direct".to_string());
        let mut params = obj.clone();
        params.insert(
            "type".to_string(),
            serde_json::Value::String(protocol.clone()),
        );
        params.insert(
            "protocol".to_string(),
            serde_json::Value::String(protocol.clone()),
        );
        return Ok(ImportedConfig {
            name,
            protocol,
            params: finish(params),
            host: String::new(),
            port: 0,
        });
    }

    let host_raw = json_string(obj, &["server", "address"])
        .ok_or_else(|| ImportError::new(ImportErrorKind::BadHost, "item is missing \"server\""))?;
    let host_trimmed = host_raw.trim();

    let host = if let Some(inner) = host_trimmed.strip_prefix('[') {
        let inner = inner
            .strip_suffix(']')
            .ok_or_else(|| bad_host("item has a bad \"server\" (unclosed '[')"))?;
        if inner.trim().is_empty() {
            return Err(bad_host("item has a bad \"server\" (empty host)"));
        }
        inner
    } else {
        host_trimmed
    };
    if host.is_empty() || host.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(bad_host("item has a bad \"server\""));
    }
    let host = host.to_string();
    let port = json_port(obj)?;

    let name = json_string(obj, &["tag", "remark", "remarks", "ps", "name"])
        .unwrap_or_else(|| format!("{protocol}-{host}:{port}"));
    let name = cap_name(name);
    require_json_credentials(&protocol, obj)?;
    let mut params = obj.clone();

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

        "snell" => present(obj, &["psk"], false),

        "mieru" => present(obj, &["username", "user"], true) && present(obj, &["password"], false),
        "juicity" => present(obj, &["uuid", "id"], true) && present(obj, &["password"], false),
        "trusttunnel" => {
            present(obj, &["username", "user"], true) && present(obj, &["password"], false)
        }
        _ => true,
    };
    if !ok {
        return Err(missing_credential(format!(
            "item is missing credentials for \"{protocol}\""
        )));
    }

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
                            if xray_terminal_kind(item).is_some() {
                                continue;
                            }
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
        if xray_terminal_kind(item).is_some() {
            continue;
        }
        let label = item
            .get("tag")
            .or_else(|| item.get("remark"))
            .and_then(|value| value.as_str())
            .unwrap_or("json");
        match json_outbound(index, item) {
            Ok(config) => out.configs.push(config),
            Err(error) => {
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

const CONF_AWG_V2_KEYS: &[&str] = &[
    "jc", "jmin", "jmax", "s1", "s2", "s3", "s4", "h1", "h2", "h3", "h4", "i1", "i2", "i3", "i4",
    "i5",
];

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
            let key = line[..i].trim().to_ascii_lowercase();
            let value = line[i + 1..].trim().to_string();
            match entry.1.get_mut(&key) {
                Some(existing) if existing.is_empty() => *existing = value,
                Some(existing) if !value.is_empty() => {
                    existing.push_str(", ");
                    existing.push_str(&value);
                }
                Some(_) => {}
                None => {
                    entry.1.insert(key, value);
                }
            }
        }
    }
    if let Some((is_peer, section)) = current {
        if is_peer {
            peers.push((section, active_iface.clone()));
        }
    }
    if peers.is_empty() {
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
        return String::new();
    }
    match unhidden.rfind('.') {
        Some(i) => unhidden[..i].to_string(),
        None => unhidden.to_string(),
    }
}

fn valid_scheme_chars(scheme: &str) -> bool {
    let mut chars = scheme.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() => {}
        _ => return false,
    }

    chars.all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.' || c == '_')
}

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
        return Err(unknown_scheme(format!(
            "unknown scheme '{}'",
            cap_echo(scheme_raw)
        )));
    }
    let scheme = scheme_raw.to_ascii_lowercase();
    let rest = &text[sep + 3..];
    match scheme.as_str() {


        "vmess" | "v2rayn" => {

            let authority_end = rest.find(['?', '#']).unwrap_or(rest.len());
            if rest[..authority_end].contains('@') {
                parse_vless_like("vmess", &split_uri(&scheme, rest)?, "uuid")
            } else {
                let payload = &rest[..authority_end];

                let fragment = match rest[authority_end..].find('#') {
                    Some(i) => &rest[authority_end + i + 1..],
                    None => "",
                };
                parse_vmess_legacy(payload, fragment)
            }
        }
        "ss" | "shadowsocks" => parse_ss_rest(rest),


        "throne" | "manyproxy" => parse_throne_rest(&scheme, rest),
        "vless" => parse_vless_like("vless", &split_uri(&scheme, rest)?, "uuid"),
        "trojan" => parse_vless_like("trojan", &split_uri(&scheme, rest)?, "password"),



        "xray" => parse_vless_like("xray", &split_uri(&scheme, rest)?, "uuid"),
        "hysteria2" | "hy2" => parse_hy2(&split_uri(&scheme, rest)?),




        "hysteria2+realm" => Err(unknown_scheme(
            "hysteria2+realm rendezvous links carry no importable endpoint mapping (realm conflicts with server addressing in sing-box 1.14.2) — import rejected; serve it via a custom full-config row with a native \"realm\" block instead",
        )),
        "hysteria" => parse_hy1(&split_uri(&scheme, rest)?),
        "tuic" => parse_tuic(&split_uri(&scheme, rest)?),
        "wireguard" | "wg" => parse_wg_uri(&split_uri(&scheme, rest)?),
        "awg" | "amneziawg" | "amnezia-wg" => parse_awg_uri(&split_uri(&scheme, rest)?),
        "socks" | "socks5" => parse_socks_like("socks", &split_uri(&scheme, rest)?, None),
        "anytls" => parse_anytls(&split_uri(&scheme, rest)?),

        "ssh" => parse_ssh(&split_uri(&scheme, rest)?),
        "noise" => parse_noise(&split_uri(&scheme, rest)?),
        "http" => parse_socks_like("http", &split_uri(&scheme, rest)?, Some(80)),
        "https" => parse_socks_like("https", &split_uri(&scheme, rest)?, Some(443)),
        "naive" | "naive+https" => parse_naive(&split_uri(&scheme, rest)?),

        "snell" => parse_snell(&split_uri(&scheme, rest)?),
        "shadowtls" | "shadow-tls" | "shadow_tls" => {
            parse_shadowtls(&split_uri(&scheme, rest)?)
        }



        "direct" => parse_direct_link(rest),



        "mieru" => parse_mieru(&split_uri(&scheme, rest)?),
        "juicity" => parse_juicity(&split_uri(&scheme, rest)?),
        "trusttunnel" => parse_trusttunnel(&split_uri(&scheme, rest)?),

        "json" => parse_json_link(rest),




        "openvpn" | "masque" | "tailscale" | "openconnect" | "autoselector" => {
            Err(unsupported_protocol_guidance(&scheme))
        }

        _ => Err(unknown_scheme(format!(
            "unknown scheme '{}' (valid: {})",
            cap_echo(&scheme),
            VALID_LINK_SCHEMES.join(", ")
        ))),
    }
}

pub fn is_library_export(text: &str) -> bool {
    let trimmed = strip_bom(text.trim()).trim();
    if !trimmed.starts_with('{') {
        return false;
    }
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(serde_json::Value::Object(map)) => {
            // T-22: a T-22 envelope identifies a full export on its own; the
            // section check below stays for pre-envelope payloads.
            let enveloped = map
                .get("schema")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|schema| schema == crate::store::LIBRARY_EXPORT_SCHEMA);
            enveloped
                || (matches!(map.get("library"), Some(serde_json::Value::Array(_)))
                    && matches!(map.get("subscriptions"), Some(serde_json::Value::Array(_))))
        }
        _ => false,
    }
}

enum TextChunk {
    Json(String),
    Line(String),
}

fn brace_depth_delta(line: &str, in_string: &mut bool, escaped: &mut bool) -> i32 {
    let mut delta = 0i32;
    for c in line.chars() {
        if *escaped {
            *escaped = false;
            continue;
        }
        if *in_string {
            match c {
                '\\' => *escaped = true,
                '"' => *in_string = false,
                _ => {}
            }
        } else {
            match c {
                '"' => *in_string = true,
                '{' => delta += 1,
                '}' => delta -= 1,
                _ => {}
            }
        }
    }
    delta
}

fn split_brace_aware(body: &str) -> Vec<TextChunk> {
    let mut chunks = Vec::new();
    let mut block = String::new();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for raw_line in body.lines() {
        if depth == 0 {
            if raw_line.trim_start().starts_with('{') {
                block.clear();
                block.push_str(raw_line);
                depth = brace_depth_delta(raw_line, &mut in_string, &mut escaped).max(0);
                if depth == 0 {
                    chunks.push(TextChunk::Json(std::mem::take(&mut block)));
                    in_string = false;
                    escaped = false;
                }
            } else {
                chunks.push(TextChunk::Line(raw_line.to_string()));
            }
        } else {
            block.push('\n');
            block.push_str(raw_line);
            depth = (depth + brace_depth_delta(raw_line, &mut in_string, &mut escaped)).max(0);
            if depth == 0 {
                chunks.push(TextChunk::Json(std::mem::take(&mut block)));
                in_string = false;
                escaped = false;
            }
        }
    }
    if depth != 0 {
        for line in block.lines() {
            chunks.push(TextChunk::Line(line.to_string()));
        }
    }
    chunks
}

enum Recursed {
    Links(String),

    VmessBlob(String),

    JsonText(String),

    Opaque,
}

fn recurse_b64(line: &str) -> Recursed {
    let mut current = line.trim().to_string();
    for _ in 0..4 {
        let next = match decode_b64_utf8(&current) {
            Ok(text) => text,
            Err(_) => return Recursed::Opaque,
        };
        let trimmed = next.trim().to_string();
        if trimmed.contains("://") {
            return Recursed::Links(trimmed);
        }
        if looks_like_vmess_json(&trimmed) {
            return Recursed::VmessBlob(current);
        }
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            return Recursed::JsonText(trimmed);
        }
        if trimmed == current {
            return Recursed::Opaque;
        }
        current = trimmed;
    }
    Recursed::Opaque
}

fn import_json_chunk(chunk: &str, index: usize, out: &mut BatchResult) {
    match serde_json::from_str::<serde_json::Value>(chunk.trim()) {
        Ok(value) => {
            if value.is_object() && xray_terminal_kind(&value).is_some() {
                return;
            }
            if value.is_object() {
                let label = value
                    .get("tag")
                    .or_else(|| value.get("remark"))
                    .or_else(|| value.get("remarks"))
                    .or_else(|| value.get("ps"))
                    .or_else(|| value.get("name"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("json");
                match json_outbound(index, &value) {
                    Ok(config) => out.configs.push(config),
                    Err(error) => {
                        if value
                            .as_object()
                            .is_some_and(|obj| obj.contains_key("outbounds"))
                        {
                            let sub = import_json_value(&value);
                            out.configs.extend(sub.configs);
                            for mut failure in sub.failures {
                                failure.index = index;
                                out.failures.push(failure);
                            }
                        } else {
                            out.failures.push(ImportFailure::new(index, label, error));
                        }
                    }
                }
            } else {
                let sub = import_json_value(&value);
                out.configs.extend(sub.configs);
                for mut failure in sub.failures {
                    failure.index = index;
                    out.failures.push(failure);
                }
            }
        }
        Err(_) => out.failures.push(ImportFailure::new(
            index,
            "json",
            invalid_json("invalid JSON"),
        )),
    }
}

pub fn import_text(body: &str) -> BatchResult {
    let trimmed = strip_bom(body.trim()).trim();
    if trimmed.is_empty() {
        return BatchResult::fail(0, ImportErrorKind::Empty, "nothing to import");
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        match serde_json::from_str::<serde_json::Value>(trimmed) {
            Ok(value) => return import_json_value(&value),
            Err(_) => {
                if looks_like_wg_conf(trimmed) {
                    return import_wg_conf(trimmed, "wireguard");
                }
                if has_clash_proxies(trimmed) {
                    return import_clash_proxies(trimmed);
                }

                let chunks = split_brace_aware(trimmed);
                if chunks
                    .iter()
                    .any(|chunk| matches!(chunk, TextChunk::Json(_)))
                {
                    let mut out = BatchResult::default();
                    let mut index = 0;
                    for chunk in &chunks {
                        match chunk {
                            TextChunk::Json(text) => {
                                import_json_chunk(text, index, &mut out);
                                index += 1;
                            }
                            TextChunk::Line(line) => {
                                let line = strip_bom(line.trim());
                                if line.is_empty() {
                                    continue;
                                }
                                match parse_link(line) {
                                    Ok(config) => out.configs.push(config),
                                    Err(error) => out.failures.push(ImportFailure::new(
                                        index,
                                        redact_link(line),
                                        error,
                                    )),
                                }
                                index += 1;
                            }
                        }
                    }
                    dedupe_names(&mut out.configs);
                    return out;
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
    for chunk in split_brace_aware(body) {
        match chunk {
            TextChunk::Json(text) => {
                import_json_chunk(&text, index, &mut out);
                index += 1;
            }
            TextChunk::Line(raw_line) => {
                let line = strip_bom(raw_line.trim());
                if line.is_empty() {
                    continue;
                }

                if !line.contains("://") {
                    match recurse_b64(line) {
                        Recursed::Links(text) => {
                            for sub in text.lines() {
                                let sub = strip_bom(sub.trim());
                                if sub.is_empty() {
                                    continue;
                                }
                                match parse_link(sub) {
                                    Ok(config) => out.configs.push(config),
                                    Err(error) => out.failures.push(ImportFailure::new(
                                        index,
                                        redact_link(sub),
                                        error,
                                    )),
                                }
                                index += 1;
                            }
                            continue;
                        }
                        Recursed::VmessBlob(blob) => {
                            match parse_link(&format!("vmess://{blob}")) {
                                Ok(config) => out.configs.push(config),
                                Err(error) => out.failures.push(ImportFailure::new(
                                    index,
                                    redact_link(line),
                                    error,
                                )),
                            }
                            index += 1;
                            continue;
                        }
                        Recursed::JsonText(json) => {
                            match serde_json::from_str::<serde_json::Value>(&json) {
                                Ok(value) => {
                                    let sub = import_json_value(&value);
                                    out.configs.extend(sub.configs);
                                    for mut failure in sub.failures {
                                        failure.index = index;
                                        out.failures.push(failure);
                                    }
                                }
                                Err(_) => out.failures.push(ImportFailure::new(
                                    index,
                                    "json",
                                    invalid_json("invalid JSON"),
                                )),
                            }
                            index += 1;
                            continue;
                        }
                        Recursed::Opaque => {}
                    }
                }
                match parse_link(line) {
                    Ok(config) => out.configs.push(config),
                    Err(error) => {
                        out.failures
                            .push(ImportFailure::new(index, redact_link(line), error))
                    }
                }
                index += 1;
            }
        }
    }

    dedupe_names(&mut out.configs);
    out
}

pub fn import_file_content(file_name: &str, content: &str) -> BatchResult {
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
        imported_ids: Vec::new(),
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
    outcome.imported_ids = added.iter().map(|config| config.id.clone()).collect();
    Ok(outcome)
}

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

    dedupe_names(&mut out.configs);
    out
}

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
                &["uuid"] as &[&str],
                &["password"],
                &["auth"],
                &["private_key", "privatekey"],
                &["public_key", "publickey"],
                &["method"],
                &["username", "user"],
                &["token"],
            ]
            .iter()
            .filter_map(|keys| get(&map, keys))
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddSubRequest {
    pub url: String,
    pub group: String,
}

fn intake_scheme(text: &str) -> Option<&str> {
    let sep = text.find("://")?;
    let scheme = &text[..sep];
    if scheme.eq_ignore_ascii_case("throne") || scheme.eq_ignore_ascii_case("manyproxy") {
        Some(scheme)
    } else {
        None
    }
}

pub fn is_addsub_attempt(line: &str) -> bool {
    let trimmed = strip_bom(line.trim()).trim();
    let Some(sep) = trimmed.find("://") else {
        return false;
    };
    if intake_scheme(trimmed).is_none() {
        return false;
    }
    trimmed[sep + 3..]
        .split('/')
        .next()
        .unwrap_or("")
        .eq_ignore_ascii_case("addsub")
}

#[cfg(test)]
pub fn is_add_attempt(line: &str) -> bool {
    let trimmed = strip_bom(line.trim()).trim();
    let Some(sep) = trimmed.find("://") else {
        return false;
    };
    if intake_scheme(trimmed).is_none() {
        return false;
    }
    trimmed[sep + 3..]
        .split('/')
        .next()
        .unwrap_or("")
        .eq_ignore_ascii_case("add")
}

pub fn is_bare_http_url(text: &str) -> bool {
    let trimmed = strip_bom(text.trim()).trim();
    if trimmed.is_empty() || trimmed.contains(['\n', '\r']) {
        return false;
    }
    if trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let after_scheme = if trimmed
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
    {
        &trimmed[7..]
    } else if trimmed
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
    {
        &trimmed[8..]
    } else {
        return false;
    };
    let host = after_scheme.split(['/', '?', '#']).next().unwrap_or("");
    !host.trim().is_empty()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenWithKind {
    BareHttp(String),
    Batch(String),
}

pub fn classify_open_with(arg: &str) -> Option<OpenWithKind> {
    let trimmed = strip_bom(arg.trim()).trim();
    if trimmed.is_empty() || trimmed.contains(['\n', '\r']) {
        return None;
    }
    if is_bare_http_url(trimmed) {
        return Some(OpenWithKind::BareHttp(trimmed.to_string()));
    }
    let sep = trimmed.find("://")?;
    if !valid_scheme_chars(&trimmed[..sep]) {
        return None;
    }
    if intake_scheme(trimmed).is_some() {
        return Some(OpenWithKind::Batch(trimmed.to_string()));
    }

    Some(OpenWithKind::Batch(trimmed.to_string()))
}

pub const MAX_IMPORT_FILE_BYTES: u64 = 50_000_000;

pub fn import_file_size_ok(len: u64) -> bool {
    len <= MAX_IMPORT_FILE_BYTES
}

pub fn reject_file_bytes(bytes: &[u8]) -> Option<&'static str> {
    const PNG: &[u8] = &[0x89, b'P', b'N', b'G'];
    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF];
    const GIF87: &[u8] = b"GIF87a";
    const GIF89: &[u8] = b"GIF89a";
    const BMP: &[u8] = b"BM";
    let is_webp = bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP";
    if bytes.starts_with(PNG)
        || bytes.starts_with(JPEG)
        || bytes.starts_with(GIF87)
        || bytes.starts_with(GIF89)
        || bytes.starts_with(BMP)
        || is_webp
    {
        return Some("image file (paste the link text instead — this build has no image/QR scan)");
    }
    if bytes.contains(&0) {
        return Some("binary file (NUL byte — not config text)");
    }
    if std::str::from_utf8(bytes).is_err() {
        return Some("file is not UTF-8 text");
    }
    None
}

pub fn parse_addsub_link(line: &str) -> Result<AddSubRequest, ImportError> {
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
    let text = strip_bom(line.trim()).trim();
    let sep = text
        .find("://")
        .ok_or_else(|| unknown_scheme("not a share link (missing '://')"))?;
    if intake_scheme(text).is_none() {
        return Err(unknown_scheme(format!("unknown scheme '{}'", &text[..sep])));
    }
    let rest = &text[sep + 3..];
    let (command, payload) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, ""),
    };
    if !command.eq_ignore_ascii_case("addsub") {
        return Err(unknown_scheme(format!(
            "unknown throne command '{}'",
            cap_echo(command)
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

pub fn import_subscription_body(body: &str) -> BatchResult {
    let trimmed = strip_bom(body.trim()).trim();
    if trimmed.is_empty() {
        return BatchResult::fail(0, ImportErrorKind::Empty, "nothing to import");
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
            return import_json_value(&value);
        }
        if looks_like_wg_conf(trimmed) {
            return import_wg_conf(trimmed, "wireguard");
        }
        let has_wg_candidate = trimmed.lines().any(|raw_line| {
            let line = raw_line.trim();
            !line.is_empty() && !line.contains("://") && is_wg_section_line(line)
        });
        if !has_wg_candidate {
            return BatchResult::fail(0, ImportErrorKind::InvalidJson, "invalid JSON");
        }
    }
    if looks_like_wg_conf(trimmed) {
        return import_wg_conf(trimmed, "wireguard");
    }
    if has_clash_proxies(trimmed) {
        return import_clash_proxies(trimmed);
    }
    if let Some(mixed) = merge_wg_segments(trimmed) {
        return mixed;
    }
    let lines: Vec<&str> = trimmed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.len() > 1 {
        let compact: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
        if let Ok(text) = decode_b64_utf8(&compact) {
            let text = text.trim();
            if text.contains("://") {
                return import_text(text);
            }
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
                if value.is_array() || value.is_object() {
                    if looks_like_vmess_json(text) {
                        return import_text(&format!("vmess://{compact}"));
                    }
                    return import_json_value(&value);
                }
            }
        }
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

    match decode_b64_utf8(single) {
        Ok(text) if text.trim().contains("://") => import_text(&text),

        Ok(text) if looks_like_vmess_json(&text) => import_text(&format!("vmess://{single}")),

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

            _ => import_text(body),
        },
    }
}

fn is_wg_section_line(line: &str) -> bool {
    match line
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    {
        Some(name) => {
            name.trim().eq_ignore_ascii_case("interface")
                || name.trim().eq_ignore_ascii_case("peer")
        }
        None => false,
    }
}

fn line_absorbs_into_wg_run(line: &str) -> bool {
    if line.contains("://") || line.starts_with('{') {
        return false;
    }
    line.starts_with('[') || line.starts_with('#') || line.starts_with(';') || line.contains('=')
}

fn merge_wg_segments(trimmed: &str) -> Option<BatchResult> {
    let opens_wg_somewhere = trimmed.lines().any(|raw_line| {
        let line = raw_line.trim();
        !line.is_empty() && !line.contains("://") && is_wg_section_line(line)
    });
    if !opens_wg_somewhere {
        return None;
    }
    enum Segment {
        Wg(String),
        Text(String),
    }
    let mut segments: Vec<Segment> = Vec::new();
    let mut text_run = String::new();
    let mut wg_run = String::new();
    let mut in_wg = false;
    fn flush_wg(segments: &mut Vec<Segment>, wg_run: &mut String, in_wg: &mut bool) {
        if *in_wg {
            segments.push(Segment::Wg(std::mem::take(wg_run)));
            *in_wg = false;
        }
    }
    for raw_line in trimmed.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            if in_wg {
                wg_run.push('\n');
            }
            continue;
        }
        if !in_wg && !line.contains("://") && is_wg_section_line(line) {
            if !text_run.trim().is_empty() {
                segments.push(Segment::Text(std::mem::take(&mut text_run)));
            }
            wg_run.push_str(line);
            in_wg = true;
            continue;
        }
        if in_wg && line_absorbs_into_wg_run(line) {
            wg_run.push('\n');
            wg_run.push_str(line);
        } else {
            flush_wg(&mut segments, &mut wg_run, &mut in_wg);
            text_run.push_str(line);
            text_run.push('\n');
        }
    }
    flush_wg(&mut segments, &mut wg_run, &mut in_wg);
    if !text_run.trim().is_empty() {
        segments.push(Segment::Text(std::mem::take(&mut text_run)));
    }
    if !segments
        .iter()
        .any(|segment| matches!(segment, Segment::Wg(_)))
    {
        return None;
    }
    let mut out = BatchResult::default();
    let mut index = 0usize;
    for segment in segments {
        match segment {
            Segment::Wg(text) => {
                let sub = import_wg_conf(&text, "wireguard");
                let produced = sub.configs.len() + sub.failures.len();
                for mut failure in sub.failures {
                    failure.index += index;
                    out.failures.push(failure);
                }
                out.configs.extend(sub.configs);
                index += produced.max(1);
            }
            Segment::Text(text) => {
                let sub = import_text(&text);
                let produced = sub.configs.len() + sub.failures.len();
                for mut failure in sub.failures {
                    failure.index += index;
                    out.failures.push(failure);
                }
                out.configs.extend(sub.configs);
                index += produced;
            }
        }
    }
    dedupe_names(&mut out.configs);
    Some(out)
}

fn is_proxies_header(line: &str) -> bool {
    line == "proxies:"
        || line
            .strip_prefix("proxies:")
            .is_some_and(|rest| rest.starts_with([' ', '\t']) || rest.starts_with('#'))
}

fn has_clash_proxies(trimmed: &str) -> bool {
    trimmed
        .lines()
        .any(|raw_line| is_proxies_header(raw_line.trim()))
}

fn import_clash_proxies(content: &str) -> BatchResult {
    fn clash_type(raw: &str) -> Result<String, ImportError> {
        let lower = raw.trim().to_ascii_lowercase();

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

            "awg" | "amnezia-wg" => Ok("amneziawg".to_string()),

            "hy2" | "hysteria2+realm" => Ok("hysteria2".to_string()),
            "wg" => Ok("wireguard".to_string()),
            "naive+https" | "naive_https" => Ok("naive".to_string()),

            "shadow-tls" | "shadow_tls" => Ok("shadowtls".to_string()),

            "xray" | "xray-vless" | "xray_vless" | "vless-xray" => Ok("xray".to_string()),

            "xhttp" => Ok("xray".to_string()),
            "mieru" => Ok("mieru".to_string()),
            "juicity" => Ok("juicity".to_string()),
            "trusttunnel" => Ok("trusttunnel".to_string()),
            "custom"
            | "custom-out"
            | "custom-outbound"
            | "custom_outbound"
            | "custom-outbound-json" => Ok("custom-outbound".to_string()),
            "custom-config" | "custom_config" | "custom-full" | "custom_full"
            | "custom-full-config" => Ok("custom-config".to_string()),
            "extra" | "extra-core" | "extra_core" | "extracore" => Ok("extra-core".to_string()),
            "chain" | "chained" | "chain-outbound" | "chain_outbound" => Ok("chain".to_string()),

            "direct" => Ok("direct".to_string()),

            "openvpn" | "masque" | "tailscale" | "openconnect" | "autoselector" => {
                Err(unknown_scheme(format!(
                    "unsupported clash type '{lower}' ({})",
                    unsupported_protocol_guidance(&lower).message,
                )))
            }
            "" => Err(invalid_json("clash entry is missing \"type\"")),

            _ => Err(unknown_scheme(format!(
                "unknown clash type '{}'",
                raw.trim().chars().take(64).collect::<String>()
            ))),
        }
    }
    fn clash_value(raw: &str) -> String {
        let trimmed = raw.trim();
        if trimmed.len() >= 2
            && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
                || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
        {
            return trimmed[1..trimmed.len() - 1].to_string();
        }

        let bytes = trimmed.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'#' && i > 0 && (bytes[i - 1] == b' ' || bytes[i - 1] == b'\t') {
                return trimmed[..i].trim_end().to_string();
            }
            i += 1;
        }
        trimmed.to_string()
    }
    fn clash_nested_key(full: &str) -> Option<&'static str> {
        match full {
            "ws-opts.path" => Some("path"),
            "ws-opts.headers.host" => Some("host"),
            "grpc-opts.grpc-service-name" => Some("service_name"),
            "reality-opts.public-key" => Some("pbk"),
            "reality-opts.short-id" => Some("sid"),
            _ => None,
        }
    }
    fn clash_field(fields: &mut Vec<(String, String)>, key: &str, value_raw: &str) {
        if key == "<<" || value_raw.trim_start().starts_with('*') {
            fields.push(("__anchor__".to_string(), clash_value(value_raw)));
            return;
        }
        fields.push((key.to_ascii_lowercase(), clash_value(value_raw)));
    }
    fn clash_nested_field(fields: &mut Vec<(String, String)>, full: &str, value_raw: &str) {
        if full.ends_with("<<") || value_raw.trim_start().starts_with('*') {
            fields.push(("__anchor__".to_string(), clash_value(value_raw)));
            return;
        }
        if let Some(mapped) = clash_nested_key(full) {
            fields.push((mapped.to_string(), clash_value(value_raw)));
        }
    }
    let mut out = BatchResult::default();
    let lines: Vec<&str> = content.lines().collect();
    let start = lines
        .iter()
        .position(|raw_line| is_proxies_header(raw_line.trim()))
        .unwrap_or(0);

    struct Pending {
        fields: Vec<(String, String)>,
        entry_indent: usize,
        key_indent: Option<usize>,
        nested: Vec<(usize, String)>,
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
            break;
        }
        let line = raw_line.trim();
        if line.starts_with("- ") || line == "-" {
            flush(&mut current, &mut entries);
            let after = line.strip_prefix("- ").unwrap_or("").trim();
            let mut fields = Vec::new();
            if !after.is_empty() {
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
                        clash_field(&mut fields, key, value);
                    }
                }
            }
            current = Some(Pending {
                fields,
                entry_indent: indent,
                key_indent: None,
                nested: Vec::new(),
            });
        } else if let Some(pending) = current.as_mut() {
            if indent <= pending.entry_indent {
                flush(&mut current, &mut entries);
                continue;
            }
            let Some(i) = line.find(':') else {
                continue;
            };
            let (key, value) = (line[..i].trim(), line[i + 1..].trim());
            if key.is_empty() || key.contains(' ') {
                continue;
            }
            if pending.key_indent.is_none() {
                pending.key_indent = Some(indent);
            }
            while pending.nested.last().is_some_and(|(top, _)| *top >= indent) {
                pending.nested.pop();
            }
            if value.is_empty() {
                pending.nested.push((indent, key.to_ascii_lowercase()));
                continue;
            }
            if pending.nested.is_empty() {
                clash_field(&mut pending.fields, key, value);
            } else {
                let prefix = pending
                    .nested
                    .iter()
                    .map(|(_, key)| key.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                let full = format!("{prefix}.{}", key.to_ascii_lowercase());
                clash_nested_field(&mut pending.fields, &full, value);
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
            if fields.iter().any(|(key, _)| key == "__anchor__") {
                return Err(invalid_json(
                    "clash anchors/aliases (\"*\", \"<<\") are not supported",
                ));
            }
            if fields.iter().any(|(key, _)| key == "__flow__") {
                return Err(invalid_json("clash flow mappings are not supported"));
            }
            let raw_type = fields
                .iter()
                .find(|(key, _)| key == "type")
                .map(|(_, value)| value.clone())
                .unwrap_or_default();
            let protocol = clash_type(&raw_type)?;

            if protocol == "direct" {
                let name = cap_name(
                    fields
                        .iter()
                        .find(|(key, _)| key == "name")
                        .map(|(_, value)| value.clone())
                        .filter(|name| !name.trim().is_empty())
                        .unwrap_or_else(|| "direct".to_string()),
                );
                let mut params = Params::new();
                params.insert(
                    "type".to_string(),
                    serde_json::Value::String(protocol.clone()),
                );
                return Ok(ImportedConfig {
                    name,
                    protocol,
                    params: finish(params),
                    host: String::new(),
                    port: 0,
                });
            }
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
            let port = parse_port(&port_raw).map_err(|_| {
                bad_port(format!(
                    "clash entry has a bad \"port\" ({})",
                    port_raw.trim().chars().take(64).collect::<String>()
                ))
            })?;
            let name = cap_name(
                fields
                    .iter()
                    .find(|(key, _)| key == "name")
                    .map(|(_, value)| value.clone())
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or_else(|| format!("{protocol}-{host}:{port}")),
            );
            let mut params = base_params(&protocol, &host, port);

            let mut security_tls = false;
            for (key, value) in fields {
                if key == "name" || key == "type" || key == "server" || key == "port" {
                    continue;
                }

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
                        "1" | "true" | "yes" | "on" | "tls"
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

pub fn export_config_link(_protocol: &str, _name: &str, _params_json: &str) -> Option<String> {
    None
}

pub fn export_config_json_link(protocol: &str, name: &str, params_json: &str) -> String {
    use base64::Engine as _;
    let mut body = serde_json::from_str::<serde_json::Value>(params_json)
        .unwrap_or_else(|_| serde_json::json!({"type": protocol, "name": name, "unparsed": true}));
    if let Some(obj) = body.as_object_mut() {
        obj.entry("type".to_string())
            .or_insert_with(|| serde_json::Value::String(protocol.to_string()));
        obj.entry("name".to_string())
            .or_insert_with(|| serde_json::Value::String(name.to_string()));
    }
    let payload = base64::engine::general_purpose::STANDARD.encode(body.to_string().as_bytes());
    format!("manyproxy://add/{payload}")
}

pub fn share_link_for_config(protocol: &str, name: &str, params_json: &str) -> String {
    export_config_link(protocol, name, params_json)
        .unwrap_or_else(|| export_config_json_link(protocol, name, params_json))
}

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
            Some(i) => {
                text.replace_range(after..after + i, "…");
            }

            None => {
                let scheme = text[..scheme_end].to_ascii_lowercase();
                if ["vmess", "v2rayn", "ss", "shadowsocks", "throne", "json"]
                    .contains(&scheme.as_str())
                {
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

    const UUID: &str = "11111111-2222-4333-8444-555555555555";

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
    fn t72_is_library_export_recognizes_envelope() {
        use crate::store::{export_payload_full, seal_library_export};
        let payload = export_payload_full(&[], &[], &[]);
        let enveloped = {
            let value: serde_json::Value = serde_json::from_str(&payload).expect("payload parses");
            seal_library_export(&value)
        };
        assert!(
            enveloped.starts_with("{\"schema\":\"manyproxy-library\""),
            "envelope leads the export: {enveloped}"
        );
        assert!(
            is_library_export(&enveloped),
            "enveloped export routes to restore"
        );
        assert!(
            is_library_export(&format!("\u{FEFF}{enveloped}")),
            "BOM-tolerant intake"
        );
        assert!(
            is_library_export(
                "{\"schema\":\"manyproxy-library\",\"schema_version\":1,\"library\":[],\"subscriptions\":[]}"
            ),
            "the envelope first key alone identifies a full export"
        );
        assert!(
            is_library_export(
                "{\"outbounds\":[],\"library\":[],\"subscriptions\":[],\"tombstones\":[]}"
            ),
            "envelope-free payloads still route, idempotently"
        );
        assert!(
            !is_library_export("{\"schema\":\"thrn-archive\",\"library\":[]}"),
            "foreign schema without sections stays import"
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

        let pasted = import_text(conf);
        assert!(
            pasted.failures.is_empty(),
            "failures: {:?}",
            pasted.failures
        );
        assert_eq!(pasted.configs.len(), 1);
        assert_eq!(pasted.configs[0].protocol, "amneziawg");
        assert_eq!(params_of(&pasted.configs[0])["jmax"], "1000");

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
        let endpoints = generated.value()["endpoints"]
            .as_array()
            .expect("endpoints array")
            .clone();
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0]["type"], serde_json::json!("wireguard"));

        assert_eq!(endpoints[0]["jc"], serde_json::json!(5));
        assert_eq!(endpoints[0]["jmin"], serde_json::json!(30));
        assert_eq!(endpoints[0]["jmax"], serde_json::json!(1000));
    }

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

    #[test]
    fn failure_bad_base64() {
        let error = parse_link("vmess://!!!not-base64!!!").expect_err("bad base64 fails");
        assert_eq!(error.kind, ImportErrorKind::BadBase64);
    }

    #[test]
    fn failure_wrong_alphabet_base64() {
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
    fn t10_share_link_fallback_never_empty_and_round_trips() {
        let params = serde_json::json!({
            "type": "vless", "server": "example.com", "server_port": 443,
            "uuid": "11111111-1111-1111-1111-111111111111",
        })
        .to_string();
        let link = share_link_for_config("vless", "row", &params);
        assert!(!link.is_empty(), "fallback never empty");
        assert!(link.starts_with("manyproxy://add/"), "{link}");
        let back = parse_link(&link).expect("share link re-imports");
        assert_eq!(back.protocol, "vless");
        assert_eq!(back.host, "example.com");
        assert_eq!(back.port, 443);

        for protocol in ["mieru", "juicity", "direct", "chain"] {
            let link = share_link_for_config(protocol, "row", &params);
            assert!(!link.is_empty(), "{protocol} shares non-empty");
        }

        let link = share_link_for_config("vless", "row", "{not json");
        assert!(!link.is_empty(), "corrupt params still share");
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
        let bare = parse_link(&format!("vless://{UUID}@::1:443#v6bare"))
            .expect("bare IPv6 host with numeric port parses");
        assert_eq!(bare.host, "::1");
        assert_eq!(bare.port, 443);
        let bare_no_port =
            parse_link("hysteria2://secret@2001:db8::1#v6noport").expect("bare IPv6 host parses");
        assert_eq!(bare_no_port.host, "2001:db8::1");
        assert_eq!(bare_no_port.port, 443);
        let error = parse_link(&format!("vless://{UUID}@foo:bar:443"))
            .expect_err("non-IP stray colons still fail");
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
        let error = parse_link("vmess:////4=").expect_err("non-UTF8 fails");
        assert_eq!(error.kind, ImportErrorKind::NonUtf8);
    }

    #[test]
    fn failure_reasons_hide_credentials() {
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

    #[test]
    fn wg_conf_comment_with_url_still_detected() {
        let conf = "# see https://example.com/docs\n[Interface]\nPrivateKey = cHJpdmF0ZWtleQ\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\n";
        let out = import_text(conf);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1);
        assert_eq!(out.configs[0].protocol, "wireguard");
    }

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

    #[test]
    fn dotfile_conf_gets_sensible_stem() {
        let conf = "[Interface]\nPrivateKey = cHJpdmF0ZWtleQ\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\n";
        let out = import_file_content(".conf", conf);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1);
        assert_eq!(out.configs[0].name, "wireguard-p1");
    }

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

    #[test]
    fn commit_dedupes_on_widened_identity() {
        let mut store = Store::open_in_memory().expect("store opens");
        let first = import_text(&format!("vless://{UUID}@a.example.com:443#one"));
        let outcome = commit_batch(&mut store, first, "g").expect("commit works");
        assert_eq!(outcome.imported, 1);

        let upper = import_text(&format!("vless://{UUID}@A.EXAMPLE.COM:443#two"));
        assert_eq!(
            normalized_identity(&upper.configs[0].protocol, &upper.configs[0].params),
            normalized_identity("vless", &store.list().expect("list").remove(0).params),
            "case is not identity"
        );
        let outcome = commit_batch(&mut store, upper, "g").expect("recommit works");
        assert_eq!(outcome.imported, 0, "case variant adds nothing");
        assert_eq!(outcome.duplicates, 1);

        let flowed = import_text(&format!(
            "vless://{UUID}@a.example.com:443?flow=xtls-rprx-vision#three"
        ));
        let outcome = commit_batch(&mut store, flowed, "g").expect("recommit works");
        assert_eq!(outcome.imported, 1, "flow variant is a distinct node");
        assert_eq!(outcome.duplicates, 0);
        assert_eq!(store.list().expect("list works").len(), 2);

        let flowed_again = import_text(&format!(
            "vless://{UUID}@a.example.com:443?flow=xtls-rprx-vision#three"
        ));
        let outcome = commit_batch(&mut store, flowed_again, "g").expect("recommit works");
        assert_eq!(outcome.imported, 0, "exact re-import adds nothing");
        assert_eq!(outcome.duplicates, 1);
    }

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

    #[test]
    fn concatenated_conf_text_does_not_bleed_private_keys() {
        let merged = "[Interface]\nPrivateKey = QUJD\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\n[Interface]\nPrivateKey = WFl6\n\n[Peer]\nPublicKey = cHVia2V5Mg\nEndpoint = 203.0.113.2:51820\n";
        let out = import_text(merged);

        assert_eq!(out.configs.len(), 2, "failures: {:?}", out.failures);
        let params: Vec<serde_json::Value> = out
            .configs
            .iter()
            .map(|config| serde_json::from_str(&config.params).expect("params JSON"))
            .collect();
        assert_eq!(params[0]["private_key"], "QUJD");
        assert_eq!(params[1]["private_key"], "WFl6");
    }

    #[test]
    fn ticket24_non_ascii_addsub_no_panic() {
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
        assert_eq!(params_of(&config)["path"], "/extra-path");
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

    #[test]
    fn t42_hy2_realm_rejects_at_import_with_guidance() {
        let error = parse_link("hysteria2+realm://letmein@example.com:8443#r")
            .expect_err("realm variant rejects at import");
        assert_eq!(error.kind, ImportErrorKind::UnknownScheme);
        assert!(error.message.contains("realm"), "names realm: {error}");
    }

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

    #[test]
    fn t07_mieru_juicity_parse_to_disabled_rows() {
        for link in [
            "mieru://user:pass@example.com:1080#m",
            "juicity://uuid:pass@example.com:23142#j",
            "MIERU://user:pass@example.com:1080#m",
        ] {
            let config = parse_link(link).unwrap_or_else(|_| panic!("{link} parses"));
            assert!(
                ["mieru", "juicity"].contains(&config.protocol.as_str()),
                "kind kept: {link}"
            );
            assert_eq!(config.host, "example.com");
        }
    }

    #[test]
    fn t07_trusttunnel_and_mieru_query_forms() {
        let trust =
            parse_link("trusttunnel://alice:wonderland@example.com:443?sni=example.com#t-one")
                .expect("trusttunnel parses");
        assert_eq!(trust.protocol, "trusttunnel");
        assert_eq!(trust.host, "example.com");
        assert_eq!(trust.port, 443);
        assert_eq!(trust.name, "t-one");
        let params = params_of(&trust);
        assert_eq!(params["username"], "alice");
        assert_eq!(params["password"], "wonderland");
        assert_eq!(params["sni"], "example.com");

        let community = parse_link(
            "mieru://example.com:1080?username=john&password=secret&transport=tcp#m-comm",
        )
        .expect("community mieru parses");
        assert_eq!(community.protocol, "mieru");
        assert_eq!(community.host, "example.com");
        assert_eq!(community.port, 1080);
        let params = params_of(&community);
        assert_eq!(params["username"], "john");
        assert_eq!(params["password"], "secret");
        assert_eq!(params["transport"], "tcp");
    }

    #[test]
    fn t07_new_scheme_credential_gates() {
        for (link, wants) in [
            ("mieru://:pass@example.com:1080#m", "username"),
            ("mieru://user:@example.com:1080#m", "password"),
            ("mieru://example.com:1080#m", "username"),
            ("juicity://:pass@example.com:23142#j", "uuid"),
            ("juicity://uuid@example.com:23142#j", "password"),
            ("trusttunnel://:pw@example.com:443#t", "username"),
            ("trusttunnel://user@example.com:443#t", "password"),
        ] {
            let error = parse_link(link).expect_err(&format!("{link} fails"));
            assert_eq!(
                error.kind,
                ImportErrorKind::MissingCredential,
                "link {link}"
            );
            assert!(error.message.contains(wants), "names {wants}: {error}");
        }

        let error = parse_link("trusttunnel://alice:wonderland@example.com:99999#t")
            .expect_err("bad port fails");
        assert_eq!(error.kind, ImportErrorKind::BadPort);
        assert!(
            !error.message.contains("wonderland"),
            "no secret echo: {error}"
        );
    }

    #[test]
    fn t07_json_link_parses_and_names() {
        let body = serde_json::json!({
            "type": "vless", "tag": "j-tag",
            "server": "a.example.com", "server_port": 443, "uuid": UUID,
        });
        let link = format!("json://{}#frag", b64(&body.to_string()));
        let config = parse_link(&link).expect("json link parses");
        assert_eq!(config.protocol, "vless");
        assert_eq!(config.host, "a.example.com");
        assert_eq!(config.name, "frag", "fragment wins over the tag");

        let bare = format!("json://{}", b64(&body.to_string()));
        let config = parse_link(&bare).expect("tag survives without fragment");
        assert_eq!(config.name, "j-tag");

        let error = parse_link("json://!!!").expect_err("bad base64 fails");
        assert_eq!(error.kind, ImportErrorKind::BadBase64);
        let error = parse_link(&format!("json://{}", b64("[1,2]"))).expect_err("non-object fails");
        assert_eq!(error.kind, ImportErrorKind::InvalidJson);

        let thin =
            serde_json::json!({"type": "vless", "server": "a.example.com", "server_port": 443});
        let error = parse_link(&format!("json://{}", b64(&thin.to_string())))
            .expect_err("missing uuid fails");
        assert_eq!(error.kind, ImportErrorKind::MissingCredential);

        let freedom = serde_json::json!({"protocol": "freedom", "tag": "direct"});
        let error = parse_link(&format!("json://{}", b64(&freedom.to_string())))
            .expect_err("freedom json link drops");
        assert_eq!(error.kind, ImportErrorKind::UnknownScheme);
        assert!(error.message.contains("freedom"), "names it: {error}");
    }

    #[test]
    fn t07_shadowtls_json_type_parses() {
        let body = serde_json::json!([{
            "type": "shadowtls", "tag": "st",
            "server": "a.example.com", "server_port": 443,
            "password": "pw", "version": "3", "sni": "a.example.com",
        }]);
        let out = import_text(&body.to_string());
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1);
        assert_eq!(out.configs[0].protocol, "shadowtls");
    }

    #[test]
    fn t07_new_types_json_and_clash() {
        let body = serde_json::json!([
            {"type": "mieru", "tag": "m", "server": "a.example.com",
             "server_port": 1080, "username": "u", "password": "p"},
            {"type": "juicity", "tag": "j", "server": "b.example.com",
             "server_port": 23142, "uuid": UUID, "password": "q"},
            {"type": "trusttunnel", "tag": "t", "server": "c.example.com",
             "server_port": 443, "username": "u", "password": "q"},
            {"type": "mieru", "tag": "thin", "server": "d.example.com",
             "server_port": 1080, "username": "u"},
        ]);
        let out = import_text(&body.to_string());
        assert_eq!(out.configs.len(), 3, "three rows land: {out:?}");
        assert_eq!(out.failures.len(), 1, "thin mieru fails: {out:?}");
        assert_eq!(out.failures[0].code, ImportErrorKind::MissingCredential);
        assert_eq!(out.configs[0].protocol, "mieru");
        assert_eq!(out.configs[1].protocol, "juicity");
        assert_eq!(out.configs[2].protocol, "trusttunnel");

        let clash = "proxies:\n\
             \x20 - name: m1\n    type: mieru\n    server: a.example.com\n    port: 1080\n    username: u\n    password: p\n\
             \x20 - name: j1\n    type: juicity\n    server: b.example.com\n    port: 23142\n    uuid: 11111111-2222-4333-8444-555555555555\n    password: q\n\
             \x20 - name: x1\n    type: xhttp\n    server: c.example.com\n    port: 443\n    uuid: 11111111-2222-4333-8444-555555555555\n";
        let out = import_text(clash);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 3);
        assert_eq!(out.configs[0].protocol, "mieru");
        assert_eq!(out.configs[1].protocol, "juicity");
        assert_eq!(out.configs[2].protocol, "xray", "xhttp folds to xray");
    }

    #[test]
    fn t07_xray_terminals_drop_from_batches() {
        let body = serde_json::json!({"outbounds": [
            {"type": "vless", "tag": "keep", "server": "a.example.com",
             "server_port": 443, "uuid": UUID},
            {"protocol": "freedom", "tag": "direct"},
            {"protocol": "blackhole", "tag": "block"},
        ]});
        let out = import_text(&body.to_string());
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1);
        assert_eq!(out.configs[0].name, "keep");

        let freedom = serde_json::json!({"protocol": "freedom", "tag": "direct"});
        let link = format!("throne://add/{}", b64(&freedom.to_string()));
        let error = parse_link(&link).expect_err("freedom add drops loud");
        assert_eq!(error.kind, ImportErrorKind::UnknownScheme);
        assert!(error.message.contains("freedom"), "names it: {error}");
    }

    #[test]
    fn t07_brace_aware_multiline_split() {
        let two = "{\"type\":\"trojan\",\"server\":\"a.example.com\",\"server_port\":443,\"password\":\"p\",\"tag\":\"one\"}\n\
                   {\"type\":\"trojan\",\"server\":\"b.example.com\",\"server_port\":443,\"password\":\"p\",\"tag\":\"two\"}";
        let out = import_text(two);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 2);
        assert_eq!(out.configs[0].name, "one");
        assert_eq!(out.configs[1].name, "two");

        let pretty = "{\n  \"type\": \"trojan\",\n  \"server\": \"c.example.com\",\n  \"server_port\": 443,\n  \"password\": \"p\",\n  \"tag\": \"pretty\"\n}";
        let out = import_text(pretty);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1);
        assert_eq!(out.configs[0].name, "pretty");

        let mixed = "trojan://p@d.example.com:443#d\n\
                     {\"type\":\"trojan\",\"server\":\"e.example.com\",\"server_port\":443,\"password\":\"p\",\"tag\":\"mid\"}\n\
                     trojan://p@f.example.com:443#f";
        let out = import_text(mixed);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 3);

        let broken = "{\"type\": \"trojan\",\n\"server\": \"g.example.com\"";
        let out = import_text(broken);
        assert!(out.configs.is_empty(), "nothing stored: {out:?}");
        assert_eq!(out.failures.len(), 1);
    }

    #[test]
    fn t07_base64_recurse() {
        let link = format!("vless://{UUID}@a.example.com:443#deep");
        let double = b64(&b64(&link));
        let out = import_text(&double);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1);
        assert_eq!(out.configs[0].name, "deep");

        let triple = b64(&double);
        let out = import_text(&triple);
        assert_eq!(out.configs.len(), 1, "three layers decode: {out:?}");

        let two_links = concat!(
            "trojan://p@h.example.com:443#h\n",
            "ss://YWVzLTEyOC1nY206dGVzdA@192.168.100.1:8888#i"
        );
        let nested = b64(&b64(two_links));
        let out = import_text(&nested);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 2);

        let out = import_text("not-a-link-at-all");
        assert!(out.configs.is_empty());
        assert_eq!(out.failures.len(), 1);
    }

    #[test]
    fn t07_open_with_classification() {
        assert_eq!(
            classify_open_with("https://example.com/sub?token=x"),
            Some(OpenWithKind::BareHttp(
                "https://example.com/sub?token=x".to_string()
            ))
        );
        assert_eq!(
            classify_open_with("manyproxy://add/eA=="),
            Some(OpenWithKind::Batch("manyproxy://add/eA==".to_string()))
        );
        assert_eq!(
            classify_open_with("manyproxy://addsub/eA=="),
            Some(OpenWithKind::Batch("manyproxy://addsub/eA==".to_string()))
        );
        assert_eq!(
            classify_open_with("throne://addsub/eA=="),
            Some(OpenWithKind::Batch("throne://addsub/eA==".to_string()))
        );
        assert_eq!(
            classify_open_with("vless://u@a.example.com:443#x"),
            Some(OpenWithKind::Batch(
                "vless://u@a.example.com:443#x".to_string()
            ))
        );
        assert_eq!(classify_open_with("--tray"), None);
        assert_eq!(classify_open_with(""), None);
        assert_eq!(classify_open_with("C:\\configs\\nodes.txt"), None);
        assert!(is_add_attempt("manyproxy://add/eA=="));
        assert!(is_add_attempt("THRONE://ADD/eA=="));
        assert!(!is_add_attempt("throne://addsub/eA=="));
        assert!(!is_add_attempt("vless://u@a.example.com:443#x"));
        assert!(is_bare_http_url("http://example.com/feed"));
        assert!(!is_bare_http_url("http://"));
        assert!(!is_bare_http_url("https://example.com/a b"));
        assert!(!is_bare_http_url("vless://u@a.example.com:443#x"));
        assert!(!is_bare_http_url(
            "https://one.example.com\nhttps://two.example.com"
        ));
    }

    #[test]
    fn t07_manyproxy_addsub_parses_like_throne() {
        let payload = b64("https://example.com/feed#mygroup");
        let request = parse_addsub_link(&format!("manyproxy://addsub/{payload}")).expect("parses");
        assert_eq!(request.url, "https://example.com/feed");
        assert_eq!(request.group, "mygroup");
        assert!(is_addsub_attempt("manyproxy://addsub/eA=="));
        assert!(is_addsub_attempt("MANYPROXY://ADDSUB/eA=="));
        let error = parse_addsub_link("manyproxy://addsub/!!!").expect_err("bad b64 fails");
        assert_eq!(error.kind, ImportErrorKind::BadBase64);

        let add = serde_json::json!({
            "type": "trojan", "tag": "via-manyproxy",
            "server": "a.example.com", "server_port": 443, "password": "p",
        });
        let config = parse_link(&format!("manyproxy://add/{}", b64(&add.to_string())))
            .expect("manyproxy add parses");
        assert_eq!(config.protocol, "trojan");
        assert_eq!(config.name, "via-manyproxy");
    }

    #[test]
    fn t07_file_guards() {
        assert!(import_file_size_ok(MAX_IMPORT_FILE_BYTES));
        assert!(!import_file_size_ok(MAX_IMPORT_FILE_BYTES + 1));
        assert_eq!(reject_file_bytes(b"vless://u@a.example.com:443#x"), None);
        assert_eq!(reject_file_bytes(b""), None);
        for magic in [
            b"\x89PNG\r\n\x1a\n....".as_slice(),
            b"\xFF\xD8\xFF\xE0....".as_slice(),
            b"GIF89a....".as_slice(),
            b"BM....".as_slice(),
        ] {
            assert!(
                reject_file_bytes(magic).is_some_and(|reason| reason.contains("image")),
                "image magic rejects: {magic:?}"
            );
        }
        assert!(
            reject_file_bytes(b"ab\x00cd").is_some_and(|reason| reason.contains("NUL")),
            "NUL rejects"
        );
        assert!(
            reject_file_bytes(b"\xFF\xFE\xFD").is_some(),
            "non-UTF8 rejects"
        );
    }

    #[test]
    fn t07_new_rows_commit_disabled() {
        let body = format!(
            "mieru://john:secret@a.example.com:1080#m-one\n\
             juicity://{UUID}:s3cret@b.example.com:23142#j-one\n\
             trusttunnel://alice:wonderland@c.example.com:443#t-one"
        );
        let batch = import_text(&body);
        assert!(batch.failures.is_empty(), "failures: {:?}", batch.failures);
        assert_eq!(batch.configs.len(), 3);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let outcome = commit_batch(&mut store, batch, "").expect("batch commits");
        assert_eq!(outcome.imported, 3);
        assert!(outcome.failures.is_empty());
        let rows = store.list().expect("list works");
        assert_eq!(rows.len(), 3);
        for row in &rows {
            assert!(!row.enabled, "import lands disabled: {}", row.name);
        }
        let by_name = |name: &str| rows.iter().find(|row| row.name == name).expect(name);
        let row_params = |name: &str| {
            let row = by_name(name);
            serde_json::from_str::<serde_json::Value>(&row.params).expect("row params are JSON")
        };
        let mieru = by_name("m-one");
        assert_eq!(mieru.protocol, "mieru");
        assert_eq!(row_params("m-one")["server"], "a.example.com");
        assert_eq!(row_params("m-one")["server_port"], 1080);
        let juicity = by_name("j-one");
        assert_eq!(juicity.protocol, "juicity");
        assert_eq!(row_params("j-one")["server"], "b.example.com");
        assert_eq!(row_params("j-one")["server_port"], 23142);
        let trust = by_name("t-one");
        assert_eq!(trust.protocol, "trusttunnel");
        assert_eq!(row_params("t-one")["server"], "c.example.com");
        assert_eq!(row_params("t-one")["server_port"], 443);
    }

    #[test]
    fn t42_endpoint_protocols_reject_with_guidance() {
        for link in [
            "openvpn://user:pass@example.com:1194#o",
            "masque://user@example.com:443#m",
        ] {
            let error = parse_link(link).expect_err(&format!("{link} rejects"));
            assert_eq!(error.kind, ImportErrorKind::UnknownScheme, "link {link}");
            assert!(
                error.message.contains("endpoint") || error.message.contains("1.14.2"),
                "names the gap: {error}"
            );
        }
    }

    #[test]
    fn t08_exotic_fates_reject_direct_lands() {
        for link in [
            "tailscale://x@example.com:443#t",
            "openvpn://user:pass@example.com:1194#o",
            "openconnect://user:pass@example.com:443#o",
            "autoselector://x#auto",
            "MASQUE://user@example.com:443#m",
        ] {
            let error = parse_link(link).expect_err(&format!("{link} rejects"));
            assert_eq!(error.kind, ImportErrorKind::UnknownScheme, "link {link}");
            assert!(
                error.message.contains("custom-config"),
                "guidance names the supported row: {error:?}"
            );
        }

        let clash = import_text(
            "proxies:\n  - name: t\n    type: tailscale\n    server: a.example.com\n    port: 443\n",
        );
        assert!(
            clash.configs.is_empty() && clash.failures.len() == 1,
            "tailscale clash entry fails: {clash:?}"
        );
        assert!(
            clash.failures[0].reason.contains("custom-config"),
            "clash guidance names the row: {:?}",
            clash.failures[0]
        );

        let named = parse_link("direct://#office").expect("direct link parses");
        assert_eq!(named.protocol, "direct");
        assert_eq!(named.name, "office");
        assert_eq!(named.params, "{\"type\":\"direct\"}");
        let bare = parse_link("direct://").expect("bare direct parses");
        assert_eq!(bare.protocol, "direct");
        assert_eq!(bare.name, "direct");
        assert_eq!(bare.params, "{\"type\":\"direct\"}");

        let batch = import_text("{\"type\":\"direct\",\"tag\":\"d1\"}");
        assert!(batch.failures.is_empty(), "failures: {:?}", batch.failures);
        assert_eq!(batch.configs.len(), 1);
        assert_eq!(batch.configs[0].protocol, "direct");
        assert_eq!(batch.configs[0].name, "d1");

        let clash = import_text("proxies:\n  - name: d\n    type: direct\n");
        assert!(clash.failures.is_empty(), "failures: {:?}", clash.failures);
        assert_eq!(clash.configs.len(), 1);
        assert_eq!(clash.configs[0].protocol, "direct");
        assert_eq!(clash.configs[0].name, "d");
    }

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

    #[test]
    fn audit_naive_and_shadow_tls_schemes_accepted() {
        let naive = parse_link("naive://user:pass@example.com:8443#n").expect("naive parses");
        assert_eq!(naive.protocol, "naive");
        assert_eq!(naive.port, 8443);
        let shadow = parse_link("shadow_tls://shadowpw@example.com:443?sni=example.com#st")
            .expect("shadow_tls parses");
        assert_eq!(shadow.protocol, "shadowtls");
        assert!(VALID_LINK_SCHEMES.contains(&"naive"));
        assert!(VALID_LINK_SCHEMES.contains(&"shadow_tls"));
    }

    #[test]
    fn audit_wg_public_key_spelling_accepted() {
        for query in ["publickey=cHVia2V5", "public_key=cHVia2V5"] {
            let link = format!("wireguard://cHJpdmF0ZWtleQ@example.com:51820?{query}#wg");
            let config = parse_link(&link).expect("wg parses");
            assert_eq!(params_of(&config)["public_key"], "cHVia2V5");
        }
    }

    #[test]
    fn audit_whitespace_only_passwords_rejected() {
        for link in [
            "anytls://%20%20@example.com:8443#x",
            "tuic://11111111-2222-4333-8444-555555555555:%20@example.com:4433#x",
            "naive://user:%20@example.com:8443#x",
            "ssh://alice:%20@example.com:22#x",
            "socks://user:%20@example.com:1080#x",
        ] {
            let error = parse_link(link).expect_err(&format!("{link} fails"));
            assert_eq!(
                error.kind,
                ImportErrorKind::MissingCredential,
                "link {link}"
            );
        }
    }

    #[test]
    fn audit_unexpected_uri_paths_rejected_trailing_slash_ok() {
        for link in [
            "hysteria2://letmein@example.com:8443/extra?sni=example.com#x",
            "tuic://11111111-2222-4333-8444-555555555555:pw@example.com:4433/extra#x",
            "anytls://secret@example.com:8443/extra#x",
            "socks://user:pass@example.com:1080/extra#x",
        ] {
            let error = parse_link(link).expect_err(&format!("{link} fails"));
            assert_eq!(error.kind, ImportErrorKind::BadHost, "link {link}");
        }

        parse_link("snell://psk@example.com:8388/#x").expect("trailing slash ok");

        let config = parse_link(&format!("vless://{UUID}@a.example.com:443/ws-path#x"))
            .expect("vless path parses");
        assert_eq!(params_of(&config)["path"], "/ws-path");
    }

    #[test]
    fn audit_bom_stripped_in_export_addsub_predicates() {
        use crate::store::export_payload_full;
        let full = export_payload_full(&[], &[], &[]);
        assert!(is_library_export(&format!("\u{FEFF}{full}")));
        assert!(is_addsub_attempt("\u{FEFF}throne://addsub/aGk"));
        assert!(!is_addsub_attempt("\u{FEFF}not a link"));
    }

    #[test]
    fn audit_query_pair_count_capped() {
        let big = (0..2000)
            .map(|i| format!("k{i}=v{i}"))
            .collect::<Vec<_>>()
            .join("&");
        let link = format!("vless://{UUID}@a.example.com:443?{big}#x");
        let config = parse_link(&link).expect("huge query parses (capped)");
        let params = params_of(&config);
        assert!(
            params.as_object().expect("object").len() < 2000,
            "pairs capped"
        );
    }

    #[test]
    fn audit_clash_detector_allows_leading_whitespace() {
        let body = "# doc\n  proxies:\n    - name: w\n      type: trojan\n      server: t.example.com\n      port: 443\n      password: pw\n";
        assert!(has_clash_proxies(body.trim()), "indented header detected");
        let out = import_text(body);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1);
    }

    #[test]
    fn audit_display_names_strip_controls_rtl_ansi() {
        let esc = '\u{1B}';
        let config = parse_link(&format!(
            "vless://{UUID}@a.example.com:443#\u{202E}evil{esc}[31mred\u{0007}name"
        ))
        .expect("remark parses");
        assert!(!config.name.contains('\u{202E}'), "got {}", config.name);
        assert!(!config.name.contains(esc), "got {}", config.name);
        assert!(!config.name.contains('\u{7}'), "got {}", config.name);
        assert!(config.name.contains("evil"), "got {}", config.name);
        assert!(config.name.chars().count() <= 256);
    }

    #[test]
    fn audit_multi_file_dedupes_names_across_files() {
        let files = [
            ("a.txt", "trojan://pw@a.example.com:443#same"),
            ("b.txt", "trojan://pw@b.example.com:443#same"),
        ];
        let out = import_multi_file_contents(&files);
        assert_eq!(out.configs.len(), 2);
        assert_eq!(out.configs[0].name, "same");
        assert_eq!(out.configs[1].name, "same-2");
    }

    #[test]
    fn audit_legacy_ss_password_kept_verbatim() {
        let config = parse_link(&format!(
            "ss://{}",
            b64("aes-256-gcm:hunter%412@example.com:8388")
        ))
        .expect("legacy ss parses");
        assert_eq!(params_of(&config)["password"], "hunter%412");
    }

    #[test]
    fn audit_json_bracket_host_validated() {
        let body = serde_json::json!([{
            "type": "vless", "tag": "k", "server": "[::1",
            "server_port": 443, "uuid": UUID,
        }]);
        let out = import_text(&body.to_string());
        assert_eq!(out.configs.len(), 0);
        assert_eq!(out.failures[0].code, ImportErrorKind::BadHost);

        assert!(split_host_port("[exa mple]:443").is_err());
        assert!(split_host_port("[example\u{7}]:443").is_err());
        assert!(split_host_port("example\u{7}:443").is_err());
    }

    #[test]
    fn audit_scheme_and_port_echoes_capped() {
        let scheme = format!("{}://example.com:443", "g".repeat(500));
        let error = parse_link(&scheme).expect_err("bad scheme fails");
        assert!(
            !error.message.contains(&"g".repeat(100)),
            "scheme echo capped, len {}",
            error.message.chars().count()
        );
        let link = format!("vless://{UUID}@example.com:{}", "9".repeat(500));
        let error = parse_link(&link).expect_err("bad port fails");
        assert_eq!(error.kind, ImportErrorKind::BadPort);
        assert!(
            error.message.chars().count() < 200,
            "port echo capped: {}",
            error.message.chars().count()
        );
    }

    #[test]
    fn audit_ssh_identity_includes_user() {
        let alice = normalized_identity(
            "ssh",
            "{\"type\":\"ssh\",\"server\":\"a.example.com\",\"server_port\":22,\"user\":\"alice\",\"username\":\"alice\",\"password\":\"p\"}",
        );
        let bob = normalized_identity(
            "ssh",
            "{\"type\":\"ssh\",\"server\":\"a.example.com\",\"server_port\":22,\"user\":\"bob\",\"username\":\"bob\",\"password\":\"p\"}",
        );
        assert_ne!(alice, bob, "different ssh users are different rows");
    }

    #[test]
    fn audit_v2rayn_redacted_and_clash_comment_stripped() {
        let redacted = redact_link(&format!("v2rayn://{}", b64("{\"add\":\"h.example.com\"}")));
        assert!(
            !redacted.contains("h.example.com"),
            "v2rayn payload masked: {redacted}"
        );
        let body = "proxies:\n  - name: c\n    type: ss\n    server: s.example.com\n    port: 8388\n    cipher: aes-256-gcm\n    password: hunter2 #comment\n";
        let out = import_subscription_body(body);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(params_of(&out.configs[0])["password"], "hunter2");
    }

    fn row_of(config: &ImportedConfig, port: u16) -> crate::store::ProxyConfig {
        crate::store::ProxyConfig {
            id: format!("bug-row-{port}"),
            name: config.name.clone(),
            protocol: config.protocol.clone(),
            params: config.params.clone(),
            group: "bug-regression".to_string(),
            enabled: true,
            bind_ip: "127.0.0.1".to_string(),
            port,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        }
    }

    #[test]
    fn bug09_clash_nested_blocks_map_and_empty_parent_never_leaks() {
        let body = "proxies:\n  - name: ws-one\n    type: vless\n    server: 1.2.3.4\n    port: 443\n    uuid: <UUID>\n    network: ws\n    ws-opts:\n      path: /p\n      headers:\n        Host: h.example.com\n";
        let out = import_subscription_body(&body.replace("<UUID>", UUID));
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1);
        let config = &out.configs[0];
        let params = params_of(config);
        assert_eq!(params["path"], "/p", "ws-opts.path folds");
        assert_eq!(
            params["host"], "h.example.com",
            "ws-opts.headers.Host folds"
        );
        assert!(
            !params.as_object().expect("object").contains_key("ws-opts"),
            "empty parent key must not leak: {params}"
        );
        let generated = crate::engine::generate(std::slice::from_ref(&row_of(config, 10801)))
            .expect("clash ws row generates without a leaked parent");
        let node = generated.value()["outbounds"]
            .as_array()
            .expect("outbounds")
            .iter()
            .find(|node| node["type"] == serde_json::json!("vless"))
            .expect("vless egress")
            .clone();
        assert_eq!(node["transport"]["type"], serde_json::json!("ws"));
        assert_eq!(node["transport"]["path"], serde_json::json!("/p"));
        assert_eq!(
            node["transport"]["headers"]["Host"],
            serde_json::json!("h.example.com")
        );

        let mut leaky = row_of(config, 10899);
        leaky.params =
            r#"{"type":"vless","server":"1.2.3.4","server_port":443,"uuid":"<UUID>","ws-opts":""}"#
                .replace("<UUID>", UUID);
        let error = crate::engine::generate(std::slice::from_ref(&leaky))
            .expect_err("an unmapped parent key is a named error, not a silent leak");
        assert!(
            error.to_string().contains("ws-opts"),
            "the error names the key: {error}"
        );

        let grpc = "proxies:\n  - name: g-one\n    type: vless\n    server: 1.2.3.4\n    port: 443\n    uuid: <UUID>\n    network: grpc\n    grpc-opts:\n      grpc-service-name: exampleSvc\n";
        let out = import_subscription_body(&grpc.replace("<UUID>", UUID));
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(params_of(&out.configs[0])["service_name"], "exampleSvc");

        let reality = "proxies:\n  - name: r-one\n    type: vless\n    server: 1.2.3.4\n    port: 443\n    uuid: <UUID>\n    reality-opts:\n      public-key: A3oRvoNAnsCtWItpFcR4nQ0VyJjEJhRq8yx6shbXSVo\n      short-id: abcd\n";
        let out = import_subscription_body(&reality.replace("<UUID>", UUID));
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        let params = params_of(&out.configs[0]);
        assert_eq!(params["pbk"], "A3oRvoNAnsCtWItpFcR4nQ0VyJjEJhRq8yx6shbXSVo");
        assert_eq!(params["sid"], "abcd");
        assert!(
            !params
                .as_object()
                .expect("object")
                .contains_key("reality-opts"),
            "reality parent must not leak: {params}"
        );
    }

    #[test]
    fn bug10_hysteria_v1_query_auth_and_speed_keys_serve() {
        let config = parse_link(
            "hysteria://h.example.com:443?auth=pw&peer=h.example.com&upmbps=100&downmbps=50#n",
        )
        .expect("standard hysteria v1 link parses");
        assert_eq!(config.protocol, "hysteria");
        let params = params_of(&config);
        assert_eq!(params["auth"], "pw", "query auth lands");
        assert_eq!(params["up"], "100", "upmbps folds to up");
        assert_eq!(params["down"], "50", "downmbps folds to down");
        assert!(
            !params.as_object().expect("object").contains_key("upmbps"),
            "alias never leaks: {params}"
        );

        let userinfo = parse_link("hysteria://pw@h.example.com:443?upmbps=100&downmbps=50#n")
            .expect("userinfo dialect with speed keys parses");
        let params = params_of(&userinfo);
        assert_eq!(params["auth"], "pw");
        assert_eq!(params["up"], "100");
        assert_eq!(params["down"], "50");

        let generated = crate::engine::generate(std::slice::from_ref(&row_of(&config, 10802)))
            .expect("hy1 row generates with folded speed keys");
        let node = generated.value()["outbounds"]
            .as_array()
            .expect("outbounds")
            .iter()
            .find(|node| node["type"] == serde_json::json!("hysteria"))
            .expect("hysteria egress")
            .clone();
        assert_eq!(node["auth_str"], serde_json::json!("pw"));
        assert_eq!(node["up"], serde_json::json!("100"));
        assert_eq!(node["down"], serde_json::json!("50"));
    }

    #[test]
    fn bug11_wrapped_base64_body_decodes() {
        let inner =
            format!("vless://{UUID}@a.example.com:443#one\ntrojan://pw@b.example.com:443#two\n");
        let flat = b64_standard(&inner);
        let wrapped: String = flat
            .as_bytes()
            .chunks(76)
            .map(|chunk| std::str::from_utf8(chunk).expect("base64 is ascii"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(wrapped.lines().count() > 1, "fixture must wrap");
        let out = import_subscription_body(&wrapped);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 2);
        assert_eq!(out.configs[0].protocol, "vless");
        assert_eq!(out.configs[1].protocol, "trojan");
    }

    #[test]
    fn bug12_vmess_legacy_tls_truthy_and_ps_sanitized() {
        for tls in [
            serde_json::json!(1),
            serde_json::json!("1"),
            serde_json::json!("TLS"),
            serde_json::json!(true),
        ] {
            let json = serde_json::json!({"add": "h.example.com", "port": "443",
                "id": UUID, "tls": tls.clone()});
            let config = parse_link(&format!("vmess://{}", b64(&json.to_string())))
                .unwrap_or_else(|_| panic!("vmess tls {tls:?} parses"));
            assert_eq!(
                params_of(&config)["tls"],
                serde_json::json!(true),
                "{tls:?}"
            );
        }
        let off = serde_json::json!({"add": "h.example.com", "port": "443",
            "id": UUID, "tls": "false"});
        let config = parse_link(&format!("vmess://{}", b64(&off.to_string())))
            .expect("explicit false parses");
        assert_eq!(params_of(&config)["tls"], serde_json::json!(false));

        let evil = format!("\u{1b}[31mred\u{7} \u{202E}evil{}", "x".repeat(400));
        let json =
            serde_json::json!({"ps": evil, "add": "h.example.com", "port": "443", "id": UUID});
        let config =
            parse_link(&format!("vmess://{}", b64(&json.to_string()))).expect("hostile ps parses");
        assert!(
            !config.name.contains('\u{1b}'),
            "ANSI stripped: {:?}",
            config.name
        );
        assert!(
            !config.name.contains('\u{7}'),
            "BEL stripped: {:?}",
            config.name
        );
        assert!(
            !config.name.contains('\u{202E}'),
            "bidi stripped: {:?}",
            config.name
        );
        assert!(
            config.name.chars().count() <= 256,
            "capped at 256: {}",
            config.name.chars().count()
        );
    }

    #[test]
    fn bug29_wg_conf_repeated_keys_merge() {
        let conf = "[Interface]\nPrivateKey = cHJpdmF0ZWtleQ\nAddress = 10.0.0.2/32\nAddress = fd00::2/128\nDNS = 1.1.1.1\nDNS = 8.8.8.8\nMTU = 1420\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\nAllowedIPs = 0.0.0.0/0\nAllowedIPs = ::/0\n";
        let out = import_file_content("dup.conf", conf);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1);
        let params = params_of(&out.configs[0]);
        assert_eq!(params["address"], "10.0.0.2/32, fd00::2/128");
        assert_eq!(params["dns"], "1.1.1.1, 8.8.8.8");
        assert_eq!(params["allowed_ips"], "0.0.0.0/0, ::/0");
        assert_eq!(params["mtu"], "1420");

        let pasted = import_text(conf);
        assert!(
            pasted.failures.is_empty(),
            "failures: {:?}",
            pasted.failures
        );
        assert_eq!(
            params_of(&pasted.configs[0])["allowed_ips"],
            "0.0.0.0/0, ::/0"
        );
    }

    #[test]
    fn bug30_decoded_host_and_vmess_add_validation() {
        for host in [
            "a%20b.example.com",
            "x%00y.example.com",
            "a%09b.example.com",
        ] {
            let error = parse_link(&format!("vless://{UUID}@{host}:443#x"))
                .expect_err("decoded bad host fails");
            assert_eq!(error.kind, ImportErrorKind::BadHost, "host {host}");
        }

        let json = serde_json::json!({"add": "a b.example.com", "port": "443", "id": UUID});
        let error = parse_link(&format!("vmess://{}", b64(&json.to_string())))
            .expect_err("unbracketed vmess add with space fails");
        assert_eq!(error.kind, ImportErrorKind::BadHost);

        let json = serde_json::json!({"add": "[::1", "port": "443", "id": UUID});
        let error = parse_link(&format!("vmess://{}", b64(&json.to_string())))
            .expect_err("unclosed bracket still fails");
        assert_eq!(error.kind, ImportErrorKind::BadHost);

        let ok = parse_link(&format!("vless://{UUID}@exam%70le.com:443#x"))
            .expect("benign decode stays valid");
        assert_eq!(ok.host, "example.com");
        let vmess_ok = serde_json::json!({"add": "[2001:db8::1]", "port": "443", "id": UUID});
        let ok = parse_link(&format!("vmess://{}", b64(&vmess_ok.to_string())))
            .expect("bracketed v6 add stays valid");
        assert_eq!(ok.host, "2001:db8::1");
    }

    #[test]
    fn bug31_uri_path_decoded_and_normalized() {
        let config = parse_link(&format!("vless://{UUID}@a.example.com:443/ws%20x#n"))
            .expect("encoded uri path parses");
        assert_eq!(
            params_of(&config)["path"],
            "/ws x",
            "percent-decoded with slash"
        );

        let config = parse_link(&format!("vless://{UUID}@a.example.com:443/uri?path=%2Fq#n"))
            .expect("query path wins");
        assert_eq!(params_of(&config)["path"], "/q");

        let config = parse_link(&format!("vless://{UUID}@a.example.com:443?path=%2Fq#n"))
            .expect("absent uri path stays empty");
        assert_eq!(params_of(&config)["path"], "/q");

        let config = parse_link(&format!("vless://{UUID}@a.example.com:443/#n"))
            .expect("bare slash is an empty path");
        assert!(
            params_of(&config).get("path").is_none(),
            "no path param for a lone slash"
        );
    }

    #[test]
    fn bug31_uri_path_plus_matches_query_path() {
        let from_uri = parse_link(&format!("vless://{UUID}@a.example.com:443/a+b#n"))
            .expect("uri path with plus parses");
        // `+` is literal-and-space in both dialects: the query key/value path and
        // the URI path decode with the same flags.
        let from_query = parse_link(&format!("vless://{UUID}@a.example.com:443?path=%2Fa+b#n"))
            .expect("query path with plus parses");
        assert_eq!(
            params_of(&from_uri)["path"],
            params_of(&from_query)["path"],
            "URI path and query path= decode identically"
        );
        assert_eq!(params_of(&from_uri)["path"], "/a b");
    }

    #[test]
    fn bug32_socks_version_query_is_honored_or_rejected() {
        let v4 = parse_link("socks://user:pass@example.com:1080?version=4")
            .expect_err("socks v4 rejects loudly");
        assert_eq!(v4.kind, ImportErrorKind::UnknownScheme);
        assert!(v4.message.contains('4'), "names v4: {v4}");

        let v5 =
            parse_link("socks://user:pass@example.com:1080?version=5").expect("socks v5 parses");
        assert_eq!(params_of(&v5)["version"], "5");

        let odd = parse_link("socks://user:pass@example.com:1080?version=9")
            .expect_err("unsupported version rejects");
        assert_eq!(odd.kind, ImportErrorKind::UnknownScheme);

        let absent = parse_link("socks://user:pass@example.com:1080")
            .expect("versionless socks defaults to v5");
        assert_eq!(params_of(&absent)["version"], "5");
    }

    #[test]
    fn bug33_clash_tls_string_is_truthy() {
        let body = "proxies:\n  - name: t-one\n    type: trojan\n    server: t.example.com\n    port: 443\n    password: pw\n    tls: tls\n";
        let out = import_subscription_body(body);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(params_of(&out.configs[0])["security"], "tls");
    }

    #[test]
    fn bug34_clash_anchors_and_aliases_are_rejected() {
        let alias = "proxies:\n  - name: a-one\n    type: trojan\n    server: t.example.com\n    port: 443\n    password: pw\n    sni: *srv\n";
        let out = import_subscription_body(alias);
        assert!(out.configs.is_empty(), "configs: {:?}", out.configs);
        assert_eq!(out.failures.len(), 1);
        assert!(
            out.failures[0].reason.contains("anchor") || out.failures[0].reason.contains("alias"),
            "names the construct: {}",
            out.failures[0].reason
        );

        let merge = "proxies:\n  - name: a-two\n    type: trojan\n    server: t.example.com\n    port: 443\n    password: pw\n    <<: *base\n";
        let out = import_subscription_body(merge);
        assert!(out.configs.is_empty(), "configs: {:?}", out.configs);
        assert_eq!(out.failures.len(), 1);
        assert_eq!(out.failures[0].code, ImportErrorKind::InvalidJson);
    }

    const T71_WG_CONF: &str = "[Interface]\nPrivateKey = wgprivkey\nAddress = 10.0.0.2/32\nDNS = 1.1.1.1\n\n[Peer]\nPublicKey = wgpubkey\nEndpoint = 203.0.113.1:51820\nAllowedIPs = 0.0.0.0/0\n";

    #[test]
    fn t71_json_fragment_imports_std_b64() {
        let json_obj = serde_json::json!({
            "type": "vless", "tag": "j-tag",
            "server": "a.example.com", "server_port": 443, "uuid": UUID,
        });
        let body = format!("json://{}#line-name", b64(&json_obj.to_string()));
        let out = import_subscription_body(&body);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1);
        assert_eq!(out.configs[0].protocol, "vless");
        assert_eq!(out.configs[0].host, "a.example.com");
        assert_eq!(out.configs[0].port, 443);
        assert_eq!(
            out.configs[0].name, "line-name",
            "fragment wins over the tag"
        );
        assert_eq!(params_of(&out.configs[0])["uuid"], UUID);

        let bare = format!("json://{}", b64(&json_obj.to_string()));
        let out = import_subscription_body(&bare);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(
            out.configs[0].name, "j-tag",
            "tag survives without fragment"
        );
    }

    #[test]
    fn t71_json_fragment_imports_urlsafe_b64() {
        let json_obj = serde_json::json!({
            "type": "vless", "tag": "qq\u{00FF}",
            "server": "u.example.com", "server_port": 443, "uuid": UUID,
        });
        let payload = base64::engine::general_purpose::URL_SAFE.encode(json_obj.to_string());
        assert!(
            payload.contains('-') || payload.contains('_'),
            "fixture uses the urlsafe alphabet: {payload}"
        );
        let body = format!("json://{}#url-name", payload);
        let out = import_subscription_body(&body);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 1);
        assert_eq!(out.configs[0].name, "url-name");
        assert_eq!(out.configs[0].host, "u.example.com");

        let link_obj = serde_json::json!({
            "type": "vless", "tag": "j-tag",
            "server": "u.example.com", "server_port": 443, "uuid": UUID,
        });
        let body = format!(
            "vless://{UUID}@a.example.com:443#link-one\njson://{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(link_obj.to_string())
        );
        let out = import_subscription_body(&body);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 2, "link and fragment coexist");
        assert!(out.configs.iter().any(|c| c.name == "j-tag"));
        assert!(out.configs.iter().any(|c| c.name == "link-one"));
    }

    #[test]
    fn t71_json_fragment_failure_reports_line_index() {
        let body = format!(
            "vless://{UUID}@a.example.com:443#good-one\njson://!!!#broken\nvless://{UUID}@b.example.com:443#good-two",
        );
        let out = import_subscription_body(&body);
        assert_eq!(out.configs.len(), 2, "other lines still import");
        assert_eq!(out.failures.len(), 1, "never silent: {:?}", out.failures);
        assert_eq!(out.failures[0].index, 1, "line 2 (0-based item index)");
        assert_eq!(out.failures[0].code, ImportErrorKind::BadBase64);
        assert!(
            !out.failures[0].label.contains("!!!"),
            "label redacts: {}",
            out.failures[0].label
        );
        assert!(
            !out.failures[0].reason.contains("!!!"),
            "reason no secret echo: {}",
            out.failures[0].reason
        );
    }

    #[test]
    fn t71_wg_ini_in_body_imports_one_row_per_block() {
        let block_two = "\n[Interface]\nPrivateKey = wgprivkey2\nAddress = 10.0.1.2/32\n\n[Peer]\nPublicKey = wgpubkey2\nEndpoint = 203.0.113.2:51821\n";
        let body = format!(
            "{}\nvless://{UUID}@a.example.com:443#link-one{}",
            T71_WG_CONF, block_two
        );
        let out = import_subscription_body(&body);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(out.configs.len(), 3, "one row per INI block plus the link");
        let wg_rows: Vec<&ImportedConfig> = out
            .configs
            .iter()
            .filter(|c| c.protocol == "wireguard")
            .collect();
        assert_eq!(wg_rows.len(), 2);
        assert!(
            out.configs
                .iter()
                .any(|c| c.protocol == "vless" && c.name == "link-one"),
            "vless link row lands amid WG blocks"
        );
        let endpoints: Vec<String> = wg_rows
            .iter()
            .map(|c| {
                params_of(c)["server"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect();
        assert_eq!(
            endpoints,
            vec!["203.0.113.1".to_string(), "203.0.113.2".to_string()]
        );
        let keys: Vec<String> = wg_rows
            .iter()
            .map(|c| {
                params_of(c)["private_key"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect();
        assert_eq!(
            keys,
            vec!["wgprivkey".to_string(), "wgprivkey2".to_string()]
        );
        let names: Vec<&str> = wg_rows.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names[0], "wireguard-p1");
        assert_eq!(names[1], "wireguard-p1-2", "dedupe keeps names distinct");
    }

    #[test]
    fn t71_wg_ini_file_vs_body_params_parity() {
        let file_out = import_file_content("wg-vpn.conf", T71_WG_CONF);
        assert!(
            file_out.failures.is_empty(),
            "failures: {:?}",
            file_out.failures
        );
        assert_eq!(file_out.configs.len(), 1);
        assert_eq!(file_out.configs[0].protocol, "wireguard");
        assert_eq!(
            file_out.configs[0].name, "wg-vpn-p1",
            "files name by file stem"
        );
        let file_params = serde_json::from_str::<serde_json::Value>(&file_out.configs[0].params)
            .expect("params are json");
        assert_eq!(file_params["type"], "wireguard");
        assert_eq!(file_params["private_key"], "wgprivkey");
        assert_eq!(file_params["public_key"], "wgpubkey");
        assert_eq!(file_params["server"], "203.0.113.1");

        let body_out = import_subscription_body(T71_WG_CONF);
        assert!(
            body_out.failures.is_empty(),
            "failures: {:?}",
            body_out.failures
        );
        assert_eq!(body_out.configs.len(), 1);
        assert_eq!(body_out.configs[0].protocol, "wireguard");
        assert_eq!(
            body_out.configs[0].name, "wireguard-p1",
            "bodies have no stem, keep the wireguard-stem naming"
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&body_out.configs[0].params)
                .expect("params are json"),
            file_params,
            "import-level params identical"
        );

        let mut file_store = Store::open_in_memory().expect("store opens");
        let file_down = {
            let file_batch = import_file_content("wg-vpn.conf", T71_WG_CONF);
            crate::dpapi::set_fail_for_test(true);
            let down = commit_batch(&mut file_store, file_batch, "default");
            crate::dpapi::set_fail_for_test(false);
            down
        };
        let error = file_down.expect_err("DPAPI-down file commit aborts");
        assert!(
            matches!(error, crate::store::StoreError::Crypto { .. }),
            "distinct crypto variant, got {error:?}"
        );
        assert!(error.to_string().contains("dpapi"), "loud dpapi cause");
        assert!(
            file_store.list().expect("list works").is_empty(),
            "aborted file commit leaves no rows"
        );

        let file_batch = import_file_content("wg-vpn.conf", T71_WG_CONF);
        let outcome =
            commit_batch(&mut file_store, file_batch, "default").expect("file commit works");
        assert_eq!(outcome.imported, 1);
        assert_eq!(outcome.failures.len(), 0);
        let file_rows = file_store.list().expect("list works");
        assert_eq!(file_rows.len(), 1);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&file_rows[0].params)
                .expect("params are json"),
            file_params,
            "file import lands through the dpapi1: sealed pipeline, read back identical"
        );

        let mut body_store = Store::open_in_memory().expect("store opens");
        let sub = body_store
            .add_subscription("Nightly", "https://example.com/sub.txt", "default", true)
            .expect("subscription adds");
        let body_down = {
            crate::dpapi::set_fail_for_test(true);
            let down = body_store.refresh_with_body(&sub.id, T71_WG_CONF, 100);
            crate::dpapi::set_fail_for_test(false);
            down
        };
        let error = body_down.expect_err("DPAPI-down body refresh aborts");
        assert!(
            matches!(error, crate::store::StoreError::Crypto { .. }),
            "distinct crypto variant, got {error:?}"
        );
        assert!(error.to_string().contains("dpapi"), "loud dpapi cause");
        assert!(
            body_store.list().expect("list works").is_empty(),
            "aborted body refresh leaves no rows"
        );

        let outcome = body_store
            .refresh_with_body(&sub.id, T71_WG_CONF, 200)
            .expect("refresh works");
        assert_eq!(outcome.failures.len(), 0);
        let body_rows = body_store.list().expect("list works");
        assert_eq!(body_rows.len(), 1);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&body_rows[0].params)
                .expect("params are json"),
            file_params,
            "body import lands through the same dpapi1: pipeline as files"
        );
    }

    #[test]
    fn t71_mixed_body_processes_all_segments() {
        let json_obj = serde_json::json!({
            "type": "vless", "tag": "json-row",
            "server": "j.example.com", "server_port": 443, "uuid": UUID,
        });
        let body = format!(
            "json://{}#json-name\n{}\ntrojan://trojanpw@t.example.com:443#trojan-row",
            b64(&json_obj.to_string()),
            T71_WG_CONF
        );
        let out = import_subscription_body(&body);
        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert_eq!(
            out.configs.len(),
            3,
            "every segment lands: {:?}",
            out.configs
        );
        assert_eq!(out.configs[0].protocol, "vless");
        assert_eq!(out.configs[0].name, "json-name");
        assert_eq!(params_of(&out.configs[0])["server"], "j.example.com");
        assert_eq!(out.configs[1].protocol, "wireguard");
        assert_eq!(out.configs[1].name, "wireguard-p1");
        assert_eq!(params_of(&out.configs[1])["private_key"], "wgprivkey");
        assert_eq!(out.configs[2].protocol, "trojan");
        assert_eq!(out.configs[2].name, "trojan-row");
    }
}
