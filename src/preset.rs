use base64::Engine as _;
use serde_json::Value;

pub const MAX_PRESET_NAME_CHARS: usize = 256;

/// Per-rule `name` labels are authoring artifacts like preset names: capped
/// with the same condensation taste (`cap_name`) at a tighter bound. Applied
/// where rules enter the store; share links and the rules JSON stay verbatim.
pub const MAX_RULE_NAME_CHARS: usize = 128;

pub const VALID_PRESET_ACTIONS: &[&str] = &[
    "route",
    "proxy",
    "direct",
    "block",
    "reject",
    "sniff",
    "resolve",
    "route-options",
];

pub fn served_action(action: &str) -> &str {
    if action == "block" {
        "reject"
    } else if action == "proxy" {
        "route"
    } else {
        action
    }
}

/// Actions whose rules may carry an `outbound` exit tag; the engine maps a
/// missing `outbound` to the row's outbound and rejects unknown exits at
/// emission. Shared by intake validation and config generation.
pub(crate) fn action_carries_outbound(action: &str) -> bool {
    action == "route" || action == "proxy" || action == "route-options"
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

pub(crate) fn cap_rule_name(name: &str) -> String {
    if name.chars().count() <= MAX_RULE_NAME_CHARS {
        return name.to_string();
    }
    let suffix = format!("-{:016x}", fnv1a_64(name));
    let keep = MAX_RULE_NAME_CHARS.saturating_sub(suffix.len());
    let mut truncated: String = name.chars().take(keep).collect();
    truncated.push_str(&suffix);
    truncated
}

/// Condense any over-long per-rule `name` labels in place, so everything the
/// store holds stays within `MAX_RULE_NAME_CHARS` and the editor renders a
/// legible label column.
pub(crate) fn cap_rule_names(rules: &mut [serde_json::Value]) {
    for rule in rules {
        let Some(obj) = rule.as_object_mut() else {
            continue;
        };
        let Some(name) = obj.get("name").and_then(Value::as_str) else {
            continue;
        };
        let capped = cap_rule_name(name);
        obj.insert("name".to_string(), Value::String(capped));
    }
}

fn bad_rule(index: usize, detail: impl Into<String>) -> PresetError {
    PresetError::new(
        PresetErrorKind::BadRule,
        format!("rule {index}: {}", detail.into()),
    )
}

/// sing-box `"a:b"` port-range shape: two u16 parts around a single colon,
/// low end first.
fn is_port_range_shape(text: &str) -> bool {
    let lo = text
        .split(':')
        .next()
        .and_then(|part| part.parse::<u16>().ok());
    let Some(lo) = lo else {
        return false;
    };
    let rest = match text.split_once(':') {
        Some((_, rest)) => rest,
        None => return false,
    };
    match rest.split_once(':') {
        None => rest.parse::<u16>().is_ok_and(|hi| lo <= hi),
        Some(_) => false,
    }
}

/// Light Throne-spirit shape checks for matcher attributes that any rule may
/// carry. A `false` here means the rule is malformed, not that the merge
/// meaning changes: valid shapes pass through emission untouched.
fn check_rule_matcher_shapes(
    obj: &serde_json::Map<String, Value>,
    fail: &dyn Fn(String) -> RuleCheckFailure,
) -> Result<(), RuleCheckFailure> {
    if let Some(value) = obj.get("source_ip_cidr") {
        let ok = value.is_string()
            || value
                .as_array()
                .is_some_and(|items| items.iter().all(Value::is_string));
        if !ok {
            return Err(fail(
                "field \"source_ip_cidr\" must be a string or a list of strings".to_string(),
            ));
        }
    }
    if let Some(value) = obj.get("source_port") {
        let ok = value
            .as_u64()
            .is_some_and(|port| port <= u64::from(u16::MAX))
            || value
                .as_str()
                .is_some_and(|text| text.parse::<u16>().is_ok());
        if !ok {
            return Err(fail(
                "field \"source_port\" must be a u16 port (0-65535) or a numeric-string"
                    .to_string(),
            ));
        }
    }
    if let Some(value) = obj.get("source_port_range") {
        if !value.as_str().is_some_and(is_port_range_shape) {
            return Err(fail(
                "field \"source_port_range\" must be an \"a:b\" port-range string".to_string(),
            ));
        }
    }
    if let Some(value) = obj.get("ip_version") {
        if !value.as_u64().is_some_and(|v| v == 4 || v == 6) {
            return Err(fail(
                "field \"ip_version\" must be an integer 4 or 6".to_string(),
            ));
        }
    }
    if let Some(value) = obj.get("invert") {
        if !value.is_boolean() {
            return Err(fail("field \"invert\" must be a boolean".to_string()));
        }
    }
    if let Some(value) = obj.get("process_path_regex") {
        if !value.is_string() {
            return Err(fail(
                "field \"process_path_regex\" must be a string".to_string(),
            ));
        }
    }
    if let Some(value) = obj.get("rule_set") {
        let ok = value
            .as_array()
            .is_some_and(|items| items.iter().all(Value::is_string));
        if !ok {
            return Err(fail(
                "field \"rule_set\" must be a list of staged set names".to_string(),
            ));
        }
    }
    Ok(())
}

/// Ordered unique `rule_set` names referenced by a rule list — the staging
/// surface's reference grammar: names, nothing else.
pub fn collect_rule_set_refs(rules: &[Value]) -> Vec<String> {
    let mut refs = Vec::new();
    for rule in rules {
        let Some(Value::Array(names)) = rule.get("rule_set") else {
            continue;
        };
        for name in names {
            if let Some(name) = name.as_str().map(str::trim) {
                if !name.is_empty() && !refs.iter().any(|seen| seen == name) {
                    refs.push(name.to_string());
                }
            }
        }
    }
    refs
}

fn unstaged_rule_set_message(name: &str) -> String {
    format!(
        "rule set \"{name}\" is not staged — add it under Rule sets before this preset can be used"
    )
}

/// T-12 attachment-time validation: rule references must name a staged set.
/// Intake stays tolerant (T-11 anchor); this gate runs when rules attach to a
/// proxy row (store) and again at config generation (engine) — same loudness
/// taste as unknown exits, never silent.
pub fn validate_rule_set_refs_staged(
    rules: &[Value],
    staged: &std::collections::BTreeSet<String>,
) -> Result<(), PresetError> {
    for (index, rule) in rules.iter().enumerate() {
        let Some(Value::Array(names)) = rule.get("rule_set") else {
            continue;
        };
        for name in names {
            let Some(name) = name.as_str() else {
                return Err(bad_rule(
                    index,
                    "field \"rule_set\" must be a list of staged set names",
                ));
            };
            let name = name.trim();
            if name.is_empty() {
                return Err(bad_rule(
                    index,
                    "field \"rule_set\" must not carry an empty set name",
                ));
            }
            if !staged.contains(name) {
                return Err(bad_rule(index, unstaged_rule_set_message(name)));
            }
        }
    }
    Ok(())
}

/// The loud Remove-guard text: the set goes nowhere while presets still
/// reference it, and the message names them.
pub fn rule_set_in_use_message(name: &str, presets: &[String]) -> String {
    format!(
        "rule set \"{name}\" is used by {} preset(s): {} — detach it from those presets first",
        presets.len(),
        presets.join(", ")
    )
}

pub fn validate_preset_rules(rules: &[Value]) -> Result<(), PresetError> {
    for (index, rule) in rules.iter().enumerate() {
        check_preset_rule(index, rule)
            .map_err(|failure| bad_rule(failure.index, failure.detail))?;
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RuleCheckFailure {
    pub index: usize,
    pub detail: String,
}

/// A rule that passed the shared structural checks; both the intake validator
/// and the engine's emission gate consume it so the per-action rule tables can
/// never drift apart.
pub(crate) struct CheckedRule<'a> {
    pub obj: &'a serde_json::Map<String, Value>,
    pub action: &'a str,
}

pub(crate) fn check_preset_rule(
    index: usize,
    rule: &Value,
) -> Result<CheckedRule<'_>, RuleCheckFailure> {
    let fail = |detail: String| RuleCheckFailure { index, detail };
    let Some(obj) = rule.as_object() else {
        return Err(fail("must be a JSON object".to_string()));
    };
    let Some(action) = obj.get("action").and_then(Value::as_str) else {
        return Err(fail("field \"action\" is required".to_string()));
    };

    if action == "warp" || action == "warp-bypass" {
        return Err(fail(format!(
            "field \"action\" \"{action}\" is not servable (warp needs an endpoint design — no sing-box outbound to point at; use \"route\", \"proxy\", \"direct\", or \"reject\")"
        )));
    }
    if !VALID_PRESET_ACTIONS.contains(&action) {
        return Err(fail(format!(
            "field \"action\" must be one of {}",
            VALID_PRESET_ACTIONS.join(", ")
        )));
    }

    if action_carries_outbound(action) {
        if let Some(outbound) = obj.get("outbound") {
            if !outbound.is_string() {
                return Err(fail("field \"outbound\" must be a string".to_string()));
            }
        }
    } else if obj.contains_key("outbound") {
        return Err(fail(
            "field \"outbound\" needs action \"route\"".to_string(),
        ));
    }

    if let Some(name) = obj.get("name") {
        if !name.is_string() {
            return Err(fail("field \"name\" must be a string".to_string()));
        }
    }

    match action {
        "resolve" => {
            for field in ["server", "strategy"] {
                if let Some(value) = obj.get(field) {
                    if !value.is_string() {
                        return Err(fail(format!("field \"{field}\" must be a string")));
                    }
                }
            }
        }
        "route-options" => {
            if let Some(value) = obj.get("override_address") {
                if !value.is_string() {
                    return Err(fail(
                        "field \"override_address\" must be a string".to_string(),
                    ));
                }
            }
            if let Some(value) = obj.get("override_port") {
                if !value
                    .as_u64()
                    .is_some_and(|port| port <= u64::from(u16::MAX))
                {
                    return Err(fail(
                        "field \"override_port\" must be a u16 port (0-65535)".to_string(),
                    ));
                }
            }
        }
        "reject" | "block" => {
            if let Some(method) = obj.get("method") {
                if !method
                    .as_str()
                    .is_some_and(|m| matches!(m, "drop" | "reply" | "default"))
                {
                    return Err(fail(
                        "field \"method\" must be one of drop, reply, default".to_string(),
                    ));
                }
            }
            if let Some(no_drop) = obj.get("no_drop") {
                if !no_drop.is_boolean() {
                    return Err(fail("field \"no_drop\" must be a boolean".to_string()));
                }
            }
        }
        _ => {}
    }

    check_rule_matcher_shapes(obj, &fail)?;

    Ok(CheckedRule { obj, action })
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
    if !action_carries_outbound(action) {
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
        ("ip_version", "IP"),
        ("process_path_regex", "Process"),
        ("invert", "Invert"),
        ("source_ip_cidr", "Source"),
        ("source_ip_is_private", "Source"),
        ("source_port", "Source"),
        ("source_port_range", "Source"),
        ("rule_set", "Rule set"),
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
        key != "action"
            && key != "outbound"
            && key != "name"
            && !GROUPS.iter().any(|(known, _)| known == key)
    }) {
        tabs.push("Other");
    }
    tabs
}

/// Valid `outbound` exit values for outbound-carrying rules, mirroring the
/// engine's gate in `preset_rules_for`: `direct` plus one entry per serving
/// row's outbound tag. Nothing invented — unknown values remain typeable but
/// fail loud at emission.
pub fn preset_exit_suggestions(serving_tags: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(1 + serving_tags.len());
    out.push("direct".to_string());
    for tag in serving_tags {
        if !tag.is_empty() && tag != "direct" && !out.iter().any(|known| known == tag) {
            out.push(tag.clone());
        }
    }
    out
}

/// T-11: the simple-rules boxes. Borrowing Throne's line-grammar parser, not
/// its four-box layout: only the three servable boxes exist here (Throne's
/// warp-bypass box targets actions this app never serves and is skipped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimpleRulesBox {
    Direct,
    Block,
    Proxy,
}

impl SimpleRulesBox {
    /// The action each box stamps onto the rules it parses: the Throne
    /// spellings — `block` normalizes to `reject` and `proxy` to `route` at
    /// emission via `served_action`, never at parse time.
    pub fn action(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Block => "block",
            Self::Proxy => "proxy",
        }
    }

    /// Editor label for the box, matching its action's guideline meaning.
    pub fn label(self) -> &'static str {
        match self {
            Self::Direct => "Direct",
            Self::Block => "Block",
            Self::Proxy => "Proxy",
        }
    }
}

/// T-11 parse output. Invalid lines are retained verbatim — the editor shows
/// them under its "Invalid lines" fold; they are never dropped, auto-corrected,
/// or merged into the rules JSON.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SimpleRulesParse {
    pub rules: Vec<Value>,
    pub invalid: Vec<String>,
}

/// Throne's exact token spellings, each mapped to the rule key it expands to.
/// Everything the grammar accepts lives in this one table; nothing DNS- or
/// geo-flavored may ever join it.
const SIMPLE_RULE_TOKENS: &[(&str, &str)] = &[
    ("domain", "domain"),
    ("suffix", "domain_suffix"),
    ("keyword", "domain_keyword"),
    ("regex", "domain_regex"),
    ("ruleset", "rule_set"),
    ("ip", "ip_cidr"),
    ("processName", "process_name"),
    ("processPath", "process_path"),
];

/// T-11 pure line parser: one Throne-style rule per line, expanded into a
/// single structured rule stamped with `action`. `box` is reserved in Rust,
/// so the first parameter is spelled `r#box`; it must agree with `action`
/// (the caller derives both the same way). Output flows through the shared
/// validation pipeline unchanged — no new intake errors are introduced here.
pub fn parse_simple_rules(r#box: SimpleRulesBox, action: &str, text: &str) -> SimpleRulesParse {
    let action = action.trim();
    debug_assert_eq!(
        action,
        r#box.action(),
        "simple-rules box {:?} stamps {:?}, not {:?}",
        r#box,
        r#box.action(),
        action,
    );
    let mut out = SimpleRulesParse::default();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        match simple_rules_line(action, trimmed) {
            Some(rule) => out.rules.push(rule),
            None => out.invalid.push(line.to_string()),
        }
    }
    out
}

/// One Throne-style line: comma/space separated `token:value` pairs merged
/// into one rule. `None` for an invalid line — unknown token, a token without
/// its `:value`, or no tokens at all (a separator-only line is junk, never a
/// catch-all rule).
fn simple_rules_line(action: &str, line: &str) -> Option<Value> {
    let mut obj = serde_json::Map::new();
    obj.insert("action".to_string(), Value::String(action.to_string()));
    for token in line
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|token| !token.is_empty())
    {
        let (name, value) = token.split_once(':')?;
        if value.is_empty() {
            return None;
        }
        let (_, key) = SIMPLE_RULE_TOKENS
            .iter()
            .find(|(known, _)| *known == name)?;
        let entry = obj
            .entry(key.to_string())
            .or_insert(Value::Array(Vec::new()));
        if let Value::Array(items) = entry {
            items.push(Value::String(value.to_string()));
        }
    }
    if obj.len() == 1 {
        return None;
    }
    Some(Value::Object(obj))
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
        let error = parse_preset_json(r#"{"name":"x","rules":[{"action":"nope"}]}"#, "")
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

    #[test]
    fn t58_intake_accepts_new_actions() {
        for action in ["sniff", "resolve", "route-options"] {
            validate_preset_rules(&[serde_json::json!({ "action": action })])
                .unwrap_or_else(|error| panic!("{action} validates at intake: {error}"));
            assert_eq!(
                served_action(action),
                action,
                "{action} is its own sing-box action"
            );
        }
        let rules = vec![
            serde_json::json!({"action": "sniff", "domain": ["example.com"]}),
            serde_json::json!({"action": "resolve", "server": "dns-local", "strategy": "prefer_ipv4"}),
            serde_json::json!({"action": "route-options", "override_address": "192.0.2.53", "override_port": 53, "outbound": "some-tag"}),
            serde_json::json!({"action": "reject", "method": "drop", "no_drop": true}),
        ];
        let text = serde_json::to_string(&serde_json::json!({
            "kind": "throne-route-profile",
            "v": 1,
            "name": "throne-profile",
            "rules": rules,
        }))
        .expect("profile serializes");
        let parsed = parse_preset_json(&text, "").expect("Throne-shaped profile imports");
        assert_eq!(parsed.name, "throne-profile");
        assert_eq!(
            parsed.rules, rules,
            "rules survive the parse byte round-trip"
        );
        let link = preset_share_link("throne-profile", &rules);
        let shared = decode_route_link(&link).expect("share link round-trips");
        assert_eq!(shared.rules, rules, "share-link round-trip is verbatim");
        let error = validate_preset_rules(&[serde_json::json!({
            "action": "sniff", "outbound": "some-tag",
        })])
        .unwrap_err();
        assert!(error.message.contains("\"outbound\""), "{error}");
    }

    #[test]
    fn t58_reject_method_and_no_drop_round_trip() {
        for method in ["drop", "reply", "default"] {
            validate_preset_rules(&[serde_json::json!({"action": "reject", "method": method})])
                .expect("method validates at intake");
        }
        validate_preset_rules(&[serde_json::json!({
            "action": "reject", "no_drop": true,
        })])
        .expect("no_drop alone validates");
        validate_preset_rules(&[serde_json::json!({
            "action": "block", "method": "reply", "no_drop": false,
        })])
        .expect("Throne block spelling keeps the same reject keys");
        for bad_method in [serde_json::json!("fallback"), serde_json::json!(3)] {
            let error = validate_preset_rules(&[serde_json::json!({
                "action": "reject", "method": bad_method,
            })])
            .unwrap_err();
            assert_eq!(error.kind, PresetErrorKind::BadRule, "{error}");
            assert!(error.message.contains("\"method\""), "{error}");
            assert!(error.message.contains("drop"), "{error}");
        }
        let error = validate_preset_rules(&[serde_json::json!({
            "action": "reject", "no_drop": "yes",
        })])
        .unwrap_err();
        assert!(error.message.contains("\"no_drop\""), "{error}");
        let rules = vec![
            serde_json::json!({"action": "reject", "method": "drop", "no_drop": true}),
            serde_json::json!({"action": "block", "method": "reply"}),
        ];
        let link = preset_share_link("reject-mix", &rules);
        let parsed = decode_route_link(&link).expect("share link decodes");
        assert_eq!(
            parsed.rules, rules,
            "method/no_drop survive round-trip verbatim"
        );
    }

    #[test]
    fn t58_route_options_override_fields_validate() {
        validate_preset_rules(&[serde_json::json!({"action": "route-options"})])
            .expect("bare route-options validates");
        validate_preset_rules(&[serde_json::json!({
            "action": "route-options",
            "override_address": "192.0.2.53",
            "override_port": 8443,
        })])
        .expect("override fields validate");
        validate_preset_rules(&[serde_json::json!({
            "action": "route-options", "outbound": "some-tag",
        })])
        .expect("outbound optional at intake; the exit gate is emission");
        validate_preset_rules(&[serde_json::json!({
            "action": "route-options", "override_port": 65535,
        })])
        .expect("65535 is the u16 ceiling");
        for bad_rule in [
            serde_json::json!({"action": "route-options", "override_address": 53}),
            serde_json::json!({"action": "route-options", "override_port": 65536}),
            serde_json::json!({"action": "route-options", "override_port": -1}),
            serde_json::json!({"action": "route-options", "override_port": "8443"}),
            serde_json::json!({"action": "route-options", "override_port": 8443.5}),
        ] {
            let error = validate_preset_rules(&[bad_rule]).unwrap_err();
            assert_eq!(error.kind, PresetErrorKind::BadRule, "{error}");
            assert!(
                error.message.contains("\"override_"),
                "names the override field: {error}"
            );
        }
    }

    #[test]
    fn t58_resolve_server_strategy_pass_through() {
        validate_preset_rules(&[serde_json::json!({
            "action": "resolve", "server": "dns-local", "strategy": "prefer_ipv4",
        })])
        .expect("resolve attrs validate");
        validate_preset_rules(&[serde_json::json!({"action": "resolve"})])
            .expect("bare resolve validates");
        let error = validate_preset_rules(&[serde_json::json!({
            "action": "resolve", "strategy": 4,
        })])
        .unwrap_err();
        assert!(error.message.contains("\"strategy\""), "{error}");
        let error = validate_preset_rules(&[serde_json::json!({
            "action": "resolve", "server": ["x"],
        })])
        .unwrap_err();
        assert!(error.message.contains("\"server\""), "{error}");
        let error = validate_preset_rules(&[serde_json::json!({
            "action": "resolve", "outbound": "some-tag",
        })])
        .unwrap_err();
        assert!(error.message.contains("\"outbound\""), "{error}");
    }

    #[test]
    fn t58_no_hijack_concept_no_mapping_no_guidance() {
        // The Throne DNS-mapping rule action is a non-concept here: nothing
        // registers, maps, or names it, so presets carrying it trip the
        // existing generic unknown-action intake error.
        let banned_action: String = ["hijack", "-dns"].concat();
        assert_eq!(
            VALID_PRESET_ACTIONS,
            &[
                "route",
                "proxy",
                "direct",
                "block",
                "reject",
                "sniff",
                "resolve",
                "route-options",
            ][..],
            "the action list stays exactly the ticket's list"
        );
        assert!(!VALID_PRESET_ACTIONS.contains(&banned_action.as_str()));
        let text = format!(r#"{{"name":"x","rules":[{{"action":"{banned_action}"}}]}}"#);
        let error = parse_preset_json(&text, "").expect_err("unknown action fails intake");
        assert_eq!(error.kind, PresetErrorKind::BadRule, "{error}");
        assert!(error.message.contains("rule 0"), "{error}");
        assert!(error.message.contains("\"action\""), "{error}");
        assert!(error.message.contains("must be one of"), "{error}");
        for known in VALID_PRESET_ACTIONS {
            assert!(error.message.contains(known), "{error}");
        }
        for source in [include_str!("preset.rs"), include_str!("engine.rs")] {
            assert!(
                !source.contains(&banned_action),
                "no new code path names the banned DNS action"
            );
        }
    }

    #[test]
    fn t59_tabs_surface_source_group() {
        assert_eq!(
            rule_attribute_tabs(&serde_json::json!({
                "action": "route",
                "source_ip_cidr": ["10.0.0.0/8"],
                "source_ip_is_private": true,
                "source_port": 8080,
                "source_port_range": "1000:2000",
            })),
            vec!["Action", "Source"]
        );
        assert_eq!(
            rule_attribute_tabs(&serde_json::json!({
                "action": "direct",
                "domain_suffix": ["cn"],
                "source_ip_is_private": true,
            })),
            vec!["Action", "Domain", "Source"]
        );
        assert_eq!(
            rule_attribute_tabs(&serde_json::json!({
                "action": "direct",
                "ip_is_private": true,
                "source_port": 8080,
            })),
            vec!["Action", "IP", "Source"]
        );
    }

    #[test]
    fn t59_tabs_surface_invert_ip_version_process_regex() {
        assert_eq!(
            rule_attribute_tabs(&serde_json::json!({
                "action": "route",
                "ip_version": 6,
                "process_path_regex": "Throne",
                "invert": true,
            })),
            vec!["Action", "IP", "Process", "Invert"]
        );
        assert_eq!(
            rule_attribute_tabs(&serde_json::json!({"action": "direct", "invert": true})),
            vec!["Action", "Invert"]
        );
        assert_eq!(
            rule_attribute_tabs(&serde_json::json!({"action": "direct", "ip_version": 4})),
            vec!["Action", "IP"]
        );
        assert_eq!(
            rule_attribute_tabs(
                &serde_json::json!({"action": "direct", "process_path_regex": "x"})
            ),
            vec!["Action", "Process"]
        );
        assert_eq!(
            rule_attribute_tabs(&serde_json::json!({
                "action": "direct",
                "invert": true,
                "source_port": 80,
            })),
            vec!["Action", "Invert", "Source"]
        );
        assert_eq!(
            rule_attribute_tabs(&serde_json::json!({
                "action": "block",
                "custom_key": 1,
                "invert": false,
            })),
            vec!["Action", "Invert", "Other"],
            "unknown keys still land on Other; the new keys no longer do"
        );
    }

    #[test]
    fn t59_validation_shape_checks() {
        for good in [
            serde_json::json!({"action": "route", "source_ip_cidr": "10.0.0.0/8"}),
            serde_json::json!({"action": "route", "source_ip_cidr": ["10.0.0.0/8", "127.0.0.1/32"]}),
            serde_json::json!({"action": "route", "source_ip_is_private": true}),
            serde_json::json!({"action": "direct", "source_port": 10808}),
            serde_json::json!({"action": "direct", "source_port": "0"}),
            serde_json::json!({"action": "direct", "source_port": "65535"}),
            serde_json::json!({"action": "direct", "source_port": 65535}),
            serde_json::json!({"action": "block", "source_port_range": "1000:2000"}),
            serde_json::json!({"action": "block", "source_port_range": "8080:8080"}),
            serde_json::json!({"action": "sniff", "ip_version": 4}),
            serde_json::json!({"action": "sniff", "ip_version": 6}),
            serde_json::json!({"action": "direct", "invert": false}),
            serde_json::json!({"action": "route", "invert": true, "source_port": 443}),
            serde_json::json!({"action": "reject", "process_path_regex": "mstsc\\.exe$"}),
        ] {
            validate_preset_rules(std::slice::from_ref(&good))
                .unwrap_or_else(|error| panic!("valid shape rejected: {error}: {good}"));
        }

        for (field, bad_rule) in [
            (
                "source_ip_cidr",
                serde_json::json!({"action": "direct", "source_ip_cidr": 7}),
            ),
            (
                "source_ip_cidr",
                serde_json::json!({"action": "direct", "source_ip_cidr": ["10.0.0.0/8", 7]}),
            ),
            (
                "source_ip_cidr",
                serde_json::json!({"action": "direct", "source_ip_cidr": true}),
            ),
            (
                "source_port",
                serde_json::json!({"action": "direct", "source_port": 70000}),
            ),
            (
                "source_port",
                serde_json::json!({"action": "direct", "source_port": -1}),
            ),
            (
                "source_port",
                serde_json::json!({"action": "direct", "source_port": "70000"}),
            ),
            (
                "source_port",
                serde_json::json!({"action": "direct", "source_port": "8443.5"}),
            ),
            (
                "source_port",
                serde_json::json!({"action": "direct", "source_port": true}),
            ),
            (
                "source_port",
                serde_json::json!({"action": "direct", "source_port": [80]}),
            ),
            (
                "source_port_range",
                serde_json::json!({"action": "direct", "source_port_range": "80"}),
            ),
            (
                "source_port_range",
                serde_json::json!({"action": "direct", "source_port_range": "70000:1"}),
            ),
            (
                "source_port_range",
                serde_json::json!({"action": "direct", "source_port_range": "a:b"}),
            ),
            (
                "source_port_range",
                serde_json::json!({"action": "direct", "source_port_range": "8080:8080:8080"}),
            ),
            (
                "source_port_range",
                serde_json::json!({"action": "direct", "source_port_range": "2000:1000"}),
            ),
            (
                "source_port_range",
                serde_json::json!({"action": "direct", "source_port_range": 8000}),
            ),
            (
                "source_port_range",
                serde_json::json!({"action": "direct", "source_port_range": ["1000:2000"]}),
            ),
            (
                "ip_version",
                serde_json::json!({"action": "direct", "ip_version": 5}),
            ),
            (
                "ip_version",
                serde_json::json!({"action": "direct", "ip_version": "6"}),
            ),
            (
                "ip_version",
                serde_json::json!({"action": "direct", "ip_version": [6]}),
            ),
            (
                "invert",
                serde_json::json!({"action": "direct", "invert": "yes"}),
            ),
            (
                "invert",
                serde_json::json!({"action": "direct", "invert": 1}),
            ),
            (
                "process_path_regex",
                serde_json::json!({"action": "direct", "process_path_regex": 7}),
            ),
            (
                "process_path_regex",
                serde_json::json!({"action": "direct", "process_path_regex": ["x"]}),
            ),
        ] {
            let error = validate_preset_rules(std::slice::from_ref(&bad_rule)).unwrap_err();
            assert_eq!(error.kind, PresetErrorKind::BadRule, "{error}");
            assert!(error.message.contains("rule 0"), "{error}");
            assert!(
                error.message.contains(&format!("\"{field}\"")),
                "names {field}: {error}"
            );
        }

        let error = validate_preset_rules(&[
            serde_json::json!({"action": "direct"}),
            serde_json::json!({
                "action": "direct",
                "source_port": 70000,
                "source_port_range": "8080",
                "invert": "no",
            }),
        ])
        .unwrap_err();
        assert!(
            error.message.contains("rule 1"),
            "index stays named: {error}"
        );

        let rules = vec![
            serde_json::json!({
                "action": "route",
                "source_ip_cidr": ["10.0.0.0/8", "127.0.0.0/8"],
                "source_port": 8443,
                "source_port_range": "1000:2000",
            }),
            serde_json::json!({
                "action": "direct",
                "source_ip_is_private": true,
                "ip_version": 6,
                "invert": true,
                "process_path_regex": "mstsc\\.exe$",
            }),
        ];
        let link = preset_share_link("t59", &rules);
        let parsed = decode_route_link(&link).expect("share link decodes");
        assert_eq!(
            parsed.rules, rules,
            "source/invert/ip_version/regex survive the share-link round-trip verbatim"
        );
    }

    #[test]
    fn t60_rule_name_round_trips_share_link() {
        let rules = vec![
            serde_json::json!({
                "action": "route", "name": "Office exit",
                "domain": ["example.com"], "outbound": "some-tag",
            }),
            serde_json::json!({
                "action": "direct", "name": "Local LAN",
                "ip_cidr": ["192.168.0.0/16"],
            }),
            serde_json::json!({"action": "block", "name": "Mail ports", "port": [25]}),
        ];
        let link = preset_share_link("named", &rules);
        let parsed = decode_route_link(&link).expect("share link decodes");
        assert_eq!(parsed.name, "named");
        assert_eq!(
            parsed.rules, rules,
            "per-rule names survive the share-link round-trip byte-exact"
        );

        let envelope = serde_json::to_string(&serde_json::json!({
            "kind": "throne-route-profile",
            "v": 1,
            "name": "named",
            "rules": rules,
        }))
        .expect("profile serializes");
        let parsed = parse_preset_json(&envelope, "").expect("object profile parses");
        assert_eq!(
            parsed.rules, rules,
            "object intake keeps per-rule names verbatim"
        );

        for bad in [
            serde_json::json!({"action": "direct", "name": 7}),
            serde_json::json!({"action": "direct", "name": ["a"]}),
            serde_json::json!({"action": "route", "outbound": "t", "name": true}),
        ] {
            let error = validate_preset_rules(std::slice::from_ref(&bad))
                .expect_err("non-string name fails intake");
            assert_eq!(error.kind, PresetErrorKind::BadRule, "{error}");
            assert!(error.message.contains("rule 0"), "{error}");
            assert!(error.message.contains("\"name\""), "{error}");
        }

        // Surfaced in the label column, so a rule name is a structural key:
        // it never lands on the Other tab the way unknown keys do.
        assert_eq!(
            rule_attribute_tabs(&serde_json::json!({"action": "direct", "name": "n"})),
            vec!["Action"]
        );
        assert_eq!(
            rule_attribute_tabs(&serde_json::json!({
                "action": "direct", "name": "n", "custom": 1,
            })),
            vec!["Action", "Other"],
            "unknown keys still land on Other; the name key no longer does"
        );
    }

    #[test]
    fn t60_rule_names_capped_for_storage() {
        // The same condensation taste as preset names, at the 128-char bound:
        // short names pass through untouched, long ones keep a hash suffix so
        // distinct prefixes stay distinct.
        assert_eq!(cap_rule_name("Office exit"), "Office exit");
        let long = "a".repeat(200);
        let capped = cap_rule_name(&long);
        assert_eq!(capped.chars().count(), MAX_RULE_NAME_CHARS);
        assert!(capped.starts_with(&"a".repeat(100)), "{capped}");
        let other = cap_rule_name(&format!("{}b", "a".repeat(199)));
        assert_ne!(capped, other, "prefix collision avoided");
        let multi = "名".repeat(160);
        let capped_multi = cap_rule_name(&multi);
        assert_eq!(capped_multi.chars().count(), MAX_RULE_NAME_CHARS);
        // Capping counts characters, not bytes: a 128-byte cut would keep
        // only 42 CJK chars, while a char cut keeps all 128 minus the
        // 17-char hash suffix.
        let kept = capped_multi.chars().take_while(|c| *c == '名').count();
        assert_eq!(kept, MAX_RULE_NAME_CHARS - 17, "caps by chars, not bytes");

        let mut rules = vec![
            serde_json::json!({"action": "direct", "name": long}),
            serde_json::json!({"action": "direct"}),
        ];
        cap_rule_names(&mut rules);
        let capped_in_place = rules[0]["name"].as_str().expect("name stays a string");
        assert_eq!(capped_in_place.chars().count(), MAX_RULE_NAME_CHARS);
        assert!(
            rules[1].as_object().unwrap().get("name").is_none(),
            "rules without a name keep no name"
        );

        assert!(
            validate_preset_rules(&[
                serde_json::json!({"action": "direct", "name": "x".repeat(MAX_RULE_NAME_CHARS)}),
            ])
            .is_ok(),
            "a capped-length name validates"
        );
    }

    #[test]
    fn t61_simple_rules_token_matrix() {
        // Throne's exact token spellings, one documented key each. The matrix
        // mirrors the token table in code — any drift on either side is the
        // grammar changing and must be deliberate.
        for (token, key, value) in [
            ("domain:example.org", "domain", "example.org"),
            ("suffix:example.org", "domain_suffix", "example.org"),
            ("keyword:example", "domain_keyword", "example"),
            ("regex:example\\.org$", "domain_regex", "example\\.org$"),
            ("ruleset:streaming-set", "rule_set", "streaming-set"),
            ("ip:192.0.2.1/32", "ip_cidr", "192.0.2.1/32"),
            ("processName:mstsc.exe", "process_name", "mstsc.exe"),
            (
                "processPath:C:\\apps\\x.exe",
                "process_path",
                "C:\\apps\\x.exe",
            ),
        ] {
            let parsed = parse_simple_rules(SimpleRulesBox::Direct, "direct", token);
            let expected = serde_json::json!({"action": "direct", (key): [value]});
            assert_eq!(
                parsed.rules,
                vec![expected],
                "{token} expands to its documented key"
            );
            assert!(parsed.invalid.is_empty(), "{token} parses clean");
        }
    }

    #[test]
    fn t61_multi_token_line_merges() {
        // One rule per line with every token merged into it; comment and
        // blank lines skipped; comma/space separators interchangeable; CRLF
        // trimmed.
        let text = concat!(
            "# shared block list\n",
            "\n",
            "domain:example.com, suffix:cn keyword:you\n",
            "domain:bing.com\n",
            "domain:a.example\tregex:^ads\\.\n",
        );
        let parsed = parse_simple_rules(SimpleRulesBox::Block, "block", text);
        assert_eq!(
            parsed.rules,
            vec![
                serde_json::json!({
                    "action": "block",
                    "domain": ["example.com"],
                    "domain_suffix": ["cn"],
                    "domain_keyword": ["you"],
                }),
                serde_json::json!({"action": "block", "domain": ["bing.com"]}),
                serde_json::json!({
                    "action": "block",
                    "domain": ["a.example"],
                    "domain_regex": ["^ads\\."],
                }),
            ],
            "each line is one merged rule"
        );
        assert!(parsed.invalid.is_empty(), "{parsed:?}");
        let repeated = parse_simple_rules(
            SimpleRulesBox::Proxy,
            "proxy",
            "domain:one.example domain:two.example",
        );
        assert_eq!(
            repeated.rules,
            vec![serde_json::json!({
                "action": "proxy",
                "domain": ["one.example", "two.example"],
            })],
            "repeated tokens merge into one matcher array"
        );
        let crlf = parse_simple_rules(
            SimpleRulesBox::Direct,
            "direct",
            "domain:x.example\r\nip:1.1.1.1/32",
        );
        assert_eq!(crlf.rules.len(), 2, "CRLF splits lines cleanly");
    }

    #[test]
    fn t61_invalid_lines_retained_not_dropped() {
        // Unknown tokens, empty values, missing colons, junk separators, and
        // wrong casings all survive verbatim in the invalid list; nothing
        // becomes a rule and nothing is auto-corrected.
        let bad_lines = [
            "geosite:ads",
            "warp:everything",
            "domain:",
            "processName:",
            "no-token-at-all",
            ":rooted",
            "Domain:cn",
            "PROCESSNAME:chrome.exe",
            ",,",
            "ip:  ,",
        ];
        let joined = bad_lines.join("\n");
        let parsed = parse_simple_rules(SimpleRulesBox::Direct, "direct", &joined);
        assert_eq!(parsed.invalid, bad_lines, "invalid lines survive verbatim");
        assert!(
            parsed.rules.is_empty(),
            "no catch-all or partial rule leaks from bad lines"
        );
        // One bad token poisons its whole line: valid siblings on it never
        // land as rules while the raw line is kept for the editor's fold.
        let mixed = "domain:good.example,geosite:bad";
        let parsed = parse_simple_rules(SimpleRulesBox::Block, "block", mixed);
        assert_eq!(parsed.invalid, vec![mixed.to_string()]);
        assert!(parsed.rules.is_empty());
    }

    #[test]
    fn t61_ruleset_token_maps_to_rule_set_ref() {
        // The rule carries rule_set:["name"]; intake stays tolerant until
        // T-12 wires the staging path (single source of validation truth
        // unchanged for T-11).
        let parsed = parse_simple_rules(SimpleRulesBox::Direct, "direct", "ruleset:my-set");
        assert_eq!(
            parsed.rules,
            vec![serde_json::json!({"action": "direct", "rule_set": ["my-set"]})]
        );
        assert!(parsed.invalid.is_empty());
        validate_preset_rules(&parsed.rules).expect("rule_set refs validate at intake");
        let staged = parse_simple_rules(SimpleRulesBox::Proxy, "proxy", "ruleset:streaming-set");
        validate_preset_rules(&staged.rules).expect("proxy box ruleset ref validates");
    }

    #[test]
    fn t61_box_actions_normalize() {
        // Each box stamps its Throne spelling; the T-08 vocabulary validates
        // the output unchanged and `served_action` does the emission
        // normalization (block→reject, proxy→route).
        for (r#box, action, served) in [
            (SimpleRulesBox::Direct, "direct", "direct"),
            (SimpleRulesBox::Block, "block", "reject"),
            (SimpleRulesBox::Proxy, "proxy", "route"),
        ] {
            let parsed = parse_simple_rules(r#box, action, "domain:example.com, suffix:cn");
            assert_eq!(
                parsed
                    .rules
                    .first()
                    .and_then(|rule| rule.get("action"))
                    .and_then(Value::as_str),
                Some(action),
                "the box stamps its Throne spelling, not the sing-box one"
            );
            validate_preset_rules(&parsed.rules)
                .expect("box rules pass the shared validation pipeline");
            assert_eq!(served_action(action), served, "{action} serves as {served}");
        }
    }

    #[test]
    fn t62_rule_set_shape_tabs_refs_and_staged_gate() {
        // T-12: `rule_set` joins the known attributes — its own tab, a
        // list-of-names shape. Intake stays tolerant about whether a name is
        // staged; the staged gate is store (attach) + engine (configgen).
        let named = serde_json::json!({"action": "direct", "rule_set": ["ads", "streaming"]});
        assert_eq!(
            rule_attribute_tabs(&named),
            vec!["Action", "Rule set"],
            "rule_set surfaces its own tab, never Other"
        );
        validate_preset_rules(std::slice::from_ref(&named)).expect("list-of-names shape is valid");

        for bad in [
            serde_json::json!({"action": "direct", "rule_set": "ads"}),
            serde_json::json!({"action": "direct", "rule_set": ["ads", 7]}),
            serde_json::json!({"action": "direct", "rule_set": 5}),
        ] {
            let error = validate_preset_rules(std::slice::from_ref(&bad)).unwrap_err();
            assert_eq!(error.kind, PresetErrorKind::BadRule, "{error}");
            assert!(error.message.contains("rule 0"), "{error}");
            assert!(error.message.contains("\"rule_set\""), "{error}");
            assert!(
                error.message.contains("list of staged set names"),
                "the shape message names the staging grammar: {error}"
            );
        }

        // References collect in first-seen order, deduped — the Remove guard
        // and the attach surface read the same list.
        let rules = vec![
            serde_json::json!({"action": "route", "rule_set": ["b", "a"]}),
            serde_json::json!({"action": "proxy", "ip_cidr": ["10.0.0.0/8"]}),
            serde_json::json!({"action": "direct", "rule_set": ["a"]}),
        ];
        assert_eq!(
            collect_rule_set_refs(&rules),
            vec!["b".to_string(), "a".to_string()],
            "refs collect first-seen and deduped"
        );
        assert!(collect_rule_set_refs(&[serde_json::json!({"action": "direct"})]).is_empty());

        // The staged gate: unlisted name is loud, staged name passes, malformed
        // and empty names fail the same way the shape check does.
        let staged: std::collections::BTreeSet<String> =
            ["b"].iter().map(|n| n.to_string()).collect();
        let error = validate_rule_set_refs_staged(&rules, &staged).unwrap_err();
        assert!(
            error.message.contains(&unstaged_rule_set_message("a")),
            "names the missing set: {error}"
        );
        validate_rule_set_refs_staged(
            &[serde_json::json!({"action": "route", "rule_set": ["b"]})],
            &staged,
        )
        .expect("every ref staged passes");
        let error = validate_rule_set_refs_staged(
            &[serde_json::json!({"action": "direct", "rule_set": [7]})],
            &staged,
        )
        .unwrap_err();
        assert!(
            error.message.contains("rule 0") && error.message.contains("\"rule_set\""),
            "{error}"
        );
        let error = validate_rule_set_refs_staged(
            &[serde_json::json!({"action": "direct", "rule_set": [" "]})],
            &staged,
        )
        .unwrap_err();
        assert!(
            error.message.contains("must not carry an empty set name"),
            "{error}"
        );
    }

    #[test]
    fn t62_remove_guard_message_names_detachment() {
        let many: Vec<String> = ["Work", "Travel"].into_iter().map(str::to_string).collect();
        assert_eq!(
            rule_set_in_use_message("ads", &many),
            "rule set \"ads\" is used by 2 preset(s): Work, Travel — detach it from those presets first"
        );
        let single = vec!["Detach first".to_string()];
        assert_eq!(
            rule_set_in_use_message("solo", &single),
            "rule set \"solo\" is used by 1 preset(s): Detach first — detach it from those presets first"
        );
    }
}
