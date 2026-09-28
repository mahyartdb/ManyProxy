//! Ticket 37: routing-rule presets (Throne-compatible).
//!
//! Pure parsers and validators for routing presets. A preset is a named,
//! ordered list of sing-box route-rule objects applied per row: on start the
//! rules merge ahead of the row's own isolation rule (see `engine.rs`).
//!
//! This module does no I/O, writes nothing, and never fetches: preset URLs
//! are decoded (never retrieved) here; fetching stays behind an explicit UI
//! action. Errors name rule indices and fields only — never values, names of
//! upstreams, or tokens.

use base64::Engine as _;
use serde_json::Value;

/// Display names (preset names, remote names) truncate here (same cap as the
/// import grammar — bounded display strings, never unbounded input).
pub const MAX_PRESET_NAME_CHARS: usize = 256;

/// Route-rule actions a preset may carry. `route` means "route to the
/// attached row's outbound" (the engine rewrites `outbound` at merge);
/// `proxy` is the Throne alias of `route` (normalized at merge);
/// `direct` passes through verbatim; `block` is the Throne spelling of the
/// sing-box `reject` action and is normalized at merge (live-proven: stock
/// sing-box 1.14.2 has no `block` action — see [`served_action`]).
/// `warp`/`warp-bypass` never validate (see [`validate_preset_rules`]):
/// warp needs an endpoint design, so it fails with a guidance error.
pub const VALID_PRESET_ACTIONS: &[&str] = &["route", "proxy", "direct", "block", "reject"];

/// Merge-time spelling for a validated preset action. Stored rules keep the
/// intake spelling (`block` stays `block` in the database); the served
/// config carries what the sidecar accepts (`reject`, and `proxy` as
/// `route`).
pub fn served_action(action: &str) -> &str {
    if action == "block" {
        "reject"
    } else if action == "proxy" {
        "route"
    } else {
        action
    }
}

/// Machine-readable failure codes for preset parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetErrorKind {
    Empty,
    BadBase64,
    NonUtf8,
    InvalidJson,
    BadName,
    BadRule,
    BadUrl,
}

impl PresetErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PresetErrorKind::Empty => "empty",
            PresetErrorKind::BadBase64 => "bad-base64",
            PresetErrorKind::NonUtf8 => "non-utf8",
            PresetErrorKind::InvalidJson => "invalid-json",
            PresetErrorKind::BadName => "bad-name",
            PresetErrorKind::BadRule => "bad-rule",
            PresetErrorKind::BadUrl => "bad-url",
        }
    }
}

/// One rejected preset import. `message` never contains values or secrets —
/// rule errors name the rule index and the field only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetError {
    pub kind: PresetErrorKind,
    pub message: String,
}

impl PresetError {
    fn new(kind: PresetErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for PresetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind.as_str(), self.message)
    }
}

/// A decoded preset: name plus ordered route-rule objects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetImport {
    pub name: String,
    pub rules: Vec<Value>,
}

/// A decoded `throne://remoteroute/...` intake: a preset URL plus its name.
/// Fetching the URL is an explicit UI action elsewhere — never here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemotePresetRequest {
    pub url: String,
    pub name: String,
}

fn cap_name(name: String) -> String {
    if name.chars().count() > MAX_PRESET_NAME_CHARS {
        name.chars().take(MAX_PRESET_NAME_CHARS).collect()
    } else {
        name
    }
}

fn bad_rule(index: usize, detail: impl Into<String>) -> PresetError {
    PresetError::new(
        PresetErrorKind::BadRule,
        format!("rule {index}: {}", detail.into()),
    )
}

/// Validate ordered route-rule objects field-by-field. Every rejection names
/// the rule index and the field — never values. Shape check only (object +
/// known `action`); sing-box `check` stays authoritative for matcher shapes.
pub fn validate_preset_rules(rules: &[Value]) -> Result<(), PresetError> {
    for (index, rule) in rules.iter().enumerate() {
        let obj = rule
            .as_object()
            .ok_or_else(|| bad_rule(index, "must be a JSON object"))?;
        let action = obj
            .get("action")
            .and_then(Value::as_str)
            .ok_or_else(|| bad_rule(index, "field \"action\" is required"))?;
        // Ticket 42 (US-29): `warp`/`warp-bypass` have no endpoint to point
        // at — a guidance error naming the missing design, never a bare
        // BadRule and never a silent drop.
        if action == "warp" || action == "warp-bypass" {
            return Err(bad_rule(
                index,
                format!(
                    "field \"action\" \"{action}\" is not servable (warp needs an endpoint design — no sing-box outbound to point at; use \"route\", \"proxy\", \"direct\", or \"reject\")"
                ),
            ));
        }
        if !VALID_PRESET_ACTIONS.contains(&action) {
            return Err(bad_rule(
                index,
                format!(
                    "field \"action\" must be one of {}",
                    VALID_PRESET_ACTIONS.join(", ")
                ),
            ));
        }
        // Ticket 42: `proxy` aliases `route` (both point at the row's
        // serving tag at merge).
        if action == "route" || action == "proxy" {
            if let Some(outbound) = obj.get("outbound") {
                if !outbound.is_string() {
                    return Err(bad_rule(index, "field \"outbound\" must be a string"));
                }
            }
        }
    }
    Ok(())
}

fn preset_name_from(obj: &serde_json::Map<String, Value>) -> Option<String> {
    ["name", "tag", "remark"]
        .iter()
        .find_map(|key| obj.get(*key))
        .and_then(Value::as_str)
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .map(cap_name)
}

/// Strip leading UTF-8 BOMs (mirrors the import grammar: a BOM is not
/// whitespace to `str::trim`, so BOM-prefixed bodies strip here before any
/// `{`/`[` sniffing).
fn strip_preset_bom(text: &str) -> &str {
    let mut rest = text;
    while let Some(stripped) = rest.strip_prefix('\u{FEFF}') {
        rest = stripped;
    }
    rest
}

/// Parse preset JSON: an object with a name plus a `rules` array, or a bare
/// rules array (named by `default_name`, e.g. the file stem). Rejections name
/// the field, or the rule index + field for bad rules.
pub fn parse_preset_json(text: &str, default_name: &str) -> Result<PresetImport, PresetError> {
    let trimmed = strip_preset_bom(text.trim()).trim();
    if trimmed.is_empty() {
        return Err(PresetError::new(
            PresetErrorKind::Empty,
            "nothing to import",
        ));
    }
    let value: Value = serde_json::from_str(trimmed)
        .map_err(|_| PresetError::new(PresetErrorKind::InvalidJson, "invalid preset JSON"))?;
    if let Some(items) = value.as_array() {
        let name = default_name.trim();
        if name.is_empty() {
            return Err(PresetError::new(
                PresetErrorKind::BadName,
                "preset needs a name",
            ));
        }
        let rules = items.clone();
        validate_preset_rules(&rules)?;
        return Ok(PresetImport {
            name: cap_name(name.to_string()),
            rules,
        });
    }
    let obj = value.as_object().ok_or_else(|| {
        PresetError::new(
            PresetErrorKind::InvalidJson,
            "preset must be an object or a rules array",
        )
    })?;
    // Ticket 42 (US-28): the Throne kind envelope is checked when present
    // (`kind` must be `throne-route-profile`, `v` must be 1) — a foreign
    // envelope never stores silently as routing policy.
    if let Some(kind) = obj.get("kind").and_then(Value::as_str) {
        if kind != "throne-route-profile" {
            return Err(PresetError::new(
                PresetErrorKind::InvalidJson,
                "preset \"kind\" must be \"throne-route-profile\"",
            ));
        }
    }
    if let Some(v) = obj.get("v") {
        let one = match v {
            Value::Number(number) => number.as_u64().is_some_and(|v| v == 1),
            Value::String(text) => text.trim() == "1",
            _ => false,
        };
        if !one {
            return Err(PresetError::new(
                PresetErrorKind::InvalidJson,
                "preset \"v\" must be 1",
            ));
        }
    }
    let name = preset_name_from(obj).or_else(|| {
        let fallback = default_name.trim();
        if fallback.is_empty() {
            None
        } else {
            Some(cap_name(fallback.to_string()))
        }
    });
    let Some(name) = name else {
        return Err(PresetError::new(
            PresetErrorKind::BadName,
            "preset needs a name",
        ));
    };
    let rules = obj
        .get("rules")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            PresetError::new(
                PresetErrorKind::InvalidJson,
                "preset needs a \"rules\" array",
            )
        })?
        .clone();
    validate_preset_rules(&rules)?;
    Ok(PresetImport { name, rules })
}

/// True when pasted/dropped text is preset JSON rather than endpoint input.
/// Preset shape: an object carrying a `rules` array and none of the endpoint
/// container keys (`outbounds`, `type`/`protocol`, `servers`, `proxies`,
/// `library`) — a full sing-box config has `outbounds` and stays on the
/// endpoint path.
pub fn looks_like_preset_json(text: &str) -> bool {
    let trimmed = text.trim();
    if !trimmed.starts_with('{') {
        return false;
    }
    match serde_json::from_str::<Value>(trimmed) {
        Ok(Value::Object(map)) => {
            map.get("rules").is_some_and(|rules| rules.is_array())
                && !map.contains_key("outbounds")
                && !map.contains_key("type")
                && !map.contains_key("protocol")
                && !map.contains_key("servers")
                && !map.contains_key("proxies")
                && !map.contains_key("library")
        }
        _ => false,
    }
}

/// Standard-base64 decode with the addsub contract (standard alphabet only:
/// urlsafe bytes are rejected; whitespace is ignored).
fn decode_std_b64(payload: &str) -> Result<Vec<u8>, PresetError> {
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
    let compact: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.is_empty() {
        return Err(PresetError::new(
            PresetErrorKind::Empty,
            "throne link is missing its payload",
        ));
    }
    if compact.chars().any(|c| c == '-' || c == '_') {
        return Err(PresetError::new(
            PresetErrorKind::BadBase64,
            "payload must be standard base64",
        ));
    }
    STANDARD
        .decode(&compact)
        .or_else(|_| STANDARD_NO_PAD.decode(&compact))
        .map_err(|_| PresetError::new(PresetErrorKind::BadBase64, "bad base64 payload"))
}

/// Byte-safe `throne://<command>/...` split (never slices inside a
/// non-ASCII char — the prefix match is ASCII so the split is safe).
fn split_throne_command(line: &str) -> Result<(&str, &str), PresetError> {
    let text = line.trim();
    let sep = text
        .find("://")
        .ok_or_else(|| PresetError::new(PresetErrorKind::BadUrl, "not a share link"))?;
    if !text[..sep].eq_ignore_ascii_case("throne") {
        return Err(PresetError::new(
            PresetErrorKind::BadUrl,
            "not a throne link",
        ));
    }
    let rest = &text[sep + 3..];
    match rest.find('/') {
        Some(i) => Ok((&rest[..i], &rest[i + 1..])),
        None => Ok((rest, "")),
    }
}

/// True for single-line `throne://route/...` intake (scheme and command both
/// case-insensitive, mirroring the addsub routing check).
pub fn is_route_attempt(line: &str) -> bool {
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
        .eq_ignore_ascii_case("route")
}

/// True for single-line `throne://remoteroute/...` intake.
pub fn is_remote_route_attempt(line: &str) -> bool {
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
        .eq_ignore_ascii_case("remoteroute")
}

/// Urlsafe-base64 decode (the route-link counterpart to [`decode_std_b64`]:
/// standard bytes are rejected, whitespace is ignored).
fn decode_urlsafe_b64(payload: &str) -> Result<Vec<u8>, PresetError> {
    use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
    let compact: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.is_empty() {
        return Err(PresetError::new(
            PresetErrorKind::Empty,
            "throne link is missing its payload",
        ));
    }
    if compact.chars().any(|c| c == '+' || c == '/') {
        return Err(PresetError::new(
            PresetErrorKind::BadBase64,
            "payload must be urlsafe base64",
        ));
    }
    URL_SAFE
        .decode(&compact)
        .or_else(|_| URL_SAFE_NO_PAD.decode(&compact))
        .map_err(|_| PresetError::new(PresetErrorKind::BadBase64, "bad base64 payload"))
}

/// Decode `throne://route/...` (the addsub envelope contract, extended by
/// ticket 42): standard-base64 of preset JSON, urlsafe-base64 of preset
/// JSON, or the raw preset JSON itself. The kind envelope (`kind`/`v`) is
/// checked when present; rejections never echo the payload.
pub fn decode_route_link(line: &str) -> Result<PresetImport, PresetError> {
    let (command, payload) = split_throne_command(line)?;
    if !command.eq_ignore_ascii_case("route") {
        return Err(PresetError::new(
            PresetErrorKind::BadUrl,
            "not a route preset link",
        ));
    }
    let trimmed = strip_preset_bom(payload.trim()).trim();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return parse_preset_json(trimmed, "");
    }
    let bytes = match decode_std_b64(payload) {
        Ok(bytes) => bytes,
        Err(std_error) => match decode_urlsafe_b64(payload) {
            Ok(bytes) => bytes,
            Err(_) => return Err(std_error),
        },
    };
    let inner = String::from_utf8(bytes)
        .map_err(|_| PresetError::new(PresetErrorKind::NonUtf8, "payload is not UTF-8"))?;
    parse_preset_json(&inner, "")
}

/// True for loopback/private/link-local/unspecified hosts (IP literals only;
/// hostnames are never internal without DNS). Mirrors the store subscription
/// policy — preset.rs is a leaf module (the store depends on it), so the
/// classifier lives here too instead of importing the store.
fn is_internal_host(host: &str) -> bool {
    let lower = host.trim().to_ascii_lowercase();
    if lower == "localhost" {
        return true;
    }
    if let Ok(ip) = lower.parse::<std::net::IpAddr>() {
        match ip {
            std::net::IpAddr::V4(v4) => {
                let octets = v4.octets();
                v4.is_loopback()
                    || v4.is_unspecified()
                    || v4.is_link_local()
                    || octets[0] == 10
                    || (octets[0] == 172 && (16..=31).contains(&octets[1]))
                    || (octets[0] == 192 && octets[1] == 168)
                    || octets[0] == 127
            }
            std::net::IpAddr::V6(v6) => {
                v6.is_loopback()
                    || v6.is_unspecified()
                    || v6.is_multicast()
                    || (v6.segments()[0] & 0xffc0) == 0xfe80
                    || (v6.segments()[0] & 0xfe00) == 0xfc00
            }
        }
    } else {
        false
    }
}

/// Decode `throne://remoteroute/<standard-base64 of "<url>[#name]">`. The URL
/// must be `http(s)` and must not target an internal host (same SSRF guard
/// as subscription intake); the name falls back to the URL host.
pub fn decode_remote_route_link(line: &str) -> Result<RemotePresetRequest, PresetError> {
    let (command, payload) = split_throne_command(line)?;
    if !command.eq_ignore_ascii_case("remoteroute") {
        return Err(PresetError::new(
            PresetErrorKind::BadUrl,
            "not a remote route preset link",
        ));
    }
    let bytes = decode_std_b64(payload)?;
    let inner = String::from_utf8(bytes)
        .map_err(|_| PresetError::new(PresetErrorKind::NonUtf8, "payload is not UTF-8"))?;
    let inner = inner.trim();
    if inner.is_empty() {
        return Err(PresetError::new(
            PresetErrorKind::Empty,
            "remote preset payload is empty",
        ));
    }
    let (url, name) = match inner.find('#') {
        Some(i) => (inner[..i].trim(), inner[i + 1..].trim().to_string()),
        None => (inner, String::new()),
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
        return Err(PresetError::new(
            PresetErrorKind::BadUrl,
            "preset url must use http(s)",
        ));
    };
    // Errors never echo the URL (tokens hide in queries).
    let authority = after_scheme.split('/').next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        host.split(':').next().unwrap_or(host)
    };
    if host.trim().is_empty() || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(PresetError::new(
            PresetErrorKind::BadUrl,
            "preset url must use http(s)",
        ));
    }
    if is_internal_host(host) {
        return Err(PresetError::new(
            PresetErrorKind::BadUrl,
            "preset url must not target internal hosts",
        ));
    }
    let name = if name.is_empty() {
        host.to_string()
    } else {
        cap_name(name)
    };
    Ok(RemotePresetRequest {
        url: url.to_string(),
        name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn b64(text: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(text.as_bytes())
    }

    fn sample_rules() -> Vec<Value> {
        serde_json::from_str::<Vec<Value>>(
            r#"[{"action":"route","domain":["example.com"]},{"action":"direct","ip_cidr":["10.0.0.0/8"]},{"action":"block","port":[25]}]"#,
        )
        .expect("sample rules parse")
    }

    #[test]
    fn parse_preset_object_names_rules() {
        let parsed = parse_preset_json(
            r#"{"name":"ads","rules":[{"action":"block","domain_suffix":["ads.example"]}]}"#,
            "",
        )
        .expect("valid preset parses");
        assert_eq!(parsed.name, "ads");
        assert_eq!(parsed.rules.len(), 1);
    }

    #[test]
    fn parse_preset_bare_array_uses_default_name() {
        let parsed =
            parse_preset_json(r#"[{"action":"direct"}]"#, "file-stem").expect("bare array parses");
        assert_eq!(parsed.name, "file-stem");
    }

    #[test]
    fn parse_preset_bare_array_without_name_fails() {
        let error =
            parse_preset_json(r#"[{"action":"direct"}]"#, "  ").expect_err("nameless array fails");
        assert_eq!(error.kind, PresetErrorKind::BadName);
    }

    #[test]
    fn parse_preset_missing_name_fails() {
        let error = parse_preset_json(r#"{"rules":[]}"#, "").expect_err("nameless fails");
        assert_eq!(error.kind, PresetErrorKind::BadName);
    }

    #[test]
    fn parse_preset_missing_rules_fails_naming_field() {
        let error = parse_preset_json(r#"{"name":"x"}"#, "").expect_err("rules-less fails");
        assert_eq!(error.kind, PresetErrorKind::InvalidJson);
        assert!(error.message.contains("\"rules\""), "{error}");
    }

    #[test]
    fn malformed_rule_names_index_and_field() {
        let error = parse_preset_json(
            r#"{"name":"x","rules":[{"action":"route"},{"nope":1}]}"#,
            "",
        )
        .expect_err("bad rule fails");
        assert_eq!(error.kind, PresetErrorKind::BadRule);
        assert!(error.message.contains("rule 1"), "{error}");
        assert!(error.message.contains("\"action\""), "{error}");
    }

    #[test]
    fn bad_action_names_index_and_field() {
        let error = parse_preset_json(r#"{"name":"x","rules":[{"action":"sniff"}]}"#, "")
            .expect_err("bad action fails");
        assert_eq!(error.kind, PresetErrorKind::BadRule);
        assert!(error.message.contains("rule 0"), "{error}");
        assert!(error.message.contains("\"action\""), "{error}");
    }

    #[test]
    fn block_spelling_accepted_but_served_as_reject() {
        validate_preset_rules(&[serde_json::json!({"action": "block"})])
            .expect("Throne block spelling validates at intake");
        validate_preset_rules(&[serde_json::json!({"action": "reject"})])
            .expect("sing-box reject spelling validates at intake");
        assert_eq!(served_action("block"), "reject");
        assert_eq!(served_action("reject"), "reject");
        assert_eq!(served_action("route"), "route");
        assert_eq!(served_action("direct"), "direct");
    }

    #[test]
    fn non_string_outbound_names_index_and_field() {
        let error = parse_preset_json(
            r#"{"name":"x","rules":[{"action":"route","outbound":7}]}"#,
            "",
        )
        .expect_err("numeric outbound fails");
        assert_eq!(error.kind, PresetErrorKind::BadRule);
        assert!(error.message.contains("rule 0"), "{error}");
        assert!(error.message.contains("\"outbound\""), "{error}");
    }

    #[test]
    fn route_link_decodes_like_addsub_envelope() {
        let payload = b64(r#"{"name":"via-link","rules":[{"action":"direct"}]}"#);
        let parsed =
            decode_route_link(&format!("throne://route/{payload}")).expect("route link decodes");
        assert_eq!(parsed.name, "via-link");
        assert_eq!(parsed.rules.len(), 1);
    }

    #[test]
    fn route_link_bad_base64_fails() {
        let error = decode_route_link("throne://route/!!!").expect_err("bad b64 fails");
        assert_eq!(error.kind, PresetErrorKind::BadBase64);
    }

    #[test]
    fn route_link_wrong_command_fails() {
        let error = decode_route_link("throne://add/eA==").expect_err("wrong command fails");
        assert_eq!(error.kind, PresetErrorKind::BadUrl);
    }

    #[test]
    fn is_route_attempt_matches_case_insensitively() {
        assert!(is_route_attempt("throne://route/eA=="));
        assert!(is_route_attempt("THRONE://ROUTE/eA=="));
        assert!(!is_route_attempt("throne://addsub/eA=="));
        assert!(!is_route_attempt("vless://x@y:1"));
    }

    #[test]
    fn remote_route_link_decodes_url_and_name() {
        let payload = b64("https://example.com/preset.json#office");
        let request = decode_remote_route_link(&format!("throne://remoteroute/{payload}"))
            .expect("remote link decodes");
        assert_eq!(request.url, "https://example.com/preset.json");
        assert_eq!(request.name, "office");
    }

    #[test]
    fn remote_route_link_rejects_internal_hosts() {
        let payload = b64("http://127.0.0.1:8080/preset.json");
        let error = decode_remote_route_link(&format!("throne://remoteroute/{payload}"))
            .expect_err("loopback blocked");
        assert_eq!(error.kind, PresetErrorKind::BadUrl);
    }

    #[test]
    fn remote_route_link_rejects_non_http() {
        let payload = b64("ftp://example.com/preset.json");
        let error = decode_remote_route_link(&format!("throne://remoteroute/{payload}"))
            .expect_err("non-http blocked");
        assert_eq!(error.kind, PresetErrorKind::BadUrl);
    }

    #[test]
    fn looks_like_preset_json_spots_shape() {
        assert!(looks_like_preset_json(
            r#"{"name":"x","rules":[{"action":"direct"}]}"#
        ));
        assert!(!looks_like_preset_json(
            r#"{"outbounds":[{"type":"vless"}],"rules":[]}"#
        ));
        assert!(!looks_like_preset_json(
            r#"{"type":"vless","server":"h","server_port":1}"#
        ));
        assert!(!looks_like_preset_json("vless://u@h:1"));
    }

    #[test]
    fn usable_sample_rules_validate() {
        validate_preset_rules(&sample_rules()).expect("sample rules validate");
    }

    // -- Ticket 42: envelope + aliases -----------------------------------
    // Route links accept three encodings with a checked kind envelope;
    // `proxy` aliases `route`; `warp` fails with a guidance error; BOMs parse.

    fn t42_route_payload(inner: &str) -> (String, String) {
        use base64::Engine as _;
        let std = base64::engine::general_purpose::STANDARD.encode(inner.as_bytes());
        let urlsafe = base64::engine::general_purpose::URL_SAFE.encode(inner.as_bytes());
        (std, urlsafe)
    }

    /// Ticket 42 (US-28): route links store in standard-b64, urlsafe-b64,
    /// and raw JSON encodings.
    #[test]
    fn t42_route_link_three_encodings_decode() {
        let inner = r#"{"name":"t42","rules":[{"action":"direct"}]}"#;
        let (std, urlsafe) = t42_route_payload(inner);
        for link in [
            format!("throne://route/{std}"),
            format!("throne://route/{urlsafe}"),
            format!("throne://route/{inner}"),
        ] {
            let parsed = decode_route_link(&link).expect("encoding decodes: {link}");
            assert_eq!(parsed.name, "t42");
            assert_eq!(parsed.rules.len(), 1);
        }
    }

    /// Ticket 42 (US-28): the kind envelope is checked when present
    /// (`kind` must be `throne-route-profile`, `v` must be 1).
    #[test]
    fn t42_route_link_kind_envelope_checked() {
        let ok =
            r#"{"kind":"throne-route-profile","v":1,"name":"k","rules":[{"action":"direct"}]}"#;
        let (std, _) = t42_route_payload(ok);
        let parsed =
            decode_route_link(&format!("throne://route/{std}")).expect("envelope accepted");
        assert_eq!(parsed.name, "k");
        let bad_kind = r#"{"kind":"something-else","name":"k","rules":[]}"#;
        let (std, _) = t42_route_payload(bad_kind);
        let error =
            decode_route_link(&format!("throne://route/{std}")).expect_err("bad kind rejects");
        assert!(error.message.contains("kind"), "names kind: {error}");
        let bad_v = r#"{"kind":"throne-route-profile","v":2,"name":"k","rules":[]}"#;
        let (std, _) = t42_route_payload(bad_v);
        let error = decode_route_link(&format!("throne://route/{std}")).expect_err("bad v rejects");
        assert!(error.message.contains("\"v\""), "names v: {error}");
    }

    /// Ticket 42 (US-29): `proxy` validates like `route` and serves as it.
    #[test]
    fn t42_proxy_action_valid_and_served_as_route() {
        validate_preset_rules(&[serde_json::json!({"action": "proxy"})]).expect("proxy validates");
        assert_eq!(served_action("proxy"), "route");
    }

    /// Ticket 42 (US-29): `warp`/`warp-bypass` fail with a guidance error
    /// naming the missing endpoint design (never a bare BadRule).
    #[test]
    fn t42_warp_action_guidance_error() {
        for action in ["warp", "warp-bypass"] {
            let error = validate_preset_rules(&[serde_json::json!({"action": action})])
                .expect_err(&format!("{action} rejects"));
            assert_eq!(error.kind, PresetErrorKind::BadRule);
            assert!(error.message.contains(action), "names action: {error}");
            assert!(
                error.message.contains("endpoint"),
                "names the missing endpoint design: {error}"
            );
        }
    }

    /// Ticket 42 (US-28): BOM-prefixed preset JSON parses (strip like import).
    #[test]
    fn t42_preset_bom_parses() {
        let parsed = parse_preset_json(
            "\u{FEFF}{\"name\":\"bom\",\"rules\":[{\"action\":\"direct\"}]}",
            "",
        )
        .expect("BOM preset parses");
        assert_eq!(parsed.name, "bom");
    }

    /// Ticket 42 (US-28): a remoteroute URL blocked at decode time is also
    /// blocked at fetch time (the fetch re-checks the SSRF policy — decode
    /// alone is insufficient against redirect/plain-http drift).
    #[test]
    fn t42_remoteroute_fetch_policy_parity() {
        use base64::Engine as _;
        let payload = base64::engine::general_purpose::STANDARD.encode("http://127.0.0.1/p.json");
        let error = decode_remote_route_link(&format!("throne://remoteroute/{payload}"))
            .expect_err("loopback blocked at decode");
        assert_eq!(error.kind, PresetErrorKind::BadUrl);
        assert!(matches!(
            crate::store::subscription_fetch_policy("http://127.0.0.1/p.json"),
            crate::store::SubscriptionFetchPolicy::Block { .. }
        ));
    }
}
