use base64::Engine as _;
use serde_json::Value;

pub const MAX_PRESET_NAME_CHARS: usize = 256;

pub const VALID_PRESET_ACTIONS: &[&str] = &["route", "proxy", "direct", "block", "reject"];

pub fn served_action(action: &str) -> &str {
    if action == "block" {
        "reject"
    } else if action == "proxy" {
        "route"
    } else {
        action
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetImport {
    pub name: String,
    pub rules: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemotePresetRequest {
    pub url: String,
    pub name: String,
}

fn fnv1a_64(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub(crate) fn cap_name(name: String) -> String {
    if name.chars().count() > MAX_PRESET_NAME_CHARS {
        let suffix = format!("-{:016x}", fnv1a_64(&name));
        let keep = MAX_PRESET_NAME_CHARS.saturating_sub(suffix.len());
        let mut truncated: String = name.chars().take(keep).collect();
        truncated.push_str(&suffix);
        truncated
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

pub fn validate_preset_rules(rules: &[Value]) -> Result<(), PresetError> {
    for (index, rule) in rules.iter().enumerate() {
        let obj = rule
            .as_object()
            .ok_or_else(|| bad_rule(index, "must be a JSON object"))?;
        let action = obj
            .get("action")
            .and_then(Value::as_str)
            .ok_or_else(|| bad_rule(index, "field \"action\" is required"))?;

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

        if action == "route" || action == "proxy" {
            if let Some(outbound) = obj.get("outbound") {
                if !outbound.is_string() {
                    return Err(bad_rule(index, "field \"outbound\" must be a string"));
                }
            }
        } else if obj.contains_key("outbound") {
            return Err(bad_rule(index, "field \"outbound\" needs action \"route\""));
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

fn strip_preset_bom(text: &str) -> &str {
    let mut rest = text;
    while let Some(stripped) = rest.strip_prefix('\u{FEFF}') {
        rest = stripped;
    }
    rest
}

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

pub fn looks_like_preset_json(text: &str) -> bool {
    let trimmed = strip_preset_bom(text.trim()).trim();
    if trimmed.starts_with('{') {
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
    } else if trimmed.starts_with('[') {
        match serde_json::from_str::<Value>(trimmed) {
            Ok(Value::Array(items)) if !items.is_empty() => items.iter().all(|item| {
                item.as_object()
                    .is_some_and(|obj| obj.get("action").is_some_and(Value::is_string))
            }),
            _ => false,
        }
    } else {
        false
    }
}

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

fn is_internal_host(host: &str) -> bool {
    let lower = host.trim().to_ascii_lowercase();
    let bare = lower.strip_suffix('.').unwrap_or(&lower);
    if bare == "localhost" || bare.ends_with(".localhost") {
        return true;
    }
    if let Ok(ip) = bare.parse::<std::net::IpAddr>() {
        match ip {
            std::net::IpAddr::V4(v4) => return is_internal_v4(&v4),
            std::net::IpAddr::V6(v6) => {
                if let Some(mapped) = v6.to_ipv4_mapped() {
                    return is_internal_v4(&mapped);
                }
                let segments = v6.segments();
                return v6.is_loopback()
                    || v6.is_unspecified()
                    || v6.is_multicast()
                    || (segments[0] & 0xffc0) == 0xfe80
                    || (segments[0] & 0xfe00) == 0xfc00;
            }
        }
    }
    false
}

fn is_internal_v4(v4: &std::net::Ipv4Addr) -> bool {
    let octets = v4.octets();
    v4.is_loopback()
        || v4.is_unspecified()
        || v4.is_link_local()
        || octets[0] == 10
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 168)
        || octets[0] == 127
}

pub fn decode_remote_route_link(line: &str) -> Result<RemotePresetRequest, PresetError> {
    let (command, payload) = split_throne_command(line)?;
    if !command.eq_ignore_ascii_case("remoteroute") {
        return Err(PresetError::new(
            PresetErrorKind::BadUrl,
            "not a remote route preset link",
        ));
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

pub fn preset_share_link(name: &str, rules: &[Value]) -> String {
    use base64::Engine as _;
    let body = serde_json::json!({"name": name, "rules": rules}).to_string();
    let payload = base64::engine::general_purpose::STANDARD.encode(body.as_bytes());
    format!("throne://route/{payload}")
}

pub const BASIC_PRESET_ACTIONS: &[&str] = &["direct", "proxy", "block"];

pub fn structured_preset_rules(action: &str) -> Result<Vec<Value>, PresetError> {
    if !BASIC_PRESET_ACTIONS.contains(&action) {
        if action == "warp" || action == "warp-bypass" {
            return validate_preset_rules(&[serde_json::json!({"action": action})])
                .map(|()| Vec::new());
        }
        return Err(PresetError::new(
            PresetErrorKind::BadRule,
            format!(
                "rule 0: field \"action\" must be one of {}",
                BASIC_PRESET_ACTIONS.join(", ")
            ),
        ));
    }
    let rules = vec![serde_json::json!({"action": action})];
    validate_preset_rules(&rules)?;
    Ok(rules)
}

pub fn set_rule_action(rule: &mut Value, action: &str) -> Result<(), PresetError> {
    if !VALID_PRESET_ACTIONS.contains(&action) && action != "proxy" {
        return validate_preset_rules(&[serde_json::json!({"action": action})]);
    }
    let obj = rule
        .as_object_mut()
        .ok_or_else(|| bad_rule(0, "must be a JSON object"))?;
    obj.insert("action".to_string(), Value::String(action.to_string()));
    if action != "route" && action != "proxy" {
        obj.remove("outbound");
    }
    validate_preset_rules(std::slice::from_ref(rule))
}

pub fn rule_attribute_tabs(rule: &Value) -> Vec<&'static str> {
    let mut tabs = vec!["Action"];
    let Some(obj) = rule.as_object() else {
        return tabs;
    };

    const GROUPS: &[(&str, &str)] = &[
        ("domain", "Domain"),
        ("domain_suffix", "Domain"),
        ("domain_keyword", "Domain"),
        ("domain_regex", "Domain"),
        ("ip_cidr", "IP"),
        ("ip_is_private", "IP"),
        ("port", "Port"),
        ("port_range", "Port"),
        ("process_name", "Process"),
        ("process_path", "Process"),
        ("network", "Network"),
        ("protocol", "Network"),
    ];
    for (_, tab) in GROUPS {
        if GROUPS
            .iter()
            .any(|(key, group)| group == tab && obj.contains_key(*key))
            && !tabs.contains(tab)
        {
            tabs.push(tab);
        }
    }
    if obj.keys().any(|key| {
        key != "action" && key != "outbound" && !GROUPS.iter().any(|(known, _)| known == key)
    }) {
        tabs.push("Other");
    }
    tabs
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

    fn t42_route_payload(inner: &str) -> (String, String) {
        use base64::Engine as _;
        let std = base64::engine::general_purpose::STANDARD.encode(inner.as_bytes());
        let urlsafe = base64::engine::general_purpose::URL_SAFE.encode(inner.as_bytes());
        (std, urlsafe)
    }

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

    #[test]
    fn t42_proxy_action_valid_and_served_as_route() {
        validate_preset_rules(&[serde_json::json!({"action": "proxy"})]).expect("proxy validates");
        assert_eq!(served_action("proxy"), "route");
    }

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

    #[test]
    fn t42_preset_bom_parses() {
        let parsed = parse_preset_json(
            "\u{FEFF}{\"name\":\"bom\",\"rules\":[{\"action\":\"direct\"}]}",
            "",
        )
        .expect("BOM preset parses");
        assert_eq!(parsed.name, "bom");
    }

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

    #[test]
    fn t48_bom_prefixed_preset_json_parses() {
        let object = "\u{FEFF}{\"name\":\"bom\",\"rules\":[{\"action\":\"direct\"}]}";
        let parsed = parse_preset_json(object, "").expect("BOM object parses");
        assert_eq!(parsed.name, "bom");
        assert_eq!(parsed.rules.len(), 1);
        let array = "\u{FEFF}[{\"action\":\"direct\"}]";
        let parsed = parse_preset_json(array, "file-stem").expect("BOM array parses");
        assert_eq!(parsed.name, "file-stem");
        let doubled = "\u{FEFF}\u{FEFF}{\"name\":\"bom\",\"rules\":[]}";
        assert!(
            parse_preset_json(doubled, "").is_ok(),
            "stacked BOMs collapse"
        );
        assert!(
            looks_like_preset_json(object),
            "shape spotted through the BOM"
        );
        assert!(!looks_like_preset_json("\u{FEFF}vless://u@h:1"));
    }

    #[test]
    fn long_preset_name_truncates_to_256_chars() {
        let long = "a".repeat(300);
        let text = format!(r#"{{"name":"{long}","rules":[]}}"#);
        let parsed = parse_preset_json(&text, "").expect("long name parses");
        assert_eq!(parsed.name.chars().count(), 256);
        assert!(parsed.name.starts_with(&"a".repeat(200)), "{parsed:?}");
        let other = format!(r#"{{"name":"{}b","rules":[]}}"#, "a".repeat(299));
        let parsed_other = parse_preset_json(&other, "").expect("long name parses");
        assert_ne!(parsed.name, parsed_other.name, "prefix collision avoided");
    }

    #[test]
    fn remote_route_link_without_name_falls_back_to_host() {
        let payload = b64("https://example.com/preset.json");
        let request = decode_remote_route_link(&format!("throne://remoteroute/{payload}"))
            .expect("remote link decodes");
        assert_eq!(request.url, "https://example.com/preset.json");
        assert_eq!(request.name, "example.com");
    }

    #[test]
    fn non_route_outbound_rejected_at_intake() {
        for action in ["direct", "reject", "block"] {
            let error = validate_preset_rules(&[serde_json::json!({
                "action": action, "outbound": "some-tag",
            })])
            .unwrap_err();
            assert_eq!(error.kind, PresetErrorKind::BadRule);
            assert!(error.message.contains("\"outbound\""), "{error}");
        }
        validate_preset_rules(&[serde_json::json!({
            "action": "route", "outbound": "some-tag",
        })])
        .expect("route may carry outbound");
    }

    #[test]
    fn bare_array_routes_to_preset_path() {
        assert!(looks_like_preset_json(r#"[{"action":"direct"}]"#));
        assert!(!looks_like_preset_json("[]"));
        assert!(!looks_like_preset_json(r#"["vless://u@h:1"]"#));
        assert!(!looks_like_preset_json(r#"[{"type":"vless"}]"#));
        assert!(!looks_like_preset_json("vless://u@h:1"));
    }

    #[test]
    fn remote_route_link_accepts_urlsafe_base64() {
        use base64::Engine as _;

        let inner = "https://example.com/p.json#\u{FFFF}";
        let payload = base64::engine::general_purpose::URL_SAFE.encode(inner.as_bytes());
        assert!(payload.contains('-') || payload.contains('_'));
        assert!(base64::engine::general_purpose::STANDARD
            .decode(&payload)
            .is_err());
        let request = decode_remote_route_link(&format!("throne://remoteroute/{payload}"))
            .expect("urlsafe remote link decodes");
        assert_eq!(request.url, "https://example.com/p.json");
        assert_eq!(request.name, "\u{FFFF}");
    }

    #[test]
    fn internal_host_blocks_mapped_loopback_and_localhost() {
        assert!(is_internal_host("::ffff:127.0.0.1"));
        assert!(is_internal_host("app.localhost"));
        assert!(is_internal_host("localhost."));
        assert!(!is_internal_host("example.com"));
        assert!(!is_internal_host("93.184.216.34"));
    }

    #[test]
    fn t10_preset_share_link_round_trips() {
        let rules = sample_rules();
        let link = preset_share_link("office", &rules);
        assert!(link.starts_with("throne://route/"), "{link}");
        let parsed = decode_route_link(&link).expect("share link decodes");
        assert_eq!(parsed.name, "office");
        assert_eq!(parsed.rules, rules);
        let empty = preset_share_link("shell", &[]);
        assert!(!empty.is_empty(), "shells still encode");
        let parsed = decode_route_link(&empty).expect("shell link decodes");
        assert_eq!(parsed.name, "shell");
        assert!(parsed.rules.is_empty());
    }

    #[test]
    fn t10_structured_preset_rules_cover_basic_actions() {
        for action in ["direct", "proxy", "block"] {
            let rules = structured_preset_rules(action).expect("basic action builds");
            assert_eq!(rules.len(), 1);
            assert_eq!(rules[0].get("action").and_then(Value::as_str), Some(action));
        }
        for action in ["warp", "warp-bypass", "sniff"] {
            let error = structured_preset_rules(action).expect_err("non-basic fails");
            assert_eq!(error.kind, PresetErrorKind::BadRule);
            assert!(error.message.contains("rule 0"), "{error}");
        }
    }

    #[test]
    fn t10_set_rule_action_rewrites_and_cleans_outbound() {
        let mut rule = serde_json::json!({"action": "route", "outbound": "tag"});
        set_rule_action(&mut rule, "direct").expect("combo rewrites");
        assert_eq!(rule.get("action").and_then(Value::as_str), Some("direct"));
        assert!(!rule.as_object().unwrap().contains_key("outbound"));
        let mut rule = serde_json::json!({"action": "direct"});
        set_rule_action(&mut rule, "proxy").expect("proxy offered");
        assert_eq!(rule.get("action").and_then(Value::as_str), Some("proxy"));
        let mut rule = serde_json::json!({"action": "direct"});
        let error = set_rule_action(&mut rule, "warp").expect_err("warp refused");
        assert!(error.message.contains("warp"), "{error}");
        assert_eq!(rule.get("action").and_then(Value::as_str), Some("direct"));
    }

    #[test]
    fn t10_rule_attribute_tabs_name_matcher_groups() {
        assert_eq!(
            rule_attribute_tabs(&serde_json::json!({"action": "direct"})),
            vec!["Action"]
        );
        assert_eq!(
            rule_attribute_tabs(
                &serde_json::json!({"action": "route", "domain_suffix": ["a"], "port": [25]})
            ),
            vec!["Action", "Domain", "Port"]
        );
        assert_eq!(
            rule_attribute_tabs(
                &serde_json::json!({"action": "block", "network": "tcp", "custom_key": 1})
            ),
            vec!["Action", "Network", "Other"]
        );
    }
}
